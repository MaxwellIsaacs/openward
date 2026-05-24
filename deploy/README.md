# OpenWard Deployment Guide

## Building

### Cross-compilation for ARM64 (Raspberry Pi 4)

The recommended approach uses [`cross`](https://github.com/cross-rs/cross):

```bash
cargo install cross --git https://github.com/cross-rs/cross
cross build --release --target aarch64-unknown-linux-gnu
```

The resulting binary will be at `target/aarch64-unknown-linux-gnu/release/openward-server`.

### Native compilation on the Pi

Install the Rust toolchain on the Raspberry Pi itself:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env
cargo build --release
```

## Installation

1. Copy the binary to the target machine:

```bash
scp target/aarch64-unknown-linux-gnu/release/openward-server pi@<host>:/opt/openward/
```

2. Create a system user:

```bash
sudo useradd --system --no-create-home --shell /usr/sbin/nologin openward
```

3. Create data directories:

```bash
sudo mkdir -p /var/lib/openward/backups
sudo chown -R openward:openward /var/lib/openward
```

4. Create the environment file:

```bash
sudo mkdir -p /etc/openward
sudo cp openward.env.example /etc/openward/openward.env
sudo chmod 600 /etc/openward/openward.env
sudo chown openward:openward /etc/openward/openward.env
```

5. Install and enable the systemd service:

```bash
sudo cp deploy/openward.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now openward
```

6. Check the logs for the default admin credentials:

```bash
sudo journalctl -u openward -n 50 --no-pager
```

## Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `OPENWARD_DB` | Path to the SQLite database file | `openward.db` |
| `OPENWARD_BIND` | Address and port to bind the HTTP server | `0.0.0.0:3000` |
| `OPENWARD_CAPACITY` | Facility capacity (used for occupancy calculations) | `500` |
| `OPENWARD_JUVENILE_AGE` | Age cutoff for juvenile classification | `18` |
| `OPENWARD_BACKUP_DIR` | Directory for automatic backups | `./backups/` |
| `OPENWARD_BACKUP_RETENTION` | Number of backup files to retain | `30` |
| `OPENWARD_SESSION_SECRET` | Secret for session cookie signing. Auto-generated if not set (sessions will not survive restarts) | *(random)* |
| `OPENWARD_LEGAL_PRESET` | Legal terminology preset (`madagascar`, etc.) | *(none)* |
| `OPENWARD_LEGAL_TERMS` | Path to a TOML file with custom legal term overrides | *(none)* |
| `RUST_LOG` | Logging level filter | `openward_server=info,tower_http=info` |

### Example `/etc/openward/openward.env`

```bash
OPENWARD_DB=/var/lib/openward/openward.db
OPENWARD_BIND=0.0.0.0:3000
OPENWARD_BACKUP_DIR=/var/lib/openward/backups
OPENWARD_BACKUP_RETENTION=30
OPENWARD_SESSION_SECRET=your-secret-here-at-least-32-hex-chars
OPENWARD_CAPACITY=200
OPENWARD_LEGAL_PRESET=madagascar
RUST_LOG=openward_server=info
```

## Health Check

The server exposes a `GET /health` endpoint (no authentication required) that returns:

```json
{
  "status": "ok",
  "database": "connected",
  "uptime_seconds": 3600,
  "version": "0.2.0"
}
```

Use this for systemd watchdog integration or external monitoring.

## First Run

On first start with an empty database, OpenWard automatically:

1. Runs all database migrations
2. Seeds default housing units
3. Creates a default admin account (`admin` / `changeme`)

The admin credentials are logged prominently. On first login, the admin is redirected to the profile page and must change the default password before accessing any other functionality.
