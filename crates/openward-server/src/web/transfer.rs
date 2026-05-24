use axum::{
    extract::State,
    http::HeaderMap,
    response::IntoResponse,
};
use super::{require_session, nav_context, AppState_};
use crate::i18n::T;
use crate::templates::{TransfersPageTemplate, TransferDisplay};
use crate::web::errors::HtmlError;
use crate::web::util;

pub async fn transfers_page(
    State(state): State<AppState_>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, HtmlError> {
    let session = require_session(&headers, state.registry.pool()).await.map_err(|_| HtmlError::redirect("/login"))?;
    let t = T::new(&session.lang, state.legal_overrides.clone());
    let raw = state.registry.recent_transfers(50).await?;

    let transfers: Vec<TransferDisplay> = raw
        .into_iter()
        .map(|(record, name)| TransferDisplay {
            date: util::format_date(&record.transfer_date.as_naive()),
            detainee_name: name,
            detainee_id: record.detainee_id.to_string(),
            destination: record.to_facility.unwrap_or_default(),
            reason: record.reason,
            authorized_by: record.authorized_by.to_string(),
        })
        .collect();

    Ok(TransfersPageTemplate {
        nav: nav_context(&session, "/transfers"),
        transfers,
        t,
    })
}
