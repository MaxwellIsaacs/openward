use chrono::NaiveDate;

use crate::analytics::{
    DailyCount,
    FacilityOverview, PopulationQuery, PopulationQueryResult, UnitHeadcount,
};
use crate::detainee::{
    AdmissionRecord, BatchAdmission, CourtDate, CourtOutcome, Detainee, ReleaseRecord,
};
use crate::errors::DomainError;
use crate::identifiers::{CourtDateId, DetaineeId, HousingUnitId, OperatorId};
use crate::legal::{DetentionBasis, FacilityStatus};
use crate::warrant::CommitmentOrder;

/// Error type for registry operations.
#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    #[error(transparent)]
    Domain(#[from] DomainError),

    #[error("database error: {0}")]
    Database(String),
}

/// The registry is the sole authority on detainee records.
pub trait Registry: Send + Sync {
    // === Admission ===
    fn admit(
        &self,
        record: AdmissionRecord,
    ) -> impl std::future::Future<Output = Result<Detainee, RegistryError>> + Send;

    fn batch_admit(
        &self,
        batch: BatchAdmission,
    ) -> impl std::future::Future<Output = Result<Vec<Detainee>, RegistryError>> + Send;

    // === Status Updates ===
    fn update_detention_basis(
        &self,
        id: DetaineeId,
        new_basis: DetentionBasis,
        operator: OperatorId,
    ) -> impl std::future::Future<Output = Result<Detainee, RegistryError>> + Send;

    fn update_facility_status(
        &self,
        id: DetaineeId,
        status: FacilityStatus,
        operator: OperatorId,
        notes: Option<String>,
    ) -> impl std::future::Future<Output = Result<Detainee, RegistryError>> + Send;

    fn assign_housing(
        &self,
        id: DetaineeId,
        unit: HousingUnitId,
        operator: OperatorId,
    ) -> impl std::future::Future<Output = Result<Detainee, RegistryError>> + Send;

    // === Warrant Management ===
    fn register_warrant(
        &self,
        order: CommitmentOrder,
        operator: OperatorId,
    ) -> impl std::future::Future<Output = Result<Detainee, RegistryError>> + Send;

    // === Court Dates ===
    fn schedule_court_date(
        &self,
        court_date: CourtDate,
        operator: OperatorId,
    ) -> impl std::future::Future<Output = Result<CourtDate, RegistryError>> + Send;

    fn record_court_outcome(
        &self,
        court_date_id: CourtDateId,
        outcome: CourtOutcome,
        operator: OperatorId,
    ) -> impl std::future::Future<Output = Result<CourtDate, RegistryError>> + Send;

    // === Release ===
    fn release(
        &self,
        record: ReleaseRecord,
    ) -> impl std::future::Future<Output = Result<Detainee, RegistryError>> + Send;

    // === Daily Count ===
    fn open_daily_count(
        &self,
        date: NaiveDate,
        operator: OperatorId,
    ) -> impl std::future::Future<Output = Result<DailyCount, RegistryError>> + Send;

    fn submit_unit_headcount(
        &self,
        date: NaiveDate,
        headcount: UnitHeadcount,
    ) -> impl std::future::Future<Output = Result<DailyCount, RegistryError>> + Send;

    fn finalize_daily_count(
        &self,
        date: NaiveDate,
        operator: OperatorId,
    ) -> impl std::future::Future<Output = Result<DailyCount, RegistryError>> + Send;

    // === Queries ===
    fn get_detainee(
        &self,
        id: DetaineeId,
    ) -> impl std::future::Future<Output = Result<Detainee, RegistryError>> + Send;

    fn search(
        &self,
        query: PopulationQuery,
        operator: OperatorId,
    ) -> impl std::future::Future<Output = Result<PopulationQueryResult, RegistryError>> + Send;

    fn overview(
        &self,
    ) -> impl std::future::Future<Output = Result<FacilityOverview, RegistryError>> + Send;
}
