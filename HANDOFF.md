# OpenWard HTMX Frontend — Handoff Document

## Your Mission

Build an HTMX-powered server-rendered frontend for OpenWard, a facility management system for correctional institutions in low-resource environments (sub-Saharan Africa, specifically Madagascar). The JSON API layer is complete — you're adding HTML views on top.

**Design constraints:**
- Raspberry Pi 4 hosting, low-bandwidth networks, possibly intermittent connectivity
- Server-rendered HTML with HTMX for interactivity (no JS framework, no SPA)
- Must feel fast — every page load is a full render, HTMX handles partial updates
- Operators may have limited technical literacy — UI must be clear and obvious

## What Exists

### Architecture

```
Browser (HTMX)
    ↕  HTTP (HTML fragments + JSON API)
openward-server        (Axum routes — JSON API done, HTML views needed)
    ↕  Arc<AppState>
openward-facility      (ServerConfig, AppState, init)
    ↕
openward-registry      (SqliteRegistry — implements Registry trait)
    ↕
openward-db            (SQLite pool, schema, WAL mode)
```

### Workspace Layout

```
crates/
  openward-core/         Domain types, business logic, traits (no async)
  openward-db/           SQLite pool + schema (14 tables)
  openward-registry/     Registry trait impl, flag computation, audit trail
  openward-facility/     ServerConfig, AppState, init()
  openward-server/       Axum HTTP server (JSON API — 15 endpoints)
  openward-medical/      Stub
  openward-disciplinary/ Stub
  openward-commissary/   Stub
  openward-visitors/     Stub
```

### JSON API (Complete — 15 endpoints)

All routes are in `crates/openward-server/src/main.rs` (~365 lines):

| Method | Path | Returns |
|--------|------|---------|
| GET | `/api/overview` | `FacilityOverview` |
| GET | `/api/detainees` | `PopulationQueryResult` (search/filter) |
| POST | `/api/detainees` | `Detainee` (admit) |
| POST | `/api/detainees/batch` | `Vec<Detainee>` (batch admit) |
| GET | `/api/detainees/{id}` | `Detainee` |
| PUT | `/api/detainees/{id}/basis` | `Detainee` (update detention basis) |
| PUT | `/api/detainees/{id}/status` | `Detainee` (update facility status) |
| PUT | `/api/detainees/{id}/housing` | `Detainee` (assign housing) |
| POST | `/api/detainees/{id}/warrants` | `Detainee` (register warrant) |
| POST | `/api/detainees/{id}/court-dates` | `CourtDate` (schedule) |
| PUT | `/api/court-dates/{id}/outcome` | `CourtDate` (record outcome) |
| POST | `/api/detainees/{id}/release` | `Detainee` (release) |
| POST | `/api/daily-counts/{date}` | `DailyCount` (open) |
| POST | `/api/daily-counts/{date}/headcounts` | `DailyCount` (submit unit count) |
| POST | `/api/daily-counts/{date}/finalize` | `DailyCount` (finalize) |

**Auth:** `X-Operator-Id` header (UUID) — placeholder for real auth. Extracted in `extract_operator()`.

**Error handling:** `ApiError` wraps `RegistryError` → maps domain errors to HTTP status codes (404, 409, 422, 400, 500).

### Key Domain Types

**Detainee** — the core record:
- `id: DetaineeId`, `identity: Identity`, `detention_basis: DetentionBasis`
- `facility_status: FacilityStatus`, `intake_date: PastDate`
- `housing_unit: Option<HousingUnitId>`, `warrants: Vec<CommitmentOrder>`
- `legal_reference`, `emergency_contacts`, `property`, `legal_representation`, `notes`

**Identity:**
- `surname`, `given_names`, `preferred_name`, `aliases: Vec<String>`
- `date_of_birth: Option<NaiveDate>`, `estimated_age_at_intake: Option<u32>`
- `sex: Sex` (Male/Female/Other), `nationality`, `national_id`, `languages`, `photo_hash`

**DetentionBasis** — enum with 7 variants (state machine):
1. `NoLegalBasis` — no legal authority (critical flag)
2. `PoliceCustody` — arrest_date, must_appear_by, arresting_authority
3. `RemandAwaitingTrial` — charges, bail_status, next_court_date
4. `OnTrial` — trial_start_date, charges
5. `ConvictedUnsentenced` — conviction_date, sentencing_date
6. `Sentenced` — sentence, release_date, credits, adjustments
7. `SentencedOnAppeal` — appeal details

**FacilityStatus** — enum: Present, InCourt, InHospital, Transferred, Released, Escaped, Deceased

**Flag** — 13 types of automated alerts (computed, not stored):
- Critical: NoLegalBasis, CustodyLimitExceeded, ReleaseDatePassed, WarrantExpired, NoActiveWarrant
- Warning: NoCourtDate, RemandReviewOverdue, ProlongedPreTrial, BailGrantedStillHeld, NoLegalRepresentation, HousingViolation
- Info: CourtDateImminent, ReleaseImminent

**FacilityOverview** — dashboard snapshot:
- total_population, facility_capacity, occupancy_percent
- basis_breakdown, time_distribution, flag_counts, sex_breakdown
- critical numbers (urgent alerts), today_count_status

**PopulationQuery** — 20+ optional filter fields:
- detention_basis, bail_status, charge_severity
- held_longer_than_days, has_court_date, court_date_overdue
- facility_status, housing_unit, sex, age_range, has_flags
- sort_by, sort_order, limit, offset

**PopulationQueryResult:**
- `detainees: Vec<DetaineeSummary>` (lightweight list items)
- `total_matching: u32`
- `statistics: QueryStatistics`

**DetaineeSummary** — for list views:
- id, name, sex, age, detention_basis (label), intake_date, days_held
- bail_status, next_court_date, release_date, housing_unit
- has_legal_representation, flags (computed)

**DailyCount** — population reconciliation:
- opening_count, admissions, releases, transfers, escapes, deaths
- unit_counts (per housing unit), computed_closing, is_balanced

### Database Schema

14 tables in `crates/openward-db/src/schema.sql` (242 lines). Key tables:
- `detainees` — main record, JSON columns for nested types
- `commitment_orders` — warrants/legal authority
- `court_dates` — scheduled appearances with outcomes
- `housing_units` — reference data
- `daily_counts` + `unit_headcounts` — population reconciliation
- `audit_entries` — append-only, SHA-256 chain-hashed
- `notes`, `property_items` — per-detainee

### Registry Trait (16 methods)

Defined in `crates/openward-core/src/traits.rs`. All async, all return `Result<T, RegistryError>`.

The trait is NOT object-safe (uses `impl Future`), so `AppState` holds concrete `SqliteRegistry`.

### Flag Computation

In `crates/openward-registry/src/flags.rs`. Function: `compute_flags(detainee, config, housing_units) -> Vec<Flag>`. Flags are computed on every request, never stored. Thresholds come from `FacilityConfig` (configurable per facility).

### Tests

`crates/openward-registry/tests/integration.rs` — 4,107 lines, 20 test functions covering the full lifecycle.

## Recommended Approach for the HTMX Layer

### Template Engine

Use **Askama** (compile-time Jinja2-like templates for Rust). Add to workspace:
```toml
askama = { version = "0.12", features = ["with-axum"] }
askama_axum = "0.4"
```

### Suggested Page Structure

1. **Dashboard** (`GET /`) — FacilityOverview rendered as HTML
   - Population gauge, basis breakdown chart (CSS-only or simple bars)
   - Critical alerts prominently displayed
   - Quick links to search, admit, daily count

2. **Population List** (`GET /detainees`) — search/filter/sort
   - Filter sidebar (detention basis, flags, bail status, etc.)
   - Table of DetaineeSummary rows with flag badges
   - HTMX: filter changes swap the table body via `hx-get`
   - Pagination with `hx-swap="innerHTML"` on table container

3. **Detainee Detail** (`GET /detainees/{id}`) — full record view
   - Identity panel, detention basis panel, warrants, court dates, notes
   - Action buttons: update basis, assign housing, schedule court date, release
   - Each action opens a form (could be inline with `hx-swap`)

4. **Admission Form** (`GET /admit`) — multi-step or single form
   - Identity fields, detention basis selection (changes form dynamically)
   - Warrant details, property logging
   - HTMX: basis type dropdown swaps in the right sub-form

5. **Daily Count** (`GET /daily-count`) — headcount reconciliation
   - Per-unit count entry
   - Running balance calculation
   - Finalize button

6. **Court Calendar** — list of upcoming court dates with outcomes

### HTMX Patterns

- **Search with filters:** `hx-get="/detainees?..."` on filter changes, target the results table
- **Inline editing:** `hx-put="/detainees/{id}/basis"` with `hx-swap="outerHTML"` to replace the panel
- **Form submission:** `hx-post` with `hx-target` for success/error feedback
- **Polling:** `hx-trigger="every 60s"` on the dashboard for live updates
- **Active search:** `hx-trigger="keyup changed delay:300ms"` on name search

### Route Organization

Keep JSON API routes under `/api/` and add HTML routes alongside:
```
GET  /                          → dashboard (HTML)
GET  /detainees                 → population list (HTML, full page)
GET  /detainees/search          → search results fragment (HTMX partial)
GET  /detainees/{id}            → detail page (HTML)
GET  /admit                     → admission form (HTML)
POST /admit                     → process admission, redirect
GET  /daily-count               → daily count page (HTML)
GET  /daily-count/{date}        → specific date count (HTML)
```

### Static Assets

For CSS, consider:
- **Pico CSS** or **Simple.css** — classless CSS for semantic HTML
- Or a minimal custom stylesheet
- No build step, just serve static files via `tower-http::services::ServeDir`

### Operator Session

Currently just a UUID header. For the HTMX frontend, you'll need:
- Login page (even if just "select your name from a list")
- Session cookie or similar to carry operator ID
- Every HTMX request automatically includes the operator

## How to Run

```bash
cd /home/max/Dropbox/dev/prof/openward

# Build
cargo build

# Run (in-memory DB for dev)
OPENWARD_DB=":memory:" cargo run --bin openward-server

# Run with persistent DB
cargo run --bin openward-server

# Run tests
cargo test
```

Environment variables: `OPENWARD_DB`, `OPENWARD_BIND`, `OPENWARD_CAPACITY`, `OPENWARD_JUVENILE_AGE`

## Files You'll Modify

1. **`Cargo.toml` (workspace root)** — add askama, askama_axum deps
2. **`crates/openward-server/Cargo.toml`** — add askama, askama_axum, tower-http (with serve-dir)
3. **`crates/openward-server/src/main.rs`** — add HTML route handlers, template rendering
4. **New: `crates/openward-server/templates/`** — Askama templates
5. **New: `crates/openward-server/static/`** — CSS, maybe a small JS file for HTMX

## Important Constraints

- **No JavaScript frameworks.** HTMX + server-rendered HTML only.
- **Minimal CSS.** Classless or utility-light. No Tailwind, no build steps.
- **Works on 1024x768.** Many facilities have old monitors.
- **Print-friendly.** Court documents, transfer papers need to print cleanly.
- **Low bandwidth.** Every byte matters. No unnecessary assets.
- **Raspberry Pi 4.** ARM64, 4GB RAM. SQLite is intentional — no Postgres.
- **French may be needed.** Madagascar uses French and Malagasy. Internationalization hooks are nice but not required yet.
- **All domain types already have Serialize/Deserialize.** You can use them directly in templates.

## What NOT to Change

- Don't modify `openward-core` types unless absolutely necessary
- Don't change the Registry trait or SqliteRegistry
- Don't change the database schema
- The JSON API endpoints should remain functional alongside HTML routes
- Don't add authentication/authorization complexity — that's a separate future task
