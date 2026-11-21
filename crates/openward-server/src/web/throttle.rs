//! Login brute-force throttling.
//!
//! Failed logins are counted per username and per client IP over a sliding
//! window. Once either key reaches `MAX_FAILURES`, further attempts for that
//! key are refused until the oldest failure ages out. A successful login
//! clears the username's counter. State is in memory: a restart resets it,
//! which is acceptable for a single-process server.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use axum::extract::ConnectInfo;
use axum::http::{Extensions, HeaderMap};

pub const MAX_FAILURES: usize = 5;
pub const WINDOW: Duration = Duration::from_secs(15 * 60);

#[derive(Default)]
struct Failures {
    by_key: HashMap<String, Vec<Instant>>,
}

impl Failures {
    fn prune(&mut self, key: &str, now: Instant) -> usize {
        let Some(times) = self.by_key.get_mut(key) else { return 0 };
        times.retain(|t| now.duration_since(*t) < WINDOW);
        let n = times.len();
        if n == 0 {
            self.by_key.remove(key);
        }
        n
    }
}

fn store() -> &'static Mutex<Failures> {
    static STORE: OnceLock<Mutex<Failures>> = OnceLock::new();
    STORE.get_or_init(Default::default)
}

fn user_key(username: &str) -> String {
    format!("u:{}", username.to_lowercase())
}

fn ip_key(ip: Option<IpAddr>) -> Option<String> {
    ip.map(|ip| format!("ip:{ip}"))
}

/// True if a login attempt for this username/IP should be refused.
pub fn is_locked(username: &str, ip: Option<IpAddr>) -> bool {
    let now = Instant::now();
    let mut f = store().lock().unwrap();
    if f.prune(&user_key(username), now) >= MAX_FAILURES {
        return true;
    }
    match ip_key(ip) {
        Some(k) => f.prune(&k, now) >= MAX_FAILURES,
        None => false,
    }
}

pub fn record_failure(username: &str, ip: Option<IpAddr>) {
    let now = Instant::now();
    let mut f = store().lock().unwrap();
    // Keep memory bounded under username spraying: sweep expired keys.
    if f.by_key.len() > 10_000 {
        f.by_key.retain(|_, times| {
            times.retain(|t| now.duration_since(*t) < WINDOW);
            !times.is_empty()
        });
    }
    f.by_key.entry(user_key(username)).or_default().push(now);
    if let Some(k) = ip_key(ip) {
        f.by_key.entry(k).or_default().push(now);
    }
}

pub fn record_success(username: &str) {
    store().lock().unwrap().by_key.remove(&user_key(username));
}

/// Best-effort client IP. When the peer is loopback we are behind the
/// reverse proxy (Caddy), which appends the real client to
/// `X-Forwarded-For`; take the last entry, the one the proxy added.
pub fn client_ip(headers: &HeaderMap, extensions: &Extensions) -> Option<IpAddr> {
    let peer = extensions
        .get::<ConnectInfo<std::net::SocketAddr>>()
        .map(|ci| ci.0.ip());
    match peer {
        Some(ip) if !ip.is_loopback() => Some(ip),
        _ => headers
            .get("x-forwarded-for")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.rsplit(',').next())
            .and_then(|v| v.trim().parse().ok())
            .or(peer),
    }
}
