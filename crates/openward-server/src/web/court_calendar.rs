use axum::{
    extract::{Form, Path, State},
    http::HeaderMap,
    response::{IntoResponse, Response},
};
use chrono::NaiveDate;
use serde::Deserialize;
use uuid::Uuid;
use openward_core::*;
use super::{require_session, nav_context, AppState_};
use crate::i18n::T;
use crate::templates::*;
use crate::web::errors::HtmlError;
use crate::web::util;

fn to_display(items: Vec<(CourtDate, String, String)>, t: &T) -> Vec<CourtDateDisplay> {
    items
        .into_iter()
        .map(|(cd, detainee_id, detainee_name)| CourtDateDisplay {
            id: cd.id.as_uuid().to_string(),
            detainee_id,
            detainee_name,
            scheduled_date: util::format_date(&cd.scheduled_date),
            court_name: cd.court_name,
            purpose: t.get(util::court_purpose_key(&cd.purpose)),
            has_outcome: cd.outcome.is_some(),
            outcome_summary: util::outcome_summary(t, &cd.outcome),
        })
        .collect()
}

pub async fn calendar_page(
    State(state): State<AppState_>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, HtmlError> {
    let session = require_session(&headers, state.registry.pool()).await.map_err(|_| HtmlError::redirect("/login"))?;
    let t = T::new(&session.lang, state.legal_overrides.clone());
    let upcoming_raw = state.registry.upcoming_court_dates(50).await?;
    let past_raw = state.registry.recent_court_dates(20).await?;
    Ok(CourtCalendarPageTemplate {
        nav: nav_context(&session, "/court-calendar"),
        upcoming: to_display(upcoming_raw, &t),
        past: to_display(past_raw, &t),
        t,
    })
}

pub async fn outcome_form(
    State(state): State<AppState_>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, HtmlError> {
    let session = require_session(&headers, state.registry.pool()).await.map_err(|_| HtmlError::redirect("/login"))?;
    let t = T::new(&session.lang, state.legal_overrides.clone());
    let court_date_id = Uuid::parse_str(&id)
        .map_err(|_| HtmlError::validation("error-invalid-id"))?;
    let court_date = state.registry.get_court_date(CourtDateId::from_uuid(court_date_id)).await?;

    let court_date_display = OutcomeFormDisplay {
        id: court_date.id.as_uuid().to_string(),
        scheduled_date: util::format_date(&court_date.scheduled_date),
        court_name: court_date.court_name,
        purpose: t.get(util::court_purpose_key(&court_date.purpose)),
    };

    Ok(OutcomeFormFragment {
        court_date: court_date_display,
        t,
    })
}

#[derive(Deserialize)]
pub struct OutcomeFormData {
    pub outcome_type: String,
    pub next_date: Option<String>,
    pub reason: Option<String>,
    pub amount: Option<u64>,
    pub sentence_days: Option<u32>,
}

pub async fn record_outcome(
    State(state): State<AppState_>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Form(form): Form<OutcomeFormData>,
) -> Result<Response, HtmlError> {
    let session = require_session(&headers, state.registry.pool()).await.map_err(|_| HtmlError::redirect("/login"))?;
    let court_date_id = Uuid::parse_str(&id)
        .map_err(|_| HtmlError::validation("error-invalid-id"))?;

    let outcome = match form.outcome_type.as_str() {
        "Adjourned" => CourtOutcome::Adjourned {
            next_date: form.next_date.as_deref().filter(|s| !s.is_empty())
                .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()),
            reason: form.reason.unwrap_or_default(),
        },
        "BailGranted" => CourtOutcome::BailGranted {
            amount: form.amount,
            conditions: vec![],
        },
        "BailDenied" => CourtOutcome::BailDenied {
            reason: form.reason.unwrap_or_default(),
        },
        "RemandContinued" => CourtOutcome::RemandContinued {
            review_date: form.next_date.as_deref().filter(|s| !s.is_empty())
                .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()),
        },
        "Acquitted" => CourtOutcome::Acquitted,
        "Sentenced" => {
            let days = form.sentence_days.unwrap_or(1);
            let sentence = SentenceDuration::from_days(days)
                .map_err(|_| HtmlError::validation("error-invalid-sentence"))?;
            CourtOutcome::Sentenced { sentence }
        }
        "ChargesWithdrawn" => CourtOutcome::ChargesWithdrawn,
        "AppealGranted" => CourtOutcome::AppealGranted,
        "AppealDenied" => CourtOutcome::AppealDenied,
        "Rescheduled" => {
            let new_date = form.next_date.as_deref().filter(|s| !s.is_empty())
                .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
                .ok_or_else(|| HtmlError::validation("error-invalid-date"))?;
            CourtOutcome::Rescheduled {
                new_date,
                reason: form.reason.unwrap_or_default(),
            }
        }
        _ => return Err(HtmlError::validation("error-invalid-data")),
    };

    state.registry.record_court_outcome(
        CourtDateId::from_uuid(court_date_id),
        outcome,
        session.operator_id,
    ).await?;

    Ok(util::hx_redirect(&headers, "/court-calendar"))
}
