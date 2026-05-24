# HANDOFF-D: Audit Trail — Human-Readable Details + Detail View (Issues #1, #11)

## Issue #1 — summaries replacing raw JSON

`convert_audit_entry` in `crates/openward-server/src/web/audit.rs` now routes
through a new `format_details(t, action, before, after)` helper. The helper
is pure (parses `serde_json::Value` defensively, never panics) and returns a
localized short string. The field `AuditEntryDisplay.details` now carries
that summary, so the existing `{{ entry.details }}` renders on both the
audit page table and the inline detainee-page audit table (the only change
on `detainee/page.html` was wrapping the timestamp in the new detail link).

Action-to-summary mapping (real action names as written by `SqliteRegistry`):

| action (raw)             | summary                                                             |
|--------------------------|---------------------------------------------------------------------|
| `release`                | "Release (<ReleaseType>)" — release type pulled from `after.release_type` |
| `admit`                  | "Admitted: <surname>, <given_names>" — from `after.identity.*`       |
| `update_detention_basis` | "Basis → <localized basis>" — tagged-enum variant name, translated   |
| `update_facility_status` | "Status → <localized status>" — from `after` (string or variant)     |
| `assign_housing`         | "Assigned to housing" (UUID omitted per spec)                        |
| `schedule_court_date`    | "Court date: YYYY-MM-DD" — from `after.scheduled_date`               |
| `update_court_date`      | same formatter as `schedule_court_date`                              |
| `register_warrant`       | "Warrant registered"                                                 |
| `record_court_outcome`   | "Court outcome: <variant>" — variant name of `after.outcome`         |
| `transfer`               | "Transferred out"                                                    |
| `add_note`               | "Note added"                                                         |
| `delete_note`            | "Note deleted"                                                       |
| `add_property_item`      | "Property logged"                                                    |
| `return_property_item`   | "Property returned"                                                  |
| unknown                  | fallback: truncated JSON (≤60 chars, with ellipsis)                  |

Notes:
- The prompt used placeholder action names (`update_basis`, `update_status`,
  `update_court_outcome`). The real names in `sqlite.rs` are
  `update_detention_basis`, `update_facility_status`, `record_court_outcome`;
  the formatter uses the real names.
- Basis/status values are localized via the existing `legal-basis-*` and
  `legal-status-*` fluent keys.
- Court-outcome summary shows the outcome variant name (not further
  localized) because the variant can carry payload and the existing
  `outcome_key` helper needs a deserialized `CourtOutcome`, which is not
  worth round-tripping here.

## Issue #11 — detail route

- **Route:** `GET /audit/{entry_id}` registered in
  `crates/openward-server/src/web/mod.rs` next to `/audit/search` and
  `/audit/verify`. Requires `Action::ViewAuditTrail` (admin/supervisor).
- **Handler:** `web::audit::detail_page` in `crates/openward-server/src/web/audit.rs`.
- **Template:** `crates/openward-server/templates/audit/detail.html`
  (extends `base.html`).
- **Registry addition:** `openward_registry::audit::get_audit_entry_by_id`
  in `crates/openward-registry/src/audit.rs`. Returns a new
  `AuditEntryDetail { entry, self_hash, chain_hash, chain_verified,
  self_hash_verified }`. It re-derives the stored hashes using the existing
  `compute_self_hash` / `compute_chain_hash` and compares against the
  preceding entry (by rowid) for chain continuity. This is purely additive
  and does not touch `SqliteRegistry` logic.
- **Clickable rows:** In `templates/audit/table_body.html` and in
  `templates/detainee/page.html` the timestamp cell is now wrapped in
  `<a href="/audit/{{ entry.id }}">`. `AuditEntryDisplay.id` already existed.
- **Error handling:** added `HtmlError::NotFound` (404) for missing/invalid
  audit IDs in `web/errors.rs`.

### Detail page fields

- **Overview** `<dl>`: timestamp (UTC), operator display name + operator
  UUID (monospace), module, localized action label + raw action code,
  target (linked to `/detainees/{id}/view` when present), entry UUID, epoch.
- **After / Before**: each JSON object rendered as a `<dl>`, one level of
  nesting flattened (`parent / child` label). Primitives printed directly;
  arrays and deeper objects printed as truncated JSON (≤200 chars). Nulls
  display as em-dashes.
- **Chain integrity**: three states — "Verified" (both self-hash and chain
  link match), "Chain link broken" (self-hash OK but chain hash diverges
  from prev), "Entry tampered" (self-hash does not recompute). Raw
  `self_hash` and `chain_hash` printed as lowercase hex.

### What isn't exposed

- Operator lookups beyond `display_name` (already joined on `operators.id`).
  No extra lookups.
- Housing unit name for `assign_housing` — per spec, the bare "Assigned to
  housing" label is shown. The UUID remains available in the detail page's
  "After" section.

## i18n keys added (all three locales: en/fr/mg — Malagasy copies English)

- Summary: `audit-summary-{release, admitted, basis-change, status-change,
  assigned-housing, court-date, warrant-registered, court-outcome, transfer,
  note-added, note-deleted, property-added, property-returned}`
- Action labels: `audit-action-{admit, update-basis, update-status,
  assign-housing, register-warrant, schedule-court-date, update-court-date,
  record-court-outcome, add-note, delete-note, add-property,
  return-property, transfer, release}`
- Detail page: `audit-detail-{title, overview, entry-id, epoch, before,
  after, integrity, integrity-ok, integrity-chain-broken, integrity-tampered,
  self-hash, chain-hash}`

Each of the three `ui.ftl` files has identical audit-key count (83).

## Files changed

- `crates/openward-registry/src/audit.rs` — added `AuditEntryDetail` +
  `get_audit_entry_by_id`.
- `crates/openward-server/src/templates.rs` — added `AuditDetailField`,
  `AuditDetailDisplay`, `AuditDetailTemplate`.
- `crates/openward-server/src/web/audit.rs` — new `format_details`,
  `flatten_fields`, `detail_page`; rewrote `convert_audit_entry` to call
  the new formatter; dropped the old `summarize_changes` dumper.
- `crates/openward-server/src/web/util.rs` — added `action_label()`.
- `crates/openward-server/src/web/errors.rs` — added `HtmlError::NotFound`.
- `crates/openward-server/src/web/mod.rs` — registered
  `/audit/{entry_id}` route.
- `crates/openward-server/templates/audit/table_body.html` — timestamp
  wrapped in detail link.
- `crates/openward-server/templates/audit/detail.html` — new template.
- `crates/openward-server/templates/detainee/page.html` — timestamp
  wrapped in detail link (the only additional edit; I re-read B's version
  first and did not touch anything else).
- `crates/openward-server/locales/{en,fr,mg}/ui.ftl` — new keys appended.

## Build + test status

- `cargo build`: clean, no warnings, no errors.
- `cargo build --tests`: clean — all test binaries compile.
- `cargo test`: **not executed in this session** — the sandbox here
  denies the `cargo test` command (permission error, independent of flags),
  so I could not get a pass/fail count. The compilation-only checks above
  demonstrate that nothing broke structurally; the i18n-validation test
  relies only on the presence of keys in `en/ui.ftl`, `fr/ui.ftl`,
  `mg/ui.ftl` (all three got the same ~40 new keys). Please run
  `cargo test` locally to confirm the ≥154 passing baseline.

## Constraints respected

- No changes to `openward-core` or `SqliteRegistry`.
- The new registry function mirrors existing patterns in
  `list_audit_for_detainee` / `verify_chain` (no schema changes).
- No JS, no build step, no framework.
- B's edits on `detainee/page.html`, `templates.rs`, `web/detainee.rs`,
  `static/style.css`, and the three `ui.ftl` files were preserved; my
  additions are additive only.
