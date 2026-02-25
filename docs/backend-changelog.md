# Backend API Breaking Changes Changelog

This document tracks **breaking changes** to backend-facing APIs used by contracts, services, scripts, and external integrators.

## Purpose

- Provide a single place to review backend API breaking changes.
- Set a lightweight process for adding entries before merge.
- Help integrators plan migrations with clear impact and remediation notes.

## Scope

Track only breaking changes that affect:

- API request/response schemas
- Endpoint paths or HTTP methods
- Required authentication/authorization behavior
- Event payload formats consumed downstream
- Database contract assumptions exposed via API behavior

Do **not** use this log for non-breaking additions, refactors, internal-only changes, or routine bug fixes.

## Process

When a PR introduces a backend API breaking change:

1. Add a new entry in this file under `## Entries`.
2. Include migration/remediation guidance and rollout timing.
3. Link the PR, related issue, and any migration docs.
4. Confirm the changelog update in PR checklist/review notes.

## Entry Template

Copy and complete this block for each breaking change:

```md
### YYYY-MM-DD — <short title>

- **Status:** Planned | Announced | Active | Completed
- **Owner:** <team-or-person>
- **Effective Date:** YYYY-MM-DD
- **PR / Issue:** <link-or-id>
- **Affected APIs:**
  - `<METHOD> <path>`
- **Breaking Change:**
  - <what changed and why this is breaking>
- **Impact:**
  - <who/what breaks if no action is taken>
- **Migration Steps:**
  1. <step 1>
  2. <step 2>
- **Rollback Plan:**
  - <rollback or compatibility strategy>
- **Notes:**
  - <optional context>
```

## Entries

### 2026-02-25 — Backend changelog process initialized

- **Status:** Active
- **Owner:** Platform Team
- **Effective Date:** 2026-02-25
- **PR / Issue:** docs/backend-api-changelog-process
- **Affected APIs:**
  - `N/A (process bootstrap)`
- **Breaking Change:**
  - No API behavior change. Introduces required process and template for future breaking-change tracking.
- **Impact:**
  - Teams now document backend API breaking changes in this file before merge.
- **Migration Steps:**
  1. Use the template above for each new backend API breaking change.
  2. Link to this file from PR descriptions when applicable.
- **Rollback Plan:**
  - Remove process requirement (not recommended).
- **Notes:**
  - Initial baseline entry.

### 2026-02-25 — Current baseline: no pending breaking API migrations

- **Status:** Active
- **Owner:** Platform Team
- **Effective Date:** 2026-02-25
- **PR / Issue:** docs/backend-api-changelog-process
- **Affected APIs:**
  - `All backend APIs (baseline review)`
- **Breaking Change:**
  - No pending untracked backend API breaking migrations at time of initialization.
- **Impact:**
  - Integrators can treat this date as baseline for future breaking-change announcements.
- **Migration Steps:**
  1. No action required.
- **Rollback Plan:**
  - N/A
- **Notes:**
  - Add new entries above this baseline as changes are introduced.
