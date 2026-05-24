use openward_core::{AuditEntry, DailyCount, DetaineeSummary};

/// Generate CSV bytes for the current population.
pub fn population_csv(detainees: &[DetaineeSummary]) -> Vec<u8> {
    let mut wtr = csv::Writer::from_writer(Vec::new());
    wtr.write_record([
        "id",
        "name",
        "sex",
        "detention_basis",
        "intake_date",
        "days_held",
        "housing_unit",
        "has_legal_rep",
        "flags",
    ])
    .unwrap();

    for d in detainees {
        let flags: Vec<String> = d.flags.iter().map(|f| format!("{:?}", f.flag_type())).collect();
        wtr.write_record(&[
            d.id.to_string(),
            d.name.clone(),
            format!("{:?}", d.sex),
            format!("{:?}", d.detention_basis),
            d.intake_date.as_naive().to_string(),
            d.days_held.to_string(),
            d.housing_unit.clone().unwrap_or_default(),
            d.has_legal_representation.to_string(),
            flags.join("; "),
        ])
        .unwrap();
    }

    wtr.into_inner().unwrap()
}

/// Generate CSV bytes for daily counts.
pub fn daily_counts_csv(counts: &[DailyCount]) -> Vec<u8> {
    let mut wtr = csv::Writer::from_writer(Vec::new());
    wtr.write_record([
        "date",
        "opening",
        "admissions",
        "releases",
        "transfers_out",
        "to_court",
        "to_hospital",
        "escapes",
        "deaths",
        "computed_closing",
        "actual_closing",
        "balanced",
    ])
    .unwrap();

    for c in counts {
        wtr.write_record(&[
            c.date.to_string(),
            c.opening_count.to_string(),
            c.admissions.to_string(),
            c.releases.to_string(),
            c.transfers_out.to_string(),
            c.to_court.to_string(),
            c.to_hospital.to_string(),
            c.escapes.to_string(),
            c.deaths.to_string(),
            c.computed_closing.to_string(),
            c.actual_closing_count.map(|v| v.to_string()).unwrap_or_default(),
            c.is_balanced.map(|b| b.to_string()).unwrap_or_default(),
        ])
        .unwrap();
    }

    wtr.into_inner().unwrap()
}

/// Generate CSV bytes for audit entries.
pub fn audit_csv(entries: &[AuditEntry]) -> Vec<u8> {
    let mut wtr = csv::Writer::from_writer(Vec::new());
    wtr.write_record([
        "id",
        "timestamp",
        "operator",
        "module",
        "action",
        "target",
        "before",
        "after",
    ])
    .unwrap();

    for e in entries {
        wtr.write_record(&[
            e.id.to_string(),
            e.timestamp.to_rfc3339(),
            e.operator.to_string(),
            e.module.to_string(),
            e.action.clone(),
            e.target.map(|t| t.to_string()).unwrap_or_default(),
            e.before.as_ref().map(|v| v.to_string()).unwrap_or_default(),
            e.after.as_ref().map(|v| v.to_string()).unwrap_or_default(),
        ])
        .unwrap();
    }

    wtr.into_inner().unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use openward_core::*;

    #[test]
    fn test_population_csv_headers() {
        let csv = population_csv(&[]);
        let text = String::from_utf8(csv).unwrap();
        assert!(text.starts_with("id,name,sex,detention_basis,intake_date,days_held,housing_unit,has_legal_rep,flags\n"));
    }

    #[test]
    fn test_daily_counts_csv_headers() {
        let csv = daily_counts_csv(&[]);
        let text = String::from_utf8(csv).unwrap();
        assert!(text.starts_with("date,opening,admissions,releases,transfers_out,to_court,to_hospital,escapes,deaths,computed_closing,actual_closing,balanced\n"));
    }

    #[test]
    fn test_audit_csv_headers() {
        let csv = audit_csv(&[]);
        let text = String::from_utf8(csv).unwrap();
        assert!(text.starts_with("id,timestamp,operator,module,action,target,before,after\n"));
    }

    #[test]
    fn test_population_csv_with_data() {
        let detainees = vec![DetaineeSummary {
            id: DetaineeId::new(),
            name: "Doe, John".to_string(),
            sex: Sex::Male,
            age: None,
            detention_basis: DetentionBasisLabel::Remand,
            intake_date: PastDate::from_trusted(chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap()),
            days_held: 78,
            bail_status: None,
            next_court_date: None,
            release_date: None,
            housing_unit: Some("Block A".to_string()),
            has_legal_representation: true,
            flags: vec![],
        }];
        let csv = population_csv(&detainees);
        let text = String::from_utf8(csv).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[1].contains("Doe"));
        assert!(lines[1].contains("Block A"));
    }
}
