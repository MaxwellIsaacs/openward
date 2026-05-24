# Prompt F — Warning Remediation Actions + Rule Explanations (Issues #8, #9)

**Priority:** High
**Phase:** 3 (parallel with E, G)
**Depends on:** Phase 2 complete. Read `HANDOFF-A.md` through `HANDOFF-D.md`.

## Setup

Read all `HANDOFF-*.md` files in the project root before starting.

## Issue #8 — Warnings Need Clickable Remediation Actions

Warnings/critical alerts on the detainee page currently just state a problem. They should link to a guided action that resolves the issue.

**Where:** `templates/detainee/page.html` lines 17-26 shows flag badges. The flags are computed in `crates/openward-registry/src/flags.rs` via `compute_flags()`.

**Current display:**
```html
<span class="badge {{ flag.css_class }}">{{ flag.label }}</span>
```

**Fix:** Each flag type should suggest (and link to) a specific remediation:

| Flag | Remediation | Link |
|------|-------------|------|
| `NoLegalBasis` | "Register a warrant to establish legal authority" | Click opens warrant form |
| `CustodyLimitExceeded` | "File for court appearance or update basis" | Click opens basis form |
| `ReleaseDatePassed` | "Process release or extend sentence" | Click opens release form |
| `WarrantExpired` | "Register a new warrant" | Click opens warrant form |
| `NoActiveWarrant` | "Register a warrant" | Click opens warrant form |
| `NoCourtDate` | "Schedule a court date" | Click opens court date form |
| `RemandReviewOverdue` | "Schedule remand review hearing" | Click opens court date form |
| `ProlongedPreTrial` | "Review case — consider bail application" | Click opens basis form |
| `BailGrantedStillHeld` | "Process release on bail" | Click opens release form |
| `NoLegalRepresentation` | "Record legal representation details" | Click opens edit form (may need new form) |
| `HousingViolation` | "Reassign housing unit" | Click opens housing form |
| `CourtDateImminent` | "Prepare for upcoming court date" | Informational, link to court calendar |
| `ReleaseImminent` | "Prepare release paperwork" | Link to release form |

**Implementation:**
1. Extend `FlagDisplay` (in `templates.rs`) to include an `action_url: Option<String>` and `action_label: String`
2. In `web/detainee.rs` where flags are built, populate the action URL based on flag type
3. Update the template to render flags as clickable links when an action URL is present
4. The link should use the existing `hx-get` + `hx-target="#action-target"` pattern to load the appropriate form

## Issue #9 — Housing Rule Violation Needs Explanation

The "Housing Rule Violation" flag just says there's a violation with no context on *why*. Need to show the specific rule being violated.

**Where:** `crates/openward-registry/src/flags.rs` — the `HousingViolation` flag variant. Check if it carries metadata about which rule was violated.

**Fix:**
1. Check if `Flag::HousingViolation` has a reason field. If not, add one (e.g., `HousingViolation { reason: String }`)
2. In `compute_flags()`, when generating a `HousingViolation`, include why:
   - "Assigned to unit designated for [Male/Female] but detainee is [Female/Male]"
   - "Unit is at capacity ([X]/[X])"
   - "Juvenile housed with adults"
3. Display the reason text alongside the flag badge in the template

**Note:** If modifying `Flag` in `openward-core`, be careful — check `HANDOFF.md` constraints about modifying core types. If `Flag` is in core and shouldn't be changed, add the explanation text at the display layer instead (compute it from context in the web handler).

## Key Files

- `crates/openward-server/templates/detainee/page.html` — flag display
- `crates/openward-server/src/web/detainee.rs` — flag display building
- `crates/openward-server/src/templates.rs` — `FlagDisplay` struct
- `crates/openward-registry/src/flags.rs` — `compute_flags()`, flag types
- `crates/openward-core/src/detainee.rs` — `Flag` enum definition

## Verification

1. `cargo build` must succeed
2. `cargo test` must pass
3. On a detainee with flags, each flag should show an action link
4. Clicking a flag's action link opens the appropriate form
5. Housing violation flag shows the specific rule being violated

## Handoff

Write `HANDOFF-F.md` in the project root with:
- Flag-to-action mapping implemented
- Whether you modified core types or kept changes in the web layer
- Any flags where remediation doesn't make sense (explain why)
