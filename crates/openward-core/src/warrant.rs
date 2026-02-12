use chrono::{NaiveDate, Utc};
use serde::{Deserialize, Serialize};

use crate::dates::{PastDate, SentenceDuration};
use crate::identifiers::{BatchId, DetaineeId, OperatorId, WarrantId};
use crate::legal::ReleaseType;

/// A legal document authorizing the facility to hold a person.
///
/// This is the physical-to-digital bridge. Every field here corresponds to
/// something a records officer can read off the physical warrant form.
/// The system doesn't generate warrants — courts and police do.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitmentOrder {
    pub id: WarrantId,
    pub detainee_id: DetaineeId,
    pub order_type: CommitmentOrderType,
    pub external_reference: Option<String>,
    pub issuing_authority: String,
    pub issuing_officer: Option<String>,
    pub date_issued: PastDate,
    pub date_received: PastDate,
    pub valid_until: Option<NaiveDate>,
    pub offence_description: Option<String>,
    pub sentence_details: Option<WarrantSentenceDetails>,
    pub is_active: bool,
    pub registered_by: OperatorId,
    pub batch_id: Option<BatchId>,
    pub document_hash: Option<String>,
}

/// What kind of legal authority this order represents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CommitmentOrderType {
    PoliceHolding,
    RemandOrder,
    RemandRenewal { renews: WarrantId },
    ConvictionCommitment,
    Transfer {
        from_facility: Option<String>,
        to_facility: String,
    },
    ProductionOrder,
    ReleaseOrder { release_type: ReleaseType },
    BailOrder {
        amount: Option<u64>,
        conditions: Vec<String>,
    },
    Other { description: String },
}

/// Sentence information as recorded on a conviction warrant.
/// This is an immutable historical record of what the warrant says.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WarrantSentenceDetails {
    pub sentence_text: String,
    pub sentence_duration: SentenceDuration,
    pub sentence_date: PastDate,
    pub commences: PastDate,
    pub fine_amount: Option<u64>,
}

/// Warrant fields that are shared across a group admission.
/// Extracted from CommitmentOrder to avoid re-entering per detainee.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SharedWarrantData {
    pub order_type: CommitmentOrderType,
    pub external_reference: Option<String>,
    pub issuing_authority: String,
    pub issuing_officer: Option<String>,
    pub date_issued: PastDate,
    pub date_received: PastDate,
    pub valid_until: Option<NaiveDate>,
    pub offence_description: Option<String>,
}

impl CommitmentOrder {
    /// Whether this warrant has expired.
    pub fn is_expired(&self) -> bool {
        match self.valid_until {
            Some(date) => date < Utc::now().date_naive(),
            None => false,
        }
    }

    /// Whether this is a remand-type order (initial or renewal).
    pub fn is_remand(&self) -> bool {
        matches!(
            self.order_type,
            CommitmentOrderType::RemandOrder | CommitmentOrderType::RemandRenewal { .. }
        )
    }
}
