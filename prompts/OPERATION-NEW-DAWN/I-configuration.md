# Prompt I — Module Stubs + Housing Single-Unit Mode + Dashboard Config (Issues #2, #10, #19)

**Priority:** Medium
**Phase:** 4 (parallel with H)
**Depends on:** Phases 2-3 complete. Read all `HANDOFF-*.md` files.

## Setup

Read all `HANDOFF-*.md` files in the project root before starting.

## Issue #2 — Module Dropdown References Unimplemented Modules

The "Module" filter dropdown in the audit trail lists modules that won't exist in the initial release (Medical, Disciplinary, Commissary, Visitors).

**Where:** `crates/openward-server/templates/audit/page.html` — lines 33-39, the module select dropdown.

**Fix:** Either:
- Hide unimplemented modules entirely from the dropdown
- Or show them as disabled with "(coming soon)" text
- Keep `Registry`, `Auth`, `Config`, `Analytics` if they're functional
- Check which modules actually have audit entries in the codebase

## Issue #10 — Housing System Needs Single-Unit Mode

Many facilities (Madagascar, Sahel, Philippines, etc.) don't have separate housing units. Need an admin option to use a single static housing unit or effectively disable housing complexity.

**Where:**
- `crates/openward-server/src/web/housing.rs` — housing management
- `crates/openward-server/templates/housing/page.html` — housing page
- `crates/openward-facility/src/lib.rs` — `ServerConfig` / `FacilityConfig`

**Fix:**
- Add a config option (e.g., `simplified_housing: bool` or `single_unit_mode: bool`) to `FacilityConfig` or `ServerConfig`
- When enabled: automatically create a single "General Population" unit, hide the housing management page from nav, auto-assign all new admissions to this unit
- When disabled: full housing management as-is
- This should be a config flag, not a runtime toggle (at least for now)
- Default should be `false` (full housing mode)

**Note:** Be careful modifying core config types. Check if `FacilityConfig` is in `openward-core` or `openward-facility`. Prefer adding the config in `openward-facility` if possible.

## Issue #19 — Admin-Configurable Dashboard

The dashboard should be configurable per facility. Current default of "pretrial population" may suit NGOs but isn't the most useful day-to-day metric for facility staff.

**Where:**
- `crates/openward-server/templates/dashboard.html` — dashboard template
- `crates/openward-server/src/web/dashboard.rs` — dashboard handler

**Fix — MVP approach:**
- Don't build a full drag-and-drop widget system
- Instead, add a "Dashboard Preferences" section to facility settings
- Allow choosing which sections are visible: population gauge, basis breakdown, critical alerts, today's count status, recent admissions/releases
- Store preferences in a config file or a new `facility_settings` table
- Default configuration should show everything

**Scope warning:** This is a medium-effort feature. If it's getting too large, implement just the foundation:
1. A `dashboard_widgets` config list in facility settings
2. The dashboard template conditionally renders sections based on this list
3. A simple settings form to toggle sections on/off

## Key Files

- `crates/openward-server/templates/audit/page.html` — module dropdown
- `crates/openward-server/src/web/housing.rs` — housing management
- `crates/openward-server/templates/housing/page.html` — housing page
- `crates/openward-facility/src/lib.rs` — config types
- `crates/openward-server/templates/dashboard.html` — dashboard
- `crates/openward-server/src/web/dashboard.rs` — dashboard handler
- `crates/openward-server/templates/base.html` — nav (for hiding housing link)

## Verification

1. `cargo build` must succeed
2. `cargo test` must pass
3. Audit module dropdown only shows implemented modules
4. With `single_unit_mode` config: housing page hidden, admissions auto-assign
5. Dashboard shows configurable sections

## Handoff

Write `HANDOFF-I.md` in the project root with:
- Which modules were hidden/kept in audit dropdown
- Housing single-unit mode implementation approach
- Dashboard configuration scope (what was implemented vs deferred)
- Any new config options added and their defaults
