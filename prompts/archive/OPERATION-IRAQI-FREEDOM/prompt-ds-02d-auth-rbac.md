# DS-02D: RBAC and Operator Management UI

**Stream:** 2D + 2E — Authentication & Authorization
**Priority:** Critical
**Dependencies:** DS-02C (login flow and sessions)

## Context

With auth storage and login in place, this stream adds role-based access control and the admin UI for managing operators.

## Tasks

### 1. Role-Based Access Control

Define permissions per role:

| Action | admin | supervisor | operator | readonly |
|--------|-------|-----------|----------|----------|
| View dashboard/population | yes | yes | yes | yes |
| View detainee detail | yes | yes | yes | yes |
| Admit detainee | yes | yes | yes | no |
| Update basis/status/housing | yes | yes | yes | no |
| Release detainee | yes | yes | no | no |
| Manage housing units | yes | yes | no | no |
| Manage operators | yes | no | no | no |
| View audit trail | yes | yes | no | no |

Implement as a `can_do(role, action) -> bool` function. Add middleware or per-handler checks. Return 403 for unauthorized actions.

### 2. Operator Management UI

Add pages (admin only):
- `GET /operators` — list operators
- `GET /operators/new` — create operator form
- `POST /operators` — create operator
- `GET /operators/:id/edit` — edit operator form
- `PUT /operators/:id` — update operator
- `POST /operators/:id/reset-password` — reset password

## Tests to Add

- Role permission checks (all role x action combinations)
- Cannot access write endpoints as readonly
- Non-admin cannot access operator management pages
- Create/edit/delete operator flows

## Exit Criteria

RBAC enforced on all endpoints. Operator management UI works for admins. All role x action combinations tested. All tests pass.
