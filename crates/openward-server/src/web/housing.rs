use axum::{
    extract::{Form, Path, State},
    http::HeaderMap,
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use uuid::Uuid;
use openward_core::*;
use super::{require_session, require_role, nav_context, AppState_};
use crate::auth::Action;
use crate::i18n::T;
use crate::templates::*;
use crate::web::errors::HtmlError;
use crate::web::util;

async fn build_unit_displays(state: &AppState_, t: &T) -> Result<Vec<HousingUnitFullDisplay>, HtmlError> {
    let units = state.registry.housing_units().await?;
    let mut displays = Vec::with_capacity(units.len());
    for unit in &units {
        let occupancy = state.registry.housing_unit_occupancy(unit.id).await?;
        displays.push(HousingUnitFullDisplay {
            id: unit.id.to_string(),
            name: unit.name.clone(),
            capacity: unit.capacity,
            occupancy,
            unit_type: t.get(util::housing_type_key(&unit.unit_type)),
            unit_type_code: util::housing_type_code(&unit.unit_type).to_string(),
            designated_sex: unit.designated_sex.as_ref().map(|s| t.get(util::sex_key(s))),
            designated_sex_code: unit.designated_sex.as_ref().map(|s| util::sex_code(s).to_string()),
            designated_age_group: unit.designated_age_group.as_ref().map(|a| t.get(util::age_group_key(a))),
            designated_age_group_code: unit.designated_age_group.as_ref().map(|a| util::age_group_code(a).to_string()),
        });
    }
    Ok(displays)
}

pub async fn housing_page(
    State(state): State<AppState_>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, HtmlError> {
    let session = require_session(&headers, state.registry.pool()).await.map_err(|_| HtmlError::redirect("/login"))?;
    let t = T::new(&session.lang, state.legal_overrides.clone());
    let displays = build_unit_displays(&state, &t).await?;

    let total_capacity: u32 = displays.iter().map(|u| u.capacity).sum();
    let total_occupancy: u32 = displays.iter().map(|u| u.occupancy).sum();

    Ok(HousingPageTemplate {
        nav: nav_context(&session, "/housing"),
        units: displays,
        total_capacity,
        total_occupancy,
        t,
    })
}

pub async fn create_form(
    State(state): State<AppState_>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, HtmlError> {
    let session = require_role(&headers, state.registry.pool(), Action::ManageHousing).await?;
    let t = T::new(&session.lang, state.legal_overrides.clone());
    Ok(HousingCreateFormFragment { t })
}

#[derive(Deserialize)]
pub struct HousingUnitForm {
    pub name: String,
    pub capacity: u32,
    pub unit_type: String,
    pub designated_sex: Option<String>,
    pub designated_age_group: Option<String>,
}

fn parse_housing_type(s: &str) -> HousingType {
    match s {
        "Medical" => HousingType::Medical,
        "Isolation" => HousingType::Isolation,
        "Protective" => HousingType::Protective,
        "PreTrial" => HousingType::PreTrial,
        _ => HousingType::General,
    }
}

fn parse_optional_sex(s: &Option<String>) -> Option<Sex> {
    s.as_deref().and_then(|v| match v {
        "Male" => Some(Sex::Male),
        "Female" => Some(Sex::Female),
        _ => None,
    })
}

fn parse_optional_age_group(s: &Option<String>) -> Option<AgeGroup> {
    s.as_deref().and_then(|v| match v {
        "Adult" => Some(AgeGroup::Adult),
        "Juvenile" => Some(AgeGroup::Juvenile),
        _ => None,
    })
}

pub async fn create_unit(
    State(state): State<AppState_>,
    headers: HeaderMap,
    Form(form): Form<HousingUnitForm>,
) -> Result<Response, HtmlError> {
    let _session = require_role(&headers, state.registry.pool(), Action::ManageHousing).await?;

    let name = form.name.trim().to_string();
    if name.is_empty() {
        return Err(HtmlError::validation("error-name-required"));
    }
    if form.capacity == 0 {
        return Err(HtmlError::validation("error-capacity-min"));
    }

    let unit = HousingUnit {
        id: HousingUnitId::from_uuid(Uuid::new_v4()),
        name,
        capacity: form.capacity,
        unit_type: parse_housing_type(&form.unit_type),
        designated_sex: parse_optional_sex(&form.designated_sex),
        designated_age_group: parse_optional_age_group(&form.designated_age_group),
    };

    state.registry.create_housing_unit(unit).await?;
    Ok(util::hx_redirect(&headers, "/housing"))
}

pub async fn edit_form(
    State(state): State<AppState_>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, HtmlError> {
    let session = require_role(&headers, state.registry.pool(), Action::ManageHousing).await?;
    let t = T::new(&session.lang, state.legal_overrides.clone());
    let uuid = Uuid::parse_str(&id)
        .map_err(|_| HtmlError::validation("error-invalid-id"))?;
    let unit = state.registry.get_housing_unit(HousingUnitId::from_uuid(uuid)).await?;
    let occupancy = state.registry.housing_unit_occupancy(unit.id).await?;

    let display = HousingUnitFullDisplay {
        id: unit.id.to_string(),
        name: unit.name.clone(),
        capacity: unit.capacity,
        occupancy,
        unit_type: t.get(util::housing_type_key(&unit.unit_type)),
        unit_type_code: util::housing_type_code(&unit.unit_type).to_string(),
        designated_sex: unit.designated_sex.as_ref().map(|s| t.get(util::sex_key(s))),
        designated_sex_code: unit.designated_sex.as_ref().map(|s| util::sex_code(s).to_string()),
        designated_age_group: unit.designated_age_group.as_ref().map(|a| t.get(util::age_group_key(a))),
        designated_age_group_code: unit.designated_age_group.as_ref().map(|a| util::age_group_code(a).to_string()),
    };

    Ok(HousingEditFormFragment { unit: display, t })
}

pub async fn update_unit(
    State(state): State<AppState_>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Form(form): Form<HousingUnitForm>,
) -> Result<Response, HtmlError> {
    let _session = require_role(&headers, state.registry.pool(), Action::ManageHousing).await?;
    let uuid = Uuid::parse_str(&id)
        .map_err(|_| HtmlError::validation("error-invalid-id"))?;

    let name = form.name.trim().to_string();
    if name.is_empty() {
        return Err(HtmlError::validation("error-name-required"));
    }

    let unit = HousingUnit {
        id: HousingUnitId::from_uuid(uuid),
        name,
        capacity: form.capacity,
        unit_type: parse_housing_type(&form.unit_type),
        designated_sex: parse_optional_sex(&form.designated_sex),
        designated_age_group: parse_optional_age_group(&form.designated_age_group),
    };

    state.registry.update_housing_unit(unit).await?;
    Ok(util::hx_redirect(&headers, "/housing"))
}

pub async fn delete_unit(
    State(state): State<AppState_>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Response, HtmlError> {
    let _session = require_role(&headers, state.registry.pool(), Action::ManageHousing).await?;
    let uuid = Uuid::parse_str(&id)
        .map_err(|_| HtmlError::validation("error-invalid-id"))?;

    state.registry.delete_housing_unit(HousingUnitId::from_uuid(uuid)).await?;
    Ok(util::hx_redirect(&headers, "/housing"))
}
