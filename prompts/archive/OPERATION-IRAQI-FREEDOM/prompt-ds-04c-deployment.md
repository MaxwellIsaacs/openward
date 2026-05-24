# DS-04C: Deployment and Operations

**Stream:** 8 — Deployment & Operations
**Priority:** High for field use
**Dependencies:** Phase 2 complete (auth and migrations must be solid before deploy)

## Tasks

### 1. Systemd Service File

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

### 2. ARM64 Build

Cross-compilation from x86 to aarch64 (Pi 4):
```bash
cross build --release --target aarch64-unknown-linux-gnu
```

Or native build on the Pi itself. Document both paths. The release binary should be a single file — no external dependencies besides libc.

### 3. First-Run Setup

On first start with an empty database:
- Apply all migrations
- Seed default housing units (existing behavior)
- Create default admin account
- Display a one-time setup message with the admin credentials
- Force password change on first admin login

### 4. Health Check Endpoint

`GET /health` — returns 200 with JSON:
```json
{
  "status": "ok",
  "database": "connected",
  "uptime_seconds": 12345,
  "version": "0.2.0"
}
```

No auth required. Useful for monitoring.

### 5. Configuration Documentation

Document all environment variables:
- `OPENWARD_DB` — database path (default: `openward.db`)
- `OPENWARD_BIND` — bind address (default: `127.0.0.1:3000`)
- `OPENWARD_CAPACITY` — facility capacity (default: 300)
- `OPENWARD_JUVENILE_AGE` — juvenile age threshold (default: 18)
- `OPENWARD_LEGAL_PRESET` — legal term preset (e.g., `fr-MG`)
- `OPENWARD_LEGAL_FILE` — runtime legal override file path
- `OPENWARD_BACKUP_DIR` — backup directory
- `OPENWARD_SESSION_SECRET` — session signing key (generate randomly if not set)

## Exit Criteria

Systemd service file works. ARM64 cross-compilation documented and tested. First-run setup creates admin and seeds data. Health check endpoint responds. All env vars documented.
