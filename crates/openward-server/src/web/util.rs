use axum::http::HeaderMap;
use axum::http::header::HeaderValue;
use axum::response::{IntoResponse, Redirect, Response};
use openward_core::*;
use crate::i18n::T;

/// Return the right kind of redirect for the request context.
/// HTMX requests get an `HX-Redirect` header (client-side redirect),
/// while regular requests get a standard HTTP 302.
pub fn hx_redirect(headers: &HeaderMap, url: &str) -> Response {
    if headers.get("HX-Request").is_some() {
        let mut resp = axum::http::StatusCode::OK.into_response();
        resp.headers_mut().insert(
            "HX-Redirect",
            HeaderValue::from_str(url).expect("valid redirect URL"),
        );
        resp
    } else {
        Redirect::to(url).into_response()
    }
}

pub fn flag_severity_class(flag: &Flag) -> &'static str {
    match flag.severity() {
        FlagSeverity::Critical => "flag-critical",
        FlagSeverity::Warning => "flag-warning",
        FlagSeverity::Info => "flag-info",
    }
}

pub fn basis_key(label: &DetentionBasisLabel) -> &'static str {
    match label {
        DetentionBasisLabel::NoLegalBasis => "legal-basis-no-legal",
        DetentionBasisLabel::PoliceCustody => "legal-basis-police-custody",
        DetentionBasisLabel::Remand => "legal-basis-remand",
        DetentionBasisLabel::OnTrial => "legal-basis-on-trial",
        DetentionBasisLabel::ConvictedUnsentenced => "legal-basis-convicted-unsentenced",
        DetentionBasisLabel::Sentenced => "legal-basis-sentenced",
        DetentionBasisLabel::Appeal => "legal-basis-appeal",
    }
}

pub fn sex_key(sex: &Sex) -> &'static str {
    match sex {
        Sex::Male => "sex-male",
        Sex::Female => "sex-female",
        Sex::Other => "sex-other",
    }
}

pub fn status_key(status: &FacilityStatus) -> &'static str {
    match status {
        FacilityStatus::Present => "legal-status-present",
        FacilityStatus::InCourt => "legal-status-in-court",
        FacilityStatus::InHospital => "legal-status-in-hospital",
        FacilityStatus::Transferred => "legal-status-transferred",
        FacilityStatus::Released => "legal-status-released",
        FacilityStatus::Escaped => "legal-status-escaped",
        FacilityStatus::Deceased => "legal-status-deceased",
    }
}

pub fn housing_type_key(ht: &HousingType) -> &'static str {
    match ht {
        HousingType::General => "housing-type-general",
        HousingType::Medical => "housing-type-medical",
        HousingType::Isolation => "housing-type-isolation",
        HousingType::Protective => "housing-type-protective",
        HousingType::PreTrial => "housing-type-pretrial",
    }
}

pub fn age_group_key(ag: &AgeGroup) -> &'static str {
    match ag {
        AgeGroup::Adult => "age-adult",
        AgeGroup::Juvenile => "age-juvenile",
    }
}

pub fn court_purpose_key(purpose: &CourtPurpose) -> &'static str {
    match purpose {
        CourtPurpose::FirstAppearance => "legal-court-first-appearance",
        CourtPurpose::BailHearing => "legal-court-bail-hearing",
        CourtPurpose::RemandReview => "legal-court-remand-review",
        CourtPurpose::TrialHearing => "legal-court-trial",
        CourtPurpose::Sentencing => "legal-court-sentencing",
        CourtPurpose::AppealHearing => "legal-court-appeal",
        CourtPurpose::Other => "legal-court-other",
    }
}

pub fn order_type_key(ot: &CommitmentOrderType) -> &'static str {
    match ot {
        CommitmentOrderType::PoliceHolding => "legal-order-police-holding",
        CommitmentOrderType::RemandOrder => "legal-order-remand",
        CommitmentOrderType::RemandRenewal { .. } => "legal-order-remand-renewal",
        CommitmentOrderType::ConvictionCommitment => "legal-order-conviction",
        CommitmentOrderType::Transfer { .. } => "legal-order-transfer",
        CommitmentOrderType::ProductionOrder => "legal-order-production",
        CommitmentOrderType::ReleaseOrder { .. } => "legal-order-release",
        CommitmentOrderType::BailOrder { .. } => "legal-order-bail",
        CommitmentOrderType::Other { .. } => "legal-order-other",
    }
}

pub fn outcome_key(outcome: &CourtOutcome) -> &'static str {
    match outcome {
        CourtOutcome::Adjourned { .. } => "legal-outcome-adjourned",
        CourtOutcome::BailGranted { .. } => "legal-outcome-bail-granted",
        CourtOutcome::BailDenied { .. } => "legal-outcome-bail-denied",
        CourtOutcome::RemandContinued { .. } => "legal-outcome-remand-continued",
        CourtOutcome::Acquitted => "legal-outcome-acquitted",
        CourtOutcome::Convicted { .. } => "legal-outcome-convicted",
        CourtOutcome::Sentenced { .. } => "legal-outcome-sentenced",
        CourtOutcome::ChargesWithdrawn => "legal-outcome-charges-withdrawn",
        CourtOutcome::AppealGranted => "legal-outcome-appeal-granted",
        CourtOutcome::AppealDenied => "legal-outcome-appeal-denied",
        CourtOutcome::Rescheduled { .. } => "legal-outcome-rescheduled",
    }
}

pub fn court_purpose_code(purpose: &CourtPurpose) -> &'static str {
    match purpose {
        CourtPurpose::FirstAppearance => "FirstAppearance",
        CourtPurpose::BailHearing => "BailHearing",
        CourtPurpose::RemandReview => "RemandReview",
        CourtPurpose::TrialHearing => "TrialHearing",
        CourtPurpose::Sentencing => "Sentencing",
        CourtPurpose::AppealHearing => "AppealHearing",
        CourtPurpose::Other => "Other",
    }
}

pub fn flag_label(t: &T, flag: &Flag) -> String {
    match flag {
        Flag::NoLegalBasis { days_held } => {
            t.get_1("legal-flag-no-legal-basis", "days", &days_held.to_string())
        }
        Flag::CustodyLimitExceeded { hours_over } => {
            t.get_1("legal-flag-custody-exceeded", "hours", &hours_over.to_string())
        }
        Flag::NoCourtDate { days_without } => {
            t.get_1("legal-flag-no-court-date", "days", &days_without.to_string())
        }
        Flag::RemandReviewOverdue { days_overdue } => {
            t.get_1("legal-flag-remand-overdue", "days", &days_overdue.to_string())
        }
        Flag::ProlongedPreTrial { days_held, .. } => {
            t.get_1("legal-flag-prolonged-pretrial", "days", &days_held.to_string())
        }
        Flag::BailGrantedStillHeld { days_since_grant } => {
            t.get_1("legal-flag-bail-still-held", "days", &days_since_grant.to_string())
        }
        Flag::ReleaseDatePassed { days_overdue } => {
            t.get_1("legal-flag-release-overdue", "days", &days_overdue.to_string())
        }
        Flag::NoLegalRepresentation => t.get("legal-flag-no-legal-rep"),
        Flag::CourtDateImminent { date } => {
            t.get_1("legal-flag-court-imminent", "date", &date.format("%Y-%m-%d").to_string())
        }
        Flag::ReleaseImminent { days_until, .. } => {
            t.get_1("legal-flag-release-imminent", "days", &days_until.to_string())
        }
        Flag::HousingViolation { .. } => t.get("legal-flag-housing-violation"),
        Flag::WarrantExpired { days_expired, .. } => {
            t.get_1("legal-flag-warrant-expired", "days", &days_expired.to_string())
        }
        Flag::NoActiveWarrant => t.get("legal-flag-no-active-warrant"),
    }
}

pub fn outcome_summary(t: &T, outcome: &Option<CourtOutcome>) -> String {
    match outcome {
        None => String::new(),
        Some(o) => t.get(outcome_key(o)),
    }
}

pub fn housing_type_code(ht: &HousingType) -> &'static str {
    match ht {
        HousingType::General => "General",
        HousingType::Medical => "Medical",
        HousingType::Isolation => "Isolation",
        HousingType::Protective => "Protective",
        HousingType::PreTrial => "PreTrial",
    }
}

pub fn sex_code(sex: &Sex) -> &'static str {
    match sex {
        Sex::Male => "Male",
        Sex::Female => "Female",
        Sex::Other => "Other",
    }
}

pub fn age_group_code(ag: &AgeGroup) -> &'static str {
    match ag {
        AgeGroup::Adult => "Adult",
        AgeGroup::Juvenile => "Juvenile",
    }
}

pub fn format_date(date: &chrono::NaiveDate) -> String {
    date.format("%Y-%m-%d").to_string()
}

/// Return a localized display label for a raw audit action name.
/// Falls back to the raw action if no i18n key is known.
pub fn action_label(t: &T, action: &str) -> String {
    let key = match action {
        "admit" => Some("audit-action-admit"),
        "update_detention_basis" => Some("audit-action-update-basis"),
        "update_facility_status" => Some("audit-action-update-status"),
        "assign_housing" => Some("audit-action-assign-housing"),
        "register_warrant" => Some("audit-action-register-warrant"),
        "schedule_court_date" => Some("audit-action-schedule-court-date"),
        "update_court_date" => Some("audit-action-update-court-date"),
        "record_court_outcome" => Some("audit-action-record-court-outcome"),
        "add_note" => Some("audit-action-add-note"),
        "delete_note" => Some("audit-action-delete-note"),
        "add_property_item" => Some("audit-action-add-property"),
        "return_property_item" => Some("audit-action-return-property"),
        "transfer" => Some("audit-action-transfer"),
        "release" => Some("audit-action-release"),
        _ => None,
    };
    match key {
        Some(k) => t.get(k),
        None => action.to_string(),
    }
}
