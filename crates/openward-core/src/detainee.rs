use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::dates::PastDate;
use crate::identifiers::{
    BatchId, CourtDateId, DetaineeId, HousingUnitId, OperatorId,
};
use crate::legal::{
    AgeGroup, Charge, DetentionBasis, EmergencyContact, FacilityStatus,
    Identity, LegalReference, LegalRepresentation, Note, PropertyItem, ReleaseType, Sex,
};
use crate::warrant::{CommitmentOrder, CommitmentOrderType, SharedWarrantData};

// ===========================================================================
// Housing
// ===========================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HousingUnit {
    pub id: HousingUnitId,
    pub name: String,
    pub capacity: u32,
    pub unit_type: HousingType,
    pub designated_sex: Option<Sex>,
    pub designated_age_group: Option<AgeGroup>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HousingType {
    General,
    Medical,
    Isolation,
    Protective,
    PreTrial,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HousingViolation {
    pub rule: String,
    pub description: String,
}

/// Checks whether a housing assignment is compatible with Mandela Rules, Rule 11.
pub fn validate_housing_assignment(
    detention_basis: &DetentionBasis,
    identity: &Identity,
    unit: &HousingUnit,
    juvenile_cutoff: u32,
) -> Vec<HousingViolation> {
    let mut violations = Vec::new();

    let skip_legal_status_check = matches!(
        unit.unit_type,
        HousingType::Medical | HousingType::Protective | HousingType::Isolation
    );

    // Rule 11(a): Sex separation.
    if let Some(designated_sex) = unit.designated_sex {
        match identity.sex {
            s if s == designated_sex => {}
            Sex::Other => {
                violations.push(HousingViolation {
                    rule: "Mandela Rules, Rule 11(a) — review required".to_string(),
                    description: format!(
                        "Detainee sex is 'Other'; assigned to {:?}-designated housing. \
                         Manual review required for appropriate placement.",
                        designated_sex
                    ),
                });
            }
            _ => {
                violations.push(HousingViolation {
                    rule: "Mandela Rules, Rule 11(a)".to_string(),
                    description: format!(
                        "{:?} detainee assigned to {:?}-designated housing",
                        identity.sex, designated_sex
                    ),
                });
            }
        }
    }

    // Rule 11(d): Age separation.
    let detainee_age_group = identity.age_group(juvenile_cutoff);
    if let (Some(designated_age), Some(actual_age)) =
        (unit.designated_age_group, detainee_age_group)
    {
        if designated_age != actual_age {
            violations.push(HousingViolation {
                rule: "Mandela Rules, Rule 11(d)".to_string(),
                description: format!(
                    "{:?} detainee assigned to {:?}-designated housing",
                    actual_age, designated_age
                ),
            });
        }
    }

    // Rule 11(b): Untried/convicted separation.
    if !skip_legal_status_check {
        let is_untried = !detention_basis.is_sentenced();
        match (is_untried, unit.unit_type) {
            (true, HousingType::PreTrial) | (false, HousingType::General) => {}
            (true, HousingType::General) => {
                violations.push(HousingViolation {
                    rule: "Mandela Rules, Rule 11(b)".to_string(),
                    description: "Untried detainee assigned to convicted-population housing"
                        .to_string(),
                });
            }
            (false, HousingType::PreTrial) => {
                violations.push(HousingViolation {
                    rule: "Mandela Rules, Rule 11(b)".to_string(),
                    description: "Sentenced detainee assigned to pre-trial housing".to_string(),
                });
            }
            _ => {}
        }
    }

    violations
}

// ===========================================================================
// The detainee record
// ===========================================================================

/// A person held in the facility, regardless of legal status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Detainee {
    pub id: DetaineeId,
    pub identity: Identity,
    pub detention_basis: DetentionBasis,
    pub facility_status: FacilityStatus,
    pub intake_date: PastDate,
    pub housing_unit: Option<HousingUnitId>,
    pub warrants: Vec<CommitmentOrder>,
    pub legal_reference: Option<LegalReference>,
    pub emergency_contacts: Vec<EmergencyContact>,
    pub property: Vec<PropertyItem>,
    pub legal_representation: Option<LegalRepresentation>,
    pub notes: Vec<Note>,
}

impl Detainee {
    /// The currently active commitment order, if any.
    pub fn active_warrant(&self) -> Option<&CommitmentOrder> {
        let mut active: Vec<&CommitmentOrder> = self
            .warrants
            .iter()
            .filter(|w| w.is_active)
            .collect();

        if active.len() > 1 {
            active.sort_by(|a, b| b.date_issued.as_naive().cmp(&a.date_issued.as_naive()));
        }

        active.into_iter().next()
    }

    pub fn has_expired_warrant(&self) -> bool {
        self.active_warrant().map_or(false, |w| w.is_expired())
    }

    pub fn has_any_warrant(&self) -> bool {
        !self.warrants.is_empty()
    }

    /// Whether the warrant state is consistent with the detention basis.
    pub fn validate_warrant_consistency(&self) -> Vec<String> {
        let mut problems = Vec::new();

        match &self.detention_basis {
            DetentionBasis::NoLegalBasis { .. } => {
                if self.active_warrant().is_some() {
                    problems.push("NoLegalBasis detainee has an active warrant".to_string());
                }
            }
            _ => {
                if self.active_warrant().is_none()
                    && self.facility_status == FacilityStatus::Present
                {
                    problems.push(
                        "Active detainee with legal basis has no active warrant".to_string(),
                    );
                }
            }
        }

        let active_count = self.warrants.iter().filter(|w| w.is_active).count();
        if active_count > 1 {
            problems.push(format!(
                "{} warrants marked as active — must be at most 1",
                active_count
            ));
        }

        problems
    }

    pub fn remand_history(&self) -> Vec<&CommitmentOrder> {
        let mut remands: Vec<&CommitmentOrder> = self
            .warrants
            .iter()
            .filter(|w| w.is_remand())
            .collect();
        remands.sort_by_key(|w| w.date_issued.as_naive());
        remands
    }

    pub fn remand_renewal_count(&self) -> usize {
        self.warrants
            .iter()
            .filter(|w| {
                matches!(w.order_type, CommitmentOrderType::RemandRenewal { .. })
            })
            .count()
    }
}

// ===========================================================================
// Court dates
// ===========================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CourtDate {
    pub id: CourtDateId,
    pub detainee_id: DetaineeId,
    pub scheduled_date: NaiveDate,
    pub court_name: String,
    pub purpose: CourtPurpose,
    pub outcome: Option<CourtOutcome>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CourtPurpose {
    FirstAppearance,
    BailHearing,
    RemandReview,
    TrialHearing,
    Sentencing,
    AppealHearing,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CourtOutcome {
    Adjourned {
        next_date: Option<NaiveDate>,
        reason: String,
    },
    BailGranted {
        amount: Option<u64>,
        conditions: Vec<String>,
    },
    BailDenied {
        reason: String,
    },
    RemandContinued {
        review_date: Option<NaiveDate>,
    },
    Convicted {
        charges: Vec<Charge>,
    },
    Acquitted,
    Sentenced {
        sentence: crate::dates::SentenceDuration,
    },
    AppealGranted,
    AppealDenied,
    ChargesWithdrawn,
}

// ===========================================================================
// Admission and release records
// ===========================================================================

/// Everything needed to admit a person to the facility.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdmissionRecord {
    pub identity: Identity,
    pub detention_basis: DetentionBasis,
    pub intake_date: PastDate,
    pub warrant: Option<CommitmentOrder>,
    pub intake_medical_notes: Option<String>,
    pub legal_reference: Option<LegalReference>,
    pub emergency_contacts: Vec<EmergencyContact>,
    pub property: Vec<PropertyItem>,
    pub legal_representation: Option<LegalRepresentation>,
    pub transfer_from: Option<String>,
    pub notes: Option<String>,
    pub admitted_by: OperatorId,
}

/// A group of detainees admitted on a single warrant.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchAdmission {
    pub batch_id: BatchId,
    pub shared_warrant: SharedWarrantData,
    pub detainees: Vec<BatchDetaineeEntry>,
    pub intake_date: PastDate,
    pub admitted_by: OperatorId,
}

/// Per-detainee data within a batch admission.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchDetaineeEntry {
    pub identity: Identity,
    pub detention_basis: DetentionBasis,
    pub additional_charges: Vec<Charge>,
    pub intake_medical_notes: Option<String>,
    pub legal_reference: Option<LegalReference>,
    pub emergency_contacts: Vec<EmergencyContact>,
    pub property: Vec<PropertyItem>,
    pub legal_representation: Option<LegalRepresentation>,
    pub notes: Option<String>,
}

/// Output of the release process.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseRecord {
    pub detainee_id: DetaineeId,
    pub release_date: PastDate,
    pub release_type: ReleaseType,
    pub authorized_by: OperatorId,
    pub all_property_returned: bool,
    pub notes: Option<String>,
}

/// Records a transfer between facilities.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferRecord {
    pub id: Uuid,
    pub detainee_id: DetaineeId,
    pub from_facility: Option<String>,
    pub to_facility: Option<String>,
    pub transfer_date: PastDate,
    pub reason: String,
    pub authorized_by: OperatorId,
}
