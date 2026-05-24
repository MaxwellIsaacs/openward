# Prompt G — Backup Completeness Audit + Auto-Refresh (Issues #17, #18)

**Priority:** Medium / Low
**Phase:** 3 (parallel with E, F)
**Depends on:** Phase 2 complete. Read `HANDOFF-A.md` through `HANDOFF-D.md`.

## Setup

Read all `HANDOFF-*.md` files in the project root before starting.

## Issue #17 — Backup Completeness Audit

Backups are in good shape overall, but need a thorough review. Anything missing should be explicitly weighed and justified. Default stance: "back it up."

**Where:**
- `crates/openward-server/src/backup.rs` — backup creation logic
- `crates/openward-server/src/web/backup.rs` — backup web handler
- `crates/openward-server/src/export.rs` — export logic (may overlap)
- `crates/openward-db/src/migrations.rs` — database schema to cross-reference

**Audit checklist — verify each is included in backup:**
- [ ] `detainees` table (all columns including JSON fields)
- [ ] `commitment_orders` (warrants)
- [ ] `court_dates` (with outcomes)
- [ ] `housing_units` (reference data)
- [ ] `daily_counts` + `unit_headcounts`
- [ ] `audit_entries` (critical — append-only chain-hashed)
- [ ] `notes`
- [ ] `property_items`
- [ ] `operators` table (if exists — auth data)
- [ ] Facility configuration / settings
- [ ] Any other tables in the schema

**Fix:**
- If the backup is a full SQLite file copy, this is automatically complete — just verify that's what's happening
- If it's a selective export, identify any missing tables and add them
- Document what's backed up and what isn't (add a comment in the backup code)

## Issue #18 — Backup Page Doesn't Auto-Update

After creating a backup, the "last backup" info doesn't refresh until manual page reload.

**Where:** `crates/openward-server/templates/backup/page.html`

**Fix:** After the backup creation form submits, the page should refresh to show updated backup info. Options:
1. **HTMX approach:** Add `hx-swap` on the backup form that replaces the backup status section after submission
2. **Redirect approach:** Have the backup handler redirect back to `/backup` after success (with the same HTMX-aware redirect pattern from Prompt A)
3. **Simplest:** Just add `hx-on::after-request="location.reload()"` on the backup form

Pick the approach that's most consistent with the existing codebase patterns.

## Key Files

- `crates/openward-server/src/backup.rs`
- `crates/openward-server/src/web/backup.rs`
- `crates/openward-server/src/export.rs`
- `crates/openward-server/templates/backup/page.html`
- `crates/openward-db/src/migrations.rs` — schema reference

## Verification

1. `cargo build` must succeed
2. Create a backup — the page should immediately show updated "last backup" info without manual reload
3. Verify backup file contains all tables (inspect the SQLite backup file if possible)

## Handoff

Write `HANDOFF-G.md` in the project root with:
- Tables/data included in backup (and any gaps found)
- Auto-refresh approach chosen
- Any concerns about backup integrity
