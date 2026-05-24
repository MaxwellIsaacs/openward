# DS-02C: Login Flow and Session Management

**Stream:** 2C + 2F — Authentication & Authorization
**Priority:** Critical
**Dependencies:** DS-02B (auth storage and password hashing)

## Context

Login currently uses a hardcoded operator dropdown. Sessions are cookie-based but not backed by real credentials.

## Tasks

### 1. Login Flow Update

Replace the hardcoded operator list with a database lookup. Login form gets username + password fields instead of operator dropdown.

On success:
- Set `openward_session` cookie (existing behavior)
- Store session mapping in SQLite (survives server restart — important for Pi deployments where restarts happen)
- Session contains: operator_id, display_name, role, language, expires_at

On failure: re-render login with error.

### 2. Session Management

- Sessions expire after 24h (existing behavior) or on logout
- Admin can view active sessions
- Admin can force-logout an operator
- Store sessions in SQLite

### 3. Profile Page

- `GET /profile` — current operator's profile (self-service language change, password change)
- `POST /profile/password` — change own password

## Tests to Add

- Login with valid credentials succeeds
- Login with invalid credentials fails
- Session expiry works
- Logout invalidates session
- Force password change on first admin login

## Exit Criteria

Real login with username/password. Sessions stored in SQLite. Profile page for self-service. All tests pass.
