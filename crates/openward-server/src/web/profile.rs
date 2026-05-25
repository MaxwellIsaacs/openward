use axum::{
    extract::{Path, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Redirect, Response},
    Form,
};
use serde::Deserialize;
use super::{require_session_allow_password_change, require_role, nav_context, AppState_};
use crate::auth::Action;
use crate::i18n::{T, SUPPORTED_LANGUAGES};
use crate::templates::{ProfileTemplate, SessionDisplay};
use crate::web::errors::HtmlError;

async fn build_profile(
    state: &AppState_,
    headers: &HeaderMap,
    flash: Option<String>,
    flash_error: Option<String>,
) -> Result<Response, Redirect> {
    let session = require_session_allow_password_change(headers, state.registry.pool()).await?;
    let t = T::new(&session.lang, state.legal_overrides.clone());
    let is_admin = session.role == "admin";

    // Fetch active sessions for this operator (admin sees all, others see own)
    let sessions = if is_admin {
        let rows: Vec<(String, String, String, String, String, String)> = sqlx::query_as(
            "SELECT s.id, s.display_name, s.role, s.operator_id, s.created_at, s.expires_at \
             FROM sessions s WHERE s.expires_at > ? ORDER BY s.created_at DESC"
        )
        .bind(chrono::Utc::now().to_rfc3339())
        .fetch_all(state.registry.pool())
        .await
        .unwrap_or_default();

        rows.into_iter().map(|(id, name, role, _op_id, created, expires)| {
            let is_current = id == session.session_id;
            SessionDisplay { id, display_name: name, role, created_at: created, expires_at: expires, is_current }
        }).collect()
    } else {
        let rows: Vec<(String, String, String, String, String)> = sqlx::query_as(
            "SELECT id, display_name, role, created_at, expires_at \
             FROM sessions WHERE operator_id = ? AND expires_at > ? ORDER BY created_at DESC"
        )
        .bind(session.operator_id.to_string())
        .bind(chrono::Utc::now().to_rfc3339())
        .fetch_all(state.registry.pool())
        .await
        .unwrap_or_default();

        rows.into_iter().map(|(id, name, role, created, expires)| {
            let is_current = id == session.session_id;
            SessionDisplay { id, display_name: name, role, created_at: created, expires_at: expires, is_current }
        }).collect()
    };

    let must_change_password = session.must_change_password;

    Ok(ProfileTemplate {
        nav: nav_context(&session, "/profile"),
        display_name: session.display_name.clone(),
        role: session.role.clone(),
        language: session.lang.clone(),
        languages: SUPPORTED_LANGUAGES,
        is_admin,
        sessions,
        flash,
        flash_error,
        must_change_password,
        t,
    }.into_response())
}

/// GET /profile
pub async fn profile_page(
    State(state): State<AppState_>,
    headers: HeaderMap,
) -> Result<Response, Redirect> {
    build_profile(&state, &headers, None, None).await
}

#[derive(Deserialize)]
pub struct PasswordForm {
    pub current_password: String,
    pub new_password: String,
    pub confirm_password: String,
}

/// POST /profile/password
pub async fn change_password(
    State(state): State<AppState_>,
    headers: HeaderMap,
    Form(form): Form<PasswordForm>,
) -> Result<Response, Redirect> {
    let session = require_session_allow_password_change(&headers, state.registry.pool()).await?;
    let t = T::new(&session.lang, state.legal_overrides.clone());

    // Validate new password == confirm
    if form.new_password != form.confirm_password {
        return build_profile(&state, &headers, None, Some(t.get("profile-password-mismatch"))).await;
    }

    // Get current password hash
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT password_hash FROM operators WHERE id = ?"
    )
    .bind(session.operator_id.to_string())
    .fetch_optional(state.registry.pool())
    .await
    .unwrap_or(None);

    let Some((current_hash,)) = row else {
        return build_profile(&state, &headers, None, Some(t.get("profile-password-wrong"))).await;
    };

    // Verify current password
    let ok = crate::auth::verify_password(&form.current_password, &current_hash)
        .unwrap_or(false);
    if !ok {
        return build_profile(&state, &headers, None, Some(t.get("profile-password-wrong"))).await;
    }

    // Hash new password
    let new_hash = crate::auth::hash_password(&form.new_password)
        .expect("failed to hash password");

    let _ = sqlx::query("UPDATE operators SET password_hash = ?, must_change_password = 0 WHERE id = ?")
        .bind(&new_hash)
        .bind(session.operator_id.to_string())
        .execute(state.registry.pool())
        .await;

    // Invalidate all other sessions for this operator
    let _ = sqlx::query("DELETE FROM sessions WHERE operator_id = ? AND id != ?")
        .bind(session.operator_id.to_string())
        .bind(&session.session_id)
        .execute(state.registry.pool())
        .await;

    build_profile(&state, &headers, Some(t.get("profile-password-changed")), None).await
}

#[derive(Deserialize)]
pub struct LanguageForm {
    pub language: String,
}

/// POST /profile/language
pub async fn change_language(
    State(state): State<AppState_>,
    headers: HeaderMap,
    Form(form): Form<LanguageForm>,
) -> Result<Response, Redirect> {
    let session = require_session_allow_password_change(&headers, state.registry.pool()).await?;

    let lang = if SUPPORTED_LANGUAGES.iter().any(|(code, _)| *code == form.language.as_str()) {
        form.language.clone()
    } else {
        session.lang.clone()
    };

    // Update operator's language preference
    let _ = sqlx::query("UPDATE operators SET language = ? WHERE id = ?")
        .bind(&lang)
        .bind(session.operator_id.to_string())
        .execute(state.registry.pool())
        .await;

    // Update session's stored language
    let _ = sqlx::query("UPDATE sessions SET language = ? WHERE id = ?")
        .bind(&lang)
        .bind(&session.session_id)
        .execute(state.registry.pool())
        .await;

    // Set language cookie and redirect back to profile
    let lang_cookie = format!(
        "openward_lang={}; Path=/; SameSite=Strict; Max-Age=31536000{}",
        lang,
        crate::web::auth::secure_attr(&headers)
    );

    let mut response = (
        StatusCode::SEE_OTHER,
        [(header::LOCATION, "/profile".to_string())],
    ).into_response();
    response.headers_mut().append(
        header::SET_COOKIE,
        lang_cookie.parse().unwrap(),
    );
    Ok(response)
}

/// POST /admin/sessions/{id}/revoke
pub async fn revoke_session(
    State(state): State<AppState_>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Response, HtmlError> {
    let session = require_role(&headers, state.registry.pool(), Action::ManageOperators).await?;

    // Don't allow revoking your own current session
    if id == session.session_id {
        return Err(HtmlError::redirect("/profile"));
    }

    let _ = sqlx::query("DELETE FROM sessions WHERE id = ?")
        .bind(&id)
        .execute(state.registry.pool())
        .await;

    Ok(Redirect::to("/profile").into_response())
}
