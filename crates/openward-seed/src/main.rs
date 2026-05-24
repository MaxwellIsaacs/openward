//! Demo data seeder for OpenWard.
//!
//! Populates a fresh database with ~80 synthetic detainees distributed
//! across detention bases, with intake dates and warrant fields calibrated
//! to trigger a realistic mix of critical and warning flags on the dashboard.
//!
//! Refuses to run against a database that already contains detainees unless
//! `--force` is passed.

use std::path::PathBuf;
use std::process::ExitCode;

use chrono::{Days, NaiveDate, Utc};
use clap::Parser;
use openward_core::dates::PastDate;
use openward_core::detainee::{
    AdmissionRecord, CourtDate, CourtPurpose, ReleaseRecord,
};
use openward_core::identifiers::{
    CourtDateId, DetaineeId, HousingUnitId, OperatorId,
};
use openward_core::legal::{
    BailStatus, Charge, ChargeSeverity, DetentionBasis, Identity,
    LegalRepresentation, ReleaseType, Sex,
};
use openward_core::traits::Registry;
use openward_facility::ServerConfig;
use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rand::{Rng, SeedableRng};
use uuid::Uuid;

const DEFAULT_SEED: u64 = 0x0DDC0FFEE_DEFAC8E0_u64;

/// Seed a fresh OpenWard database with demo data for an HTTPS demo.
#[derive(Parser, Debug)]
#[command(version, about)]
struct Args {
    /// Path to the SQLite database. Must match the server's OPENWARD_DB.
    #[arg(long)]
    db: PathBuf,

    /// Number of detainees to generate.
    #[arg(long, default_value_t = 80)]
    count: usize,

    /// Proceed even if detainees already exist. USE WITH CARE.
    #[arg(long)]
    force: bool,

    /// RNG seed for reproducibility.
    #[arg(long, default_value_t = DEFAULT_SEED)]
    seed: u64,
}

#[tokio::main]
async fn main() -> ExitCode {
    let args = Args::parse();

    // The server picks up DB path from OPENWARD_DB; mirror that for
    // ServerConfig::from_env, so existing facility init logic runs unchanged.
    std::env::set_var("OPENWARD_DB", &args.db);
    let config = ServerConfig::from_env();

    let state = match openward_facility::init(&config).await {
        Ok(state) => state,
        Err(e) => {
            eprintln!("failed to initialize facility: {e}");
            return ExitCode::FAILURE;
        }
    };

    if let Err(e) = openward_server::auth::seed_default_admin(state.registry.pool()).await {
        eprintln!("failed to seed default admin: {e}");
        return ExitCode::FAILURE;
    }

    let admin_id = match fetch_admin_id(state.registry.pool()).await {
        Ok(id) => id,
        Err(e) => {
            eprintln!("failed to look up seeded admin: {e}");
            return ExitCode::FAILURE;
        }
    };

    let housing_id = match fetch_default_housing(state.registry.pool()).await {
        Ok(id) => id,
        Err(e) => {
            eprintln!("failed to look up default housing unit: {e}");
            return ExitCode::FAILURE;
        }
    };

    let existing = match count_detainees(state.registry.pool()).await {
        Ok(n) => n,
        Err(e) => {
            eprintln!("failed to count existing detainees: {e}");
            return ExitCode::FAILURE;
        }
    };
    if existing > 0 && !args.force {
        eprintln!(
            "database already has {existing} detainees. Refusing to seed without --force."
        );
        return ExitCode::FAILURE;
    }

    let mut rng = StdRng::seed_from_u64(args.seed);
    let specs = build_specs(args.count, &mut rng);

    let mut admitted: Vec<DetaineeId> = Vec::with_capacity(specs.len());
    let mut sentenced_present: Vec<DetaineeId> = Vec::new();
    let mut remand_or_trial: Vec<DetaineeId> = Vec::new();

    for spec in &specs {
        let identity = build_identity(spec, &mut rng);
        let intake = past_date(spec.intake_days_ago);
        let basis = build_basis(spec, intake, &mut rng);
        let legal_rep = if spec.has_legal_rep {
            Some(LegalRepresentation {
                representative_name: pick(&LEGAL_REPS, &mut rng).to_string(),
                representative_type: "Public Defender".to_string(),
                contact: None,
                assigned_date: intake,
            })
        } else {
            None
        };

        let record = AdmissionRecord {
            identity,
            detention_basis: basis,
            intake_date: intake,
            warrant: None,
            intake_medical_notes: None,
            legal_reference: None,
            emergency_contacts: Vec::new(),
            property: Vec::new(),
            legal_representation: legal_rep,
            transfer_from: None,
            notes: None,
            admitted_by: admin_id,
        };

        match state.registry.admit(record).await {
            Ok(detainee) => {
                let id = detainee.id;
                if spec.assign_housing {
                    let _ = state
                        .registry
                        .assign_housing(id, housing_id, admin_id)
                        .await;
                }
                if matches!(spec.bucket, Bucket::SentencedPastRelease | Bucket::SentencedImminent | Bucket::SentencedNormal) {
                    sentenced_present.push(id);
                }
                if matches!(spec.bucket, Bucket::RemandNormal | Bucket::RemandProlongedWarn | Bucket::RemandProlongedCritical | Bucket::RemandReviewOverdue | Bucket::OnTrial) {
                    remand_or_trial.push(id);
                }
                admitted.push(id);
            }
            Err(e) => {
                eprintln!("admit failed for {} {}: {e}", spec.surname, spec.given_names);
            }
        }
    }

    // Schedule a handful of court dates against remand/trial detainees,
    // including 1-2 within the next 48h so CourtDateImminent fires.
    let court_targets = sample(&remand_or_trial, 10, &mut rng);
    for (i, did) in court_targets.iter().enumerate() {
        let offset_days: i64 = if i == 0 { 1 } else if i == 1 { 2 } else { (i as i64 + 5) % 60 + 7 };
        let scheduled = Utc::now().date_naive() + chrono::Duration::days(offset_days);
        let court = CourtDate {
            id: CourtDateId::new(),
            detainee_id: *did,
            scheduled_date: scheduled,
            court_name: pick(&COURTS, &mut rng).to_string(),
            purpose: CourtPurpose::RemandReview,
            outcome: None,
        };
        let _ = state.registry.schedule_court_date(court, admin_id).await;
    }

    // Release a few earlier detainees so the "released" UI is exercised.
    let release_targets = sample(&admitted, 5, &mut rng);
    for did in &release_targets {
        let release = ReleaseRecord {
            detainee_id: *did,
            release_date: past_date(rng.gen_range(2..20)),
            release_type: ReleaseType::Bail,
            authorized_by: admin_id,
            all_property_returned: true,
            notes: None,
        };
        let _ = state.registry.release(release).await;
    }

    println!(
        "seeded {} detainees ({} sentenced & present, {} on remand or trial); \
         scheduled {} court dates; released {}",
        admitted.len(),
        sentenced_present.len(),
        remand_or_trial.len(),
        court_targets.len(),
        release_targets.len(),
    );
    println!("dashboard should show flags across critical and warning categories.");
    ExitCode::SUCCESS
}

// ===========================================================================
// Distribution
// ===========================================================================

#[derive(Clone, Copy, Debug)]
enum Bucket {
    NoLegalBasis,
    PoliceCustodyExceeded,
    PoliceCustodyOk,
    RemandProlongedCritical, // intake >= 365 days
    RemandProlongedWarn,     // intake 180-364 days
    RemandReviewOverdue,
    RemandNormal,
    OnTrial,
    ConvictedUnsentenced,
    SentencedPastRelease,
    SentencedImminent,
    SentencedNormal,
    Appeal,
}

#[derive(Debug)]
struct Spec {
    bucket: Bucket,
    surname: String,
    given_names: String,
    sex: Sex,
    age: u32,
    intake_days_ago: u32,
    assign_housing: bool,
    has_legal_rep: bool,
}

fn build_specs(total: usize, rng: &mut StdRng) -> Vec<Spec> {
    // Calibrated counts; rest of `total` goes into RemandNormal so the
    // total comes out right regardless of `count`.
    let fixed: Vec<(Bucket, usize)> = vec![
        (Bucket::NoLegalBasis, 2),
        (Bucket::PoliceCustodyExceeded, 2),
        (Bucket::PoliceCustodyOk, 1),
        (Bucket::RemandProlongedCritical, 3),
        (Bucket::RemandProlongedWarn, 5),
        (Bucket::RemandReviewOverdue, 2),
        (Bucket::OnTrial, 8),
        (Bucket::ConvictedUnsentenced, 4),
        (Bucket::SentencedPastRelease, 2),
        (Bucket::SentencedImminent, 3),
        (Bucket::SentencedNormal, 20),
        (Bucket::Appeal, 3),
    ];
    let fixed_total: usize = fixed.iter().map(|(_, n)| n).sum();
    let remand_normal = total.saturating_sub(fixed_total);

    let mut buckets: Vec<Bucket> = Vec::with_capacity(total);
    for (b, n) in &fixed {
        for _ in 0..*n {
            buckets.push(*b);
        }
    }
    for _ in 0..remand_normal {
        buckets.push(Bucket::RemandNormal);
    }
    buckets.shuffle(rng);

    buckets
        .into_iter()
        .map(|bucket| {
            let sex = if rng.gen_bool(0.95) { Sex::Male } else { Sex::Female };
            let (surname, given) = pick_name(sex, rng);
            let intake_days_ago = intake_offset(bucket, rng);
            Spec {
                bucket,
                surname: surname.to_string(),
                given_names: given.to_string(),
                sex,
                age: weighted_age(rng),
                intake_days_ago,
                assign_housing: rng.gen_bool(0.85),
                has_legal_rep: rng.gen_bool(0.5),
            }
        })
        .collect()
}

fn intake_offset(bucket: Bucket, rng: &mut StdRng) -> u32 {
    match bucket {
        Bucket::NoLegalBasis => rng.gen_range(20..200),
        Bucket::PoliceCustodyExceeded => rng.gen_range(5..14),
        Bucket::PoliceCustodyOk => rng.gen_range(0..2),
        Bucket::RemandProlongedCritical => rng.gen_range(380..900),
        Bucket::RemandProlongedWarn => rng.gen_range(190..360),
        Bucket::RemandReviewOverdue => rng.gen_range(60..170),
        Bucket::RemandNormal => rng.gen_range(5..170),
        Bucket::OnTrial => rng.gen_range(40..400),
        Bucket::ConvictedUnsentenced => rng.gen_range(10..120),
        Bucket::SentencedPastRelease => rng.gen_range(400..1100),
        Bucket::SentencedImminent => rng.gen_range(50..200),
        Bucket::SentencedNormal => rng.gen_range(30..900),
        Bucket::Appeal => rng.gen_range(60..500),
    }
}

fn weighted_age(rng: &mut StdRng) -> u32 {
    // Weighted toward 22-35; tail to 62.
    let buckets = [(18..22, 1), (22..36, 6), (36..50, 3), (50..63, 1)];
    let total: u32 = buckets.iter().map(|(_, w)| *w as u32).sum();
    let mut roll: u32 = rng.gen_range(0..total);
    for (range, weight) in &buckets {
        if roll < *weight as u32 {
            return rng.gen_range(range.start..range.end);
        }
        roll -= *weight as u32;
    }
    30
}

// ===========================================================================
// Building domain values
// ===========================================================================

fn build_identity(spec: &Spec, _rng: &mut StdRng) -> Identity {
    let today = Utc::now().date_naive();
    let dob = today
        .checked_sub_signed(chrono::Duration::days(spec.age as i64 * 365 + 30))
        .unwrap_or(today);
    Identity {
        surname: spec.surname.clone(),
        given_names: spec.given_names.clone(),
        preferred_name: None,
        aliases: Vec::new(),
        date_of_birth: Some(dob),
        estimated_age_at_intake: None,
        estimated_age_date: None,
        sex: spec.sex,
        nationality: Some("Dominican".to_string()),
        national_id: None,
        languages: vec!["English".to_string()],
        photo_hash: None,
    }
}

fn build_basis(spec: &Spec, intake: PastDate, rng: &mut StdRng) -> DetentionBasis {
    let today = Utc::now().date_naive();
    match spec.bucket {
        Bucket::NoLegalBasis => DetentionBasis::NoLegalBasis {
            discovered_date: intake,
            circumstances: "Held without documented warrant; under records review.".to_string(),
        },
        Bucket::PoliceCustodyExceeded => DetentionBasis::PoliceCustody {
            arrest_date: intake,
            arresting_authority: pick(&POLICE_DIVISIONS, rng).to_string(),
            must_appear_by: today - chrono::Duration::days(rng.gen_range(2..5)),
            suspected_offences: Some(vec![sample_charge(rng)]),
        },
        Bucket::PoliceCustodyOk => DetentionBasis::PoliceCustody {
            arrest_date: intake,
            arresting_authority: pick(&POLICE_DIVISIONS, rng).to_string(),
            must_appear_by: today + chrono::Duration::days(1),
            suspected_offences: Some(vec![sample_charge(rng)]),
        },
        Bucket::RemandProlongedCritical | Bucket::RemandProlongedWarn | Bucket::RemandNormal => {
            DetentionBasis::RemandAwaitingTrial {
                first_appearance_date: intake,
                next_court_date: Some(today + chrono::Duration::days(rng.gen_range(15..120))),
                remand_review_due: Some(today + chrono::Duration::days(rng.gen_range(5..40))),
                bail_status: random_bail(intake, rng),
                charges: vec![sample_charge(rng)],
            }
        }
        Bucket::RemandReviewOverdue => DetentionBasis::RemandAwaitingTrial {
            first_appearance_date: intake,
            next_court_date: Some(today + chrono::Duration::days(rng.gen_range(20..90))),
            remand_review_due: Some(today - chrono::Duration::days(rng.gen_range(3..30))),
            bail_status: BailStatus::NotApplied,
            charges: vec![sample_charge(rng)],
        },
        Bucket::OnTrial => DetentionBasis::OnTrial {
            trial_start_date: past_date(spec.intake_days_ago / 2),
            next_court_date: Some(today + chrono::Duration::days(rng.gen_range(7..45))),
            bail_status: BailStatus::Denied {
                date: intake,
                reason: "Risk of flight".to_string(),
            },
            charges: vec![sample_charge(rng)],
        },
        Bucket::ConvictedUnsentenced => DetentionBasis::ConvictedUnsentenced {
            conviction_date: past_date(spec.intake_days_ago.min(30)),
            sentencing_date: Some(today + chrono::Duration::days(rng.gen_range(7..30))),
            bail_status: None,
            charges: vec![sample_charge(rng)],
        },
        Bucket::SentencedPastRelease => {
            // Past release date: bypass the public ReleaseDate constructor
            // (which clamps to today) by going through serde.
            sentenced_via_json(
                intake.as_naive(),
                rng.gen_range(60..180),
                today - chrono::Duration::days(rng.gen_range(2..20)),
                rng,
            )
        }
        Bucket::SentencedImminent => sentenced_via_json(
            intake.as_naive(),
            rng.gen_range(60..240),
            today + chrono::Duration::days(rng.gen_range(1..7)),
            rng,
        ),
        Bucket::SentencedNormal => sentenced_via_json(
            intake.as_naive(),
            rng.gen_range(180..1800),
            today + chrono::Duration::days(rng.gen_range(30..1500)),
            rng,
        ),
        Bucket::Appeal => sentenced_appeal_via_json(intake.as_naive(), rng),
    }
}

/// Build a Sentenced variant with an arbitrary release_date by routing
/// through serde. `compute_release_date` clamps past dates to today, which
/// would mask the ReleaseDatePassed flag; this round-trip bypasses that
/// clamp for demo purposes only.
fn sentenced_via_json(
    intake: NaiveDate,
    sentence_days: u32,
    release: NaiveDate,
    rng: &mut StdRng,
) -> DetentionBasis {
    let json = serde_json::json!({
        "Sentenced": {
            "sentence_date": intake,
            "sentence": { "days": sentence_days },
            "release_date": release,
            "credits": [],
            "adjustments": [],
            "charges": [serde_json::to_value(sample_charge(rng)).unwrap()],
        }
    });
    serde_json::from_value(json).expect("hand-built Sentenced JSON must deserialize")
}

fn sentenced_appeal_via_json(intake: NaiveDate, rng: &mut StdRng) -> DetentionBasis {
    let today = Utc::now().date_naive();
    let json = serde_json::json!({
        "SentencedOnAppeal": {
            "sentence": { "days": rng.gen_range(365..1800u32) },
            "provisional_release_date": today + chrono::Duration::days(rng.gen_range(60..900)),
            "appeal_filed_date": intake,
            "next_hearing_date": today + chrono::Duration::days(rng.gen_range(20..120)),
            "credits": [],
            "adjustments": [],
            "charges": [serde_json::to_value(sample_charge(rng)).unwrap()],
        }
    });
    serde_json::from_value(json).expect("hand-built SentencedOnAppeal JSON must deserialize")
}

fn random_bail(intake: PastDate, rng: &mut StdRng) -> BailStatus {
    match rng.gen_range(0..4) {
        0 => BailStatus::NotApplied,
        1 => BailStatus::Applied { date: intake },
        2 => BailStatus::Denied {
            date: intake,
            reason: "Risk of interference with witnesses".to_string(),
        },
        _ => BailStatus::Granted {
            date: intake,
            amount: Some(rng.gen_range(500..15000)),
            conditions: vec!["Surrender travel documents".to_string()],
        },
    }
}

fn sample_charge(rng: &mut StdRng) -> Charge {
    let (description, severity) = pick(&OFFENCES, rng);
    Charge {
        id: openward_core::identifiers::ChargeId::new(),
        description: description.to_string(),
        statute: Some(format!("Criminal Code Chap. 10:01 s.{}", rng.gen_range(20..240))),
        severity: *severity,
        date_of_alleged_offence: None,
        count_number: Some(1),
    }
}

// ===========================================================================
// DB lookups
// ===========================================================================

async fn fetch_admin_id(pool: &sqlx::SqlitePool) -> Result<OperatorId, sqlx::Error> {
    let row: (String,) = sqlx::query_as("SELECT id FROM operators WHERE username = 'admin'")
        .fetch_one(pool)
        .await?;
    let uuid = Uuid::parse_str(&row.0).expect("admin id is a valid uuid");
    Ok(OperatorId::from_uuid(uuid))
}

async fn fetch_default_housing(pool: &sqlx::SqlitePool) -> Result<HousingUnitId, sqlx::Error> {
    let row: (String,) =
        sqlx::query_as("SELECT id FROM housing_units ORDER BY rowid LIMIT 1")
            .fetch_one(pool)
            .await?;
    let uuid = Uuid::parse_str(&row.0).expect("housing id is a valid uuid");
    Ok(HousingUnitId::from_uuid(uuid))
}

async fn count_detainees(pool: &sqlx::SqlitePool) -> Result<i64, sqlx::Error> {
    let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM detainees")
        .fetch_one(pool)
        .await?;
    Ok(row.0)
}

// ===========================================================================
// Random helpers
// ===========================================================================

fn past_date(days_ago: u32) -> PastDate {
    let today = Utc::now().date_naive();
    let date = today
        .checked_sub_days(Days::new(days_ago as u64))
        .unwrap_or(today);
    PastDate::new(date).unwrap_or_else(|_| PastDate::from_trusted(date))
}

fn pick<'a, T>(slice: &'a [T], rng: &mut StdRng) -> &'a T {
    &slice[rng.gen_range(0..slice.len())]
}

fn pick_name(sex: Sex, rng: &mut StdRng) -> (&'static str, &'static str) {
    let surname = pick(SURNAMES, rng);
    let given = match sex {
        Sex::Male => pick(MALE_NAMES, rng),
        _ => pick(FEMALE_NAMES, rng),
    };
    (surname, given)
}

fn sample<T: Clone>(pool: &[T], k: usize, rng: &mut StdRng) -> Vec<T> {
    let mut copy: Vec<T> = pool.to_vec();
    copy.shuffle(rng);
    copy.into_iter().take(k.min(pool.len())).collect()
}

// ===========================================================================
// Name + reference data (Dominica-flavoured)
// ===========================================================================

const SURNAMES: &[&str] = &[
    "Joseph", "Charles", "Augustus", "Lawrence", "Williams", "Jno-Baptiste",
    "Defoe", "Lestrade", "Toussaint", "Casimir", "Etienne", "George",
    "Andrew", "Burton", "Faustin", "Magloire", "Pascal", "Theodore",
    "Pierre", "Riviere", "Roberts", "Simon", "Stedman", "Telemaque",
    "Vidal", "Walsh", "Xavier", "Bruney", "Carbon", "Daniel",
    "Edwards", "Felix", "Gachette", "Henderson", "Isaac", "Jeffers",
    "Lebrun", "Mason", "Nicholas", "Olivacce", "Phillip", "Quashie",
];

const MALE_NAMES: &[&str] = &[
    "Alex", "Brandon", "Carlos", "Damien", "Elroy", "Fabian", "Glen",
    "Hartley", "Ignatius", "Jamal", "Kemar", "Leighton", "Marcus",
    "Nigel", "Orville", "Patrick", "Quincy", "Roland", "Sherman",
    "Terrence", "Uriel", "Vernon", "Wendell", "Xavier", "Yannick", "Zane",
    "Andre", "Benjamin", "Curtis", "Devon",
];

const FEMALE_NAMES: &[&str] = &[
    "Adriana", "Brianna", "Celeste", "Danielle", "Estelle", "Francine",
    "Giselle", "Helene", "Imani", "Jacinta", "Kadijah", "Latoya",
    "Marcia", "Nadine", "Ophelia", "Patricia", "Renee", "Sasha",
    "Tanya", "Ursuline", "Valerie", "Yvette",
];

const COURTS: &[&str] = &[
    "Roseau Magistrate's Court",
    "Portsmouth Magistrate's Court",
    "Eastern Magistrate's Court (Marigot)",
    "High Court of Justice — Roseau",
];

const POLICE_DIVISIONS: &[&str] = &[
    "Roseau Police Headquarters",
    "Portsmouth Division",
    "Marigot Division",
    "Grand Bay Division",
];

const LEGAL_REPS: &[&str] = &[
    "Public Solicitors Office",
    "Legal Aid Clinic (UWI)",
    "Counsel: M. Joseph",
    "Counsel: R. Williams",
    "Counsel: L. Charles",
];

const OFFENCES: &[(&str, ChargeSeverity)] = &[
    ("Possession of controlled drugs", ChargeSeverity::Moderate),
    ("Larceny from a dwelling", ChargeSeverity::Moderate),
    ("Burglary", ChargeSeverity::Serious),
    ("Wounding with intent", ChargeSeverity::Serious),
    ("Robbery", ChargeSeverity::Serious),
    ("Assault occasioning bodily harm", ChargeSeverity::Moderate),
    ("Indecent assault", ChargeSeverity::Serious),
    ("Possession of firearm without licence", ChargeSeverity::Serious),
    ("Disorderly conduct", ChargeSeverity::Minor),
    ("Driving under the influence", ChargeSeverity::Minor),
    ("Malicious damage to property", ChargeSeverity::Minor),
    ("Fraud", ChargeSeverity::Moderate),
];
