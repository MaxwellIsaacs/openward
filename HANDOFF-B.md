# HANDOFF-B: Released Detainee UI (Issue #12)

## Root cause

Frontend/display only. The backend is correct: `SqliteRegistry::release()` in `crates/openward-registry/src/sqlite.rs` already flips `facility_status` to `Released` and deactivates active warrants. The detainee detail page, however, gave the release no visual prominence — `detention_basis_label` ("On Trial", etc.) stayed prominent under a "Legal Basis" header, `facility_status` ("Released") was rendered as a plain `<dd>`, and every action button (Edit Basis, Assign Housing, Schedule Court, Transfer, Release) remained active. The fix is purely in the Askama template, the `DetaineeDisplay` struct, the CSS, and the locale files.

## Files changed

- `crates/openward-server/src/templates.rs` — added `is_released: bool` field to `DetaineeDisplay` so the template can branch without string-comparing a localized label.
- `crates/openward-server/src/web/detainee.rs` — `detail_page()` populates `is_released` from `detainee.facility_status == FacilityStatus::Released` (uses the existing `use openward_core::*;` import).
- `crates/openward-server/templates/detainee/page.html` — adds a prominent RELEASED banner above the identity/detention grid; wraps "Legal Basis" value in `<s class="stale-basis">` with an "(at time of release)" suffix; renders the "Facility Status" value as `<strong class="status-released">`; wraps the `.actions-row` in `{% if !detainee.is_released %}` so all five action buttons disappear once released.
- `crates/openward-server/static/style.css` — adds `.released-banner` (green, heavy border, uppercase title) and `.status-released`/`.stale-basis` helpers. No new framework; matches the Pico-classless style of existing banners like `.flash-success`.
- `crates/openward-server/locales/{en,fr,mg}/ui.ftl` — added three new keys (see below). French is translated; Malagasy falls back to English copy per the i18n validation test's rules.

## i18n keys added

- `detainee-released-banner-title` — banner title (en/fr/mg).
- `detainee-released-banner-body` — banner body explaining actions are disabled.
- `detainee-basis-at-release` — suffix after the struck-through legal basis label.

## Build + test

- `cargo build`: clean, no warnings from the changed crates.
- `cargo test`: all tests pass (154 tests across the workspace, 0 failures, 0 ignored). The i18n validation test at `crates/openward-server/tests/i18n_validation.rs` still passes — each of the three new keys is present in `en/`, `fr/`, and `mg/` ui.ftl.
