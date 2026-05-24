# Prompt L — PDF Document Generation (Issue #5)

**Priority:** Medium (deferred)
**Phase:** 6 (parallel with K)
**Depends on:** All prior phases. Read ALL `HANDOFF-*.md` files.

## Setup

Read ALL `HANDOFF-*.md` files in the project root.

## Problem

Current print functionality is just browser "Print Page." Should instead generate proper PDF documents from database data in configurable formats. The documents produced by this system may be filed in courts, so they need to look professional and correct.

## Design Considerations

The original spec mentioned a LaTeX-based template system, but that requires a LaTeX installation on the Raspberry Pi (heavy). Consider alternatives:

### Option A — LaTeX Templates (Max's Preference, But Heavy)
- Pro: Beautiful output, configurable templates, facilities can write their own
- Con: Requires `texlive` on the Pi (~200MB+), slow compilation
- If chosen: use `tectonic` (smaller, self-contained LaTeX engine)

### Option B — HTML-to-PDF (Lighter)
- Pro: Already have HTML templates, lightweight
- Con: Less control over page layout, headers/footers
- Use `weasyprint` or `wkhtmltopdf` as a subprocess
- Or a Rust-native solution like `printpdf` / `genpdf`

### Option C — Rust-native PDF (Lightest)
- Pro: No external dependencies, fast
- Con: More code to write, less flexible templates
- Use `genpdf` crate — Markdown-like API for PDF generation

**Recommendation:** Start with Option C (`genpdf`) for the MVP. It's the lightest dependency and runs natively on ARM. Templates can be Rust structs/functions initially, with a move to external templates later.

## Documents to Generate

1. **Admission Record** — formal admission document with detainee identity, basis, intake date
2. **Release Certificate** — official release document with release type, date, authorized by
3. **Transfer Order** — transfer document with from/to facility, reason, authorization
4. **Court Appearance Summary** — upcoming court date details for the detainee to carry
5. **Population Report** — current population snapshot, basis breakdown, flag summary
6. **Daily Count Report** — formal count reconciliation for a specific date

## Implementation

1. Add `genpdf` to `openward-server/Cargo.toml`
2. Create `crates/openward-server/src/pdf.rs` — PDF generation functions
3. Add routes:
   - `GET /detainees/{id}/pdf/admission` — admission record PDF
   - `GET /detainees/{id}/pdf/release` — release certificate PDF
   - `GET /detainees/{id}/pdf/court-summary` — court summary PDF
   - `GET /reports/population.pdf` — population report PDF
   - `GET /reports/daily-count/{date}.pdf` — daily count report PDF
4. Replace the "Print" button on the detainee page with a dropdown of document types
5. Each generates a PDF and serves it with `Content-Type: application/pdf`

## Template Structure

Each PDF should include:
- Facility name and header (from config)
- Document title
- Date generated
- Body content (varies by document type)
- Footer with page numbers
- Optional: facility logo (if configured)

## Key Files

- `crates/openward-server/Cargo.toml` — add genpdf dependency
- New: `crates/openward-server/src/pdf.rs` — PDF generation
- `crates/openward-server/templates/detainee/page.html` — replace print button
- `crates/openward-server/src/web/detainee.rs` — add PDF route handlers
- `crates/openward-facility/src/lib.rs` — facility name/config for headers

## Verification

1. `cargo build` must succeed (including on ARM — check genpdf compiles for aarch64)
2. Navigate to a detainee, click "Admission Record" — downloads a PDF
3. PDF should be readable, professional, and contain correct data
4. Population report PDF should reflect current data

## Handoff

Write `HANDOFF-L.md` in the project root with:
- PDF generation approach chosen and why
- Documents implemented
- Any external dependencies added
- File size / generation speed for typical documents
- Whether it compiles for ARM (aarch64-unknown-linux-gnu)
