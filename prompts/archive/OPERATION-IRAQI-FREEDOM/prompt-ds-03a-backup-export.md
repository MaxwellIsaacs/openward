# DS-03A: Backup and Export

**Stream:** 4 — Backup & Export
**Priority:** Medium — data loss in a prison registry is catastrophic
**Dependencies:** Phase 1 complete (auth for permission checks, migrations for clean restore)

## Tasks

### 1. SQLite Backup

SQLite's `.backup` API (via `VACUUM INTO` or the backup API) can produce a consistent snapshot while the database is in use.

- `GET /api/backup` (admin only) — triggers a backup, returns the file for download
- `GET /backup` (admin HTML page) — simple page with "Download Backup" button and last-backup timestamp
- Backup file naming: `openward-YYYY-MM-DD-HHMMSS.db`

On a Pi with hundreds of detainees, this will be <10MB.

### 2. Automated Daily Backup

On each `finalize_daily_count()`, trigger an automatic backup to a configurable directory (`OPENWARD_BACKUP_DIR`, default `./backups/`). Keep the last N backups (configurable, default 30). This ties backup to the daily workflow.

### 3. CSV Export

- `GET /api/export/population.csv` (admin/supervisor) — current population as CSV
- `GET /api/export/daily-counts.csv?from=DATE&to=DATE` — daily count history
- `GET /api/export/audit.csv?from=DATE&to=DATE` — audit trail

Use the `csv` crate. Columns should match detainee summary fields.

### 4. Print / PDF

Add "Print Report" button on dashboard and detainee detail pages that triggers `window.print()`. No server-side PDF generation — browser print-to-PDF is sufficient. The existing print CSS is good.

## Exit Criteria

Backup download works. Auto-backup on daily count finalization. CSV exports for population, daily counts, and audit trail. Print buttons on key pages. All tests pass.
