use axum::{
    extract::{Form, Path, State},
    http::HeaderMap,
    response::{IntoResponse, Redirect, Response},
};
use chrono::NaiveDate;
use serde::Deserialize;
use uuid::Uuid;
use openward_core::*;
use openward_registry::flags::compute_flags;
use super::{require_session, require_role, nav_context, AppState_};
use crate::auth::{Action, can_do};
use crate::i18n::T;
use crate::templates::{
    DetaineeDetailTemplate, DetaineeDisplay, LegalRepDisplay, WarrantDisplay,
    CourtDateDetailDisplay, NoteDisplay, ContactDisplay, PropertyDisplay,
    FlagDisplay,
    BasisFormFragment, HousingFormFragment, HousingUnitDisplay, CourtDateFormFragment,
    ReleaseFormFragment, NoteFormFragment, PropertyFormFragment,
    TransferFormFragment, EditCourtDateFormFragment,
};
use crate::web::errors::HtmlError;
use crate::web::util;

fn parse_id(id: &str) -> Result<DetaineeId, HtmlError> {
    let uuid = Uuid::parse_str(id).map_err(|_| {
        HtmlError::Registry(RegistryError::Domain(DomainError::DetaineeNotFound {
            id: DetaineeId::from_uuid(Uuid::nil()),
        }))
    })?;
    Ok(DetaineeId::from_uuid(uuid))
}

pub async fn detail_page(
    State(state): State<AppState_>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, HtmlError> {
    let session = require_session(&headers, state.registry.pool()).await.map_err(|_| HtmlError::redirect("/login"))?;
    let t = T::new(&session.lang, state.legal_overrides.clone());
    let detainee_id = parse_id(&id)?;
    let detainee = state.registry.get_detainee(detainee_id).await?;
    let court_dates = state.registry.court_dates_for_detainee(detainee_id).await?;
    let housing_units = state.registry.housing_units().await?;
    let flags = compute_flags(&detainee, state.registry.config(), &housing_units);
    let flag_displays: Vec<FlagDisplay> = flags
        .iter()
        .map(|f| FlagDisplay {
            label: util::flag_label(&t, f),
            css_class: util::flag_severity_class(f).to_string(),
        })
        .collect();

    let detainee_display = DetaineeDisplay {
        id: detainee.id.to_string(),
        surname: detainee.identity.surname.clone(),
        given_names: detainee.identity.given_names.clone(),
        preferred_name: detainee.identity.preferred_name.clone(),
        sex: t.get(util::sex_key(&detainee.identity.sex)),
        date_of_birth: detainee.identity.date_of_birth.as_ref().map(|d| util::format_date(d)),
        estimated_age: detainee.identity.estimated_age_at_intake,
        nationality: detainee.identity.nationality.clone(),
        national_id: detainee.identity.national_id.clone(),
        languages: detainee.identity.languages.clone(),
        detention_basis_label: t.get(util::basis_key(&detainee.detention_basis.label())),
        intake_date: util::format_date(&detainee.intake_date.as_naive()),
        facility_status: t.get(util::status_key(&detainee.facility_status)),
        is_released: detainee.facility_status == FacilityStatus::Released,
        housing_unit: detainee.housing_unit.map(|u| u.to_string()),
        legal_representative: detainee.legal_representation.as_ref().map(|lr| LegalRepDisplay {
            name: lr.representative_name.clone(),
            contact: lr.contact.clone().unwrap_or_else(|| t.get("unspecified")),
        }),
        warrants: detainee.warrants.iter().map(|w| WarrantDisplay {
            issuing_authority: w.issuing_authority.clone(),
            order_type: t.get(util::order_type_key(&w.order_type)),
            date_issued: util::format_date(&w.date_issued.as_naive()),
            valid_until: w.valid_until.as_ref().map(|d| util::format_date(d)),
            is_active: w.is_active,
        }).collect(),
        court_dates: court_dates.iter().map(|cd| CourtDateDetailDisplay {
            id: cd.id.as_uuid().to_string(),
            scheduled_date: util::format_date(&cd.scheduled_date),
            court_name: cd.court_name.clone(),
            purpose: t.get(util::court_purpose_key(&cd.purpose)),
            purpose_code: util::court_purpose_code(&cd.purpose).to_string(),
            has_outcome: cd.outcome.is_some(),
            outcome: cd.outcome.as_ref().map(|o| t.get(util::outcome_key(o))),
        }).collect(),
        notes: detainee.notes.iter().map(|n| NoteDisplay {
            id: n.id,
            timestamp: n.timestamp.format("%Y-%m-%d %H:%M").to_string(),
            author: n.author.to_string(),
            content: n.content.clone(),
            note_type: t.get(&format!("note-type-{}", n.note_type.as_str())),
        }).collect(),
        emergency_contacts: detainee.emergency_contacts.iter().map(|ec| ContactDisplay {
            name: ec.name.clone(),
            relationship: ec.relationship.clone(),
            phone: ec.phone.clone().unwrap_or_else(|| t.get("unspecified")),
        }).collect(),
        property: detainee.property.iter().map(|p| PropertyDisplay {
            id: p.id,
            description: p.description.clone(),
            quantity: p.quantity,
            status: if p.returned { t.get("detainee-property-returned") } else { t.get("detainee-property-held") },
            returned: p.returned,
        }).collect(),
    };

    let can_delete_notes = can_do(&session.role, Action::DeleteNote);
    let can_add_notes = can_do(&session.role, Action::AddNote);
    let show_audit = can_do(&session.role, Action::ViewAuditTrail);

    let audit_entries = if show_audit {
        let raw = openward_registry::audit::list_audit_for_detainee(
            state.registry.pool(),
            detainee_id,
            20,
        )
        .await
        .unwrap_or_default();
        raw.into_iter()
            .map(|e| crate::web::audit::convert_audit_entry(&t, e))
            .collect()
    } else {
        Vec::new()
    };

    Ok(DetaineeDetailTemplate {
        nav: nav_context(&session, &format!("/detainees/{}", id)),
        detainee: detainee_display,
        flags: flag_displays,
        can_delete_notes,
        can_add_notes,
        audit_entries,
        show_audit,
        t,
    })
}

pub async fn basis_form(
    State(state): State<AppState_>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, HtmlError> {
    let session = require_role(&headers, state.registry.pool(), Action::UpdateBasisStatusHousing).await?;
    let t = T::new(&session.lang, state.legal_overrides.clone());
    let detainee_id = parse_id(&id)?;
    let detainee = state.registry.get_detainee(detainee_id).await?;
    Ok(BasisFormFragment {
        detainee_id: id,
        current_basis: t.get(util::basis_key(&detainee.detention_basis.label())),
        t,
    })
}

#[derive(Deserialize)]
#[allow(dead_code)]
pub struct BasisUpdateForm {
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
    pub next_hearing_date: Option<String>,
    pub circumstances: Option<String>,
}

fn parse_date(s: &str) -> Result<NaiveDate, HtmlError> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .map_err(|_| HtmlError::validation("error-invalid-date"))
}

fn parse_past_date(s: &str) -> Result<PastDate, HtmlError> {
    let date = parse_date(s)?;
    Ok(PastDate::from_trusted(date))
}

pub async fn update_basis(
    State(state): State<AppState_>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Form(form): Form<BasisUpdateForm>,
) -> Result<Response, HtmlError> {
    let session = require_role(&headers, state.registry.pool(), Action::UpdateBasisStatusHousing).await?;
    let detainee_id = parse_id(&id)?;

    let new_basis = match form.basis_type.as_str() {
        "NoLegalBasis" => DetentionBasis::NoLegalBasis {
            discovered_date: parse_past_date(
                form.arrest_date.as_deref().unwrap_or(""),
            )?,
            circumstances: form.circumstances.unwrap_or_default(),
        },
        "PoliceCustody" => DetentionBasis::PoliceCustody {
            arrest_date: parse_past_date(
                form.arrest_date.as_deref().unwrap_or(""),
            )?,
            arresting_authority: form.arresting_authority.unwrap_or_default(),
            must_appear_by: parse_date(
                form.must_appear_by.as_deref().unwrap_or(""),
            )?,
            suspected_offences: None,
        },
        "RemandAwaitingTrial" => DetentionBasis::RemandAwaitingTrial {
            first_appearance_date: parse_past_date(
                form.first_appearance_date.as_deref().unwrap_or(""),
            )?,
            next_court_date: form.next_court_date.as_deref().and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()),
            remand_review_due: form.remand_review_due.as_deref().and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()),
            bail_status: BailStatus::NotApplied,
            charges: vec![],
        },
        "OnTrial" => DetentionBasis::OnTrial {
            trial_start_date: parse_past_date(
                form.trial_start_date.as_deref().unwrap_or(""),
            )?,
            next_court_date: form.next_court_date.as_deref().and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()),
            bail_status: BailStatus::NotApplied,
            charges: vec![],
        },
        "ConvictedUnsentenced" => DetentionBasis::ConvictedUnsentenced {
            conviction_date: parse_past_date(
                form.conviction_date.as_deref().unwrap_or(""),
            )?,
            sentencing_date: form.sentencing_date.as_deref().and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()),
            bail_status: None,
            charges: vec![],
        },
        "Sentenced" => {
            let sentence_days = form.sentence_days.unwrap_or(1);
            let sentence = SentenceDuration::from_days(sentence_days)
                .map_err(|_| HtmlError::validation("error-invalid-sentence"))?;
            let sentence_date = parse_past_date(
                form.sentence_date.as_deref().unwrap_or(""),
            )?;
            let release_date = openward_core::compute_release_date(
                sentence_date, sentence, &[], &[],
            );
            DetentionBasis::Sentenced {
                sentence_date,
                sentence,
                release_date,
                credits: vec![],
                adjustments: vec![],
                charges: vec![],
            }
        },
        _ => return Err(HtmlError::validation("error-invalid-data")),
    };

    let _detainee = state
        .registry
        .update_detention_basis(detainee_id, new_basis, session.operator_id)
        .await?;

    Ok(util::hx_redirect(&headers, &format!("/detainees/{}/view", id)))
}

pub async fn housing_form(
    State(state): State<AppState_>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, HtmlError> {
    let session = require_role(&headers, state.registry.pool(), Action::UpdateBasisStatusHousing).await?;
    let t = T::new(&session.lang, state.legal_overrides.clone());
    let detainee_id = parse_id(&id)?;
    let detainee = state.registry.get_detainee(detainee_id).await?;
    let housing_units = state.registry.housing_units().await?;

    let housing_units_display: Vec<HousingUnitDisplay> = housing_units.iter().map(|hu| HousingUnitDisplay {
        id: hu.id.to_string(),
        name: hu.name.clone(),
        capacity: hu.capacity,
        designated_sex: hu.designated_sex.as_ref().map(|s| t.get(util::sex_key(s))),
    }).collect();

    Ok(HousingFormFragment {
        detainee_id: id,
        housing_units: housing_units_display,
        current_unit: detainee.housing_unit.map(|u| u.to_string()),
        t,
    })
}

#[derive(Deserialize)]
pub struct HousingUpdateForm {
    pub unit_id: String,
}

pub async fn update_housing(
    State(state): State<AppState_>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Form(form): Form<HousingUpdateForm>,
) -> Result<Response, HtmlError> {
    let session = require_role(&headers, state.registry.pool(), Action::UpdateBasisStatusHousing).await?;
    let detainee_id = parse_id(&id)?;
    let unit_uuid = Uuid::parse_str(&form.unit_id)
        .map_err(|_| HtmlError::validation("error-invalid-unit"))?;
    let unit = HousingUnitId::from_uuid(unit_uuid);
    state.registry.assign_housing(detainee_id, unit, session.operator_id).await?;
    Ok(util::hx_redirect(&headers, &format!("/detainees/{}/view", id)))
}

pub async fn court_date_form(
    State(state): State<AppState_>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, HtmlError> {
    let session = require_role(&headers, state.registry.pool(), Action::UpdateBasisStatusHousing).await?;
    let t = T::new(&session.lang, state.legal_overrides.clone());
    Ok(CourtDateFormFragment {
        detainee_id: id,
        t,
    })
}

#[derive(Deserialize)]
pub struct CourtDateForm {
    pub scheduled_date: String,
    pub court_name: String,
    pub purpose: String,
}

pub async fn schedule_court_date(
    State(state): State<AppState_>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Form(form): Form<CourtDateForm>,
) -> Result<Response, HtmlError> {
    let session = require_role(&headers, state.registry.pool(), Action::UpdateBasisStatusHousing).await?;
    let detainee_id = parse_id(&id)?;
    let scheduled_date = parse_date(&form.scheduled_date)?;
    let purpose = match form.purpose.as_str() {
        "FirstAppearance" => CourtPurpose::FirstAppearance,
        "BailHearing" => CourtPurpose::BailHearing,
        "RemandReview" => CourtPurpose::RemandReview,
        "TrialHearing" => CourtPurpose::TrialHearing,
        "Sentencing" => CourtPurpose::Sentencing,
        "AppealHearing" => CourtPurpose::AppealHearing,
        _ => CourtPurpose::Other,
    };
    let court_date = CourtDate {
        id: CourtDateId::new(),
        detainee_id,
        scheduled_date,
        court_name: form.court_name,
        purpose,
        outcome: None,
    };
    state.registry.schedule_court_date(court_date, session.operator_id).await?;
    Ok(util::hx_redirect(&headers, &format!("/detainees/{}/view", id)))
}

pub async fn release_form(
    State(state): State<AppState_>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, HtmlError> {
    let session = require_role(&headers, state.registry.pool(), Action::ReleaseDetainee).await?;
    let t = T::new(&session.lang, state.legal_overrides.clone());
    Ok(ReleaseFormFragment {
        detainee_id: id,
        t,
    })
}

#[derive(Deserialize)]
pub struct ReleaseFormData {
    pub release_type: String,
    pub notes: Option<String>,
}

pub async fn do_release(
    State(state): State<AppState_>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Form(form): Form<ReleaseFormData>,
) -> Result<Response, HtmlError> {
    let session = require_role(&headers, state.registry.pool(), Action::ReleaseDetainee).await?;
    let detainee_id = parse_id(&id)?;
    let release_type = match form.release_type.as_str() {
        "SentenceExpired" => ReleaseType::SentenceExpired,
        "Bail" => ReleaseType::Bail,
        "ChargesWithdrawn" => ReleaseType::ChargesWithdrawn,
        "Acquitted" => ReleaseType::Acquitted,
        "CourtOrdered" => ReleaseType::CourtOrdered,
        "Pardon" => ReleaseType::Pardon,
        "NoLegalBasis" => ReleaseType::NoLegalBasis,
        "Transfer" => ReleaseType::Transfer,
        _ => ReleaseType::Other,
    };
    let today = PastDate::from_trusted(chrono::Utc::now().date_naive());
    let record = ReleaseRecord {
        detainee_id,
        release_date: today,
        release_type,
        authorized_by: session.operator_id,
        all_property_returned: false,
        notes: form.notes,
    };
    state.registry.release(record).await?;
    Ok(util::hx_redirect(&headers, &format!("/detainees/{}/view", id)))
}

// === Note CRUD ===

pub async fn note_form(
    State(state): State<AppState_>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, HtmlError> {
    let session = require_role(&headers, state.registry.pool(), Action::AddNote).await?;
    let t = T::new(&session.lang, state.legal_overrides.clone());
    Ok(NoteFormFragment {
        detainee_id: id,
        t,
    })
}

#[derive(Deserialize)]
pub struct AddNoteForm {
    pub content: String,
    pub note_type: String,
}

pub async fn add_note(
    State(state): State<AppState_>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Form(form): Form<AddNoteForm>,
) -> Result<Response, HtmlError> {
    let session = require_role(&headers, state.registry.pool(), Action::AddNote).await?;
    let detainee_id = parse_id(&id)?;
    let content = form.content.trim().to_string();
    if content.is_empty() {
        return Err(HtmlError::validation("error-invalid-data"));
    }
    let note_type = NoteType::from_str_lossy(&form.note_type);
    state.registry.add_note(detainee_id, content, note_type, session.operator_id).await?;
    Ok(Redirect::to(&format!("/detainees/{}/view", id)).into_response())
}

pub async fn delete_note(
    State(state): State<AppState_>,
    headers: HeaderMap,
    Path((id, nid)): Path<(String, i64)>,
) -> Result<Response, HtmlError> {
    let session = require_role(&headers, state.registry.pool(), Action::DeleteNote).await?;
    state.registry.delete_note(nid, session.operator_id).await?;
    Ok(Redirect::to(&format!("/detainees/{}/view", id)).into_response())
}

// === Property CRUD ===

pub async fn property_form(
    State(state): State<AppState_>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, HtmlError> {
    let session = require_role(&headers, state.registry.pool(), Action::AddNote).await?;
    let t = T::new(&session.lang, state.legal_overrides.clone());
    Ok(PropertyFormFragment {
        detainee_id: id,
        t,
    })
}

#[derive(Deserialize)]
pub struct AddPropertyForm {
    pub description: String,
    pub quantity: Option<u32>,
}

pub async fn add_property(
    State(state): State<AppState_>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Form(form): Form<AddPropertyForm>,
) -> Result<Response, HtmlError> {
    let session = require_role(&headers, state.registry.pool(), Action::AddNote).await?;
    let detainee_id = parse_id(&id)?;
    let description = form.description.trim().to_string();
    if description.is_empty() {
        return Err(HtmlError::validation("error-invalid-data"));
    }
    let quantity = form.quantity.unwrap_or(1).max(1);
    state.registry.add_property_item(detainee_id, description, quantity, session.operator_id).await?;
    Ok(Redirect::to(&format!("/detainees/{}/view", id)).into_response())
}

pub async fn return_property(
    State(state): State<AppState_>,
    headers: HeaderMap,
    Path((id, pid)): Path<(String, i64)>,
) -> Result<Response, HtmlError> {
    let session = require_role(&headers, state.registry.pool(), Action::AddNote).await?;
    state.registry.return_property_item(pid, session.operator_id).await?;
    Ok(Redirect::to(&format!("/detainees/{}/view", id)).into_response())
}

// === Transfer ===

pub async fn transfer_form(
    State(state): State<AppState_>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, HtmlError> {
    let session = require_role(&headers, state.registry.pool(), Action::ReleaseDetainee).await?;
    let t = T::new(&session.lang, state.legal_overrides.clone());
    Ok(TransferFormFragment {
        detainee_id: id,
        t,
    })
}

#[derive(Deserialize)]
pub struct TransferFormData {
    pub to_facility: String,
    pub reason: String,
    pub transfer_date: String,
    pub reference_number: Option<String>,
}

pub async fn do_transfer(
    State(state): State<AppState_>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Form(form): Form<TransferFormData>,
) -> Result<Response, HtmlError> {
    let session = require_role(&headers, state.registry.pool(), Action::ReleaseDetainee).await?;
    let detainee_id = parse_id(&id)?;
    let transfer_date = parse_past_date(&form.transfer_date)?;
    let mut reason = form.reason;
    if let Some(ref_num) = form.reference_number.filter(|s| !s.is_empty()) {
        reason = format!("{} (ref: {})", reason, ref_num);
    }
    let record = TransferRecord {
        id: Uuid::new_v4(),
        detainee_id,
        from_facility: None,
        to_facility: Some(form.to_facility),
        transfer_date,
        reason,
        authorized_by: session.operator_id,
    };
    state.registry.transfer(record).await?;
    Ok(util::hx_redirect(&headers, &format!("/detainees/{}/view", id)))
}

// === Court Date Edit ===

pub async fn edit_court_date_form(
    State(state): State<AppState_>,
    headers: HeaderMap,
    Path((id, cdid)): Path<(String, String)>,
) -> Result<impl IntoResponse, HtmlError> {
    let session = require_role(&headers, state.registry.pool(), Action::UpdateBasisStatusHousing).await?;
    let t = T::new(&session.lang, state.legal_overrides.clone());
    let court_date_id = Uuid::parse_str(&cdid)
        .map_err(|_| HtmlError::validation("error-invalid-id"))?;
    let cd = state.registry.get_court_date(CourtDateId::from_uuid(court_date_id)).await?;
    Ok(EditCourtDateFormFragment {
        detainee_id: id,
        court_date_id: cdid,
        scheduled_date: util::format_date(&cd.scheduled_date),
        court_name: cd.court_name,
        purpose_code: util::court_purpose_code(&cd.purpose).to_string(),
        t,
    })
}

pub async fn update_court_date(
    State(state): State<AppState_>,
    headers: HeaderMap,
    Path((id, cdid)): Path<(String, String)>,
    Form(form): Form<CourtDateForm>,
) -> Result<Response, HtmlError> {
    let session = require_role(&headers, state.registry.pool(), Action::UpdateBasisStatusHousing).await?;
    let court_date_id = Uuid::parse_str(&cdid)
        .map_err(|_| HtmlError::validation("error-invalid-id"))?;
    let scheduled_date = parse_date(&form.scheduled_date)?;
    let purpose = match form.purpose.as_str() {
        "FirstAppearance" => CourtPurpose::FirstAppearance,
        "BailHearing" => CourtPurpose::BailHearing,
        "RemandReview" => CourtPurpose::RemandReview,
        "TrialHearing" => CourtPurpose::TrialHearing,
        "Sentencing" => CourtPurpose::Sentencing,
        "AppealHearing" => CourtPurpose::AppealHearing,
        _ => CourtPurpose::Other,
    };
    state.registry.update_court_date(
        CourtDateId::from_uuid(court_date_id),
        scheduled_date,
        form.court_name,
        purpose,
        session.operator_id,
    ).await?;
    Ok(util::hx_redirect(&headers, &format!("/detainees/{}/view", id)))
}
