use chrono::{NaiveDate, Utc};
use sqlx::SqlitePool;

use openward_core::*;

use crate::audit;
use crate::flags::{FacilityConfig};

/// SQLite-backed implementation of the Registry trait.
pub struct SqliteRegistry {
    pool: SqlitePool,
    config: FacilityConfig,
}

impl SqliteRegistry {
    pub fn new(pool: SqlitePool, config: FacilityConfig) -> Self {
        Self { pool, config }
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    pub fn config(&self) -> &FacilityConfig {
        &self.config
    }

    // === Internal helpers ===

    /// Load a full Detainee from the database by ID.
    async fn load_detainee(&self, id: DetaineeId) -> Result<Detainee, RegistryError> {
        let id_str = id.to_string();

        // Load the main detainee row
        let row: Option<DetaineeRow> = sqlx::query_as(
            "SELECT id, surname, given_names, preferred_name, sex, date_of_birth, \
             nationality, national_id, detention_basis_label, detention_basis_data, \
             facility_status, intake_date, housing_unit_id, identity_extra, \
             legal_reference, legal_representation, emergency_contacts \
             FROM detainees WHERE id = ?"
        )
        .bind(&id_str)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| RegistryError::Database(e.to_string()))?;

        let row = row.ok_or_else(|| RegistryError::Domain(DomainError::DetaineeNotFound { id }))?;

        // Load warrants
        let warrant_rows: Vec<WarrantRow> = sqlx::query_as(
            "SELECT id, detainee_id, order_type, order_data, external_reference, \
             issuing_authority, issuing_officer, date_issued, date_received, valid_until, \
             offence_description, sentence_details, is_active, registered_by, batch_id, \
             document_hash \
             FROM commitment_orders WHERE detainee_id = ? ORDER BY created_at"
        )
        .bind(&id_str)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RegistryError::Database(e.to_string()))?;

        // Load property items
        let property_rows: Vec<PropertyRow> = sqlx::query_as(
            "SELECT description, quantity, logged_date, logged_by, returned \
             FROM property_items WHERE detainee_id = ?"
        )
        .bind(&id_str)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RegistryError::Database(e.to_string()))?;

        // Load notes
        let note_rows: Vec<NoteRow> = sqlx::query_as(
            "SELECT content, author, timestamp FROM notes WHERE detainee_id = ? ORDER BY timestamp"
        )
        .bind(&id_str)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RegistryError::Database(e.to_string()))?;

        // Load aliases
        let aliases: Vec<(String,)> = sqlx::query_as(
            "SELECT alias FROM detainee_aliases WHERE detainee_id = ?"
        )
        .bind(&id_str)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RegistryError::Database(e.to_string()))?;

        // Reconstruct the Detainee from rows
        row_to_detainee(row, warrant_rows, property_rows, note_rows, aliases)
    }
}

// === Row types for sqlx query_as ===

#[derive(sqlx::FromRow)]
struct DetaineeRow {
    id: String,
    surname: String,
    given_names: String,
    preferred_name: Option<String>,
    sex: String,
    date_of_birth: Option<String>,
    nationality: Option<String>,
    national_id: Option<String>,
    detention_basis_label: String,
    detention_basis_data: String,
    facility_status: String,
    intake_date: String,
    housing_unit_id: Option<String>,
    identity_extra: String,
    legal_reference: Option<String>,
    legal_representation: Option<String>,
    emergency_contacts: String,
}

#[derive(sqlx::FromRow)]
struct WarrantRow {
    id: String,
    detainee_id: String,
    #[allow(dead_code)]
    order_type: String,
    order_data: String,
    external_reference: Option<String>,
    issuing_authority: String,
    issuing_officer: Option<String>,
    date_issued: String,
    date_received: String,
    valid_until: Option<String>,
    offence_description: Option<String>,
    sentence_details: Option<String>,
    is_active: i32,
    registered_by: String,
    batch_id: Option<String>,
    document_hash: Option<String>,
}

#[derive(sqlx::FromRow)]
struct PropertyRow {
    description: String,
    quantity: i32,
    logged_date: String,
    logged_by: String,
    returned: i32,
}

#[derive(sqlx::FromRow)]
struct NoteRow {
    content: String,
    author: String,
    timestamp: String,
}

#[derive(sqlx::FromRow)]
struct DailyCountRow {
    id: String,
    date: String,
    opening_count: i64,
    admissions: i64,
    transfers_in: i64,
    court_returns: i64,
    hospital_returns: i64,
    releases: i64,
    transfers_out: i64,
    to_court: i64,
    to_hospital: i64,
    escapes: i64,
    deaths: i64,
    computed_closing: i64,
    actual_closing_count: Option<i64>,
    is_balanced: Option<i64>,
    discrepancy_note: Option<String>,
    counted_by: Option<String>,
    finalized_by: Option<String>,
    finalized_at: Option<String>,
}

// === Row → Domain type conversions ===

fn parse_uuid(s: &str) -> Result<uuid::Uuid, RegistryError> {
    uuid::Uuid::parse_str(s).map_err(|e| RegistryError::Database(format!("invalid UUID: {}", e)))
}

fn parse_date(s: &str) -> Result<NaiveDate, RegistryError> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .map_err(|e| RegistryError::Database(format!("invalid date '{}': {}", s, e)))
}

fn parse_sex(s: &str) -> Sex {
    match s {
        "Male" => Sex::Male,
        "Female" => Sex::Female,
        _ => Sex::Other,
    }
}

fn parse_facility_status(s: &str) -> FacilityStatus {
    match s {
        "Present" => FacilityStatus::Present,
        "InCourt" => FacilityStatus::InCourt,
        "InHospital" => FacilityStatus::InHospital,
        "Transferred" => FacilityStatus::Transferred,
        "Released" => FacilityStatus::Released,
        "Escaped" => FacilityStatus::Escaped,
        "Deceased" => FacilityStatus::Deceased,
        _ => FacilityStatus::Present,
    }
}

fn row_to_detainee(
    row: DetaineeRow,
    warrant_rows: Vec<WarrantRow>,
    property_rows: Vec<PropertyRow>,
    note_rows: Vec<NoteRow>,
    alias_rows: Vec<(String,)>,
) -> Result<Detainee, RegistryError> {
    let id = DetaineeId::from_uuid(parse_uuid(&row.id)?);

    // Parse identity_extra JSON for additional identity fields
    let extra: serde_json::Value = serde_json::from_str(&row.identity_extra)
        .unwrap_or_default();

    let identity = Identity {
        surname: row.surname,
        given_names: row.given_names,
        preferred_name: row.preferred_name,
        aliases: alias_rows.into_iter().map(|(a,)| a).collect(),
        date_of_birth: row.date_of_birth.as_deref().and_then(|s| parse_date(s).ok()),
        estimated_age_at_intake: extra.get("estimated_age_at_intake")
            .and_then(|v| v.as_u64())
            .map(|v| v as u32),
        estimated_age_date: extra.get("estimated_age_date")
            .and_then(|v| v.as_str())
            .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
            .map(PastDate::from_trusted),
        sex: parse_sex(&row.sex),
        nationality: row.nationality,
        national_id: row.national_id,
        languages: extra.get("languages")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default(),
        photo_hash: extra.get("photo_hash")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
    };

    let detention_basis: DetentionBasis = serde_json::from_str(&row.detention_basis_data)
        .map_err(|e| RegistryError::Database(format!("invalid detention basis JSON: {}", e)))?;

    let facility_status = parse_facility_status(&row.facility_status);
    let intake_date = PastDate::from_trusted(parse_date(&row.intake_date)?);
    let housing_unit = row.housing_unit_id
        .as_deref()
        .map(|s| parse_uuid(s).map(HousingUnitId::from_uuid))
        .transpose()?;

    let legal_reference: Option<LegalReference> = row.legal_reference
        .as_deref()
        .map(|s| serde_json::from_str(s))
        .transpose()
        .map_err(|e| RegistryError::Database(format!("invalid legal_reference JSON: {}", e)))?;

    let legal_representation: Option<LegalRepresentation> = row.legal_representation
        .as_deref()
        .map(|s| serde_json::from_str(s))
        .transpose()
        .map_err(|e| RegistryError::Database(format!("invalid legal_representation JSON: {}", e)))?;

    let emergency_contacts: Vec<EmergencyContact> = serde_json::from_str(&row.emergency_contacts)
        .unwrap_or_default();

    // Parse warrants
    let warrants: Vec<CommitmentOrder> = warrant_rows
        .into_iter()
        .map(|wr| row_to_warrant(wr))
        .collect::<Result<Vec<_>, _>>()?;

    // Parse property
    let property: Vec<PropertyItem> = property_rows
        .into_iter()
        .map(|pr| {
            Ok(PropertyItem {
                description: pr.description,
                quantity: pr.quantity as u32,
                logged_date: PastDate::from_trusted(parse_date(&pr.logged_date)?),
                logged_by: OperatorId::from_uuid(parse_uuid(&pr.logged_by)?),
                returned: pr.returned != 0,
            })
        })
        .collect::<Result<Vec<_>, RegistryError>>()?;

    // Parse notes
    let notes: Vec<Note> = note_rows
        .into_iter()
        .map(|nr| {
            let ts = chrono::DateTime::parse_from_rfc3339(&nr.timestamp)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now());
            Ok(Note {
                content: nr.content,
                author: OperatorId::from_uuid(parse_uuid(&nr.author)?),
                timestamp: ts,
            })
        })
        .collect::<Result<Vec<_>, RegistryError>>()?;

    Ok(Detainee {
        id,
        identity,
        detention_basis,
        facility_status,
        intake_date,
        housing_unit,
        warrants,
        legal_reference,
        emergency_contacts,
        property,
        legal_representation,
        notes,
    })
}

fn row_to_warrant(row: WarrantRow) -> Result<CommitmentOrder, RegistryError> {
    let order_type: CommitmentOrderType = serde_json::from_str(&row.order_data)
        .map_err(|e| RegistryError::Database(format!("invalid order_type JSON: {}", e)))?;

    let sentence_details: Option<WarrantSentenceDetails> = row.sentence_details
        .as_deref()
        .map(|s| serde_json::from_str(s))
        .transpose()
        .map_err(|e| RegistryError::Database(format!("invalid sentence_details JSON: {}", e)))?;

    Ok(CommitmentOrder {
        id: WarrantId::from_uuid(parse_uuid(&row.id)?),
        detainee_id: DetaineeId::from_uuid(parse_uuid(&row.detainee_id)?),
        order_type,
        external_reference: row.external_reference,
        issuing_authority: row.issuing_authority,
        issuing_officer: row.issuing_officer,
        date_issued: PastDate::from_trusted(parse_date(&row.date_issued)?),
        date_received: PastDate::from_trusted(parse_date(&row.date_received)?),
        valid_until: row.valid_until.as_deref().and_then(|s| parse_date(s).ok()),
        offence_description: row.offence_description,
        sentence_details,
        is_active: row.is_active != 0,
        registered_by: OperatorId::from_uuid(parse_uuid(&row.registered_by)?),
        batch_id: row.batch_id
            .as_deref()
            .map(|s| parse_uuid(s).map(BatchId::from_uuid))
            .transpose()?,
        document_hash: row.document_hash,
    })
}

// === Registry trait implementation ===

impl Registry for SqliteRegistry {
    async fn admit(&self, mut record: AdmissionRecord) -> Result<Detainee, RegistryError> {
        let detainee_id = DetaineeId::new();
        let now = Utc::now();
        let now_str = now.to_rfc3339();

        // Serialize complex fields
        let basis_label = record.detention_basis.label().to_string();
        let basis_data = serde_json::to_string(&record.detention_basis)
            .map_err(|e| RegistryError::Database(e.to_string()))?;
        let sex_str = format!("{:?}", record.identity.sex);
        let dob_str = record.identity.date_of_birth.map(|d| d.to_string());
        let intake_str = record.intake_date.as_naive().to_string();
        let id_str = detainee_id.to_string();

        let identity_extra = serde_json::json!({
            "estimated_age_at_intake": record.identity.estimated_age_at_intake,
            "estimated_age_date": record.identity.estimated_age_date.map(|d| d.as_naive().to_string()),
            "languages": record.identity.languages,
            "photo_hash": record.identity.photo_hash,
        });
        let extra_str = identity_extra.to_string();

        let legal_ref_str = record.legal_reference.as_ref()
            .map(|lr| serde_json::to_string(lr).unwrap_or_default());
        let legal_rep_str = record.legal_representation.as_ref()
            .map(|lr| serde_json::to_string(lr).unwrap_or_default());
        let contacts_str = serde_json::to_string(&record.emergency_contacts)
            .unwrap_or_else(|_| "[]".to_string());

        let mut tx = self.pool.begin().await
            .map_err(|e| RegistryError::Database(e.to_string()))?;

        // Insert detainee
        sqlx::query(
            "INSERT INTO detainees (id, surname, given_names, preferred_name, sex, date_of_birth, \
             nationality, national_id, detention_basis_label, detention_basis_data, \
             facility_status, intake_date, identity_extra, legal_reference, \
             legal_representation, emergency_contacts, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'Present', ?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(&id_str)
        .bind(&record.identity.surname)
        .bind(&record.identity.given_names)
        .bind(&record.identity.preferred_name)
        .bind(&sex_str)
        .bind(&dob_str)
        .bind(&record.identity.nationality)
        .bind(&record.identity.national_id)
        .bind(&basis_label)
        .bind(&basis_data)
        .bind(&intake_str)
        .bind(&extra_str)
        .bind(&legal_ref_str)
        .bind(&legal_rep_str)
        .bind(&contacts_str)
        .bind(&now_str)
        .bind(&now_str)
        .execute(&mut *tx)
        .await
        .map_err(|e| RegistryError::Database(e.to_string()))?;

        // Insert aliases
        for alias in &record.identity.aliases {
            sqlx::query("INSERT INTO detainee_aliases (detainee_id, alias) VALUES (?, ?)")
                .bind(&id_str)
                .bind(alias)
                .execute(&mut *tx)
                .await
                .map_err(|e| RegistryError::Database(e.to_string()))?;
        }

        // Insert warrant if provided
        if let Some(ref mut warrant) = record.warrant {
            warrant.detainee_id = detainee_id;
            warrant.is_active = true;
            insert_warrant(&mut tx, warrant).await?;
        }

        // Insert property items
        for item in &record.property {
            sqlx::query(
                "INSERT INTO property_items (detainee_id, description, quantity, logged_date, logged_by, returned) \
                 VALUES (?, ?, ?, ?, ?, 0)"
            )
            .bind(&id_str)
            .bind(&item.description)
            .bind(item.quantity as i32)
            .bind(item.logged_date.as_naive().to_string())
            .bind(item.logged_by.as_uuid().to_string())
            .execute(&mut *tx)
            .await
            .map_err(|e| RegistryError::Database(e.to_string()))?;
        }

        // Insert intake note if provided
        if let Some(note_text) = &record.notes {
            sqlx::query(
                "INSERT INTO notes (detainee_id, content, author, timestamp) VALUES (?, ?, ?, ?)"
            )
            .bind(&id_str)
            .bind(note_text)
            .bind(record.admitted_by.as_uuid().to_string())
            .bind(&now_str)
            .execute(&mut *tx)
            .await
            .map_err(|e| RegistryError::Database(e.to_string()))?;
        }

        // Insert medical intake note as a note if provided
        if let Some(medical_notes) = &record.intake_medical_notes {
            let medical_note = format!("[INTAKE MEDICAL] {}", medical_notes);
            sqlx::query(
                "INSERT INTO notes (detainee_id, content, author, timestamp) VALUES (?, ?, ?, ?)"
            )
            .bind(&id_str)
            .bind(&medical_note)
            .bind(record.admitted_by.as_uuid().to_string())
            .bind(&now_str)
            .execute(&mut *tx)
            .await
            .map_err(|e| RegistryError::Database(e.to_string()))?;
        }

        tx.commit().await
            .map_err(|e| RegistryError::Database(e.to_string()))?;

        // Create audit entry
        let after = serde_json::to_value(&record)
            .map_err(|e| RegistryError::Database(e.to_string()))?;
        let _ = audit::create_audit_entry(
            &self.pool,
            record.admitted_by,
            ModuleName::Registry,
            "admit",
            Some(detainee_id),
            None,
            Some(after),
        )
        .await;

        self.load_detainee(detainee_id).await
    }

    async fn batch_admit(&self, batch: BatchAdmission) -> Result<Vec<Detainee>, RegistryError> {
        let mut results = Vec::new();

        // Record the batch
        let batch_id_str = batch.batch_id.as_uuid().to_string();
        let shared_warrant_json = serde_json::to_string(&batch.shared_warrant)
            .map_err(|e| RegistryError::Database(e.to_string()))?;
        let now_str = Utc::now().to_rfc3339();

        sqlx::query(
            "INSERT INTO batch_admissions (id, shared_warrant, intake_date, admitted_by, detainee_count, created_at) \
             VALUES (?, ?, ?, ?, ?, ?)"
        )
        .bind(&batch_id_str)
        .bind(&shared_warrant_json)
        .bind(batch.intake_date.as_naive().to_string())
        .bind(batch.admitted_by.as_uuid().to_string())
        .bind(batch.detainees.len() as i32)
        .bind(&now_str)
        .execute(&self.pool)
        .await
        .map_err(|e| RegistryError::Database(e.to_string()))?;

        // Admit each detainee
        for entry in batch.detainees {
            let warrant = CommitmentOrder {
                id: WarrantId::new(),
                detainee_id: DetaineeId::new(), // placeholder, overwritten in admit()
                order_type: batch.shared_warrant.order_type.clone(),
                external_reference: batch.shared_warrant.external_reference.clone(),
                issuing_authority: batch.shared_warrant.issuing_authority.clone(),
                issuing_officer: batch.shared_warrant.issuing_officer.clone(),
                date_issued: batch.shared_warrant.date_issued,
                date_received: batch.shared_warrant.date_received,
                valid_until: batch.shared_warrant.valid_until,
                offence_description: batch.shared_warrant.offence_description.clone(),
                sentence_details: None,
                is_active: true,
                registered_by: batch.admitted_by,
                batch_id: Some(batch.batch_id),
                document_hash: None,
            };

            let record = AdmissionRecord {
                identity: entry.identity,
                detention_basis: entry.detention_basis,
                intake_date: batch.intake_date,
                warrant: Some(warrant),
                intake_medical_notes: entry.intake_medical_notes,
                legal_reference: entry.legal_reference,
                emergency_contacts: entry.emergency_contacts,
                property: entry.property,
                legal_representation: entry.legal_representation,
                transfer_from: None,
                notes: entry.notes,
                admitted_by: batch.admitted_by,
            };

            let detainee = self.admit(record).await?;
            results.push(detainee);
        }

        Ok(results)
    }

    async fn update_detention_basis(
        &self,
        id: DetaineeId,
        new_basis: DetentionBasis,
        operator: OperatorId,
    ) -> Result<Detainee, RegistryError> {
        let current = self.load_detainee(id).await?;

        // Validate the transition
        validate_basis_transition(
            current.detention_basis.label(),
            new_basis.label(),
        )?;

        // Serialize
        let basis_label = new_basis.label().to_string();
        let basis_data = serde_json::to_string(&new_basis)
            .map_err(|e| RegistryError::Database(e.to_string()))?;
        let id_str = id.to_string();
        let now_str = Utc::now().to_rfc3339();

        sqlx::query(
            "UPDATE detainees SET detention_basis_label = ?, detention_basis_data = ?, updated_at = ? WHERE id = ?"
        )
        .bind(&basis_label)
        .bind(&basis_data)
        .bind(&now_str)
        .bind(&id_str)
        .execute(&self.pool)
        .await
        .map_err(|e| RegistryError::Database(e.to_string()))?;

        // Audit
        let before = serde_json::to_value(&current.detention_basis).ok();
        let after = serde_json::to_value(&new_basis).ok();
        let _ = audit::create_audit_entry(
            &self.pool, operator, ModuleName::Registry,
            "update_detention_basis", Some(id), before, after,
        ).await;

        self.load_detainee(id).await
    }

    async fn update_facility_status(
        &self,
        id: DetaineeId,
        status: FacilityStatus,
        operator: OperatorId,
        notes: Option<String>,
    ) -> Result<Detainee, RegistryError> {
        let current = self.load_detainee(id).await?;
        let id_str = id.to_string();
        let status_str = status.to_string();
        let now_str = Utc::now().to_rfc3339();

        sqlx::query("UPDATE detainees SET facility_status = ?, updated_at = ? WHERE id = ?")
            .bind(&status_str)
            .bind(&now_str)
            .bind(&id_str)
            .execute(&self.pool)
            .await
            .map_err(|e| RegistryError::Database(e.to_string()))?;

        // Add note if provided
        if let Some(note_text) = notes {
            sqlx::query(
                "INSERT INTO notes (detainee_id, content, author, timestamp) VALUES (?, ?, ?, ?)"
            )
            .bind(&id_str)
            .bind(&note_text)
            .bind(operator.as_uuid().to_string())
            .bind(&now_str)
            .execute(&self.pool)
            .await
            .map_err(|e| RegistryError::Database(e.to_string()))?;
        }

        // Audit
        let before = serde_json::to_value(&current.facility_status).ok();
        let after = serde_json::to_value(&status).ok();
        let _ = audit::create_audit_entry(
            &self.pool, operator, ModuleName::Registry,
            "update_facility_status", Some(id), before, after,
        ).await;

        self.load_detainee(id).await
    }

    async fn assign_housing(
        &self,
        id: DetaineeId,
        unit: HousingUnitId,
        operator: OperatorId,
    ) -> Result<Detainee, RegistryError> {
        let current = self.load_detainee(id).await?;
        let id_str = id.to_string();
        let unit_str = unit.as_uuid().to_string();
        let now_str = Utc::now().to_rfc3339();

        sqlx::query("UPDATE detainees SET housing_unit_id = ?, updated_at = ? WHERE id = ?")
            .bind(&unit_str)
            .bind(&now_str)
            .bind(&id_str)
            .execute(&self.pool)
            .await
            .map_err(|e| RegistryError::Database(e.to_string()))?;

        // Audit
        let before = current.housing_unit.map(|u| serde_json::json!(u.as_uuid().to_string()));
        let after = Some(serde_json::json!(unit_str));
        let _ = audit::create_audit_entry(
            &self.pool, operator, ModuleName::Registry,
            "assign_housing", Some(id), before, after,
        ).await;

        self.load_detainee(id).await
    }

    async fn register_warrant(
        &self,
        mut order: CommitmentOrder,
        operator: OperatorId,
    ) -> Result<Detainee, RegistryError> {
        let detainee_id = order.detainee_id;

        // Ensure the detainee exists
        let _ = self.load_detainee(detainee_id).await?;

        let mut tx = self.pool.begin().await
            .map_err(|e| RegistryError::Database(e.to_string()))?;

        // Deactivate previous active warrant
        let det_id_str = detainee_id.to_string();
        sqlx::query(
            "UPDATE commitment_orders SET is_active = 0 WHERE detainee_id = ? AND is_active = 1"
        )
        .bind(&det_id_str)
        .execute(&mut *tx)
        .await
        .map_err(|e| RegistryError::Database(e.to_string()))?;

        // Insert new warrant as active
        order.is_active = true;
        insert_warrant_tx(&mut tx, &order).await?;

        tx.commit().await
            .map_err(|e| RegistryError::Database(e.to_string()))?;

        // Audit
        let after = serde_json::to_value(&order).ok();
        let _ = audit::create_audit_entry(
            &self.pool, operator, ModuleName::Registry,
            "register_warrant", Some(detainee_id), None, after,
        ).await;

        self.load_detainee(detainee_id).await
    }

    async fn schedule_court_date(
        &self,
        court_date: CourtDate,
        operator: OperatorId,
    ) -> Result<CourtDate, RegistryError> {
        // Verify detainee exists
        let _ = self.load_detainee(court_date.detainee_id).await?;

        let id_str = court_date.id.as_uuid().to_string();
        let det_str = court_date.detainee_id.to_string();
        let date_str = court_date.scheduled_date.to_string();
        let purpose_str = format!("{:?}", court_date.purpose);
        let now_str = Utc::now().to_rfc3339();

        sqlx::query(
            "INSERT INTO court_dates (id, detainee_id, scheduled_date, court_name, purpose, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(&id_str)
        .bind(&det_str)
        .bind(&date_str)
        .bind(&court_date.court_name)
        .bind(&purpose_str)
        .bind(&now_str)
        .bind(&now_str)
        .execute(&self.pool)
        .await
        .map_err(|e| RegistryError::Database(e.to_string()))?;

        // Audit
        let after = serde_json::to_value(&court_date).ok();
        let _ = audit::create_audit_entry(
            &self.pool, operator, ModuleName::Registry,
            "schedule_court_date", Some(court_date.detainee_id), None, after,
        ).await;

        Ok(court_date)
    }

    async fn record_court_outcome(
        &self,
        court_date_id: CourtDateId,
        outcome: CourtOutcome,
        operator: OperatorId,
    ) -> Result<CourtDate, RegistryError> {
        let id_str = court_date_id.as_uuid().to_string();
        let outcome_json = serde_json::to_string(&outcome)
            .map_err(|e| RegistryError::Database(e.to_string()))?;
        let now_str = Utc::now().to_rfc3339();

        sqlx::query(
            "UPDATE court_dates SET outcome = ?, updated_at = ? WHERE id = ?"
        )
        .bind(&outcome_json)
        .bind(&now_str)
        .bind(&id_str)
        .execute(&self.pool)
        .await
        .map_err(|e| RegistryError::Database(e.to_string()))?;

        // Load the court date back
        let row: (String, String, String, String, String, Option<String>) = sqlx::query_as(
            "SELECT id, detainee_id, scheduled_date, court_name, purpose, outcome FROM court_dates WHERE id = ?"
        )
        .bind(&id_str)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| RegistryError::Database(e.to_string()))?;

        let detainee_id = DetaineeId::from_uuid(parse_uuid(&row.1)?);

        // Audit
        let after = serde_json::to_value(&outcome).ok();
        let _ = audit::create_audit_entry(
            &self.pool, operator, ModuleName::Registry,
            "record_court_outcome", Some(detainee_id), None, after,
        ).await;

        Ok(CourtDate {
            id: court_date_id,
            detainee_id,
            scheduled_date: parse_date(&row.2)?,
            court_name: row.3,
            purpose: serde_json::from_str(&row.4).unwrap_or(CourtPurpose::Other),
            outcome: Some(outcome),
        })
    }

    async fn release(&self, record: ReleaseRecord) -> Result<Detainee, RegistryError> {
        let current = self.load_detainee(record.detainee_id).await?;

        // Check detainee is active
        if !matches!(
            current.facility_status,
            FacilityStatus::Present | FacilityStatus::InCourt | FacilityStatus::InHospital
        ) {
            return Err(RegistryError::Domain(DomainError::InactiveDetainee {
                id: record.detainee_id,
            }));
        }

        let id_str = record.detainee_id.to_string();
        let now_str = Utc::now().to_rfc3339();

        sqlx::query("UPDATE detainees SET facility_status = 'Released', updated_at = ? WHERE id = ?")
            .bind(&now_str)
            .bind(&id_str)
            .execute(&self.pool)
            .await
            .map_err(|e| RegistryError::Database(e.to_string()))?;

        // Deactivate active warrant
        sqlx::query("UPDATE commitment_orders SET is_active = 0 WHERE detainee_id = ? AND is_active = 1")
            .bind(&id_str)
            .execute(&self.pool)
            .await
            .map_err(|e| RegistryError::Database(e.to_string()))?;

        // Add release note
        if let Some(note) = &record.notes {
            sqlx::query(
                "INSERT INTO notes (detainee_id, content, author, timestamp) VALUES (?, ?, ?, ?)"
            )
            .bind(&id_str)
            .bind(note)
            .bind(record.authorized_by.as_uuid().to_string())
            .bind(&now_str)
            .execute(&self.pool)
            .await
            .map_err(|e| RegistryError::Database(e.to_string()))?;
        }

        // Audit
        let before = serde_json::to_value(&current.facility_status).ok();
        let after = serde_json::to_value(&record).ok();
        let _ = audit::create_audit_entry(
            &self.pool, record.authorized_by, ModuleName::Registry,
            "release", Some(record.detainee_id), before, after,
        ).await;

        self.load_detainee(record.detainee_id).await
    }

    async fn open_daily_count(
        &self,
        date: NaiveDate,
        _operator: OperatorId,
    ) -> Result<DailyCount, RegistryError> {
        let date_str = date.to_string();

        // Check if count already exists
        let existing: Option<(String,)> = sqlx::query_as(
            "SELECT id FROM daily_counts WHERE date = ?"
        )
        .bind(&date_str)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| RegistryError::Database(e.to_string()))?;

        if let Some((id_str,)) = existing {
            // Return existing count
            return self.load_daily_count(&id_str).await;
        }

        // Get opening count from previous day's closing or count present detainees
        let prev_date = date - chrono::Duration::days(1);
        let prev_date_str = prev_date.to_string();

        let opening: u32 = sqlx::query_as::<_, (i64,)>(
            "SELECT COALESCE( \
                (SELECT computed_closing FROM daily_counts WHERE date = ?), \
                (SELECT COUNT(*) FROM detainees WHERE facility_status IN ('Present', 'InCourt', 'InHospital')) \
             )"
        )
        .bind(&prev_date_str)
        .fetch_one(&self.pool)
        .await
        .map(|(c,)| c as u32)
        .unwrap_or(0);

        let count_id = DailyCountId::new();
        let id_str = count_id.as_uuid().to_string();
        let now_str = Utc::now().to_rfc3339();

        sqlx::query(
            "INSERT INTO daily_counts (id, date, opening_count, computed_closing, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?)"
        )
        .bind(&id_str)
        .bind(&date_str)
        .bind(opening as i32)
        .bind(opening as i32)  // computed_closing starts equal to opening
        .bind(&now_str)
        .bind(&now_str)
        .execute(&self.pool)
        .await
        .map_err(|e| RegistryError::Database(e.to_string()))?;

        self.load_daily_count(&id_str).await
    }

    async fn submit_unit_headcount(
        &self,
        date: NaiveDate,
        headcount: UnitHeadcount,
    ) -> Result<DailyCount, RegistryError> {
        let date_str = date.to_string();

        // Get the daily count for this date
        let count_row: Option<(String,)> = sqlx::query_as(
            "SELECT id FROM daily_counts WHERE date = ?"
        )
        .bind(&date_str)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| RegistryError::Database(e.to_string()))?;

        let count_id = count_row
            .ok_or_else(|| RegistryError::Database("no daily count for date".into()))?
            .0;

        let unit_str = headcount.unit_id.as_uuid().to_string();
        let counted_by_str = headcount.counted_by.as_uuid().to_string();
        let counted_at_str = headcount.counted_at.to_rfc3339();
        let received_at_str = headcount.received_at.to_rfc3339();

        sqlx::query(
            "INSERT OR REPLACE INTO unit_headcounts (daily_count_id, unit_id, count, counted_by, counted_at, received_at, was_offline) \
             VALUES (?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(&count_id)
        .bind(&unit_str)
        .bind(headcount.count as i32)
        .bind(&counted_by_str)
        .bind(&counted_at_str)
        .bind(&received_at_str)
        .bind(headcount.was_offline as i32)
        .execute(&self.pool)
        .await
        .map_err(|e| RegistryError::Database(e.to_string()))?;

        // Recompute actual_closing_count
        let total: (i64,) = sqlx::query_as(
            "SELECT COALESCE(SUM(count), 0) FROM unit_headcounts WHERE daily_count_id = ?"
        )
        .bind(&count_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| RegistryError::Database(e.to_string()))?;

        let actual = total.0 as u32;
        let now_str = Utc::now().to_rfc3339();

        // Load the computed_closing to check balance
        let computed: (i64,) = sqlx::query_as(
            "SELECT computed_closing FROM daily_counts WHERE id = ?"
        )
        .bind(&count_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| RegistryError::Database(e.to_string()))?;

        let is_balanced = actual == computed.0 as u32;

        sqlx::query(
            "UPDATE daily_counts SET actual_closing_count = ?, is_balanced = ?, counted_by = ?, updated_at = ? WHERE id = ?"
        )
        .bind(actual as i32)
        .bind(is_balanced as i32)
        .bind(&headcount.counted_by.as_uuid().to_string())
        .bind(&now_str)
        .bind(&count_id)
        .execute(&self.pool)
        .await
        .map_err(|e| RegistryError::Database(e.to_string()))?;

        self.load_daily_count(&count_id).await
    }

    async fn finalize_daily_count(
        &self,
        date: NaiveDate,
        operator: OperatorId,
    ) -> Result<DailyCount, RegistryError> {
        let date_str = date.to_string();
        let now = Utc::now();
        let now_str = now.to_rfc3339();
        let op_str = operator.as_uuid().to_string();

        sqlx::query(
            "UPDATE daily_counts SET finalized_by = ?, finalized_at = ?, updated_at = ? WHERE date = ?"
        )
        .bind(&op_str)
        .bind(&now_str)
        .bind(&now_str)
        .bind(&date_str)
        .execute(&self.pool)
        .await
        .map_err(|e| RegistryError::Database(e.to_string()))?;

        let count_row: (String,) = sqlx::query_as(
            "SELECT id FROM daily_counts WHERE date = ?"
        )
        .bind(&date_str)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| RegistryError::Database(e.to_string()))?;

        self.load_daily_count(&count_row.0).await
    }

    async fn get_detainee(&self, id: DetaineeId) -> Result<Detainee, RegistryError> {
        self.load_detainee(id).await
    }

    async fn search(
        &self,
        query: PopulationQuery,
        _operator: OperatorId,
    ) -> Result<PopulationQueryResult, RegistryError> {
        let qb = crate::queries::QueryBuilder::from_query(&query);

        // Get total count
        let count_sql = qb.count_query();
        let total: (i64,) = sqlx::query_as(&count_sql)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| RegistryError::Database(e.to_string()))?;

        // Get rows
        let select_sql = qb.select_query();
        let rows: Vec<DetaineeRow> = sqlx::query_as(&select_sql)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| RegistryError::Database(e.to_string()))?;

        // Convert to summaries
        let today = Utc::now().date_naive();
        let mut summaries = Vec::new();
        for row in rows {
            let id = DetaineeId::from_uuid(parse_uuid(&row.id)?);
            let intake = parse_date(&row.intake_date)?;
            let days_held = (today - intake).num_days().max(0) as u32;

            let basis_label: DetentionBasisLabel = match row.detention_basis_label.as_str() {
                "NoLegalBasis" => DetentionBasisLabel::NoLegalBasis,
                "PoliceCustody" => DetentionBasisLabel::PoliceCustody,
                "Remand" => DetentionBasisLabel::Remand,
                "OnTrial" => DetentionBasisLabel::OnTrial,
                "ConvictedUnsentenced" => DetentionBasisLabel::ConvictedUnsentenced,
                "Sentenced" => DetentionBasisLabel::Sentenced,
                "Appeal" => DetentionBasisLabel::Appeal,
                _ => DetentionBasisLabel::NoLegalBasis,
            };

            summaries.push(DetaineeSummary {
                id,
                name: format!("{}, {}", row.surname, row.given_names),
                sex: parse_sex(&row.sex),
                age: None, // Would need DOB calculation
                detention_basis: basis_label,
                intake_date: PastDate::from_trusted(intake),
                days_held,
                bail_status: None,
                next_court_date: None,
                release_date: None,
                housing_unit: row.housing_unit_id,
                has_legal_representation: row.legal_representation.is_some(),
                flags: Vec::new(), // Flags computed separately if needed
            });
        }

        Ok(PopulationQueryResult {
            detainees: summaries,
            total_matching: total.0 as u32,
            statistics: default_statistics(),
        })
    }

    async fn overview(&self) -> Result<FacilityOverview, RegistryError> {
        let today = Utc::now();

        // Total population (active detainees)
        let total: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM detainees WHERE facility_status IN ('Present', 'InCourt', 'InHospital')"
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| RegistryError::Database(e.to_string()))?;

        // Basis breakdown
        let basis = load_basis_breakdown(&self.pool).await?;

        let total_pop = total.0 as u32;
        let pretrial = basis.no_legal_basis + basis.police_custody + basis.remand
            + basis.on_trial + basis.convicted_unsentenced;
        let pretrial_pct = if total_pop > 0 {
            pretrial as f64 / total_pop as f64 * 100.0
        } else {
            0.0
        };

        let system_status = if PastDate::clock_is_sane() {
            SystemStatus::Healthy
        } else {
            SystemStatus::ClockUnsynchronized { system_time: today }
        };

        Ok(FacilityOverview {
            as_of: today,
            system_status,
            total_population: total_pop,
            facility_capacity: self.config.capacity,
            occupancy_percent: if self.config.capacity > 0 {
                total_pop as f64 / self.config.capacity as f64 * 100.0
            } else {
                0.0
            },
            basis_breakdown: basis,
            pretrial_percent: pretrial_pct,
            time_distribution: TimeDistribution::default(),
            flag_counts: FlagCounts::default(),
            sex_breakdown: SexBreakdown::default(),
            today_count_status: DailyCountStatus::NotStarted,
            critical: CriticalNumbers::default(),
        })
    }
}

// === Helper functions ===

async fn insert_warrant(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    warrant: &CommitmentOrder,
) -> Result<(), RegistryError> {
    insert_warrant_tx(tx, warrant).await
}

async fn insert_warrant_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    warrant: &CommitmentOrder,
) -> Result<(), RegistryError> {
    let id_str = warrant.id.as_uuid().to_string();
    let det_str = warrant.detainee_id.to_string();
    let order_type_str = match &warrant.order_type {
        CommitmentOrderType::PoliceHolding => "PoliceHolding",
        CommitmentOrderType::RemandOrder => "RemandOrder",
        CommitmentOrderType::RemandRenewal { .. } => "RemandRenewal",
        CommitmentOrderType::ConvictionCommitment => "ConvictionCommitment",
        CommitmentOrderType::Transfer { .. } => "Transfer",
        CommitmentOrderType::ProductionOrder => "ProductionOrder",
        CommitmentOrderType::ReleaseOrder { .. } => "ReleaseOrder",
        CommitmentOrderType::BailOrder { .. } => "BailOrder",
        CommitmentOrderType::Other { .. } => "Other",
    };
    let order_data = serde_json::to_string(&warrant.order_type)
        .map_err(|e| RegistryError::Database(e.to_string()))?;
    let date_issued_str = warrant.date_issued.as_naive().to_string();
    let date_received_str = warrant.date_received.as_naive().to_string();
    let valid_until_str = warrant.valid_until.map(|d| d.to_string());
    let sentence_details_str = warrant.sentence_details.as_ref()
        .map(|sd| serde_json::to_string(sd).unwrap_or_default());
    let registered_by_str = warrant.registered_by.as_uuid().to_string();
    let batch_id_str = warrant.batch_id.map(|b| b.as_uuid().to_string());
    let now_str = Utc::now().to_rfc3339();

    sqlx::query(
        "INSERT INTO commitment_orders (id, detainee_id, order_type, order_data, external_reference, \
         issuing_authority, issuing_officer, date_issued, date_received, valid_until, \
         offence_description, sentence_details, is_active, registered_by, batch_id, document_hash, created_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
    )
    .bind(&id_str)
    .bind(&det_str)
    .bind(order_type_str)
    .bind(&order_data)
    .bind(&warrant.external_reference)
    .bind(&warrant.issuing_authority)
    .bind(&warrant.issuing_officer)
    .bind(&date_issued_str)
    .bind(&date_received_str)
    .bind(&valid_until_str)
    .bind(&warrant.offence_description)
    .bind(&sentence_details_str)
    .bind(warrant.is_active as i32)
    .bind(&registered_by_str)
    .bind(&batch_id_str)
    .bind(&warrant.document_hash)
    .bind(&now_str)
    .execute(&mut **tx)
    .await
    .map_err(|e| RegistryError::Database(e.to_string()))?;

    Ok(())
}

impl SqliteRegistry {
    async fn load_daily_count(&self, id: &str) -> Result<DailyCount, RegistryError> {
        let row: DailyCountRow = sqlx::query_as(
                "SELECT id, date, opening_count, admissions, transfers_in, court_returns, hospital_returns, \
                 releases, transfers_out, to_court, to_hospital, escapes, deaths, computed_closing, \
                 actual_closing_count, is_balanced, discrepancy_note, counted_by, finalized_by, finalized_at \
                 FROM daily_counts WHERE id = ?"
            )
            .bind(id)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| RegistryError::Database(e.to_string()))?;

        let count_id = DailyCountId::from_uuid(parse_uuid(&row.id)?);
        let date = parse_date(&row.date)?;

        // Load unit headcounts
        let headcount_rows: Vec<(String, i64, String, String, String, i64)> = sqlx::query_as(
            "SELECT unit_id, count, counted_by, counted_at, received_at, was_offline \
             FROM unit_headcounts WHERE daily_count_id = ?"
        )
        .bind(id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RegistryError::Database(e.to_string()))?;

        let unit_counts: Vec<UnitHeadcount> = headcount_rows.into_iter().map(|hr| {
            UnitHeadcount {
                unit_id: HousingUnitId::from_uuid(uuid::Uuid::parse_str(&hr.0).unwrap_or_default()),
                count: hr.1 as u32,
                counted_by: OperatorId::from_uuid(uuid::Uuid::parse_str(&hr.2).unwrap_or_default()),
                counted_at: chrono::DateTime::parse_from_rfc3339(&hr.3)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
                received_at: chrono::DateTime::parse_from_rfc3339(&hr.4)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
                was_offline: hr.5 != 0,
            }
        }).collect();

        Ok(DailyCount {
            id: count_id,
            date,
            opening_count: row.opening_count as u32,
            admissions: row.admissions as u32,
            transfers_in: row.transfers_in as u32,
            court_returns: row.court_returns as u32,
            hospital_returns: row.hospital_returns as u32,
            releases: row.releases as u32,
            transfers_out: row.transfers_out as u32,
            to_court: row.to_court as u32,
            to_hospital: row.to_hospital as u32,
            escapes: row.escapes as u32,
            deaths: row.deaths as u32,
            computed_closing: row.computed_closing as u32,
            unit_counts,
            actual_closing_count: row.actual_closing_count.map(|v| v as u32),
            is_balanced: row.is_balanced.map(|v| v != 0),
            closing_by_basis: BasisBreakdown::default(),
            closing_by_sex: SexBreakdown::default(),
            discrepancy_note: row.discrepancy_note,
            counted_by: row.counted_by.as_deref()
                .and_then(|s| uuid::Uuid::parse_str(s).ok())
                .map(OperatorId::from_uuid),
            finalized_by: row.finalized_by.as_deref()
                .and_then(|s| uuid::Uuid::parse_str(s).ok())
                .map(OperatorId::from_uuid),
            finalized_at: row.finalized_at.as_deref()
                .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                .map(|dt| dt.with_timezone(&Utc)),
        })
    }
}

async fn load_basis_breakdown(pool: &SqlitePool) -> Result<BasisBreakdown, RegistryError> {
    let rows: Vec<(String, i64)> = sqlx::query_as(
        "SELECT detention_basis_label, COUNT(*) FROM detainees \
         WHERE facility_status IN ('Present', 'InCourt', 'InHospital') \
         GROUP BY detention_basis_label"
    )
    .fetch_all(pool)
    .await
    .map_err(|e| RegistryError::Database(e.to_string()))?;

    let mut breakdown = BasisBreakdown::default();
    for (label, count) in rows {
        let c = count as u32;
        match label.as_str() {
            "NoLegalBasis" => breakdown.no_legal_basis = c,
            "PoliceCustody" => breakdown.police_custody = c,
            "Remand" => breakdown.remand = c,
            "OnTrial" => breakdown.on_trial = c,
            "ConvictedUnsentenced" => breakdown.convicted_unsentenced = c,
            "Sentenced" => breakdown.sentenced = c,
            "Appeal" => breakdown.appeal = c,
            _ => {}
        }
    }
    Ok(breakdown)
}

fn default_statistics() -> QueryStatistics {
    QueryStatistics {
        total: 0,
        by_detention_basis: BasisBreakdown::default(),
        time_held: TimeDistribution::default(),
        flag_counts: FlagCounts::default(),
        with_court_date: 0,
        without_court_date: 0,
        with_legal_representation: 0,
        without_legal_representation: 0,
        bail_granted_still_held: 0,
        bail_not_applied: 0,
        by_sex: SexBreakdown::default(),
        facility_capacity: 0,
        occupancy_rate: 0.0,
    }
}
