use chrono::{DateTime, Datelike, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

use crate::dates::{
    PastDate, ReleaseDate, SentenceAdjustment, SentenceDuration, TimeCredit,
};
use crate::errors::DomainError;
use crate::identifiers::{ChargeId, OperatorId};

// ===========================================================================
// Identity
// ===========================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Sex {
    Male,
    Female,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgeGroup {
    Juvenile,
    Adult,
}

/// Personal identity information for a detainee.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Identity {
    pub surname: String,
    pub given_names: String,
    pub preferred_name: Option<String>,
    pub aliases: Vec<String>,
    pub date_of_birth: Option<NaiveDate>,
    pub estimated_age_at_intake: Option<u32>,
    pub estimated_age_date: Option<PastDate>,
    pub sex: Sex,
    pub nationality: Option<String>,
    pub national_id: Option<String>,
    pub languages: Vec<String>,
    pub photo_hash: Option<String>,
}

impl Identity {
    /// Compute the detainee's current age. Returns None if neither DOB nor
    /// estimated age is available.
    pub fn current_age(&self) -> Option<u32> {
        let today = Utc::now().date_naive();
        if let Some(dob) = self.date_of_birth {
            let age = today.year() - dob.year();
            let had_birthday = (today.month(), today.day()) >= (dob.month(), dob.day());
            Some(if had_birthday { age as u32 } else { (age - 1) as u32 })
        } else if let (Some(est_age), Some(est_date)) =
            (self.estimated_age_at_intake, self.estimated_age_date)
        {
            let days_since = (today - est_date.as_naive()).num_days().max(0) as f64;
            let years_since = (days_since / 365.25) as u32;
            Some(est_age + years_since)
        } else {
            None
        }
    }

    /// Compute the detainee's age group for housing classification.
    pub fn age_group(&self, juvenile_cutoff: u32) -> Option<AgeGroup> {
        self.current_age().map(|age| {
            if age < juvenile_cutoff {
                AgeGroup::Juvenile
            } else {
                AgeGroup::Adult
            }
        })
    }
}

// ===========================================================================
// Legal domain types
// ===========================================================================

/// A criminal charge.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Charge {
    pub id: ChargeId,
    pub description: String,
    pub statute: Option<String>,
    pub severity: ChargeSeverity,
    pub date_of_alleged_offence: Option<NaiveDate>,
    pub count_number: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChargeSeverity {
    Minor,
    Moderate,
    Serious,
}

/// Tracks the bail status of a detainee.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BailStatus {
    NotApplied,
    Applied { date: PastDate },
    Granted {
        date: PastDate,
        amount: Option<u64>,
        conditions: Vec<String>,
    },
    Denied { date: PastDate, reason: String },
    Revoked { date: PastDate, reason: String },
}

/// Links a detainee to their external legal case.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LegalReference {
    pub case_number: String,
    pub court: String,
    pub judge: Option<String>,
}

// ===========================================================================
// Detention basis
// ===========================================================================

/// Why this person is being held.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DetentionBasis {
    NoLegalBasis {
        discovered_date: PastDate,
        circumstances: String,
    },

    PoliceCustody {
        arrest_date: PastDate,
        arresting_authority: String,
        must_appear_by: NaiveDate,
        suspected_offences: Option<Vec<Charge>>,
    },

    RemandAwaitingTrial {
        first_appearance_date: PastDate,
        next_court_date: Option<NaiveDate>,
        remand_review_due: Option<NaiveDate>,
        bail_status: BailStatus,
        charges: Vec<Charge>,
    },

    OnTrial {
        trial_start_date: PastDate,
        next_court_date: Option<NaiveDate>,
        bail_status: BailStatus,
        charges: Vec<Charge>,
    },

    ConvictedUnsentenced {
        conviction_date: PastDate,
        sentencing_date: Option<NaiveDate>,
        bail_status: Option<BailStatus>,
        charges: Vec<Charge>,
    },

    Sentenced {
        sentence_date: PastDate,
        sentence: SentenceDuration,
        release_date: ReleaseDate,
        credits: Vec<TimeCredit>,
        adjustments: Vec<SentenceAdjustment>,
        charges: Vec<Charge>,
    },

    SentencedOnAppeal {
        sentence: SentenceDuration,
        provisional_release_date: ReleaseDate,
        appeal_filed_date: PastDate,
        next_hearing_date: Option<NaiveDate>,
        credits: Vec<TimeCredit>,
        adjustments: Vec<SentenceAdjustment>,
        charges: Vec<Charge>,
    },
}

impl DetentionBasis {
    pub fn is_sentenced(&self) -> bool {
        matches!(
            self,
            DetentionBasis::Sentenced { .. } | DetentionBasis::SentencedOnAppeal { .. }
        )
    }

    pub fn is_undocumented(&self) -> bool {
        matches!(self, DetentionBasis::NoLegalBasis { .. })
    }

    pub fn next_court_date(&self) -> Option<NaiveDate> {
        match self {
            DetentionBasis::NoLegalBasis { .. } => None,
            DetentionBasis::PoliceCustody { must_appear_by, .. } => Some(*must_appear_by),
            DetentionBasis::RemandAwaitingTrial { next_court_date, .. } => *next_court_date,
            DetentionBasis::OnTrial { next_court_date, .. } => *next_court_date,
            DetentionBasis::ConvictedUnsentenced { sentencing_date, .. } => *sentencing_date,
            DetentionBasis::Sentenced { .. } => None,
            DetentionBasis::SentencedOnAppeal { next_hearing_date, .. } => *next_hearing_date,
        }
    }

    pub fn charges(&self) -> &[Charge] {
        match self {
            DetentionBasis::NoLegalBasis { .. } => &[],
            DetentionBasis::PoliceCustody { .. } => &[],
            DetentionBasis::RemandAwaitingTrial { charges, .. } => charges,
            DetentionBasis::OnTrial { charges, .. } => charges,
            DetentionBasis::ConvictedUnsentenced { charges, .. } => charges,
            DetentionBasis::Sentenced { charges, .. } => charges,
            DetentionBasis::SentencedOnAppeal { charges, .. } => charges,
        }
    }

    pub fn label(&self) -> DetentionBasisLabel {
        match self {
            DetentionBasis::NoLegalBasis { .. } => DetentionBasisLabel::NoLegalBasis,
            DetentionBasis::PoliceCustody { .. } => DetentionBasisLabel::PoliceCustody,
            DetentionBasis::RemandAwaitingTrial { .. } => DetentionBasisLabel::Remand,
            DetentionBasis::OnTrial { .. } => DetentionBasisLabel::OnTrial,
            DetentionBasis::ConvictedUnsentenced { .. } => DetentionBasisLabel::ConvictedUnsentenced,
            DetentionBasis::Sentenced { .. } => DetentionBasisLabel::Sentenced,
            DetentionBasis::SentencedOnAppeal { .. } => DetentionBasisLabel::Appeal,
        }
    }

    /// Extract bail status if present on this basis variant.
    pub fn bail_status(&self) -> Option<&BailStatus> {
        match self {
            DetentionBasis::RemandAwaitingTrial { bail_status, .. } => Some(bail_status),
            DetentionBasis::OnTrial { bail_status, .. } => Some(bail_status),
            DetentionBasis::ConvictedUnsentenced { bail_status, .. } => bail_status.as_ref(),
            _ => None,
        }
    }
}

/// Simplified label for UI display and filtering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DetentionBasisLabel {
    NoLegalBasis,
    PoliceCustody,
    Remand,
    OnTrial,
    ConvictedUnsentenced,
    Sentenced,
    Appeal,
}

impl std::fmt::Display for DetentionBasisLabel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoLegalBasis => write!(f, "NoLegalBasis"),
            Self::PoliceCustody => write!(f, "PoliceCustody"),
            Self::Remand => write!(f, "Remand"),
            Self::OnTrial => write!(f, "OnTrial"),
            Self::ConvictedUnsentenced => write!(f, "ConvictedUnsentenced"),
            Self::Sentenced => write!(f, "Sentenced"),
            Self::Appeal => write!(f, "Appeal"),
        }
    }
}

// ===========================================================================
// Detention basis transition validation
// ===========================================================================

/// Returns the set of valid target statuses from a given source status.
pub fn valid_transitions(from: DetentionBasisLabel) -> &'static [DetentionBasisLabel] {
    use DetentionBasisLabel::*;
    match from {
        NoLegalBasis => &[PoliceCustody, Remand, OnTrial, ConvictedUnsentenced],
        PoliceCustody => &[Remand, OnTrial],
        Remand => &[OnTrial, Sentenced],
        OnTrial => &[ConvictedUnsentenced, Sentenced, Remand],
        ConvictedUnsentenced => &[Sentenced, OnTrial, Remand],
        Sentenced => &[Appeal],
        Appeal => &[Sentenced, OnTrial, Remand],
    }
}

/// Validates whether a transition from one DetentionBasis to another is
/// legally permissible.
pub fn validate_basis_transition(
    from: DetentionBasisLabel,
    to: DetentionBasisLabel,
) -> Result<(), DomainError> {
    if valid_transitions(from).contains(&to) {
        Ok(())
    } else {
        Err(DomainError::InvalidBasisTransition { from, to })
    }
}

// ===========================================================================
// Facility status
// ===========================================================================

/// Where the detainee physically is right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FacilityStatus {
    Present,
    InCourt,
    InHospital,
    Transferred,
    Released,
    Escaped,
    Deceased,
}

impl std::fmt::Display for FacilityStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Present => write!(f, "Present"),
            Self::InCourt => write!(f, "InCourt"),
            Self::InHospital => write!(f, "InHospital"),
            Self::Transferred => write!(f, "Transferred"),
            Self::Released => write!(f, "Released"),
            Self::Escaped => write!(f, "Escaped"),
            Self::Deceased => write!(f, "Deceased"),
        }
    }
}

// ===========================================================================
// Release type
// ===========================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReleaseType {
    SentenceExpired,
    Bail,
    ChargesWithdrawn,
    Acquitted,
    CourtOrdered,
    Pardon,
    NoLegalBasis,
    Transfer,
    Death,
    Other,
}

// ===========================================================================
// Supporting types
// ===========================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LegalRepresentation {
    pub representative_name: String,
    pub representative_type: String,
    pub contact: Option<String>,
    pub assigned_date: PastDate,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmergencyContact {
    pub name: String,
    pub relationship: String,
    pub phone: Option<String>,
    pub address: Option<String>,
}

/// Personal belongings logged at intake.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropertyItem {
    pub description: String,
    pub quantity: u32,
    pub logged_date: PastDate,
    pub logged_by: OperatorId,
    pub returned: bool,
}

/// A timestamped free-text note attached to a detainee record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Note {
    pub content: String,
    pub author: OperatorId,
    pub timestamp: DateTime<Utc>,
}

/// Simplified bail status label for display in list views.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BailStatusLabel {
    NoBail,
    BailApplied,
    BailGranted,
    BailDenied,
}
