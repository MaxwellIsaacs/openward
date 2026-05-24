# DS-04B: Advanced Search and Quick Filters

**Stream:** 7D — Operational Completeness
**Priority:** Medium
**Dependencies:** Phase 2 complete (populated search statistics, auth)

## Context

The `PopulationQuery` has 20+ filter fields but the HTML search bar only exposes a few.

## Tasks

### 1. Advanced Search Panel

- Advanced search toggle (expands full filter panel)
- Filter by: flag type, bail status, charge severity, days held range, sex, housing unit
- HTMX-powered filter updates

### 2. Quick Filters from Dashboard

Clicking a critical number on the dashboard (e.g., "5 with no legal basis") pre-fills the search with the corresponding filter. These are URL links with query parameters that map to `PopulationQuery` fields.

## Exit Criteria

All `PopulationQuery` filter fields exposed in the UI. Quick filter links from dashboard work. HTMX interactions smooth. All tests pass.
