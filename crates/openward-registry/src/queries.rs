use openward_core::{
    DetentionBasisFilter, FacilityStatus, PopulationQuery, SortField, SortOrder,
};

/// Builds a dynamic SQL WHERE clause and ORDER BY from a PopulationQuery.
///
/// Returns (where_clause, order_clause, bind_values) where bind_values
/// are strings that will be bound as parameters.
pub struct QueryBuilder {
    conditions: Vec<String>,
    #[allow(dead_code)]
    binds: Vec<String>,
    order_clause: String,
    limit_offset: String,
}

impl QueryBuilder {
    pub fn from_query(query: &PopulationQuery) -> Self {
        let mut builder = Self {
            conditions: Vec::new(),
            binds: Vec::new(),
            order_clause: String::new(),
            limit_offset: String::new(),
        };

        // Detention basis filter
        if let Some(basis) = &query.detention_basis {
            match basis {
                DetentionBasisFilter::NoLegalBasis => {
                    builder.conditions.push("detention_basis_label = 'NoLegalBasis'".into());
                }
                DetentionBasisFilter::PoliceCustody => {
                    builder.conditions.push("detention_basis_label = 'PoliceCustody'".into());
                }
                DetentionBasisFilter::RemandAwaitingTrial => {
                    builder.conditions.push("detention_basis_label = 'Remand'".into());
                }
                DetentionBasisFilter::OnTrial => {
                    builder.conditions.push("detention_basis_label = 'OnTrial'".into());
                }
                DetentionBasisFilter::ConvictedUnsentenced => {
                    builder.conditions.push("detention_basis_label = 'ConvictedUnsentenced'".into());
                }
                DetentionBasisFilter::Sentenced => {
                    builder.conditions.push("detention_basis_label = 'Sentenced'".into());
                }
                DetentionBasisFilter::SentencedOnAppeal => {
                    builder.conditions.push("detention_basis_label = 'Appeal'".into());
                }
                DetentionBasisFilter::AnyPreTrial => {
                    builder.conditions.push(
                        "detention_basis_label IN ('PoliceCustody', 'Remand', 'OnTrial', 'ConvictedUnsentenced')".into()
                    );
                }
                DetentionBasisFilter::Undocumented => {
                    builder.conditions.push("detention_basis_label = 'NoLegalBasis'".into());
                }
            }
        }

        // Facility status filter
        if let Some(status) = &query.facility_status {
            let status_str = match status {
                FacilityStatus::Present => "Present",
                FacilityStatus::InCourt => "InCourt",
                FacilityStatus::InHospital => "InHospital",
                FacilityStatus::Transferred => "Transferred",
                FacilityStatus::Released => "Released",
                FacilityStatus::Escaped => "Escaped",
                FacilityStatus::Deceased => "Deceased",
            };
            builder.conditions.push(format!("facility_status = '{}'", status_str));
        }

        // Sex filter
        if let Some(sex) = &query.sex {
            builder.conditions.push(format!("sex = '{:?}'", sex));
        }

        // Housing unit filter
        if let Some(unit_id) = &query.housing_unit {
            builder.conditions.push(format!(
                "housing_unit_id = '{}'",
                unit_id.as_uuid()
            ));
        }

        // Has legal representation
        if let Some(has_rep) = query.has_legal_representation {
            if has_rep {
                builder.conditions.push("legal_representation IS NOT NULL".into());
            } else {
                builder.conditions.push("legal_representation IS NULL".into());
            }
        }

        // Days held filters
        if let Some(days) = query.held_longer_than_days {
            builder.conditions.push(format!(
                "julianday('now') - julianday(intake_date) > {}",
                days
            ));
        }
        if let Some(days) = query.held_shorter_than_days {
            builder.conditions.push(format!(
                "julianday('now') - julianday(intake_date) < {}",
                days
            ));
        }

        // Intake date range
        if let Some(range) = &query.intake_date_range {
            if let Some(from) = range.from {
                builder.conditions.push(format!("intake_date >= '{}'", from));
            }
            if let Some(to) = range.to {
                builder.conditions.push(format!("intake_date <= '{}'", to));
            }
        }

        // Sort
        let order_col = match query.sort_by.unwrap_or(SortField::IntakeDate) {
            SortField::IntakeDate => "intake_date",
            SortField::DaysHeld => "intake_date", // earlier intake = more days held
            SortField::Name => "surname",
            SortField::DetentionBasis => "detention_basis_label",
            SortField::NextCourtDate => "intake_date", // fallback
            SortField::ReleaseDate => "intake_date",    // fallback
            SortField::FlagSeverity => "intake_date",   // fallback
        };
        let order_dir = match query.sort_order.unwrap_or(SortOrder::Desc) {
            SortOrder::Asc => "ASC",
            SortOrder::Desc => "DESC",
        };
        // For DaysHeld, we reverse direction since we sort by intake_date
        let effective_dir = if matches!(query.sort_by, Some(SortField::DaysHeld)) {
            if order_dir == "ASC" { "DESC" } else { "ASC" }
        } else {
            order_dir
        };
        builder.order_clause = format!("ORDER BY {} {}", order_col, effective_dir);

        // Limit/Offset
        if let Some(limit) = query.limit {
            builder.limit_offset = format!("LIMIT {}", limit);
            if let Some(offset) = query.offset {
                builder.limit_offset = format!("LIMIT {} OFFSET {}", limit, offset);
            }
        }

        builder
    }

    /// Build the WHERE clause (without the "WHERE" keyword if empty).
    pub fn where_clause(&self) -> String {
        if self.conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", self.conditions.join(" AND "))
        }
    }

    /// Build the full query suffix: WHERE ... ORDER BY ... LIMIT ...
    pub fn suffix(&self) -> String {
        let mut parts = Vec::new();
        let wc = self.where_clause();
        if !wc.is_empty() {
            parts.push(wc);
        }
        if !self.order_clause.is_empty() {
            parts.push(self.order_clause.clone());
        }
        if !self.limit_offset.is_empty() {
            parts.push(self.limit_offset.clone());
        }
        parts.join(" ")
    }

    /// Build a COUNT query.
    pub fn count_query(&self) -> String {
        let wc = self.where_clause();
        if wc.is_empty() {
            "SELECT COUNT(*) FROM detainees".to_string()
        } else {
            format!("SELECT COUNT(*) FROM detainees {}", wc)
        }
    }

    /// Build the SELECT query.
    pub fn select_query(&self) -> String {
        format!("SELECT * FROM detainees {}", self.suffix())
    }
}
