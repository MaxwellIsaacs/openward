# Prompt H — Count System Simplification (Issue #3)

**Priority:** High
**Phase:** 4 (parallel with I)
**Depends on:** Phases 2-3 complete. Read all `HANDOFF-*.md` files.

## Setup

Read all `HANDOFF-*.md` files in the project root before starting.

## Problem

The daily count workflow is overly complex. This is likely one of the highest-value features (daily population reconciliation) and one of the biggest friction points when transitioning from paper to a digital system. Needs simplification.

## Context

The daily count is the process of physically counting every person in the facility and reconciling against the register. The goal: at any point, you should be able to answer "how many people are here and does that match our records?"

Target users are facility staff who may have limited technical literacy and are accustomed to a paper-based system (tally marks, handwritten totals).

## Current Implementation

**Key files:**
- `crates/openward-server/templates/daily_count/page.html` — count form template
- `crates/openward-server/src/web/daily_count.rs` — count handler
- `crates/openward-core/src/traits.rs` — `DailyCount` type, count-related methods
- `crates/openward-registry/src/sqlite.rs` — count persistence

**Current flow (likely):**
1. Open a count for today
2. Enter headcounts per housing unit
3. System shows expected vs actual
4. Finalize when balanced

## Fix — Simplification Principles

1. **One-page workflow:** The entire count should happen on a single page, not require navigating between multiple views
2. **Per-unit entry:** Show all housing units with an input field for each. Pre-fill with expected count.
3. **Live balance indicator:** As numbers are entered, show a running total vs expected. Use clear color coding (green = balanced, red = discrepancy)
4. **Discrepancy handling:** When counts don't match, show a clear summary: "Expected 142, counted 140 — discrepancy of 2"
5. **No jargon:** Labels should be plain language ("How many people are in [Unit Name]?")
6. **Big touch targets:** Input fields and buttons should be large — remember, possibly using on phones
7. **Auto-open:** If no count exists for today, automatically start one when the page is visited (or show a single "Start Today's Count" button)
8. **History view:** Show recent count history below the current count form (last 7 days)

## UX Flow (Simplified)

```
/daily-count
  ┌─────────────────────────────┐
  │ Daily Count — March 21, 2026│
  │                             │
  │ Unit A:  [___12___]         │
  │ Unit B:  [___8____]         │
  │ Unit C:  [___15___]         │
  │                             │
  │ Your Count:     35          │
  │ Expected:       36          │
  │ Discrepancy:    -1  ⚠       │
  │                             │
  │ [Submit Count]              │
  └─────────────────────────────┘

  Recent Counts:
  Mar 20: 36/36 ✓
  Mar 19: 35/35 ✓
  Mar 18: 34/36 ⚠ (2 unresolved)
```

## Key Files

- `crates/openward-server/templates/daily_count/page.html` — redesign this
- `crates/openward-server/src/web/daily_count.rs` — simplify handler logic
- `crates/openward-server/static/style.css` — add count-specific styles
- `crates/openward-core/src/traits.rs` — `DailyCount` type (don't modify unless necessary)

## Verification

1. `cargo build` must succeed
2. `cargo test` must pass
3. Navigate to `/daily-count` — should show today's count form
4. Enter numbers for each unit, see live totals
5. Submit and see the result
6. Previous days' counts visible below

## Handoff

Write `HANDOFF-H.md` in the project root with:
- Before/after description of the workflow
- Any backend changes needed
- Whether you modified core types
