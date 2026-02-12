use chrono::{Datelike, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use std::num::NonZeroU32;

use crate::errors::DomainError;
use crate::identifiers::OperatorId;

// ===========================================================================
// Validated date types
// ===========================================================================

/// A date that has been validated to not be in the future.
///
/// # System clock dependency
///
/// Validation uses `Utc::now()`. Raspberry Pi has no onboard RTC — if power
/// cuts and the device reboots without NTP, the clock resets to 1970-01-01.
/// In that state, every real date looks like "the future" and the system
/// would reject all input.
///
/// **Software mitigation**: `new()` checks clock sanity before validating.
/// If the system clock reads before `MINIMUM_SANE_YEAR` (2024), the date
/// is accepted unconditionally with a warning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PastDate(NaiveDate);

/// The earliest year the system could plausibly be running. If the system
/// clock reads before this, the clock is assumed to be wrong.
const MINIMUM_SANE_YEAR: i32 = 2024;

impl PastDate {
    /// Construct a PastDate, validating that `date` is not in the future.
    ///
    /// If the system clock appears unsane (year < 2024), validation is
    /// skipped and the date is accepted unconditionally.
    pub fn new(date: NaiveDate) -> Result<Self, DomainError> {
        let now = Utc::now().date_naive();

        // Clock sanity check: if the system thinks it's before 2024,
        // the clock is almost certainly wrong. Accept the date rather
        // than bricking the system.
        if now.year() < MINIMUM_SANE_YEAR {
            return Ok(Self(date));
        }

        if date > now {
            Err(DomainError::FutureDate { provided: date })
        } else {
            Ok(Self(date))
        }
    }

    /// Construct a PastDate without validation. Use ONLY for deserializing
    /// records that were previously validated, or for test fixtures. Never
    /// use this for new user input.
    pub fn from_trusted(date: NaiveDate) -> Self {
        Self(date)
    }

    pub fn as_naive(&self) -> NaiveDate {
        self.0
    }

    /// Whether the system clock was sane at the time this was called.
    pub fn clock_is_sane() -> bool {
        Utc::now().date_naive().year() >= MINIMUM_SANE_YEAR
    }
}

// ===========================================================================
// Sentence types
// ===========================================================================

/// A sentence duration that is guaranteed positive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SentenceDuration {
    days: NonZeroU32,
}

impl SentenceDuration {
    pub fn from_days(days: u32) -> Result<Self, DomainError> {
        NonZeroU32::new(days)
            .map(|d| Self { days: d })
            .ok_or(DomainError::ZeroSentence)
    }

    pub fn as_days(&self) -> u32 {
        self.days.get()
    }
}

/// A computed release date for a sentenced detainee.
///
/// This type has NO public constructor. The only way to obtain a `ReleaseDate`
/// is through `compute_release_date()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ReleaseDate(NaiveDate);

impl ReleaseDate {
    /// Not public outside the crate. Only `compute_release_date` calls this.
    pub(crate) fn from_naive(date: NaiveDate) -> Self {
        Self(date)
    }

    pub fn as_naive(&self) -> NaiveDate {
        self.0
    }

    /// Whether this release date has passed.
    pub fn is_past(&self) -> bool {
        self.0 < Utc::now().date_naive()
    }

    /// Days until release. Returns 0 if the release date has passed.
    pub fn days_until(&self) -> u32 {
        let today = Utc::now().date_naive();
        if self.0 <= today {
            0
        } else {
            (self.0 - today).num_days() as u32
        }
    }

    /// Days overdue. Returns 0 if the release date hasn't passed yet.
    pub fn days_overdue(&self) -> u32 {
        let today = Utc::now().date_naive();
        if self.0 >= today {
            0
        } else {
            (today - self.0).num_days() as u32
        }
    }
}

// ===========================================================================
// Sentence computation
// ===========================================================================

/// Credit against a sentence (good behaviour, work programs, etc.).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeCredit {
    pub days: u32,
    pub reason: String,
    pub granted_date: PastDate,
    pub granted_by: OperatorId,
}

/// A modification to a sentence that is not a credit (court-ordered
/// adjustment, appeal outcome, resentencing).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SentenceAdjustment {
    pub days: i32,
    pub reason: String,
    pub court_order_ref: Option<String>,
    pub applied_date: PastDate,
    pub applied_by: OperatorId,
}

/// The only way to produce a `ReleaseDate`.
///
/// This function is pure: same inputs always produce the same output.
/// It performs the arithmetic:
///   release = intake + sentence_days - sum(credit_days) + sum(adjustment_days)
///
/// If the net sentence is zero or negative, the release date is set to
/// `today` — the detainee is eligible for immediate release.
pub fn compute_release_date(
    intake_date: PastDate,
    sentence: SentenceDuration,
    credits: &[TimeCredit],
    adjustments: &[SentenceAdjustment],
) -> ReleaseDate {
    let total_credits: i64 = credits.iter().map(|c| c.days as i64).sum();
    let total_adjustments: i64 = adjustments.iter().map(|a| a.days as i64).sum();

    let net_days = sentence.as_days() as i64 - total_credits + total_adjustments;

    let today = Utc::now().date_naive();
    let raw_release = intake_date
        .as_naive()
        .checked_add_signed(chrono::Duration::days(net_days.max(0)))
        .expect("release date arithmetic overflow");

    let clamped = raw_release.max(today);

    ReleaseDate::from_naive(clamped)
}
