use chrono::Utc;
use openward_core::{
    BailStatus, Detainee, DetentionBasis, FacilityStatus, Flag, FlagCounts, HousingUnit,
};
use serde::{Deserialize, Serialize};

/// Jurisdiction-specific configuration thresholds for flag computation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FacilityConfig {
    pub capacity: u32,
    /// Maximum hours police can hold someone before first court appearance (default: 48).
    pub police_custody_limit_hours: u32,
    /// Days of pre-trial detention before ProlongedPreTrial warning (default: 180).
    pub prolonged_pretrial_warn_days: u32,
    /// Days threshold for ProlongedPreTrial escalation to critical (default: 365).
    pub prolonged_pretrial_critical_days: u32,
    /// Hours before a court date triggers CourtDateImminent (default: 48).
    pub court_date_imminent_hours: u32,
    /// Days before release triggers ReleaseImminent (default: 7).
    pub release_imminent_days: u32,
    /// Age below which a detainee is classified as Juvenile (default: 18).
    pub juvenile_cutoff_age: u32,
    /// Days between mandatory remand reviews (default: 90).
    pub remand_review_period_days: u32,
}

impl Default for FacilityConfig {
    fn default() -> Self {
        Self {
            capacity: 500,
            police_custody_limit_hours: 48,
            prolonged_pretrial_warn_days: 180,
            prolonged_pretrial_critical_days: 365,
            court_date_imminent_hours: 48,
            release_imminent_days: 7,
            juvenile_cutoff_age: 18,
            remand_review_period_days: 90,
        }
    }
}

/// Compute all applicable flags for a single detainee.
///
/// Flags are computed, not stored. This function examines the detainee's
/// current state and returns all applicable flags.
pub fn compute_flags(
    detainee: &Detainee,
    config: &FacilityConfig,
    housing_units: &[HousingUnit],
) -> Vec<Flag> {
    let mut flags = Vec::new();
    let today = Utc::now().date_naive();

    // Only compute flags for detainees who are physically present (or in transit).
    // Released, transferred, escaped, deceased detainees don't get flags.
    let is_active = matches!(
        detainee.facility_status,
        FacilityStatus::Present | FacilityStatus::InCourt | FacilityStatus::InHospital
    );
    if !is_active {
        return flags;
    }

    let days_held = (today - detainee.intake_date.as_naive()).num_days().max(0) as u32;

    // 1. NoLegalBasis
    if detainee.detention_basis.is_undocumented() {
        flags.push(Flag::NoLegalBasis { days_held });
    }

    // 2. CustodyLimitExceeded
    if let DetentionBasis::PoliceCustody {
        must_appear_by, ..
    } = &detainee.detention_basis
    {
        if today > *must_appear_by {
            let hours_over = ((today - *must_appear_by).num_hours()).max(0) as u32;
            flags.push(Flag::CustodyLimitExceeded { hours_over });
        }
    }

    // 3. NoCourtDate (non-sentenced detainee with no next court date)
    if !detainee.detention_basis.is_sentenced()
        && !detainee.detention_basis.is_undocumented()
    {
        if detainee.detention_basis.next_court_date().is_none() {
            flags.push(Flag::NoCourtDate {
                days_without: days_held,
            });
        }
    }

    // 4. RemandReviewOverdue
    if let DetentionBasis::RemandAwaitingTrial {
        remand_review_due: Some(review_due),
        ..
    } = &detainee.detention_basis
    {
        if today > *review_due {
            let days_overdue = (today - *review_due).num_days().max(0) as u32;
            flags.push(Flag::RemandReviewOverdue { days_overdue });
        }
    }

    // 5. ProlongedPreTrial
    if !detainee.detention_basis.is_sentenced()
        && !detainee.detention_basis.is_undocumented()
        && days_held > config.prolonged_pretrial_warn_days
    {
        flags.push(Flag::ProlongedPreTrial {
            days_held,
            threshold_days: config.prolonged_pretrial_warn_days,
        });
    }

    // 6. BailGrantedStillHeld
    if let Some(BailStatus::Granted { date, .. }) = detainee.detention_basis.bail_status() {
        if detainee.facility_status == FacilityStatus::Present {
            let days_since = (today - date.as_naive()).num_days().max(0) as u32;
            flags.push(Flag::BailGrantedStillHeld {
                days_since_grant: days_since,
            });
        }
    }

    // 7. ReleaseDatePassed
    if let DetentionBasis::Sentenced { release_date, .. } = &detainee.detention_basis {
        if release_date.is_past() && detainee.facility_status == FacilityStatus::Present {
            flags.push(Flag::ReleaseDatePassed {
                days_overdue: release_date.days_overdue(),
            });
        }
    }

    // 8. NoLegalRepresentation
    if detainee.legal_representation.is_none() {
        flags.push(Flag::NoLegalRepresentation);
    }

    // 9. CourtDateImminent
    if let Some(court_date) = detainee.detention_basis.next_court_date() {
        let hours_until = (court_date - today).num_hours();
        if hours_until >= 0 && hours_until <= config.court_date_imminent_hours as i64 {
            flags.push(Flag::CourtDateImminent { date: court_date });
        }
    }

    // 10. ReleaseImminent
    if let DetentionBasis::Sentenced { release_date, .. } = &detainee.detention_basis {
        let days_until = release_date.days_until();
        if days_until > 0 && days_until <= config.release_imminent_days {
            flags.push(Flag::ReleaseImminent {
                release_date: release_date.as_naive(),
                days_until,
            });
        }
    }

    // 11. HousingViolation
    if let Some(unit_id) = detainee.housing_unit {
        if let Some(unit) = housing_units.iter().find(|u| u.id == unit_id) {
            let violations = openward_core::validate_housing_assignment(
                &detainee.detention_basis,
                &detainee.identity,
                unit,
                config.juvenile_cutoff_age,
            );
            if !violations.is_empty() {
                flags.push(Flag::HousingViolation { violations });
            }
        }
    }

    // 12. WarrantExpired
    if let Some(warrant) = detainee.active_warrant() {
        if warrant.is_expired() {
            if let Some(valid_until) = warrant.valid_until {
                let days_expired = (today - valid_until).num_days().max(0) as u32;
                flags.push(Flag::WarrantExpired {
                    warrant_id: warrant.id,
                    expired_date: valid_until,
                    days_expired,
                });
            }
        }
    }

    // 13. NoActiveWarrant (non-NoLegalBasis detainee with no active warrant)
    if !detainee.detention_basis.is_undocumented() && detainee.active_warrant().is_none() {
        flags.push(Flag::NoActiveWarrant);
    }

    flags
}

/// Compute aggregate flag counts across a population.
pub fn compute_flag_counts(flags_per_detainee: &[Vec<Flag>]) -> FlagCounts {
    let mut counts = FlagCounts::default();
    for flags in flags_per_detainee {
        for flag in flags {
            match flag {
                Flag::NoLegalBasis { .. } => counts.no_legal_basis += 1,
                Flag::CustodyLimitExceeded { .. } => counts.custody_limit_exceeded += 1,
                Flag::NoCourtDate { .. } => counts.no_court_date += 1,
                Flag::RemandReviewOverdue { .. } => counts.remand_review_overdue += 1,
                Flag::ProlongedPreTrial { .. } => counts.prolonged_pre_trial += 1,
                Flag::BailGrantedStillHeld { .. } => counts.bail_granted_still_held += 1,
                Flag::ReleaseDatePassed { .. } => counts.release_date_passed += 1,
                Flag::NoLegalRepresentation => counts.no_legal_representation += 1,
                Flag::CourtDateImminent { .. } => counts.court_date_imminent += 1,
                Flag::ReleaseImminent { .. } => counts.release_imminent += 1,
                Flag::HousingViolation { .. } => counts.housing_violation += 1,
                Flag::WarrantExpired { .. } => counts.warrant_expired += 1,
                Flag::NoActiveWarrant => counts.no_active_warrant += 1,
            }
        }
    }
    counts
}
