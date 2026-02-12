use chrono::NaiveDate;

use crate::identifiers::DetaineeId;
use crate::legal::DetentionBasisLabel;

/// Errors that can arise from domain logic, as opposed to infrastructure
/// errors (database failures, IO errors).
#[derive(Debug, Clone, thiserror::Error)]
pub enum DomainError {
    #[error("date {provided} is in the future")]
    FutureDate { provided: NaiveDate },

    #[error("sentence duration must be at least 1 day")]
    ZeroSentence,

    #[error("invalid detention basis transition: {from:?} -> {to:?}")]
    InvalidBasisTransition {
        from: DetentionBasisLabel,
        to: DetentionBasisLabel,
    },

    #[error("detainee {id} not found")]
    DetaineeNotFound { id: DetaineeId },

    #[error("detainee {id} is no longer active")]
    InactiveDetainee { id: DetaineeId },

    #[error("detainee {id} has a release hold: {reason}")]
    ReleaseHold { id: DetaineeId, reason: String },
}

/// Errors from the analytics subsystem. Separate from DomainError because
/// analytics failures should not prevent core registry operations.
#[derive(Debug, Clone, thiserror::Error)]
pub enum AnalyticsError {
    #[error("detainee {id} not found")]
    DetaineeNotFound { id: DetaineeId },

    #[error("invalid query: {reason}")]
    InvalidQuery { reason: String },

    #[error("storage error: {message}")]
    StorageError { message: String },
}
