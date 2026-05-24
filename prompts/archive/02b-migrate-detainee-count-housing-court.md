# Phase 4b: Migrate Detainee Detail, Daily Count, Housing, Court Calendar

## Prerequisites

Phase 2+3 (prompt 01) must be complete. All template structs in `templates.rs` already have `t: T`. `filters.rs` is deleted. `require_session()` returns `(OperatorId, String)`.

Read `specs/i18n.md` for the full string inventory and key names.

## General Pattern

Same as Phase 4a — every handler needs:
```rust
let (operator, lang) = require_session(&headers)?;
let t = T::new(&lang, state.legal_overrides.clone());
```
Add `use crate::i18n::T;` to each handler file.

## Task 1: Migrate `web/detainee.rs`

Read the current file. This is the largest handler file with 8 handlers.

**detail_page()**: Builds `DetaineeDisplay` with pre-resolved French strings:
- `sex`: from `sex_label_fr()` → store fluent key (`sex-male`, `sex-female`, `sex-other`) or resolve via `t.get()`
- `detention_basis_label`: from `basis_label_fr()` → store fluent key or resolve via `t.get("legal-basis-*")`
- `facility_status`: from `facility_status_fr()` → resolve via `t.get("legal-status-*")`
- `housing_unit`: just a name, no translation needed
- Warrant `order_type`: from `commitment_order_type_fr()` → resolve via `t.get("legal-order-*")`
- Court date `purpose`: from `court_purpose_fr()` → resolve via `t.get("legal-court-*")`
- Court date `outcome`: from `court_outcome_summary()` → resolve via `t.get("legal-outcome-*")`
- `FlagDisplay.label`: from `flag_label_fr()` → resolve via parametric `t.get_1("legal-flag-*", ...)`

Create helper functions that map each enum variant to its fluent key:
```rust
fn basis_key(label: &DetentionBasisLabel) -> &'static str {
    match label {
        DetentionBasisLabel::NoLegalBasis => "legal-basis-no-legal",
        DetentionBasisLabel::PoliceCustody => "legal-basis-police-custody",
        // ...
    }
}
```
Then call `t.get(basis_key(&label))`.

**basis_form()**: Needs `t` for the form fragment.
**update_basis()**: Has French error messages → use `t.get("error-*")`.
**housing_form()**: Builds `HousingUnitDisplay` — the `designated_sex` field uses `sex_label_fr()` → resolve via `t.get()`.
**update_housing()**: French error messages → `t.get("error-*")`.
**court_date_form()**: Needs `t` for the form fragment.
**schedule_court_date()**: French error messages → `t.get("error-*")`.
**do_release()**: French error messages → `t.get("error-*")`.

Pass `t` to all template structs.

## Task 2: Migrate detainee templates (5 files)

Read all templates in `templates/detainee/`:
- `page.html` — All section headers, labels, button text → use `detainee-*` keys
- `basis_form.html` — Form labels, dropdown options → use `basis-form-*` and `legal-basis-*` keys
- `housing_form.html` — Labels, dropdown → use `housing-form-*` keys
- `court_date_form.html` — Labels, dropdown options → use `court-form-*` and `legal-court-*` keys
- `release_form.html` — Labels, warnings, dropdown options → use `release-form-*` and `legal-release-*` keys

## Task 3: Migrate `web/daily_count.rs`

Read the current file. The daily count handler has validation error messages and template data.

- Error messages → `t.get("error-*")`
- Pass `t` to `DailyCountPageTemplate`

## Task 4: Migrate `templates/daily_count/page.html`

Read the template. Replace all French:
- Title, headers → `daily-count-*` keys
- Movement type labels → `daily-count-transfers-out`, `daily-count-to-court`, etc.
- Status labels → `daily-count-balanced`, `daily-count-unbalanced`
- Buttons → `daily-count-finalize`, `daily-count-open`, `daily-count-enter`
- Empty state message → `daily-count-not-opened`, `daily-count-no-units`

## Task 5: Migrate `web/housing.rs`

Read the current file. Housing handlers:
- Build `HousingUnitFullDisplay` with `unit_type`, `designated_sex`, `designated_age_group` as French strings
- After migration: resolve via `t.get("housing-type-*")`, `t.get("sex-*")`, `t.get("age-*")`
- Validation errors → `t.get("error-name-required")`, `t.get("error-capacity-min")`, `t.get("error-invalid-id")`

## Task 6: Migrate housing templates (3 files)

Read all templates in `templates/housing/`:
- `page.html` — Headers, buttons, summary labels → `housing-*` keys
- `create_form.html` — Form labels, dropdown options → `housing-*`, `housing-type-*`, `sex-*`, `age-*` keys
- `edit_form.html` — Same as create but with pre-filled values

## Task 7: Migrate `web/court_calendar.rs`

Read the current file. Calendar handlers:
- Build `CourtDateDisplay` with `purpose` and `outcome_summary` as French strings
- After: resolve `purpose` via `t.get("legal-court-*")`, `outcome_summary` via `t.get("legal-outcome-*")`
- Build `OutcomeFormDisplay` with `purpose` → resolve via `t.get()`
- Error messages → `t.get("error-*")`

## Task 8: Migrate court calendar templates (2 files)

Read all templates in `templates/court_calendar/`:
- `page.html` — Headers, table headers, buttons → `court-calendar-*` keys
- `outcome_form.html` — Form labels, dropdown options → `outcome-form-*` and `legal-outcome-*` keys

## Task 9: Verify compilation

Run `cargo check` after all changes. Fix any issues.

## Date Formatting

Per spec section 8, all dates should use ISO format `%Y-%m-%d` instead of the old `%d/%m/%Y`. Change all `format_date()` calls and hardcoded `%d/%m/%Y` to `%Y-%m-%d`. Datetime stamps use `%Y-%m-%d %H:%M`.

## Helper Functions Pattern

For mapping enums to fluent keys, create these helpers (in the handler file or a shared `web/i18n_keys.rs` util module):

```rust
fn basis_key(label: &DetentionBasisLabel) -> &'static str { ... }
fn status_key(status: &FacilityStatus) -> &'static str { ... }
fn court_purpose_key(purpose: &CourtPurpose) -> &'static str { ... }
fn court_outcome_key(outcome: &CourtOutcome) -> &'static str { ... }
fn order_type_key(ot: &CommitmentOrderType) -> &'static str { ... }
fn release_type_key(rt: &ReleaseType) -> &'static str { ... }
fn sex_key(sex: &Sex) -> &'static str { ... }
fn housing_type_key(ht: &HousingType) -> &'static str { ... }
fn age_group_key(ag: &AgeGroup) -> &'static str { ... }
fn flag_severity_class(flag: &Flag) -> &'static str { ... }  // CSS, not translation
```

These replace the `*_fr()` functions from the deleted `filters.rs`.

If you make a shared module, add `pub mod i18n_keys;` to `web/mod.rs` and use it from the handler files. Alternatively, just put these in each handler that needs them — the detainee handler needs most of them.
