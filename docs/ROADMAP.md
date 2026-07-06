# OpenWard — Next Steps After the Hetzner Demo

This document captures everything between **"clickable HTTPS demo on
Hetzner"** (where the codebase is now, at commit `f5d7335`) and
**"a real product the Commonwealth of Dominica or Grenada could put
real prisoner data into."**

The work splits into two parallel tracks:

1. A **VM-first dry run** of the Hetzner runbook, before touching the
   real Hetzner box. Bugs in the runbook are far cheaper to find on a
   throwaway VM than on a €4 box whose cold compile takes ten minutes
   per iteration.
2. A **tiered punch list** of what's actually missing for a real-data
   pilot, organized by what can and cannot be skipped.

---

## Part 1 — VM-first dry run of the deploy runbook

### Goal

Exercise every step in `deploy/README.md` ("Hetzner Demo Deployment"
section, steps 3–10) against a throwaway local VM before paying for
Hetzner time. Catch the easy bugs — wrong package name, missing
directory, permission inversion, systemd-resolved fight with Caddy —
locally.

### Why Multipass

Closest local approximation to a Hetzner Ubuntu instance with the
lowest setup cost. On a Linux host: `apt install multipass` (snap on
non-NixOS, or use the GitHub release). On NixOS, multipass isn't
straightforward; an alternative is `lima` with an Ubuntu image, or
`qemu-system-x86_64` with the cloud image and `cloud-init`.

The VM will be **x86** even though the real target is ARM. That's
fine: the runbook is identical except for the build step. Cross-arch
build-time differences ("does this Rust version compile cleanly on
aarch64?") will surface only on Hetzner — but they're rare with stable
Rust against `aarch64-unknown-linux-gnu`.

### Workflow

```bash
# 1. Spin up the VM. CAX11 = 2 vCPU + 4 GB RAM; mirror that.
multipass launch 24.04 --name openward-test --cpus 2 --memory 4G --disk 10G
multipass shell openward-test

# 2. Inside the VM, run deploy/README.md steps 3, 4, 5, 7 verbatim.
#    Steps to ADJUST for the local VM run:
#
#    - Step 5: instead of `git clone https://github.com/openward/openward`,
#      use `multipass transfer` from the host to push the working tree.
#      Either:
#        host$ multipass transfer ../openward openward-test:/opt/openward-src --recursive
#      or push a tarball over scp via multipass-exposed IP.
#
#    - Step 6: every env var stays the same EXCEPT bind. For a local-only
#      LAN smoke test, you can use 0.0.0.0:3000 and skip Caddy, OR keep
#      127.0.0.1:3000 + install Caddy.
#
#    - Step 9 Caddyfile: replace the openward-demo.maxisaacs.com site
#      address with `localhost` (a hostname is required — `:443` alone
#      means no SNI match, no leaf cert, TLS handshake fails):
#
#        localhost {
#            tls internal
#            reverse_proxy 127.0.0.1:3000
#        }
#
#      Caddy issues a self-signed cert via its local CA. Browser warns
#      — accept it. Tests TLS termination without public DNS.

# 3. From the VM, smoke test:
#    sudo -u openward /opt/openward/openward-seed --db /var/lib/openward/openward.db --count 80
#    curl -k https://localhost/health

# 4. From the host:
multipass info openward-test           # get the VM's IP
curl -k https://<vm-ip>/health         # confirm end-to-end TLS works

# 5. Tear down when done.
multipass delete openward-test && multipass purge
```

### What the VM run catches

- Package-name drift between distro versions (`caddy` vs. `caddy2`).
- The `caddy` user not existing in the install path (snap vs. apt).
- File permissions on `/etc/openward/openward.env` (must be `0600`,
  systemd silently ignores it otherwise).
- systemd `ProtectSystem=strict` blocking a path the app actually
  needs to write to.
- Caddy fighting `systemd-resolved` for port 53 (rare but happens on
  fresh Ubuntu).
- The bind-address typo or `OPENWARD_DB` permission inversion that
  causes the server to silently refuse to start.
- Whether the runbook's command ordering actually works (env file must
  exist before `systemctl start openward`, etc.).

### What the VM run does NOT catch

- Let's Encrypt issuance (no public DNS).
- ARM-specific build issues on the actual CAX11.
- Hetzner-specific cloud-init quirks (none expected on Ubuntu 24.04).
- Real network performance from your prospective demo audience's
  location.

### Cycle expectation

Two to three iterations on the VM, plus runbook edits, should
shake out the predictable bugs. Total: 60–90 minutes. After that
the Hetzner deploy should be a clean walk-through.

---

## Part 2 — Gap analysis for a real-data pilot

A pilot is not a demo. The threshold for "real prisoners' data may
touch this software" is categorically higher than "an official can
click around and see what it does." The items below are organized by
what cannot be skipped vs. what can be deferred and for how long.

### Tier 0 — Categorical blockers. Cannot deploy real data without.

| # | Item | Why it's Tier 0 |
|---|------|-----------------|
| 0.1 | **CSRF tokens on all mutating forms** | Cookie-session + form POST = CSRF-vulnerable. A malicious link in an officer's email could trigger state-changing requests. Standard per-session hidden-field token, validated server-side. |
| 0.2 | **Encrypted database at rest** | A Pi can be physically stolen. SQLite file is plaintext. SQLCipher (a sqlx feature flag away) or LUKS on the data volume. Without this, the on-premises sovereignty pitch is theatre. |
| 0.3 | **Login rate limiting + account lockout** | Argon2 slows credential stuffing but doesn't stop it. Need lockout after N failures + IP throttling at Caddy. |
| 0.4 | **`Secure` cookie flag when behind TLS** | Verify the session cookie sets `Secure` when the request came in over HTTPS. Without it, a downgrade attack steals the session. |
| 0.5 | **Tested backup-restore procedure** | You back up. You've never restored. Document the procedure AND prove it works by restoring onto a second box from the rotated backups. |
| 0.6 | **Time-integrity warning banner** | The clock-sanity check in `PastDate::new` already disables validation when the system clock is before 2024. But there's no UI banner telling the operator "clock is unsane; audit timestamps are unreliable." Pi has no RTC — this *will* happen after a power cut without NTP. |
| 0.7 | **DPIA + signed MoU** | Not engineering. Dominica's Data Protection Act 2022 and Grenada's equivalent require: identified data controller, processor, retention schedule, breach notification policy, right-of-access process. No NGO will sponsor a pilot without this paperwork. |

### Tier 1 — Required before the second user touches the system

| # | Item | Notes |
|---|------|-------|
| 1.1 | **Onboarding wizard** | First-run currently dumps the admin at "change your password, now figure it out." Needs: facility name, capacity, custody-time thresholds, create N housing units, create first 3 operator accounts. The prompt `prompts/OPERATION-NEW-DAWN/K-onboarding-wizard.md` already exists; the work doesn't. |
| 1.2 | **Operator training one-pager** | Plus a laminated quick-reference card per workstation. Prison staff turnover is high; they don't read manuals. |
| 1.3 | **Photo capture at intake** | `Identity.photo_hash` exists; no actual photo storage. For a prison, photo ID is non-optional (release verification, court appearances, identifying the deceased). Webcam → resize → file storage with hash chain. |
| 1.4 | **PDF generation** | Release certificate, daily count sheet, custody-limit notification. Court still runs on paper; if the system can't produce print-quality documents with formal headers, officers will keep parallel paper records. Prompt `L-pdf-generation.md` exists. |
| 1.5 | **Offsite backup strategy** | Local backups die with the Pi. Options: encrypted rsync to S3-compatible (Backblaze B2 cheapest), or USB-stick rotation (matches operational reality — a senior officer takes the latest stick home weekly). |
| 1.6 | **Bulk data import** | Day 1, the facility has existing paper records. Needs a CSV-import path that lets a clerk batch-load the current population. Not 300 individual admit forms. |
| 1.7 | **Hardware specification document** | Exact Pi 4 SKU, industrial-grade SD card vendor, USB SSD (real ones, not knockoffs), UPS recommendation (Pi corruption from power cuts is real), monitor, keyboard. Without this the "deploy on Pi" story collapses at the procurement step. |
| 1.8 | **Penetration test of auth flows** | SQL injection through JSON params, XSS in note fields, session fixation, password-reset flow. Either DIY or via the NGO sponsor's tech contacts. |
| 1.9 | **Caribbean charge-code mapping** | The `en-DM.toml` preset is a thin stub. Real terms come from Dominica's Criminal Code Chap. 10:01; charge severity classification mapped to local categorization. Half a day with the Code. |

### Tier 2 — Within the first three months of pilot

| # | Item | Notes |
|---|------|-------|
| 2.1 | **Systemd watchdog** (`WatchdogSec=`) | App can hang and stay "running" from systemd's perspective. |
| 2.2 | **Deeper `/health` endpoint** | Currently checks DB connection only. Should check: WAL size, last backup age, disk free percentage, audit chain head verifies. |
| 2.3 | **Visitor management** (`openward-visitors`) | Currently stub. First missing-feature complaint, guaranteed. |
| 2.4 | **Basic medical** (`openward-medical`) | Currently stub. Intake medical, allergies, current medications, sick-call log. Doesn't need to be EHR-quality. |
| 2.5 | **Monitoring / daily heartbeat** | Even a daily emailed heartbeat. A Pi that dies silently in a back office can sit dead for days. |
| 2.6 | **Migration rollback story** | sqlx migrations are forward-only. Document what happens if `0.x.y → 0.x.y+1` migration goes wrong on a live Pi. |
| 2.7 | **Mobile/tablet responsiveness** | Prompt `J-mobile-responsive.md` exists; needs actual testing on an Android tablet. Officers carry tablets on rounds. |
| 2.8 | **Audit retention policy** | `audit_entries` grows forever. Document whether this is intentional (for prison context, probably yes) and capacity-plan accordingly. |
| 2.9 | **Concurrent-edit semantics** | Document what happens when two officers schedule a court date for the same detainee at the same instant. Optimistic locking is probably right. |
| 2.10 | **Juvenile-separation enforcement** | `HousingViolation` flag *warns*. For a real product it should *refuse* the housing assignment. |

### Tier 3 — For scale-up to a second facility

| # | Item | Notes |
|---|------|-------|
| 3.1 | **Multi-facility data model** | Currently single-facility implied. Transfer feature exists but assumes external destinations. |
| 3.2 | **Aggregate reporting** | Monthly/quarterly stats reports for the Commissioner of Prisons. No PII. |
| 3.3 | **Disciplinary module** | `openward-disciplinary` stub. Incident reports, hearings, sanctions. |
| 3.4 | **Read-only auditor API** | External oversight body needs read access without an operator account. |
| 3.5 | **Deceased workflow** | `FacilityStatus::Deceased` exists; surrounding workflow (coroner notification, family contact, archival rules) does not. |
| 3.6 | **Commissary module** | `openward-commissary` stub. Lower priority than visitor/medical/disciplinary. |
| 3.7 | **Search performance at scale** | At 80 detainees fine. At 300+ with multi-year history, profile and add indexes as needed. |
| 3.8 | **Right-of-access workflow** | Detainee request to see their own record — documented mechanism, even if it's "request to Commissioner via paper form." |

---

## Suggested execution order

1. **Now:** Run the VM dry-run (Part 1). 60–90 min. Fix any runbook bugs
   surfaced. Update `deploy/README.md`.
2. **Deploy Hetzner demo.** 25 min if DNS propagates promptly.
3. **Start NGO outreach in parallel** with Tier 0 engineering — the
   conversation with Penal Reform International / ICPR Birkbeck /
   Commonwealth Foundation / a regional contact takes weeks. Lead time
   on the relationship matches lead time on the Tier 0 engineering.
4. **Tier 0 engineering, in order of cheapest-first:**
   - 0.4 (`Secure` cookie flag) — minutes
   - 0.6 (time-integrity banner) — half day
   - 0.3 (login rate limiting + lockout) — one day
   - 0.1 (CSRF tokens) — one to two days
   - 0.5 (tested restore procedure + docs) — one day
   - 0.2 (encrypted DB at rest, SQLCipher or LUKS) — two to three days
     including evaluation of options
   - 0.7 (DPIA + MoU) — runs on a paperwork track, not engineering
5. **Tier 1 engineering**, prioritized by what unblocks NGO/pilot
   conversations — start with the hardware spec (1.7) and operator
   one-pager (1.2) since those are external-facing.
6. **Pilot.** Tier 2 work happens in flight.

Total realistic timeline from today to "pilot-ready with the first
NGO sponsor identified": **4–6 focused weeks of engineering** on top
of where the codebase is now, in parallel with **6–12 weeks of
relationship building** with the sponsoring organization.

---

## What's explicitly NOT on this list

- Rewriting anything that already works. The domain model, audit
  chain, flag computation, and HTMX rendering are sound.
- Migrating off SQLite. Single-facility low-volume — SQLite is
  correct.
- Adding a JavaScript framework, an SPA, or a separate API client.
  The HTMX architecture is correct for the target environment.
- Multi-language SDKs, GraphQL, OpenAPI generation. The HTTP surface
  is small and stable.
- Kubernetes, Docker Compose, or any container orchestration. A Pi
  with systemd is the deployment unit.
