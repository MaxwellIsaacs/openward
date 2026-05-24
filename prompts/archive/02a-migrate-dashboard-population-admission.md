# Phase 4a: Migrate Dashboard, Population, and Admission

## Prerequisites

Phase 2+3 (prompt 01) must be complete. All template structs in `templates.rs` already have `t: T`. `filters.rs` is deleted. `require_session()` returns `(OperatorId, String)`.

Read `specs/i18n.md` for the full string inventory and key names.

## General Pattern for Each Handler

Every handler follows the same migration pattern:

```rust
// Before:
pub async fn handler(State(state): State<AppState_>, headers: HeaderMap) -> ... {
    let operator = require_session(&headers)?;
    // ... build template with hardcoded French strings ...
}

// After:
pub async fn handler(State(state): State<AppState_>, headers: HeaderMap) -> ... {
    let (operator, lang) = require_session(&headers)?;
    let t = T::new(&lang, state.legal_overrides.clone());
    // ... build template using t.get() or fluent keys ...
    // ... pass t to template struct ...
}
```

Add `use crate::i18n::T;` to each handler file.

## Task 1: Migrate `web/dashboard.rs`

Read the current file. The dashboard handler constructs:
- `system_warning`: French string about clock desync → use `t.get("dashboard-system-warning")` and format with the time
- `count_status` / `count_detail`: French status strings → use `count-not-started`, `count-in-progress`, `count-submitted`, `count-finalized`, `count-computed-closing`, `count-computed`, `count-balanced`, `count-unbalanced`, `count-final` keys
- `basis_breakdown`: Vec of `BasisSegment` with French labels → use fluent keys: `legal-basis-no-legal`, `legal-basis-police-custody`, etc. Store as `label_key` (the field was renamed in Phase 2+3)
- `alerts`: Vec of `AlertDisplay` with French labels → use `legal-alert-*` keys. Store as `label_key`

For parametric strings like count status, use `t.get_1()`:
```rust
// Before:
count_detail: Some(format!("Décompte calculé: {}", n))
// After:
count_detail: Some(t.get_1("count-computed-closing", "count", &n.to_string()))
```

Pass `t` as the last field of `DashboardTemplate`.

## Task 2: Migrate `templates/dashboard.html`

Read the current template. Replace all hardcoded French:
- Page title, section headers → `{{ t.get("dashboard-*") }}`
- Status labels → already resolved in handler (count_status, count_detail are Strings)
- Basis segment labels: `{{ t.get(&segment.label_key) }}` (note: ampersand for borrow)
- Alert labels: `{{ t.get(&alert.label_key) }}`
- Button text → `{{ t.get("dashboard-new-admission") }}`, `{{ t.get("dashboard-view-population") }}`, etc.

## Task 3: Migrate `web/population.rs`

Read the current file. The handler builds `DetaineeSummaryDisplay` items. Currently these have pre-resolved French strings for `sex`, `detention_basis`, and `flags`.

After migration:
- `sex` field: store fluent keys (`sex-male`, `sex-female`, `sex-other`)
- `detention_basis` field: store fluent keys (`legal-basis-*`)
- `flags` field: currently `Vec<String>` of French labels. Change to store fluent keys or pre-resolved via `t.get()`. Since flags are parametric (they have days/hours), it's simpler to resolve them in the handler using `t.get_1()` and keep flags as resolved strings.

For the flag resolution, use the pattern from `locales/en/legal.ftl`:
```rust
// Before (called filters::flag_label_fr):
Flag::NoLegalBasis { days_held } => format!("Sans base legale ({} j)", days_held),
// After:
Flag::NoLegalBasis { days_held } => t.get_1("legal-flag-no-legal-basis", "days", &days_held.to_string()),
```

Write a helper function (in the handler file or a shared util) that maps `Flag` → fluent key + args and calls `t.get_1()` / `t.get()`.

Similarly for `flag_severity_class` — this returns CSS classes not translations, so keep it as-is (it should have been moved to a util in Phase 2+3).

Pass `t` to both `PopulationPageTemplate` and `PopulationTableFragment`.

## Task 4: Migrate `templates/population/page.html` and `table_body.html`

Read both templates. Replace all French:
- Page title "Population" → `{{ t.get("population-title") }}`
- Subtitle → `{{ t.get("population-subtitle") }}`
- Search placeholder → `{{ t.get("population-search-placeholder") }}`
- Filter options (basis types) → `{{ t.get("legal-basis-police-custody") }}`, etc.
- Sort options → `{{ t.get("population-sort-*") }}`
- Table headers → `{{ t.get("population-col-*") }}`
- Pagination → `{{ t.get("population-previous") }}`, `{{ t.get("population-next") }}`, etc.
- Total count → `{{ t.get_1("population-total", "count", &total_matching.to_string()) }}`
- Page indicator → use `t.get_with()` or format in handler

## Task 5: Migrate `web/admission.rs`

Read the current file. The admission handler is mostly a form — the template does most of the display. The handler:
- Parses form data with French error messages → use `t.get("error-*")` keys
- Maps basis types (string values from form) → these stay as-is (form values are internal, not displayed)

For error messages in form validation:
```rust
// Before:
"Durée de peine invalide"
// After:
t.get("error-invalid-sentence")
```

The handler needs `t` to construct error messages. Pass `t` to `AdmissionPageTemplate` and `BasisFieldsFragment`.

## Task 6: Migrate `templates/admission/page.html` and `basis_fields.html`

Read both templates. Replace all French strings:
- `admission/page.html`: All form labels, section headers, button text → use `admission-*`, `form-*`, `sex-*` keys
- `admission/basis_fields.html`: Dynamic field labels for each basis type → use `basis-form-*` keys
- Dropdown options for basis types → use `legal-basis-*` keys
- Form buttons → `{{ t.get("admission-submit") }}`, `{{ t.get("form-cancel") }}`

## Task 7: Verify compilation

Run `cargo check` after all changes. Fix any issues.

## Date Formatting

Per spec section 8, all dates should use ISO format `%Y-%m-%d` instead of the old `%d/%m/%Y`. If you encounter `format_date()` calls or hardcoded `%d/%m/%Y` patterns, change them to `%Y-%m-%d`.
