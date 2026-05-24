# Prompt K — Bulk Onboarding Wizard (Issue #16)

**Priority:** High (but deferred)
**Phase:** 6 (parallel with L)
**Depends on:** All prior phases. Read ALL `HANDOFF-*.md` files.

## Setup

Read ALL `HANDOFF-*.md` files in the project root.

## Problem

A prison already operating with hundreds or thousands of detainees can't onboard via "New Admission" one at a time. Needs a dedicated bulk onboarding workflow for multiple people to spend days entering data cleanly.

**Context:** An NGO will already be on-site to configure the Raspberry Pi. OCR is overengineered — won't work reliably on target hardware, may fail on handwriting or Malagasy. Plan for manual data entry.

## Design

The onboarding wizard is a dedicated mode for initial facility setup, distinct from day-to-day admissions.

### Workflow

1. **Start Onboarding Mode** — accessible from admin settings or first-run wizard
2. **Streamlined Entry Form** — stripped-down version of the admission form with only essential fields:
   - Surname, given names
   - Sex
   - Date of birth or estimated age
   - Detention basis (simplified selector)
   - Housing unit
   - Intake date (important — not today's date, the *actual* intake date from records)
3. **Quick-add loop** — after submitting one entry, immediately show the form again (no redirect to detail page)
4. **Progress tracker** — "42 detainees entered" counter at the top
5. **Batch review** — before finalizing, show a table of all entered records for review
6. **Error correction** — click any row in the review table to edit it

### Key Differences from Regular Admission

- No warrant registration (can be added later)
- No property logging (can be added later)
- No emergency contacts (can be added later)
- Minimal validation — just names, sex, basis, intake date
- Intake date field defaults to empty (must be manually entered), NOT today
- No audit trail for the bulk import (or a single "bulk import" audit entry for the batch)

### Implementation Notes

- New route: `GET /onboarding` — the wizard page
- New route: `POST /onboarding/add` — add one record (HTMX, returns updated form + counter)
- New route: `GET /onboarding/review` — review all entered records
- New route: `POST /onboarding/finalize` — commit the batch
- Consider using the existing `batch_admit` API endpoint for the actual persistence
- The wizard should be accessible only to admins
- Could store in-progress entries in a temporary table or in the session

## Key Files

- `crates/openward-server/src/web/admission.rs` — existing admission logic (reference)
- `crates/openward-server/src/api.rs` — batch admit endpoint (reference)
- `crates/openward-server/templates/admission/page.html` — existing form (simplify for wizard)
- New: `crates/openward-server/templates/onboarding/` — wizard templates
- New: `crates/openward-server/src/web/onboarding.rs` — wizard handler

## Verification

1. `cargo build` must succeed
2. `cargo test` must pass
3. Navigate to `/onboarding` as admin
4. Enter 3-4 test detainees quickly
5. Review the batch
6. Finalize — all should appear in the population list
7. Counter shows correct number throughout

## Handoff

Write `HANDOFF-K.md` in the project root with:
- Data entry flow implemented
- Whether temporary storage or direct-to-DB was used
- Any fields deferred from the onboarding form
- Performance considerations for large batches (100+ records)
