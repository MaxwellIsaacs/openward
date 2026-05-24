# Operation Desert Storm — Master Plan

## Status Quo

OpenWard is a correctional facility management system targeting Madagascar, designed to run on a Raspberry Pi 4 with SQLite. The following is complete and compiling:

- **Domain model** (`openward-core`): ~50 types, legal basis state machine, Mandela Rules validation, 13-flag system
- **Database** (`openward-db`): 14-table SQLite schema with chain-hashed audit trail
- **Registry** (`openward-registry`): Full CRUD implementation (~1700 lines), flag computation, dynamic query builder, 97 integration tests
- **Server** (`openward-server`): 15 JSON API endpoints, 28 HTML routes, 19 Askama templates, full i18n (en/fr/mg), HTMX interactivity, Pico CSS, print styles

What is **not** done:

| Gap | Severity | Notes |
|-----|----------|-------|
| Auth is fake (3 hardcoded operators, no passwords) | Critical | No roles, no permissions |
| `overview()` returns defaults for time_distribution, flag_counts, sex_breakdown, critical, today_count_status | High | Dashboard is half-populated |
| `search()` returns empty statistics and no flags on summaries | High | Population list missing key data |
| No DB migration strategy | High | Only CREATE IF NOT EXISTS; no ALTER support |
| 4 stub modules (medical, disciplinary, commissary, visitors) | Medium | Empty crates |
| Malagasy translations are English placeholders | Medium | French is done |
| No backup/export | Medium | Single SQLite file, no recovery story |
| No offline/sync (`pending_mutations` table unused) | Low | Pi connectivity is intermittent |
| No photo/document upload | Low | `photo_hash` and `document_hash` fields exist but nothing writes them |

---

## Work Streams

The work is organized into **8 streams**. Dependencies are marked. Streams without dependencies on each other can be executed in parallel.

---

### Stream 1: Complete the Registry (Fill Stubs)

**Priority:** Highest — the dashboard and population list are the most-used views.

**No dependencies.** Can start immediately.

#### 1A. Implement `overview()` fully

Currently returns `TimeDistribution::default()`, `FlagCounts::default()`, `SexBreakdown::default()`, `CriticalNumbers::default()`, `DailyCountStatus::NotStarted`.

Needs to:

1. **SexBreakdown**: Query `SELECT sex, COUNT(*) FROM detainees WHERE facility_status IN ('Present','InCourt','InHospital') GROUP BY sex` and map to the struct.

2. **TimeDistribution**: For each active detainee, compute `(today - intake_date).num_days()` and bucket into the 8 ranges. Calculate median and mean. This can be done in SQL:
   ```sql
   SELECT
     SUM(CASE WHEN julianday('now') - julianday(intake_date) < 2 THEN 1 ELSE 0 END) as under_48h,
     SUM(CASE WHEN julianday('now') - julianday(intake_date) < 7 THEN 1 ELSE 0 END) as under_1w,
     -- ... etc
   FROM detainees WHERE facility_status IN ('Present','InCourt','InHospital')
   ```
   Note the buckets are cumulative in the struct names but should be exclusive (under_48h means 0-2 days, under_1_week means 2-7 days, etc.). Check the struct definition. Median requires loading all days_held values and sorting — acceptable for Pi-scale populations (hundreds, not millions).

3. **FlagCounts + CriticalNumbers**: Load all active detainees, run `compute_flags()` on each, aggregate. `compute_flags()` already exists in `flags.rs`. The `FlagCounts` and `CriticalNumbers` structs overlap significantly — CriticalNumbers is a subset. Both need the same flag pass. Do this in one loop.

4. **DailyCountStatus**: Query `daily_counts` for today's date. If no row → `NotStarted`. If row with `is_finalized = 0` and no `actual_closing` → `Open { computed_closing }`. If row with `actual_closing` but not finalized → `Submitted { computed, actual, balanced }`. If finalized → `Finalized { closing, balanced }`.

5. **Housing units**: The flag computation needs housing units. Load them once and pass to each `compute_flags()` call.

**Tests to add:**
- `overview_time_distribution` — seed detainees with known intake dates, verify buckets
- `overview_sex_breakdown` — seed with mixed sexes, verify counts
- `overview_flag_counts` — seed detainees triggering known flags, verify aggregation
- `overview_critical_numbers` — same
- `overview_daily_count_status_*` — test each status variant

#### 1B. Implement search statistics and summary flags

Currently `search()` returns `default_statistics()` and empty `flags` on each `DetaineeSummary`.

1. **Summary flags**: After loading each detainee summary, compute flags. The `DetaineeSummary` already has a `flags: Vec<Flag>` field. The `compute_flags()` function needs a full `Detainee` struct. Two approaches:
   - **Option A (simple)**: For each search result, load the full detainee and compute flags. Acceptable at Pi scale with PAGE_SIZE=25.
   - **Option B (efficient)**: Add a lightweight flag computation that works on summary data. More code, less I/O.

   Recommend Option A. Keep it simple.

2. **QueryStatistics**: After running the search query, compute aggregate statistics over the result set. Some fields (`total`, `with_court_date`, `without_court_date`, etc.) can be computed during the main query. Others (flag_counts, time_held) require post-processing. Since the result is paginated, statistics must be computed over the *entire matching set*, not just the current page. This means either:
   - A separate aggregation query that applies the same WHERE clause but computes counts
   - Or load all matching IDs and aggregate in Rust

   Recommend a separate SQL aggregation query for the simple counts, and Rust-side aggregation for flags (since flag computation is Rust-only).

**Tests to add:**
- `search_returns_flags` — seed a detainee with a known flag, search, verify flag appears
- `search_statistics_populated` — seed known data, verify statistics fields are non-default
- `search_statistics_reflect_full_result_set` — verify stats cover all matches, not just page

---

### Stream 2: Authentication & Authorization

**Priority:** Critical for any real deployment.

**No dependencies** on Stream 1. Can run in parallel.

#### 2A. Operator storage in SQLite

Add an `operators` table:

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

Add the table to `schema.sql`. Add a `seed_default_admin()` function that creates a default admin account if no operators exist (username: `admin`, password: `changeme`, force password change on first login).

#### 2B. Password hashing

Add `argon2` dependency (or `bcrypt` — argon2 is preferred but check ARM64 performance on Pi 4). Implement `hash_password()` and `verify_password()` in a new `crates/openward-server/src/auth.rs` (separate from `web/auth.rs` which handles HTTP concerns).

#### 2C. Login flow update

Replace the hardcoded operator list with a database lookup. Login form gets a username + password field instead of operator dropdown. On success:
- Set `openward_session` cookie (existing behavior)
- Store session mapping in memory (HashMap<SessionId, OperatorSession>) or in SQLite
- Session contains: operator_id, display_name, role, language, expires_at

On failure: re-render login with error.

#### 2D. Role-based access control

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

#### 2E. Operator management UI

Add pages:
- `GET /operators` — list operators (admin only)
- `GET /operators/new` — create operator form (admin only)
- `POST /operators` — create operator (admin only)
- `GET /operators/:id/edit` — edit operator form (admin only)
- `PUT /operators/:id` — update operator (admin only)
- `POST /operators/:id/reset-password` — reset password (admin only)
- `GET /profile` — current operator's profile (self-service language change, password change)
- `POST /profile/password` — change own password

#### 2F. Session management

- Sessions expire after 24h (existing) or on logout
- Admin can view active sessions
- Admin can force-logout an operator
- Store sessions in SQLite (survives server restart) or in-memory (simpler, sessions lost on restart). Recommend SQLite for Pi deployments where restarts happen.

**Tests:**
- Password hash/verify roundtrip
- Login with valid/invalid credentials
- Role permission checks (all role × action combinations)
- Session expiry
- Cannot access write endpoints as readonly

---

### Stream 3: Database Migrations

**Priority:** High — blocks any schema change in production.

**No dependencies.** Can run in parallel with everything.

#### 3A. Migration framework

Replace the current `apply_schema()` (split-on-semicolons, CREATE IF NOT EXISTS) with a proper migration system. Options:

1. **sqlx-cli migrations** — sqlx already has built-in migration support via `sqlx::migrate!()`. This is the path of least resistance since sqlx is already a dependency.
2. **Hand-rolled** — version table + numbered SQL files.

Recommend **sqlx migrations**:

- Create `crates/openward-db/migrations/` directory
- `001_initial.sql` — the current full schema (verbatim from schema.sql)
- Future migrations are `002_add_operators.sql`, `003_add_sessions.sql`, etc.
- Replace `apply_schema()` with `sqlx::migrate!("./migrations").run(&pool).await`
- The `_sqlx_migrations` table tracks what's been applied

#### 3B. Migration for auth tables

Once the framework is in place, `002_add_operators.sql` adds the operators and sessions tables. This is the first real migration and proves the system works.

**Tests:**
- Fresh database gets all migrations applied
- Database with migration 001 already applied only runs 002+
- Migration is idempotent (running twice doesn't error)

---

### Stream 4: Backup & Export

**Priority:** Medium — data loss in a prison registry is catastrophic.

**Depends on:** Stream 3 (migration framework should be in place so backups restore cleanly).

#### 4A. SQLite backup

SQLite's `.backup` API (via `VACUUM INTO` or the backup API) can produce a consistent snapshot while the database is in use. Implement:

- `GET /api/backup` (admin only) — triggers a backup, returns the file for download
- `GET /backup` (admin HTML page) — simple page with a "Download Backup" button and last-backup timestamp
- Backup file naming: `openward-YYYY-MM-DD-HHMMSS.db`

The backup should be a full copy of the database file. On a Pi with hundreds of detainees, this will be <10MB.

#### 4B. Automated daily backup

A simple approach: on each `finalize_daily_count()`, trigger an automatic backup to a configurable directory (`OPENWARD_BACKUP_DIR`, default `./backups/`). Keep the last N backups (configurable, default 30). This ties backup to the daily workflow — if you're doing your count, you're getting a backup.

#### 4C. CSV export

- `GET /api/export/population.csv` (admin/supervisor) — current population as CSV
- `GET /api/export/daily-counts.csv?from=DATE&to=DATE` — daily count history
- `GET /api/export/audit.csv?from=DATE&to=DATE` — audit trail

Use the `csv` crate. Columns should match the detainee summary fields. This enables external analysis and reporting.

#### 4D. Print / PDF

The existing print CSS is good. Add a "Print Report" button on the dashboard and detainee detail pages that triggers `window.print()`. No server-side PDF generation needed — browser print-to-PDF is sufficient for the target environment.

---

### Stream 5: Notes & Property Management

**Priority:** Medium — these features exist in the data model but have no UI for creation.

**No dependencies.** Can run in parallel.

#### 5A. Notes CRUD

Notes exist in the schema and are displayed on the detainee detail page, but there's no way to add/edit/delete them through the UI.

Add:
- `POST /detainees/:id/notes` — add a note (HTMX form on detail page)
- `DELETE /notes/:id` — delete a note (admin/supervisor only)
- Note types: `general`, `medical`, `legal`, `behavioral` — selectable in the form
- Notes are append-only for operators; only admin/supervisor can delete
- Display notes in reverse chronological order on the detail page

#### 5B. Property CRUD

Same situation — property items exist in the schema but no creation UI.

Add:
- `POST /detainees/:id/property` — log a property item
- `PUT /property/:id/return` — mark property as returned (sets `returned = 1`)
- Display on detail page with return status

---

### Stream 6: Audit Trail UI

**Priority:** Medium — the audit trail is being written but nobody can read it.

**No dependencies.** Can run in parallel.

#### 6A. Audit log viewer

Add:
- `GET /audit` — paginated audit log (admin/supervisor only)
- Filters: date range, operator, module, detainee
- Table columns: timestamp, operator name, module, action, detainee name/id, details
- HTMX pagination and filtering

#### 6B. Audit integrity verification

Add:
- `GET /audit/verify` — runs chain hash verification across all entries, reports any breaks
- This is a background operation on large datasets. For Pi scale it will be fast (seconds for thousands of entries).
- Display: "Chain verified: 1,234 entries, 2 epochs, no breaks" or "BREAK DETECTED at entry #567"

#### 6C. Per-detainee audit trail

On the detainee detail page, add an "Audit History" section showing all audit entries for that detainee. This gives operators a complete chronological view of all actions taken on a specific record.

---

### Stream 7: Operational Completeness

**Priority:** Medium — quality of life improvements that make the system genuinely usable.

**Depends on:** Stream 2 (auth) for permission checks on some features.

#### 7A. Transfer workflow

Transfers exist in the data model (`TransferRecord`, `transfers` table) but have no dedicated UI. Currently, transfer is just a `FacilityStatus::Transferred` update.

Add:
- `GET /detainees/:id/transfer-form` — transfer form (HTMX)
- `POST /detainees/:id/transfer` — execute transfer
- Transfer form: destination facility, reason, transfer date, reference number
- Inserts a `transfers` record AND updates facility_status to Transferred
- Add a `GET /transfers` page listing recent transfers (for coordination with other facilities)

#### 7B. Batch admission improvements

Batch admission exists in the API but has no HTML form. For intake from court (multiple people arriving at once):

- `GET /admission/batch` — batch admission form
- Multi-row form where each row is a minimal admission (name, sex, basis type, warrant reference)
- Submit creates all detainees in one transaction

#### 7C. Court date management improvements

- Add ability to edit a scheduled court date (change date, court, purpose) before outcome is recorded
- Add "Reschedule" as a court outcome option (creates a new court date automatically)
- Dashboard alert for detainees with no court date scheduled and pretrial basis

#### 7D. Search improvements

The `PopulationQuery` has 20+ filter fields but the HTML search bar only exposes a few. Add:
- Advanced search toggle (expands full filter panel)
- Filter by: flag type, bail status, charge severity, days held range, sex, housing unit
- "Quick filters" on dashboard: clicking a critical number (e.g., "5 with no legal basis") pre-fills the search

#### 7E. Malagasy translations

The Malagasy `.ftl` files contain English placeholder text. This requires a Malagasy speaker. The i18n validation test will continue passing (keys exist in all languages) but the content is wrong.

This is a human task, not a coding task. The `.ftl` file format is designed for non-programmers. Prepare a translation guide and hand off.

---

### Stream 8: Deployment & Operations

**Priority:** High for actual field use, low for development.

**Depends on:** Streams 2 and 3 (auth and migrations must be solid before deploy).

#### 8A. Systemd service file

Create `deploy/openward.service`:
```ini
[Unit]
Description=OpenWard Facility Management
After=network.target

[Service]
Type=simple
User=openward
Group=openward
WorkingDirectory=/opt/openward
ExecStart=/opt/openward/openward-server
Environment=OPENWARD_DB=/opt/openward/data/openward.db
Environment=OPENWARD_BIND=0.0.0.0:8080
Environment=OPENWARD_LEGAL_PRESET=fr-MG
Restart=always
RestartSec=5

[Install]
WantedBy=multi-user.target
```

#### 8B. Build script for ARM64

Cross-compilation from x86 to aarch64 (Pi 4):
```bash
cross build --release --target aarch64-unknown-linux-gnu
```

Or native build on the Pi itself. Document both paths. The release binary should be a single file — no external dependencies besides libc.

#### 8C. First-run setup

On first start with an empty database:
- Apply all migrations
- Seed default housing units (existing behavior)
- Create default admin account
- Display a one-time setup message with the admin credentials
- Force password change on first admin login

#### 8D. Health check endpoint

`GET /health` — returns 200 with a JSON body:
```json
{
  "status": "ok",
  "database": "connected",
  "uptime_seconds": 12345,
  "version": "0.2.0"
}
```

No auth required. Useful for monitoring.

#### 8E. Configuration documentation

Document all environment variables:
- `OPENWARD_DB` — database path (default: `openward.db`)
- `OPENWARD_BIND` — bind address (default: `127.0.0.1:3000`)
- `OPENWARD_CAPACITY` — facility capacity (default: 300)
- `OPENWARD_JUVENILE_AGE` — juvenile age threshold (default: 18)
- `OPENWARD_LEGAL_PRESET` — legal term preset (e.g., `fr-MG`)
- `OPENWARD_LEGAL_FILE` — runtime legal override file path
- `OPENWARD_BACKUP_DIR` — backup directory
- `OPENWARD_SESSION_SECRET` — session signing key (generate randomly if not set)

---

## Phasing and Parallelism

### Phase 1: Foundation (Streams 1, 2, 3 in parallel)

These three are independent and together close the critical gaps:

```
┌─────────────────────┐  ┌─────────────────────┐  ┌─────────────────────┐
│ Stream 1             │  │ Stream 2             │  │ Stream 3             │
│ Registry stubs       │  │ Auth & RBAC          │  │ DB Migrations        │
│                      │  │                      │  │                      │
│ 1A: overview()       │  │ 2A: operators table  │  │ 3A: sqlx migrations  │
│ 1B: search stats     │  │ 2B: password hashing │  │ 3B: auth migration   │
│     + summary flags  │  │ 2C: login flow       │  │                      │
│                      │  │ 2D: RBAC             │  │                      │
│                      │  │ 2E: operator UI      │  │                      │
│                      │  │ 2F: sessions         │  │                      │
└─────────────────────┘  └─────────────────────┘  └─────────────────────┘
```

**Exit criteria:** Dashboard fully populated. Real login with passwords. Migrations working. All existing tests still pass, new tests for overview/search/auth.

### Phase 2: Usability (Streams 4, 5, 6 in parallel)

Once the foundation is solid, add the features that make it genuinely usable:

```
┌─────────────────────┐  ┌─────────────────────┐  ┌─────────────────────┐
│ Stream 4             │  │ Stream 5             │  │ Stream 6             │
│ Backup & Export      │  │ Notes & Property     │  │ Audit Trail UI       │
│                      │  │                      │  │                      │
│ 4A: SQLite backup    │  │ 5A: Notes CRUD       │  │ 6A: Audit viewer     │
│ 4B: Auto backup      │  │ 5B: Property CRUD    │  │ 6B: Integrity check  │
│ 4C: CSV export       │  │                      │  │ 6C: Per-detainee     │
│ 4D: Print buttons    │  │                      │  │                      │
└─────────────────────┘  └─────────────────────┘  └─────────────────────┘
```

**Exit criteria:** Backup/restore tested. Notes and property manageable through UI. Audit trail readable and verifiable.

### Phase 3: Operational Polish (Stream 7, Stream 8)

```
┌──────────────────────────────────┐  ┌─────────────────────────────┐
│ Stream 7                          │  │ Stream 8                     │
│ Operational Completeness          │  │ Deployment & Operations      │
│                                   │  │                              │
│ 7A: Transfer workflow             │  │ 8A: Systemd service          │
│ 7B: Batch admission UI           │  │ 8B: ARM64 build              │
│ 7C: Court date improvements      │  │ 8C: First-run setup          │
│ 7D: Search improvements          │  │ 8D: Health check             │
│ 7E: Malagasy translations (human)│  │ 8E: Config docs              │
└──────────────────────────────────┘  └─────────────────────────────┘
```

**Exit criteria:** All workflows complete. Deployable on Raspberry Pi. Documentation sufficient for a field deployment.

---

## What's Explicitly Out of Scope

These are real features that would make the system better but are not required for a viable v1:

1. **Medical module** — Requires medical domain expertise. Separate project phase.
2. **Disciplinary module** — Requires understanding of local disciplinary processes.
3. **Commissary module** — Financial module with its own complexity.
4. **Visitors module** — Scheduling system with its own UX.
5. **Offline/sync** — The `pending_mutations` table is scaffolding. Real offline support requires conflict resolution and is a substantial engineering effort. The Pi will have intermittent connectivity, not zero connectivity — the system works when connected, and operators wait when it's not.
6. **Photo/document upload** — Raspberry Pi storage constraints make this tricky. Defer.
7. **Multi-facility federation** — The system is designed for one facility. Multi-facility is a future concern.
8. **SMS/notification integration** — Court date reminders to legal reps, etc. Future.

---

## Prompt Decomposition Plan

This master plan should be broken into the following executable prompts:

### Parallel batch 1 (Phase 1):
- `prompt-ds-01a-overview-implementation.md` — Stream 1A (fill overview stubs)
- `prompt-ds-01b-search-statistics.md` — Stream 1B (fill search stubs)
- `prompt-ds-02a-migration-framework.md` — Stream 3A+3B (set up sqlx migrations)
- `prompt-ds-02b-auth-storage.md` — Stream 2A+2B (operators table, password hashing)
- `prompt-ds-02c-auth-login.md` — Stream 2C+2F (login flow, sessions) — depends on 02b
- `prompt-ds-02d-auth-rbac.md` — Stream 2D+2E (RBAC, operator management UI) — depends on 02c

### Parallel batch 2 (Phase 2):
- `prompt-ds-03a-backup-export.md` — Stream 4 (backup, export, print)
- `prompt-ds-03b-notes-property.md` — Stream 5 (notes and property CRUD)
- `prompt-ds-03c-audit-ui.md` — Stream 6 (audit trail viewer, verification)

### Parallel batch 3 (Phase 3):
- `prompt-ds-04a-workflows.md` — Stream 7A+7B+7C (transfer, batch admission, court dates)
- `prompt-ds-04b-search-polish.md` — Stream 7D (advanced search, quick filters)
- `prompt-ds-04c-deployment.md` — Stream 8 (systemd, ARM64, health check, docs)

### Sequential:
- `prompt-ds-05-integration-test.md` — End-to-end integration test across all streams (after all batches complete)
