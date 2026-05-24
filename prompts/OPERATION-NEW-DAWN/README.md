# OPERATION NEW DAWN — Prompt Sequence

Fixes from Smoke Screen Test 001. 20 issues across 10 prompts.

## Dependency Graph

```
A -> B || C || D -> E || F || G -> H || I -> J -> K || L
```

**Phase 1:** `A` (sequential — foundation fix)
**Phase 2:** `B || C || D` (parallel — independent bug fixes)
**Phase 3:** `E || F || G` (parallel — validation, warnings, backups)
**Phase 4:** `H || I` (parallel — count system, configuration)
**Phase 5:** `J` (sequential — mobile nav)
**Phase 6:** `K || L` (parallel — large features, deferred)

## Legend

- `->` means "must complete before next phase starts"
- `||` means "can run in parallel"
- HANDOFF files: `A` writes `HANDOFF-A.md`, which `B`, `C`, `D` read before starting

## Prompts

| Prompt | Issues | Severity | Description |
|--------|--------|----------|-------------|
| **A** | #7 | Critical | Fix HTMX content stacking on detainee actions |
| **B** | #12 | Critical | Fix release not updating status display |
| **C** | #20 | Critical | Fix population filters not working |
| **D** | #1, #11 | High | Audit trail: human-readable details + detail view |
| **E** | #4, #6, #13 | Medium | Phone validation + tab indicator + nav polish |
| **F** | #8, #9 | High | Warning remediation actions + rule explanations |
| **G** | #17, #18 | Medium/Low | Backup completeness audit + auto-refresh |
| **H** | #3 | High | Count system simplification |
| **I** | #2, #10, #19 | Medium | Module stubs + housing single-unit mode + dashboard config |
| **J** | #14, #15 | High/Low | Mobile nav fix + responsive pass |
| **K** | #16 | High | Bulk onboarding wizard (deferred) |
| **L** | #5 | Medium | PDF document generation (deferred) |

## Running

Each prompt is self-contained. Run in a fresh Claude Code session:

```bash
# Phase 1
claude -p prompts/OPERATION-NEW-DAWN/A-htmx-content-stacking.md

# Phase 2 (three terminals)
claude -p prompts/OPERATION-NEW-DAWN/B-release-status.md &
claude -p prompts/OPERATION-NEW-DAWN/C-population-filters.md &
claude -p prompts/OPERATION-NEW-DAWN/D-audit-trail.md &

# Phase 3 (three terminals)
claude -p prompts/OPERATION-NEW-DAWN/E-validation-polish.md &
claude -p prompts/OPERATION-NEW-DAWN/F-warning-remediation.md &
claude -p prompts/OPERATION-NEW-DAWN/G-backup-improvements.md &

# Phase 4 (two terminals)
claude -p prompts/OPERATION-NEW-DAWN/H-count-simplification.md &
claude -p prompts/OPERATION-NEW-DAWN/I-configuration.md &

# Phase 5
claude -p prompts/OPERATION-NEW-DAWN/J-mobile-responsive.md

# Phase 6 (deferred — two terminals)
claude -p prompts/OPERATION-NEW-DAWN/K-onboarding-wizard.md &
claude -p prompts/OPERATION-NEW-DAWN/L-pdf-generation.md &
```
