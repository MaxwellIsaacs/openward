use axum::{
    extract::State,
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Form,
};
use serde::Deserialize;
use sqlx::SqlitePool;
use openward_core::OperatorId;
use crate::i18n::{T, DEFAULT_LANGUAGE, SUPPORTED_LANGUAGES};
use crate::templates::LoginTemplate;
use super::AppState_;

const COOKIE_NAME: &str = "openward_session";
const LANG_COOKIE_NAME: &str = "openward_lang";

/// Session data extracted from the database.
pub struct Session {
    pub session_id: String,
    pub operator_id: OperatorId,
    pub display_name: String,
    pub role: String,
    pub lang: String,
    pub must_change_password: bool,
}

/// Extract a cookie value by name from headers.
fn get_cookie<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    let cookie_header = headers.get(header::COOKIE)?.to_str().ok()?;
    for cookie in cookie_header.split(';') {
        let cookie = cookie.trim();
        if let Some(value) = cookie.strip_prefix(&format!("{}=", name)) {
            let value = value.trim();
            if !value.is_empty() {
                return Some(value);
            }
        }
    }
    None
}

/// Extract session from cookie + database. Returns None if no valid session.
pub async fn extract_session(headers: &HeaderMap, pool: &SqlitePool) -> Option<Session> {
    let token = get_cookie(headers, COOKIE_NAME)?;
    if token.is_empty() {
        return None;
    }

    let now = chrono::Utc::now().to_rfc3339();
    let row: Option<(String, String, String, String, String)> = sqlx::query_as(
        "SELECT id, operator_id, display_name, role, language \
         FROM sessions WHERE id = ? AND expires_at > ?"
    )
    .bind(token)
    .bind(&now)
    .fetch_optional(pool)
    .await
    .ok()?;

    let (session_id, operator_id_str, display_name, role, language) = row?;
    let uuid = uuid::Uuid::parse_str(&operator_id_str).ok()?;

    // Check must_change_password flag on the operator
    let mcp_row: Option<(i32,)> = sqlx::query_as(
        "SELECT must_change_password FROM operators WHERE id = ?"
    )
    .bind(&operator_id_str)
    .fetch_optional(pool)
    .await
    .ok()?;
    let must_change_password = mcp_row.map(|(v,)| v != 0).unwrap_or(false);

    // Language: cookie override > session stored value
    let lang = get_cookie(headers, LANG_COOKIE_NAME)
        .filter(|l| SUPPORTED_LANGUAGES.iter().any(|(code, _)| code == l))
        .map(|l| l.to_string())
        .unwrap_or(language);

    Some(Session {
        session_id,
        operator_id: OperatorId::from_uuid(uuid),
        display_name,
        role,
        lang,
        must_change_password,
    })
}

/// Pre-session language detection for login page.
/// Checks: openward_lang cookie > Accept-Language header > DEFAULT_LANGUAGE.
pub fn extract_lang(headers: &HeaderMap) -> String {
    // 1. Check openward_lang cookie
    if let Some(lang) = get_cookie(headers, LANG_COOKIE_NAME) {
        if SUPPORTED_LANGUAGES.iter().any(|(code, _)| code == &lang) {
            return lang.to_string();
        }
    }

    // 2. Check Accept-Language header (simple first-match)
    if let Some(accept) = headers.get(header::ACCEPT_LANGUAGE) {
        if let Ok(accept_str) = accept.to_str() {
            for part in accept_str.split(',') {
                let tag = part.split(';').next().unwrap_or("").trim();
                // Check exact match first, then prefix match
                let code = if tag.len() > 2 { &tag[..2] } else { tag };
                if SUPPORTED_LANGUAGES.iter().any(|(c, _)| *c == code) {
                    return code.to_string();
                }
            }
        }
    }

    // 3. Fallback
    DEFAULT_LANGUAGE.to_string()
}

/// GET /login
pub async fn login_page(
    State(state): State<AppState_>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let lang = extract_lang(&headers);
    let t = T::new(&lang, state.legal_overrides.clone());
    LoginTemplate {
        error: None,
        languages: SUPPORTED_LANGUAGES,
        t,
    }
}

#[derive(Deserialize)]
pub struct LoginForm {
    pub username: String,
    pub password: String,
    pub language: String,
}

/// POST /login
pub async fn login_submit(
    State(state): State<AppState_>,
    Form(form): Form<LoginForm>,
) -> Response {
    let pool = state.registry.pool();

    // Validate language, fallback to default
    let lang = if SUPPORTED_LANGUAGES.iter().any(|(code, _)| *code == form.language.as_str()) {
        form.language.clone()
    } else {
        DEFAULT_LANGUAGE.to_string()
    };

    let t = T::new(&lang, state.legal_overrides.clone());

    // Look up operator by username
    let row: Option<(String, String, String, String, String, i32)> = sqlx::query_as(
        "SELECT id, display_name, password_hash, role, language, must_change_password \
         FROM operators WHERE username = ? AND is_active = 1"
    )
    .bind(&form.username)
    .fetch_optional(pool)
    .await
    .unwrap_or(None);

    let Some((operator_id, display_name, password_hash, role, _op_lang, must_change_pw)) = row else {
        return LoginTemplate {
            error: Some(t.get("login-invalid-credentials")),
            languages: SUPPORTED_LANGUAGES,
            t,
        }.into_response();
    };

    // Verify password
    let pw_ok = crate::auth::verify_password(&form.password, &password_hash)
        .unwrap_or(false);

    if !pw_ok {
        return LoginTemplate {
            error: Some(t.get("login-invalid-credentials")),
            languages: SUPPORTED_LANGUAGES,
            t,
        }.into_response();
    }

    // Create session
    let session_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now();
    let expires = now + chrono::Duration::hours(24);

    let _ = sqlx::query(
        "INSERT INTO sessions (id, operator_id, display_name, role, language, created_at, expires_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?)"
    )
    .bind(&session_id)
    .bind(&operator_id)
    .bind(&display_name)
    .bind(&role)
    .bind(&lang)
    .bind(now.to_rfc3339())
    .bind(expires.to_rfc3339())
    .execute(pool)
    .await;

    // Update last_login
    let _ = sqlx::query("UPDATE operators SET last_login = ? WHERE id = ?")
        .bind(now.to_rfc3339())
        .bind(&operator_id)
        .execute(pool)
        .await;

    let session_cookie = format!(
        "{}={}; Path=/; HttpOnly; SameSite=Strict; Max-Age=86400",
        COOKIE_NAME, session_id
    );
    let lang_cookie = format!(
        "{}={}; Path=/; SameSite=Strict; Max-Age=31536000",
        LANG_COOKIE_NAME, lang
    );

    let redirect_to = if must_change_pw != 0 { "/profile" } else { "/" };

    let mut response = (
        StatusCode::SEE_OTHER,
        [(header::LOCATION, redirect_to.to_string())],
    )
        .into_response();
    response.headers_mut().append(
        header::SET_COOKIE,
        session_cookie.parse().unwrap(),
    );
    response.headers_mut().append(
        header::SET_COOKIE,
        lang_cookie.parse().unwrap(),
    );
    response
}

/// POST /logout
pub async fn logout(
    State(state): State<AppState_>,
    headers: HeaderMap,
) -> Response {
    // Delete session from DB if it exists
    if let Some(token) = get_cookie(&headers, COOKIE_NAME) {
        let _ = sqlx::query("DELETE FROM sessions WHERE id = ?")
            .bind(token)
            .execute(state.registry.pool())
            .await;
    }

    let cookie = format!(
        "{}=; Path=/; HttpOnly; SameSite=Strict; Max-Age=0",
        COOKIE_NAME
    );
    (
        StatusCode::SEE_OTHER,
        [
            (header::SET_COOKIE, cookie),
            (header::LOCATION, "/login".to_string()),
        ],
    )
        .into_response()
}
