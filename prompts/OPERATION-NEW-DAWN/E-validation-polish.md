# Prompt E — Phone Validation + Tab Indicator + Nav Polish (Issues #4, #6, #13)

**Priority:** Medium
**Phase:** 3 (parallel with F, G)
**Depends on:** Phase 2 complete. Read `HANDOFF-A.md`, `HANDOFF-B.md`, `HANDOFF-C.md`, `HANDOFF-D.md` for context.

## Setup

Read all `HANDOFF-*.md` files in the project root before starting.

## Issue #4 — Phone Number Validation

Emergency contact phone numbers accept obviously invalid input (e.g., "777"). Phone numbers are a special case — this may be the *only* time this data is collected.

**Where:** The admission form (`templates/admission/page.html`) has emergency contact fields. The admission handler processes them in `web/admission.rs`.

**Fix:**
- Validation should be lenient (accept various international formats, with/without country codes, spaces, dashes, dots)
- But reject clearly invalid entries: fewer than 6 digits, clearly not a phone number
- Add validation in the Rust handler (server-side), not just HTML `pattern`
- A simple regex like `\d{6,}` after stripping non-digits is sufficient
- Show a clear error message if validation fails
- Don't use a phone number parsing library — keep it simple

## Issue #6 — Active Tab Indicator Nearly Invisible

When viewing a detainee, there's a slight visual cue for the active tab (nav item), but it's extremely hard to see.

**Where:** `templates/base.html` — nav items use `aria-current="page"` for the active state. Pico CSS styles this, but the visual difference is minimal.

**Fix:** Add CSS in `static/style.css` to make `[aria-current="page"]` much more obvious:
- Stronger bottom border or underline
- Different text color or weight
- Maybe a subtle background color
- Must still work on dark backgrounds and be accessible

## Issue #13 — Top Nav Needs Minimal Styling

The top nav needs basic readability improvements — separators between items, slightly more readable text. Target devices are ~$80 phones, so nothing fancy.

**Where:** Same nav in `templates/base.html`, styled via `static/style.css`.

**Fix:**
- Add subtle separators (border-right or `|` character) between nav items
- Slightly increase font size or weight for readability
- Ensure sufficient contrast
- Keep it minimal — this is low priority polish

## Key Files

- `crates/openward-server/src/web/admission.rs` — admission handler with phone field processing
- `crates/openward-server/templates/admission/page.html` — admission form template
- `crates/openward-server/templates/base.html` — navigation template
- `crates/openward-server/static/style.css` — custom styles

## Verification

1. `cargo build` must succeed
2. Try admitting with phone "777" — should get validation error
3. Try admitting with phone "+261 34 12 345 67" — should succeed
4. Active nav tab should be clearly visible on all pages
5. Nav items should have visible separators

## Handoff

Write `HANDOFF-E.md` in the project root with:
- Phone validation regex/logic used
- CSS changes made for nav
