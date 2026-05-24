use axum::{
    extract::{Form, Query, State},
    http::HeaderMap,
    response::{IntoResponse, Redirect, Response},
};
use chrono::NaiveDate;
use serde::Deserialize;
use openward_core::*;
use super::{require_role, nav_context, AppState_};
use crate::auth::Action;
use crate::i18n::T;
use crate::templates::*;
use crate::web::errors::HtmlError;

pub async fn admission_page(
    State(state): State<AppState_>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, HtmlError> {
    let session = require_role(&headers, state.registry.pool(), Action::AdmitDetainee).await?;
    let t = T::new(&session.lang, state.legal_overrides.clone());
    Ok(AdmissionPageTemplate {
        nav: nav_context(&session, "/admit"),
        t,
    })
}

#[derive(Deserialize)]
pub struct BasisFieldsQuery {
    pub basis_type: String,
}

pub async fn basis_fields_fragment(
    State(state): State<AppState_>,
    headers: HeaderMap,
    Query(params): Query<BasisFieldsQuery>,
) -> Result<impl IntoResponse, HtmlError> {
    let session = require_role(&headers, state.registry.pool(), Action::AdmitDetainee).await?;
    let t = T::new(&session.lang, state.legal_overrides.clone());
    Ok(BasisFieldsFragment {
        basis_type: params.basis_type,
        t,
    })
}

#[derive(Deserialize)]
#[allow(dead_code)]
pub struct AdmissionFormData {
    pub surname: String,
    pub given_names: String,
    pub preferred_name: Option<String>,
    pub sex: String,
    pub date_of_birth: Option<String>,
    pub estimated_age: Option<u32>,
    pub nationality: Option<String>,
    pub national_id: Option<String>,
    pub basis_type: String,
    pub arrest_date: Option<String>,
    pub arresting_authority: Option<String>,
    pub must_appear_by: Option<String>,
    pub first_appearance_date: Option<String>,
    pub next_court_date: Option<String>,
    pub remand_review_due: Option<String>,
    pub trial_start_date: Option<String>,
    pub conviction_date: Option<String>,
    pub sentencing_date: Option<String>,
    pub sentence_date: Option<String>,
    pub sentence_days: Option<u32>,
    pub appeal_filed_date: Option<String>,
    pub circumstances: Option<String>,
    pub warrant_issuing_authority: Option<String>,
    pub warrant_external_reference: Option<String>,
    pub intake_medical_notes: Option<String>,
    pub notes: Option<String>,
}

fn parse_date(s: &str) -> Result<NaiveDate, HtmlError> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .map_err(|_| HtmlError::validation("error-invalid-date"))
}

fn parse_past_date(s: &str) -> Result<PastDate, HtmlError> {
    let date = parse_date(s)?;
    Ok(PastDate::from_trusted(date))
}

pub async fn submit_admission(
    State(state): State<AppState_>,
    headers: HeaderMap,
    Form(form): Form<AdmissionFormData>,
) -> Result<Response, HtmlError> {
    let session = require_role(&headers, state.registry.pool(), Action::AdmitDetainee).await?;

    let sex = match form.sex.as_str() {
        "Male" => Sex::Male,
        "Female" => Sex::Female,
        _ => Sex::Other,
    };

    let identity = Identity {
        surname: form.surname,
        given_names: form.given_names,
        preferred_name: form.preferred_name.filter(|s| !s.is_empty()),
        aliases: vec![],
        date_of_birth: form
            .date_of_birth
            .as_deref()
            .filter(|s| !s.is_empty())
            .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()),
        estimated_age_at_intake: form.estimated_age,
        estimated_age_date: form.estimated_age.map(|_| PastDate::from_trusted(chrono::Utc::now().date_naive())),
        sex,
        nationality: form.nationality.filter(|s| !s.is_empty()),
        national_id: form.national_id.filter(|s| !s.is_empty()),
        languages: vec![],
        photo_hash: None,
    };

    let today = PastDate::from_trusted(chrono::Utc::now().date_naive());

    let detention_basis = match form.basis_type.as_str() {
        "PoliceCustody" => DetentionBasis::PoliceCustody {
            arrest_date: parse_past_date(form.arrest_date.as_deref().unwrap_or(""))?,
            arresting_authority: form.arresting_authority.unwrap_or_default(),
            must_appear_by: parse_date(form.must_appear_by.as_deref().unwrap_or(""))?,
            suspected_offences: None,
        },
        "RemandAwaitingTrial" => DetentionBasis::RemandAwaitingTrial {
            first_appearance_date: parse_past_date(form.first_appearance_date.as_deref().unwrap_or(""))?,
            next_court_date: form.next_court_date.as_deref().filter(|s| !s.is_empty()).and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()),
            remand_review_due: form.remand_review_due.as_deref().filter(|s| !s.is_empty()).and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()),
            bail_status: BailStatus::NotApplied,
            charges: vec![],
        },
        "OnTrial" => DetentionBasis::OnTrial {
            trial_start_date: parse_past_date(form.trial_start_date.as_deref().unwrap_or(""))?,
            next_court_date: form.next_court_date.as_deref().filter(|s| !s.is_empty()).and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()),
            bail_status: BailStatus::NotApplied,
            charges: vec![],
        },
        "ConvictedUnsentenced" => DetentionBasis::ConvictedUnsentenced {
            conviction_date: parse_past_date(form.conviction_date.as_deref().unwrap_or(""))?,
            sentencing_date: form.sentencing_date.as_deref().filter(|s| !s.is_empty()).and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()),
            bail_status: None,
            charges: vec![],
        },
        "Sentenced" => {
            let days = form.sentence_days.unwrap_or(1);
            let sentence = SentenceDuration::from_days(days)
                .map_err(|_| HtmlError::validation("error-invalid-sentence"))?;
            let sd = parse_past_date(form.sentence_date.as_deref().unwrap_or(""))?;
            let release_date = compute_release_date(sd, sentence, &[], &[]);
            DetentionBasis::Sentenced {
                sentence_date: sd,
                sentence,
                release_date,
                credits: vec![],
                adjustments: vec![],
                charges: vec![],
            }
        }
        _ => DetentionBasis::NoLegalBasis {
            discovered_date: today,
            circumstances: form.circumstances.unwrap_or_default(),
        },
    };

    let record = AdmissionRecord {
        identity,
        detention_basis,
        intake_date: today,
        warrant: None,
        intake_medical_notes: form.intake_medical_notes.filter(|s| !s.is_empty()),
        legal_reference: None,
        emergency_contacts: vec![],
        property: vec![],
        legal_representation: None,
        transfer_from: None,
        notes: form.notes.filter(|s| !s.is_empty()),
        admitted_by: session.operator_id,
    };

    let detainee = state.registry.admit(record).await?;
    Ok(Redirect::to(&format!("/detainees/{}/view", detainee.id)).into_response())
}

// === Batch Admission ===

pub async fn batch_admission_page(
    State(state): State<AppState_>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, HtmlError> {
    let session = require_role(&headers, state.registry.pool(), Action::AdmitDetainee).await?;
    let t = T::new(&session.lang, state.legal_overrides.clone());
    Ok(crate::templates::BatchAdmissionPageTemplate {
        nav: super::nav_context(&session, "/admit/batch"),
        t,
    })
}

#[derive(Deserialize)]
pub struct BatchAdmissionFormData {
    pub warrant_issuing_authority: String,
    pub warrant_external_reference: Option<String>,
    pub warrant_order_type: String,
    pub warrant_date: Option<String>,
    // Per-detainee indexed fields
    pub surname: Vec<String>,
    pub given_names: Vec<String>,
    pub sex: Vec<String>,
    pub basis_type: Vec<String>,
}

pub async fn submit_batch_admission(
    State(state): State<AppState_>,
    headers: HeaderMap,
    Form(form): Form<BatchAdmissionFormData>,
) -> Result<Response, HtmlError> {
    let session = require_role(&headers, state.registry.pool(), Action::AdmitDetainee).await?;
    let today = PastDate::from_trusted(chrono::Utc::now().date_naive());

    let order_type = match form.warrant_order_type.as_str() {
        "RemandOrder" => CommitmentOrderType::RemandOrder,
        "PoliceHolding" => CommitmentOrderType::PoliceHolding,
        "ConvictionCommitment" => CommitmentOrderType::ConvictionCommitment,
        _ => CommitmentOrderType::Other { description: form.warrant_order_type.clone() },
    };

    let warrant_date = form.warrant_date.as_deref()
        .filter(|s| !s.is_empty())
        .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
        .map(PastDate::from_trusted)
        .unwrap_or(today);

    let shared_warrant = SharedWarrantData {
        issuing_authority: form.warrant_issuing_authority,
        issuing_officer: None,
        external_reference: form.warrant_external_reference.filter(|s| !s.is_empty()),
        order_type,
        date_issued: warrant_date,
        date_received: warrant_date,
        valid_until: None,
        offence_description: None,
    };

    let mut entries = Vec::new();
    let count = form.surname.len();
    for i in 0..count {
        let surname = form.surname[i].trim().to_string();
        if surname.is_empty() {
            continue; // skip empty rows
        }
        let given_names = form.given_names.get(i).map(|s| s.trim().to_string()).unwrap_or_default();
        let sex = match form.sex.get(i).map(String::as_str) {
            Some("Male") => Sex::Male,
            Some("Female") => Sex::Female,
            _ => Sex::Other,
        };
        let basis_type = form.basis_type.get(i).map(String::as_str).unwrap_or("RemandAwaitingTrial");

        let detention_basis = match basis_type {
            "PoliceCustody" => DetentionBasis::PoliceCustody {
                arrest_date: today,
                arresting_authority: String::new(),
                must_appear_by: chrono::Utc::now().date_naive() + chrono::Duration::days(2),
                suspected_offences: None,
            },
            "Sentenced" => {
                let sentence = SentenceDuration::from_days(30).unwrap();
                let release_date = openward_core::compute_release_date(today, sentence, &[], &[]);
                DetentionBasis::Sentenced {
                    sentence_date: today,
                    sentence,
                    release_date,
                    credits: vec![],
                    adjustments: vec![],
                    charges: vec![],
                }
            }
            _ => DetentionBasis::RemandAwaitingTrial {
                first_appearance_date: today,
                next_court_date: None,
                remand_review_due: None,
                bail_status: BailStatus::NotApplied,
                charges: vec![],
            },
        };

        let identity = Identity {
            surname,
            given_names,
            preferred_name: None,
            aliases: vec![],
            date_of_birth: None,
            estimated_age_at_intake: None,
            estimated_age_date: None,
            sex,
            nationality: None,
            national_id: None,
            languages: vec![],
            photo_hash: None,
        };

        entries.push(BatchDetaineeEntry {
            identity,
            detention_basis,
            additional_charges: vec![],
            intake_medical_notes: None,
            legal_reference: None,
            emergency_contacts: vec![],
            property: vec![],
            legal_representation: None,
            notes: None,
        });
    }

    if entries.is_empty() {
        return Err(HtmlError::validation("error-invalid-data"));
    }

    let batch = BatchAdmission {
        batch_id: BatchId::new(),
        shared_warrant,
        detainees: entries,
        intake_date: today,
        admitted_by: session.operator_id,
    };

    let detainees = state.registry.batch_admit(batch).await?;
    if let Some(first) = detainees.first() {
        Ok(Redirect::to(&format!("/detainees/{}/view", first.id)).into_response())
    } else {
        Ok(Redirect::to("/detainees").into_response())
    }
}
