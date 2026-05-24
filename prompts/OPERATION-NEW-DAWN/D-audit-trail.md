# Prompt D — Audit Trail: Human-Readable Details + Detail View (Issues #1, #11)

**Priority:** High
**Phase:** 2 (parallel with B, C)
**Depends on:** A (read `HANDOFF-A.md` first for context)

## Setup

Read `HANDOFF-A.md` in the project root before starting.

## Problem 1 — Issue #1: Raw JSON in Details Column

The audit log "Details" column displays raw JSON blobs. For example, a release event shows:
```json
{"all_property_returned":false,"authorized_by":"a023a51f-..."}
```

This is unreadable, especially for critical actions like `release`. Needs human-readable formatting.

## Problem 2 — Issue #11: Audit Entries Should Be Clickable

Each audit log row should link to a dedicated detail page showing a clear, human-readable breakdown: who performed the action, what they did, when, how, whether it was authorized, etc. Not a raw JSON dump — a proper formatted view.

## Key Files

- `crates/openward-server/templates/audit/table_body.html` — line 30: `{{ entry.details }}` renders raw JSON
- `crates/openward-server/src/web/audit.rs` — `convert_audit_entry()` function that builds display structs
- `crates/openward-registry/src/audit.rs` — raw audit entry retrieval, `AuditEntry` struct
- `crates/openward-core/src/traits.rs` — audit-related types
- `crates/openward-server/templates/detainee/page.html` — lines 301-331: inline audit display on detainee page also shows raw details

## Fix — Issue #1: Human-Readable Details

In `web/audit.rs` (or wherever `convert_audit_entry` lives), parse the JSON `details` field and format it into a human-readable summary based on the `action` type. For example:

| Action | Raw JSON | Should Display |
|--------|----------|----------------|
| `release` | `{"release_type":"Bail","authorized_by":"uuid"}` | "Release (Bail)" |
| `admit` | `{"identity":{"surname":"Doe",...}}` | "Admitted: Doe, John" |
| `update_basis` | `{"new_basis":"Sentenced",...}` | "Basis changed to: Sentenced" |
| `assign_housing` | `{"unit_id":"uuid"}` | "Assigned to: Unit A" |
| `schedule_court_date` | `{"scheduled_date":"2026-04-01","court":"..."}` | "Court date: 2026-04-01" |

Don't try to parse every possible field — extract the 1-2 most important pieces of info per action type and show those. Fall back to a truncated version for unknown action types.

## Fix — Issue #11: Clickable Audit Detail View

1. Add a new route: `GET /audit/{entry_id}` that shows a full detail page for a single audit entry
2. Create a new template: `templates/audit/detail.html`
3. The detail page should show:
   - Timestamp
   - Operator who performed the action (name, not UUID)
   - Action performed (human-readable)
   - Target (link to detainee if applicable)
   - Module
   - Full details rendered as a formatted key-value list (parse JSON, display each field with a label)
   - Chain hash verification status
4. In `audit/table_body.html`, make each row clickable (link the timestamp or add a "View" column)

## Verification

1. `cargo build` must succeed
2. Audit log page shows human-readable summaries, not raw JSON
3. Clicking an audit entry opens a detail page with formatted information
4. Detainee page audit section also shows human-readable details

## Handoff

Write `HANDOFF-D.md` in the project root with:
- How you formatted each action type
- The new route and template added
- Any action types you couldn't format (explain why)
