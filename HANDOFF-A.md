# HANDOFF-A: Fix HTMX Content Stacking (Issue #7)

## Approach: Option 2 — Detect HTMX requests

Added a `hx_redirect` helper that checks for the `HX-Request` header. HTMX requests get an `HX-Redirect` response header (200 OK + header), while regular requests get a standard HTTP 302. This preserves correct behavior for both HTMX and non-HTMX form submissions.

## Files Changed

### `crates/openward-server/src/web/util.rs`
- Added `hx_redirect(headers, url)` helper function

### `crates/openward-server/src/web/detainee.rs`
Fixed 6 HTMX-submitted handlers:
- `update_basis` (line 265)
- `update_housing` (line 311)
- `schedule_court_date` (line 361)
- `do_release` (line 412)
- `do_transfer` (line 557)
- `update_court_date` (line 608)

Left 4 handlers unchanged (they use standard `<form method="post">`, not HTMX):
- `add_note`, `delete_note`, `add_property`, `return_property`

### `crates/openward-server/src/web/court_calendar.rs`
- Fixed `record_outcome` handler (line 136) — form uses `hx-post` with `hx-target`
- Removed unused `Redirect` import

### `crates/openward-server/src/web/housing.rs`
- Fixed `create_unit` (line 126), `update_unit` (line 182), `delete_unit` (line 195) — all use `hx-post`/`hx-delete` with `hx-target`
- Removed unused `Redirect` import

### `crates/openward-server/src/web/daily_count.rs`
- Fixed `open_count` (line 86), `submit_headcount` (line 116), `finalize` (line 129) — all use `hx-post`
- Added `use crate::web::util` import
- Left the GET redirect in `daily_count_today` unchanged (not a form submission)

## Build & Test

- `cargo build`: pass
- `cargo test`: 155 tests pass, 0 failures
