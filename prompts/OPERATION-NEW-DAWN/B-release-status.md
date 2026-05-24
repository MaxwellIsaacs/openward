# Prompt B — Fix Released Detainee Still Shows "On Trial" (Issue #12)

**Priority:** Critical / Showstopper
**Phase:** 2 (parallel with C, D)
**Depends on:** A (read `HANDOFF-A.md` first for context on recent HTMX fixes)

## Setup

Read `HANDOFF-A.md` in the project root before starting. It describes changes made to the HTMX redirect pattern that may affect the same files.

## Problem

After releasing a detainee, their facility status still displays as "On Trial" (or whatever their detention basis was) instead of "Released." The release action either isn't updating the status correctly, or the UI isn't reflecting the change.

## Investigation Path

1. **Check the release handler:** `crates/openward-server/src/web/detainee.rs` — `do_release()` calls `state.registry.release(record)`. Does `registry.release()` set `facility_status` to `Released`?

2. **Check the registry implementation:** `crates/openward-registry/src/sqlite.rs` — find the `release()` method. Verify it updates `facility_status` to `Released` in the database.

3. **Check the detainee display:** `crates/openward-server/src/web/detainee.rs` — `detail_page()` builds `DetaineeDisplay`. The `facility_status` field (line 67) uses `t.get(util::status_key(&detainee.facility_status))`. Is this showing the correct field?

4. **Check the template:** `crates/openward-server/templates/detainee/page.html` — line 81 shows `{{ detainee.facility_status }}`. Is this the right field, or should it also check `detention_basis_label`?

5. **Possible issue — detention_basis vs facility_status confusion:** The UI shows `detention_basis_label` (line 75) prominently as "Legal Basis" and `facility_status` (line 81) as "Facility Status". If the user is seeing "On Trial" after release, it might be that `detention_basis` isn't being updated on release (it stays as `OnTrial`) while only `facility_status` changes to `Released`. Consider: should the detention basis label update on release? Or should the UI make the `facility_status: Released` display more prominent/obvious when released?

## Key Files

- `crates/openward-server/src/web/detainee.rs` — release handler + detail page builder
- `crates/openward-registry/src/sqlite.rs` — `release()` implementation
- `crates/openward-core/src/detainee.rs` — `FacilityStatus` enum, `DetentionBasis` enum
- `crates/openward-server/templates/detainee/page.html` — display template
- `crates/openward-server/src/web/util.rs` — `status_key()`, `basis_key()` helper functions

## Fix

Whatever the root cause:
- After release, the detainee detail page must clearly show they are **Released**
- The "On Trial" detention basis should either be updated or visually de-emphasized when status is Released
- Consider adding a prominent "RELEASED" banner at the top of the detail page when `facility_status == Released`
- Consider disabling/hiding the action buttons (Release, Update Basis, etc.) for released detainees

## Verification

1. `cargo build` must succeed
2. `cargo test` must pass
3. Manually test: admit a detainee with "On Trial" basis, then release them, then view their detail page — should clearly show Released status

## Handoff

Write `HANDOFF-B.md` in the project root with:
- Root cause explanation
- What you changed
- Whether the fix was backend (data not updating) or frontend (display not reflecting data)
