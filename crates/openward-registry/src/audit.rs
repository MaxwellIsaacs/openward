use chrono::{DateTime, NaiveDate, Utc};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use uuid::Uuid;

use openward_core::{AuditEntry, DetaineeId, ModuleName, OperatorId};

// =========================================================================
// Query types
// =========================================================================

/// Filter parameters for paginated audit log queries.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct AuditQuery {
    pub date_from: Option<NaiveDate>,
    pub date_to: Option<NaiveDate>,
    pub operator: Option<String>,
    pub module: Option<String>,
    pub target: Option<String>,
    pub page: Option<u32>,
}

/// Paginated result of an audit query.
pub struct AuditQueryResult {
    pub entries: Vec<AuditEntryWithOperator>,
    pub total_count: u32,
}

/// An audit entry enriched with human-readable operator and target names.
pub struct AuditEntryWithOperator {
    pub id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub operator_id: OperatorId,
    pub operator_name: String,
    pub module: ModuleName,
    pub action: String,
    pub target_id: Option<DetaineeId>,
    pub target_name: Option<String>,
    pub before: Option<serde_json::Value>,
    pub after: Option<serde_json::Value>,
    pub epoch: u64,
}

/// Result of a chain integrity verification.
pub struct ChainVerificationResult {
    pub total_entries: u64,
    pub total_epochs: u64,
    pub is_valid: bool,
    pub breaks: Vec<ChainBreak>,
}

/// A single break detected in the hash chain.
pub struct ChainBreak {
    pub entry_index: u64,
    pub entry_id: String,
    pub epoch: u64,
    pub break_type: BreakType,
}

/// Type of chain integrity break.
pub enum BreakType {
    SelfHashMismatch,
    ChainHashMismatch,
}

/// Brief operator record for filter dropdowns.
pub struct OperatorBrief {
    pub id: String,
    pub display_name: String,
}

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

// =========================================================================
// Query functions
// =========================================================================

/// Paginated, filtered audit log query with operator and detainee name JOINs.
pub async fn list_audit_entries(
    pool: &SqlitePool,
    query: &AuditQuery,
    page_size: u32,
) -> Result<AuditQueryResult, sqlx::Error> {
    let page = query.page.unwrap_or(1).max(1);
    let offset = (page - 1) * page_size;

    let mut wheres: Vec<String> = Vec::new();

    if let Some(ref date_from) = query.date_from {
        wheres.push(format!("ae.timestamp >= '{}'", date_from));
    }
    if let Some(ref date_to) = query.date_to {
        // Include the full day
        wheres.push(format!("ae.timestamp < '{}T23:59:59Z'", date_to));
    }
    if let Some(ref op) = query.operator {
        if !op.is_empty() {
            wheres.push(format!("ae.operator = '{}'", op.replace('\'', "''")));
        }
    }
    if let Some(ref module) = query.module {
        if !module.is_empty() {
            wheres.push(format!("ae.module = '{}'", module.replace('\'', "''")));
        }
    }
    if let Some(ref target) = query.target {
        if !target.is_empty() {
            wheres.push(format!("ae.target = '{}'", target.replace('\'', "''")));
        }
    }

    let where_clause = if wheres.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", wheres.join(" AND "))
    };

    // Total count
    let count_sql = format!(
        "SELECT COUNT(*) FROM audit_entries ae {}",
        where_clause
    );
    let (total,): (i64,) = sqlx::query_as(&count_sql)
        .fetch_one(pool)
        .await?;

    // Fetch page
    let select_sql = format!(
        "SELECT ae.id, ae.timestamp, ae.operator, ae.module, ae.action, ae.target, \
         ae.before_data, ae.after_data, ae.epoch, \
         o.display_name AS operator_name, \
         COALESCE(d.surname || ', ' || d.given_names, '') AS target_name \
         FROM audit_entries ae \
         JOIN operators o ON ae.operator = o.id \
         LEFT JOIN detainees d ON ae.target = d.id \
         {} \
         ORDER BY ae.timestamp DESC \
         LIMIT ? OFFSET ?",
        where_clause
    );

    let rows: Vec<(String, String, String, String, String, Option<String>, Option<String>, Option<String>, i64, String, String)> =
        sqlx::query_as(&select_sql)
            .bind(page_size)
            .bind(offset)
            .fetch_all(pool)
            .await?;

    let entries = rows
        .into_iter()
        .map(|row| {
            let module = row.3.parse::<ModuleName>().unwrap_or(ModuleName::Registry);
            AuditEntryWithOperator {
                id: Uuid::parse_str(&row.0).unwrap_or_default(),
                timestamp: DateTime::parse_from_rfc3339(&row.1)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
                operator_id: OperatorId::from_uuid(
                    Uuid::parse_str(&row.2).unwrap_or_default(),
                ),
                operator_name: row.9,
                module,
                action: row.4,
                target_id: row.5.as_ref().and_then(|s| {
                    Uuid::parse_str(s).ok().map(DetaineeId::from_uuid)
                }),
                target_name: if row.10.is_empty() { None } else { Some(row.10) },
                before: row.6.and_then(|s| serde_json::from_str(&s).ok()),
                after: row.7.and_then(|s| serde_json::from_str(&s).ok()),
                epoch: row.8 as u64,
            }
        })
        .collect();

    Ok(AuditQueryResult {
        entries,
        total_count: total as u32,
    })
}

/// Fetch recent audit entries for a specific detainee (for the detail page).
pub async fn list_audit_for_detainee(
    pool: &SqlitePool,
    detainee_id: DetaineeId,
    limit: u32,
) -> Result<Vec<AuditEntryWithOperator>, sqlx::Error> {
    let target_str = detainee_id.to_string();
    let rows: Vec<(String, String, String, String, String, Option<String>, Option<String>, Option<String>, i64, String, String)> =
        sqlx::query_as(
            "SELECT ae.id, ae.timestamp, ae.operator, ae.module, ae.action, ae.target, \
             ae.before_data, ae.after_data, ae.epoch, \
             o.display_name AS operator_name, \
             COALESCE(d.surname || ', ' || d.given_names, '') AS target_name \
             FROM audit_entries ae \
             JOIN operators o ON ae.operator = o.id \
             LEFT JOIN detainees d ON ae.target = d.id \
             WHERE ae.target = ? \
             ORDER BY ae.timestamp DESC \
             LIMIT ?"
        )
        .bind(&target_str)
        .bind(limit)
        .fetch_all(pool)
        .await?;

    Ok(rows
        .into_iter()
        .map(|row| {
            let module = row.3.parse::<ModuleName>().unwrap_or(ModuleName::Registry);
            AuditEntryWithOperator {
                id: Uuid::parse_str(&row.0).unwrap_or_default(),
                timestamp: DateTime::parse_from_rfc3339(&row.1)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
                operator_id: OperatorId::from_uuid(
                    Uuid::parse_str(&row.2).unwrap_or_default(),
                ),
                operator_name: row.9,
                module,
                action: row.4,
                target_id: row.5.as_ref().and_then(|s| {
                    Uuid::parse_str(s).ok().map(DetaineeId::from_uuid)
                }),
                target_name: if row.10.is_empty() { None } else { Some(row.10) },
                before: row.6.and_then(|s| serde_json::from_str(&s).ok()),
                after: row.7.and_then(|s| serde_json::from_str(&s).ok()),
                epoch: row.8 as u64,
            }
        })
        .collect())
}

/// A single audit entry enriched with hashes and chain verification status.
pub struct AuditEntryDetail {
    pub entry: AuditEntryWithOperator,
    pub self_hash: [u8; 32],
    pub chain_hash: [u8; 32],
    /// True when the stored self_hash recomputes correctly and the
    /// chain_hash matches SHA256(prev_chain_hash || self_hash).
    /// For the first entry in the chain, prev is the zero hash.
    pub chain_verified: bool,
    pub self_hash_verified: bool,
}

/// Fetch a single audit entry by its UUID, enriched with operator/target
/// names and raw hashes, plus a local verification against the preceding
/// entry's chain_hash.
pub async fn get_audit_entry_by_id(
    pool: &SqlitePool,
    id: Uuid,
) -> Result<Option<AuditEntryDetail>, sqlx::Error> {
    let id_str = id.to_string();
    let row: Option<(String, String, String, String, String, Option<String>, Option<String>, Option<String>, i64, String, String, Vec<u8>, Vec<u8>)> =
        sqlx::query_as(
            "SELECT ae.id, ae.timestamp, ae.operator, ae.module, ae.action, ae.target, \
             ae.before_data, ae.after_data, ae.epoch, \
             o.display_name AS operator_name, \
             COALESCE(d.surname || ', ' || d.given_names, '') AS target_name, \
             ae.self_hash, ae.chain_hash \
             FROM audit_entries ae \
             JOIN operators o ON ae.operator = o.id \
             LEFT JOIN detainees d ON ae.target = d.id \
             WHERE ae.id = ?"
        )
        .bind(&id_str)
        .fetch_optional(pool)
        .await?;
    let Some(row) = row else { return Ok(None) };

    let module = row.3.parse::<ModuleName>().unwrap_or(ModuleName::Registry);
    let entry_id = Uuid::parse_str(&row.0).unwrap_or_default();
    let timestamp = DateTime::parse_from_rfc3339(&row.1)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now());
    let operator_id =
        OperatorId::from_uuid(Uuid::parse_str(&row.2).unwrap_or_default());
    let target_id = row.5.as_ref().and_then(|s| {
        Uuid::parse_str(s).ok().map(DetaineeId::from_uuid)
    });
    let before: Option<serde_json::Value> =
        row.6.as_ref().and_then(|s| serde_json::from_str(s).ok());
    let after: Option<serde_json::Value> =
        row.7.as_ref().and_then(|s| serde_json::from_str(s).ok());
    let epoch = row.8 as u64;

    let mut self_hash = [0u8; 32];
    if row.11.len() == 32 {
        self_hash.copy_from_slice(&row.11);
    }
    let mut chain_hash = [0u8; 32];
    if row.12.len() == 32 {
        chain_hash.copy_from_slice(&row.12);
    }

    // Re-derive the self_hash from content fields
    let action_str = row.4.clone();
    let computed_self = compute_self_hash(
        &entry_id,
        &timestamp,
        &operator_id,
        &module,
        &action_str,
        target_id.as_ref(),
        before.as_ref(),
        after.as_ref(),
        epoch,
    );
    let self_hash_verified = computed_self == self_hash;

    // Fetch the previous entry's chain_hash (by rowid order).
    let prev: Option<(Vec<u8>,)> = sqlx::query_as(
        "SELECT chain_hash FROM audit_entries \
         WHERE rowid < (SELECT rowid FROM audit_entries WHERE id = ?) \
         ORDER BY rowid DESC LIMIT 1",
    )
    .bind(&id_str)
    .fetch_optional(pool)
    .await?;

    let prev_chain_hash = match prev {
        Some((bytes,)) if bytes.len() == 32 => {
            let mut h = [0u8; 32];
            h.copy_from_slice(&bytes);
            h
        }
        _ => [0u8; 32],
    };
    let computed_chain = compute_chain_hash(&prev_chain_hash, &computed_self);
    let chain_verified = self_hash_verified && computed_chain == chain_hash;

    let target_name = if row.10.is_empty() { None } else { Some(row.10) };

    let enriched = AuditEntryWithOperator {
        id: entry_id,
        timestamp,
        operator_id,
        operator_name: row.9,
        module,
        action: action_str,
        target_id,
        target_name,
        before,
        after,
        epoch,
    };

    Ok(Some(AuditEntryDetail {
        entry: enriched,
        self_hash,
        chain_hash,
        chain_verified,
        self_hash_verified,
    }))
}

/// Verify the entire audit chain. Streams all entries by rowid and checks
/// self-hash integrity and chain-hash continuity.
pub async fn verify_chain(pool: &SqlitePool) -> Result<ChainVerificationResult, sqlx::Error> {
    let rows: Vec<(String, String, String, String, String, Option<String>, Option<String>, Option<String>, Vec<u8>, Vec<u8>, i64)> =
        sqlx::query_as(
            "SELECT id, timestamp, operator, module, action, target, \
             before_data, after_data, self_hash, chain_hash, epoch \
             FROM audit_entries ORDER BY rowid ASC"
        )
        .fetch_all(pool)
        .await?;

    let total_entries = rows.len() as u64;
    let mut breaks = Vec::new();
    let mut prev_chain_hash = [0u8; 32];
    let mut epochs = std::collections::HashSet::new();

    for (idx, row) in rows.iter().enumerate() {
        let epoch = row.10 as u64;
        epochs.insert(epoch);

        let id = Uuid::parse_str(&row.0).unwrap_or_default();
        let timestamp = DateTime::parse_from_rfc3339(&row.1)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now());
        let operator = OperatorId::from_uuid(Uuid::parse_str(&row.2).unwrap_or_default());
        let module = row.3.parse::<ModuleName>().unwrap_or(ModuleName::Registry);
        let action = &row.4;
        let target = row.5.as_ref().and_then(|s| Uuid::parse_str(s).ok().map(DetaineeId::from_uuid));
        let before: Option<serde_json::Value> = row.6.as_ref().and_then(|s| serde_json::from_str(s).ok());
        let after: Option<serde_json::Value> = row.7.as_ref().and_then(|s| serde_json::from_str(s).ok());

        // Stored hashes
        let mut stored_self_hash = [0u8; 32];
        if row.8.len() == 32 {
            stored_self_hash.copy_from_slice(&row.8);
        }
        let mut stored_chain_hash = [0u8; 32];
        if row.9.len() == 32 {
            stored_chain_hash.copy_from_slice(&row.9);
        }

        // Verify self-hash
        let computed_self = compute_self_hash(
            &id, &timestamp, &operator, &module, action,
            target.as_ref(), before.as_ref(), after.as_ref(), epoch,
        );
        if computed_self != stored_self_hash {
            breaks.push(ChainBreak {
                entry_index: idx as u64,
                entry_id: row.0.clone(),
                epoch,
                break_type: BreakType::SelfHashMismatch,
            });
            // Still update prev for chain continuity check
            prev_chain_hash = stored_chain_hash;
            continue;
        }

        // Verify chain hash
        // At epoch boundaries the prev_chain_hash resets (first entry of new epoch
        // may chain from the epoch checkpoint), but since our create_audit_entry
        // always chains from the previous entry's chain_hash, we just verify
        // continuity entry-by-entry.
        if idx == 0 {
            // First entry: prev is the zero hash (genesis)
            let computed_chain = compute_chain_hash(&[0u8; 32], &computed_self);
            if computed_chain != stored_chain_hash {
                breaks.push(ChainBreak {
                    entry_index: 0,
                    entry_id: row.0.clone(),
                    epoch,
                    break_type: BreakType::ChainHashMismatch,
                });
            }
        } else {
            let computed_chain = compute_chain_hash(&prev_chain_hash, &computed_self);
            if computed_chain != stored_chain_hash {
                breaks.push(ChainBreak {
                    entry_index: idx as u64,
                    entry_id: row.0.clone(),
                    epoch,
                    break_type: BreakType::ChainHashMismatch,
                });
            }
        }

        prev_chain_hash = stored_chain_hash;
    }

    Ok(ChainVerificationResult {
        total_entries,
        total_epochs: epochs.len() as u64,
        is_valid: breaks.is_empty(),
        breaks,
    })
}

/// List all operators (id + display_name) for filter dropdowns.
pub async fn list_operators_brief(pool: &SqlitePool) -> Result<Vec<OperatorBrief>, sqlx::Error> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT id, display_name FROM operators ORDER BY display_name"
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|(id, display_name)| OperatorBrief { id, display_name })
        .collect())
}
