use axum::{
    extract::State,
    http::HeaderMap,
    response::{IntoResponse, Redirect},
};
use openward_core::Registry;
use super::{require_session, nav_context, AppState_};
use crate::i18n::T;
use crate::templates::{DashboardTemplate, BasisSegment, AlertDisplay};

pub async fn dashboard(
    State(state): State<AppState_>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, Redirect> {
    let session = require_session(&headers, state.registry.pool()).await?;
    let t = T::new(&session.lang, state.legal_overrides.clone());
    let overview = state.registry.overview().await.map_err(|_| Redirect::to("/login"))?;

    let system_warning = match overview.system_status {
        openward_core::SystemStatus::Healthy => None,
        openward_core::SystemStatus::ClockUnsynchronized { system_time } => {
            Some(format!("{}: {}", t.get("dashboard-system-warning"), system_time))
        }
    };

    let (count_status, count_detail, count_badge_class) = match overview.today_count_status {
        openward_core::DailyCountStatus::NotStarted => {
            (t.get("count-not-started"), None, "badge-secondary".to_string())
        }
        openward_core::DailyCountStatus::Open { computed_closing } => {
            let detail = t.get_1("count-computed-closing", "count", &computed_closing.to_string());
            (t.get("count-in-progress"), Some(detail), "badge-info".to_string())
        }
        openward_core::DailyCountStatus::Submitted { computed, actual, balanced } => {
            let detail = t.get_1("count-computed", "computed", &computed.to_string());
            let balanced_str = if balanced { t.get("count-balanced") } else { t.get("count-unbalanced") };
            let detail = format!("{}, {}: {} - {}", detail, t.get("count-submitted"), actual, balanced_str);
            let badge_class = if balanced { "badge-success" } else { "badge-warning" };
            (t.get("count-submitted"), Some(detail), badge_class.to_string())
        }
        openward_core::DailyCountStatus::Finalized { closing, balanced } => {
            let balanced_str = if balanced { t.get("count-balanced") } else { t.get("count-unbalanced") };
            let detail = format!("{} - {}", t.get_1("count-final", "closing", &closing.to_string()), balanced_str);
            let badge_class = if balanced { "badge-success" } else { "badge-danger" };
            (t.get("count-finalized"), Some(detail), badge_class.to_string())
        }
    };

    let basis_breakdown = vec![
        BasisSegment {
            label_key: "legal-basis-no-legal".to_string(),
            count: overview.basis_breakdown.no_legal_basis,
            css_class: "basis-no-legal".to_string(),
            link: "/detainees?basis=NoLegalBasis".to_string(),
        },
        BasisSegment {
            label_key: "legal-basis-police-custody".to_string(),
            count: overview.basis_breakdown.police_custody,
            css_class: "basis-police".to_string(),
            link: "/detainees?basis=PoliceCustody".to_string(),
        },
        BasisSegment {
            label_key: "legal-basis-remand".to_string(),
            count: overview.basis_breakdown.remand,
            css_class: "basis-remand".to_string(),
            link: "/detainees?basis=RemandAwaitingTrial".to_string(),
        },
        BasisSegment {
            label_key: "legal-basis-on-trial".to_string(),
            count: overview.basis_breakdown.on_trial,
            css_class: "basis-trial".to_string(),
            link: "/detainees?basis=OnTrial".to_string(),
        },
        BasisSegment {
            label_key: "legal-basis-convicted-unsentenced".to_string(),
            count: overview.basis_breakdown.convicted_unsentenced,
            css_class: "basis-convicted".to_string(),
            link: "/detainees?basis=ConvictedUnsentenced".to_string(),
        },
        BasisSegment {
            label_key: "legal-basis-sentenced".to_string(),
            count: overview.basis_breakdown.sentenced,
            css_class: "basis-sentenced".to_string(),
            link: "/detainees?basis=Sentenced".to_string(),
        },
        BasisSegment {
            label_key: "legal-basis-appeal".to_string(),
            count: overview.basis_breakdown.appeal,
            css_class: "basis-appeal".to_string(),
            link: "/detainees?basis=SentencedOnAppeal".to_string(),
        },
    ];

    let mut alerts = Vec::new();
    if overview.critical.no_legal_basis > 0 {
        alerts.push(AlertDisplay {
            count: overview.critical.no_legal_basis,
            label_key: "legal-alert-no-legal-basis".to_string(),
            css_class: "alert-critical".to_string(),
            link: "/detainees?basis=NoLegalBasis".to_string(),
        });
    }
    if overview.critical.custody_limit_breaches > 0 {
        alerts.push(AlertDisplay {
            count: overview.critical.custody_limit_breaches,
            label_key: "legal-alert-custody-exceeded".to_string(),
            css_class: "alert-critical".to_string(),
            link: "/detainees?basis=PoliceCustody".to_string(),
        });
    }
    if overview.critical.no_court_date > 0 {
        alerts.push(AlertDisplay {
            count: overview.critical.no_court_date,
            label_key: "legal-alert-no-court-date".to_string(),
            css_class: "alert-warning".to_string(),
            link: "/detainees?has_court_date=no".to_string(),
        });
    }
    if overview.critical.pretrial_over_1_year > 0 {
        alerts.push(AlertDisplay {
            count: overview.critical.pretrial_over_1_year,
            label_key: "legal-alert-prolonged-pretrial".to_string(),
            css_class: "alert-warning".to_string(),
            link: "/detainees?basis=AnyPreTrial&held_min=365".to_string(),
        });
    }
    if overview.critical.bail_granted_still_held > 0 {
        alerts.push(AlertDisplay {
            count: overview.critical.bail_granted_still_held,
            label_key: "legal-alert-bail-still-held".to_string(),
            css_class: "alert-critical".to_string(),
            link: "/detainees?bail=GrantedStillHeld".to_string(),
        });
    }
    if overview.critical.release_overdue > 0 {
        alerts.push(AlertDisplay {
            count: overview.critical.release_overdue,
            label_key: "legal-alert-release-overdue".to_string(),
            css_class: "alert-critical".to_string(),
            link: "/detainees?release_overdue=yes".to_string(),
        });
    }
    if overview.critical.warrant_expired > 0 {
        alerts.push(AlertDisplay {
            count: overview.critical.warrant_expired,
            label_key: "legal-alert-warrant-expired".to_string(),
            css_class: "alert-warning".to_string(),
            link: "/detainees?basis=AnyPreTrial".to_string(),
        });
    }
    if overview.critical.court_dates_48h > 0 {
        alerts.push(AlertDisplay {
            count: overview.critical.court_dates_48h,
            label_key: "legal-alert-court-48h".to_string(),
            css_class: "alert-info".to_string(),
            link: "/detainees?release_within=2".to_string(),
        });
    }
    if overview.critical.releases_7_days > 0 {
        alerts.push(AlertDisplay {
            count: overview.critical.releases_7_days,
            label_key: "legal-alert-release-7d".to_string(),
            css_class: "alert-info".to_string(),
            link: "/detainees?release_within=7".to_string(),
        });
    }

    Ok(DashboardTemplate {
        nav: nav_context(&session, "/"),
        total_population: overview.total_population,
        facility_capacity: overview.facility_capacity,
        occupancy_percent: overview.occupancy_percent as u32,
        pretrial_percent: overview.pretrial_percent as u32,
        as_of: overview.as_of.format("%Y-%m-%d %H:%M").to_string(),
        system_warning,
        count_status,
        count_detail,
        count_badge_class,
        basis_breakdown,
        alerts,
        t,
    })
}
