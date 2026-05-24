# OPERATION NEW DAWN — Smoke Screen Test 001

**Date:** 2026-03-21
**Tester:** Max
**Scope:** General smoketest across core workflows (admissions, release, counts, audit trail, backups, housing, navigation, population)

---

## Critical / Showstoppers

### Issue 7: Actions load new content below existing content instead of replacing it
Performing an action on a detainee view stacks the new page vertically below the current one — two title bars, two full pages. Must replace content, not append. **Update:** Goes away on second navigation, but root cause still needs fixing.

### Issue 12: Released detainee still shows "on trial" status
After releasing a fake inmate, their status still displays as "on trial" instead of "released." Either the release action isn't updating the status, or the UI isn't reflecting the change. Status must accurately reflect the current state.

### Issue 20: Population filters do not work
Neither the standard filters nor the advanced filters on the Population page are functional.

---

## Audit Trail

### Issue 1: Audit log "Details" column displays raw JSON blobs
The release event for "Isaacs, Maxwell" shows the full serialized JSON (`{"all_property_returned":false,"authorized_by":"a023a51f-..."}`) unformatted in the Details column. Unreadable, especially for critical actions like `release`. Needs human-readable formatting in the UI.

### Issue 11: Audit trail entries should be clickable with detailed view
Each audit log action should link to a dedicated page showing a clear, human-readable breakdown: who performed the action, what they did, when, how, whether it was authorized, etc. Not raw JSON — a proper formatted view.

---

## Validation & Data Integrity

### Issue 4: Phone number validation is too lenient
Emergency contact phone numbers accept obviously invalid input (e.g., "777"). Most fields don't need strict validation, but phone numbers are a special case — this may be the *only* time this data is collected. Validation should be lenient (accept various formats) but reject clearly invalid entries.

---

## Workflow & UX

### Issue 3: Count system needs to be more straightforward
The count workflow is overly complex. This is likely one of the highest-value features and one of the biggest friction points when transitioning to a technology-based system. Needs simplification.

### Issue 5: Printing should generate formatted PDF documents, not print the page
Current print functionality feels like browser "print page." Should instead generate proper PDF documents from database data in configurable formats. Leaning toward a LaTeX-based template system so facilities can write templates matching their specific legal system's requirements. Open question: templates loaded at compile time (simpler) vs. runtime-configurable (more flexible). Decision deferred.

### Issue 6: Active tab indicator is nearly invisible
When viewing a detainee, there is a slight visual cue for the active tab, but it's extremely hard to see. Needs to be much more obvious.

### Issue 8: Warnings need immediate, clickable remediation actions
Warnings/critical alerts should not just state a problem. They should link to a guided walkthrough that spoon-feeds the user exactly what to do to resolve it. Easy wins, huge impact.

### Issue 9: "Housing rule violation" warning needs explanation
Currently just says there's a violation with no context on *why*. Needs to show the specific rule being violated and what's wrong.

---

## Configuration & Flexibility

### Issue 2: Module dropdown references stubbed/unimplemented modules
The "Module" filter dropdown lists modules that won't exist in the initial release. Should be gracefully stubbed — hidden, disabled with a "coming soon" indicator, or simply not listed until implemented.

### Issue 10: Housing system needs a simplified/single-unit mode
Many facilities (Madagascar, Sahel, Philippines, etc.) don't have separate housing units. Admin option needed to use a single static housing unit or effectively disable housing complexity. Keep the full feature — it's mandatory for facilities that need it — but make it flexible for simpler facilities.

### Issue 19: Admin-configurable dashboard via prison settings
There should be a prison settings section (in the setup wizard and accessible to admins) that lets you configure what the dashboard displays. Current default of "pretrial population" may suit NGOs but isn't the most useful day-to-day metric for facility staff (e.g., "last daily count" might matter more). Dashboard widgets should be configurable per facility.

---

## Onboarding & Setup

### Issue 16: Initial setup wizard needed for existing facilities
A prison already operating with hundreds or thousands of detainees can't onboard via "New Admission" one at a time. Needs a dedicated bulk onboarding workflow for multiple people to spend days entering data cleanly. OCR is likely overengineered — won't work reliably on target hardware, may fail on handwriting or Malagasy. Better to plan for manual setup, since an NGO will already be on-site to configure the Pi.

---

## Backups

### Issue 17: Backup completeness audit needed
Backups are in good shape overall, but anything missing (individual or full) should be explicitly weighed and justified. Default should be "back it up."

### Issue 18: Backup page doesn't auto-update after creating a backup
After making a backup, the "last backup" info doesn't refresh until manual page reload. Easy fix.

---

## Navigation & Mobile

### Issue 13: Top nav needs minimal styling improvements
Doesn't need to be fancy (target devices are ~$80 phones), but needs basic readability — separators between items, slightly more readable text. Current look is not final. Low priority.

### Issue 14: Nav breaks on mobile
Literally broken on mobile devices. Critical given the target hardware.

### Issue 15: Full mobile responsiveness pass needed
Will require a concerted effort to port the UI to work well on mobile. Final-stage effort once the foundation is solid, but non-negotiable given the target users and devices.

---

## Summary

| Severity | Count | Issues |
|----------|-------|--------|
| Critical / Showstopper | 3 | #7, #12, #20 |
| High | 6 | #1, #3, #8, #11, #14, #16 |
| Medium | 7 | #4, #5, #6, #9, #10, #17, #19 |
| Low / Polish | 4 | #2, #13, #15, #18 |
| **Total** | **20** | |
