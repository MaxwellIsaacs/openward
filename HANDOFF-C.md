# HANDOFF-C: Fix Population Filters (Issue #20)

## Root cause — NOT what the brief guessed

The route exists, the handler is wired, HTMX targets and fragments are all
correct.  The real bug was **serde rejecting empty strings for `Option<u32>`**.

`SearchFormParams` in `crates/openward-server/src/web/population.rs` had three
optional numeric fields — `held_min`, `held_max`, `release_within` — plus
`page`, each typed `Option<u32>`.  The population page's HTML
`<input type="number">` controls (lines 100, 109, 154 of `population/page.html`)
always submit their `name` attribute in the query string, and when empty the
browser sends `held_min=` (literal empty value).  `serde_urlencoded` (used by
Axum's `Query<T>` extractor) then fails the whole deserialization — it cannot
parse `""` as `Option<u32>`.  The extractor returns a `400 Bad Request`, which
HTMX silently drops (no swap, no error banner), and from the user's
perspective "no filter does anything".

As soon as *any* other filter triggered a request (name typed, dropdown
changed, etc.) it carried the untouched empty numeric fields and died at
deserialization.  That is why every filter on the page appeared broken even
though only the numeric fields were empty.

## Fix

Added a small `deserialize_optional_u32` helper that treats `None`, `""`, and
whitespace as `None`, and parses other strings as `u32`.  Applied it via
`#[serde(default, deserialize_with = "...")]` to `held_min`, `held_max`,
`release_within`, and `page`.

Also added three unit tests in the same module covering:

1. All numeric fields submitted empty → parse as `None`.
2. Populated numeric fields → parse correctly.
3. Full round-trip of `into_query()` wiring every filter to the right
   `PopulationQuery` field (guards against future mapping regressions).

## Files changed

- `crates/openward-server/src/web/population.rs` — one helper fn, four
  `#[serde(default, deserialize_with = ...)]` annotations, and a `#[cfg(test)]`
  module.  No other file touched.

## Filters verified (via code review + `into_query` unit test)

Every filter exposed in `templates/population/page.html` maps to a field in
`PopulationQuery` that `QueryBuilder::from_query` in
`crates/openward-registry/src/queries.rs` translates into a SQL predicate:

| UI control         | `SearchFormParams` | `PopulationQuery`               | SQL (queries.rs) |
|--------------------|--------------------|---------------------------------|------------------|
| text "name"        | `name`             | `name_search`                   | line 28          |
| basis dropdown     | `basis`            | `detention_basis`               | line 37          |
| bail dropdown      | `bail`             | `bail_status`                   | line 122         |
| severity dropdown  | `severity`         | `charge_severity`               | line 134         |
| sex dropdown       | `sex`              | `sex`                           | line 86          |
| has court date     | `has_court_date`   | `has_court_date`                | line 144         |
| has legal rep      | `has_legal_rep`    | `has_legal_representation`      | line 99          |
| held_min (number)  | `held_min`         | `held_longer_than_days`         | line 108         |
| held_max (number)  | `held_max`         | `held_shorter_than_days`        | line 114         |
| court overdue      | `court_overdue`    | `court_date_overdue`            | line 153         |
| release overdue    | `release_overdue`  | `release_overdue`               | line 158         |
| release within     | `release_within`   | `release_within_days`           | line 165         |
| sort dropdown      | `sort`             | `sort_by` + `sort_order`        | line 188         |
| pagination link    | `page`             | `offset` (via `PAGE_SIZE`)      | line 210         |

The `flag` form field in `SearchFormParams` is accepted and preserved in
`FilterState` but has no UI control that emits it and no mapping into
`PopulationQuery.has_flags`.  I did NOT wire flag filtering — the template
doesn't ask for it and this isn't on the task.

## Not verified end-to-end

I could not run the server or `cargo test` from this sandbox — the sandbox
blocks `cargo test` and network-y commands.  Verification relies on:

- `cargo build` clean before adding tests (confirmed: "Finished `dev` profile"
  output after the serde fix, prior to the concurrent `templates.rs` edit).
- The three new unit tests in `population::tests` exercise the serde path
  using axum's own `Query::try_from_uri`, so they will be picked up by
  `cargo test -p openward-server --lib` once the build is green.

## Build status / known blocker NOT caused by this change

After I finished the fix, `cargo build --tests` started failing with:

```
error: template "audit/detail.html" not found in directories [...]
   --> crates/openward-server/src/templates.rs:556:19
```

`crates/openward-server/src/templates.rs` grew from 606 to 640 lines during my
session — a parallel agent (presumably D) appended an `AuditDetailTemplate`
struct pointing to a template file that does not yet exist in
`crates/openward-server/templates/audit/`.  This is outside my scope (and I
am instructed not to touch files the other agents are editing).  D will need
to add `audit/detail.html` for the crate to build.  My earlier `cargo build`
(immediately after the serde fix, before D's append) compiled cleanly.

## Constraints honoured

- Did not touch `templates.rs` (B's file), `web/detainee.rs`,
  `templates/detainee/page.html`, `static/style.css`, or `locales/*/ui.ftl`.
- Did not touch `openward-core`, the `Registry` trait, or the schema.
- No commits made.
