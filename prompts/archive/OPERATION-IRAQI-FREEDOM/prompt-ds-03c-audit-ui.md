# DS-03C: Audit Trail UI

**Stream:** 6 — Audit Trail UI
**Priority:** Medium — the audit trail is being written but nobody can read it
**Dependencies:** Phase 1 complete (auth for permission checks)

## Tasks

### 1. Audit Log Viewer

Add:
- `GET /audit` — paginated audit log (admin/supervisor only)
- Filters: date range, operator, module, detainee
- Table columns: timestamp, operator name, module, action, detainee name/id, details
- HTMX pagination and filtering

### 2. Audit Integrity Verification

Add:
- `GET /audit/verify` — runs chain hash verification across all entries, reports any breaks
- For Pi scale it will be fast (seconds for thousands of entries)
- Display: "Chain verified: 1,234 entries, 2 epochs, no breaks" or "BREAK DETECTED at entry #567"

### 3. Per-Detainee Audit Trail

On the detainee detail page, add an "Audit History" section showing all audit entries for that detainee. This gives operators a complete chronological view of all actions taken on a specific record.

## Exit Criteria

Audit log viewable and filterable by admin/supervisor. Chain hash verification runs and reports results. Per-detainee audit history visible on detail page. All tests pass.
