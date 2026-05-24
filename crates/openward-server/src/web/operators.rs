use axum::{
    extract::{Form, Path, State},
    http::HeaderMap,
    response::{IntoResponse, Redirect, Response},
};
use serde::Deserialize;
use super::{require_role, nav_context, AppState_};
use crate::auth::Action;
use crate::i18n::{T, SUPPORTED_LANGUAGES};
use crate::templates::*;
use crate::web::errors::HtmlError;

fn role_label(t: &T, role: &str) -> String {
    match role {
        "admin" => t.get("role-admin"),
        "supervisor" => t.get("role-supervisor"),
        "operator" => t.get("role-operator"),
        "readonly" => t.get("role-readonly"),
        _ => role.to_string(),
    }
}

/// GET /operators
pub async fn operators_page(
    State(state): State<AppState_>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, HtmlError> {
    let session = require_role(&headers, state.registry.pool(), Action::ManageOperators).await?;
    let t = T::new(&session.lang, state.legal_overrides.clone());

    let rows: Vec<(String, String, String, String, bool, Option<String>)> = sqlx::query_as(
        "SELECT id, username, display_name, role, is_active, last_login \
         FROM operators ORDER BY username"
    )
    .fetch_all(state.registry.pool())
    .await
    .map_err(|e| HtmlError::Registry(openward_core::RegistryError::Database(e.to_string())))?;

    let operators: Vec<OperatorDisplay> = rows.into_iter().map(|(id, username, display_name, role, is_active, last_login)| {
        let role_label = role_label(&t, &role);
        OperatorDisplay { id, username, display_name, role, role_label, is_active, last_login }
    }).collect();

    Ok(OperatorsPageTemplate {
        nav: nav_context(&session, "/operators"),
        operators,
        flash: None,
        t,
    })
}

/// GET /operators/new
pub async fn new_form(
    State(state): State<AppState_>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, HtmlError> {
    let session = require_role(&headers, state.registry.pool(), Action::ManageOperators).await?;
    let t = T::new(&session.lang, state.legal_overrides.clone());

    Ok(OperatorNewTemplate {
        nav: nav_context(&session, "/operators/new"),
        languages: SUPPORTED_LANGUAGES,
        error: None,
        t,
    })
}

#[derive(Deserialize)]
pub struct CreateOperatorForm {
    pub username: String,
    pub display_name: String,
    pub password: String,
    pub confirm_password: String,
    pub role: String,
    pub language: String,
}

/// POST /operators
pub async fn create_operator(
    State(state): State<AppState_>,
    headers: HeaderMap,
    Form(form): Form<CreateOperatorForm>,
) -> Result<Response, HtmlError> {
    let session = require_role(&headers, state.registry.pool(), Action::ManageOperators).await?;
    let t = T::new(&session.lang, state.legal_overrides.clone());

    let username = form.username.trim().to_string();
    if username.is_empty() {
        return Ok(OperatorNewTemplate {
            nav: nav_context(&session, "/operators/new"),
            languages: SUPPORTED_LANGUAGES,
            error: Some(t.get("error-name-required")),
            t,
        }.into_response());
    }

    if form.password != form.confirm_password {
        return Ok(OperatorNewTemplate {
            nav: nav_context(&session, "/operators/new"),
            languages: SUPPORTED_LANGUAGES,
            error: Some(t.get("profile-password-mismatch")),
            t,
        }.into_response());
    }

    if form.password.is_empty() {
        return Ok(OperatorNewTemplate {
            nav: nav_context(&session, "/operators/new"),
            languages: SUPPORTED_LANGUAGES,
            error: Some(t.get("error-password-required")),
            t,
        }.into_response());
    }

    // Check for duplicate username
    let existing: Option<(String,)> = sqlx::query_as(
        "SELECT id FROM operators WHERE username = ?"
    )
    .bind(&username)
    .fetch_optional(state.registry.pool())
    .await
    .map_err(|e| HtmlError::Registry(openward_core::RegistryError::Database(e.to_string())))?;

    if existing.is_some() {
        return Ok(OperatorNewTemplate {
            nav: nav_context(&session, "/operators/new"),
            languages: SUPPORTED_LANGUAGES,
            error: Some(t.get("error-username-taken")),
            t,
        }.into_response());
    }

    let role = validate_role(&form.role);
    let language = validate_language(&form.language);
    let password_hash = crate::auth::hash_password(&form.password)
        .expect("failed to hash password");
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    sqlx::query(
        "INSERT INTO operators (id, username, display_name, password_hash, role, language, is_active, created_at) \
         VALUES (?, ?, ?, ?, ?, ?, 1, ?)"
    )
    .bind(&id)
    .bind(&username)
    .bind(form.display_name.trim())
    .bind(&password_hash)
    .bind(&role)
    .bind(&language)
    .bind(&now)
    .execute(state.registry.pool())
    .await
    .map_err(|e| HtmlError::Registry(openward_core::RegistryError::Database(e.to_string())))?;

    Ok(Redirect::to("/operators").into_response())
}

/// GET /operators/{id}/edit
pub async fn edit_form(
    State(state): State<AppState_>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, HtmlError> {
    let session = require_role(&headers, state.registry.pool(), Action::ManageOperators).await?;
    let t = T::new(&session.lang, state.legal_overrides.clone());

    let row: Option<(String, String, String, String, bool, Option<String>, String)> = sqlx::query_as(
        "SELECT id, username, display_name, role, is_active, last_login, language \
         FROM operators WHERE id = ?"
    )
    .bind(&id)
    .fetch_optional(state.registry.pool())
    .await
    .map_err(|e| HtmlError::Registry(openward_core::RegistryError::Database(e.to_string())))?;

    let Some((op_id, username, display_name, role, is_active, last_login, language)) = row else {
        return Err(HtmlError::validation("error-not-found"));
    };

    let rl = role_label(&t, &role);
    let operator = OperatorDisplay { id: op_id, username, display_name, role, role_label: rl, is_active, last_login };

    Ok(OperatorEditTemplate {
        nav: nav_context(&session, &format!("/operators/{}/edit", id)),
        operator,
        languages: SUPPORTED_LANGUAGES,
        current_language: language,
        error: None,
        t,
    })
}

#[derive(Deserialize)]
pub struct UpdateOperatorForm {
    pub display_name: String,
    pub role: String,
    pub language: String,
    pub is_active: Option<String>,
}

/// PUT /operators/{id}
pub async fn update_operator(
    State(state): State<AppState_>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Form(form): Form<UpdateOperatorForm>,
) -> Result<Response, HtmlError> {
    let _session = require_role(&headers, state.registry.pool(), Action::ManageOperators).await?;

    let role = validate_role(&form.role);
    let language = validate_language(&form.language);
    let is_active = form.is_active.as_deref() == Some("on") || form.is_active.as_deref() == Some("true");

    sqlx::query(
        "UPDATE operators SET display_name = ?, role = ?, language = ?, is_active = ? WHERE id = ?"
    )
    .bind(form.display_name.trim())
    .bind(&role)
    .bind(&language)
    .bind(is_active)
    .bind(&id)
    .execute(state.registry.pool())
    .await
    .map_err(|e| HtmlError::Registry(openward_core::RegistryError::Database(e.to_string())))?;

    Ok(Redirect::to("/operators").into_response())
}

/// GET /operators/{id}/reset-password
pub async fn reset_password_form(
    State(state): State<AppState_>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, HtmlError> {
    let session = require_role(&headers, state.registry.pool(), Action::ManageOperators).await?;
    let t = T::new(&session.lang, state.legal_overrides.clone());

    let row: Option<(String, String)> = sqlx::query_as(
        "SELECT id, display_name FROM operators WHERE id = ?"
    )
    .bind(&id)
    .fetch_optional(state.registry.pool())
    .await
    .map_err(|e| HtmlError::Registry(openward_core::RegistryError::Database(e.to_string())))?;

    let Some((op_id, display_name)) = row else {
        return Err(HtmlError::validation("error-not-found"));
    };

    Ok(OperatorResetPasswordTemplate {
        nav: nav_context(&session, &format!("/operators/{}/reset-password", id)),
        operator_id: op_id,
        operator_name: display_name,
        error: None,
        t,
    })
}

#[derive(Deserialize)]
pub struct ResetPasswordForm {
    pub new_password: String,
    pub confirm_password: String,
}

/// POST /operators/{id}/reset-password
pub async fn reset_password(
    State(state): State<AppState_>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Form(form): Form<ResetPasswordForm>,
) -> Result<Response, HtmlError> {
    let session = require_role(&headers, state.registry.pool(), Action::ManageOperators).await?;
    let t = T::new(&session.lang, state.legal_overrides.clone());

    if form.new_password != form.confirm_password {
        let row: Option<(String,)> = sqlx::query_as("SELECT display_name FROM operators WHERE id = ?")
            .bind(&id)
            .fetch_optional(state.registry.pool())
            .await
            .map_err(|e| HtmlError::Registry(openward_core::RegistryError::Database(e.to_string())))?;
        let name = row.map(|r| r.0).unwrap_or_default();

        return Ok(OperatorResetPasswordTemplate {
            nav: nav_context(&session, &format!("/operators/{}/reset-password", id)),
            operator_id: id,
            operator_name: name,
            error: Some(t.get("profile-password-mismatch")),
            t,
        }.into_response());
    }

    if form.new_password.is_empty() {
        let row: Option<(String,)> = sqlx::query_as("SELECT display_name FROM operators WHERE id = ?")
            .bind(&id)
            .fetch_optional(state.registry.pool())
            .await
            .map_err(|e| HtmlError::Registry(openward_core::RegistryError::Database(e.to_string())))?;
        let name = row.map(|r| r.0).unwrap_or_default();

        return Ok(OperatorResetPasswordTemplate {
            nav: nav_context(&session, &format!("/operators/{}/reset-password", id)),
            operator_id: id,
            operator_name: name,
            error: Some(t.get("error-password-required")),
            t,
        }.into_response());
    }

    let password_hash = crate::auth::hash_password(&form.new_password)
        .expect("failed to hash password");

    sqlx::query("UPDATE operators SET password_hash = ? WHERE id = ?")
        .bind(&password_hash)
        .bind(&id)
        .execute(state.registry.pool())
        .await
        .map_err(|e| HtmlError::Registry(openward_core::RegistryError::Database(e.to_string())))?;

    // Invalidate all sessions for that operator
    sqlx::query("DELETE FROM sessions WHERE operator_id = ?")
        .bind(&id)
        .execute(state.registry.pool())
        .await
        .map_err(|e| HtmlError::Registry(openward_core::RegistryError::Database(e.to_string())))?;

    Ok(Redirect::to("/operators").into_response())
}

fn validate_role(role: &str) -> &str {
    match role {
        "admin" | "supervisor" | "operator" | "readonly" => role,
        _ => "operator",
    }
}

fn validate_language(lang: &str) -> &str {
    if SUPPORTED_LANGUAGES.iter().any(|(code, _)| *code == lang) {
        lang
    } else {
        "en"
    }
}
