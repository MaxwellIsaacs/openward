use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use uuid::Uuid;

use openward_core::{AuditEntry, DetaineeId, ModuleName, OperatorId};

/// Maximum entries per epoch before starting a new one.
const ENTRIES_PER_EPOCH: u64 = 1000;

/// Compute the self-hash of an audit entry (all content fields, excluding
/// self_hash and chain_hash themselves).
pub fn compute_self_hash(
    id: &Uuid,
    timestamp: &DateTime<Utc>,
    operator: &OperatorId,
    module: &ModuleName,
    action: &str,
    target: Option<&DetaineeId>,
    before: Option<&serde_json::Value>,
    after: Option<&serde_json::Value>,
    epoch: u64,
) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(id.as_bytes());
    hasher.update(timestamp.to_rfc3339().as_bytes());
    hasher.update(operator.as_uuid().as_bytes());
    hasher.update(format!("{:?}", module).as_bytes());
    hasher.update(action.as_bytes());
    if let Some(t) = target {
        hasher.update(t.as_uuid().as_bytes());
    }
    if let Some(b) = before {
        hasher.update(b.to_string().as_bytes());
    }
    if let Some(a) = after {
        hasher.update(a.to_string().as_bytes());
    }
    hasher.update(epoch.to_le_bytes());
    hasher.finalize().into()
}

/// Compute the chain hash: SHA-256(previous_chain_hash || self_hash).
pub fn compute_chain_hash(
    previous_chain_hash: &[u8; 32],
    self_hash: &[u8; 32],
) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(previous_chain_hash);
    hasher.update(self_hash);
    hasher.finalize().into()
}

/// Create and persist an audit entry. Returns the created entry.
pub async fn create_audit_entry(
    pool: &SqlitePool,
    operator: OperatorId,
    module: ModuleName,
    action: &str,
    target: Option<DetaineeId>,
    before: Option<serde_json::Value>,
    after: Option<serde_json::Value>,
) -> Result<AuditEntry, sqlx::Error> {
    let id = Uuid::new_v4();
    let timestamp = Utc::now();

    // Get the current epoch and previous chain hash
    let (epoch, prev_chain_hash) = get_current_epoch_info(pool).await?;

    let self_hash = compute_self_hash(
        &id,
        &timestamp,
        &operator,
        &module,
        action,
        target.as_ref(),
        before.as_ref(),
        after.as_ref(),
        epoch,
    );

    let chain_hash = compute_chain_hash(&prev_chain_hash, &self_hash);

    let entry = AuditEntry {
        id,
        timestamp,
        operator,
        module,
        action: action.to_string(),
        target,
        before,
        after,
        self_hash,
        chain_hash,
        epoch,
    };

    // Persist
    let id_str = entry.id.to_string();
    let ts_str = entry.timestamp.to_rfc3339();
    let op_str = entry.operator.as_uuid().to_string();
    let mod_str = entry.module.to_string();
    let target_str = entry.target.map(|t| t.to_string());
    let before_str = entry.before.as_ref().map(|v| v.to_string());
    let after_str = entry.after.as_ref().map(|v| v.to_string());

    sqlx::query(
        "INSERT INTO audit_entries (id, timestamp, operator, module, action, target, before_data, after_data, self_hash, chain_hash, epoch) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
    )
    .bind(&id_str)
    .bind(&ts_str)
    .bind(&op_str)
    .bind(&mod_str)
    .bind(&entry.action)
    .bind(&target_str)
    .bind(&before_str)
    .bind(&after_str)
    .bind(entry.self_hash.as_slice())
    .bind(entry.chain_hash.as_slice())
    .bind(entry.epoch as i64)
    .execute(pool)
    .await?;

    // Check if we need to start a new epoch
    let entry_count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM audit_entries WHERE epoch = ?"
    )
    .bind(epoch as i64)
    .fetch_one(pool)
    .await?;

    if entry_count.0 >= ENTRIES_PER_EPOCH as i64 {
        start_new_epoch(pool, epoch + 1, &chain_hash).await?;
    }

    Ok(entry)
}

/// Get the current epoch number and the last chain hash in that epoch.
async fn get_current_epoch_info(pool: &SqlitePool) -> Result<(u64, [u8; 32]), sqlx::Error> {
    // Try to get the latest audit entry
    let row: Option<(i64, Vec<u8>)> = sqlx::query_as(
        "SELECT epoch, chain_hash FROM audit_entries ORDER BY rowid DESC LIMIT 1"
    )
    .fetch_optional(pool)
    .await?;

    match row {
        Some((epoch, hash_bytes)) => {
            let mut hash = [0u8; 32];
            if hash_bytes.len() == 32 {
                hash.copy_from_slice(&hash_bytes);
            }
            Ok((epoch as u64, hash))
        }
        None => {
            // First entry ever — epoch 0 with a zero hash as the genesis.
            // Also create the initial epoch record.
            let checkpoint_hash = [0u8; 32];
            let _ = start_new_epoch(pool, 0, &checkpoint_hash).await;
            Ok((0, checkpoint_hash))
        }
    }
}

/// Start a new epoch with the given checkpoint hash.
async fn start_new_epoch(
    pool: &SqlitePool,
    epoch: u64,
    checkpoint_hash: &[u8; 32],
) -> Result<(), sqlx::Error> {
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT OR IGNORE INTO audit_epochs (epoch, started_at, checkpoint_hash, entry_count) VALUES (?, ?, ?, 0)"
    )
    .bind(epoch as i64)
    .bind(&now)
    .bind(checkpoint_hash.as_slice())
    .execute(pool)
    .await?;
    Ok(())
}

/// Verify a single audit entry's self-hash.
pub fn verify_entry(entry: &AuditEntry) -> bool {
    let computed = compute_self_hash(
        &entry.id,
        &entry.timestamp,
        &entry.operator,
        &entry.module,
        &entry.action,
        entry.target.as_ref(),
        entry.before.as_ref(),
        entry.after.as_ref(),
        entry.epoch,
    );
    computed == entry.self_hash
}
