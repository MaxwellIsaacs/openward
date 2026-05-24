# Prompt J — Mobile Nav Fix + Responsive Pass (Issues #14, #15)

**Priority:** High (nav) / Low (full pass)
**Phase:** 5 (sequential — after all prior phases)
**Depends on:** All prior phases. Read ALL `HANDOFF-*.md` files.

## Setup

Read ALL `HANDOFF-*.md` files in the project root. This prompt runs last (before deferred features) because earlier phases may have changed templates and styles.

## Issue #14 — Nav Breaks on Mobile

The navigation is literally broken on mobile devices. Critical given the target hardware (~$80 phones with small screens).

**Where:** `crates/openward-server/templates/base.html` — nav structure, `static/style.css`

**Fix:**
- The nav has three `<ul>` blocks: logo, navigation links, and user info
- On mobile, these likely overflow or stack badly
- Implement a hamburger menu or collapsible nav for mobile:
  - Show logo + hamburger icon on small screens
  - Tap hamburger to expand full nav vertically
  - Minimal JS is acceptable for toggle (a few lines, no framework)
  - Or use a CSS-only approach with a hidden checkbox + label
- The user menu (profile + logout) should be accessible from the mobile nav
- Active page indicator (from Prompt E) must work in mobile view

## Issue #15 — Full Mobile Responsiveness Pass

Every page needs to work on mobile. This is non-negotiable given the target users and devices.

**Key pages to check and fix:**

1. **Dashboard** (`/`) — cards/grid should stack vertically on mobile
2. **Population list** (`/detainees`) — table should be scrollable horizontally or reformatted as cards on mobile. Filters should collapse.
3. **Detainee detail** (`/detainees/{id}/view`) — two-column grid should stack. Action buttons should wrap.
4. **Admission form** (`/admit`) — form fields should be full-width on mobile
5. **Daily count** (`/daily-count`) — count inputs should be large and easy to tap
6. **Audit trail** (`/audit`) — table should scroll or reformat
7. **Housing** (`/housing`) — cards should stack
8. **Court calendar** (`/court-calendar`) — table should scroll

**General mobile CSS principles:**
- Breakpoint: `max-width: 768px` (covers most target phones)
- Tables: wrap in `<div style="overflow-x: auto">` for horizontal scroll on mobile
- Grids: stack to single column
- Buttons: minimum 44px touch targets
- Font sizes: minimum 16px for inputs (prevents iOS zoom)
- Forms: full-width fields
- Don't hide information — reflow, don't remove

**CSS approach:**
- Add responsive styles to `static/style.css`
- Use `@media (max-width: 768px)` queries
- Pico CSS provides some responsiveness already — build on it, don't fight it

## Key Files

- `crates/openward-server/templates/base.html` — nav structure
- `crates/openward-server/static/style.css` — all responsive styles go here
- Every template in `templates/` — check each for mobile issues
- `crates/openward-server/static/` — may need a tiny JS file for hamburger menu

## Verification

1. `cargo build` must succeed
2. Open the app in a browser and resize to 375px width (iPhone SE size)
3. Nav should be usable — hamburger menu works, all links accessible
4. Every page should be readable and functional at mobile width
5. No horizontal overflow (except intentional table scroll)
6. Buttons and inputs should be easily tappable
7. Test at 768px width (tablet) as a middle ground

## Handoff

Write `HANDOFF-J.md` in the project root with:
- Mobile nav approach (hamburger vs accordion vs other)
- Pages that needed the most work
- Any pages that still need attention
- Whether you added any JS and how much
