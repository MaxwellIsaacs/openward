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
| `OPENWARD_LEGAL_PRESET` | Compiled-in legal terminology preset. Valid values: `fr-MG` (Madagascar), `en-DM` (Dominica) | *(none)* |
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

## Hetzner Demo Deployment

Runbook for the public HTTPS demo at `openward-demo.maxisaacs.com`. This
target is a Hetzner CAX11 (ARM `aarch64`, 2 vCPU, 4 GB RAM, ~€4/mo) —
the same instruction set as the Raspberry Pi 4 the production pilot
targets. Synthetic demo data only; do **not** put real prisoner data on
this box.

### 1. Provision the server

- Create a Hetzner CAX11, Ubuntu 24.04 LTS. Record the IPv4.
- Add your SSH key during creation.

### 2. DNS

At your DNS provider for `maxisaacs.com`, create an A record:

```
openward-demo  A  <hetzner-ipv4>
```

Wait for it to resolve (`dig +short openward-demo.maxisaacs.com` from
another host) before continuing — Caddy will fail to issue a certificate
if the name does not resolve to this server.

### 3. Box bootstrap (as `root`)

Caddy is not in the default Ubuntu 24.04 archive — add the official
Cloudsmith apt source first.

```bash
apt update
apt install -y debian-keyring debian-archive-keyring apt-transport-https curl gnupg ca-certificates
curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/gpg.key' \
    | gpg --dearmor -o /usr/share/keyrings/caddy-stable-archive-keyring.gpg
curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/debian.deb.txt' \
    | tee /etc/apt/sources.list.d/caddy-stable.list
apt update
apt install -y build-essential pkg-config libssl-dev sqlite3 ufw caddy git
```

Firewall — never expose the OpenWard port directly:

```bash
ufw allow 22/tcp
ufw allow 80/tcp
ufw allow 443/tcp
ufw --force enable
```

### 4. System user and directories

```bash
useradd --system --no-create-home --shell /usr/sbin/nologin openward
mkdir -p /var/lib/openward/backups /etc/openward /opt/openward /opt/openward-src
chown -R openward:openward /var/lib/openward
```

### 5. Build OpenWard natively on the box

Build as the default `ubuntu` user, not as `openward` — the openward
service user has no shell and no home (created with `--no-create-home
--shell /usr/sbin/nologin` in step 4 so that a stolen service ticket
can't open a login session). Use any non-root account with a home and
a shell; `ubuntu` is the default on Hetzner Ubuntu images.

```bash
sudo -u ubuntu bash <<'EOS'
cd ~
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable --profile minimal
source ~/.cargo/env
git clone https://github.com/openward/openward /tmp/openward-src
sudo mv /tmp/openward-src/* /tmp/openward-src/.[!.]* /opt/openward-src/ 2>/dev/null || true
sudo chown -R ubuntu:ubuntu /opt/openward-src
cd /opt/openward-src
cargo build --release --workspace
EOS

# Cold compile: ~10 min on a CAX11, ~3 min on a fast x86 VM — normal.
install -m 0755 /opt/openward-src/target/release/openward-server /opt/openward/openward-server
install -m 0755 /opt/openward-src/target/release/openward-seed /opt/openward/openward-seed
```

### 6. Environment file

```bash
SECRET=$(openssl rand -hex 32)
cat >/etc/openward/openward.env <<EOF
OPENWARD_DB=/var/lib/openward/openward.db
OPENWARD_BIND=127.0.0.1:3000
OPENWARD_BACKUP_DIR=/var/lib/openward/backups
OPENWARD_BACKUP_RETENTION=30
OPENWARD_SESSION_SECRET=$SECRET
OPENWARD_CAPACITY=150
OPENWARD_LEGAL_PRESET=en-DM
RUST_LOG=openward_server=info
EOF
chmod 600 /etc/openward/openward.env
chown openward:openward /etc/openward/openward.env
```

`OPENWARD_BIND=127.0.0.1:3000` is deliberate: the server is only
reachable through Caddy. The firewall blocks 3000 anyway, but binding to
loopback removes the attack surface entirely.

### 7. systemd unit

```bash
install -m 0644 /opt/openward-src/deploy/openward.service /etc/systemd/system/openward.service
systemctl daemon-reload
systemctl enable --now openward
journalctl -u openward -n 50 --no-pager   # confirm "starting OpenWard"
```

### 8. Seed the demo data

```bash
sudo -u openward /opt/openward/openward-seed \
    --db /var/lib/openward/openward.db \
    --count 80
```

Re-running without `--force` aborts — this is the safety against
accidental reseeding.

### 9. Caddy

Create and chown the log directory **before** loading the new
Caddyfile. The reload runs under the already-running `caddy` user; if
the log path doesn't exist yet, Caddy will fail to open the log writer
and refuse the new config.

```bash
mkdir -p /var/log/caddy
chown caddy:caddy /var/log/caddy
install -m 0644 /opt/openward-src/deploy/Caddyfile /etc/caddy/Caddyfile
caddy validate --config /etc/caddy/Caddyfile
systemctl reload caddy
journalctl -u caddy -n 30 --no-pager      # look for "certificate obtained"
```

If a previous reload attempt left a stale `openward-demo.log` owned by
root, delete it before retrying — Caddy cannot reopen a root-owned
file as its own user.

### 10. Smoke test

```bash
curl -fsS https://openward-demo.maxisaacs.com/health
curl -fI  http://openward-demo.maxisaacs.com    # expect 308 redirect
curl -vI https://openward-demo.maxisaacs.com 2>&1 | grep -i issuer
```

Then browse to `https://openward-demo.maxisaacs.com`, log in with
`admin` / `changeme`, change the password when prompted, and click
through the dashboard, population list, a detainee detail page, and the
audit trail.

### 11. Update path

For minor fixes during the demo period:

```bash
sudo -u openward bash -c 'cd /opt/openward-src && git pull && cargo build --release --workspace'
install -m 0755 /opt/openward-src/target/release/openward-server /opt/openward/openward-server
install -m 0755 /opt/openward-src/target/release/openward-seed /opt/openward/openward-seed
systemctl restart openward
```
