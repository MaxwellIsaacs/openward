# DS-03B: Notes and Property CRUD

**Stream:** 5 — Notes & Property Management
**Priority:** Medium — features exist in the data model but have no UI for creation
**Dependencies:** Phase 1 complete (auth for permission checks)

## Tasks

### 1. Notes CRUD

Notes exist in the schema and are displayed on the detainee detail page, but there's no way to add/edit/delete them through the UI.

Add:
- `POST /detainees/:id/notes` — add a note (HTMX form on detail page)
- `DELETE /notes/:id` — delete a note (admin/supervisor only)
- Note types: `general`, `medical`, `legal`, `behavioral` — selectable in the form
- Notes are append-only for operators; only admin/supervisor can delete
- Display notes in reverse chronological order on the detail page

### 2. Property CRUD

Same situation — property items exist in the schema but no creation UI.

Add:
- `POST /detainees/:id/property` — log a property item
- `PUT /property/:id/return` — mark property as returned (sets `returned = 1`)
- Display on detail page with return status

## Exit Criteria

Notes can be created, viewed, and deleted (with permissions). Property items can be logged and marked as returned. HTMX interactions work on the detail page. All tests pass.
