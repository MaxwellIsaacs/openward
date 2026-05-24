use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post, put},
    Json, Router,
};
use chrono::NaiveDate;
use serde::Deserialize;
use uuid::Uuid;

use openward_core::*;
use openward_facility::AppState;
use crate::auth::{Action, can_do};
use crate::web::auth as web_auth;

// === Error handling ===

struct ApiError(RegistryError);

impl From<RegistryError> for ApiError {
    fn from(e: RegistryError) -> Self {
        Self(e)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match &self.0 {
            RegistryError::Domain(d) => match d {
                DomainError::DetaineeNotFound { .. } => {
                    (StatusCode::NOT_FOUND, self.0.to_string())
                }
                DomainError::InactiveDetainee { .. } | DomainError::ReleaseHold { .. } => {
                    (StatusCode::CONFLICT, self.0.to_string())
                }
                DomainError::InvalidBasisTransition { .. } => {
                    (StatusCode::UNPROCESSABLE_ENTITY, self.0.to_string())
                }
                DomainError::FutureDate { .. } | DomainError::ZeroSentence => {
                    (StatusCode::BAD_REQUEST, self.0.to_string())
                }
            },
            RegistryError::Database(_) => {
                tracing::error!("{}", self.0);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal server error".to_string(),
                )
            }
        };

        (status, Json(serde_json::json!({ "error": message }))).into_response()
    }
}

// === Session-based auth for API ===

struct ApiForbidden;

impl IntoResponse for ApiForbidden {
    fn into_response(self) -> Response {
        (StatusCode::FORBIDDEN, Json(serde_json::json!({"error": "forbidden"}))).into_response()
    }
}

struct ApiUnauthorized;

impl IntoResponse for ApiUnauthorized {
    fn into_response(self) -> Response {
        (StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error": "unauthorized"}))).into_response()
    }
}

enum ApiAuthError {
    Unauthorized,
    Forbidden,
}

impl IntoResponse for ApiAuthError {
    fn into_response(self) -> Response {
        match self {
            Self::Unauthorized => ApiUnauthorized.into_response(),
            Self::Forbidden => ApiForbidden.into_response(),
        }
    }
}

impl From<ApiAuthError> for ApiError {
    fn from(e: ApiAuthError) -> Self {
        match e {
            ApiAuthError::Unauthorized => ApiError(RegistryError::Database("unauthorized".to_string())),
            ApiAuthError::Forbidden => ApiError(RegistryError::Database("forbidden".to_string())),
        }
    }
}

async fn require_api_session(headers: &HeaderMap, pool: &sqlx::SqlitePool) -> Result<web_auth::Session, ApiAuthError> {
    web_auth::extract_session(headers, pool).await.ok_or(ApiAuthError::Unauthorized)
}

async fn require_api_role(headers: &HeaderMap, pool: &sqlx::SqlitePool, action: Action) -> Result<web_auth::Session, ApiAuthError> {
    let session = require_api_session(headers, pool).await?;
    if !can_do(&session.role, action) {
        return Err(ApiAuthError::Forbidden);
    }
    Ok(session)
}

fn extract_operator_from_session(session: &web_auth::Session) -> OperatorId {
    session.operator_id
}

fn auth_err(e: ApiAuthError) -> ApiError {
    match e {
        ApiAuthError::Unauthorized => ApiError(RegistryError::Database("unauthorized".into())),
        ApiAuthError::Forbidden => ApiError(RegistryError::Database("forbidden".into())),
    }
}


fn parse_detainee_id(id: &str) -> Result<DetaineeId, ApiError> {
    let uuid = Uuid::parse_str(id).map_err(|_| {
        ApiError(RegistryError::Domain(DomainError::DetaineeNotFound {
            id: DetaineeId::from_uuid(Uuid::nil()),
        }))
    })?;
    Ok(DetaineeId::from_uuid(uuid))
}

fn parse_court_date_id(id: &str) -> Result<CourtDateId, ApiError> {
    let uuid = Uuid::parse_str(id).map_err(|_| {
        ApiError(RegistryError::Database(
            "invalid court date ID".to_string(),
        ))
    })?;
    Ok(CourtDateId::from_uuid(uuid))
}

fn parse_date(date: &str) -> Result<NaiveDate, ApiError> {
    NaiveDate::parse_from_str(date, "%Y-%m-%d").map_err(|_| {
        ApiError(RegistryError::Database(
            "invalid date format, expected YYYY-MM-DD".to_string(),
        ))
    })
}

type AppState_ = Arc<AppState>;

// === Health check ===

async fn health_check(
    State(state): State<AppState_>,
) -> Json<serde_json::Value> {
    let db_ok = sqlx::query_scalar::<_, i32>("SELECT 1")
        .fetch_one(state.registry.pool())
        .await
        .is_ok();

    let uptime = state.started_at.elapsed().as_secs();
    let version = env!("CARGO_PKG_VERSION");

    Json(serde_json::json!({
        "status": if db_ok { "ok" } else { "degraded" },
        "database": if db_ok { "connected" } else { "unreachable" },
        "uptime_seconds": uptime,
        "version": version,
    }))
}

// === Route handlers ===

// GET /api/overview
async fn get_overview(
    State(state): State<AppState_>,
    headers: HeaderMap,
) -> Result<Json<FacilityOverview>, ApiError> {
    require_api_session(&headers, state.registry.pool()).await.map_err(auth_err)?;
    let overview = state.registry.overview().await?;
    Ok(Json(overview))
}

// GET /api/detainees
async fn search_detainees(
    State(state): State<AppState_>,
    headers: HeaderMap,
    Query(query): Query<PopulationQuery>,
) -> Result<Json<PopulationQueryResult>, ApiError> {
    let session = require_api_role(&headers, state.registry.pool(), Action::ViewPopulation).await.map_err(auth_err)?;
    let op = extract_operator_from_session(&session);
    let result = state.registry.search(query, op).await?;
    Ok(Json(result))
}

// POST /api/detainees
async fn admit_detainee(
    State(state): State<AppState_>,
    headers: HeaderMap,
    Json(record): Json<AdmissionRecord>,
) -> Result<(StatusCode, Json<Detainee>), ApiError> {
    require_api_role(&headers, state.registry.pool(), Action::AdmitDetainee).await.map_err(auth_err)?;
    let detainee = state.registry.admit(record).await?;
    Ok((StatusCode::CREATED, Json(detainee)))
}

// POST /api/detainees/batch
async fn batch_admit(
    State(state): State<AppState_>,
    headers: HeaderMap,
    Json(batch): Json<BatchAdmission>,
) -> Result<(StatusCode, Json<Vec<Detainee>>), ApiError> {
    require_api_role(&headers, state.registry.pool(), Action::AdmitDetainee).await.map_err(auth_err)?;
    let detainees = state.registry.batch_admit(batch).await?;
    Ok((StatusCode::CREATED, Json(detainees)))
}

// GET /api/detainees/:id
async fn get_detainee(
    State(state): State<AppState_>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Detainee>, ApiError> {
    require_api_role(&headers, state.registry.pool(), Action::ViewDetaineeDetail).await.map_err(auth_err)?;
    let detainee_id = parse_detainee_id(&id)?;
    let detainee = state.registry.get_detainee(detainee_id).await?;
    Ok(Json(detainee))
}

// PUT /api/detainees/:id/basis
async fn update_basis(
    State(state): State<AppState_>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(new_basis): Json<DetentionBasis>,
) -> Result<Json<Detainee>, ApiError> {
    let session = require_api_role(&headers, state.registry.pool(), Action::UpdateBasisStatusHousing).await.map_err(auth_err)?;
    let detainee_id = parse_detainee_id(&id)?;
    let op = extract_operator_from_session(&session);
    let detainee = state
        .registry
        .update_detention_basis(detainee_id, new_basis, op)
        .await?;
    Ok(Json(detainee))
}

// PUT /api/detainees/:id/status
#[derive(Deserialize)]
struct UpdateStatusBody {
    status: FacilityStatus,
    notes: Option<String>,
}

async fn update_status(
    State(state): State<AppState_>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<UpdateStatusBody>,
) -> Result<Json<Detainee>, ApiError> {
    let session = require_api_role(&headers, state.registry.pool(), Action::UpdateBasisStatusHousing).await.map_err(auth_err)?;
    let detainee_id = parse_detainee_id(&id)?;
    let op = extract_operator_from_session(&session);
    let detainee = state
        .registry
        .update_facility_status(detainee_id, body.status, op, body.notes)
        .await?;
    Ok(Json(detainee))
}

// PUT /api/detainees/:id/housing
#[derive(Deserialize)]
struct AssignHousingBody {
    unit_id: Uuid,
}

async fn assign_housing(
    State(state): State<AppState_>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<AssignHousingBody>,
) -> Result<Json<Detainee>, ApiError> {
    let session = require_api_role(&headers, state.registry.pool(), Action::UpdateBasisStatusHousing).await.map_err(auth_err)?;
    let detainee_id = parse_detainee_id(&id)?;
    let op = extract_operator_from_session(&session);
    let unit = HousingUnitId::from_uuid(body.unit_id);
    let detainee = state
        .registry
        .assign_housing(detainee_id, unit, op)
        .await?;
    Ok(Json(detainee))
}

// POST /api/detainees/:id/warrants
async fn register_warrant(
    State(state): State<AppState_>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(mut order): Json<CommitmentOrder>,
) -> Result<(StatusCode, Json<Detainee>), ApiError> {
    let session = require_api_role(&headers, state.registry.pool(), Action::UpdateBasisStatusHousing).await.map_err(auth_err)?;
    let detainee_id = parse_detainee_id(&id)?;
    let op = extract_operator_from_session(&session);
    order.detainee_id = detainee_id;
    let detainee = state.registry.register_warrant(order, op).await?;
    Ok((StatusCode::CREATED, Json(detainee)))
}

// POST /api/detainees/:id/court-dates
async fn schedule_court_date(
    State(state): State<AppState_>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(mut court_date): Json<CourtDate>,
) -> Result<(StatusCode, Json<CourtDate>), ApiError> {
    let session = require_api_role(&headers, state.registry.pool(), Action::UpdateBasisStatusHousing).await.map_err(auth_err)?;
    let detainee_id = parse_detainee_id(&id)?;
    let op = extract_operator_from_session(&session);
    court_date.detainee_id = detainee_id;
    let result = state.registry.schedule_court_date(court_date, op).await?;
    Ok((StatusCode::CREATED, Json(result)))
}

// PUT /api/court-dates/:id/outcome
async fn record_outcome(
    State(state): State<AppState_>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(outcome): Json<CourtOutcome>,
) -> Result<Json<CourtDate>, ApiError> {
    let session = require_api_role(&headers, state.registry.pool(), Action::UpdateBasisStatusHousing).await.map_err(auth_err)?;
    let court_date_id = parse_court_date_id(&id)?;
    let op = extract_operator_from_session(&session);
    let result = state
        .registry
        .record_court_outcome(court_date_id, outcome, op)
        .await?;
    Ok(Json(result))
}

// POST /api/detainees/:id/release
async fn release_detainee(
    State(state): State<AppState_>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(mut record): Json<ReleaseRecord>,
) -> Result<Json<Detainee>, ApiError> {
    require_api_role(&headers, state.registry.pool(), Action::ReleaseDetainee).await.map_err(auth_err)?;
    let detainee_id = parse_detainee_id(&id)?;
    record.detainee_id = detainee_id;
    let detainee = state.registry.release(record).await?;
    Ok(Json(detainee))
}

// POST /api/daily-counts/:date
async fn open_daily_count(
    State(state): State<AppState_>,
    headers: HeaderMap,
    Path(date): Path<String>,
) -> Result<(StatusCode, Json<DailyCount>), ApiError> {
    let session = require_api_role(&headers, state.registry.pool(), Action::UpdateBasisStatusHousing).await.map_err(auth_err)?;
    let date = parse_date(&date)?;
    let op = extract_operator_from_session(&session);
    let count = state.registry.open_daily_count(date, op).await?;
    Ok((StatusCode::CREATED, Json(count)))
}

// POST /api/daily-counts/:date/headcounts
async fn submit_headcount(
    State(state): State<AppState_>,
    headers: HeaderMap,
    Path(date): Path<String>,
    Json(headcount): Json<UnitHeadcount>,
) -> Result<Json<DailyCount>, ApiError> {
    require_api_role(&headers, state.registry.pool(), Action::UpdateBasisStatusHousing).await.map_err(auth_err)?;
    let date = parse_date(&date)?;
    let count = state.registry.submit_unit_headcount(date, headcount).await?;
    Ok(Json(count))
}

// POST /api/daily-counts/:date/finalize
async fn finalize_count(
    State(state): State<AppState_>,
    headers: HeaderMap,
    Path(date): Path<String>,
) -> Result<Json<DailyCount>, ApiError> {
    let session = require_api_role(&headers, state.registry.pool(), Action::UpdateBasisStatusHousing).await.map_err(auth_err)?;
    let date = parse_date(&date)?;
    let op = extract_operator_from_session(&session);
    let count = state.registry.finalize_daily_count(date, op).await?;
    spawn_auto_backup(&state);
    Ok(Json(count))
}

// === Backup & Export handlers ===

// GET /api/backup — download a backup of the database
async fn download_backup(
    State(state): State<AppState_>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    require_api_role(&headers, state.registry.pool(), Action::ManageBackups).await.map_err(auth_err)?;

    let backup_path = crate::backup::create_backup(state.registry.pool(), &state.backup_dir)
        .await
        .map_err(|e| ApiError(RegistryError::Database(e)))?;

    let bytes = tokio::fs::read(&backup_path)
        .await
        .map_err(|e| ApiError(RegistryError::Database(format!("failed to read backup: {}", e))))?;

    let filename = backup_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("openward-backup.db")
        .to_string();

    let mut resp_headers = HeaderMap::new();
    resp_headers.insert(
        axum::http::header::CONTENT_TYPE,
        "application/x-sqlite3".parse().unwrap(),
    );
    resp_headers.insert(
        axum::http::header::CONTENT_DISPOSITION,
        format!("attachment; filename=\"{}\"", filename).parse().unwrap(),
    );

    Ok((StatusCode::OK, resp_headers, bytes).into_response())
}

#[derive(Deserialize)]
struct DateRangeQuery {
    from: String,
    to: String,
}

// GET /api/export/population.csv
async fn export_population(
    State(state): State<AppState_>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    require_api_role(&headers, state.registry.pool(), Action::ExportData).await.map_err(auth_err)?;

    let detainees = state.registry.all_active_detainees_for_export().await?;
    let csv_bytes = crate::export::population_csv(&detainees);

    Ok((
        StatusCode::OK,
        [
            (axum::http::header::CONTENT_TYPE, "text/csv; charset=utf-8"),
            (
                axum::http::header::CONTENT_DISPOSITION,
                "attachment; filename=\"population.csv\"",
            ),
        ],
        csv_bytes,
    )
        .into_response())
}

// GET /api/export/daily-counts.csv?from=DATE&to=DATE
async fn export_daily_counts(
    State(state): State<AppState_>,
    headers: HeaderMap,
    Query(params): Query<DateRangeQuery>,
) -> Result<Response, ApiError> {
    require_api_role(&headers, state.registry.pool(), Action::ExportData).await.map_err(auth_err)?;

    let from = parse_date(&params.from)?;
    let to = parse_date(&params.to)?;
    let counts = state.registry.daily_counts_in_range(from, to).await?;
    let csv_bytes = crate::export::daily_counts_csv(&counts);

    Ok((
        StatusCode::OK,
        [
            (axum::http::header::CONTENT_TYPE, "text/csv; charset=utf-8"),
            (
                axum::http::header::CONTENT_DISPOSITION,
                "attachment; filename=\"daily-counts.csv\"",
            ),
        ],
        csv_bytes,
    )
        .into_response())
}

// GET /api/export/audit.csv?from=DATE&to=DATE
async fn export_audit(
    State(state): State<AppState_>,
    headers: HeaderMap,
    Query(params): Query<DateRangeQuery>,
) -> Result<Response, ApiError> {
    require_api_role(&headers, state.registry.pool(), Action::ExportData).await.map_err(auth_err)?;

    let from = parse_date(&params.from)?;
    let to = parse_date(&params.to)?;
    let entries = state.registry.audit_entries_in_range(from, to).await?;
    let csv_bytes = crate::export::audit_csv(&entries);

    Ok((
        StatusCode::OK,
        [
            (axum::http::header::CONTENT_TYPE, "text/csv; charset=utf-8"),
            (
                axum::http::header::CONTENT_DISPOSITION,
                "attachment; filename=\"audit.csv\"",
            ),
        ],
        csv_bytes,
    )
        .into_response())
}

/// Spawn a background auto-backup after daily count finalization.
pub fn spawn_auto_backup(state: &AppState_) {
    let pool = state.registry.pool().clone();
    let backup_dir = state.backup_dir.clone();
    let retention = state.backup_retention;

    tokio::spawn(async move {
        match crate::backup::create_backup(&pool, &backup_dir).await {
            Ok(path) => {
                tracing::info!("auto-backup created: {}", path.display());
                if let Err(e) = crate::backup::prune_backups(&backup_dir, retention) {
                    tracing::warn!("backup pruning failed: {}", e);
                }
            }
            Err(e) => {
                tracing::error!("auto-backup failed: {}", e);
            }
        }
    });
}

// === Router ===

pub fn api_router() -> Router<AppState_> {
    Router::new()
        // Health (no auth)
        .route("/health", get(health_check))
        // Overview
        .route("/api/overview", get(get_overview))
        // Detainees
        .route("/api/detainees", get(search_detainees).post(admit_detainee))
        .route("/api/detainees/batch", post(batch_admit))
        .route("/api/detainees/{id}", get(get_detainee))
        .route("/api/detainees/{id}/basis", put(update_basis))
        .route("/api/detainees/{id}/status", put(update_status))
        .route("/api/detainees/{id}/housing", put(assign_housing))
        .route("/api/detainees/{id}/warrants", post(register_warrant))
        .route("/api/detainees/{id}/court-dates", post(schedule_court_date))
        .route("/api/detainees/{id}/release", post(release_detainee))
        // Court dates
        .route("/api/court-dates/{id}/outcome", put(record_outcome))
        // Daily counts
        .route("/api/daily-counts/{date}", post(open_daily_count))
        .route(
            "/api/daily-counts/{date}/headcounts",
            post(submit_headcount),
        )
        .route(
            "/api/daily-counts/{date}/finalize",
            post(finalize_count),
        )
        // Backup & Export
        .route("/api/backup", get(download_backup))
        .route("/api/export/population.csv", get(export_population))
        .route("/api/export/daily-counts.csv", get(export_daily_counts))
        .route("/api/export/audit.csv", get(export_audit))
}
