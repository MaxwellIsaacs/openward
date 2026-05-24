# Prompt C — Fix Population Filters (Issue #20)

**Priority:** Critical / Showstopper
**Phase:** 2 (parallel with B, D)
**Depends on:** A (read `HANDOFF-A.md` first for context on recent HTMX fixes)

## Setup

Read `HANDOFF-A.md` in the project root before starting.

## Problem

Neither the standard filters nor the advanced filters on the Population page (`/detainees`) are functional. Changing a filter dropdown or typing in the search box does nothing.

## Investigation Path

1. **Check routing:** `crates/openward-server/src/web/mod.rs` or `main.rs` — is there a route registered for `GET /detainees/search`? The template uses `hx-get="/detainees/search"` (see `population/page.html` lines 18-21).

2. **Check the search handler:** `crates/openward-server/src/web/population.rs` — `search_fragment()` exists but is it wired up in the router?

3. **Check the template fragment:** `crates/openward-server/templates/population/table_body.html` — this is the HTMX fragment that should be returned by the search endpoint. Verify it matches what `PopulationTableFragment` renders.

4. **Check HTMX attributes:** The filter elements in `population/page.html` use `hx-get="/detainees/search"`, `hx-target="#results-container"`, `hx-trigger="change"`, and `hx-include="#filter-form"`. Verify:
   - The `id="results-container"` div exists (line 181)
   - The `id="filter-form"` form exists (line 12)
   - The HTMX attributes are correct

5. **Check query parameter mapping:** `SearchFormParams` in `population.rs` — do the form field `name` attributes match the struct field names? E.g., `name="basis"` in the template maps to `pub basis: Option<String>` in the struct.

6. **Check the core query:** `PopulationQuery` — does `search()` in the registry actually filter by the provided fields? Some filters might be silently ignored.

## Key Files

- `crates/openward-server/src/web/mod.rs` — route registration
- `crates/openward-server/src/main.rs` — router setup
- `crates/openward-server/src/web/population.rs` — handlers
- `crates/openward-server/templates/population/page.html` — full page template
- `crates/openward-server/templates/population/table_body.html` — HTMX fragment
- `crates/openward-registry/src/queries.rs` — actual query implementation
- `crates/openward-core/src/traits.rs` — `search()` method signature

## Fix

Most likely the `/detainees/search` route is either missing from the router or returning the wrong content type. Fix the routing and ensure:
1. `GET /detainees/search` returns an HTML fragment (just the table + pagination, no `<html>` wrapper)
2. All query parameters from the form are correctly parsed
3. The fragment matches what `#results-container` expects

## Verification

1. `cargo build` must succeed
2. Run server, go to `/detainees`
3. Type in the search box — table should filter by name
4. Change detention basis dropdown — table should filter
5. Open advanced filters, change bail status — table should filter
6. Clear filters button should reset everything
7. Pagination should work within filtered results

## Handoff

Write `HANDOFF-C.md` in the project root with:
- Root cause (missing route? wrong response? parameter mismatch?)
- What you changed
- Which filters now work and any that still don't (with explanation)
