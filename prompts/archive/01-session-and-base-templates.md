# Phase 2+3: Session/Auth Wiring + Base Templates

## Context

The i18n infrastructure is already in place:
- `crates/openward-server/src/i18n.rs` — `T` struct, `static_loader!`, `SUPPORTED_LANGUAGES`, `load_legal_overrides()`
- `crates/openward-server/locales/{en,fr,mg}/{ui,legal}.ftl` — all .ftl files
- `crates/openward-server/legal-presets/fr-MG.toml` — legal preset
- `crates/openward-facility/src/lib.rs` — `AppState` now has `legal_overrides: Option<Arc<HashMap<String, String>>>`
- `main.rs` already loads legal overrides and wires them into state

Read `specs/i18n.md` for full spec details. You are implementing sections 5, 7a (templates.rs changes), 7c (delete filters.rs), and the base.html/login.html/error.html migrations.

## Tasks

### 1. Modify `Operator` struct in `web/auth.rs`

Add a `language` field:

```rust
pub struct Operator {
    pub id: String,
    pub name: String,
    pub role: String,
    pub language: String,  // "en", "fr", "mg"
}
```

Update `known_operators()` to give each operator a default language (use `"fr"` for all three since they're Malagasy operators).

### 2. Add `openward_lang` cookie handling in `web/auth.rs`

The language is tracked via a separate `openward_lang` cookie. This persists across sessions.

Modify `extract_session()` to return `Option<(OperatorId, String)>` where the String is the language code. The language comes from:
1. `openward_lang` cookie (if present)
2. Operator's default language (from `known_operators()`)
3. `DEFAULT_LANGUAGE` ("en") as final fallback

Add a helper function `extract_lang(headers)` for pre-session language detection (login page):
1. Check `openward_lang` cookie
2. Check `Accept-Language` header (simple first-match against SUPPORTED_LANGUAGES)
3. Fall back to DEFAULT_LANGUAGE

### 3. Modify `require_session` in `web/mod.rs`

Change from:
```rust
pub fn require_session(headers: &HeaderMap) -> Result<OperatorId, Redirect>
```
To:
```rust
pub fn require_session(headers: &HeaderMap) -> Result<(OperatorId, String), Redirect>
```
Where the String is the language code. Update `nav_context` to stay the same (it doesn't need lang).

### 4. Modify login flow in `web/auth.rs`

**LoginForm** gains a `language` field:
```rust
pub struct LoginForm {
    pub operator_id: String,
    pub language: String,
}
```

**login_page()**: Construct a `T` from pre-session language detection. Pass it to LoginTemplate along with the languages list.

**login_submit()**: On success, set TWO cookies:
- `openward_session=<uuid>; Path=/; HttpOnly; SameSite=Strict; Max-Age=86400`
- `openward_lang=<lang>; Path=/; SameSite=Strict; Max-Age=31536000` (1 year, NOT HttpOnly so JS could read it if needed)

On error, re-render login with the `T` constructed from the form's language.

### 5. Add `t: T` to ALL template structs in `templates.rs`

Every template struct that derives `Template` needs a `t: T` field added. This includes:
- `LoginTemplate` — add `t: T` and `languages: &'static [(&'static str, &'static str)]`
- `ErrorTemplate` — add `t: T`
- `DashboardTemplate` — add `t: T`
- `PopulationPageTemplate` — add `t: T`
- `PopulationTableFragment` — add `t: T`
- `DetaineeDetailTemplate` — add `t: T`
- `BasisFormFragment` — add `t: T`
- `HousingFormFragment` — add `t: T`
- `CourtDateFormFragment` — add `t: T`
- `ReleaseFormFragment` — add `t: T`
- `AdmissionPageTemplate` — add `t: T`
- `BasisFieldsFragment` — add `t: T`
- `DailyCountPageTemplate` — add `t: T`
- `HousingPageTemplate` — add `t: T`
- `HousingCreateFormFragment` — needs to become a struct with `t: T` (currently a unit struct)
- `HousingEditFormFragment` — add `t: T`
- `CourtCalendarPageTemplate` — add `t: T`
- `OutcomeFormFragment` — add `t: T`

Add the import at the top of templates.rs:
```rust
use crate::i18n::T;
```

Also change `BasisSegment` and `AlertDisplay`: the `label` field should be renamed to `label_key` (String) — handlers will store fluent keys, templates will resolve via `t.get()`.

### 6. Delete `filters.rs`

Remove `pub mod filters;` from `web/mod.rs`. Delete `crates/openward-server/src/web/filters.rs`.

The `flag_severity_class()` function is still needed (it returns CSS classes, not translated strings). Move it to a small utility — either inline it in the handlers that use it, or keep a minimal `web/util.rs` with just that function.

### 7. Migrate `base.html`

Read the current `templates/base.html`. Replace all hardcoded French nav labels with `{{ t.get("key") }}` calls:
- "Tableau de bord" → `{{ t.get("nav-dashboard") }}`
- "Population" → `{{ t.get("nav-population") }}`
- "Admission" → `{{ t.get("nav-admission") }}`
- "Hébergement" → `{{ t.get("nav-housing") }}`
- "Comptage" → `{{ t.get("nav-count") }}`
- "Audiences" → `{{ t.get("nav-court-calendar") }}`
- "Déconnexion" → `{{ t.get("nav-logout") }}`

Add dynamic lang attribute: `<html lang="{{ t.lang_code() }}" ...>`

Since base.html is extended by child templates via Askama's `{% extends "base.html" %}`, and child templates have `t: T`, the `t` will be available in base.html through Askama's template inheritance.

### 8. Migrate `login.html`

Read the current template. Replace all French strings with `t.get()` calls using the `login-*` keys. Add the language selector dropdown per spec section 5d.

### 9. Migrate `error.html`

Read the current template. Replace French strings with `t.get()` calls using `error-*` keys.

### 10. Update `web/errors.rs`

Read the current file. Replace all hardcoded French error messages with calls to `t.get()`. The `HtmlError` → `ErrorTemplate` conversion needs access to a `T`. Either:
- Pass the language code through the error path, or
- Have the error handler accept a `T` parameter

The simplest approach: modify the error rendering to accept lang/overrides and construct a `T` internally.

### 11. Verify compilation

After all changes, run `cargo check` to make sure everything compiles. Fix any issues.

## Important Notes

- Import `T` as `crate::i18n::T`
- Import `SUPPORTED_LANGUAGES` as `crate::i18n::SUPPORTED_LANGUAGES`
- The `T` struct is NOT Clone — it must be constructed fresh for each request
- Every handler that creates a template now needs to construct `T::new(&lang, state.legal_overrides.clone())`
- For now, handlers that you DON'T migrate (dashboard, population, etc.) can construct `T` with default values and pass it — they'll be fully migrated in the next phase
- The `t: T` field should come LAST in template structs (convention)
