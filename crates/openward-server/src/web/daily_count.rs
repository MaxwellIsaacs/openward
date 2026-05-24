use axum::{
    extract::{Form, Path, State},
    http::HeaderMap,
    response::{IntoResponse, Redirect, Response},
};
use chrono::NaiveDate;
use serde::Deserialize;
use uuid::Uuid;
use openward_core::*;
use super::{require_session, nav_context, AppState_};
use crate::web::util;
use crate::i18n::T;
use crate::templates::*;
use crate::web::errors::HtmlError;


pub async fn daily_count_page(
    State(state): State<AppState_>,
    headers: HeaderMap,
) -> Result<Redirect, Redirect> {
    let _ = require_session(&headers, state.registry.pool()).await?;
    let today = chrono::Utc::now().date_naive();
    Ok(Redirect::to(&format!("/daily-count/{}", today)))
}

pub async fn daily_count_for_date(
    State(state): State<AppState_>,
    headers: HeaderMap,
    Path(date): Path<String>,
) -> Result<impl IntoResponse, HtmlError> {
    let session = require_session(&headers, state.registry.pool()).await.map_err(|_| HtmlError::redirect("/login"))?;
    let t = T::new(&session.lang, state.legal_overrides.clone());
    let _date_parsed = NaiveDate::parse_from_str(&date, "%Y-%m-%d")
        .map_err(|_| HtmlError::validation("error-invalid-date"))?;
    let count = state.registry.get_daily_count_for_date(_date_parsed).await?;
    let housing_units = state.registry.housing_units().await?;

    let count_display = count.map(|c| DailyCountDisplay {
        opening_count: c.opening_count,
        admissions: c.admissions,
        releases: c.releases,
        transfers_out: c.transfers_out,
        to_court: c.to_court,
        to_hospital: c.to_hospital,
        escapes: c.escapes,
        deaths: c.deaths,
        computed_closing: c.computed_closing,
        actual_closing_count: c.actual_closing_count,
        is_balanced: c.is_balanced,
        finalized_by: c.finalized_by.map(|op| op.to_string()),
        unit_counts: c.unit_counts.iter().map(|uc| UnitCountDisplay {
            unit_id: uc.unit_id.to_string(),
            count: uc.count,
        }).collect(),
    });

    let units_with_counts: Vec<HousingUnitWithCount> = housing_units.iter().map(|hu| {
        let unit_id_str = hu.id.to_string();
        let matched_count = count_display.as_ref().and_then(|cd| {
            cd.unit_counts.iter().find(|uc| uc.unit_id == unit_id_str).map(|uc| uc.count)
        });
        HousingUnitWithCount {
            id: unit_id_str,
            name: hu.name.clone(),
            count: matched_count,
        }
    }).collect();

    Ok(DailyCountPageTemplate {
        nav: nav_context(&session, &format!("/daily-count/{}", date)),
        count: count_display,
        date,
        units_with_counts,
        t,
    })
}

pub async fn open_count(
    State(state): State<AppState_>,
    headers: HeaderMap,
    Path(date): Path<String>,
) -> Result<Response, HtmlError> {
    let session = require_session(&headers, state.registry.pool()).await.map_err(|_| HtmlError::redirect("/login"))?;
    let date_parsed = NaiveDate::parse_from_str(&date, "%Y-%m-%d")
        .map_err(|_| HtmlError::validation("error-invalid-date"))?;
    let _count = state.registry.open_daily_count(date_parsed, session.operator_id).await?;
    Ok(util::hx_redirect(&headers, &format!("/daily-count/{}", date)))
}

#[derive(Deserialize)]
pub struct HeadcountForm {
    pub unit_id: String,
    pub count: u32,
}

pub async fn submit_headcount(
    State(state): State<AppState_>,
    headers: HeaderMap,
    Path(date): Path<String>,
    Form(form): Form<HeadcountForm>,
) -> Result<Response, HtmlError> {
    let session = require_session(&headers, state.registry.pool()).await.map_err(|_| HtmlError::redirect("/login"))?;
    let date_parsed = NaiveDate::parse_from_str(&date, "%Y-%m-%d")
        .map_err(|_| HtmlError::validation("error-invalid-date"))?;
    let unit_uuid = Uuid::parse_str(&form.unit_id)
        .map_err(|_| HtmlError::validation("error-invalid-unit"))?;
    let now = chrono::Utc::now();
    let headcount = UnitHeadcount {
        unit_id: HousingUnitId::from_uuid(unit_uuid),
        count: form.count,
        counted_by: session.operator_id,
        counted_at: now,
        received_at: now,
        was_offline: false,
    };
    state.registry.submit_unit_headcount(date_parsed, headcount).await?;
    Ok(util::hx_redirect(&headers, &format!("/daily-count/{}", date)))
}

pub async fn finalize(
    State(state): State<AppState_>,
    headers: HeaderMap,
    Path(date): Path<String>,
) -> Result<Response, HtmlError> {
    let session = require_session(&headers, state.registry.pool()).await.map_err(|_| HtmlError::redirect("/login"))?;
    let date_parsed = NaiveDate::parse_from_str(&date, "%Y-%m-%d")
        .map_err(|_| HtmlError::validation("error-invalid-date"))?;
    state.registry.finalize_daily_count(date_parsed, session.operator_id).await?;
    crate::api::spawn_auto_backup(&state);
    Ok(util::hx_redirect(&headers, &format!("/daily-count/{}", date)))
}
