# DS-01A: Implement `overview()` Fully

**Stream:** 1A — Complete the Registry (Fill Stubs)
**Priority:** Highest
**Dependencies:** None

## Context

The `overview()` function currently returns default values for `TimeDistribution`, `FlagCounts`, `SexBreakdown`, `CriticalNumbers`, and `DailyCountStatus`. The dashboard is half-populated as a result.

## Tasks

### 1. SexBreakdown

Query active detainees grouped by sex:

```sql
SELECT sex, COUNT(*) FROM detainees
WHERE facility_status IN ('Present','InCourt','InHospital')
GROUP BY sex
```

Map results to the `SexBreakdown` struct.

### 2. TimeDistribution

For each active detainee, compute `(today - intake_date).num_days()` and bucket into the 8 ranges. Calculate median and mean.

SQL approach:
```sql
SELECT
  SUM(CASE WHEN julianday('now') - julianday(intake_date) < 2 THEN 1 ELSE 0 END) as under_48h,
  SUM(CASE WHEN julianday('now') - julianday(intake_date) < 7 THEN 1 ELSE 0 END) as under_1w,
  -- ... etc
FROM detainees WHERE facility_status IN ('Present','InCourt','InHospital')
```

**Important:** The buckets are cumulative in the struct names but should be exclusive (under_48h means 0-2 days, under_1_week means 2-7 days, etc.). Check the struct definition. Median requires loading all `days_held` values and sorting — acceptable for Pi-scale populations.

### 3. FlagCounts + CriticalNumbers

Load all active detainees, run `compute_flags()` on each, aggregate. `compute_flags()` already exists in `flags.rs`. The `FlagCounts` and `CriticalNumbers` structs overlap significantly — CriticalNumbers is a subset. Both need the same flag pass. Do this in one loop.

**Housing units:** The flag computation needs housing units. Load them once and pass to each `compute_flags()` call.

### 4. DailyCountStatus

Query `daily_counts` for today's date:
- No row → `NotStarted`
- Row with `is_finalized = 0` and no `actual_closing` → `Open { computed_closing }`
- Row with `actual_closing` but not finalized → `Submitted { computed, actual, balanced }`
- Finalized → `Finalized { closing, balanced }`

## Tests to Add

- `overview_time_distribution` — seed detainees with known intake dates, verify buckets
- `overview_sex_breakdown` — seed with mixed sexes, verify counts
- `overview_flag_counts` — seed detainees triggering known flags, verify aggregation
- `overview_critical_numbers` — same
- `overview_daily_count_status_*` — test each status variant

## Exit Criteria

Dashboard fully populated with real data. All new tests pass. All existing tests still pass.
