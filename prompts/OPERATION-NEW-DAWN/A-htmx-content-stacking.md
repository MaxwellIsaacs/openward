# Prompt A — Fix HTMX Content Stacking (Issue #7)

**Priority:** Critical / Showstopper
**Phase:** 1 (must complete before all others)

## Problem

Performing an action on a detainee view (e.g. clicking "Release", "Update Basis", "Assign Housing") loads the resulting page *below* the existing content — two title bars, two full pages stacked vertically. The content should replace, not append. It goes away on second navigation, but the root cause needs fixing.

## Root Cause

The action buttons on the detainee page use `hx-target="#action-target"` to load forms into a `<div id="action-target">` container. The forms themselves (e.g. `release_form.html`) submit via `hx-post` targeting `#action-target`. But the Rust handlers (e.g. `do_release`, `update_basis`, `update_housing`) return `Redirect::to(...)` — a standard HTTP 302. When HTMX follows a 302 redirect, it loads the full page response (complete `<html><body>` from `base.html`) into the `#action-target` div, causing the stacking.

## Key Files

- `crates/openward-server/templates/detainee/page.html` — line 99: `<div id="action-target"></div>`, lines 101-126: action buttons with `hx-target="#action-target"`
- `crates/openward-server/templates/detainee/release_form.html` — line 3: `hx-post` targeting `#action-target`
- `crates/openward-server/templates/detainee/basis_form.html` — similar pattern
- `crates/openward-server/templates/detainee/housing_form.html` — similar pattern
- `crates/openward-server/templates/detainee/court_date_form.html` — similar pattern
- `crates/openward-server/templates/detainee/transfer_form.html` — similar pattern
- `crates/openward-server/templates/detainee/note_form.html` — similar pattern
- `crates/openward-server/templates/detainee/property_form.html` — similar pattern
- `crates/openward-server/src/web/detainee.rs` — all `do_*` handlers return `Redirect::to(...)`

## Fix

Use the `HX-Redirect` response header instead of HTTP 302 for all HTMX form submissions. When HTMX sees `HX-Redirect`, it does a full-page client-side redirect instead of swapping content into the target.

Two approaches (pick the cleaner one):

**Option 1 — HX-Redirect header:** In each handler that currently returns `Redirect`, return a response with `HX-Redirect` header:
```rust
use axum::http::header::HeaderValue;
// Instead of: Ok(Redirect::to(&url).into_response())
// Do:
let mut resp = axum::http::StatusCode::OK.into_response();
resp.headers_mut().insert("HX-Redirect", HeaderValue::from_str(&url).unwrap());
Ok(resp)
```

**Option 2 — Detect HTMX requests:** Check for `HX-Request` header and conditionally return `HX-Redirect` vs regular `Redirect`. This preserves non-HTMX form submission behavior.

Apply this fix to ALL handlers in `detainee.rs` that return redirects after form submissions:
- `update_basis` (line 265)
- `update_housing` (line 311)
- `schedule_court_date` (line 361)
- `do_release` (line 412)
- `add_note` (line 450)
- `delete_note` (line 460)
- `add_property` (line 498)
- `return_property` (line 508)
- `do_transfer` (line 557)
- `update_court_date` (line 608)

Also check ALL other web handlers that might have the same pattern — search for `Redirect::to` in the `web/` directory.

## Verification

1. `cargo build` must succeed
2. Run the server and navigate to a detainee
3. Click any action button (Release, Update Basis, etc.)
4. Submit the form — should cleanly navigate to the refreshed detainee page, NOT stack content

## Handoff

When done, write `HANDOFF-A.md` in the project root with:
- Exactly what you changed (files + approach)
- Whether you chose Option 1 or 2 and why
- Any other instances of this pattern you found and fixed outside `detainee.rs`
- Whether `cargo build` and `cargo test` pass
