use axum::{
    extract::{Query, State},
    http::HeaderMap,
    response::{IntoResponse, Redirect},
};
use openward_core::{
    BailStatusFilter, DetentionBasisFilter, PopulationQuery, Registry, SortField, SortOrder,
};
use serde::{Deserialize, Deserializer};
use super::{require_session, nav_context, AppState_};
use crate::i18n::T;
use crate::templates::{
    FilterState, PopulationPageTemplate, PopulationTableFragment, DetaineeSummaryDisplay,
};
use crate::web::util;

const PAGE_SIZE: u32 = 25;

/// Deserialize an `Option<u32>` that tolerates empty strings as `None`.
///
/// HTML `<input type="number">` fields submit as `foo=` (empty string) when
/// cleared, which Axum's default `Query<T>` extractor cannot parse into
/// `Option<u32>` — the request would fail with a 4xx that HTMX silently
/// swallows, making every filter change look broken to the user.
fn deserialize_optional_u32<'de, D>(deserializer: D) -> Result<Option<u32>, D::Error>
where
    D: Deserializer<'de>,
{
    let opt: Option<String> = Option::deserialize(deserializer)?;
    match opt.as_deref().map(str::trim) {
        None | Some("") => Ok(None),
        Some(s) => s.parse::<u32>().map(Some).map_err(serde::de::Error::custom),
    }
}

/// HTML form deserialization struct that maps HTML-friendly field names
/// to `PopulationQuery`. Avoids polluting the core struct with serde renames.
#[derive(Debug, Default, Deserialize)]
pub struct SearchFormParams {
    pub name: Option<String>,
    pub basis: Option<String>,
    pub bail: Option<String>,
    pub severity: Option<String>,
    pub sex: Option<String>,
    pub has_court_date: Option<String>,
    pub has_legal_rep: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_u32")]
    pub held_min: Option<u32>,
    #[serde(default, deserialize_with = "deserialize_optional_u32")]
    pub held_max: Option<u32>,
    pub court_overdue: Option<String>,
    pub release_overdue: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_u32")]
    pub release_within: Option<u32>,
    pub flag: Option<String>,
    pub sort: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_u32")]
    pub page: Option<u32>,
}

impl SearchFormParams {
    fn into_query(self) -> PopulationQuery {
        let mut q = PopulationQuery::default();

        // Name search
        if let Some(ref name) = self.name {
            let trimmed = name.trim();
            if !trimmed.is_empty() {
                q.name_search = Some(trimmed.to_string());
            }
        }

        // Detention basis
        q.detention_basis = self.basis.as_deref().and_then(|b| match b {
            "NoLegalBasis" => Some(DetentionBasisFilter::NoLegalBasis),
            "PoliceCustody" => Some(DetentionBasisFilter::PoliceCustody),
            "RemandAwaitingTrial" => Some(DetentionBasisFilter::RemandAwaitingTrial),
            "OnTrial" => Some(DetentionBasisFilter::OnTrial),
            "ConvictedUnsentenced" => Some(DetentionBasisFilter::ConvictedUnsentenced),
            "Sentenced" => Some(DetentionBasisFilter::Sentenced),
            "SentencedOnAppeal" => Some(DetentionBasisFilter::SentencedOnAppeal),
            "AnyPreTrial" => Some(DetentionBasisFilter::AnyPreTrial),
            "Undocumented" => Some(DetentionBasisFilter::Undocumented),
            _ => None,
        });

        // Bail status
        q.bail_status = self.bail.as_deref().and_then(|b| match b {
            "NotApplied" => Some(BailStatusFilter::NotApplied),
            "Applied" => Some(BailStatusFilter::Applied),
            "GrantedStillHeld" => Some(BailStatusFilter::GrantedStillHeld),
            "Denied" => Some(BailStatusFilter::Denied),
            "Revoked" => Some(BailStatusFilter::Revoked),
            _ => None,
        });

        // Charge severity
        q.charge_severity = self.severity.as_deref().and_then(|s| match s {
            "Minor" => Some(openward_core::ChargeSeverity::Minor),
            "Moderate" => Some(openward_core::ChargeSeverity::Moderate),
            "Serious" => Some(openward_core::ChargeSeverity::Serious),
            _ => None,
        });

        // Sex
        q.sex = self.sex.as_deref().and_then(|s| match s {
            "Male" => Some(openward_core::Sex::Male),
            "Female" => Some(openward_core::Sex::Female),
            "Other" => Some(openward_core::Sex::Other),
            _ => None,
        });

        // Boolean toggles
        q.has_court_date = self.has_court_date.as_deref().and_then(|v| match v {
            "yes" => Some(true),
            "no" => Some(false),
            _ => None,
        });

        q.has_legal_representation = self.has_legal_rep.as_deref().and_then(|v| match v {
            "yes" => Some(true),
            "no" => Some(false),
            _ => None,
        });

        q.court_date_overdue = self.court_overdue.as_deref().map(|v| v == "yes");
        if q.court_date_overdue == Some(false) {
            q.court_date_overdue = None;
        }

        q.release_overdue = self.release_overdue.as_deref().map(|v| v == "yes");
        if q.release_overdue == Some(false) {
            q.release_overdue = None;
        }

        // Days held
        q.held_longer_than_days = self.held_min;
        q.held_shorter_than_days = self.held_max;

        // Release within
        q.release_within_days = self.release_within;

        // Sort
        if let Some(ref sort) = self.sort {
            match sort.as_str() {
                "name" => {
                    q.sort_by = Some(SortField::Name);
                    q.sort_order = Some(SortOrder::Asc);
                }
                "intake_date_desc" => {
                    q.sort_by = Some(SortField::IntakeDate);
                    q.sort_order = Some(SortOrder::Desc);
                }
                "intake_date_asc" => {
                    q.sort_by = Some(SortField::IntakeDate);
                    q.sort_order = Some(SortOrder::Asc);
                }
                "days_held_desc" => {
                    q.sort_by = Some(SortField::DaysHeld);
                    q.sort_order = Some(SortOrder::Desc);
                }
                "days_held_asc" => {
                    q.sort_by = Some(SortField::DaysHeld);
                    q.sort_order = Some(SortOrder::Asc);
                }
                _ => {}
            }
        }

        // Pagination
        if let Some(page) = self.page {
            if page > 1 {
                q.offset = Some((page - 1) * PAGE_SIZE);
            }
        }

        q
    }

    fn to_filter_state(&self) -> FilterState {
        FilterState {
            name: self.name.clone().unwrap_or_default(),
            basis: self.basis.clone().unwrap_or_default(),
            bail: self.bail.clone().unwrap_or_default(),
            severity: self.severity.clone().unwrap_or_default(),
            sex: self.sex.clone().unwrap_or_default(),
            has_court_date: self.has_court_date.clone().unwrap_or_default(),
            has_legal_rep: self.has_legal_rep.clone().unwrap_or_default(),
            held_min: self.held_min.map(|v| v.to_string()).unwrap_or_default(),
            held_max: self.held_max.map(|v| v.to_string()).unwrap_or_default(),
            court_overdue: self.court_overdue.clone().unwrap_or_default(),
            release_overdue: self.release_overdue.clone().unwrap_or_default(),
            release_within: self.release_within.map(|v| v.to_string()).unwrap_or_default(),
            flag: self.flag.clone().unwrap_or_default(),
            sort: self.sort.clone().unwrap_or_default(),
        }
    }
}

fn convert_summary(d: openward_core::DetaineeSummary, t: &T) -> DetaineeSummaryDisplay {
    let flag_labels: Vec<String> = d.flags.iter().map(|f| util::flag_label(t, f)).collect();
    DetaineeSummaryDisplay {
        id: d.id.to_string(),
        name: d.name,
        sex: t.get(util::sex_key(&d.sex)),
        detention_basis: t.get(util::basis_key(&d.detention_basis)),
        intake_date: util::format_date(&d.intake_date.as_naive()),
        days_held: d.days_held,
        next_court_date: d.next_court_date.map(|date| util::format_date(&date)),
        flags: flag_labels,
    }
}

pub async fn population_page(
    State(state): State<AppState_>,
    headers: HeaderMap,
    Query(params): Query<SearchFormParams>,
) -> Result<impl IntoResponse, Redirect> {
    let session = require_session(&headers, state.registry.pool()).await?;
    let t = T::new(&session.lang, state.legal_overrides.clone());
    let filters = params.to_filter_state();
    let mut query = params.into_query();
    let page = query.offset.unwrap_or(0) / PAGE_SIZE + 1;
    if query.limit.is_none() {
        query.limit = Some(PAGE_SIZE);
    }
    let result = state.registry.search(query, session.operator_id).await.map_err(|_| Redirect::to("/"))?;
    let total_pages = (result.total_matching + PAGE_SIZE - 1) / PAGE_SIZE;
    let detainees: Vec<DetaineeSummaryDisplay> = result.detainees.into_iter().map(|d| convert_summary(d, &t)).collect();

    Ok(PopulationPageTemplate {
        nav: nav_context(&session, "/detainees"),
        detainees,
        total_matching: result.total_matching,
        page,
        total_pages,
        filters,
        t,
    })
}

pub async fn search_fragment(
    State(state): State<AppState_>,
    headers: HeaderMap,
    Query(params): Query<SearchFormParams>,
) -> Result<impl IntoResponse, Redirect> {
    let session = require_session(&headers, state.registry.pool()).await?;
    let t = T::new(&session.lang, state.legal_overrides.clone());
    let mut query = params.into_query();
    let page = query.offset.unwrap_or(0) / PAGE_SIZE + 1;
    if query.limit.is_none() {
        query.limit = Some(PAGE_SIZE);
    }
    let result = state.registry.search(query, session.operator_id).await.map_err(|_| Redirect::to("/"))?;
    let total_pages = (result.total_matching + PAGE_SIZE - 1) / PAGE_SIZE;
    let detainees: Vec<DetaineeSummaryDisplay> = result.detainees.into_iter().map(|d| convert_summary(d, &t)).collect();

    Ok(PopulationTableFragment {
        detainees,
        total_matching: result.total_matching,
        page,
        total_pages,
        t,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::extract::Query as AxumQuery;
    use axum::http::Uri;

    fn parse(qs: &str) -> SearchFormParams {
        let uri: Uri = format!("http://test/?{}", qs).parse().unwrap();
        let AxumQuery(p) = AxumQuery::<SearchFormParams>::try_from_uri(&uri)
            .expect("query must parse");
        p
    }

    /// Regression: HTML `<input type="number">` submits `held_min=` (empty
    /// string) when cleared.  Axum's `Query<T>` extractor must accept that as
    /// `None`, otherwise every filter change on the population page fails with
    /// a 4xx that HTMX silently swallows.
    #[test]
    fn search_params_tolerate_empty_numeric_fields() {
        let params = parse("name=alice&basis=Sentenced&held_min=&held_max=&release_within=&page=");
        assert_eq!(params.name.as_deref(), Some("alice"));
        assert_eq!(params.basis.as_deref(), Some("Sentenced"));
        assert_eq!(params.held_min, None);
        assert_eq!(params.held_max, None);
        assert_eq!(params.release_within, None);
        assert_eq!(params.page, None);
    }

    #[test]
    fn search_params_parse_populated_numeric_fields() {
        let params = parse("held_min=7&held_max=90&release_within=30&page=3");
        assert_eq!(params.held_min, Some(7));
        assert_eq!(params.held_max, Some(90));
        assert_eq!(params.release_within, Some(30));
        assert_eq!(params.page, Some(3));
    }

    #[test]
    fn into_query_wires_all_filters() {
        let params = SearchFormParams {
            name: Some("  Alice ".to_string()),
            basis: Some("Sentenced".to_string()),
            bail: Some("Denied".to_string()),
            severity: Some("Serious".to_string()),
            sex: Some("Female".to_string()),
            has_court_date: Some("yes".to_string()),
            has_legal_rep: Some("no".to_string()),
            held_min: Some(10),
            held_max: Some(90),
            court_overdue: Some("yes".to_string()),
            release_overdue: Some("yes".to_string()),
            release_within: Some(30),
            flag: None,
            sort: Some("days_held_desc".to_string()),
            page: Some(2),
        };
        let q = params.into_query();
        assert_eq!(q.name_search.as_deref(), Some("Alice"));
        assert!(matches!(q.detention_basis, Some(DetentionBasisFilter::Sentenced)));
        assert!(matches!(q.bail_status, Some(BailStatusFilter::Denied)));
        assert!(matches!(q.charge_severity, Some(openward_core::ChargeSeverity::Serious)));
        assert!(matches!(q.sex, Some(openward_core::Sex::Female)));
        assert_eq!(q.has_court_date, Some(true));
        assert_eq!(q.has_legal_representation, Some(false));
        assert_eq!(q.held_longer_than_days, Some(10));
        assert_eq!(q.held_shorter_than_days, Some(90));
        assert_eq!(q.court_date_overdue, Some(true));
        assert_eq!(q.release_overdue, Some(true));
        assert_eq!(q.release_within_days, Some(30));
        assert!(matches!(q.sort_by, Some(SortField::DaysHeld)));
        assert!(matches!(q.sort_order, Some(SortOrder::Desc)));
        assert_eq!(q.offset, Some(PAGE_SIZE));
    }
}
