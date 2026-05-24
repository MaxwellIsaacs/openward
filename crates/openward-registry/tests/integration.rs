use chrono::{NaiveDate, Utc};

use openward_core::*;
use openward_db::{apply_schema, create_pool};
use openward_registry::{FacilityConfig, SqliteRegistry};

// === Test helpers ===

async fn setup() -> SqliteRegistry {
    let pool = create_pool("sqlite::memory:").await.unwrap();
    apply_schema(&pool).await.unwrap();
    SqliteRegistry::new(pool, FacilityConfig::default())
}

fn operator() -> OperatorId {
    OperatorId::new()
}

fn today() -> PastDate {
    PastDate::from_trusted(Utc::now().date_naive())
}

fn past_date(y: i32, m: u32, d: u32) -> PastDate {
    PastDate::from_trusted(NaiveDate::from_ymd_opt(y, m, d).unwrap())
}

fn make_identity(surname: &str, given: &str, sex: Sex) -> Identity {
    Identity {
        surname: surname.to_string(),
        given_names: given.to_string(),
        preferred_name: None,
        aliases: Vec::new(),
        date_of_birth: Some(NaiveDate::from_ymd_opt(1990, 6, 15).unwrap()),
        estimated_age_at_intake: None,
        estimated_age_date: None,
        sex,
        nationality: Some("Malawian".to_string()),
        national_id: None,
        languages: vec!["Chichewa".to_string()],
        photo_hash: None,
    }
}

fn police_custody_basis() -> DetentionBasis {
    let tomorrow = Utc::now().date_naive() + chrono::Duration::days(1);
    DetentionBasis::PoliceCustody {
        arrest_date: today(),
        arresting_authority: "Lilongwe Police".to_string(),
        must_appear_by: tomorrow,
        suspected_offences: None,
    }
}

fn remand_basis() -> DetentionBasis {
    let next_court = Utc::now().date_naive() + chrono::Duration::days(30);
    DetentionBasis::RemandAwaitingTrial {
        first_appearance_date: today(),
        next_court_date: Some(next_court),
        remand_review_due: Some(Utc::now().date_naive() + chrono::Duration::days(90)),
        bail_status: BailStatus::NotApplied,
        charges: vec![Charge {
            id: ChargeId::new(),
            description: "Theft".to_string(),
            statute: Some("Section 278".to_string()),
            severity: ChargeSeverity::Moderate,
            date_of_alleged_offence: Some(NaiveDate::from_ymd_opt(2025, 1, 1).unwrap()),
            count_number: Some(1),
        }],
    }
}

fn simple_admission(basis: DetentionBasis) -> AdmissionRecord {
    AdmissionRecord {
        identity: make_identity("Banda", "John", Sex::Male),
        detention_basis: basis,
        intake_date: today(),
        warrant: Some(CommitmentOrder {
            id: WarrantId::new(),
            detainee_id: DetaineeId::new(), // placeholder
            order_type: CommitmentOrderType::PoliceHolding,
            external_reference: Some("POL/2025/001".to_string()),
            issuing_authority: "Lilongwe Magistrate Court".to_string(),
            issuing_officer: Some("Magistrate Phiri".to_string()),
            date_issued: today(),
            date_received: today(),
            valid_until: Some(Utc::now().date_naive() + chrono::Duration::days(14)),
            offence_description: Some("Suspected theft".to_string()),
            sentence_details: None,
            is_active: true,
            registered_by: operator(),
            batch_id: None,
            document_hash: None,
        }),
        intake_medical_notes: Some("No visible injuries".to_string()),
        legal_reference: None,
        emergency_contacts: vec![EmergencyContact {
            name: "Mary Banda".to_string(),
            relationship: "Mother".to_string(),
            phone: Some("+265 999 123456".to_string()),
            address: None,
        }],
        property: vec![PropertyItem {
            id: 0,
            description: "Mobile phone".to_string(),
            quantity: 1,
            logged_date: today(),
            logged_by: operator(),
            returned: false,
        }],
        legal_representation: None,
        transfer_from: None,
        notes: Some("Cooperative during intake".to_string()),
        admitted_by: operator(),
    }
}

// ===========================================================================
// Test 1: Full lifecycle
// ===========================================================================

#[tokio::test]
async fn test_full_lifecycle_admit_through_release() {
    let reg = setup().await;
    let op = operator();

    // 1. Admit with PoliceCustody basis
    let detainee = reg.admit(simple_admission(police_custody_basis())).await.unwrap();
    assert_eq!(detainee.identity.surname, "Banda");
    assert_eq!(detainee.identity.given_names, "John");
    assert!(matches!(detainee.facility_status, FacilityStatus::Present));
    assert!(matches!(
        detainee.detention_basis,
        DetentionBasis::PoliceCustody { .. }
    ));
    assert_eq!(detainee.warrants.len(), 1);
    assert!(detainee.active_warrant().is_some());
    assert_eq!(detainee.property.len(), 1);
    assert!(detainee.notes.len() >= 1); // intake + medical notes

    let id = detainee.id;

    // 2. Transition PoliceCustody → Remand
    let updated = reg
        .update_detention_basis(id, remand_basis(), op)
        .await
        .unwrap();
    assert!(matches!(
        updated.detention_basis,
        DetentionBasis::RemandAwaitingTrial { .. }
    ));

    // 3. Transition Remand → OnTrial
    let on_trial = DetentionBasis::OnTrial {
        trial_start_date: today(),
        next_court_date: Some(Utc::now().date_naive() + chrono::Duration::days(14)),
        bail_status: BailStatus::Denied {
            date: today(),
            reason: "Flight risk".to_string(),
        },
        charges: vec![Charge {
            id: ChargeId::new(),
            description: "Theft".to_string(),
            statute: Some("Section 278".to_string()),
            severity: ChargeSeverity::Moderate,
            date_of_alleged_offence: Some(NaiveDate::from_ymd_opt(2025, 1, 1).unwrap()),
            count_number: Some(1),
        }],
    };
    let updated = reg
        .update_detention_basis(id, on_trial, op)
        .await
        .unwrap();
    assert!(matches!(
        updated.detention_basis,
        DetentionBasis::OnTrial { .. }
    ));

    // 4. Transition OnTrial → Sentenced
    let sentenced = DetentionBasis::Sentenced {
        sentence_date: today(),
        sentence: SentenceDuration::from_days(365).unwrap(),
        release_date: compute_release_date(
            today(),
            SentenceDuration::from_days(365).unwrap(),
            &[],
            &[],
        ),
        credits: Vec::new(),
        adjustments: Vec::new(),
        charges: vec![Charge {
            id: ChargeId::new(),
            description: "Theft".to_string(),
            statute: Some("Section 278".to_string()),
            severity: ChargeSeverity::Moderate,
            date_of_alleged_offence: Some(NaiveDate::from_ymd_opt(2025, 1, 1).unwrap()),
            count_number: Some(1),
        }],
    };
    let updated = reg
        .update_detention_basis(id, sentenced, op)
        .await
        .unwrap();
    assert!(matches!(
        updated.detention_basis,
        DetentionBasis::Sentenced { .. }
    ));

    // 5. Release
    let release = ReleaseRecord {
        detainee_id: id,
        release_date: today(),
        release_type: ReleaseType::SentenceExpired,
        authorized_by: op,
        all_property_returned: true,
        notes: Some("Sentence served".to_string()),
    };
    let released = reg.release(release).await.unwrap();
    assert_eq!(released.facility_status, FacilityStatus::Released);
    // Warrant should be deactivated
    assert!(released.active_warrant().is_none());
}

// ===========================================================================
// Test 2: Batch admission
// ===========================================================================

#[tokio::test]
async fn test_batch_admission() {
    let reg = setup().await;
    let op = operator();
    let batch_id = BatchId::new();

    let batch = BatchAdmission {
        batch_id,
        shared_warrant: SharedWarrantData {
            order_type: CommitmentOrderType::RemandOrder,
            external_reference: Some("REM/2025/100".to_string()),
            issuing_authority: "High Court".to_string(),
            issuing_officer: Some("Justice Mwale".to_string()),
            date_issued: today(),
            date_received: today(),
            valid_until: Some(Utc::now().date_naive() + chrono::Duration::days(30)),
            offence_description: Some("Armed robbery".to_string()),
        },
        detainees: vec![
            BatchDetaineeEntry {
                identity: make_identity("Phiri", "James", Sex::Male),
                detention_basis: remand_basis(),
                additional_charges: Vec::new(),
                intake_medical_notes: None,
                legal_reference: None,
                emergency_contacts: Vec::new(),
                property: Vec::new(),
                legal_representation: None,
                notes: None,
            },
            BatchDetaineeEntry {
                identity: make_identity("Mwanza", "Peter", Sex::Male),
                detention_basis: remand_basis(),
                additional_charges: Vec::new(),
                intake_medical_notes: None,
                legal_reference: None,
                emergency_contacts: Vec::new(),
                property: Vec::new(),
                legal_representation: None,
                notes: None,
            },
            BatchDetaineeEntry {
                identity: make_identity("Nkhoma", "Gift", Sex::Male),
                detention_basis: remand_basis(),
                additional_charges: Vec::new(),
                intake_medical_notes: None,
                legal_reference: None,
                emergency_contacts: Vec::new(),
                property: Vec::new(),
                legal_representation: None,
                notes: None,
            },
        ],
        intake_date: today(),
        admitted_by: op,
    };

    let detainees = reg.batch_admit(batch).await.unwrap();
    assert_eq!(detainees.len(), 3);

    // Verify all detainees have warrants linked to the batch
    for d in &detainees {
        assert_eq!(d.warrants.len(), 1);
        assert!(d.active_warrant().is_some());
        let warrant = d.active_warrant().unwrap();
        assert!(warrant.batch_id.is_some());
        assert_eq!(warrant.batch_id.unwrap(), batch_id);
        assert!(matches!(warrant.order_type, CommitmentOrderType::RemandOrder));
    }

    // Verify they're distinct
    let surnames: Vec<&str> = detainees.iter().map(|d| d.identity.surname.as_str()).collect();
    assert!(surnames.contains(&"Phiri"));
    assert!(surnames.contains(&"Mwanza"));
    assert!(surnames.contains(&"Nkhoma"));
}

// ===========================================================================
// Test 3: Invalid basis transitions
// ===========================================================================

#[tokio::test]
async fn test_invalid_basis_transition_rejected() {
    let reg = setup().await;
    let op = operator();

    // Admit with PoliceCustody
    let detainee = reg.admit(simple_admission(police_custody_basis())).await.unwrap();
    let id = detainee.id;

    // PoliceCustody → Sentenced should fail (must go through Remand or OnTrial first)
    let sentenced = DetentionBasis::Sentenced {
        sentence_date: today(),
        sentence: SentenceDuration::from_days(365).unwrap(),
        release_date: compute_release_date(
            today(),
            SentenceDuration::from_days(365).unwrap(),
            &[],
            &[],
        ),
        credits: Vec::new(),
        adjustments: Vec::new(),
        charges: vec![Charge {
            id: ChargeId::new(),
            description: "Theft".to_string(),
            statute: None,
            severity: ChargeSeverity::Moderate,
            date_of_alleged_offence: None,
            count_number: None,
        }],
    };

    let result = reg.update_detention_basis(id, sentenced, op).await;
    assert!(result.is_err());
    match result.unwrap_err() {
        RegistryError::Domain(DomainError::InvalidBasisTransition { from, to }) => {
            assert_eq!(from, DetentionBasisLabel::PoliceCustody);
            assert_eq!(to, DetentionBasisLabel::Sentenced);
        }
        other => panic!("Expected InvalidBasisTransition, got: {:?}", other),
    }
}

// ===========================================================================
// Test 4: Release of already-released detainee
// ===========================================================================

#[tokio::test]
async fn test_cannot_release_already_released() {
    let reg = setup().await;
    let op = operator();

    let detainee = reg.admit(simple_admission(police_custody_basis())).await.unwrap();
    let id = detainee.id;

    // First release succeeds
    let release = ReleaseRecord {
        detainee_id: id,
        release_date: today(),
        release_type: ReleaseType::CourtOrdered,
        authorized_by: op,
        all_property_returned: true,
        notes: None,
    };
    reg.release(release).await.unwrap();

    // Second release should fail
    let release2 = ReleaseRecord {
        detainee_id: id,
        release_date: today(),
        release_type: ReleaseType::CourtOrdered,
        authorized_by: op,
        all_property_returned: true,
        notes: None,
    };
    let result = reg.release(release2).await;
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        RegistryError::Domain(DomainError::InactiveDetainee { .. })
    ));
}

// ===========================================================================
// Test 5: Get detainee round-trip
// ===========================================================================

#[tokio::test]
async fn test_get_detainee_roundtrip() {
    let reg = setup().await;

    let detainee = reg.admit(simple_admission(police_custody_basis())).await.unwrap();
    let fetched = reg.get_detainee(detainee.id).await.unwrap();

    assert_eq!(fetched.id, detainee.id);
    assert_eq!(fetched.identity.surname, "Banda");
    assert_eq!(fetched.identity.given_names, "John");
    assert!(matches!(fetched.identity.sex, Sex::Male));
    assert_eq!(fetched.property.len(), 1);
    assert_eq!(fetched.property[0].description, "Mobile phone");
    assert_eq!(fetched.emergency_contacts.len(), 1);
    assert_eq!(fetched.emergency_contacts[0].name, "Mary Banda");
}

// ===========================================================================
// Test 6: Warrant management
// ===========================================================================

#[tokio::test]
async fn test_warrant_registration_deactivates_previous() {
    let reg = setup().await;
    let op = operator();

    let detainee = reg.admit(simple_admission(police_custody_basis())).await.unwrap();
    let id = detainee.id;
    let original_warrant_id = detainee.active_warrant().unwrap().id;

    // Register a new warrant — should deactivate the old one
    let new_warrant = CommitmentOrder {
        id: WarrantId::new(),
        detainee_id: id,
        order_type: CommitmentOrderType::RemandOrder,
        external_reference: Some("REM/2025/002".to_string()),
        issuing_authority: "Lilongwe Magistrate Court".to_string(),
        issuing_officer: None,
        date_issued: today(),
        date_received: today(),
        valid_until: Some(Utc::now().date_naive() + chrono::Duration::days(30)),
        offence_description: None,
        sentence_details: None,
        is_active: true,
        registered_by: op,
        batch_id: None,
        document_hash: None,
    };

    let updated = reg.register_warrant(new_warrant.clone(), op).await.unwrap();

    // Should have 2 warrants total, 1 active
    assert_eq!(updated.warrants.len(), 2);
    let active_count = updated.warrants.iter().filter(|w| w.is_active).count();
    assert_eq!(active_count, 1);

    // The active one should be the new one
    let active = updated.active_warrant().unwrap();
    assert_ne!(active.id, original_warrant_id);
    assert!(matches!(active.order_type, CommitmentOrderType::RemandOrder));
}

// ===========================================================================
// Test 7: Court date scheduling and outcome recording
// ===========================================================================

#[tokio::test]
async fn test_court_date_schedule_and_record_outcome() {
    let reg = setup().await;
    let op = operator();

    let detainee = reg.admit(simple_admission(remand_basis())).await.unwrap();
    let id = detainee.id;

    let court_date = CourtDate {
        id: CourtDateId::new(),
        detainee_id: id,
        scheduled_date: Utc::now().date_naive() + chrono::Duration::days(7),
        court_name: "Lilongwe Magistrate Court".to_string(),
        purpose: CourtPurpose::RemandReview,
        outcome: None,
    };

    let scheduled = reg.schedule_court_date(court_date.clone(), op).await.unwrap();
    assert_eq!(scheduled.detainee_id, id);
    assert!(scheduled.outcome.is_none());

    // Record outcome
    let outcome = CourtOutcome::RemandContinued {
        review_date: Some(Utc::now().date_naive() + chrono::Duration::days(90)),
    };
    let with_outcome = reg
        .record_court_outcome(court_date.id, outcome, op)
        .await
        .unwrap();
    assert!(with_outcome.outcome.is_some());
    assert!(matches!(
        with_outcome.outcome,
        Some(CourtOutcome::RemandContinued { .. })
    ));
}

// ===========================================================================
// Test 8: Facility status update
// ===========================================================================

#[tokio::test]
async fn test_facility_status_update() {
    let reg = setup().await;
    let op = operator();

    let detainee = reg.admit(simple_admission(police_custody_basis())).await.unwrap();
    let id = detainee.id;

    // Move to InCourt
    let updated = reg
        .update_facility_status(id, FacilityStatus::InCourt, op, Some("Court appearance".to_string()))
        .await
        .unwrap();
    assert_eq!(updated.facility_status, FacilityStatus::InCourt);

    // Move back to Present
    let returned = reg
        .update_facility_status(id, FacilityStatus::Present, op, Some("Returned from court".to_string()))
        .await
        .unwrap();
    assert_eq!(returned.facility_status, FacilityStatus::Present);
    // Should have intake notes + status change notes
    assert!(returned.notes.len() >= 2);
}

// ===========================================================================
// Test 9: Search / query
// ===========================================================================

#[tokio::test]
async fn test_search_with_filters() {
    let reg = setup().await;
    let op = operator();

    // Admit several detainees with different bases
    let mut admission1 = simple_admission(police_custody_basis());
    admission1.identity = make_identity("Alpha", "One", Sex::Male);
    let _d1 = reg.admit(admission1).await.unwrap();

    let mut admission2 = simple_admission(remand_basis());
    admission2.identity = make_identity("Beta", "Two", Sex::Female);
    let _d2 = reg.admit(admission2).await.unwrap();

    let mut admission3 = simple_admission(remand_basis());
    admission3.identity = make_identity("Gamma", "Three", Sex::Male);
    let _d3 = reg.admit(admission3).await.unwrap();

    // Search all
    let result = reg
        .search(PopulationQuery::default(), op)
        .await
        .unwrap();
    assert_eq!(result.total_matching, 3);
    assert_eq!(result.detainees.len(), 3);

    // Search by detention basis = Remand
    let query = PopulationQuery {
        detention_basis: Some(DetentionBasisFilter::RemandAwaitingTrial),
        ..Default::default()
    };
    let result = reg.search(query, op).await.unwrap();
    assert_eq!(result.total_matching, 2);

    // Search by sex = Female
    let query = PopulationQuery {
        sex: Some(Sex::Female),
        ..Default::default()
    };
    let result = reg.search(query, op).await.unwrap();
    assert_eq!(result.total_matching, 1);
    assert_eq!(result.detainees[0].name, "Beta, Two");

    // Search by detention basis = PoliceCustody
    let query = PopulationQuery {
        detention_basis: Some(DetentionBasisFilter::PoliceCustody),
        ..Default::default()
    };
    let result = reg.search(query, op).await.unwrap();
    assert_eq!(result.total_matching, 1);
    assert_eq!(result.detainees[0].name, "Alpha, One");
}

// ===========================================================================
// Test 10: Search with pagination
// ===========================================================================

#[tokio::test]
async fn test_search_pagination() {
    let reg = setup().await;
    let op = operator();

    // Admit 5 detainees
    for i in 0..5 {
        let mut admission = simple_admission(remand_basis());
        admission.identity = make_identity(&format!("Name{}", i), &format!("Given{}", i), Sex::Male);
        reg.admit(admission).await.unwrap();
    }

    // Page 1: limit 2
    let query = PopulationQuery {
        limit: Some(2),
        offset: Some(0),
        ..Default::default()
    };
    let result = reg.search(query, op).await.unwrap();
    assert_eq!(result.total_matching, 5); // Total is still 5
    assert_eq!(result.detainees.len(), 2); // But only 2 returned

    // Page 2: limit 2, offset 2
    let query = PopulationQuery {
        limit: Some(2),
        offset: Some(2),
        ..Default::default()
    };
    let result = reg.search(query, op).await.unwrap();
    assert_eq!(result.detainees.len(), 2);

    // Page 3: limit 2, offset 4
    let query = PopulationQuery {
        limit: Some(2),
        offset: Some(4),
        ..Default::default()
    };
    let result = reg.search(query, op).await.unwrap();
    assert_eq!(result.detainees.len(), 1); // Only 1 remaining
}

// ===========================================================================
// Test 11: Overview
// ===========================================================================

#[tokio::test]
async fn test_facility_overview() {
    let reg = setup().await;

    // Admit 3 detainees: 2 remand, 1 police custody
    let mut a1 = simple_admission(remand_basis());
    a1.identity = make_identity("A", "1", Sex::Male);
    reg.admit(a1).await.unwrap();

    let mut a2 = simple_admission(remand_basis());
    a2.identity = make_identity("B", "2", Sex::Female);
    reg.admit(a2).await.unwrap();

    let mut a3 = simple_admission(police_custody_basis());
    a3.identity = make_identity("C", "3", Sex::Male);
    reg.admit(a3).await.unwrap();

    let overview = reg.overview().await.unwrap();
    assert_eq!(overview.total_population, 3);
    assert_eq!(overview.facility_capacity, 500); // default config
    assert!(overview.occupancy_percent > 0.0);
    assert_eq!(overview.basis_breakdown.remand, 2);
    assert_eq!(overview.basis_breakdown.police_custody, 1);
    // All are pretrial
    assert!(overview.pretrial_percent > 99.0);
}

// ===========================================================================
// Test 12: Flag computation — NoLegalBasis
// ===========================================================================

#[tokio::test]
async fn test_flag_no_legal_basis() {
    let reg = setup().await;

    let basis = DetentionBasis::NoLegalBasis {
        discovered_date: today(),
        circumstances: "Found in facility without warrant".to_string(),
    };
    let mut admission = simple_admission(basis);
    admission.warrant = None; // No warrant for NoLegalBasis
    admission.legal_representation = None;
    let detainee = reg.admit(admission).await.unwrap();

    let config = FacilityConfig::default();
    let flags = openward_registry::flags::compute_flags(&detainee, &config, &[]);

    assert!(flags.iter().any(|f| matches!(f, Flag::NoLegalBasis { .. })));
    assert!(flags.iter().any(|f| matches!(f, Flag::NoLegalRepresentation)));
    // NoActiveWarrant is NOT raised for NoLegalBasis detainees — by design,
    // since they inherently have no warrant and the flag is for detainees
    // who *should* have one.
    assert!(!flags.iter().any(|f| matches!(f, Flag::NoActiveWarrant)));
}

// ===========================================================================
// Test 13: Flag computation — CustodyLimitExceeded
// ===========================================================================

#[tokio::test]
async fn test_flag_custody_limit_exceeded() {
    let reg = setup().await;

    // Create a police custody with must_appear_by in the past
    let yesterday = Utc::now().date_naive() - chrono::Duration::days(1);
    let basis = DetentionBasis::PoliceCustody {
        arrest_date: past_date(2025, 1, 1),
        arresting_authority: "Police".to_string(),
        must_appear_by: yesterday,
        suspected_offences: None,
    };
    let mut admission = simple_admission(basis);
    admission.intake_date = past_date(2025, 1, 1);
    let detainee = reg.admit(admission).await.unwrap();

    let config = FacilityConfig::default();
    let flags = openward_registry::flags::compute_flags(&detainee, &config, &[]);

    assert!(flags.iter().any(|f| matches!(f, Flag::CustodyLimitExceeded { .. })));
}

// ===========================================================================
// Test 14: Flag computation — NoCourtDate
// ===========================================================================

#[tokio::test]
async fn test_flag_no_court_date() {
    let reg = setup().await;

    // Remand with no next_court_date
    let basis = DetentionBasis::RemandAwaitingTrial {
        first_appearance_date: today(),
        next_court_date: None,
        remand_review_due: None,
        bail_status: BailStatus::NotApplied,
        charges: vec![Charge {
            id: ChargeId::new(),
            description: "Theft".to_string(),
            statute: None,
            severity: ChargeSeverity::Minor,
            date_of_alleged_offence: None,
            count_number: None,
        }],
    };

    let admission = simple_admission(basis);
    let detainee = reg.admit(admission).await.unwrap();

    let config = FacilityConfig::default();
    let flags = openward_registry::flags::compute_flags(&detainee, &config, &[]);

    assert!(flags.iter().any(|f| matches!(f, Flag::NoCourtDate { .. })));
}

// ===========================================================================
// Test 15: Flag computation — BailGrantedStillHeld
// ===========================================================================

#[tokio::test]
async fn test_flag_bail_granted_still_held() {
    let reg = setup().await;

    let basis = DetentionBasis::RemandAwaitingTrial {
        first_appearance_date: today(),
        next_court_date: Some(Utc::now().date_naive() + chrono::Duration::days(30)),
        remand_review_due: None,
        bail_status: BailStatus::Granted {
            date: today(),
            amount: Some(50_000),
            conditions: vec!["Report weekly".to_string()],
        },
        charges: vec![Charge {
            id: ChargeId::new(),
            description: "Assault".to_string(),
            statute: None,
            severity: ChargeSeverity::Moderate,
            date_of_alleged_offence: None,
            count_number: None,
        }],
    };

    let admission = simple_admission(basis);
    let detainee = reg.admit(admission).await.unwrap();

    let config = FacilityConfig::default();
    let flags = openward_registry::flags::compute_flags(&detainee, &config, &[]);

    assert!(flags
        .iter()
        .any(|f| matches!(f, Flag::BailGrantedStillHeld { .. })));
}

// ===========================================================================
// Test 16: Housing assignment
// ===========================================================================

#[tokio::test]
async fn test_assign_housing() {
    let reg = setup().await;
    let op = operator();

    // Insert a housing unit into DB manually
    let unit_id = HousingUnitId::new();
    let unit_str = unit_id.as_uuid().to_string();
    sqlx::query(
        "INSERT INTO housing_units (id, name, capacity, unit_type, designated_sex) VALUES (?, ?, ?, ?, ?)"
    )
    .bind(&unit_str)
    .bind("Block A")
    .bind(50i32)
    .bind("General")
    .bind("Male")
    .execute(reg.pool())
    .await
    .unwrap();

    let detainee = reg.admit(simple_admission(police_custody_basis())).await.unwrap();
    let id = detainee.id;
    assert!(detainee.housing_unit.is_none());

    let updated = reg.assign_housing(id, unit_id, op).await.unwrap();
    assert_eq!(updated.housing_unit, Some(unit_id));
}

// ===========================================================================
// Test 17: Audit trail exists after mutations
// ===========================================================================

#[tokio::test]
async fn test_audit_entries_created() {
    let reg = setup().await;
    let op = operator();

    // Admit
    let detainee = reg.admit(simple_admission(police_custody_basis())).await.unwrap();
    let id = detainee.id;

    // Update basis
    reg.update_detention_basis(id, remand_basis(), op).await.unwrap();

    // Release
    let release = ReleaseRecord {
        detainee_id: id,
        release_date: today(),
        release_type: ReleaseType::CourtOrdered,
        authorized_by: op,
        all_property_returned: true,
        notes: None,
    };
    reg.release(release).await.unwrap();

    // Check audit trail — should have at least 3 entries (admit, update_basis, release)
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM audit_entries")
        .fetch_one(reg.pool())
        .await
        .unwrap();
    assert!(count.0 >= 3, "Expected at least 3 audit entries, got {}", count.0);

    // Check that audit entries have valid self_hashes
    let entries: Vec<(String, String, String, String, String, Option<String>, Option<String>, Option<String>, Vec<u8>, Vec<u8>, i64)> = sqlx::query_as(
        "SELECT id, timestamp, operator, module, action, target, before_data, after_data, self_hash, chain_hash, epoch FROM audit_entries ORDER BY rowid"
    )
    .fetch_all(reg.pool())
    .await
    .unwrap();

    for entry_row in &entries {
        assert!(!entry_row.8.is_empty(), "self_hash should not be empty");
        assert!(!entry_row.9.is_empty(), "chain_hash should not be empty");
        assert_eq!(entry_row.8.len(), 32, "self_hash should be 32 bytes");
        assert_eq!(entry_row.9.len(), 32, "chain_hash should be 32 bytes");
    }

    // Verify the target field matches our detainee
    let admit_entry = entries.iter().find(|e| e.4 == "admit").unwrap();
    assert_eq!(
        admit_entry.5.as_deref(),
        Some(id.to_string().as_str()),
        "Audit target should match detainee ID"
    );
}

// ===========================================================================
// Test 18: Daily count lifecycle
// ===========================================================================

#[tokio::test]
async fn test_daily_count_lifecycle() {
    let reg = setup().await;
    let op = operator();

    // Insert a housing unit
    let unit_id = HousingUnitId::new();
    sqlx::query(
        "INSERT INTO housing_units (id, name, capacity, unit_type) VALUES (?, ?, ?, ?)"
    )
    .bind(unit_id.as_uuid().to_string())
    .bind("Block A")
    .bind(50i32)
    .bind("General")
    .execute(reg.pool())
    .await
    .unwrap();

    let today_date = Utc::now().date_naive();

    // Open daily count
    let count = reg.open_daily_count(today_date, op).await.unwrap();
    assert_eq!(count.date, today_date);
    assert!(count.finalized_by.is_none());

    // Submit unit headcount
    let headcount = UnitHeadcount {
        unit_id,
        count: 25,
        counted_by: op,
        counted_at: Utc::now(),
        received_at: Utc::now(),
        was_offline: false,
    };
    let updated = reg.submit_unit_headcount(today_date, headcount).await.unwrap();
    assert_eq!(updated.actual_closing_count, Some(25));

    // Finalize
    let finalized = reg.finalize_daily_count(today_date, op).await.unwrap();
    assert!(finalized.finalized_by.is_some());
    assert!(finalized.finalized_at.is_some());
}

// ===========================================================================
// Test 19: Search with facility status filter
// ===========================================================================

#[tokio::test]
async fn test_search_by_facility_status() {
    let reg = setup().await;
    let op = operator();

    // Admit 2 detainees
    let mut a1 = simple_admission(police_custody_basis());
    a1.identity = make_identity("Active", "One", Sex::Male);
    let _d1 = reg.admit(a1).await.unwrap();

    let mut a2 = simple_admission(police_custody_basis());
    a2.identity = make_identity("Released", "Two", Sex::Female);
    let d2 = reg.admit(a2).await.unwrap();

    // Release d2
    reg.release(ReleaseRecord {
        detainee_id: d2.id,
        release_date: today(),
        release_type: ReleaseType::CourtOrdered,
        authorized_by: op,
        all_property_returned: true,
        notes: None,
    })
    .await
    .unwrap();

    // Search for Present only
    let query = PopulationQuery {
        facility_status: Some(FacilityStatus::Present),
        ..Default::default()
    };
    let result = reg.search(query, op).await.unwrap();
    assert_eq!(result.total_matching, 1);
    assert_eq!(result.detainees[0].name, "Active, One");
}

// ===========================================================================
// Test 20: Detainee not found
// ===========================================================================

#[tokio::test]
async fn test_detainee_not_found() {
    let reg = setup().await;

    let fake_id = DetaineeId::new();
    let result = reg.get_detainee(fake_id).await;
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        RegistryError::Domain(DomainError::DetaineeNotFound { .. })
    ));
}

// ===========================================================================
// Test 21: Admission without a warrant
// ===========================================================================

#[tokio::test]
async fn test_admit_without_warrant() {
    let reg = setup().await;

    let mut admission = simple_admission(police_custody_basis());
    admission.warrant = None;
    let detainee = reg.admit(admission).await.unwrap();

    assert!(detainee.warrants.is_empty());
    assert!(detainee.active_warrant().is_none());
    assert_eq!(detainee.facility_status, FacilityStatus::Present);
}

// ===========================================================================
// Test 22: Admission with aliases round-trips correctly
// ===========================================================================

#[tokio::test]
async fn test_admit_with_aliases() {
    let reg = setup().await;

    let mut admission = simple_admission(police_custody_basis());
    admission.identity.aliases = vec![
        "JB".to_string(),
        "Johnny B".to_string(),
        "The Bandster".to_string(),
    ];
    let detainee = reg.admit(admission).await.unwrap();

    let fetched = reg.get_detainee(detainee.id).await.unwrap();
    assert_eq!(fetched.identity.aliases.len(), 3);
    assert!(fetched.identity.aliases.contains(&"JB".to_string()));
    assert!(fetched.identity.aliases.contains(&"Johnny B".to_string()));
    assert!(fetched.identity.aliases.contains(&"The Bandster".to_string()));
}

// ===========================================================================
// Test 23: Admission with all optional fields populated
// ===========================================================================

#[tokio::test]
async fn test_admit_with_all_optional_fields() {
    let reg = setup().await;

    let mut admission = simple_admission(police_custody_basis());
    admission.identity.preferred_name = Some("Johnny".to_string());
    admission.identity.national_id = Some("MW-12345678".to_string());
    admission.identity.photo_hash = Some("abc123hash".to_string());
    admission.identity.estimated_age_at_intake = Some(30);
    admission.identity.estimated_age_date = Some(today());
    admission.legal_reference = Some(LegalReference {
        case_number: "CASE/2025/001".to_string(),
        court: "High Court Lilongwe".to_string(),
        judge: Some("Justice Nyirenda".to_string()),
    });
    admission.legal_representation = Some(LegalRepresentation {
        representative_name: "Advocate Chirwa".to_string(),
        representative_type: "Public Defender".to_string(),
        contact: Some("+265 888 555 000".to_string()),
        assigned_date: today(),
    });
    admission.notes = Some("Intake note".to_string());
    admission.intake_medical_notes = Some("Blood pressure normal".to_string());
    admission.transfer_from = None;

    let detainee = reg.admit(admission).await.unwrap();
    let fetched = reg.get_detainee(detainee.id).await.unwrap();

    assert_eq!(fetched.identity.preferred_name.as_deref(), Some("Johnny"));
    assert_eq!(fetched.identity.national_id.as_deref(), Some("MW-12345678"));
    assert!(fetched.legal_reference.is_some());
    assert_eq!(fetched.legal_reference.as_ref().unwrap().case_number, "CASE/2025/001");
    assert!(fetched.legal_representation.is_some());
    assert_eq!(
        fetched.legal_representation.as_ref().unwrap().representative_name,
        "Advocate Chirwa"
    );
    // Should have both intake note and medical note
    assert!(fetched.notes.len() >= 2);
    let note_texts: Vec<&str> = fetched.notes.iter().map(|n| n.content.as_str()).collect();
    assert!(note_texts.iter().any(|n| n.contains("Intake note")));
    assert!(note_texts.iter().any(|n| n.contains("[INTAKE MEDICAL]")));
}

// ===========================================================================
// Test 24: Admission without notes or medical notes
// ===========================================================================

#[tokio::test]
async fn test_admit_minimal_no_notes() {
    let reg = setup().await;

    let mut admission = simple_admission(police_custody_basis());
    admission.notes = None;
    admission.intake_medical_notes = None;
    admission.property = Vec::new();
    admission.emergency_contacts = Vec::new();
    admission.legal_representation = None;
    admission.legal_reference = None;

    let detainee = reg.admit(admission).await.unwrap();
    let fetched = reg.get_detainee(detainee.id).await.unwrap();

    assert!(fetched.notes.is_empty());
    assert!(fetched.property.is_empty());
    assert!(fetched.emergency_contacts.is_empty());
    assert!(fetched.legal_representation.is_none());
    assert!(fetched.legal_reference.is_none());
}

// ===========================================================================
// Test 25: Release from InCourt status succeeds
// ===========================================================================

#[tokio::test]
async fn test_release_from_in_court() {
    let reg = setup().await;
    let op = operator();

    let detainee = reg.admit(simple_admission(police_custody_basis())).await.unwrap();
    let id = detainee.id;

    // Move to InCourt
    reg.update_facility_status(id, FacilityStatus::InCourt, op, None)
        .await
        .unwrap();

    // Release from InCourt should succeed
    let release = ReleaseRecord {
        detainee_id: id,
        release_date: today(),
        release_type: ReleaseType::Acquitted,
        authorized_by: op,
        all_property_returned: true,
        notes: Some("Acquitted in court".to_string()),
    };
    let released = reg.release(release).await.unwrap();
    assert_eq!(released.facility_status, FacilityStatus::Released);
}

// ===========================================================================
// Test 26: Release from InHospital status succeeds
// ===========================================================================

#[tokio::test]
async fn test_release_from_in_hospital() {
    let reg = setup().await;
    let op = operator();

    let detainee = reg.admit(simple_admission(police_custody_basis())).await.unwrap();
    let id = detainee.id;

    // Move to InHospital
    reg.update_facility_status(id, FacilityStatus::InHospital, op, None)
        .await
        .unwrap();

    // Release from InHospital should succeed
    let release = ReleaseRecord {
        detainee_id: id,
        release_date: today(),
        release_type: ReleaseType::CourtOrdered,
        authorized_by: op,
        all_property_returned: true,
        notes: None,
    };
    let released = reg.release(release).await.unwrap();
    assert_eq!(released.facility_status, FacilityStatus::Released);
}

// ===========================================================================
// Test 27: Cannot release from Transferred status
// ===========================================================================

#[tokio::test]
async fn test_cannot_release_transferred() {
    let reg = setup().await;
    let op = operator();

    let detainee = reg.admit(simple_admission(police_custody_basis())).await.unwrap();
    let id = detainee.id;

    reg.update_facility_status(id, FacilityStatus::Transferred, op, None)
        .await
        .unwrap();

    let release = ReleaseRecord {
        detainee_id: id,
        release_date: today(),
        release_type: ReleaseType::CourtOrdered,
        authorized_by: op,
        all_property_returned: true,
        notes: None,
    };
    let result = reg.release(release).await;
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        RegistryError::Domain(DomainError::InactiveDetainee { .. })
    ));
}

// ===========================================================================
// Test 28: Cannot release from Escaped status
// ===========================================================================

#[tokio::test]
async fn test_cannot_release_escaped() {
    let reg = setup().await;
    let op = operator();

    let detainee = reg.admit(simple_admission(police_custody_basis())).await.unwrap();
    let id = detainee.id;

    reg.update_facility_status(id, FacilityStatus::Escaped, op, None)
        .await
        .unwrap();

    let release = ReleaseRecord {
        detainee_id: id,
        release_date: today(),
        release_type: ReleaseType::CourtOrdered,
        authorized_by: op,
        all_property_returned: true,
        notes: None,
    };
    let result = reg.release(release).await;
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        RegistryError::Domain(DomainError::InactiveDetainee { .. })
    ));
}

// ===========================================================================
// Test 29: Cannot release from Deceased status
// ===========================================================================

#[tokio::test]
async fn test_cannot_release_deceased() {
    let reg = setup().await;
    let op = operator();

    let detainee = reg.admit(simple_admission(police_custody_basis())).await.unwrap();
    let id = detainee.id;

    reg.update_facility_status(id, FacilityStatus::Deceased, op, None)
        .await
        .unwrap();

    let release = ReleaseRecord {
        detainee_id: id,
        release_date: today(),
        release_type: ReleaseType::Death,
        authorized_by: op,
        all_property_returned: true,
        notes: None,
    };
    let result = reg.release(release).await;
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        RegistryError::Domain(DomainError::InactiveDetainee { .. })
    ));
}

// ===========================================================================
// Test 30: Release deactivates warrant
// ===========================================================================

#[tokio::test]
async fn test_release_deactivates_warrant() {
    let reg = setup().await;
    let op = operator();

    let detainee = reg.admit(simple_admission(police_custody_basis())).await.unwrap();
    let id = detainee.id;
    assert!(detainee.active_warrant().is_some());

    let release = ReleaseRecord {
        detainee_id: id,
        release_date: today(),
        release_type: ReleaseType::CourtOrdered,
        authorized_by: op,
        all_property_returned: true,
        notes: None,
    };
    let released = reg.release(release).await.unwrap();
    assert!(released.active_warrant().is_none());
    // Warrant should still exist but be inactive
    assert_eq!(released.warrants.len(), 1);
    assert!(!released.warrants[0].is_active);
}

// ===========================================================================
// Test 31: Release adds note when provided
// ===========================================================================

#[tokio::test]
async fn test_release_with_note() {
    let reg = setup().await;
    let op = operator();

    let detainee = reg.admit(simple_admission(police_custody_basis())).await.unwrap();
    let id = detainee.id;
    let notes_before = detainee.notes.len();

    let release = ReleaseRecord {
        detainee_id: id,
        release_date: today(),
        release_type: ReleaseType::CourtOrdered,
        authorized_by: op,
        all_property_returned: true,
        notes: Some("Released per court order #123".to_string()),
    };
    let released = reg.release(release).await.unwrap();
    assert!(released.notes.len() > notes_before);
    assert!(released
        .notes
        .iter()
        .any(|n| n.content.contains("Released per court order #123")));
}

// ===========================================================================
// Test 32: Multiple warrant registrations
// ===========================================================================

#[tokio::test]
async fn test_multiple_warrant_registrations() {
    let reg = setup().await;
    let op = operator();

    let detainee = reg.admit(simple_admission(police_custody_basis())).await.unwrap();
    let id = detainee.id;

    // Register second warrant
    let warrant2 = CommitmentOrder {
        id: WarrantId::new(),
        detainee_id: id,
        order_type: CommitmentOrderType::RemandOrder,
        external_reference: Some("REM/2025/002".to_string()),
        issuing_authority: "Court".to_string(),
        issuing_officer: None,
        date_issued: today(),
        date_received: today(),
        valid_until: None,
        offence_description: None,
        sentence_details: None,
        is_active: true,
        registered_by: op,
        batch_id: None,
        document_hash: None,
    };
    let updated = reg.register_warrant(warrant2, op).await.unwrap();
    assert_eq!(updated.warrants.len(), 2);

    // Register third warrant
    let warrant3 = CommitmentOrder {
        id: WarrantId::new(),
        detainee_id: id,
        order_type: CommitmentOrderType::ConvictionCommitment,
        external_reference: Some("CONV/2025/001".to_string()),
        issuing_authority: "High Court".to_string(),
        issuing_officer: Some("Justice M".to_string()),
        date_issued: today(),
        date_received: today(),
        valid_until: None,
        offence_description: Some("Armed robbery".to_string()),
        sentence_details: None,
        is_active: true,
        registered_by: op,
        batch_id: None,
        document_hash: None,
    };
    let updated = reg.register_warrant(warrant3, op).await.unwrap();
    assert_eq!(updated.warrants.len(), 3);

    // Only one should be active
    let active_count = updated.warrants.iter().filter(|w| w.is_active).count();
    assert_eq!(active_count, 1);

    // The active one should be the most recent
    let active = updated.active_warrant().unwrap();
    assert!(matches!(active.order_type, CommitmentOrderType::ConvictionCommitment));
}

// ===========================================================================
// Test 33: All valid basis transitions succeed
// ===========================================================================

#[tokio::test]
async fn test_valid_transitions_no_legal_basis_to_police_custody() {
    let reg = setup().await;
    let op = operator();

    let basis = DetentionBasis::NoLegalBasis {
        discovered_date: today(),
        circumstances: "Unknown".to_string(),
    };
    let mut admission = simple_admission(basis);
    admission.warrant = None;
    let detainee = reg.admit(admission).await.unwrap();

    // NoLegalBasis → PoliceCustody
    let result = reg
        .update_detention_basis(detainee.id, police_custody_basis(), op)
        .await;
    assert!(result.is_ok());
    assert!(matches!(
        result.unwrap().detention_basis,
        DetentionBasis::PoliceCustody { .. }
    ));
}

#[tokio::test]
async fn test_valid_transitions_no_legal_basis_to_remand() {
    let reg = setup().await;
    let op = operator();

    let basis = DetentionBasis::NoLegalBasis {
        discovered_date: today(),
        circumstances: "Unknown".to_string(),
    };
    let mut admission = simple_admission(basis);
    admission.warrant = None;
    let detainee = reg.admit(admission).await.unwrap();

    // NoLegalBasis → Remand
    let result = reg
        .update_detention_basis(detainee.id, remand_basis(), op)
        .await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_valid_transition_remand_to_sentenced() {
    let reg = setup().await;
    let op = operator();

    let detainee = reg.admit(simple_admission(remand_basis())).await.unwrap();
    let id = detainee.id;

    let sentenced = DetentionBasis::Sentenced {
        sentence_date: today(),
        sentence: SentenceDuration::from_days(365).unwrap(),
        release_date: compute_release_date(
            today(),
            SentenceDuration::from_days(365).unwrap(),
            &[],
            &[],
        ),
        credits: Vec::new(),
        adjustments: Vec::new(),
        charges: vec![Charge {
            id: ChargeId::new(),
            description: "Theft".to_string(),
            statute: None,
            severity: ChargeSeverity::Moderate,
            date_of_alleged_offence: None,
            count_number: None,
        }],
    };
    let result = reg.update_detention_basis(id, sentenced, op).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_valid_transition_sentenced_to_appeal() {
    let reg = setup().await;
    let op = operator();

    // Admit → Remand → Sentenced → Appeal
    let detainee = reg.admit(simple_admission(remand_basis())).await.unwrap();
    let id = detainee.id;

    let sentenced = DetentionBasis::Sentenced {
        sentence_date: today(),
        sentence: SentenceDuration::from_days(365).unwrap(),
        release_date: compute_release_date(
            today(),
            SentenceDuration::from_days(365).unwrap(),
            &[],
            &[],
        ),
        credits: Vec::new(),
        adjustments: Vec::new(),
        charges: vec![Charge {
            id: ChargeId::new(),
            description: "Theft".to_string(),
            statute: None,
            severity: ChargeSeverity::Moderate,
            date_of_alleged_offence: None,
            count_number: None,
        }],
    };
    reg.update_detention_basis(id, sentenced, op).await.unwrap();

    let appeal = DetentionBasis::SentencedOnAppeal {
        sentence: SentenceDuration::from_days(365).unwrap(),
        provisional_release_date: compute_release_date(
            today(),
            SentenceDuration::from_days(365).unwrap(),
            &[],
            &[],
        ),
        appeal_filed_date: today(),
        next_hearing_date: Some(Utc::now().date_naive() + chrono::Duration::days(60)),
        credits: Vec::new(),
        adjustments: Vec::new(),
        charges: vec![Charge {
            id: ChargeId::new(),
            description: "Theft".to_string(),
            statute: None,
            severity: ChargeSeverity::Moderate,
            date_of_alleged_offence: None,
            count_number: None,
        }],
    };
    let result = reg.update_detention_basis(id, appeal, op).await;
    assert!(result.is_ok());
    assert!(matches!(
        result.unwrap().detention_basis,
        DetentionBasis::SentencedOnAppeal { .. }
    ));
}

// ===========================================================================
// Test 34: More invalid basis transitions
// ===========================================================================

#[tokio::test]
async fn test_invalid_transition_remand_to_police_custody() {
    let reg = setup().await;
    let op = operator();

    let detainee = reg.admit(simple_admission(remand_basis())).await.unwrap();
    let result = reg
        .update_detention_basis(detainee.id, police_custody_basis(), op)
        .await;
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        RegistryError::Domain(DomainError::InvalidBasisTransition { .. })
    ));
}

#[tokio::test]
async fn test_invalid_transition_sentenced_to_police_custody() {
    let reg = setup().await;
    let op = operator();

    // Admit → Remand → Sentenced
    let detainee = reg.admit(simple_admission(remand_basis())).await.unwrap();
    let id = detainee.id;
    let sentenced = DetentionBasis::Sentenced {
        sentence_date: today(),
        sentence: SentenceDuration::from_days(365).unwrap(),
        release_date: compute_release_date(
            today(),
            SentenceDuration::from_days(365).unwrap(),
            &[],
            &[],
        ),
        credits: Vec::new(),
        adjustments: Vec::new(),
        charges: vec![Charge {
            id: ChargeId::new(),
            description: "Theft".to_string(),
            statute: None,
            severity: ChargeSeverity::Moderate,
            date_of_alleged_offence: None,
            count_number: None,
        }],
    };
    reg.update_detention_basis(id, sentenced, op).await.unwrap();

    // Sentenced → PoliceCustody should fail
    let result = reg
        .update_detention_basis(id, police_custody_basis(), op)
        .await;
    assert!(result.is_err());
    match result.unwrap_err() {
        RegistryError::Domain(DomainError::InvalidBasisTransition { from, to }) => {
            assert_eq!(from, DetentionBasisLabel::Sentenced);
            assert_eq!(to, DetentionBasisLabel::PoliceCustody);
        }
        other => panic!("Expected InvalidBasisTransition, got: {:?}", other),
    }
}

// ===========================================================================
// Test 35: Update basis on non-existent detainee
// ===========================================================================

#[tokio::test]
async fn test_update_basis_detainee_not_found() {
    let reg = setup().await;
    let op = operator();

    let fake_id = DetaineeId::new();
    let result = reg
        .update_detention_basis(fake_id, remand_basis(), op)
        .await;
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        RegistryError::Domain(DomainError::DetaineeNotFound { .. })
    ));
}

// ===========================================================================
// Test 36: Facility status update without note
// ===========================================================================

#[tokio::test]
async fn test_facility_status_update_no_note() {
    let reg = setup().await;
    let op = operator();

    let detainee = reg.admit(simple_admission(police_custody_basis())).await.unwrap();
    let id = detainee.id;
    let initial_notes = detainee.notes.len();

    let updated = reg
        .update_facility_status(id, FacilityStatus::InCourt, op, None)
        .await
        .unwrap();
    assert_eq!(updated.facility_status, FacilityStatus::InCourt);
    // No additional note should be added
    assert_eq!(updated.notes.len(), initial_notes);
}

// ===========================================================================
// Test 37: Flag — RemandReviewOverdue
// ===========================================================================

#[tokio::test]
async fn test_flag_remand_review_overdue() {
    let reg = setup().await;

    let yesterday = Utc::now().date_naive() - chrono::Duration::days(1);
    let basis = DetentionBasis::RemandAwaitingTrial {
        first_appearance_date: past_date(2025, 1, 1),
        next_court_date: Some(Utc::now().date_naive() + chrono::Duration::days(30)),
        remand_review_due: Some(yesterday),
        bail_status: BailStatus::NotApplied,
        charges: vec![Charge {
            id: ChargeId::new(),
            description: "Theft".to_string(),
            statute: None,
            severity: ChargeSeverity::Minor,
            date_of_alleged_offence: None,
            count_number: None,
        }],
    };

    let admission = simple_admission(basis);
    let detainee = reg.admit(admission).await.unwrap();

    let config = FacilityConfig::default();
    let flags = openward_registry::flags::compute_flags(&detainee, &config, &[]);

    assert!(
        flags
            .iter()
            .any(|f| matches!(f, Flag::RemandReviewOverdue { .. })),
        "Expected RemandReviewOverdue flag, got: {:?}",
        flags
    );
}

// ===========================================================================
// Test 38: Flag — RemandReviewOverdue NOT raised when review not yet due
// ===========================================================================

#[tokio::test]
async fn test_flag_remand_review_not_overdue() {
    let reg = setup().await;

    let future = Utc::now().date_naive() + chrono::Duration::days(30);
    let basis = DetentionBasis::RemandAwaitingTrial {
        first_appearance_date: today(),
        next_court_date: Some(Utc::now().date_naive() + chrono::Duration::days(30)),
        remand_review_due: Some(future),
        bail_status: BailStatus::NotApplied,
        charges: vec![Charge {
            id: ChargeId::new(),
            description: "Theft".to_string(),
            statute: None,
            severity: ChargeSeverity::Minor,
            date_of_alleged_offence: None,
            count_number: None,
        }],
    };

    let admission = simple_admission(basis);
    let detainee = reg.admit(admission).await.unwrap();

    let config = FacilityConfig::default();
    let flags = openward_registry::flags::compute_flags(&detainee, &config, &[]);

    assert!(
        !flags
            .iter()
            .any(|f| matches!(f, Flag::RemandReviewOverdue { .. })),
        "RemandReviewOverdue should NOT be raised when review is in the future"
    );
}

// ===========================================================================
// Test 39: Flag — ProlongedPreTrial
// ===========================================================================

#[tokio::test]
async fn test_flag_prolonged_pretrial() {
    let reg = setup().await;

    // Admit with intake date far in the past
    let basis = DetentionBasis::RemandAwaitingTrial {
        first_appearance_date: past_date(2024, 1, 1),
        next_court_date: Some(Utc::now().date_naive() + chrono::Duration::days(30)),
        remand_review_due: None,
        bail_status: BailStatus::NotApplied,
        charges: vec![Charge {
            id: ChargeId::new(),
            description: "Theft".to_string(),
            statute: None,
            severity: ChargeSeverity::Minor,
            date_of_alleged_offence: None,
            count_number: None,
        }],
    };
    let mut admission = simple_admission(basis);
    admission.intake_date = past_date(2024, 1, 1); // > 180 days ago

    let detainee = reg.admit(admission).await.unwrap();

    let config = FacilityConfig::default(); // 180 days warn threshold
    let flags = openward_registry::flags::compute_flags(&detainee, &config, &[]);

    assert!(
        flags
            .iter()
            .any(|f| matches!(f, Flag::ProlongedPreTrial { .. })),
        "Expected ProlongedPreTrial flag for detainee held >180 days, got: {:?}",
        flags
    );
}

// ===========================================================================
// Test 40: Flag — ProlongedPreTrial NOT raised for short-term
// ===========================================================================

#[tokio::test]
async fn test_flag_prolonged_pretrial_not_raised_short_term() {
    let reg = setup().await;

    let admission = simple_admission(remand_basis());
    let detainee = reg.admit(admission).await.unwrap();

    let config = FacilityConfig::default();
    let flags = openward_registry::flags::compute_flags(&detainee, &config, &[]);

    assert!(
        !flags
            .iter()
            .any(|f| matches!(f, Flag::ProlongedPreTrial { .. })),
        "ProlongedPreTrial should NOT be raised for same-day admission"
    );
}

// ===========================================================================
// Test 41: Flag — ProlongedPreTrial NOT raised for sentenced detainees
// ===========================================================================

#[tokio::test]
async fn test_flag_prolonged_pretrial_not_raised_for_sentenced() {
    let reg = setup().await;
    let op = operator();

    let mut admission = simple_admission(remand_basis());
    admission.intake_date = past_date(2024, 1, 1);
    let detainee = reg.admit(admission).await.unwrap();

    // Transition to Sentenced
    let sentenced = DetentionBasis::Sentenced {
        sentence_date: today(),
        sentence: SentenceDuration::from_days(365).unwrap(),
        release_date: compute_release_date(
            today(),
            SentenceDuration::from_days(365).unwrap(),
            &[],
            &[],
        ),
        credits: Vec::new(),
        adjustments: Vec::new(),
        charges: vec![Charge {
            id: ChargeId::new(),
            description: "Theft".to_string(),
            statute: None,
            severity: ChargeSeverity::Moderate,
            date_of_alleged_offence: None,
            count_number: None,
        }],
    };
    let updated = reg
        .update_detention_basis(detainee.id, sentenced, op)
        .await
        .unwrap();

    let config = FacilityConfig::default();
    let flags = openward_registry::flags::compute_flags(&updated, &config, &[]);

    assert!(
        !flags
            .iter()
            .any(|f| matches!(f, Flag::ProlongedPreTrial { .. })),
        "ProlongedPreTrial should NOT be raised for sentenced detainees"
    );
}

// ===========================================================================
// Test 42: Flag — CourtDateImminent
// ===========================================================================

#[tokio::test]
async fn test_flag_court_date_imminent() {
    let reg = setup().await;

    // Set next_court_date to tomorrow (within 48h default threshold)
    let tomorrow = Utc::now().date_naive() + chrono::Duration::days(1);
    let basis = DetentionBasis::RemandAwaitingTrial {
        first_appearance_date: today(),
        next_court_date: Some(tomorrow),
        remand_review_due: None,
        bail_status: BailStatus::NotApplied,
        charges: vec![Charge {
            id: ChargeId::new(),
            description: "Theft".to_string(),
            statute: None,
            severity: ChargeSeverity::Minor,
            date_of_alleged_offence: None,
            count_number: None,
        }],
    };

    let admission = simple_admission(basis);
    let detainee = reg.admit(admission).await.unwrap();

    let config = FacilityConfig::default(); // 48h threshold
    let flags = openward_registry::flags::compute_flags(&detainee, &config, &[]);

    assert!(
        flags
            .iter()
            .any(|f| matches!(f, Flag::CourtDateImminent { .. })),
        "Expected CourtDateImminent flag, got: {:?}",
        flags
    );
}

// ===========================================================================
// Test 43: Flag — CourtDateImminent NOT raised when far away
// ===========================================================================

#[tokio::test]
async fn test_flag_court_date_not_imminent() {
    let reg = setup().await;

    let far_future = Utc::now().date_naive() + chrono::Duration::days(30);
    let basis = DetentionBasis::RemandAwaitingTrial {
        first_appearance_date: today(),
        next_court_date: Some(far_future),
        remand_review_due: None,
        bail_status: BailStatus::NotApplied,
        charges: vec![Charge {
            id: ChargeId::new(),
            description: "Theft".to_string(),
            statute: None,
            severity: ChargeSeverity::Minor,
            date_of_alleged_offence: None,
            count_number: None,
        }],
    };

    let admission = simple_admission(basis);
    let detainee = reg.admit(admission).await.unwrap();

    let config = FacilityConfig::default();
    let flags = openward_registry::flags::compute_flags(&detainee, &config, &[]);

    assert!(
        !flags
            .iter()
            .any(|f| matches!(f, Flag::CourtDateImminent { .. })),
        "CourtDateImminent should NOT be raised when court date is 30 days away"
    );
}

// ===========================================================================
// Test 44: Flag — CourtDateImminent NOT raised for past court dates
// ===========================================================================

#[tokio::test]
async fn test_flag_court_date_imminent_not_for_past_dates() {
    let reg = setup().await;

    // Court date was yesterday — should NOT trigger imminent
    let yesterday = Utc::now().date_naive() - chrono::Duration::days(1);
    let basis = DetentionBasis::RemandAwaitingTrial {
        first_appearance_date: past_date(2025, 1, 1),
        next_court_date: Some(yesterday),
        remand_review_due: None,
        bail_status: BailStatus::NotApplied,
        charges: vec![Charge {
            id: ChargeId::new(),
            description: "Theft".to_string(),
            statute: None,
            severity: ChargeSeverity::Minor,
            date_of_alleged_offence: None,
            count_number: None,
        }],
    };
    let mut admission = simple_admission(basis);
    admission.intake_date = past_date(2025, 1, 1);
    let detainee = reg.admit(admission).await.unwrap();

    let config = FacilityConfig::default();
    let flags = openward_registry::flags::compute_flags(&detainee, &config, &[]);

    assert!(
        !flags
            .iter()
            .any(|f| matches!(f, Flag::CourtDateImminent { .. })),
        "CourtDateImminent should NOT be raised when court date is in the past"
    );
}

// ===========================================================================
// Test 45: Flag — NoActiveWarrant for non-NoLegalBasis detainee
// ===========================================================================

#[tokio::test]
async fn test_flag_no_active_warrant() {
    let reg = setup().await;

    // Admit without a warrant, but with a real detention basis
    let mut admission = simple_admission(remand_basis());
    admission.warrant = None;
    let detainee = reg.admit(admission).await.unwrap();

    let config = FacilityConfig::default();
    let flags = openward_registry::flags::compute_flags(&detainee, &config, &[]);

    assert!(
        flags.iter().any(|f| matches!(f, Flag::NoActiveWarrant)),
        "Expected NoActiveWarrant flag for remand detainee without warrant, got: {:?}",
        flags
    );
}

// ===========================================================================
// Test 46: Flag — WarrantExpired
// ===========================================================================

#[tokio::test]
async fn test_flag_warrant_expired() {
    let reg = setup().await;

    // Create warrant with valid_until in the past
    let yesterday = Utc::now().date_naive() - chrono::Duration::days(1);
    let mut admission = simple_admission(police_custody_basis());
    admission.warrant.as_mut().unwrap().valid_until = Some(yesterday);
    let detainee = reg.admit(admission).await.unwrap();

    let config = FacilityConfig::default();
    let flags = openward_registry::flags::compute_flags(&detainee, &config, &[]);

    assert!(
        flags
            .iter()
            .any(|f| matches!(f, Flag::WarrantExpired { .. })),
        "Expected WarrantExpired flag, got: {:?}",
        flags
    );
}

// ===========================================================================
// Test 47: Flag — WarrantExpired NOT raised when valid
// ===========================================================================

#[tokio::test]
async fn test_flag_warrant_not_expired() {
    let reg = setup().await;

    // Default admission has valid_until 14 days in the future
    let detainee = reg.admit(simple_admission(police_custody_basis())).await.unwrap();

    let config = FacilityConfig::default();
    let flags = openward_registry::flags::compute_flags(&detainee, &config, &[]);

    assert!(
        !flags
            .iter()
            .any(|f| matches!(f, Flag::WarrantExpired { .. })),
        "WarrantExpired should NOT be raised when warrant is still valid"
    );
}

// ===========================================================================
// Test 48: No flags computed for Released detainee
// ===========================================================================

#[tokio::test]
async fn test_no_flags_for_released() {
    let reg = setup().await;
    let op = operator();

    let detainee = reg.admit(simple_admission(police_custody_basis())).await.unwrap();
    let id = detainee.id;

    let release = ReleaseRecord {
        detainee_id: id,
        release_date: today(),
        release_type: ReleaseType::CourtOrdered,
        authorized_by: op,
        all_property_returned: true,
        notes: None,
    };
    let released = reg.release(release).await.unwrap();

    let config = FacilityConfig::default();
    let flags = openward_registry::flags::compute_flags(&released, &config, &[]);
    assert!(
        flags.is_empty(),
        "Released detainee should have no flags, got: {:?}",
        flags
    );
}

// ===========================================================================
// Test 49: No flags for Transferred detainee
// ===========================================================================

#[tokio::test]
async fn test_no_flags_for_transferred() {
    let reg = setup().await;
    let op = operator();

    let detainee = reg.admit(simple_admission(police_custody_basis())).await.unwrap();
    reg.update_facility_status(detainee.id, FacilityStatus::Transferred, op, None)
        .await
        .unwrap();
    let transferred = reg.get_detainee(detainee.id).await.unwrap();

    let config = FacilityConfig::default();
    let flags = openward_registry::flags::compute_flags(&transferred, &config, &[]);
    assert!(
        flags.is_empty(),
        "Transferred detainee should have no flags, got: {:?}",
        flags
    );
}

// ===========================================================================
// Test 50: No flags for Escaped detainee
// ===========================================================================

#[tokio::test]
async fn test_no_flags_for_escaped() {
    let reg = setup().await;
    let op = operator();

    let detainee = reg.admit(simple_admission(police_custody_basis())).await.unwrap();
    reg.update_facility_status(detainee.id, FacilityStatus::Escaped, op, None)
        .await
        .unwrap();
    let escaped = reg.get_detainee(detainee.id).await.unwrap();

    let config = FacilityConfig::default();
    let flags = openward_registry::flags::compute_flags(&escaped, &config, &[]);
    assert!(
        flags.is_empty(),
        "Escaped detainee should have no flags, got: {:?}",
        flags
    );
}

// ===========================================================================
// Test 51: Flags still computed for InCourt detainee
// ===========================================================================

#[tokio::test]
async fn test_flags_for_in_court_detainee() {
    let reg = setup().await;
    let op = operator();

    // Admit with no legal representation + no warrant
    let mut admission = simple_admission(remand_basis());
    admission.warrant = None;
    admission.legal_representation = None;
    let detainee = reg.admit(admission).await.unwrap();
    reg.update_facility_status(detainee.id, FacilityStatus::InCourt, op, None)
        .await
        .unwrap();
    let in_court = reg.get_detainee(detainee.id).await.unwrap();

    let config = FacilityConfig::default();
    let flags = openward_registry::flags::compute_flags(&in_court, &config, &[]);

    // InCourt detainee should still get flags
    assert!(
        !flags.is_empty(),
        "InCourt detainee should still get flags computed"
    );
    assert!(flags.iter().any(|f| matches!(f, Flag::NoLegalRepresentation)));
    assert!(flags.iter().any(|f| matches!(f, Flag::NoActiveWarrant)));
}

// ===========================================================================
// Test 52: Flags computed for InHospital detainee
// ===========================================================================

#[tokio::test]
async fn test_flags_for_in_hospital_detainee() {
    let reg = setup().await;
    let op = operator();

    let mut admission = simple_admission(remand_basis());
    admission.warrant = None;
    admission.legal_representation = None;
    let detainee = reg.admit(admission).await.unwrap();
    reg.update_facility_status(detainee.id, FacilityStatus::InHospital, op, None)
        .await
        .unwrap();
    let in_hospital = reg.get_detainee(detainee.id).await.unwrap();

    let config = FacilityConfig::default();
    let flags = openward_registry::flags::compute_flags(&in_hospital, &config, &[]);

    assert!(
        !flags.is_empty(),
        "InHospital detainee should still get flags computed"
    );
}

// ===========================================================================
// Test 53: BailGrantedStillHeld NOT raised for InCourt
// ===========================================================================

#[tokio::test]
async fn test_flag_bail_granted_not_raised_for_in_court() {
    let reg = setup().await;
    let op = operator();

    let basis = DetentionBasis::RemandAwaitingTrial {
        first_appearance_date: today(),
        next_court_date: Some(Utc::now().date_naive() + chrono::Duration::days(30)),
        remand_review_due: None,
        bail_status: BailStatus::Granted {
            date: today(),
            amount: Some(50_000),
            conditions: vec![],
        },
        charges: vec![Charge {
            id: ChargeId::new(),
            description: "Assault".to_string(),
            statute: None,
            severity: ChargeSeverity::Moderate,
            date_of_alleged_offence: None,
            count_number: None,
        }],
    };

    let admission = simple_admission(basis);
    let detainee = reg.admit(admission).await.unwrap();
    reg.update_facility_status(detainee.id, FacilityStatus::InCourt, op, None)
        .await
        .unwrap();
    let in_court = reg.get_detainee(detainee.id).await.unwrap();

    let config = FacilityConfig::default();
    let flags = openward_registry::flags::compute_flags(&in_court, &config, &[]);

    assert!(
        !flags
            .iter()
            .any(|f| matches!(f, Flag::BailGrantedStillHeld { .. })),
        "BailGrantedStillHeld should NOT be raised when facility_status is InCourt"
    );
}

// ===========================================================================
// Test 54: Flag — NoLegalRepresentation NOT raised when represented
// ===========================================================================

#[tokio::test]
async fn test_flag_no_legal_rep_not_raised_when_represented() {
    let reg = setup().await;

    let mut admission = simple_admission(police_custody_basis());
    admission.legal_representation = Some(LegalRepresentation {
        representative_name: "Adv. Smith".to_string(),
        representative_type: "Private".to_string(),
        contact: None,
        assigned_date: today(),
    });
    let detainee = reg.admit(admission).await.unwrap();

    let config = FacilityConfig::default();
    let flags = openward_registry::flags::compute_flags(&detainee, &config, &[]);

    assert!(
        !flags.iter().any(|f| matches!(f, Flag::NoLegalRepresentation)),
        "NoLegalRepresentation should NOT be raised when detainee has representation"
    );
}

// ===========================================================================
// Test 55: FlagCounts aggregation
// ===========================================================================

#[tokio::test]
async fn test_flag_counts_aggregation() {
    // Create synthetic flag vectors to test the counting function
    let flags_per_detainee = vec![
        vec![
            Flag::NoLegalBasis { days_held: 10 },
            Flag::NoLegalRepresentation,
        ],
        vec![
            Flag::CustodyLimitExceeded { hours_over: 5 },
            Flag::NoLegalRepresentation,
        ],
        vec![
            Flag::NoCourtDate { days_without: 30 },
            Flag::RemandReviewOverdue { days_overdue: 5 },
            Flag::ProlongedPreTrial { days_held: 200, threshold_days: 180 },
            Flag::BailGrantedStillHeld { days_since_grant: 3 },
        ],
        vec![
            Flag::ReleaseDatePassed { days_overdue: 2 },
            Flag::NoActiveWarrant,
        ],
    ];

    let counts = openward_registry::flags::compute_flag_counts(&flags_per_detainee);

    assert_eq!(counts.no_legal_basis, 1);
    assert_eq!(counts.custody_limit_exceeded, 1);
    assert_eq!(counts.no_court_date, 1);
    assert_eq!(counts.remand_review_overdue, 1);
    assert_eq!(counts.prolonged_pre_trial, 1);
    assert_eq!(counts.bail_granted_still_held, 1);
    assert_eq!(counts.release_date_passed, 1);
    assert_eq!(counts.no_legal_representation, 2);
    assert_eq!(counts.court_date_imminent, 0);
    assert_eq!(counts.release_imminent, 0);
    assert_eq!(counts.housing_violation, 0);
    assert_eq!(counts.warrant_expired, 0);
    assert_eq!(counts.no_active_warrant, 1);
}

// ===========================================================================
// Test 56: FlagCounts with empty input
// ===========================================================================

#[tokio::test]
async fn test_flag_counts_empty() {
    let counts = openward_registry::flags::compute_flag_counts(&[]);
    assert_eq!(counts.no_legal_basis, 0);
    assert_eq!(counts.no_legal_representation, 0);
    assert_eq!(counts.no_active_warrant, 0);
}

// ===========================================================================
// Test 57: Search by AnyPreTrial filter
// ===========================================================================

#[tokio::test]
async fn test_search_any_pretrial() {
    let reg = setup().await;
    let op = operator();

    // Admit: 1 PoliceCustody + 1 Remand + 1 Sentenced
    let mut a1 = simple_admission(police_custody_basis());
    a1.identity = make_identity("Police", "One", Sex::Male);
    reg.admit(a1).await.unwrap();

    let mut a2 = simple_admission(remand_basis());
    a2.identity = make_identity("Remand", "Two", Sex::Male);
    let _d2 = reg.admit(a2).await.unwrap();

    // Admit a third and sentence them
    let mut a3 = simple_admission(remand_basis());
    a3.identity = make_identity("Sentenced", "Three", Sex::Male);
    let d3 = reg.admit(a3).await.unwrap();
    let sentenced = DetentionBasis::Sentenced {
        sentence_date: today(),
        sentence: SentenceDuration::from_days(365).unwrap(),
        release_date: compute_release_date(
            today(),
            SentenceDuration::from_days(365).unwrap(),
            &[],
            &[],
        ),
        credits: Vec::new(),
        adjustments: Vec::new(),
        charges: vec![Charge {
            id: ChargeId::new(),
            description: "Theft".to_string(),
            statute: None,
            severity: ChargeSeverity::Moderate,
            date_of_alleged_offence: None,
            count_number: None,
        }],
    };
    reg.update_detention_basis(d3.id, sentenced, op)
        .await
        .unwrap();

    // AnyPreTrial should return PoliceCustody + Remand but NOT Sentenced
    let query = PopulationQuery {
        detention_basis: Some(DetentionBasisFilter::AnyPreTrial),
        ..Default::default()
    };
    let result = reg.search(query, op).await.unwrap();
    assert_eq!(result.total_matching, 2);
}

// ===========================================================================
// Test 58: Search by has_legal_representation
// ===========================================================================

#[tokio::test]
async fn test_search_has_legal_representation() {
    let reg = setup().await;
    let op = operator();

    // One with representation, one without
    let mut a1 = simple_admission(police_custody_basis());
    a1.identity = make_identity("WithRep", "One", Sex::Male);
    a1.legal_representation = Some(LegalRepresentation {
        representative_name: "Adv. Smith".to_string(),
        representative_type: "Private".to_string(),
        contact: None,
        assigned_date: today(),
    });
    reg.admit(a1).await.unwrap();

    let mut a2 = simple_admission(police_custody_basis());
    a2.identity = make_identity("NoRep", "Two", Sex::Female);
    a2.legal_representation = None;
    reg.admit(a2).await.unwrap();

    // Search has_legal_representation = true
    let query = PopulationQuery {
        has_legal_representation: Some(true),
        ..Default::default()
    };
    let result = reg.search(query, op).await.unwrap();
    assert_eq!(result.total_matching, 1);
    assert_eq!(result.detainees[0].name, "WithRep, One");

    // Search has_legal_representation = false
    let query = PopulationQuery {
        has_legal_representation: Some(false),
        ..Default::default()
    };
    let result = reg.search(query, op).await.unwrap();
    assert_eq!(result.total_matching, 1);
    assert_eq!(result.detainees[0].name, "NoRep, Two");
}

// ===========================================================================
// Test 59: Search sort by name ascending
// ===========================================================================

#[tokio::test]
async fn test_search_sort_by_name() {
    let reg = setup().await;
    let op = operator();

    let names = vec!["Charlie", "Alpha", "Beta"];
    for name in &names {
        let mut a = simple_admission(police_custody_basis());
        a.identity = make_identity(name, "X", Sex::Male);
        reg.admit(a).await.unwrap();
    }

    let query = PopulationQuery {
        sort_by: Some(SortField::Name),
        sort_order: Some(SortOrder::Asc),
        ..Default::default()
    };
    let result = reg.search(query, op).await.unwrap();
    assert_eq!(result.detainees.len(), 3);
    assert_eq!(result.detainees[0].name, "Alpha, X");
    assert_eq!(result.detainees[1].name, "Beta, X");
    assert_eq!(result.detainees[2].name, "Charlie, X");
}

// ===========================================================================
// Test 60: Search sort by detention basis
// ===========================================================================

#[tokio::test]
async fn test_search_sort_by_detention_basis() {
    let reg = setup().await;
    let op = operator();

    let mut a1 = simple_admission(remand_basis());
    a1.identity = make_identity("Remand", "One", Sex::Male);
    reg.admit(a1).await.unwrap();

    let mut a2 = simple_admission(police_custody_basis());
    a2.identity = make_identity("Police", "Two", Sex::Male);
    reg.admit(a2).await.unwrap();

    // Sort by detention basis ascending
    let query = PopulationQuery {
        sort_by: Some(SortField::DetentionBasis),
        sort_order: Some(SortOrder::Asc),
        ..Default::default()
    };
    let result = reg.search(query, op).await.unwrap();
    assert_eq!(result.total_matching, 2);
    // PoliceCustody < Remand alphabetically
    assert_eq!(result.detainees[0].detention_basis, DetentionBasisLabel::PoliceCustody);
    assert_eq!(result.detainees[1].detention_basis, DetentionBasisLabel::Remand);
}

// ===========================================================================
// Test 61: Search with limit but no offset
// ===========================================================================

#[tokio::test]
async fn test_search_limit_no_offset() {
    let reg = setup().await;
    let op = operator();

    for i in 0..5 {
        let mut a = simple_admission(police_custody_basis());
        a.identity = make_identity(&format!("Name{}", i), "X", Sex::Male);
        reg.admit(a).await.unwrap();
    }

    let query = PopulationQuery {
        limit: Some(3),
        ..Default::default()
    };
    let result = reg.search(query, op).await.unwrap();
    assert_eq!(result.total_matching, 5);
    assert_eq!(result.detainees.len(), 3);
}

// ===========================================================================
// Test 62: Search with offset beyond results returns empty
// ===========================================================================

#[tokio::test]
async fn test_search_offset_beyond_results() {
    let reg = setup().await;
    let op = operator();

    let mut a = simple_admission(police_custody_basis());
    a.identity = make_identity("Only", "One", Sex::Male);
    reg.admit(a).await.unwrap();

    let query = PopulationQuery {
        limit: Some(10),
        offset: Some(100),
        ..Default::default()
    };
    let result = reg.search(query, op).await.unwrap();
    assert_eq!(result.total_matching, 1);
    assert!(result.detainees.is_empty());
}

// ===========================================================================
// Test 63: Search empty database
// ===========================================================================

#[tokio::test]
async fn test_search_empty_database() {
    let reg = setup().await;
    let op = operator();

    let result = reg
        .search(PopulationQuery::default(), op)
        .await
        .unwrap();
    assert_eq!(result.total_matching, 0);
    assert!(result.detainees.is_empty());
}

// ===========================================================================
// Test 64: Overview with empty facility
// ===========================================================================

#[tokio::test]
async fn test_overview_empty_facility() {
    let reg = setup().await;

    let overview = reg.overview().await.unwrap();
    assert_eq!(overview.total_population, 0);
    assert_eq!(overview.occupancy_percent, 0.0);
    assert_eq!(overview.pretrial_percent, 0.0);
    assert_eq!(overview.basis_breakdown.remand, 0);
    assert_eq!(overview.basis_breakdown.police_custody, 0);
    assert_eq!(overview.basis_breakdown.sentenced, 0);
}

// ===========================================================================
// Test 65: Overview with zero capacity config
// ===========================================================================

#[tokio::test]
async fn test_overview_zero_capacity() {
    let pool = create_pool("sqlite::memory:").await.unwrap();
    apply_schema(&pool).await.unwrap();
    let config = FacilityConfig {
        capacity: 0,
        ..FacilityConfig::default()
    };
    let reg = SqliteRegistry::new(pool, config);

    // Admit one detainee
    let admission = simple_admission(police_custody_basis());
    reg.admit(admission).await.unwrap();

    let overview = reg.overview().await.unwrap();
    assert_eq!(overview.total_population, 1);
    assert_eq!(overview.facility_capacity, 0);
    // Division by zero should be handled gracefully
    assert_eq!(overview.occupancy_percent, 0.0);
}

// ===========================================================================
// Test 66: Overview only counts active detainees
// ===========================================================================

#[tokio::test]
async fn test_overview_excludes_released() {
    let reg = setup().await;
    let op = operator();

    let mut a1 = simple_admission(police_custody_basis());
    a1.identity = make_identity("Active", "One", Sex::Male);
    reg.admit(a1).await.unwrap();

    let mut a2 = simple_admission(police_custody_basis());
    a2.identity = make_identity("Released", "Two", Sex::Male);
    let d2 = reg.admit(a2).await.unwrap();

    reg.release(ReleaseRecord {
        detainee_id: d2.id,
        release_date: today(),
        release_type: ReleaseType::CourtOrdered,
        authorized_by: op,
        all_property_returned: true,
        notes: None,
    })
    .await
    .unwrap();

    let overview = reg.overview().await.unwrap();
    assert_eq!(overview.total_population, 1);
}

// ===========================================================================
// Test 67: Audit chain hash verification
// ===========================================================================

#[tokio::test]
async fn test_audit_chain_hash_continuity() {
    let reg = setup().await;
    let op = operator();

    // Create multiple audit entries via operations
    let d = reg.admit(simple_admission(police_custody_basis())).await.unwrap();
    reg.update_detention_basis(d.id, remand_basis(), op)
        .await
        .unwrap();
    reg.update_facility_status(d.id, FacilityStatus::InCourt, op, Some("Court".to_string()))
        .await
        .unwrap();

    // Fetch all audit entries in order
    let entries: Vec<(Vec<u8>, Vec<u8>, i64)> = sqlx::query_as(
        "SELECT self_hash, chain_hash, epoch FROM audit_entries ORDER BY rowid"
    )
    .fetch_all(reg.pool())
    .await
    .unwrap();

    assert!(entries.len() >= 3, "Expected at least 3 audit entries");

    // Verify chain: each chain_hash = SHA256(prev_chain_hash || self_hash)
    for i in 1..entries.len() {
        let prev_chain: [u8; 32] = entries[i - 1].1.clone().try_into().unwrap();
        let self_hash: [u8; 32] = entries[i].0.clone().try_into().unwrap();

        let expected_chain = openward_registry::audit::compute_chain_hash(&prev_chain, &self_hash);
        let actual_chain: [u8; 32] = entries[i].1.clone().try_into().unwrap();

        assert_eq!(
            expected_chain, actual_chain,
            "Chain hash mismatch at entry {}",
            i
        );
    }
}

// ===========================================================================
// Test 68: Audit verify_entry function
// ===========================================================================

#[tokio::test]
async fn test_audit_verify_entry() {
    use openward_core::{AuditEntry, ModuleName, OperatorId};

    let id = uuid::Uuid::new_v4();
    let timestamp = Utc::now();
    let operator = OperatorId::new();
    let module = ModuleName::Registry;
    let action = "test_action";
    let epoch = 0u64;

    let self_hash = openward_registry::audit::compute_self_hash(
        &id, &timestamp, &operator, &module, action, None, None, None, epoch,
    );

    let entry = AuditEntry {
        id,
        timestamp,
        operator,
        module,
        action: action.to_string(),
        target: None,
        before: None,
        after: None,
        self_hash,
        chain_hash: [0u8; 32], // doesn't affect self-hash verification
        epoch,
    };

    assert!(
        openward_registry::audit::verify_entry(&entry),
        "verify_entry should return true for a correctly hashed entry"
    );
}

// ===========================================================================
// Test 69: Audit verify_entry fails for tampered entry
// ===========================================================================

#[tokio::test]
async fn test_audit_verify_entry_tampered() {
    use openward_core::{AuditEntry, ModuleName, OperatorId};

    let id = uuid::Uuid::new_v4();
    let timestamp = Utc::now();
    let operator = OperatorId::new();
    let module = ModuleName::Registry;
    let action = "test_action";
    let epoch = 0u64;

    let self_hash = openward_registry::audit::compute_self_hash(
        &id, &timestamp, &operator, &module, action, None, None, None, epoch,
    );

    let mut entry = AuditEntry {
        id,
        timestamp,
        operator,
        module,
        action: action.to_string(),
        target: None,
        before: None,
        after: None,
        self_hash,
        chain_hash: [0u8; 32],
        epoch,
    };

    // Tamper with the action
    entry.action = "tampered_action".to_string();

    assert!(
        !openward_registry::audit::verify_entry(&entry),
        "verify_entry should return false for a tampered entry"
    );
}

// ===========================================================================
// Test 70: Audit self-hash is deterministic
// ===========================================================================

#[tokio::test]
async fn test_audit_self_hash_deterministic() {
    use openward_core::{ModuleName, OperatorId};

    let id = uuid::Uuid::new_v4();
    let timestamp = Utc::now();
    let operator = OperatorId::new();
    let module = ModuleName::Registry;
    let action = "admit";
    let epoch = 42u64;
    let target = DetaineeId::new();
    let before = Some(serde_json::json!({"status": "present"}));
    let after = Some(serde_json::json!({"status": "released"}));

    let hash1 = openward_registry::audit::compute_self_hash(
        &id, &timestamp, &operator, &module, action,
        Some(&target), before.as_ref(), after.as_ref(), epoch,
    );
    let hash2 = openward_registry::audit::compute_self_hash(
        &id, &timestamp, &operator, &module, action,
        Some(&target), before.as_ref(), after.as_ref(), epoch,
    );

    assert_eq!(hash1, hash2, "Same inputs must produce same hash");
}

// ===========================================================================
// Test 71: Audit self-hash differs with different inputs
// ===========================================================================

#[tokio::test]
async fn test_audit_self_hash_differs() {
    use openward_core::{ModuleName, OperatorId};

    let id = uuid::Uuid::new_v4();
    let timestamp = Utc::now();
    let operator = OperatorId::new();
    let module = ModuleName::Registry;
    let epoch = 0u64;

    let hash1 = openward_registry::audit::compute_self_hash(
        &id, &timestamp, &operator, &module, "admit", None, None, None, epoch,
    );
    let hash2 = openward_registry::audit::compute_self_hash(
        &id, &timestamp, &operator, &module, "release", None, None, None, epoch,
    );

    assert_ne!(hash1, hash2, "Different actions must produce different hashes");
}

// ===========================================================================
// Test 72: Daily count idempotency — opening same day returns existing
// ===========================================================================

#[tokio::test]
async fn test_daily_count_open_idempotent() {
    let reg = setup().await;
    let op = operator();
    let today_date = Utc::now().date_naive();

    let count1 = reg.open_daily_count(today_date, op).await.unwrap();
    let count2 = reg.open_daily_count(today_date, op).await.unwrap();

    assert_eq!(count1.id, count2.id);
    assert_eq!(count1.opening_count, count2.opening_count);
}

// ===========================================================================
// Test 73: Daily count with multiple unit headcounts
// ===========================================================================

#[tokio::test]
async fn test_daily_count_multiple_units() {
    let reg = setup().await;
    let op = operator();

    // Create two housing units
    let unit_a = HousingUnitId::new();
    let unit_b = HousingUnitId::new();
    for (uid, name) in [(unit_a, "Block A"), (unit_b, "Block B")] {
        sqlx::query(
            "INSERT INTO housing_units (id, name, capacity, unit_type) VALUES (?, ?, ?, ?)"
        )
        .bind(uid.as_uuid().to_string())
        .bind(name)
        .bind(50i32)
        .bind("General")
        .execute(reg.pool())
        .await
        .unwrap();
    }

    let today_date = Utc::now().date_naive();
    reg.open_daily_count(today_date, op).await.unwrap();

    // Submit unit A headcount
    let hc_a = UnitHeadcount {
        unit_id: unit_a,
        count: 20,
        counted_by: op,
        counted_at: Utc::now(),
        received_at: Utc::now(),
        was_offline: false,
    };
    let after_a = reg.submit_unit_headcount(today_date, hc_a).await.unwrap();
    assert_eq!(after_a.actual_closing_count, Some(20));

    // Submit unit B headcount
    let hc_b = UnitHeadcount {
        unit_id: unit_b,
        count: 15,
        counted_by: op,
        counted_at: Utc::now(),
        received_at: Utc::now(),
        was_offline: false,
    };
    let after_b = reg.submit_unit_headcount(today_date, hc_b).await.unwrap();
    assert_eq!(after_b.actual_closing_count, Some(35)); // 20 + 15
    assert_eq!(after_b.unit_counts.len(), 2);
}

// ===========================================================================
// Test 74: Unit headcount idempotent (INSERT OR REPLACE)
// ===========================================================================

#[tokio::test]
async fn test_unit_headcount_replace() {
    let reg = setup().await;
    let op = operator();

    let unit_id = HousingUnitId::new();
    sqlx::query(
        "INSERT INTO housing_units (id, name, capacity, unit_type) VALUES (?, ?, ?, ?)"
    )
    .bind(unit_id.as_uuid().to_string())
    .bind("Block A")
    .bind(50i32)
    .bind("General")
    .execute(reg.pool())
    .await
    .unwrap();

    let today_date = Utc::now().date_naive();
    reg.open_daily_count(today_date, op).await.unwrap();

    // Submit first count
    let hc1 = UnitHeadcount {
        unit_id,
        count: 20,
        counted_by: op,
        counted_at: Utc::now(),
        received_at: Utc::now(),
        was_offline: false,
    };
    let after1 = reg.submit_unit_headcount(today_date, hc1).await.unwrap();
    assert_eq!(after1.actual_closing_count, Some(20));

    // Submit updated count for same unit — should replace
    let hc2 = UnitHeadcount {
        unit_id,
        count: 25,
        counted_by: op,
        counted_at: Utc::now(),
        received_at: Utc::now(),
        was_offline: false,
    };
    let after2 = reg.submit_unit_headcount(today_date, hc2).await.unwrap();
    assert_eq!(after2.actual_closing_count, Some(25)); // Updated, not 45
    assert_eq!(after2.unit_counts.len(), 1); // Still just one unit
}

// ===========================================================================
// Test 75: Submit headcount without opening count fails
// ===========================================================================

#[tokio::test]
async fn test_submit_headcount_without_open() {
    let reg = setup().await;

    let unit_id = HousingUnitId::new();
    let op = operator();
    let today_date = Utc::now().date_naive();

    // Don't open daily count first
    let hc = UnitHeadcount {
        unit_id,
        count: 10,
        counted_by: op,
        counted_at: Utc::now(),
        received_at: Utc::now(),
        was_offline: false,
    };
    let result = reg.submit_unit_headcount(today_date, hc).await;
    assert!(result.is_err(), "Should fail when no daily count exists for date");
}

// ===========================================================================
// Test 76: Court date scheduling for non-existent detainee
// ===========================================================================

#[tokio::test]
async fn test_schedule_court_date_detainee_not_found() {
    let reg = setup().await;
    let op = operator();

    let court_date = CourtDate {
        id: CourtDateId::new(),
        detainee_id: DetaineeId::new(), // non-existent
        scheduled_date: Utc::now().date_naive() + chrono::Duration::days(7),
        court_name: "Test Court".to_string(),
        purpose: CourtPurpose::RemandReview,
        outcome: None,
    };

    let result = reg.schedule_court_date(court_date, op).await;
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        RegistryError::Domain(DomainError::DetaineeNotFound { .. })
    ));
}

// ===========================================================================
// Test 77: Register warrant for non-existent detainee
// ===========================================================================

#[tokio::test]
async fn test_register_warrant_detainee_not_found() {
    let reg = setup().await;
    let op = operator();

    let warrant = CommitmentOrder {
        id: WarrantId::new(),
        detainee_id: DetaineeId::new(), // non-existent
        order_type: CommitmentOrderType::RemandOrder,
        external_reference: None,
        issuing_authority: "Court".to_string(),
        issuing_officer: None,
        date_issued: today(),
        date_received: today(),
        valid_until: None,
        offence_description: None,
        sentence_details: None,
        is_active: true,
        registered_by: op,
        batch_id: None,
        document_hash: None,
    };

    let result = reg.register_warrant(warrant, op).await;
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        RegistryError::Domain(DomainError::DetaineeNotFound { .. })
    ));
}

// ===========================================================================
// Test 78: Assign housing for non-existent detainee
// ===========================================================================

#[tokio::test]
async fn test_assign_housing_detainee_not_found() {
    let reg = setup().await;
    let op = operator();

    let result = reg
        .assign_housing(DetaineeId::new(), HousingUnitId::new(), op)
        .await;
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        RegistryError::Domain(DomainError::DetaineeNotFound { .. })
    ));
}

// ===========================================================================
// Test 79: Update facility status for non-existent detainee
// ===========================================================================

#[tokio::test]
async fn test_update_status_detainee_not_found() {
    let reg = setup().await;
    let op = operator();

    let result = reg
        .update_facility_status(DetaineeId::new(), FacilityStatus::InCourt, op, None)
        .await;
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        RegistryError::Domain(DomainError::DetaineeNotFound { .. })
    ));
}

// ===========================================================================
// Test 80: Batch admission with single detainee
// ===========================================================================

#[tokio::test]
async fn test_batch_admission_single_detainee() {
    let reg = setup().await;
    let op = operator();
    let batch_id = BatchId::new();

    let batch = BatchAdmission {
        batch_id,
        shared_warrant: SharedWarrantData {
            order_type: CommitmentOrderType::RemandOrder,
            external_reference: None,
            issuing_authority: "Court".to_string(),
            issuing_officer: None,
            date_issued: today(),
            date_received: today(),
            valid_until: None,
            offence_description: None,
        },
        detainees: vec![BatchDetaineeEntry {
            identity: make_identity("Solo", "Person", Sex::Female),
            detention_basis: remand_basis(),
            additional_charges: Vec::new(),
            intake_medical_notes: None,
            legal_reference: None,
            emergency_contacts: Vec::new(),
            property: Vec::new(),
            legal_representation: None,
            notes: None,
        }],
        intake_date: today(),
        admitted_by: op,
    };

    let detainees = reg.batch_admit(batch).await.unwrap();
    assert_eq!(detainees.len(), 1);
    assert_eq!(detainees[0].identity.surname, "Solo");
    assert!(detainees[0].active_warrant().is_some());
    assert_eq!(detainees[0].active_warrant().unwrap().batch_id, Some(batch_id));
}

// ===========================================================================
// Test 81: Multiple court outcomes
// ===========================================================================

#[tokio::test]
async fn test_court_outcome_adjourned() {
    let reg = setup().await;
    let op = operator();

    let detainee = reg.admit(simple_admission(remand_basis())).await.unwrap();

    let court_date = CourtDate {
        id: CourtDateId::new(),
        detainee_id: detainee.id,
        scheduled_date: Utc::now().date_naive() + chrono::Duration::days(7),
        court_name: "Magistrate Court".to_string(),
        purpose: CourtPurpose::TrialHearing,
        outcome: None,
    };
    reg.schedule_court_date(court_date.clone(), op).await.unwrap();

    let outcome = CourtOutcome::Adjourned {
        next_date: Some(Utc::now().date_naive() + chrono::Duration::days(30)),
        reason: "Witness unavailable".to_string(),
    };
    let result = reg
        .record_court_outcome(court_date.id, outcome, op)
        .await
        .unwrap();
    assert!(matches!(
        result.outcome,
        Some(CourtOutcome::Adjourned { .. })
    ));
}

#[tokio::test]
async fn test_court_outcome_acquitted() {
    let reg = setup().await;
    let op = operator();

    let detainee = reg.admit(simple_admission(remand_basis())).await.unwrap();

    let court_date = CourtDate {
        id: CourtDateId::new(),
        detainee_id: detainee.id,
        scheduled_date: Utc::now().date_naive() + chrono::Duration::days(7),
        court_name: "High Court".to_string(),
        purpose: CourtPurpose::TrialHearing,
        outcome: None,
    };
    reg.schedule_court_date(court_date.clone(), op).await.unwrap();

    let result = reg
        .record_court_outcome(court_date.id, CourtOutcome::Acquitted, op)
        .await
        .unwrap();
    assert!(matches!(result.outcome, Some(CourtOutcome::Acquitted)));
}

// ===========================================================================
// Test 82: Search by Sentenced filter
// ===========================================================================

#[tokio::test]
async fn test_search_by_sentenced() {
    let reg = setup().await;
    let op = operator();

    // Admit and sentence one
    let mut a1 = simple_admission(remand_basis());
    a1.identity = make_identity("Sentenced", "One", Sex::Male);
    let d1 = reg.admit(a1).await.unwrap();

    let sentenced = DetentionBasis::Sentenced {
        sentence_date: today(),
        sentence: SentenceDuration::from_days(365).unwrap(),
        release_date: compute_release_date(
            today(),
            SentenceDuration::from_days(365).unwrap(),
            &[],
            &[],
        ),
        credits: Vec::new(),
        adjustments: Vec::new(),
        charges: vec![Charge {
            id: ChargeId::new(),
            description: "Theft".to_string(),
            statute: None,
            severity: ChargeSeverity::Moderate,
            date_of_alleged_offence: None,
            count_number: None,
        }],
    };
    reg.update_detention_basis(d1.id, sentenced, op)
        .await
        .unwrap();

    // Admit a non-sentenced
    let mut a2 = simple_admission(police_custody_basis());
    a2.identity = make_identity("NotSentenced", "Two", Sex::Male);
    reg.admit(a2).await.unwrap();

    let query = PopulationQuery {
        detention_basis: Some(DetentionBasisFilter::Sentenced),
        ..Default::default()
    };
    let result = reg.search(query, op).await.unwrap();
    assert_eq!(result.total_matching, 1);
    assert_eq!(result.detainees[0].detention_basis, DetentionBasisLabel::Sentenced);
}

// ===========================================================================
// Test 83: Search by Released facility status
// ===========================================================================

#[tokio::test]
async fn test_search_by_released_status() {
    let reg = setup().await;
    let op = operator();

    let mut a1 = simple_admission(police_custody_basis());
    a1.identity = make_identity("Active", "One", Sex::Male);
    reg.admit(a1).await.unwrap();

    let mut a2 = simple_admission(police_custody_basis());
    a2.identity = make_identity("Released", "Two", Sex::Female);
    let d2 = reg.admit(a2).await.unwrap();

    reg.release(ReleaseRecord {
        detainee_id: d2.id,
        release_date: today(),
        release_type: ReleaseType::CourtOrdered,
        authorized_by: op,
        all_property_returned: true,
        notes: None,
    })
    .await
    .unwrap();

    let query = PopulationQuery {
        facility_status: Some(FacilityStatus::Released),
        ..Default::default()
    };
    let result = reg.search(query, op).await.unwrap();
    assert_eq!(result.total_matching, 1);
    assert_eq!(result.detainees[0].name, "Released, Two");
}

// ===========================================================================
// Test 84: Property round-trip with multiple items
// ===========================================================================

#[tokio::test]
async fn test_property_roundtrip() {
    let reg = setup().await;

    let mut admission = simple_admission(police_custody_basis());
    admission.property = vec![
        PropertyItem {
            id: 0,
            description: "Mobile phone".to_string(),
            quantity: 1,
            logged_date: today(),
            logged_by: operator(),
            returned: false,
        },
        PropertyItem {
            id: 0,
            description: "Cash (MWK)".to_string(),
            quantity: 5000,
            logged_date: today(),
            logged_by: operator(),
            returned: false,
        },
        PropertyItem {
            id: 0,
            description: "Belt".to_string(),
            quantity: 1,
            logged_date: today(),
            logged_by: operator(),
            returned: false,
        },
    ];

    let detainee = reg.admit(admission).await.unwrap();
    let fetched = reg.get_detainee(detainee.id).await.unwrap();

    assert_eq!(fetched.property.len(), 3);
    let descs: Vec<&str> = fetched.property.iter().map(|p| p.description.as_str()).collect();
    assert!(descs.contains(&"Mobile phone"));
    assert!(descs.contains(&"Cash (MWK)"));
    assert!(descs.contains(&"Belt"));

    // Check quantity is preserved
    let cash = fetched.property.iter().find(|p| p.description == "Cash (MWK)").unwrap();
    assert_eq!(cash.quantity, 5000);
}

// ===========================================================================
// Test 85: Emergency contacts round-trip
// ===========================================================================

#[tokio::test]
async fn test_emergency_contacts_roundtrip() {
    let reg = setup().await;

    let mut admission = simple_admission(police_custody_basis());
    admission.emergency_contacts = vec![
        EmergencyContact {
            name: "Mary Banda".to_string(),
            relationship: "Mother".to_string(),
            phone: Some("+265 999 111111".to_string()),
            address: Some("Lilongwe Area 25".to_string()),
        },
        EmergencyContact {
            name: "James Banda".to_string(),
            relationship: "Brother".to_string(),
            phone: None,
            address: None,
        },
    ];

    let detainee = reg.admit(admission).await.unwrap();
    let fetched = reg.get_detainee(detainee.id).await.unwrap();

    assert_eq!(fetched.emergency_contacts.len(), 2);
    assert_eq!(fetched.emergency_contacts[0].name, "Mary Banda");
    assert_eq!(fetched.emergency_contacts[0].address.as_deref(), Some("Lilongwe Area 25"));
    assert_eq!(fetched.emergency_contacts[1].name, "James Banda");
    assert!(fetched.emergency_contacts[1].phone.is_none());
}

// ===========================================================================
// Test 86: Admit with Female sex round-trips correctly
// ===========================================================================

#[tokio::test]
async fn test_sex_female_roundtrip() {
    let reg = setup().await;

    let mut admission = simple_admission(police_custody_basis());
    admission.identity = make_identity("Test", "Female", Sex::Female);
    let detainee = reg.admit(admission).await.unwrap();

    let fetched = reg.get_detainee(detainee.id).await.unwrap();
    assert!(matches!(fetched.identity.sex, Sex::Female));
}

// ===========================================================================
// Test 87: Admit with Other sex round-trips correctly
// ===========================================================================

#[tokio::test]
async fn test_sex_other_roundtrip() {
    let reg = setup().await;

    let mut admission = simple_admission(police_custody_basis());
    admission.identity = make_identity("Test", "Other", Sex::Other);
    let detainee = reg.admit(admission).await.unwrap();

    let fetched = reg.get_detainee(detainee.id).await.unwrap();
    assert!(matches!(fetched.identity.sex, Sex::Other));
}

// ===========================================================================
// Test 88: Search by Undocumented filter
// ===========================================================================

#[tokio::test]
async fn test_search_by_undocumented() {
    let reg = setup().await;
    let op = operator();

    // Admit NoLegalBasis
    let basis = DetentionBasis::NoLegalBasis {
        discovered_date: today(),
        circumstances: "Found without warrant".to_string(),
    };
    let mut a1 = simple_admission(basis);
    a1.identity = make_identity("Undoc", "One", Sex::Male);
    a1.warrant = None;
    reg.admit(a1).await.unwrap();

    // Admit a normal one
    let mut a2 = simple_admission(police_custody_basis());
    a2.identity = make_identity("Normal", "Two", Sex::Male);
    reg.admit(a2).await.unwrap();

    let query = PopulationQuery {
        detention_basis: Some(DetentionBasisFilter::Undocumented),
        ..Default::default()
    };
    let result = reg.search(query, op).await.unwrap();
    assert_eq!(result.total_matching, 1);
    assert_eq!(result.detainees[0].name, "Undoc, One");
}

// ===========================================================================
// Test 89: CustodyLimitExceeded NOT raised when must_appear_by is in future
// ===========================================================================

#[tokio::test]
async fn test_flag_custody_limit_not_exceeded() {
    let reg = setup().await;

    // must_appear_by is tomorrow — should NOT trigger
    let admission = simple_admission(police_custody_basis()); // default has tomorrow
    let detainee = reg.admit(admission).await.unwrap();

    let config = FacilityConfig::default();
    let flags = openward_registry::flags::compute_flags(&detainee, &config, &[]);

    assert!(
        !flags
            .iter()
            .any(|f| matches!(f, Flag::CustodyLimitExceeded { .. })),
        "CustodyLimitExceeded should NOT be raised when must_appear_by is in the future"
    );
}

// ===========================================================================
// Test 90: NoCourtDate NOT raised for sentenced detainees
// ===========================================================================

#[tokio::test]
async fn test_flag_no_court_date_not_raised_for_sentenced() {
    let reg = setup().await;
    let op = operator();

    let detainee = reg.admit(simple_admission(remand_basis())).await.unwrap();

    let sentenced = DetentionBasis::Sentenced {
        sentence_date: today(),
        sentence: SentenceDuration::from_days(365).unwrap(),
        release_date: compute_release_date(
            today(),
            SentenceDuration::from_days(365).unwrap(),
            &[],
            &[],
        ),
        credits: Vec::new(),
        adjustments: Vec::new(),
        charges: vec![Charge {
            id: ChargeId::new(),
            description: "Theft".to_string(),
            statute: None,
            severity: ChargeSeverity::Moderate,
            date_of_alleged_offence: None,
            count_number: None,
        }],
    };
    let updated = reg
        .update_detention_basis(detainee.id, sentenced, op)
        .await
        .unwrap();

    let config = FacilityConfig::default();
    let flags = openward_registry::flags::compute_flags(&updated, &config, &[]);

    assert!(
        !flags.iter().any(|f| matches!(f, Flag::NoCourtDate { .. })),
        "NoCourtDate should NOT be raised for sentenced detainees"
    );
}

// ===========================================================================
// Test 91: Warrant with all order types round-trip
// ===========================================================================

#[tokio::test]
async fn test_warrant_order_types_roundtrip() {
    let reg = setup().await;
    let op = operator();

    let detainee = reg.admit(simple_admission(police_custody_basis())).await.unwrap();
    let id = detainee.id;

    // Register a BailOrder warrant
    let bail_warrant = CommitmentOrder {
        id: WarrantId::new(),
        detainee_id: id,
        order_type: CommitmentOrderType::BailOrder {
            amount: Some(100_000),
            conditions: vec!["Report weekly".to_string(), "Surrender passport".to_string()],
        },
        external_reference: Some("BAIL/2025/001".to_string()),
        issuing_authority: "High Court".to_string(),
        issuing_officer: Some("Justice K".to_string()),
        date_issued: today(),
        date_received: today(),
        valid_until: Some(Utc::now().date_naive() + chrono::Duration::days(90)),
        offence_description: Some("Fraud".to_string()),
        sentence_details: None,
        is_active: true,
        registered_by: op,
        batch_id: None,
        document_hash: Some("sha256:abc123".to_string()),
    };

    let updated = reg.register_warrant(bail_warrant, op).await.unwrap();
    let active = updated.active_warrant().unwrap();
    assert!(matches!(active.order_type, CommitmentOrderType::BailOrder { .. }));
    if let CommitmentOrderType::BailOrder { amount, conditions } = &active.order_type {
        assert_eq!(*amount, Some(100_000));
        assert_eq!(conditions.len(), 2);
    }
    assert_eq!(active.document_hash.as_deref(), Some("sha256:abc123"));
}

// ===========================================================================
// Test 92: Detention basis data round-trips through JSON
// ===========================================================================

#[tokio::test]
async fn test_detention_basis_json_roundtrip() {
    let reg = setup().await;
    let op = operator();

    // Admit with ConvictedUnsentenced basis
    let basis = DetentionBasis::ConvictedUnsentenced {
        conviction_date: today(),
        sentencing_date: Some(Utc::now().date_naive() + chrono::Duration::days(14)),
        bail_status: Some(BailStatus::Denied {
            date: today(),
            reason: "Severity of offence".to_string(),
        }),
        charges: vec![
            Charge {
                id: ChargeId::new(),
                description: "Armed robbery".to_string(),
                statute: Some("Section 301".to_string()),
                severity: ChargeSeverity::Serious,
                date_of_alleged_offence: Some(NaiveDate::from_ymd_opt(2025, 1, 15).unwrap()),
                count_number: Some(1),
            },
            Charge {
                id: ChargeId::new(),
                description: "Possession of firearm".to_string(),
                statute: Some("Section 28".to_string()),
                severity: ChargeSeverity::Serious,
                date_of_alleged_offence: Some(NaiveDate::from_ymd_opt(2025, 1, 15).unwrap()),
                count_number: Some(2),
            },
        ],
    };

    // We need NoLegalBasis → ConvictedUnsentenced transition to be valid
    let no_legal = DetentionBasis::NoLegalBasis {
        discovered_date: today(),
        circumstances: "test".to_string(),
    };
    let mut admission = simple_admission(no_legal);
    admission.warrant = None;
    let detainee = reg.admit(admission).await.unwrap();

    let updated = reg
        .update_detention_basis(detainee.id, basis, op)
        .await
        .unwrap();

    if let DetentionBasis::ConvictedUnsentenced {
        sentencing_date,
        bail_status,
        charges,
        ..
    } = &updated.detention_basis
    {
        assert!(sentencing_date.is_some());
        assert!(bail_status.is_some());
        assert_eq!(charges.len(), 2);
        assert_eq!(charges[0].description, "Armed robbery");
        assert_eq!(charges[1].description, "Possession of firearm");
    } else {
        panic!("Expected ConvictedUnsentenced basis");
    }
}

// ===========================================================================
// Test 93: Search by intake date range
// ===========================================================================

#[tokio::test]
async fn test_search_intake_date_range() {
    let reg = setup().await;
    let op = operator();

    // Admit with different intake dates
    let mut a1 = simple_admission(police_custody_basis());
    a1.identity = make_identity("Old", "One", Sex::Male);
    a1.intake_date = past_date(2025, 1, 1);
    reg.admit(a1).await.unwrap();

    let mut a2 = simple_admission(police_custody_basis());
    a2.identity = make_identity("Recent", "Two", Sex::Male);
    a2.intake_date = today();
    reg.admit(a2).await.unwrap();

    // Search for only old ones
    let query = PopulationQuery {
        intake_date_range: Some(DateRange {
            from: Some(NaiveDate::from_ymd_opt(2025, 1, 1).unwrap()),
            to: Some(NaiveDate::from_ymd_opt(2025, 1, 31).unwrap()),
        }),
        ..Default::default()
    };
    let result = reg.search(query, op).await.unwrap();
    assert_eq!(result.total_matching, 1);
    assert_eq!(result.detainees[0].name, "Old, One");
}

// ===========================================================================
// Test 94: Search by sex = Other
// ===========================================================================

#[tokio::test]
async fn test_search_by_sex_other() {
    let reg = setup().await;
    let op = operator();

    let mut a1 = simple_admission(police_custody_basis());
    a1.identity = make_identity("Male", "One", Sex::Male);
    reg.admit(a1).await.unwrap();

    let mut a2 = simple_admission(police_custody_basis());
    a2.identity = make_identity("Other", "Two", Sex::Other);
    reg.admit(a2).await.unwrap();

    let query = PopulationQuery {
        sex: Some(Sex::Other),
        ..Default::default()
    };
    let result = reg.search(query, op).await.unwrap();
    assert_eq!(result.total_matching, 1);
    assert_eq!(result.detainees[0].name, "Other, Two");
}

// ===========================================================================
// Test 95: CustodyLimitExceeded hours_over value is correct
// ===========================================================================

#[tokio::test]
async fn test_flag_custody_limit_hours_over_value() {
    let reg = setup().await;

    let three_days_ago = Utc::now().date_naive() - chrono::Duration::days(3);
    let basis = DetentionBasis::PoliceCustody {
        arrest_date: past_date(2025, 1, 1),
        arresting_authority: "Police".to_string(),
        must_appear_by: three_days_ago,
        suspected_offences: None,
    };
    let mut admission = simple_admission(basis);
    admission.intake_date = past_date(2025, 1, 1);
    let detainee = reg.admit(admission).await.unwrap();

    let config = FacilityConfig::default();
    let flags = openward_registry::flags::compute_flags(&detainee, &config, &[]);

    let custody_flag = flags.iter().find(|f| matches!(f, Flag::CustodyLimitExceeded { .. }));
    assert!(custody_flag.is_some());

    if let Some(Flag::CustodyLimitExceeded { hours_over }) = custody_flag {
        // 3 days ago, so roughly 72 hours over (dates are approximate)
        assert!(*hours_over >= 48, "Expected at least 48 hours over, got {}", hours_over);
    }
}

// ===========================================================================
// Test 96: Facility status transition through all statuses
// ===========================================================================

#[tokio::test]
async fn test_facility_status_all_transitions() {
    let reg = setup().await;
    let op = operator();

    let detainee = reg.admit(simple_admission(police_custody_basis())).await.unwrap();
    let id = detainee.id;

    // Present → InCourt
    let d = reg.update_facility_status(id, FacilityStatus::InCourt, op, None).await.unwrap();
    assert_eq!(d.facility_status, FacilityStatus::InCourt);

    // InCourt → Present
    let d = reg.update_facility_status(id, FacilityStatus::Present, op, None).await.unwrap();
    assert_eq!(d.facility_status, FacilityStatus::Present);

    // Present → InHospital
    let d = reg.update_facility_status(id, FacilityStatus::InHospital, op, None).await.unwrap();
    assert_eq!(d.facility_status, FacilityStatus::InHospital);

    // InHospital → Present
    let d = reg.update_facility_status(id, FacilityStatus::Present, op, None).await.unwrap();
    assert_eq!(d.facility_status, FacilityStatus::Present);

    // Present → Escaped
    let d = reg.update_facility_status(id, FacilityStatus::Escaped, op, None).await.unwrap();
    assert_eq!(d.facility_status, FacilityStatus::Escaped);
}

// ===========================================================================
// Test 97: Warrant with no valid_until (perpetual validity)
// ===========================================================================

#[tokio::test]
async fn test_warrant_no_expiry() {
    let reg = setup().await;

    let mut admission = simple_admission(police_custody_basis());
    admission.warrant.as_mut().unwrap().valid_until = None;
    let detainee = reg.admit(admission).await.unwrap();

    let active = detainee.active_warrant().unwrap();
    assert!(active.valid_until.is_none());

    // Should NOT trigger WarrantExpired
    let config = FacilityConfig::default();
    let flags = openward_registry::flags::compute_flags(&detainee, &config, &[]);
    assert!(
        !flags.iter().any(|f| matches!(f, Flag::WarrantExpired { .. })),
        "Warrant with no valid_until should not trigger WarrantExpired"
    );
}

// ===========================================================================
// Test 98: Detainee name in search summary is "surname, given_names"
// ===========================================================================

#[tokio::test]
async fn test_search_summary_name_format() {
    let reg = setup().await;
    let op = operator();

    let mut admission = simple_admission(police_custody_basis());
    admission.identity = make_identity("O'Brien", "Mary Jane", Sex::Female);
    reg.admit(admission).await.unwrap();

    let result = reg
        .search(PopulationQuery::default(), op)
        .await
        .unwrap();
    assert_eq!(result.detainees[0].name, "O'Brien, Mary Jane");
}

// ===========================================================================
// Test 99: Overview basis breakdown sums to total
// ===========================================================================

#[tokio::test]
async fn test_overview_basis_breakdown_sums() {
    let reg = setup().await;

    // Admit different basis types
    let mut a1 = simple_admission(police_custody_basis());
    a1.identity = make_identity("A", "1", Sex::Male);
    reg.admit(a1).await.unwrap();

    let mut a2 = simple_admission(remand_basis());
    a2.identity = make_identity("B", "2", Sex::Male);
    reg.admit(a2).await.unwrap();

    let basis_nl = DetentionBasis::NoLegalBasis {
        discovered_date: today(),
        circumstances: "test".to_string(),
    };
    let mut a3 = simple_admission(basis_nl);
    a3.identity = make_identity("C", "3", Sex::Male);
    a3.warrant = None;
    reg.admit(a3).await.unwrap();

    let overview = reg.overview().await.unwrap();
    let bb = &overview.basis_breakdown;
    let sum = bb.no_legal_basis + bb.police_custody + bb.remand + bb.on_trial
        + bb.convicted_unsentenced + bb.sentenced + bb.appeal;

    assert_eq!(
        sum, overview.total_population,
        "Basis breakdown sum ({}) should equal total population ({})",
        sum, overview.total_population
    );
}

// ===========================================================================
// Test 100: Audit chain hash starts from zero hash for first entry
// ===========================================================================

#[tokio::test]
async fn test_audit_first_entry_chain_hash() {
    let reg = setup().await;

    // Trigger a single audit entry
    reg.admit(simple_admission(police_custody_basis())).await.unwrap();

    let entry: (Vec<u8>, Vec<u8>) = sqlx::query_as(
        "SELECT self_hash, chain_hash FROM audit_entries ORDER BY rowid LIMIT 1"
    )
    .fetch_one(reg.pool())
    .await
    .unwrap();

    let self_hash: [u8; 32] = entry.0.try_into().unwrap();
    let chain_hash: [u8; 32] = entry.1.try_into().unwrap();

    // First entry's chain_hash should be SHA256(zero_hash || self_hash)
    let zero_hash = [0u8; 32];
    let expected = openward_registry::audit::compute_chain_hash(&zero_hash, &self_hash);
    assert_eq!(
        chain_hash, expected,
        "First audit entry should chain from the zero hash"
    );
}

// ===========================================================================
// Test 101: Custom FacilityConfig thresholds affect flag computation
// ===========================================================================

#[tokio::test]
async fn test_custom_config_thresholds() {
    let pool = create_pool("sqlite::memory:").await.unwrap();
    apply_schema(&pool).await.unwrap();

    // Very strict config: 12h custody limit, 30 day pretrial warn
    let config = FacilityConfig {
        capacity: 100,
        police_custody_limit_hours: 12,
        prolonged_pretrial_warn_days: 30,
        prolonged_pretrial_critical_days: 60,
        court_date_imminent_hours: 72,
        release_imminent_days: 14,
        juvenile_cutoff_age: 18,
        remand_review_period_days: 30,
    };
    let reg = SqliteRegistry::new(pool, config.clone());

    // Admit with remand basis and intake 40 days ago (> 30 day threshold)
    let forty_days_ago = Utc::now().date_naive() - chrono::Duration::days(40);
    let basis = DetentionBasis::RemandAwaitingTrial {
        first_appearance_date: PastDate::from_trusted(forty_days_ago),
        next_court_date: Some(Utc::now().date_naive() + chrono::Duration::days(30)),
        remand_review_due: None,
        bail_status: BailStatus::NotApplied,
        charges: vec![Charge {
            id: ChargeId::new(),
            description: "Theft".to_string(),
            statute: None,
            severity: ChargeSeverity::Minor,
            date_of_alleged_offence: None,
            count_number: None,
        }],
    };
    let mut admission = simple_admission(basis);
    admission.intake_date = PastDate::from_trusted(forty_days_ago);
    let detainee = reg.admit(admission).await.unwrap();

    let flags = openward_registry::flags::compute_flags(&detainee, &config, &[]);

    // Should trigger ProlongedPreTrial with the 30-day threshold
    assert!(
        flags.iter().any(|f| matches!(f, Flag::ProlongedPreTrial { threshold_days: 30, .. })),
        "Expected ProlongedPreTrial with custom 30-day threshold, got: {:?}",
        flags
    );
}

// ===========================================================================
// Test 102: Court date today is imminent (boundary: 0 hours until)
// ===========================================================================

#[tokio::test]
async fn test_flag_court_date_today_is_imminent() {
    let reg = setup().await;

    let today_date = Utc::now().date_naive();
    let basis = DetentionBasis::RemandAwaitingTrial {
        first_appearance_date: today(),
        next_court_date: Some(today_date),
        remand_review_due: None,
        bail_status: BailStatus::NotApplied,
        charges: vec![Charge {
            id: ChargeId::new(),
            description: "Theft".to_string(),
            statute: None,
            severity: ChargeSeverity::Minor,
            date_of_alleged_offence: None,
            count_number: None,
        }],
    };

    let admission = simple_admission(basis);
    let detainee = reg.admit(admission).await.unwrap();

    let config = FacilityConfig::default();
    let flags = openward_registry::flags::compute_flags(&detainee, &config, &[]);

    assert!(
        flags.iter().any(|f| matches!(f, Flag::CourtDateImminent { .. })),
        "A court date today (0 hours until) should be flagged as imminent, got: {:?}",
        flags
    );
}

// ===========================================================================
// Test 103: Multiple flags on single detainee
// ===========================================================================

#[tokio::test]
async fn test_multiple_flags_on_single_detainee() {
    let reg = setup().await;

    // NoLegalBasis detainee with no representation → should get multiple flags
    let basis = DetentionBasis::NoLegalBasis {
        discovered_date: past_date(2024, 6, 1),
        circumstances: "Found in facility without warrant".to_string(),
    };
    let mut admission = simple_admission(basis);
    admission.warrant = None;
    admission.legal_representation = None;
    admission.intake_date = past_date(2024, 6, 1);
    let detainee = reg.admit(admission).await.unwrap();

    let config = FacilityConfig::default();
    let flags = openward_registry::flags::compute_flags(&detainee, &config, &[]);

    // Should have at least NoLegalBasis and NoLegalRepresentation
    assert!(flags.iter().any(|f| matches!(f, Flag::NoLegalBasis { .. })));
    assert!(flags.iter().any(|f| matches!(f, Flag::NoLegalRepresentation)));
    assert!(flags.len() >= 2, "Expected at least 2 flags, got {}", flags.len());
}

// ===========================================================================
// Test 104: Search default sort (by intake_date DESC)
// ===========================================================================

#[tokio::test]
async fn test_search_default_sort() {
    let reg = setup().await;
    let op = operator();

    let mut a1 = simple_admission(police_custody_basis());
    a1.identity = make_identity("First", "A", Sex::Male);
    a1.intake_date = past_date(2025, 1, 1);
    reg.admit(a1).await.unwrap();

    let mut a2 = simple_admission(police_custody_basis());
    a2.identity = make_identity("Second", "B", Sex::Male);
    a2.intake_date = past_date(2025, 6, 1);
    reg.admit(a2).await.unwrap();

    // Default sort is IntakeDate DESC (most recent first)
    let result = reg
        .search(PopulationQuery::default(), op)
        .await
        .unwrap();
    assert_eq!(result.detainees.len(), 2);
    assert_eq!(result.detainees[0].name, "Second, B"); // June comes first (DESC)
    assert_eq!(result.detainees[1].name, "First, A");
}

// ===========================================================================
// Test 105: Assign housing to non-existent detainee
// ===========================================================================

#[tokio::test]
async fn test_release_for_non_existent_detainee() {
    let reg = setup().await;
    let op = operator();

    let release = ReleaseRecord {
        detainee_id: DetaineeId::new(),
        release_date: today(),
        release_type: ReleaseType::CourtOrdered,
        authorized_by: op,
        all_property_returned: true,
        notes: None,
    };
    let result = reg.release(release).await;
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        RegistryError::Domain(DomainError::DetaineeNotFound { .. })
    ));
}

// ===========================================================================
// Test 106: DaysHeld sort reversal works correctly
// ===========================================================================

#[tokio::test]
async fn test_search_sort_by_days_held_ascending() {
    let reg = setup().await;
    let op = operator();

    // "Longest" held — earlier intake date = more days held
    let mut a1 = simple_admission(police_custody_basis());
    a1.identity = make_identity("Longest", "Held", Sex::Male);
    a1.intake_date = past_date(2024, 1, 1);
    reg.admit(a1).await.unwrap();

    let mut a2 = simple_admission(police_custody_basis());
    a2.identity = make_identity("Shortest", "Held", Sex::Male);
    a2.intake_date = today();
    reg.admit(a2).await.unwrap();

    // Sort by DaysHeld ascending = least days held first
    let query = PopulationQuery {
        sort_by: Some(SortField::DaysHeld),
        sort_order: Some(SortOrder::Asc),
        ..Default::default()
    };
    let result = reg.search(query, op).await.unwrap();

    // DaysHeld ASC means the one admitted today (fewest days) comes first
    assert_eq!(result.detainees[0].name, "Shortest, Held");
    assert_eq!(result.detainees[1].name, "Longest, Held");
}

// ===========================================================================
// Test 107: Search by NoLegalBasis filter
// ===========================================================================

#[tokio::test]
async fn test_search_by_no_legal_basis() {
    let reg = setup().await;
    let op = operator();

    let basis = DetentionBasis::NoLegalBasis {
        discovered_date: today(),
        circumstances: "test".to_string(),
    };
    let mut a1 = simple_admission(basis);
    a1.identity = make_identity("NLB", "One", Sex::Male);
    a1.warrant = None;
    reg.admit(a1).await.unwrap();

    let mut a2 = simple_admission(police_custody_basis());
    a2.identity = make_identity("PC", "Two", Sex::Male);
    reg.admit(a2).await.unwrap();

    let query = PopulationQuery {
        detention_basis: Some(DetentionBasisFilter::NoLegalBasis),
        ..Default::default()
    };
    let result = reg.search(query, op).await.unwrap();
    assert_eq!(result.total_matching, 1);
    assert_eq!(result.detainees[0].name, "NLB, One");
}

// ===========================================================================
// Test 108: Batch admission preserves individual identity fields
// ===========================================================================

#[tokio::test]
async fn test_batch_admission_individual_identities() {
    let reg = setup().await;
    let op = operator();
    let batch_id = BatchId::new();

    let batch = BatchAdmission {
        batch_id,
        shared_warrant: SharedWarrantData {
            order_type: CommitmentOrderType::RemandOrder,
            external_reference: None,
            issuing_authority: "Court".to_string(),
            issuing_officer: None,
            date_issued: today(),
            date_received: today(),
            valid_until: None,
            offence_description: None,
        },
        detainees: vec![
            BatchDetaineeEntry {
                identity: make_identity("Phiri", "James", Sex::Male),
                detention_basis: remand_basis(),
                additional_charges: Vec::new(),
                intake_medical_notes: Some("Healthy".to_string()),
                legal_reference: None,
                emergency_contacts: vec![EmergencyContact {
                    name: "Phiri Sr".to_string(),
                    relationship: "Father".to_string(),
                    phone: None,
                    address: None,
                }],
                property: Vec::new(),
                legal_representation: None,
                notes: Some("First detainee note".to_string()),
            },
            BatchDetaineeEntry {
                identity: make_identity("Mwanza", "Grace", Sex::Female),
                detention_basis: remand_basis(),
                additional_charges: Vec::new(),
                intake_medical_notes: None,
                legal_reference: None,
                emergency_contacts: Vec::new(),
                property: Vec::new(),
                legal_representation: None,
                notes: None,
            },
        ],
        intake_date: today(),
        admitted_by: op,
    };

    let detainees = reg.batch_admit(batch).await.unwrap();
    assert_eq!(detainees.len(), 2);

    // Check Phiri
    let phiri = detainees.iter().find(|d| d.identity.surname == "Phiri").unwrap();
    assert!(matches!(phiri.identity.sex, Sex::Male));
    assert_eq!(phiri.emergency_contacts.len(), 1);

    // Check Mwanza
    let mwanza = detainees.iter().find(|d| d.identity.surname == "Mwanza").unwrap();
    assert!(matches!(mwanza.identity.sex, Sex::Female));
    assert!(mwanza.emergency_contacts.is_empty());

    // Both should have distinct IDs
    assert_ne!(phiri.id, mwanza.id);
}

// ===========================================================================
// Test 109: Search combined filters (basis + sex)
// ===========================================================================

#[tokio::test]
async fn test_search_combined_filters() {
    let reg = setup().await;
    let op = operator();

    // 1 male remand, 1 female remand, 1 male police custody
    let mut a1 = simple_admission(remand_basis());
    a1.identity = make_identity("MaleRemand", "One", Sex::Male);
    reg.admit(a1).await.unwrap();

    let mut a2 = simple_admission(remand_basis());
    a2.identity = make_identity("FemaleRemand", "Two", Sex::Female);
    reg.admit(a2).await.unwrap();

    let mut a3 = simple_admission(police_custody_basis());
    a3.identity = make_identity("MalePolice", "Three", Sex::Male);
    reg.admit(a3).await.unwrap();

    // Search: Remand + Male → should get only 1
    let query = PopulationQuery {
        detention_basis: Some(DetentionBasisFilter::RemandAwaitingTrial),
        sex: Some(Sex::Male),
        ..Default::default()
    };
    let result = reg.search(query, op).await.unwrap();
    assert_eq!(result.total_matching, 1);
    assert_eq!(result.detainees[0].name, "MaleRemand, One");
}

// ===========================================================================
// Test 110: Search by OnTrial filter
// ===========================================================================

#[tokio::test]
async fn test_search_by_on_trial() {
    let reg = setup().await;
    let op = operator();

    let detainee = reg.admit(simple_admission(remand_basis())).await.unwrap();

    let on_trial = DetentionBasis::OnTrial {
        trial_start_date: today(),
        next_court_date: Some(Utc::now().date_naive() + chrono::Duration::days(14)),
        bail_status: BailStatus::NotApplied,
        charges: vec![Charge {
            id: ChargeId::new(),
            description: "Theft".to_string(),
            statute: None,
            severity: ChargeSeverity::Moderate,
            date_of_alleged_offence: None,
            count_number: None,
        }],
    };
    reg.update_detention_basis(detainee.id, on_trial, op)
        .await
        .unwrap();

    // Also admit a non-OnTrial
    let mut a2 = simple_admission(police_custody_basis());
    a2.identity = make_identity("Not", "OnTrial", Sex::Male);
    reg.admit(a2).await.unwrap();

    let query = PopulationQuery {
        detention_basis: Some(DetentionBasisFilter::OnTrial),
        ..Default::default()
    };
    let result = reg.search(query, op).await.unwrap();
    assert_eq!(result.total_matching, 1);
    assert_eq!(result.detainees[0].detention_basis, DetentionBasisLabel::OnTrial);
}

// ===========================================================================
// Overview: SexBreakdown
// ===========================================================================

#[tokio::test]
async fn test_overview_sex_breakdown() {
    let reg = setup().await;

    // 2 males, 1 female, 1 other
    for (surname, sex) in [
        ("A", Sex::Male),
        ("B", Sex::Male),
        ("C", Sex::Female),
        ("D", Sex::Other),
    ] {
        let mut a = simple_admission(remand_basis());
        a.identity = make_identity(surname, "X", sex);
        reg.admit(a).await.unwrap();
    }

    let overview = reg.overview().await.unwrap();
    assert_eq!(overview.sex_breakdown.male, 2);
    assert_eq!(overview.sex_breakdown.female, 1);
    assert_eq!(overview.sex_breakdown.other, 1);
}

// ===========================================================================
// Overview: TimeDistribution
// ===========================================================================

#[tokio::test]
async fn test_overview_time_distribution() {
    let reg = setup().await;
    let today_naive = Utc::now().date_naive();

    // Admit detainees with specific intake dates to land in known buckets:
    // - under_48h: intake today (0 days)
    // - under_1_week: intake 3 days ago
    // - under_1_month: intake 10 days ago
    // - under_3_months: intake 60 days ago
    // - under_6_months: intake 120 days ago
    let intake_days_ago = [0i64, 3, 10, 60, 120];

    for (i, days_ago) in intake_days_ago.iter().enumerate() {
        let intake = today_naive - chrono::Duration::days(*days_ago);
        let mut a = simple_admission(remand_basis());
        a.identity = make_identity(&format!("D{}", i), "X", Sex::Male);
        a.intake_date = PastDate::from_trusted(intake);
        reg.admit(a).await.unwrap();
    }

    let overview = reg.overview().await.unwrap();
    let td = &overview.time_distribution;

    assert_eq!(td.under_48_hours, 1, "under_48h");
    assert_eq!(td.under_1_week, 1, "under_1_week");
    assert_eq!(td.under_1_month, 1, "under_1_month");
    assert_eq!(td.under_3_months, 1, "under_3_months");
    assert_eq!(td.under_6_months, 1, "under_6_months");
    assert_eq!(td.under_1_year, 0);
    assert_eq!(td.under_2_years, 0);
    assert_eq!(td.over_2_years, 0);

    // Median of [0, 3, 10, 60, 120] = 10
    assert_eq!(td.median_days, 10);
    // Mean = (0+3+10+60+120)/5 = 38.6
    assert!((td.mean_days - 38.6).abs() < 0.1);
}

// ===========================================================================
// Overview: FlagCounts
// ===========================================================================

#[tokio::test]
async fn test_overview_flag_counts() {
    let reg = setup().await;

    // 1) NoLegalBasis detainee
    let no_basis = DetentionBasis::NoLegalBasis {
        discovered_date: today(),
        circumstances: "Unknown".to_string(),
    };
    let mut a1 = simple_admission(no_basis);
    a1.warrant = None;
    a1.identity = make_identity("NoBasis", "X", Sex::Male);
    reg.admit(a1).await.unwrap();

    // 2) Bail granted but still held
    let bail_basis = DetentionBasis::RemandAwaitingTrial {
        first_appearance_date: today(),
        next_court_date: Some(Utc::now().date_naive() + chrono::Duration::days(30)),
        remand_review_due: None,
        bail_status: BailStatus::Granted {
            date: today(),
            amount: Some(10_000),
            conditions: vec![],
        },
        charges: vec![Charge {
            id: ChargeId::new(),
            description: "Theft".to_string(),
            statute: None,
            severity: ChargeSeverity::Minor,
            date_of_alleged_offence: None,
            count_number: None,
        }],
    };
    let mut a2 = simple_admission(bail_basis);
    a2.identity = make_identity("BailHeld", "X", Sex::Male);
    reg.admit(a2).await.unwrap();

    let overview = reg.overview().await.unwrap();

    assert_eq!(overview.flag_counts.no_legal_basis, 1);
    assert_eq!(overview.flag_counts.bail_granted_still_held, 1);
    // Both detainees have no legal representation
    assert_eq!(overview.flag_counts.no_legal_representation, 2);
}

// ===========================================================================
// Overview: CriticalNumbers
// ===========================================================================

#[tokio::test]
async fn test_overview_critical_numbers() {
    let reg = setup().await;

    // NoLegalBasis detainee → critical.no_legal_basis
    let no_basis = DetentionBasis::NoLegalBasis {
        discovered_date: today(),
        circumstances: "Unknown".to_string(),
    };
    let mut a1 = simple_admission(no_basis);
    a1.warrant = None;
    a1.identity = make_identity("NoBasis", "X", Sex::Male);
    reg.admit(a1).await.unwrap();

    // BailGrantedStillHeld → critical.bail_granted_still_held
    let bail_basis = DetentionBasis::RemandAwaitingTrial {
        first_appearance_date: today(),
        next_court_date: Some(Utc::now().date_naive() + chrono::Duration::days(30)),
        remand_review_due: None,
        bail_status: BailStatus::Granted {
            date: today(),
            amount: Some(10_000),
            conditions: vec![],
        },
        charges: vec![Charge {
            id: ChargeId::new(),
            description: "Theft".to_string(),
            statute: None,
            severity: ChargeSeverity::Minor,
            date_of_alleged_offence: None,
            count_number: None,
        }],
    };
    let mut a2 = simple_admission(bail_basis);
    a2.identity = make_identity("BailHeld", "X", Sex::Male);
    reg.admit(a2).await.unwrap();

    let overview = reg.overview().await.unwrap();

    assert_eq!(overview.critical.no_legal_basis, 1);
    assert_eq!(overview.critical.bail_granted_still_held, 1);
}

// ===========================================================================
// Overview: DailyCountStatus — NotStarted (no row for today)
// ===========================================================================

#[tokio::test]
async fn test_overview_daily_count_status_not_started() {
    let reg = setup().await;

    // No daily count opened → NotStarted
    let overview = reg.overview().await.unwrap();
    assert!(matches!(overview.today_count_status, DailyCountStatus::NotStarted));
}

// ===========================================================================
// Overview: DailyCountStatus — Open
// ===========================================================================

#[tokio::test]
async fn test_overview_daily_count_status_open() {
    let reg = setup().await;
    let op = operator();

    // Admit someone so the count is non-trivial
    let mut a = simple_admission(remand_basis());
    a.identity = make_identity("Open", "Count", Sex::Male);
    reg.admit(a).await.unwrap();

    // Open daily count for today
    let today_naive = Utc::now().date_naive();
    reg.open_daily_count(today_naive, op).await.unwrap();

    let overview = reg.overview().await.unwrap();
    match overview.today_count_status {
        DailyCountStatus::Open { computed_closing } => {
            assert!(computed_closing > 0);
        }
        other => panic!("expected Open, got {:?}", other),
    }
}

// ===========================================================================
// Overview: DailyCountStatus — Finalized
// ===========================================================================

#[tokio::test]
async fn test_overview_daily_count_status_finalized() {
    let reg = setup().await;
    let op = operator();

    // Admit someone
    let mut a = simple_admission(remand_basis());
    a.identity = make_identity("Final", "Count", Sex::Male);
    reg.admit(a).await.unwrap();

    let today_naive = Utc::now().date_naive();
    reg.open_daily_count(today_naive, op).await.unwrap();
    reg.finalize_daily_count(today_naive, op).await.unwrap();

    let overview = reg.overview().await.unwrap();
    match overview.today_count_status {
        DailyCountStatus::Finalized { closing, .. } => {
            assert!(closing > 0);
        }
        other => panic!("expected Finalized, got {:?}", other),
    }
}

// ===========================================================================
// Search: Flags and Statistics
// ===========================================================================

#[tokio::test]
async fn test_search_returns_flags() {
    let reg = setup().await;
    let op = operator();

    // Admit with no legal representation → NoLegalRepresentation flag
    let a = simple_admission(police_custody_basis());
    reg.admit(a).await.unwrap();

    let result = reg
        .search(PopulationQuery::default(), op)
        .await
        .unwrap();
    assert_eq!(result.detainees.len(), 1);
    let flags = &result.detainees[0].flags;
    assert!(
        !flags.is_empty(),
        "Expected at least one flag on the detainee"
    );
    assert!(
        flags.iter().any(|f| matches!(f, Flag::NoLegalRepresentation)),
        "Expected NoLegalRepresentation flag, got: {:?}",
        flags
    );
}

#[tokio::test]
async fn test_search_statistics_populated() {
    let reg = setup().await;
    let op = operator();

    // Admit two detainees with different bases
    let a1 = simple_admission(police_custody_basis());
    reg.admit(a1).await.unwrap();

    let mut a2 = simple_admission(remand_basis());
    a2.identity = make_identity("Phiri", "Grace", Sex::Female);
    reg.admit(a2).await.unwrap();

    let result = reg
        .search(PopulationQuery::default(), op)
        .await
        .unwrap();

    let stats = &result.statistics;
    assert_eq!(stats.total, 2);
    assert_eq!(stats.by_detention_basis.police_custody, 1);
    assert_eq!(stats.by_detention_basis.remand, 1);
    assert_eq!(stats.by_sex.male, 1);
    assert_eq!(stats.by_sex.female, 1);
    assert_eq!(stats.without_legal_representation, 2);
    assert!(stats.flag_counts.no_legal_representation >= 2);
    assert!(stats.facility_capacity > 0);
}

#[tokio::test]
async fn test_search_statistics_reflect_full_result_set() {
    let reg = setup().await;
    let op = operator();

    // Admit 5 detainees
    for i in 0..5 {
        let mut a = simple_admission(police_custody_basis());
        a.identity = make_identity(&format!("Surname{}", i), "Test", Sex::Male);
        reg.admit(a).await.unwrap();
    }

    // Search with limit=2 (paginated)
    let query = PopulationQuery {
        limit: Some(2),
        ..Default::default()
    };
    let result = reg.search(query, op).await.unwrap();

    // Page has only 2 results
    assert_eq!(result.detainees.len(), 2);
    // But total_matching covers all 5
    assert_eq!(result.total_matching, 5);
    // Statistics must reflect ALL 5, not just the page
    assert_eq!(result.statistics.total, 5);
    assert_eq!(result.statistics.by_sex.male, 5);
    assert_eq!(result.statistics.by_detention_basis.police_custody, 5);
    assert_eq!(result.statistics.without_legal_representation, 5);
}
