# DS-02B: Auth Storage and Password Hashing

**Stream:** 2A + 2B — Authentication & Authorization
**Priority:** Critical
**Dependencies:** DS-02A (migration framework — for the operators table migration)

## Context

Auth is currently fake: 3 hardcoded operators, no passwords, no roles, no permissions. This stream adds the storage and crypto foundation.

## Tasks

### 1. Operator Storage in SQLite

Add an `operators` table (via migration from DS-02A):

```sql
CREATE TABLE IF NOT EXISTS operators (
    id TEXT PRIMARY KEY,
    username TEXT NOT NULL UNIQUE,
    display_name TEXT NOT NULL,
    password_hash TEXT NOT NULL,
    role TEXT NOT NULL DEFAULT 'operator',
    language TEXT NOT NULL DEFAULT 'fr',
    is_active INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    last_login TEXT
);
```

Roles: `admin`, `supervisor`, `operator`, `readonly`. Keep it simple — no ACL tables, just a single role enum.

Add a `seed_default_admin()` function that creates a default admin account if no operators exist (username: `admin`, password: `changeme`, force password change on first login).

### 2. Password Hashing

Add `argon2` dependency (or `bcrypt` — argon2 is preferred but check ARM64 performance on Pi 4).

Implement `hash_password()` and `verify_password()` in a new `crates/openward-server/src/auth.rs` (separate from `web/auth.rs` which handles HTTP concerns).

## Tests to Add

- Password hash/verify roundtrip
- Seed default admin creates account when DB is empty
- Seed default admin is a no-op when operators already exist

## Exit Criteria

Operators table exists via migration. Password hashing works. Default admin seeding works. All tests pass.
