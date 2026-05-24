use axum::{
    extract::State,
    http::HeaderMap,
    response::IntoResponse,
};
use crate::auth::Action;
use crate::i18n::T;
use crate::templates::*;
use crate::web::errors::HtmlError;
use super::{require_role, nav_context, AppState_};

pub async fn backup_page(
    State(state): State<AppState_>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, HtmlError> {
    let session = require_role(&headers, state.registry.pool(), Action::ManageBackups).await?;
    let t = T::new(&session.lang, state.legal_overrides.clone());

    let last_backup = crate::backup::last_backup_time(&state.backup_dir);
    let today = chrono::Utc::now().date_naive();
    let thirty_days_ago = today - chrono::Duration::days(30);

    Ok(BackupPageTemplate {
        nav: nav_context(&session, "/backup"),
        last_backup,
        default_from: thirty_days_ago.to_string(),
        default_to: today.to_string(),
        t,
    })
}
