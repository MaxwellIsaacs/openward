# OpenWard Mobile-First Redesign

## Context

OpenWard's north-star user is a corrections officer working on a **sub-$100 Android phone** in a Caribbean facility — entry-level Snapdragon 4xx / Helio A-series, 2–3 GB RAM, 5–6" screen, intermittent 3G/4G, often in direct sunlight.

The current UI was built classless-Pico-first, desktop-implicit. A mobile-rework pass 1 (commit `bfc4bcc..63e9c5f` on `mobile-rework`) added a hamburger nav, a stacked-card layout for the population table, vertical filter bars, and tap-target sizing. This spec covers everything still needed to make every screen feel native-on-phone, not desktop-shrunk.

## Decisions (settled)

| Decision | Choice |
|----------|--------|
| Approach | Mobile-first CSS, desktop as enhancement above 768px |
| JS budget | Server-rendered HTML + HTMX. No SPA frameworks. No client-side rendering. |
| CSS budget | Pico (≈10 KB gzipped) + custom (target ≤8 KB gzipped, currently ≈6 KB) |
| Nav pattern | Hamburger drawer on mobile, horizontal nav ≥ 768px — pass 1 already shipped |
| Table pattern | Card-per-row under 768px via `.responsive-table` + `data-label` TDs |
| Form pattern | Labels stacked above inputs, inputs full-width, native pickers (`type=date` etc.) |
| Touch target floor | 48 px (Material) for primary actions, 44 px (HIG) for nav and incidental |
| Contrast floor | WCAG AA across the board; AAA for body text where Pico defaults clear it |
| Bottom-tab bar | **Deferred**. Hamburger is sufficient for v1; revisit after pilot feedback. |
| Dark mode | Out of scope for pilot. Pico supports it but we won't tune it. |
| Offline / PWA | Out of scope for pilot. |

## Pass 1 — already shipped on `mobile-rework`

Done in commits `bfc4bcc`, `3070aa0`, `a405062`, `63e9c5f`:

- Sticky header with CSS-only hamburger drawer (checkbox + `~` siblings)
- `.responsive-table` card-per-row layout for population
- Horizontal-scroll fallback for non-converted tables (`main table:not(.responsive-table) { overflow-x: auto }`)
- Stacked `.filter-bar`, `.filter-grid`, `.actions`, `.quick-links` under 768px
- Single-column `.detail-grid`, 2-up `.stats-grid` under 768px
- Login page uses `.login-main` / `.login-card` instead of inline styles
- `.link-button` (logout) styled as a plain link, not a Pico primary button
- High-contrast active nav item

## Pass 2 — table conversions

Every remaining table needs `class="responsive-table"` on the `<table>` element and `data-label="{{ t.get("...-col-X") }}"` on each `<td>`. The first-column TD (usually a linked name or date) should also get `class="cell-primary"` so the card renders with a clear primary line.

| Template | Current | Notes |
|---|---|---|
| `templates/audit/table_body.html` | wide multi-col table | High-frequency for admins. Convert first. |
| `templates/audit/detail.html` | mixed | Probably a key-value layout — use `.detail-grid` instead, not card-table. |
| `templates/audit/verify.html` | single result table | Convert. |
| `templates/court_calendar/page.html` | 2 tables (today + upcoming) | Both convert. Date as `.cell-primary`. |
| `templates/housing/page.html` | block/cell occupancy | Convert. Add capacity bar inside each card. |
| `templates/transfers/page.html` | transfer log | Convert. |
| `templates/daily_count/page.html` | 2 tables (current + history) | Convert. |
| `templates/operators/page.html` | operator list | Admin-only. Convert. |
| `templates/admission/batch.html` | `id="batch-table"` — dynamic intake builder | **Edge case.** Not a list of records; it's a form-as-table where each row is an in-progress intake. Probably keep as horizontal-scroll OR redesign the batch flow as stacked forms — see Pass 4. |
| `templates/detainee/page.html` | 3 tables (court history, etc.) | Convert. Detail page is the most-visited screen after dashboard. |
| `templates/profile.html` | single info table | Convert, or replace with `.detail-grid`. |

**Acceptance:** every table-bearing screen renders without horizontal scrolling on a 360 px wide viewport. Tap any row → matching detail page.

## Pass 3 — forms

Touch every form template (~12 files under `templates/**/*_form.html` plus `admit/page.html`, `operators/new.html`, etc.) and standardize:

- Labels rendered as block elements above their input (Pico's default if you put input *inside* `<label>` — confirm template idiom is consistent)
- All inputs `width: 100%` on mobile; `box-sizing: border-box` already global via Pico
- All inputs `min-height: 48 px`; `font-size: 16 px` minimum on inputs (iOS Safari otherwise auto-zooms on focus — affects emulator testing even if Android-primary)
- Submit buttons `width: 100%` on mobile, normal width on desktop
- Side-by-side fieldsets (e.g., "min" + "max" days held) collapse to one-per-row under 768 px
- `<select>` elements: rely on native picker — do NOT replace with custom JS dropdown

Forms to audit:

- `admission/page.html` and `admission/basis_fields.html` — multi-step intake; verify htmx-swapped fragments inherit the same mobile styles
- `detainee/*_form.html` — 8 files
- `housing/{create,edit}_form.html`
- `operators/{new,edit,reset_password}.html`
- `court_calendar/outcome_form.html`

**Acceptance:** every form is operable one-thumbed on a 5" portrait screen. No horizontal scrolling. No accidental zoom on focus.

## Pass 4 — dashboard and high-traffic screens

These need design attention, not just CSS:

### Dashboard (`templates/dashboard.html`)
- Population gauge full-width, prominent
- Stats grid: 2-up on phone (already), maybe a "today" summary card at top
- Alerts grid: currently 250 px minmax cards; force single-column under 480 px so each alert is a full-width target
- Quick-links row: stack vertically on phone

### Detainee detail (`templates/detainee/page.html`)
This page is 350+ lines and the most-clicked. Mobile redesign considerations:
- Sticky sub-nav with section anchors (Identity, Legal Basis, Court, Housing, Notes) — only on this page
- Action buttons (`.actions`) as a wrapping grid of full-width primary buttons on mobile
- Inline `.action-form` panels collapse/expand cleanly; verify no overflow

### Admission batch flow (`templates/admission/batch.html`)
- Currently a wide table for entering N detainees at once. On a phone, this is unusable.
- **Decision needed:** single-intake-per-screen with a "next" button, or accept that batch admission is a desktop-only flow and surface a "use a tablet/desktop" hint on phones.
- Recommend: single-per-screen, persists draft in session.

## Pass 5 — polish

- `:active` states on all tappable elements for tactile feedback (subtle scale or background flash)
- `prefers-reduced-motion: reduce` honored for the hamburger animation
- Focus-visible rings clearly visible on dark and light backgrounds
- Loading indicators (`.htmx-indicator`) sized appropriately for mobile — currently inline-block, may need to be more prominent
- Flash messages (`.flash`) full-width on mobile, sticky-toast on top so they're visible even after scroll

## Performance budget

Target: full page render under 2 s on a simulated **"Slow 4G"** in Chrome devtools on a 4× CPU slowdown.

- Total bytes per page: ≤ 80 KB gzipped (HTML + CSS + JS, excluding fonts)
- LCP: ≤ 2.0 s
- TTI: ≤ 2.5 s
- CLS: < 0.1 (sticky header + image-free design makes this easy)

Audit tools: Lighthouse mobile preset, then real-device test on whatever cheap Android the user has.

## Out of scope (for now)

- Native app
- Service worker / offline
- Push notifications
- Camera capture (for detainee photos — currently file upload only, fine on mobile)
- Biometrics
- Tablet-specific layouts (we just inherit either mobile or desktop based on width)

## Sequencing

1. **Pass 2 (tables)** — biggest UX win, all CSS-class-and-data-label additions. Half a day.
2. **Pass 3 (forms)** — half a day. Most of it is CSS; a few templates may need restructure.
3. **Pass 4 (dashboard + detainee detail)** — one day. The detainee detail page is the big one; the dashboard is mostly CSS.
4. **Pass 4 (admission batch)** — half a day, requires a small handler change to persist a single-intake draft across requests.
5. **Pass 5 (polish + perf)** — half a day.

Total estimate: ≈ 3 working days. Each pass deploys independently to `openward-demo.maxisaacs.com` via `git pull` + (if templates touched) `cargo build -p openward-server && systemctl restart openward`.

## Testing

- **Real device:** the user's actual sub-$100 Android, as the canonical north-star check.
- **Emulation:** Chrome devtools → Device Mode → Galaxy A51/71 or Pixel 4a profiles. Throttle CPU 4×, network Slow 4G.
- **Smallest target:** 360 × 640 px (covers Galaxy A series, Moto E, Tecno Spark, common Caribbean phones).
- **Cross-browser:** Chrome (primary), Samsung Internet (secondary), Firefox Mobile (tertiary). No iOS Safari support promised for the pilot, but it should mostly work since CSS is standards-only.

## Risks

- **Pico classless conflicts:** Pico's `nav > ul`, `table`, `details`/`summary` etc. all have opinionated defaults. Pass 1 already needed `!important` to override nav display. Expect more of this in passes 2–4; consider whether to swap Pico for a thinner reset later, but **not for this pilot.**
- **HTMX-swapped content:** when HTMX swaps `#results-container`, our mobile styles need to apply to the new fragment automatically. They do (CSS is global), but verify each htmx-target after conversion.
- **i18n widths:** French translations are ~30% longer than English. Buttons and labels designed at "English width" may break on French. Test with `OPENWARD_LEGAL_PRESET=fr-MG` or similar.
- **Caddy log volume:** mobile users will retry on flaky connections, inflating logs. Not a redesign issue but worth noting for retention tuning.
