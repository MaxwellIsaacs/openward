# DS-02A: Database Migration Framework

**Stream:** 3A + 3B — Database Migrations
**Priority:** High — blocks any schema change in production
**Dependencies:** None

## Context

The current `apply_schema()` uses split-on-semicolons with `CREATE IF NOT EXISTS`. There is no ALTER support, no versioning, no way to evolve the schema in production.

## Tasks

### 1. Set Up sqlx Migrations

sqlx already has built-in migration support via `sqlx::migrate!()`. This is the path of least resistance since sqlx is already a dependency.

- Create `crates/openward-db/migrations/` directory
- `001_initial.sql` — the current full schema (verbatim from schema.sql)
- Replace `apply_schema()` with `sqlx::migrate!("./migrations").run(&pool).await`
- The `_sqlx_migrations` table tracks what's been applied

### 2. Auth Tables Migration

Once the framework is in place, create `002_add_operators.sql` to add the operators and sessions tables. This is the first real migration and proves the system works.

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

## Tests to Add

- Fresh database gets all migrations applied
- Database with migration 001 already applied only runs 002+
- Migration is idempotent (running twice doesn't error)

## Exit Criteria

All schema creation goes through sqlx migrations. Existing tests pass with the new migration approach. Auth tables migration exists and applies cleanly.
