# DS-01B: Implement Search Statistics and Summary Flags

**Stream:** 1B — Complete the Registry (Fill Stubs)
**Priority:** Highest
**Dependencies:** None

## Context

Currently `search()` returns `default_statistics()` and empty `flags` on each `DetaineeSummary`. The population list is missing key data.

## Tasks

### 1. Summary Flags

After loading each detainee summary, compute flags. The `DetaineeSummary` already has a `flags: Vec<Flag>` field. The `compute_flags()` function needs a full `Detainee` struct.

**Approach (Option A — simple):** For each search result, load the full detainee and compute flags. Acceptable at Pi scale with PAGE_SIZE=25.

Option B (efficient, not recommended yet): Add a lightweight flag computation that works on summary data. More code, less I/O.

### 2. QueryStatistics

After running the search query, compute aggregate statistics over the result set. Some fields (`total`, `with_court_date`, `without_court_date`, etc.) can be computed during the main query. Others (flag_counts, time_held) require post-processing.

Since the result is paginated, statistics must be computed over the *entire matching set*, not just the current page. This means:
- A separate SQL aggregation query that applies the same WHERE clause but computes counts (recommended for simple counts)
- Rust-side aggregation for flags (since flag computation is Rust-only)

## Tests to Add

- `search_returns_flags` — seed a detainee with a known flag, search, verify flag appears
- `search_statistics_populated` — seed known data, verify statistics fields are non-default
- `search_statistics_reflect_full_result_set` — verify stats cover all matches, not just page

## Exit Criteria

Population list shows flags per detainee. Search returns populated statistics. All new tests pass. All existing tests still pass.
