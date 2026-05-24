use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    response::IntoResponse,
};
use openward_core::RegistryError;
use openward_registry::audit::{
    self, AuditEntryWithOperator, AuditQuery, BreakType,
};
use uuid::Uuid;

use super::{nav_context, require_role, AppState_};
use crate::auth::Action;
use crate::i18n::T;
use crate::templates::*;
use crate::web::errors::HtmlError;
use crate::web::util;

fn db_err(e: sqlx::Error) -> HtmlError {
    HtmlError::Registry(RegistryError::Database(e.to_string()))
}

const PAGE_SIZE: u32 = 25;

fn module_i18n_key(module: &openward_core::ModuleName) -> &'static str {
    match module {
        openward_core::ModuleName::Registry => "audit-module-registry",
        openward_core::ModuleName::Medical => "audit-module-medical",
        openward_core::ModuleName::Disciplinary => "audit-module-disciplinary",
        openward_core::ModuleName::Commissary => "audit-module-commissary",
        openward_core::ModuleName::Visitors => "audit-module-visitors",
        openward_core::ModuleName::Analytics => "audit-module-analytics",
        openward_core::ModuleName::Auth => "audit-module-auth",
        openward_core::ModuleName::Config => "audit-module-config",
    }
}

/// Truncate a raw JSON blob for fallback display.
fn truncate_raw(v: &serde_json::Value, max: usize) -> String {
    let s = v.to_string();
    if s.chars().count() > max {
        let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
        out.push('…');
        out
    } else {
        s
    }
}

/// Extract a string field from a JSON object (tolerant: tries plain strings,
/// unit-variant enum names as `"Variant"`, and single-key `{"Variant": …}`
/// tagged-variant objects).
fn extract_variant_name(v: &serde_json::Value) -> Option<String> {
    match v {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Object(m) if m.len() == 1 => m.keys().next().cloned(),
        _ => None,
    }
}

/// Translate a localized label for a `DetentionBasisLabel` stringly. The
/// basis JSON is typically a tagged enum like `{"OnTrial": {...}}`; for
/// plain unit variants like `"Sentenced"` it's just the string.
fn localized_basis_label(t: &T, name: &str) -> String {
    let key = match name {
        "NoLegalBasis" => Some("legal-basis-no-legal"),
        "PoliceCustody" => Some("legal-basis-police-custody"),
        "Remand" => Some("legal-basis-remand"),
        "OnTrial" => Some("legal-basis-on-trial"),
        "ConvictedUnsentenced" => Some("legal-basis-convicted-unsentenced"),
        "Sentenced" => Some("legal-basis-sentenced"),
        "Appeal" => Some("legal-basis-appeal"),
        _ => None,
    };
    match key {
        Some(k) => t.get(k),
        None => name.to_string(),
    }
}

fn localized_status_label(t: &T, name: &str) -> String {
    let key = match name {
        "Present" => Some("legal-status-present"),
        "InCourt" => Some("legal-status-in-court"),
        "InHospital" => Some("legal-status-in-hospital"),
        "Transferred" => Some("legal-status-transferred"),
        "Released" => Some("legal-status-released"),
        "Escaped" => Some("legal-status-escaped"),
        "Deceased" => Some("legal-status-deceased"),
        _ => None,
    };
    match key {
        Some(k) => t.get(k),
        None => name.to_string(),
    }
}

/// Build a short, human-readable summary of an audit entry based on its
/// action name and before/after payloads. Falls back to a truncated JSON
/// blob when the action is unknown or the payload can't be parsed.
pub fn format_details(
    t: &T,
    action: &str,
    before: Option<&serde_json::Value>,
    after: Option<&serde_json::Value>,
) -> String {
    match action {
        "release" => {
            let release_type = after
                .and_then(|v| v.get("release_type"))
                .and_then(extract_variant_name);
            match release_type {
                Some(rt) => format!("{} ({})", t.get("audit-summary-release"), rt),
                None => t.get("audit-summary-release"),
            }
        }
        "admit" => {
            let surname = after
                .and_then(|v| v.pointer("/identity/surname"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let given = after
                .and_then(|v| v.pointer("/identity/given_names"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let name = if !surname.is_empty() && !given.is_empty() {
                format!("{}, {}", surname, given)
            } else if !surname.is_empty() {
                surname.to_string()
            } else {
                given.to_string()
            };
            if name.is_empty() {
                t.get("audit-summary-admitted")
            } else {
                format!("{}: {}", t.get("audit-summary-admitted"), name)
            }
        }
        "update_detention_basis" => {
            let label = after
                .and_then(extract_variant_name)
                .map(|n| localized_basis_label(t, &n));
            match label {
                Some(l) => format!("{} → {}", t.get("audit-summary-basis-change"), l),
                None => t.get("audit-summary-basis-change"),
            }
        }
        "update_facility_status" => {
            let label = after
                .and_then(extract_variant_name)
                .map(|n| localized_status_label(t, &n));
            match label {
                Some(l) => format!("{} → {}", t.get("audit-summary-status-change"), l),
                None => t.get("audit-summary-status-change"),
            }
        }
        "assign_housing" => t.get("audit-summary-assigned-housing"),
        "schedule_court_date" | "update_court_date" => {
            let date = after
                .and_then(|v| v.get("scheduled_date"))
                .and_then(|v| v.as_str());
            match date {
                Some(d) => format!("{}: {}", t.get("audit-summary-court-date"), d),
                None => t.get("audit-summary-court-date"),
            }
        }
        "register_warrant" => t.get("audit-summary-warrant-registered"),
        "record_court_outcome" => {
            let outcome = after
                .and_then(|v| v.get("outcome"))
                .and_then(extract_variant_name)
                .or_else(|| after.and_then(extract_variant_name));
            match outcome {
                Some(o) => format!("{}: {}", t.get("audit-summary-court-outcome"), o),
                None => t.get("audit-summary-court-outcome"),
            }
        }
        "transfer" => t.get("audit-summary-transfer"),
        "add_note" => t.get("audit-summary-note-added"),
        "delete_note" => t.get("audit-summary-note-deleted"),
        "add_property_item" => t.get("audit-summary-property-added"),
        "return_property_item" => t.get("audit-summary-property-returned"),
        _ => {
            // Unknown — truncate the raw payload.
            match after.or(before) {
                Some(v) => truncate_raw(v, 60),
                None => String::new(),
            }
        }
    }
}

pub fn convert_audit_entry(t: &T, raw: AuditEntryWithOperator) -> AuditEntryDisplay {
    let details = format_details(t, &raw.action, raw.before.as_ref(), raw.after.as_ref());
    AuditEntryDisplay {
        id: raw.id.to_string(),
        timestamp: raw.timestamp.format("%Y-%m-%d %H:%M:%S").to_string(),
        operator_name: raw.operator_name,
        module: t.get(module_i18n_key(&raw.module)),
        action: raw.action,
        target_id: raw.target_id.map(|id| id.to_string()),
        target_name: raw.target_name,
        details,
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

/// Humanize a snake_case JSON key into Title Case.
fn humanize_key(key: &str) -> String {
    let mut out = String::with_capacity(key.len());
    let mut next_upper = true;
    for ch in key.chars() {
        if ch == '_' || ch == '-' {
            out.push(' ');
            next_upper = true;
        } else if next_upper {
            out.extend(ch.to_uppercase());
            next_upper = false;
        } else {
            out.push(ch);
        }
    }
    out
}

fn flatten_fields(value: &serde_json::Value) -> Vec<AuditDetailField> {
    fn value_to_string(v: &serde_json::Value) -> String {
        match v {
            serde_json::Value::String(s) => s.clone(),
            serde_json::Value::Null => "—".to_string(),
            serde_json::Value::Bool(b) => b.to_string(),
            serde_json::Value::Number(n) => n.to_string(),
            serde_json::Value::Array(_) | serde_json::Value::Object(_) => {
                let s = v.to_string();
                if s.chars().count() > 200 {
                    let mut out: String = s.chars().take(199).collect();
                    out.push('…');
                    out
                } else {
                    s
                }
            }
        }
    }

    let mut fields = Vec::new();
    match value {
        serde_json::Value::Object(map) => {
            for (k, v) in map {
                if let serde_json::Value::Object(inner) = v {
                    // Flatten one level: prefix with parent key.
                    for (ik, iv) in inner {
                        fields.push(AuditDetailField {
                            label: format!("{} / {}", humanize_key(k), humanize_key(ik)),
                            value: value_to_string(iv),
                        });
                    }
                } else {
                    fields.push(AuditDetailField {
                        label: humanize_key(k),
                        value: value_to_string(v),
                    });
                }
            }
        }
        other => {
            fields.push(AuditDetailField {
                label: "value".to_string(),
                value: value_to_string(other),
            });
        }
    }
    fields
}

pub async fn audit_page(
    State(state): State<AppState_>,
    headers: HeaderMap,
    Query(query): Query<AuditQuery>,
) -> Result<impl IntoResponse, HtmlError> {
    let session =
        require_role(&headers, state.registry.pool(), Action::ViewAuditTrail).await?;
    let t = T::new(&session.lang, state.legal_overrides.clone());

    let result =
        audit::list_audit_entries(state.registry.pool(), &query, PAGE_SIZE).await.map_err(db_err)?;
    let operators_brief =
        audit::list_operators_brief(state.registry.pool()).await.map_err(db_err)?;

    let page = query.page.unwrap_or(1).max(1);
    let total_pages = if result.total_count == 0 {
        1
    } else {
        (result.total_count + PAGE_SIZE - 1) / PAGE_SIZE
    };

    let entries: Vec<AuditEntryDisplay> = result
        .entries
        .into_iter()
        .map(|e| convert_audit_entry(&t, e))
        .collect();

    let operators: Vec<OperatorOption> = operators_brief
        .into_iter()
        .map(|o| OperatorOption {
            id: o.id,
            display_name: o.display_name,
        })
        .collect();

    Ok(AuditPageTemplate {
        nav: nav_context(&session, "/audit"),
        entries,
        operators,
        total_count: result.total_count,
        page,
        total_pages,
        t,
    })
}

pub async fn search_fragment(
    State(state): State<AppState_>,
    headers: HeaderMap,
    Query(query): Query<AuditQuery>,
) -> Result<impl IntoResponse, HtmlError> {
    let session =
        require_role(&headers, state.registry.pool(), Action::ViewAuditTrail).await?;
    let t = T::new(&session.lang, state.legal_overrides.clone());

    let result =
        audit::list_audit_entries(state.registry.pool(), &query, PAGE_SIZE).await.map_err(db_err)?;

    let page = query.page.unwrap_or(1).max(1);
    let total_pages = if result.total_count == 0 {
        1
    } else {
        (result.total_count + PAGE_SIZE - 1) / PAGE_SIZE
    };

    let entries: Vec<AuditEntryDisplay> = result
        .entries
        .into_iter()
        .map(|e| convert_audit_entry(&t, e))
        .collect();

    Ok(AuditTableFragment {
        entries,
        total_count: result.total_count,
        page,
        total_pages,
        t,
    })
}

pub async fn detail_page(
    State(state): State<AppState_>,
    headers: HeaderMap,
    Path(entry_id): Path<String>,
) -> Result<impl IntoResponse, HtmlError> {
    let session =
        require_role(&headers, state.registry.pool(), Action::ViewAuditTrail).await?;
    let t = T::new(&session.lang, state.legal_overrides.clone());

    let id = Uuid::parse_str(&entry_id).map_err(|_| HtmlError::NotFound)?;
    let detail = audit::get_audit_entry_by_id(state.registry.pool(), id)
        .await
        .map_err(db_err)?
        .ok_or(HtmlError::NotFound)?;

    let summary = format_details(
        &t,
        &detail.entry.action,
        detail.entry.before.as_ref(),
        detail.entry.after.as_ref(),
    );
    let action_label = util::action_label(&t, &detail.entry.action);

    let before_fields = detail
        .entry
        .before
        .as_ref()
        .map(flatten_fields)
        .unwrap_or_default();
    let after_fields = detail
        .entry
        .after
        .as_ref()
        .map(flatten_fields)
        .unwrap_or_default();

    let display = AuditDetailDisplay {
        id: detail.entry.id.to_string(),
        timestamp: detail.entry.timestamp.format("%Y-%m-%d %H:%M:%S UTC").to_string(),
        operator_name: detail.entry.operator_name,
        operator_id: detail.entry.operator_id.as_uuid().to_string(),
        module: t.get(module_i18n_key(&detail.entry.module)),
        action_raw: detail.entry.action.clone(),
        action_label,
        summary,
        target_id: detail.entry.target_id.map(|t| t.to_string()),
        target_name: detail.entry.target_name,
        epoch: detail.entry.epoch,
        before_fields,
        after_fields,
        self_hash_hex: hex_encode(&detail.self_hash),
        chain_hash_hex: hex_encode(&detail.chain_hash),
        self_hash_verified: detail.self_hash_verified,
        chain_verified: detail.chain_verified,
    };

    Ok(AuditDetailTemplate {
        nav: nav_context(&session, "/audit"),
        entry: display,
        t,
    })
}

pub async fn verify_page(
    State(state): State<AppState_>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, HtmlError> {
    let session =
        require_role(&headers, state.registry.pool(), Action::ViewAuditTrail).await?;
    let t = T::new(&session.lang, state.legal_overrides.clone());

    let verification = audit::verify_chain(state.registry.pool()).await.map_err(db_err)?;

    let breaks: Vec<BreakDisplay> = verification
        .breaks
        .into_iter()
        .map(|b| BreakDisplay {
            entry_index: b.entry_index,
            entry_id: b.entry_id,
            epoch: b.epoch,
            break_type: match b.break_type {
                BreakType::SelfHashMismatch => t.get("audit-break-self-hash"),
                BreakType::ChainHashMismatch => t.get("audit-break-chain-hash"),
            },
        })
        .collect();

    Ok(AuditVerifyTemplate {
        nav: nav_context(&session, "/audit/verify"),
        result: VerificationDisplay {
            total_entries: verification.total_entries,
            total_epochs: verification.total_epochs,
            is_valid: verification.is_valid,
            breaks,
        },
        t,
    })
}
