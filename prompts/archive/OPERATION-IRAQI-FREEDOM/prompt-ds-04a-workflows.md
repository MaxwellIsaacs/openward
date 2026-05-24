# DS-04A: Transfer, Batch Admission, and Court Date Workflows

**Stream:** 7A + 7B + 7C — Operational Completeness
**Priority:** Medium
**Dependencies:** Phase 2 complete (auth/RBAC for permission checks)

## Tasks

### 1. Transfer Workflow

Transfers exist in the data model (`TransferRecord`, `transfers` table) but have no dedicated UI. Currently, transfer is just a `FacilityStatus::Transferred` update.

Add:
- `GET /detainees/:id/transfer-form` — transfer form (HTMX)
- `POST /detainees/:id/transfer` — execute transfer
- Transfer form: destination facility, reason, transfer date, reference number
- Inserts a `transfers` record AND updates facility_status to Transferred
- `GET /transfers` — page listing recent transfers (for coordination with other facilities)

### 2. Batch Admission UI

Batch admission exists in the API but has no HTML form. For intake from court (multiple people arriving at once):

- `GET /admission/batch` — batch admission form
- Multi-row form where each row is a minimal admission (name, sex, basis type, warrant reference)
- Submit creates all detainees in one transaction

### 3. Court Date Management Improvements

- Add ability to edit a scheduled court date (change date, court, purpose) before outcome is recorded
- Add "Reschedule" as a court outcome option (creates a new court date automatically)
- Dashboard alert for detainees with no court date scheduled and pretrial basis

## Exit Criteria

Transfer workflow end-to-end. Batch admission form works. Court date editing and rescheduling functional. All tests pass.
