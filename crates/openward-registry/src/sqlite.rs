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

    // === Read-only convenience methods ===

    /// Get all court dates for a specific detainee, ordered by scheduled date (newest first).
    pub async fn court_dates_for_detainee(&self, id: DetaineeId) -> Result<Vec<CourtDate>, RegistryError> {
        let id_str = id.to_string();

        let rows: Vec<CourtDateRow> = sqlx::query_as(
            "SELECT id, detainee_id, scheduled_date, court_name, purpose, outcome \
             FROM court_dates WHERE detainee_id = ? ORDER BY scheduled_date DESC"
        )
        .bind(&id_str)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RegistryError::Database(e.to_string()))?;

        rows.into_iter()
            .map(row_to_court_date)
            .collect::<Result<Vec<_>, _>>()
    }

    /// Ensure at least one default housing unit exists.
    /// Called during initialization to make the system turnkey.
    pub async fn seed_default_housing(&self) -> Result<(), RegistryError> {
        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM housing_units")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| RegistryError::Database(e.to_string()))?;

        if count.0 == 0 {
            let id = uuid::Uuid::new_v4().to_string();
            sqlx::query(
                "INSERT INTO housing_units (id, name, capacity, unit_type) \
                 VALUES (?, 'Main Wing', ?, 'General')"
            )
            .bind(&id)
            .bind(self.config.capacity as i32)
            .execute(&self.pool)
            .await
            .map_err(|e| RegistryError::Database(e.to_string()))?;
        }

        Ok(())
    }

    /// Get a single housing unit by ID.
    pub async fn get_housing_unit(&self, id: HousingUnitId) -> Result<HousingUnit, RegistryError> {
        let id_str = id.as_uuid().to_string();

        let row: HousingUnitRow = sqlx::query_as(
            "SELECT id, name, capacity, unit_type, designated_sex, designated_age_group \
             FROM housing_units WHERE id = ?"
        )
        .bind(&id_str)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| RegistryError::Database(format!("housing unit not found: {}", e)))?;

        row_to_housing_unit(row)
    }

    /// Create a new housing unit.
    pub async fn create_housing_unit(&self, unit: HousingUnit) -> Result<HousingUnit, RegistryError> {
        let id_str = unit.id.as_uuid().to_string();
        let type_str = housing_type_str(&unit.unit_type);
        let sex_str = unit.designated_sex.as_ref().map(|s| format!("{:?}", s));
        let age_str = unit.designated_age_group.as_ref().map(|a| format!("{:?}", a));

        sqlx::query(
            "INSERT INTO housing_units (id, name, capacity, unit_type, designated_sex, designated_age_group) \
             VALUES (?, ?, ?, ?, ?, ?)"
        )
        .bind(&id_str)
        .bind(&unit.name)
        .bind(unit.capacity as i32)
        .bind(type_str)
        .bind(&sex_str)
        .bind(&age_str)
        .execute(&self.pool)
        .await
        .map_err(|e| RegistryError::Database(e.to_string()))?;

        Ok(unit)
    }

    /// Update an existing housing unit.
    pub async fn update_housing_unit(&self, unit: HousingUnit) -> Result<HousingUnit, RegistryError> {
        let id_str = unit.id.as_uuid().to_string();
        let type_str = housing_type_str(&unit.unit_type);
        let sex_str = unit.designated_sex.as_ref().map(|s| format!("{:?}", s));
        let age_str = unit.designated_age_group.as_ref().map(|a| format!("{:?}", a));

        let result = sqlx::query(
            "UPDATE housing_units SET name = ?, capacity = ?, unit_type = ?, \
             designated_sex = ?, designated_age_group = ? WHERE id = ?"
        )
        .bind(&unit.name)
        .bind(unit.capacity as i32)
        .bind(type_str)
        .bind(&sex_str)
        .bind(&age_str)
        .bind(&id_str)
        .execute(&self.pool)
        .await
        .map_err(|e| RegistryError::Database(e.to_string()))?;

        if result.rows_affected() == 0 {
            return Err(RegistryError::Database("housing unit not found".into()));
        }

        Ok(unit)
    }

    /// Delete a housing unit. Fails if detainees are currently assigned to it.
    pub async fn delete_housing_unit(&self, id: HousingUnitId) -> Result<(), RegistryError> {
        let id_str = id.as_uuid().to_string();

        // Check for assigned detainees
        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM detainees WHERE housing_unit_id = ? \
             AND facility_status IN ('Present', 'InCourt', 'InHospital')"
        )
        .bind(&id_str)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| RegistryError::Database(e.to_string()))?;

        if count.0 > 0 {
            return Err(RegistryError::Database(
                format!("{} detenu(s) encore assigne(s) a cette unite", count.0)
            ));
        }

        // Clear housing_unit_id from any released/inactive detainees referencing this unit
        sqlx::query(
            "UPDATE detainees SET housing_unit_id = NULL WHERE housing_unit_id = ?"
        )
        .bind(&id_str)
        .execute(&self.pool)
        .await
        .map_err(|e| RegistryError::Database(e.to_string()))?;

        let result = sqlx::query("DELETE FROM housing_units WHERE id = ?")
            .bind(&id_str)
            .execute(&self.pool)
            .await
            .map_err(|e| RegistryError::Database(e.to_string()))?;

        if result.rows_affected() == 0 {
            return Err(RegistryError::Database("housing unit not found".into()));
        }

        Ok(())
    }

    /// Count active detainees assigned to a housing unit.
    pub async fn housing_unit_occupancy(&self, id: HousingUnitId) -> Result<u32, RegistryError> {
        let id_str = id.as_uuid().to_string();

        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM detainees WHERE housing_unit_id = ? \
             AND facility_status IN ('Present', 'InCourt', 'InHospital')"
        )
        .bind(&id_str)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| RegistryError::Database(e.to_string()))?;

        Ok(count.0 as u32)
    }

    /// Get all housing units, ordered by name.
    pub async fn housing_units(&self) -> Result<Vec<HousingUnit>, RegistryError> {
        let rows: Vec<HousingUnitRow> = sqlx::query_as(
            "SELECT id, name, capacity, unit_type, designated_sex, designated_age_group \
             FROM housing_units ORDER BY name"
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RegistryError::Database(e.to_string()))?;

        rows.into_iter()
            .map(row_to_housing_unit)
            .collect::<Result<Vec<_>, _>>()
    }

    /// Get upcoming court dates (no outcome yet and scheduled for today or later).
    /// Returns tuples of (CourtDate, detainee_name, detainee_id_string).
    pub async fn upcoming_court_dates(&self, limit: u32) -> Result<Vec<(CourtDate, String, String)>, RegistryError> {
        let today = chrono::Utc::now().date_naive().to_string();

        let rows: Vec<CourtDateWithDetaineeRow> = sqlx::query_as(
            "SELECT cd.id, cd.detainee_id, cd.scheduled_date, cd.court_name, cd.purpose, cd.outcome, \
                    d.surname, d.given_names \
             FROM court_dates cd \
             JOIN detainees d ON cd.detainee_id = d.id \
             WHERE cd.outcome IS NULL AND cd.scheduled_date >= ? \
             ORDER BY cd.scheduled_date ASC \
             LIMIT ?"
        )
        .bind(&today)
        .bind(limit as i32)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RegistryError::Database(e.to_string()))?;

        rows.into_iter()
            .map(|row| {
                let court_date = row_to_court_date(CourtDateRow {
                    id: row.id.clone(),
                    detainee_id: row.detainee_id.clone(),
                    scheduled_date: row.scheduled_date,
                    court_name: row.court_name,
                    purpose: row.purpose,
                    outcome: row.outcome,
                })?;
                let name = format!("{}, {}", row.surname, row.given_names);
                Ok((court_date, name, row.detainee_id))
            })
            .collect::<Result<Vec<_>, _>>()
    }

    /// Get recent court dates (have an outcome or scheduled before today).
    /// Returns tuples of (CourtDate, detainee_name, detainee_id_string).
    pub async fn recent_court_dates(&self, limit: u32) -> Result<Vec<(CourtDate, String, String)>, RegistryError> {
        let today = chrono::Utc::now().date_naive().to_string();

        let rows: Vec<CourtDateWithDetaineeRow> = sqlx::query_as(
            "SELECT cd.id, cd.detainee_id, cd.scheduled_date, cd.court_name, cd.purpose, cd.outcome, \
                    d.surname, d.given_names \
             FROM court_dates cd \
             JOIN detainees d ON cd.detainee_id = d.id \
             WHERE cd.outcome IS NOT NULL OR cd.scheduled_date < ? \
             ORDER BY cd.scheduled_date DESC \
             LIMIT ?"
        )
        .bind(&today)
        .bind(limit as i32)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RegistryError::Database(e.to_string()))?;

        rows.into_iter()
            .map(|row| {
                let court_date = row_to_court_date(CourtDateRow {
                    id: row.id.clone(),
                    detainee_id: row.detainee_id.clone(),
                    scheduled_date: row.scheduled_date,
                    court_name: row.court_name,
                    purpose: row.purpose,
                    outcome: row.outcome,
                })?;
                let name = format!("{}, {}", row.surname, row.given_names);
                Ok((court_date, name, row.detainee_id))
            })
            .collect::<Result<Vec<_>, _>>()
    }

    /// Get a daily count for a specific date if it exists.
    pub async fn get_daily_count_for_date(&self, date: NaiveDate) -> Result<Option<DailyCount>, RegistryError> {
        let date_str = date.to_string();

        let row: Option<(String,)> = sqlx::query_as(
            "SELECT id FROM daily_counts WHERE date = ?"
        )
        .bind(&date_str)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| RegistryError::Database(e.to_string()))?;

        match row {
            Some((id,)) => Ok(Some(self.load_daily_count(&id).await?)),
            None => Ok(None),
        }
    }

    /// Get a single court date by ID.
    pub async fn get_court_date(&self, id: CourtDateId) -> Result<CourtDate, RegistryError> {
        let id_str = id.as_uuid().to_string();

        let row: CourtDateRow = sqlx::query_as(
            "SELECT id, detainee_id, scheduled_date, court_name, purpose, outcome \
             FROM court_dates WHERE id = ?"
        )
        .bind(&id_str)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| RegistryError::Database(e.to_string()))?;

        row_to_court_date(row)
    }

    /// Get recent transfers with detainee names joined.
    pub async fn recent_transfers(&self, limit: u32) -> Result<Vec<(TransferRecord, String)>, RegistryError> {
        let rows: Vec<(String, String, Option<String>, Option<String>, String, String, String, String, String)> = sqlx::query_as(
            "SELECT t.id, t.detainee_id, t.from_facility, t.to_facility, t.transfer_date, t.reason, t.authorized_by, \
                    d.surname, d.given_names \
             FROM transfers t \
             JOIN detainees d ON t.detainee_id = d.id \
             ORDER BY t.transfer_date DESC \
             LIMIT ?"
        )
        .bind(limit as i32)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RegistryError::Database(e.to_string()))?;

        rows.into_iter()
            .map(|row| {
                let record = TransferRecord {
                    id: parse_uuid(&row.0)?,
                    detainee_id: DetaineeId::from_uuid(parse_uuid(&row.1)?),
                    from_facility: row.2,
                    to_facility: row.3,
                    transfer_date: PastDate::from_trusted(parse_date(&row.4)?),
                    reason: row.5,
                    authorized_by: OperatorId::from_uuid(parse_uuid(&row.6)?),
                };
                let name = format!("{}, {}", row.7, row.8);
                Ok((record, name))
            })
            .collect::<Result<Vec<_>, _>>()
    }

    /// Update a court date (only if outcome has not been recorded).
    pub async fn update_court_date(
        &self,
        court_date_id: CourtDateId,
        scheduled_date: NaiveDate,
        court_name: String,
        purpose: CourtPurpose,
        operator: OperatorId,
    ) -> Result<CourtDate, RegistryError> {
        let id_str = court_date_id.as_uuid().to_string();
        let now_str = Utc::now().to_rfc3339();
        let purpose_json = serde_json::to_string(&purpose)
            .map_err(|e| RegistryError::Database(e.to_string()))?;

        // Only update if outcome is NULL
        let result = sqlx::query(
            "UPDATE court_dates SET scheduled_date = ?, court_name = ?, purpose = ?, updated_at = ? \
             WHERE id = ? AND outcome IS NULL"
        )
        .bind(scheduled_date.to_string())
        .bind(&court_name)
        .bind(&purpose_json)
        .bind(&now_str)
        .bind(&id_str)
        .execute(&self.pool)
        .await
        .map_err(|e| RegistryError::Database(e.to_string()))?;

        if result.rows_affected() == 0 {
            return Err(RegistryError::Database(
                "court date not found or outcome already recorded".into()
            ));
        }

        // Audit
        let after = serde_json::json!({
            "scheduled_date": scheduled_date.to_string(),
            "court_name": court_name,
            "purpose": purpose_json,
        });
        let cd = self.get_court_date(court_date_id).await?;
        let _ = audit::create_audit_entry(
            &self.pool, operator, ModuleName::Registry,
            "update_court_date", Some(cd.detainee_id), None, Some(after),
        ).await;

        Ok(cd)
    }

    // === Export / backup query helpers ===

    /// Load daily counts within a date range.
    pub async fn daily_counts_in_range(
        &self,
        from: NaiveDate,
        to: NaiveDate,
    ) -> Result<Vec<DailyCount>, RegistryError> {
        let from_str = from.to_string();
        let to_str = to.to_string();

        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT id FROM daily_counts WHERE date BETWEEN ? AND ? ORDER BY date"
        )
        .bind(&from_str)
        .bind(&to_str)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RegistryError::Database(e.to_string()))?;

        let mut counts = Vec::with_capacity(rows.len());
        for (id,) in rows {
            counts.push(self.load_daily_count(&id).await?);
        }
        Ok(counts)
    }

    /// Load audit entries within a timestamp range.
    pub async fn audit_entries_in_range(
        &self,
        from: NaiveDate,
        to: NaiveDate,
    ) -> Result<Vec<AuditEntry>, RegistryError> {
        let from_str = format!("{}T00:00:00+00:00", from);
        let to_str = format!("{}T23:59:59+00:00", to);

        let rows: Vec<AuditRow> = sqlx::query_as(
            "SELECT id, timestamp, operator, module, action, target, \
             before_data, after_data, self_hash, chain_hash, epoch \
             FROM audit_entries WHERE timestamp BETWEEN ? AND ? ORDER BY timestamp"
        )
        .bind(&from_str)
        .bind(&to_str)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RegistryError::Database(e.to_string()))?;

        rows.into_iter()
            .map(row_to_audit_entry)
            .collect::<Result<Vec<_>, _>>()
    }

    /// Load all active detainees as summaries (for CSV export, no pagination).
    pub async fn all_active_detainees_for_export(&self) -> Result<Vec<DetaineeSummary>, RegistryError> {
        let today = Utc::now().date_naive();

        let rows: Vec<DetaineeRow> = sqlx::query_as(
            "SELECT id, surname, given_names, preferred_name, sex, date_of_birth, \
             nationality, national_id, detention_basis_label, detention_basis_data, \
             facility_status, intake_date, housing_unit_id, identity_extra, \
             legal_reference, legal_representation, emergency_contacts \
             FROM detainees WHERE facility_status IN ('Present', 'InCourt', 'InHospital') \
             ORDER BY surname, given_names"
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RegistryError::Database(e.to_string()))?;

        let housing_units = self.housing_units().await?;

        let mut summaries = Vec::with_capacity(rows.len());
        for row in &rows {
            let id = DetaineeId::from_uuid(parse_uuid(&row.id)?);
            let intake = parse_date(&row.intake_date)?;
            let days_held = (today - intake).num_days().max(0) as u32;
            let basis_label = parse_basis_label(&row.detention_basis_label);

            let detainee = self.load_detainee(id).await?;
            let flags = crate::flags::compute_flags(&detainee, &self.config, &housing_units);

            // Resolve housing unit name
            let housing_name = match &row.housing_unit_id {
                Some(uid) => {
                    let uuid = parse_uuid(uid).ok();
                    uuid.and_then(|u| {
                        housing_units.iter().find(|hu| hu.id.as_uuid() == &u).map(|hu| hu.name.clone())
                    })
                }
                None => None,
            };

            summaries.push(DetaineeSummary {
                id,
                name: format!("{}, {}", row.surname, row.given_names),
                sex: parse_sex(&row.sex),
                age: None,
                detention_basis: basis_label,
                intake_date: PastDate::from_trusted(intake),
                days_held,
                bail_status: None,
                next_court_date: None,
                release_date: None,
                housing_unit: housing_name,
                has_legal_representation: row.legal_representation.is_some(),
                flags,
            });
        }

        Ok(summaries)
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
            "SELECT id, description, quantity, logged_date, logged_by, returned \
             FROM property_items WHERE detainee_id = ?"
        )
        .bind(&id_str)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RegistryError::Database(e.to_string()))?;

        // Load notes
        let note_rows: Vec<NoteRow> = sqlx::query_as(
            "SELECT id, content, author, timestamp, note_type FROM notes WHERE detainee_id = ? ORDER BY timestamp DESC"
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
    id: i64,
    description: String,
    quantity: i32,
    logged_date: String,
    logged_by: String,
    returned: i32,
}

#[derive(sqlx::FromRow)]
struct NoteRow {
    id: i64,
    content: String,
    author: String,
    timestamp: String,
    note_type: String,
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

#[derive(sqlx::FromRow)]
struct CourtDateRow {
    id: String,
    detainee_id: String,
    scheduled_date: String,
    court_name: String,
    purpose: String,
    outcome: Option<String>,
}

#[derive(sqlx::FromRow)]
struct CourtDateWithDetaineeRow {
    id: String,
    detainee_id: String,
    scheduled_date: String,
    court_name: String,
    purpose: String,
    outcome: Option<String>,
    surname: String,
    given_names: String,
}

#[derive(sqlx::FromRow)]
struct HousingUnitRow {
    id: String,
    name: String,
    capacity: i64,
    unit_type: String,
    designated_sex: Option<String>,
    designated_age_group: Option<String>,
}

#[derive(sqlx::FromRow)]
struct AuditRow {
    id: String,
    timestamp: String,
    operator: String,
    module: String,
    action: String,
    target: Option<String>,
    before_data: Option<String>,
    after_data: Option<String>,
    self_hash: Vec<u8>,
    chain_hash: Vec<u8>,
    epoch: i64,
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

fn parse_basis_label(s: &str) -> DetentionBasisLabel {
    match s {
        "NoLegalBasis" => DetentionBasisLabel::NoLegalBasis,
        "PoliceCustody" => DetentionBasisLabel::PoliceCustody,
        "Remand" => DetentionBasisLabel::Remand,
        "OnTrial" => DetentionBasisLabel::OnTrial,
        "ConvictedUnsentenced" => DetentionBasisLabel::ConvictedUnsentenced,
        "Sentenced" => DetentionBasisLabel::Sentenced,
        "Appeal" => DetentionBasisLabel::Appeal,
        _ => DetentionBasisLabel::NoLegalBasis,
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
                id: pr.id,
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
                id: nr.id,
                content: nr.content,
                author: OperatorId::from_uuid(parse_uuid(&nr.author)?),
                timestamp: ts,
                note_type: NoteType::from_str_lossy(&nr.note_type),
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

fn row_to_court_date(row: CourtDateRow) -> Result<CourtDate, RegistryError> {
    let purpose: CourtPurpose = serde_json::from_str(&row.purpose)
        .unwrap_or(CourtPurpose::Other);

    let outcome: Option<CourtOutcome> = row.outcome
        .as_deref()
        .map(|s| serde_json::from_str(s))
        .transpose()
        .map_err(|e| RegistryError::Database(format!("invalid court outcome JSON: {}", e)))?;

    Ok(CourtDate {
        id: CourtDateId::from_uuid(parse_uuid(&row.id)?),
        detainee_id: DetaineeId::from_uuid(parse_uuid(&row.detainee_id)?),
        scheduled_date: parse_date(&row.scheduled_date)?,
        court_name: row.court_name,
        purpose,
        outcome,
    })
}

fn parse_module_name(s: &str) -> ModuleName {
    match s {
        "Registry" => ModuleName::Registry,
        "Medical" => ModuleName::Medical,
        "Disciplinary" => ModuleName::Disciplinary,
        "Commissary" => ModuleName::Commissary,
        "Visitors" => ModuleName::Visitors,
        "Analytics" => ModuleName::Analytics,
        "Auth" => ModuleName::Auth,
        "Config" => ModuleName::Config,
        _ => ModuleName::Registry,
    }
}

fn row_to_audit_entry(row: AuditRow) -> Result<AuditEntry, RegistryError> {
    let id = parse_uuid(&row.id)?;
    let timestamp = chrono::DateTime::parse_from_rfc3339(&row.timestamp)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| RegistryError::Database(format!("invalid audit timestamp: {}", e)))?;
    let operator = OperatorId::from_uuid(parse_uuid(&row.operator)?);
    let module = parse_module_name(&row.module);
    let target = row.target.as_deref()
        .map(|s| parse_uuid(s).map(DetaineeId::from_uuid))
        .transpose()?;
    let before: Option<serde_json::Value> = row.before_data.as_deref()
        .map(|s| serde_json::from_str(s))
        .transpose()
        .map_err(|e| RegistryError::Database(format!("invalid audit before JSON: {}", e)))?;
    let after: Option<serde_json::Value> = row.after_data.as_deref()
        .map(|s| serde_json::from_str(s))
        .transpose()
        .map_err(|e| RegistryError::Database(format!("invalid audit after JSON: {}", e)))?;

    let mut self_hash = [0u8; 32];
    if row.self_hash.len() == 32 {
        self_hash.copy_from_slice(&row.self_hash);
    }
    let mut chain_hash = [0u8; 32];
    if row.chain_hash.len() == 32 {
        chain_hash.copy_from_slice(&row.chain_hash);
    }

    Ok(AuditEntry {
        id,
        timestamp,
        operator,
        module,
        action: row.action,
        target,
        before,
        after,
        self_hash,
        chain_hash,
        epoch: row.epoch as u64,
    })
}

fn housing_type_str(ht: &HousingType) -> &'static str {
    match ht {
        HousingType::General => "General",
        HousingType::Medical => "Medical",
        HousingType::Isolation => "Isolation",
        HousingType::Protective => "Protective",
        HousingType::PreTrial => "PreTrial",
    }
}

fn row_to_housing_unit(row: HousingUnitRow) -> Result<HousingUnit, RegistryError> {
    let unit_type = match row.unit_type.as_str() {
        "General" => HousingType::General,
        "Medical" => HousingType::Medical,
        "Isolation" => HousingType::Isolation,
        "Protective" => HousingType::Protective,
        "PreTrial" => HousingType::PreTrial,
        _ => HousingType::General,
    };

    let designated_sex = row.designated_sex.as_deref().map(parse_sex);

    let designated_age_group = row.designated_age_group.as_deref().and_then(|s| match s {
        "Juvenile" => Some(AgeGroup::Juvenile),
        "Adult" => Some(AgeGroup::Adult),
        _ => None,
    });

    Ok(HousingUnit {
        id: HousingUnitId::from_uuid(parse_uuid(&row.id)?),
        name: row.name,
        capacity: row.capacity as u32,
        unit_type,
        designated_sex,
        designated_age_group,
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
                "INSERT INTO notes (detainee_id, content, author, timestamp, note_type) VALUES (?, ?, ?, ?, 'general')"
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
                "INSERT INTO notes (detainee_id, content, author, timestamp, note_type) VALUES (?, ?, ?, ?, 'medical')"
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
                "INSERT INTO notes (detainee_id, content, author, timestamp, note_type) VALUES (?, ?, ?, ?, 'general')"
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

        let court_name = row.3;
        let purpose = serde_json::from_str(&row.4).unwrap_or(CourtPurpose::Other);

        // If Rescheduled, automatically create a new court date
        if let CourtOutcome::Rescheduled { new_date, .. } = &outcome {
            let new_id = CourtDateId::new();
            let new_id_str = new_id.as_uuid().to_string();
            let det_id_str = detainee_id.to_string();
            let purpose_json = serde_json::to_string(&purpose)
                .unwrap_or_else(|_| "\"Other\"".to_string());

            sqlx::query(
                "INSERT INTO court_dates (id, detainee_id, scheduled_date, court_name, purpose, created_at, updated_at) \
                 VALUES (?, ?, ?, ?, ?, ?, ?)"
            )
            .bind(&new_id_str)
            .bind(&det_id_str)
            .bind(new_date.to_string())
            .bind(&court_name)
            .bind(&purpose_json)
            .bind(&now_str)
            .bind(&now_str)
            .execute(&self.pool)
            .await
            .map_err(|e| RegistryError::Database(e.to_string()))?;
        }

        Ok(CourtDate {
            id: court_date_id,
            detainee_id,
            scheduled_date: parse_date(&row.2)?,
            court_name,
            purpose,
            outcome: Some(outcome),
        })
    }

    async fn add_note(
        &self,
        detainee_id: DetaineeId,
        content: String,
        note_type: NoteType,
        operator: OperatorId,
    ) -> Result<Detainee, RegistryError> {
        let _ = self.load_detainee(detainee_id).await?;
        let id_str = detainee_id.to_string();
        let now_str = Utc::now().to_rfc3339();

        sqlx::query(
            "INSERT INTO notes (detainee_id, content, author, timestamp, note_type) VALUES (?, ?, ?, ?, ?)"
        )
        .bind(&id_str)
        .bind(&content)
        .bind(operator.as_uuid().to_string())
        .bind(&now_str)
        .bind(note_type.as_str())
        .execute(&self.pool)
        .await
        .map_err(|e| RegistryError::Database(e.to_string()))?;

        let after = serde_json::json!({ "content": content, "note_type": note_type.as_str() });
        let _ = audit::create_audit_entry(
            &self.pool, operator, ModuleName::Registry,
            "add_note", Some(detainee_id), None, Some(after),
        ).await;

        self.load_detainee(detainee_id).await
    }

    async fn delete_note(
        &self,
        note_id: i64,
        operator: OperatorId,
    ) -> Result<(), RegistryError> {
        // Verify the note exists and get detainee_id for audit
        let row: Option<(String,)> = sqlx::query_as(
            "SELECT detainee_id FROM notes WHERE id = ?"
        )
        .bind(note_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| RegistryError::Database(e.to_string()))?;

        let detainee_id_str = row
            .ok_or_else(|| RegistryError::Database("note not found".into()))?
            .0;

        sqlx::query("DELETE FROM notes WHERE id = ?")
            .bind(note_id)
            .execute(&self.pool)
            .await
            .map_err(|e| RegistryError::Database(e.to_string()))?;

        let det_id = DetaineeId::from_uuid(parse_uuid(&detainee_id_str)?);
        let before = serde_json::json!({ "note_id": note_id });
        let _ = audit::create_audit_entry(
            &self.pool, operator, ModuleName::Registry,
            "delete_note", Some(det_id), Some(before), None,
        ).await;

        Ok(())
    }

    async fn add_property_item(
        &self,
        detainee_id: DetaineeId,
        description: String,
        quantity: u32,
        operator: OperatorId,
    ) -> Result<Detainee, RegistryError> {
        let _ = self.load_detainee(detainee_id).await?;
        let id_str = detainee_id.to_string();
        let today = Utc::now().date_naive().to_string();

        sqlx::query(
            "INSERT INTO property_items (detainee_id, description, quantity, logged_date, logged_by, returned) \
             VALUES (?, ?, ?, ?, ?, 0)"
        )
        .bind(&id_str)
        .bind(&description)
        .bind(quantity as i32)
        .bind(&today)
        .bind(operator.as_uuid().to_string())
        .execute(&self.pool)
        .await
        .map_err(|e| RegistryError::Database(e.to_string()))?;

        let after = serde_json::json!({ "description": description, "quantity": quantity });
        let _ = audit::create_audit_entry(
            &self.pool, operator, ModuleName::Registry,
            "add_property_item", Some(detainee_id), None, Some(after),
        ).await;

        self.load_detainee(detainee_id).await
    }

    async fn return_property_item(
        &self,
        property_id: i64,
        operator: OperatorId,
    ) -> Result<(), RegistryError> {
        // Verify it exists and get detainee_id
        let row: Option<(String,)> = sqlx::query_as(
            "SELECT detainee_id FROM property_items WHERE id = ?"
        )
        .bind(property_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| RegistryError::Database(e.to_string()))?;

        let detainee_id_str = row
            .ok_or_else(|| RegistryError::Database("property item not found".into()))?
            .0;

        sqlx::query("UPDATE property_items SET returned = 1 WHERE id = ?")
            .bind(property_id)
            .execute(&self.pool)
            .await
            .map_err(|e| RegistryError::Database(e.to_string()))?;

        let det_id = DetaineeId::from_uuid(parse_uuid(&detainee_id_str)?);
        let before = serde_json::json!({ "property_id": property_id, "returned": false });
        let after = serde_json::json!({ "property_id": property_id, "returned": true });
        let _ = audit::create_audit_entry(
            &self.pool, operator, ModuleName::Registry,
            "return_property_item", Some(det_id), Some(before), Some(after),
        ).await;

        Ok(())
    }

    async fn transfer(&self, record: TransferRecord) -> Result<Detainee, RegistryError> {
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

        // INSERT into transfers table
        sqlx::query(
            "INSERT INTO transfers (id, detainee_id, from_facility, to_facility, transfer_date, reason, authorized_by) \
             VALUES (?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(record.id.to_string())
        .bind(&id_str)
        .bind(&record.from_facility)
        .bind(&record.to_facility)
        .bind(record.transfer_date.as_naive().to_string())
        .bind(&record.reason)
        .bind(record.authorized_by.as_uuid().to_string())
        .execute(&self.pool)
        .await
        .map_err(|e| RegistryError::Database(e.to_string()))?;

        // UPDATE facility_status to Transferred
        sqlx::query("UPDATE detainees SET facility_status = 'Transferred', updated_at = ? WHERE id = ?")
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

        // Add note if reason provided
        if !record.reason.is_empty() {
            let note_content = format!("Transfer: {}", record.reason);
            sqlx::query(
                "INSERT INTO notes (detainee_id, content, author, timestamp, note_type) VALUES (?, ?, ?, ?, 'general')"
            )
            .bind(&id_str)
            .bind(&note_content)
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
            "transfer", Some(record.detainee_id), before, after,
        ).await;

        self.load_detainee(record.detainee_id).await
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
                "INSERT INTO notes (detainee_id, content, author, timestamp, note_type) VALUES (?, ?, ?, ?, 'general')"
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
        let total_matching = total.0 as u32;

        // Get page rows
        let select_sql = qb.select_query();
        let rows: Vec<DetaineeRow> = sqlx::query_as(&select_sql)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| RegistryError::Database(e.to_string()))?;

        // Load housing units for flag computation
        let housing_units = self.housing_units().await?;

        // Convert to summaries with flags
        let today = Utc::now().date_naive();
        let mut summaries = Vec::new();
        for row in &rows {
            let id = DetaineeId::from_uuid(parse_uuid(&row.id)?);
            let intake = parse_date(&row.intake_date)?;
            let days_held = (today - intake).num_days().max(0) as u32;

            let basis_label = parse_basis_label(&row.detention_basis_label);

            // Load full detainee to compute flags (Option A — simple, OK at Pi scale)
            let detainee = self.load_detainee(id).await?;
            let flags = crate::flags::compute_flags(&detainee, &self.config, &housing_units);

            summaries.push(DetaineeSummary {
                id,
                name: format!("{}, {}", row.surname, row.given_names),
                sex: parse_sex(&row.sex),
                age: None,
                detention_basis: basis_label,
                intake_date: PastDate::from_trusted(intake),
                days_held,
                bail_status: None,
                next_court_date: None,
                release_date: None,
                housing_unit: row.housing_unit_id.clone(),
                has_legal_representation: row.legal_representation.is_some(),
                flags,
            });
        }

        // Compute statistics over the full matching set
        let statistics = self
            .compute_search_statistics(&qb.where_clause(), total_matching, &housing_units)
            .await?;

        Ok(PopulationQueryResult {
            detainees: summaries,
            total_matching,
            statistics,
        })
    }

    async fn overview(&self) -> Result<FacilityOverview, RegistryError> {
        let today = Utc::now();
        let today_naive = today.date_naive();

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

        // --- SexBreakdown ---
        let sex_rows: Vec<(String, i64)> = sqlx::query_as(
            "SELECT sex, COUNT(*) FROM detainees \
             WHERE facility_status IN ('Present', 'InCourt', 'InHospital') \
             GROUP BY sex"
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RegistryError::Database(e.to_string()))?;

        let mut sex_breakdown = SexBreakdown::default();
        for (sex, count) in &sex_rows {
            match sex.as_str() {
                "Male" => sex_breakdown.male = *count as u32,
                "Female" => sex_breakdown.female = *count as u32,
                _ => sex_breakdown.other = *count as u32,
            }
        }

        // --- TimeDistribution ---
        let intake_rows: Vec<(String,)> = sqlx::query_as(
            "SELECT intake_date FROM detainees \
             WHERE facility_status IN ('Present', 'InCourt', 'InHospital')"
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RegistryError::Database(e.to_string()))?;

        let mut days_list: Vec<u32> = Vec::with_capacity(intake_rows.len());
        let mut time_dist = TimeDistribution::default();
        let mut total_days: u64 = 0;

        for (intake_str,) in &intake_rows {
            if let Ok(intake) = NaiveDate::parse_from_str(intake_str, "%Y-%m-%d") {
                let days = (today_naive - intake).num_days().max(0) as u32;
                days_list.push(days);
                total_days += days as u64;

                // Exclusive buckets
                if days < 2 {
                    time_dist.under_48_hours += 1;
                } else if days < 7 {
                    time_dist.under_1_week += 1;
                } else if days < 30 {
                    time_dist.under_1_month += 1;
                } else if days < 90 {
                    time_dist.under_3_months += 1;
                } else if days < 180 {
                    time_dist.under_6_months += 1;
                } else if days < 365 {
                    time_dist.under_1_year += 1;
                } else if days < 730 {
                    time_dist.under_2_years += 1;
                } else {
                    time_dist.over_2_years += 1;
                }
            }
        }

        if !days_list.is_empty() {
            time_dist.mean_days = total_days as f64 / days_list.len() as f64;
            days_list.sort_unstable();
            let mid = days_list.len() / 2;
            time_dist.median_days = if days_list.len() % 2 == 0 {
                (days_list[mid - 1] + days_list[mid]) / 2
            } else {
                days_list[mid]
            };
        }

        // --- FlagCounts + CriticalNumbers ---
        // Load all active detainees and housing units for flag computation
        let active_rows: Vec<DetaineeRow> = sqlx::query_as(
            "SELECT id, surname, given_names, preferred_name, sex, date_of_birth, \
             nationality, national_id, detention_basis_label, detention_basis_data, \
             facility_status, intake_date, housing_unit_id, identity_extra, \
             legal_reference, legal_representation, emergency_contacts \
             FROM detainees WHERE facility_status IN ('Present', 'InCourt', 'InHospital')"
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RegistryError::Database(e.to_string()))?;

        let housing_units = self.housing_units().await?;

        let mut all_flags: Vec<Vec<Flag>> = Vec::with_capacity(active_rows.len());
        for row in active_rows {
            let det_id_str = row.id.clone();

            let warrant_rows: Vec<WarrantRow> = sqlx::query_as(
                "SELECT id, detainee_id, order_type, order_data, external_reference, \
                 issuing_authority, issuing_officer, date_issued, date_received, valid_until, \
                 offence_description, sentence_details, is_active, registered_by, batch_id, \
                 document_hash \
                 FROM commitment_orders WHERE detainee_id = ? ORDER BY created_at"
            )
            .bind(&det_id_str)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| RegistryError::Database(e.to_string()))?;

            let alias_rows: Vec<(String,)> = sqlx::query_as(
                "SELECT alias FROM detainee_aliases WHERE detainee_id = ?"
            )
            .bind(&det_id_str)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| RegistryError::Database(e.to_string()))?;

            match row_to_detainee(row, warrant_rows, Vec::new(), Vec::new(), alias_rows) {
                Ok(detainee) => {
                    let flags = crate::flags::compute_flags(&detainee, &self.config, &housing_units);
                    all_flags.push(flags);
                }
                Err(_) => continue,
            }
        }

        let flag_counts = crate::flags::compute_flag_counts(&all_flags);

        // Build CriticalNumbers from the same flag data
        let mut critical = CriticalNumbers::default();
        for flags in &all_flags {
            for flag in flags {
                match flag {
                    Flag::NoLegalBasis { .. } => critical.no_legal_basis += 1,
                    Flag::CustodyLimitExceeded { .. } => critical.custody_limit_breaches += 1,
                    Flag::NoCourtDate { .. } => critical.no_court_date += 1,
                    Flag::ProlongedPreTrial { days_held, .. } if *days_held > 365 => {
                        critical.pretrial_over_1_year += 1;
                    }
                    Flag::BailGrantedStillHeld { .. } => critical.bail_granted_still_held += 1,
                    Flag::ReleaseDatePassed { .. } => critical.release_overdue += 1,
                    Flag::WarrantExpired { .. } => critical.warrant_expired += 1,
                    Flag::CourtDateImminent { .. } => critical.court_dates_48h += 1,
                    Flag::ReleaseImminent { .. } => critical.releases_7_days += 1,
                    _ => {}
                }
            }
        }

        // --- DailyCountStatus ---
        let today_str = today_naive.format("%Y-%m-%d").to_string();
        let dc_row: Option<DailyCountRow> = sqlx::query_as(
            "SELECT id, date, opening_count, admissions, transfers_in, court_returns, hospital_returns, \
             releases, transfers_out, to_court, to_hospital, escapes, deaths, computed_closing, \
             actual_closing_count, is_balanced, discrepancy_note, counted_by, finalized_by, finalized_at \
             FROM daily_counts WHERE date = ?"
        )
        .bind(&today_str)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| RegistryError::Database(e.to_string()))?;

        let today_count_status = match dc_row {
            None => DailyCountStatus::NotStarted,
            Some(row) => {
                if row.finalized_by.is_some() {
                    // Finalized
                    let closing = row.actual_closing_count.unwrap_or(row.computed_closing) as u32;
                    let balanced = row.is_balanced.map(|v| v != 0).unwrap_or(false);
                    DailyCountStatus::Finalized { closing, balanced }
                } else if row.actual_closing_count.is_some() {
                    // Submitted but not finalized
                    DailyCountStatus::Submitted {
                        computed: row.computed_closing as u32,
                        actual: row.actual_closing_count.unwrap() as u32,
                        balanced: row.is_balanced.map(|v| v != 0).unwrap_or(false),
                    }
                } else {
                    // Open
                    DailyCountStatus::Open {
                        computed_closing: row.computed_closing as u32,
                    }
                }
            }
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
            time_distribution: time_dist,
            flag_counts,
            sex_breakdown,
            today_count_status,
            critical,
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

    /// Compute aggregate statistics for a search result set.
    async fn compute_search_statistics(
        &self,
        where_clause: &str,
        total_matching: u32,
        housing_units: &[HousingUnit],
    ) -> Result<QueryStatistics, RegistryError> {
        let today_naive = Utc::now().date_naive();

        let sql = format!(
            "SELECT id, surname, given_names, preferred_name, sex, date_of_birth, \
             nationality, national_id, detention_basis_label, detention_basis_data, \
             facility_status, intake_date, housing_unit_id, identity_extra, \
             legal_reference, legal_representation, emergency_contacts \
             FROM detainees {}",
            where_clause
        );
        let rows: Vec<DetaineeRow> = sqlx::query_as(&sql)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| RegistryError::Database(e.to_string()))?;

        let mut basis = BasisBreakdown::default();
        let mut sex = SexBreakdown::default();
        let mut time_dist = TimeDistribution::default();
        let mut days_list: Vec<u32> = Vec::with_capacity(rows.len());
        let mut total_days: u64 = 0;
        let mut with_court = 0u32;
        let mut without_court = 0u32;
        let mut with_legal = 0u32;
        let mut without_legal = 0u32;
        let mut bail_granted = 0u32;
        let mut bail_not_applied = 0u32;
        let mut all_flags: Vec<Vec<Flag>> = Vec::with_capacity(rows.len());

        for row in rows {
            match row.detention_basis_label.as_str() {
                "NoLegalBasis" => basis.no_legal_basis += 1,
                "PoliceCustody" => basis.police_custody += 1,
                "Remand" => basis.remand += 1,
                "OnTrial" => basis.on_trial += 1,
                "ConvictedUnsentenced" => basis.convicted_unsentenced += 1,
                "Sentenced" => basis.sentenced += 1,
                "Appeal" => basis.appeal += 1,
                _ => {}
            }

            match row.sex.as_str() {
                "Male" => sex.male += 1,
                "Female" => sex.female += 1,
                _ => sex.other += 1,
            }

            if let Ok(intake) = NaiveDate::parse_from_str(&row.intake_date, "%Y-%m-%d") {
                let days = (today_naive - intake).num_days().max(0) as u32;
                days_list.push(days);
                total_days += days as u64;

                if days < 2 {
                    time_dist.under_48_hours += 1;
                } else if days < 7 {
                    time_dist.under_1_week += 1;
                } else if days < 30 {
                    time_dist.under_1_month += 1;
                } else if days < 90 {
                    time_dist.under_3_months += 1;
                } else if days < 180 {
                    time_dist.under_6_months += 1;
                } else if days < 365 {
                    time_dist.under_1_year += 1;
                } else if days < 730 {
                    time_dist.under_2_years += 1;
                } else {
                    time_dist.over_2_years += 1;
                }
            }

            if row.legal_representation.is_some() {
                with_legal += 1;
            } else {
                without_legal += 1;
            }

            if let Ok(det_basis) = serde_json::from_str::<DetentionBasis>(&row.detention_basis_data) {
                if det_basis.next_court_date().is_some() {
                    with_court += 1;
                } else {
                    without_court += 1;
                }
                if let Some(bail) = det_basis.bail_status() {
                    match bail {
                        BailStatus::Granted { .. } => bail_granted += 1,
                        BailStatus::NotApplied => bail_not_applied += 1,
                        _ => {}
                    }
                }
            } else {
                without_court += 1;
            }

            let det_id_str = row.id.clone();
            let warrant_rows: Vec<WarrantRow> = sqlx::query_as(
                "SELECT id, detainee_id, order_type, order_data, external_reference, \
                 issuing_authority, issuing_officer, date_issued, date_received, valid_until, \
                 offence_description, sentence_details, is_active, registered_by, batch_id, \
                 document_hash \
                 FROM commitment_orders WHERE detainee_id = ? ORDER BY created_at"
            )
            .bind(&det_id_str)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| RegistryError::Database(e.to_string()))?;

            let alias_rows: Vec<(String,)> = sqlx::query_as(
                "SELECT alias FROM detainee_aliases WHERE detainee_id = ?"
            )
            .bind(&det_id_str)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| RegistryError::Database(e.to_string()))?;

            if let Ok(detainee) = row_to_detainee(row, warrant_rows, Vec::new(), Vec::new(), alias_rows) {
                let flags = crate::flags::compute_flags(&detainee, &self.config, housing_units);
                all_flags.push(flags);
            }
        }

        if !days_list.is_empty() {
            time_dist.mean_days = total_days as f64 / days_list.len() as f64;
            days_list.sort_unstable();
            let mid = days_list.len() / 2;
            time_dist.median_days = if days_list.len() % 2 == 0 {
                (days_list[mid - 1] + days_list[mid]) / 2
            } else {
                days_list[mid]
            };
        }

        let flag_counts = crate::flags::compute_flag_counts(&all_flags);

        Ok(QueryStatistics {
            total: total_matching,
            by_detention_basis: basis,
            time_held: time_dist,
            flag_counts,
            with_court_date: with_court,
            without_court_date: without_court,
            with_legal_representation: with_legal,
            without_legal_representation: without_legal,
            bail_granted_still_held: bail_granted,
            bail_not_applied,
            by_sex: sex,
            facility_capacity: self.config.capacity,
            occupancy_rate: if self.config.capacity > 0 {
                total_matching as f64 / self.config.capacity as f64 * 100.0
            } else {
                0.0
            },
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

