# DS-05: End-to-End Integration Test

**Stream:** Final validation
**Priority:** Required before release
**Dependencies:** ALL previous prompts (01a through 04c) must be complete

## Context

With all streams implemented, this prompt validates that everything works together as a cohesive system.

## Tasks

### 0. Pre-flight Fixes

*EDIT:* These broke during the 01–04 sprint and need to be fixed before writing any new tests.

**Compile errors in `crates/openward-server/src/web/admission.rs`:**
- Line ~234: `SharedWarrantData` initializer is missing fields `date_received`, `issuing_officer`, and `offence_description`. Add them.
- Line ~267: `release_date` expects a `ReleaseDate` but gets a `NaiveDate`. Wrap it in the correct type.

**Migration test assertions in `crates/openward-db/tests/migrations.rs`:**
- `fresh_database_gets_all_migrations` and `migrations_are_idempotent` hardcode an expected count of 3 migrations. There are now 5. Update the assertions.

After these fixes: `cargo check` should pass and `cargo test --workspace` should have zero failures.

### 1. Full Lifecycle Test

Write an integration test that exercises the complete workflow:
1. Fresh database — migrations apply
2. Default admin created via first-run setup
3. Admin logs in, changes password
4. Admin creates operators with different roles
5. Operator admits a detainee (full admission flow)
6. Detainee appears on dashboard with correct statistics
7. Search returns the detainee with flags
8. Add notes and property to the detainee
9. Schedule and resolve a court date
10. Update legal basis
11. Transfer the detainee
12. Daily count workflow
13. Backup download
14. CSV export contains expected data
15. Audit trail shows all actions with intact chain hash
16. Readonly user cannot perform write operations
17. Health check endpoint responds

### 2. Cross-Stream Validation

Verify that:
- Flag counts on dashboard match search statistics
- Audit entries exist for every mutating operation
- Session management works across role changes
- Backup restores cleanly and all migrations are satisfied

### 3. Regression Check

Run the full existing test suite (97+ integration tests) to confirm nothing is broken.

## Exit Criteria

Full lifecycle test passes. All 97+ existing tests still pass. No regressions. System is ready for field deployment.
