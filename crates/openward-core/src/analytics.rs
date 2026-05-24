use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::dates::PastDate;
use crate::detainee::HousingViolation;
use crate::identifiers::{
    DailyCountId, DetaineeId, HousingUnitId, OperatorId, WarrantId,
};
use crate::legal::{
    BailStatusLabel, ChargeSeverity, DetentionBasisLabel, FacilityStatus, Sex,
};

// ===========================================================================
// Flags
// ===========================================================================

/// Discriminant-only version of Flag for use in query filters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FlagType {
    NoLegalBasis,
    CustodyLimitExceeded,
    NoCourtDate,
    RemandReviewOverdue,
    ProlongedPreTrial,
    BailGrantedStillHeld,
    ReleaseDatePassed,
    NoLegalRepresentation,
    CourtDateImminent,
    ReleaseImminent,
    HousingViolation,
    WarrantExpired,
    NoActiveWarrant,
}

/// A condition on a detainee's record that requires attention.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Flag {
    NoLegalBasis { days_held: u32 },

    CustodyLimitExceeded { hours_over: u32 },

    NoCourtDate { days_without: u32 },

    RemandReviewOverdue { days_overdue: u32 },

    ProlongedPreTrial {
        days_held: u32,
        threshold_days: u32,
    },

    BailGrantedStillHeld { days_since_grant: u32 },

    ReleaseDatePassed { days_overdue: u32 },

    NoLegalRepresentation,

    CourtDateImminent { date: NaiveDate },

    ReleaseImminent {
        release_date: NaiveDate,
        days_until: u32,
    },

    HousingViolation {
        violations: Vec<HousingViolation>,
    },

    WarrantExpired {
        warrant_id: WarrantId,
        expired_date: NaiveDate,
        days_expired: u32,
    },

    NoActiveWarrant,
}

impl Flag {
    pub fn severity(&self) -> FlagSeverity {
        match self {
            Flag::NoLegalBasis { .. } => FlagSeverity::Critical,
            Flag::CustodyLimitExceeded { .. } => FlagSeverity::Critical,
            Flag::ReleaseDatePassed { .. } => FlagSeverity::Critical,
            Flag::NoCourtDate { days_without } if *days_without > 90 => FlagSeverity::Critical,
            Flag::ProlongedPreTrial {
                days_held,
                threshold_days,
            } if *days_held > *threshold_days * 2 => FlagSeverity::Critical,
            Flag::NoCourtDate { .. } => FlagSeverity::Warning,
            Flag::RemandReviewOverdue { .. } => FlagSeverity::Warning,
            Flag::ProlongedPreTrial { .. } => FlagSeverity::Warning,
            Flag::BailGrantedStillHeld { .. } => FlagSeverity::Warning,
            Flag::NoLegalRepresentation => FlagSeverity::Warning,
            Flag::HousingViolation { .. } => FlagSeverity::Warning,
            Flag::CourtDateImminent { .. } => FlagSeverity::Info,
            Flag::ReleaseImminent { .. } => FlagSeverity::Info,
            Flag::WarrantExpired { .. } => FlagSeverity::Critical,
            Flag::NoActiveWarrant => FlagSeverity::Critical,
        }
    }

    pub fn flag_type(&self) -> FlagType {
        match self {
            Flag::NoLegalBasis { .. } => FlagType::NoLegalBasis,
            Flag::CustodyLimitExceeded { .. } => FlagType::CustodyLimitExceeded,
            Flag::NoCourtDate { .. } => FlagType::NoCourtDate,
            Flag::RemandReviewOverdue { .. } => FlagType::RemandReviewOverdue,
            Flag::ProlongedPreTrial { .. } => FlagType::ProlongedPreTrial,
            Flag::BailGrantedStillHeld { .. } => FlagType::BailGrantedStillHeld,
            Flag::ReleaseDatePassed { .. } => FlagType::ReleaseDatePassed,
            Flag::NoLegalRepresentation => FlagType::NoLegalRepresentation,
            Flag::CourtDateImminent { .. } => FlagType::CourtDateImminent,
            Flag::ReleaseImminent { .. } => FlagType::ReleaseImminent,
            Flag::HousingViolation { .. } => FlagType::HousingViolation,
            Flag::WarrantExpired { .. } => FlagType::WarrantExpired,
            Flag::NoActiveWarrant => FlagType::NoActiveWarrant,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum FlagSeverity {
    Info,
    Warning,
    Critical,
}

// ===========================================================================
// Population query
// ===========================================================================

/// Filter criteria for querying the detainee population.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PopulationQuery {
    pub name_search: Option<String>,
    pub detention_basis: Option<DetentionBasisFilter>,
    pub bail_status: Option<BailStatusFilter>,
    pub charge_severity: Option<ChargeSeverity>,
    pub held_longer_than_days: Option<u32>,
    pub held_shorter_than_days: Option<u32>,
    pub intake_date_range: Option<DateRange>,
    pub has_court_date: Option<bool>,
    pub court_date_range: Option<DateRange>,
    pub court_date_overdue: Option<bool>,
    pub remand_review_overdue: Option<bool>,
    pub facility_status: Option<FacilityStatus>,
    pub housing_unit: Option<HousingUnitId>,
    pub release_within_days: Option<u32>,
    pub release_overdue: Option<bool>,
    pub has_legal_representation: Option<bool>,
    pub sex: Option<Sex>,
    pub age_range: Option<(u32, u32)>,
    pub has_flags: Option<Vec<FlagType>>,
    pub sort_by: Option<SortField>,
    pub sort_order: Option<SortOrder>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DetentionBasisFilter {
    NoLegalBasis,
    PoliceCustody,
    RemandAwaitingTrial,
    OnTrial,
    ConvictedUnsentenced,
    Sentenced,
    SentencedOnAppeal,
    AnyPreTrial,
    Undocumented,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BailStatusFilter {
    NotApplied,
    Applied,
    GrantedStillHeld,
    Denied,
    Revoked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SortField {
    IntakeDate,
    DaysHeld,
    NextCourtDate,
    ReleaseDate,
    Name,
    DetentionBasis,
    FlagSeverity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SortOrder {
    Asc,
    Desc,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct DateRange {
    pub from: Option<NaiveDate>,
    pub to: Option<NaiveDate>,
}

// ===========================================================================
// Query results
// ===========================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PopulationQueryResult {
    pub detainees: Vec<DetaineeSummary>,
    pub total_matching: u32,
    pub statistics: QueryStatistics,
}

/// Lightweight view of a detainee for list rendering.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetaineeSummary {
    pub id: DetaineeId,
    pub name: String,
    pub sex: Sex,
    pub age: Option<u32>,
    pub detention_basis: DetentionBasisLabel,
    pub intake_date: PastDate,
    pub days_held: u32,
    pub bail_status: Option<BailStatusLabel>,
    pub next_court_date: Option<NaiveDate>,
    pub release_date: Option<NaiveDate>,
    pub housing_unit: Option<String>,
    pub has_legal_representation: bool,
    pub flags: Vec<Flag>,
}

// ===========================================================================
// Aggregate statistics
// ===========================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryStatistics {
    pub total: u32,
    pub by_detention_basis: BasisBreakdown,
    pub time_held: TimeDistribution,
    pub flag_counts: FlagCounts,
    pub with_court_date: u32,
    pub without_court_date: u32,
    pub with_legal_representation: u32,
    pub without_legal_representation: u32,
    pub bail_granted_still_held: u32,
    pub bail_not_applied: u32,
    pub by_sex: SexBreakdown,
    pub facility_capacity: u32,
    pub occupancy_rate: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BasisBreakdown {
    pub no_legal_basis: u32,
    pub police_custody: u32,
    pub remand: u32,
    pub on_trial: u32,
    pub convicted_unsentenced: u32,
    pub sentenced: u32,
    pub appeal: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TimeDistribution {
    pub under_48_hours: u32,
    pub under_1_week: u32,
    pub under_1_month: u32,
    pub under_3_months: u32,
    pub under_6_months: u32,
    pub under_1_year: u32,
    pub under_2_years: u32,
    pub over_2_years: u32,
    pub median_days: u32,
    pub mean_days: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FlagCounts {
    pub no_legal_basis: u32,
    pub custody_limit_exceeded: u32,
    pub no_court_date: u32,
    pub remand_review_overdue: u32,
    pub prolonged_pre_trial: u32,
    pub bail_granted_still_held: u32,
    pub release_date_passed: u32,
    pub no_legal_representation: u32,
    pub court_date_imminent: u32,
    pub release_imminent: u32,
    pub housing_violation: u32,
    pub warrant_expired: u32,
    pub no_active_warrant: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SexBreakdown {
    pub male: u32,
    pub female: u32,
    pub other: u32,
}

// ===========================================================================
// Facility overview
// ===========================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FacilityOverview {
    pub as_of: DateTime<Utc>,
    pub system_status: SystemStatus,
    pub total_population: u32,
    pub facility_capacity: u32,
    pub occupancy_percent: f64,
    pub basis_breakdown: BasisBreakdown,
    pub pretrial_percent: f64,
    pub time_distribution: TimeDistribution,
    pub flag_counts: FlagCounts,
    pub sex_breakdown: SexBreakdown,
    pub today_count_status: DailyCountStatus,
    pub critical: CriticalNumbers,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SystemStatus {
    Healthy,
    ClockUnsynchronized { system_time: DateTime<Utc> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DailyCountStatus {
    NotStarted,
    Open { computed_closing: u32 },
    Submitted {
        computed: u32,
        actual: u32,
        balanced: bool,
    },
    Finalized { closing: u32, balanced: bool },
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CriticalNumbers {
    pub no_legal_basis: u32,
    pub custody_limit_breaches: u32,
    pub no_court_date: u32,
    pub pretrial_over_1_year: u32,
    pub bail_granted_still_held: u32,
    pub release_overdue: u32,
    pub warrant_expired: u32,
    pub court_dates_48h: u32,
    pub releases_7_days: u32,
}

// ===========================================================================
// Trend tracking
// ===========================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrendMetric {
    TotalPopulation,
    PreTrialPercent,
    OccupancyPercent,
    MedianDaysHeld,
    CustodyLimitBreaches,
    NoCourtDate,
    PreTrialOver1Year,
    BailGrantedStillHeld,
    NoLegalRepresentation,
    NoLegalBasis,
    HousingViolations,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrendPoint {
    pub date: NaiveDate,
    pub value: f64,
}

// ===========================================================================
// Daily count
// ===========================================================================

/// A daily population reconciliation record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyCount {
    pub id: DailyCountId,
    pub date: NaiveDate,
    pub opening_count: u32,
    pub admissions: u32,
    pub transfers_in: u32,
    pub court_returns: u32,
    pub hospital_returns: u32,
    pub releases: u32,
    pub transfers_out: u32,
    pub to_court: u32,
    pub to_hospital: u32,
    pub escapes: u32,
    pub deaths: u32,
    pub computed_closing: u32,
    pub unit_counts: Vec<UnitHeadcount>,
    pub actual_closing_count: Option<u32>,
    pub is_balanced: Option<bool>,
    pub closing_by_basis: BasisBreakdown,
    pub closing_by_sex: SexBreakdown,
    pub discrepancy_note: Option<String>,
    pub counted_by: Option<OperatorId>,
    pub finalized_by: Option<OperatorId>,
    pub finalized_at: Option<DateTime<Utc>>,
}

impl DailyCount {
    pub fn calculate_closing(&self) -> u32 {
        let inflows =
            self.admissions + self.transfers_in + self.court_returns + self.hospital_returns;
        let outflows = self.releases
            + self.transfers_out
            + self.to_court
            + self.to_hospital
            + self.escapes
            + self.deaths;
        (self.opening_count + inflows).saturating_sub(outflows)
    }

    pub fn net_change(&self) -> i32 {
        let inflows = (self.admissions + self.transfers_in + self.court_returns
            + self.hospital_returns) as i32;
        let outflows = (self.releases + self.transfers_out + self.to_court + self.to_hospital
            + self.escapes + self.deaths) as i32;
        inflows - outflows
    }

    pub fn is_finalized(&self) -> bool {
        self.finalized_by.is_some()
    }

    pub fn is_internally_consistent(&self) -> bool {
        self.computed_closing == self.calculate_closing()
    }
}

/// A single housing unit's headcount.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnitHeadcount {
    pub unit_id: HousingUnitId,
    pub count: u32,
    pub counted_by: OperatorId,
    pub counted_at: DateTime<Utc>,
    pub received_at: DateTime<Utc>,
    pub was_offline: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CountSubmissionStatus {
    pub date: NaiveDate,
    pub submitted_units: Vec<UnitSubmission>,
    pub pending_units: Vec<HousingUnitId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnitSubmission {
    pub unit_id: HousingUnitId,
    pub submitted_by: OperatorId,
    pub submitted_at: DateTime<Utc>,
    pub was_offline: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyMovementSummary {
    pub date: NaiveDate,
    pub opening: u32,
    pub closing: u32,
    pub admissions: u32,
    pub releases: u32,
    pub net_change: i32,
    pub is_balanced: bool,
}

// ===========================================================================
// Audit trail
// ===========================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub operator: OperatorId,
    pub module: ModuleName,
    pub action: String,
    pub target: Option<DetaineeId>,
    pub before: Option<serde_json::Value>,
    pub after: Option<serde_json::Value>,
    pub self_hash: [u8; 32],
    pub chain_hash: [u8; 32],
    pub epoch: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModuleName {
    Registry,
    Medical,
    Disciplinary,
    Commissary,
    Visitors,
    Analytics,
    Auth,
    Config,
}

impl std::fmt::Display for ModuleName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Registry => write!(f, "Registry"),
            Self::Medical => write!(f, "Medical"),
            Self::Disciplinary => write!(f, "Disciplinary"),
            Self::Commissary => write!(f, "Commissary"),
            Self::Visitors => write!(f, "Visitors"),
            Self::Analytics => write!(f, "Analytics"),
            Self::Auth => write!(f, "Auth"),
            Self::Config => write!(f, "Config"),
        }
    }
}

impl std::str::FromStr for ModuleName {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Registry" => Ok(Self::Registry),
            "Medical" => Ok(Self::Medical),
            "Disciplinary" => Ok(Self::Disciplinary),
            "Commissary" => Ok(Self::Commissary),
            "Visitors" => Ok(Self::Visitors),
            "Analytics" => Ok(Self::Analytics),
            "Auth" => Ok(Self::Auth),
            "Config" => Ok(Self::Config),
            other => Err(format!("unknown module: {}", other)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryAuditEntry {
    pub id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub operator: OperatorId,
    pub query: PopulationQuery,
    pub result_count: u32,
    pub exported: bool,
    pub export_format: Option<ExportFormat>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExportFormat {
    Csv,
    Pdf,
    Print,
}

// ===========================================================================
// Offline sync
// ===========================================================================

/// A mutation that was generated while the client was offline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingMutation {
    pub client_id: Uuid,
    pub client_timestamp: DateTime<Utc>,
    pub device_id: String,
    pub operator: OperatorId,
    pub payload: serde_json::Value,
    pub action_type: String,
}
