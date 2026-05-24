pub mod auth;
pub mod backup;
pub mod dashboard;
pub mod population;
pub mod detainee;
pub mod admission;
pub mod daily_count;
pub mod court_calendar;
pub mod housing;
pub mod transfer;
pub mod errors;
pub mod util;
pub mod profile;
pub mod operators;
pub mod audit;

use std::sync::Arc;
use axum::{
    http::HeaderMap,
    response::Redirect,
    routing::{get, post},
    Router,
};
use openward_facility::AppState;
use sqlx::SqlitePool;
use crate::auth::{Action, can_do};
use crate::web::errors::HtmlError;

pub type AppState_ = Arc<AppState>;

/// Require a logged-in operator session. Redirect to /login if missing.
/// If the operator must change their password, redirect to /profile.
pub async fn require_session(headers: &HeaderMap, pool: &SqlitePool) -> Result<auth::Session, Redirect> {
    let session = auth::extract_session(headers, pool).await.ok_or(Redirect::to("/login"))?;
    if session.must_change_password {
        return Err(Redirect::to("/profile"));
    }
    Ok(session)
}

/// Like `require_session` but allows operators who still need to change their
/// password.  Used by profile and logout routes so they remain accessible.
pub async fn require_session_allow_password_change(headers: &HeaderMap, pool: &SqlitePool) -> Result<auth::Session, Redirect> {
    auth::extract_session(headers, pool).await.ok_or(Redirect::to("/login"))
}

/// Require a logged-in operator with permission for the given action.
/// Returns 403 Forbidden if the session role lacks permission.
pub async fn require_role(headers: &HeaderMap, pool: &SqlitePool, action: Action) -> Result<auth::Session, HtmlError> {
    let session = require_session(headers, pool).await
        .map_err(|_| HtmlError::redirect("/login"))?;
    if !can_do(&session.role, action) {
        return Err(HtmlError::Forbidden);
    }
    Ok(session)
}

/// Build nav context for templates.
pub fn nav_context(session: &auth::Session, path: &str) -> crate::templates::NavContext {
    crate::templates::NavContext {
        operator_name: session.display_name.clone(),
        role: session.role.clone(),
        current_path: path.to_string(),
    }
}

pub fn html_router() -> Router<AppState_> {
    Router::new()
        // Auth
        .route("/login", get(auth::login_page).post(auth::login_submit))
        .route("/logout", post(auth::logout))
        // Profile
        .route("/profile", get(profile::profile_page))
        .route("/profile/password", post(profile::change_password))
        .route("/profile/language", post(profile::change_language))
        // Admin session management
        .route("/admin/sessions/{id}/revoke", post(profile::revoke_session))
        // Dashboard
        .route("/", get(dashboard::dashboard))
        // Population
        .route("/detainees", get(population::population_page))
        .route("/detainees/search", get(population::search_fragment))
        // Detainee detail
        .route("/detainees/{id}/view", get(detainee::detail_page))
        .route("/detainees/{id}/basis-form", get(detainee::basis_form))
        .route("/detainees/{id}/update-basis", post(detainee::update_basis))
        .route("/detainees/{id}/housing-form", get(detainee::housing_form))
        .route("/detainees/{id}/update-housing", post(detainee::update_housing))
        .route("/detainees/{id}/court-date-form", get(detainee::court_date_form))
        .route("/detainees/{id}/schedule-court-date", post(detainee::schedule_court_date))
        .route("/detainees/{id}/transfer-form", get(detainee::transfer_form))
        .route("/detainees/{id}/do-transfer", post(detainee::do_transfer))
        .route("/detainees/{id}/release-form", get(detainee::release_form))
        .route("/detainees/{id}/do-release", post(detainee::do_release))
        .route("/detainees/{id}/edit-court-date/{cdid}", get(detainee::edit_court_date_form))
        .route("/detainees/{id}/update-court-date/{cdid}", post(detainee::update_court_date))
        .route("/detainees/{id}/note-form", get(detainee::note_form))
        .route("/detainees/{id}/add-note", post(detainee::add_note))
        .route("/detainees/{id}/delete-note/{nid}", post(detainee::delete_note))
        .route("/detainees/{id}/property-form", get(detainee::property_form))
        .route("/detainees/{id}/add-property", post(detainee::add_property))
        .route("/detainees/{id}/return-property/{pid}", post(detainee::return_property))
        // Admission
        .route("/admit", get(admission::admission_page).post(admission::submit_admission))
        .route("/admit/batch", get(admission::batch_admission_page).post(admission::submit_batch_admission))
        .route("/admit/basis-fields", get(admission::basis_fields_fragment))
        // Transfers
        .route("/transfers", get(transfer::transfers_page))
        // Daily count
        .route("/daily-count", get(daily_count::daily_count_page))
        .route("/daily-count/{date}", get(daily_count::daily_count_for_date))
        .route("/daily-count/{date}/open", post(daily_count::open_count))
        .route("/daily-count/{date}/headcount", post(daily_count::submit_headcount))
        .route("/daily-count/{date}/finalize", post(daily_count::finalize))
        // Housing management
        .route("/housing", get(housing::housing_page).post(housing::create_unit))
        .route("/housing/create-form", get(housing::create_form))
        .route("/housing/{id}/edit-form", get(housing::edit_form))
        .route("/housing/{id}", axum::routing::delete(housing::delete_unit).put(housing::update_unit))
        // Court calendar
        .route("/court-calendar", get(court_calendar::calendar_page))
        .route("/court-calendar/{id}/outcome-form", get(court_calendar::outcome_form))
        .route("/court-calendar/{id}/outcome", post(court_calendar::record_outcome))
        // Audit trail (admin/supervisor)
        .route("/audit", get(audit::audit_page))
        .route("/audit/search", get(audit::search_fragment))
        .route("/audit/verify", get(audit::verify_page))
        .route("/audit/{entry_id}", get(audit::detail_page))
        // Backup & export (admin only)
        .route("/backup", get(backup::backup_page))
        // Operator management (admin only)
        .route("/operators", get(operators::operators_page).post(operators::create_operator))
        .route("/operators/new", get(operators::new_form))
        .route("/operators/{id}/edit", get(operators::edit_form))
        .route("/operators/{id}/update", post(operators::update_operator))
        .route("/operators/{id}/reset-password", get(operators::reset_password_form).post(operators::reset_password))
}
