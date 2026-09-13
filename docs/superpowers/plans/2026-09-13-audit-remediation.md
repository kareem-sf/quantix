# Tender workflow audit remediation implementation plan

> **For agentic workers:** Use superpowers:subagent-driven-development for independent bounded tasks, with root-owned integration and verification. Track each step below.

**Goal:** Repair the concrete audit findings across evidence, estimates, reports, progress and responsive navigation.

**Architecture:** Keep the current local service and approval boundaries. Add focused source-BOQ and analysis-selection helpers; extend typed records only where needed to preserve evidence and applicability. Derive UI links and freshness from saved records.

**Tech Stack:** Python 3.12, SQLite, FastAPI/Pydantic, React/TypeScript, existing Base UI/shadcn, pytest/Vitest.

**Spec:** `docs/design/audit-remediation.md`

## Global constraints

- Preserve supplied Tender files, existing uncommitted changes, source/version identities and engineer authority.
- Runtime, synthetic stores, snapshots and logs remain beneath `~/.quantix`.
- Read `docs/contracts.md`; update Pydantic schemas and generated frontend types together.
- No release package, paid AI request, real Tender approval or commercial send.
- Root owns architecture, integration, inspection and final verification. Each delegate owns only its specified files; do not commit or start another agent.
- Ruling: execute in the existing checkout and use a home-scoped audit ledger/baselines; the operative tree is mostly uncommitted and must not be replaced with the obsolete Git baseline.

## Task 1: Attributable analysis exports

Files: `backend/quantix/outputs.py`, new `backend/quantix/report_analysis.py`, `backend/quantix/output_word.py`, `backend/tests/test_outputs.py` or a focused new report test module.
Consumes: repository messages/runs and existing output capture. Produces: deterministic selected engineering message plus explicit limitations without an HTTP shape change.

- [x] Reproduce analysis → greeting → export, failed later analysis, and no attributable analysis with real Repository/OutputService tests.
- [x] Select a completed manager/engineering run rather than the most recent manager-role message. Preserve source-current checks and stable export fingerprints after unrelated conversation.
- [x] Run focused tests; root inspects the exact changes and tests.

## Task 2: Source-linked BOQ proposals

Files: `backend/quantix/db.py` if migration required, `estimate_models.py`, `estimates.py`, focused new source-BOQ service/model helper, `estimate_routes.py`, `office_types.py`, `office_business.py`/`office_quantities.py`, frontend Estimate/source-row form and affected tests.
Consumes: exact current evidence and existing estimate confirmation/pricing. Produces: unconfirmed source BOQ rows with multiple rows per source, no invented source evidence, and normal downstream rates/totals/exports.

- [x] Write synthetic PDF-page/Word-source tests for two distinct rows on one source, unchanged retry, cross-Tender/stale/altered-excerpt rejection, refresh preservation and explicit confirmation.
- [x] Implement source row identity and a tested transactional migration preserving existing IDs/FKs if the existing unique key requires change.
- [x] Expose the proposal API and Manager proposal publication within the existing source/active-run fence; never let the model confirm rows.
- [x] Add the plain-language engineer form and actual source navigation. Verify the full source proposal → confirm → price → draft path with synthetic data.

## Task 3: Conditional requirements

Files: `requirement_models.py`, `tender_requirements.py`, `office_project.py`, `office.py`, `SubmissionRequirements.tsx`, relevant tests.
Consumes: current inspected evidence. Produces: source clause/applicability/condition/exception metadata and explicit review.

- [x] Add failing tests for an English/Arabic conditional clause presented as unconditional, preserved exceptions, missing attribution and historical proposals.
- [x] Validate selected clause against actual evidence; require qualification preservation without treating string matching as complete semantic understanding.
- [x] Show the clause, applicability and conditions in proposal/review; keep real prior records untouched and all decisions explicit.

## Task 4: Results, progress and usable navigation

Files: `repository.py`, `models.py`, `work_brief.py`/models, Manager publication/continuation, `ManagerMessage.tsx`, `CurrentWork.tsx`, workspace routing, `AppShell.tsx`, `Files.tsx`, shared checkbox styles and affected tests.
Consumes: actual saved result IDs, run provenance and source revisions. Produces: actionable result links, visible stale progress, truthful extraction labels and accessible navigation.

- [x] Reproduce missing requirement/draft links and outdated Current work after a later material engineering result.
- [x] Extend server-derived links and existing record routes; require/guide continuation brief updates and make missing updates truthful.
- [x] Add an always-reachable narrow navigation trigger, correct legacy checkbox sizing, and explicit extraction terminology.
- [x] Verify keyboard and source-return behavior, light/dark, 736px/desktop and enlarged layout.

## Task 5: Integration and verification

- [x] Regenerate API bindings after all schema changes; verify typecheck and schema consistency.
- [x] Run affected backend/UI tests, then full suites on integrated code; correct failures without weakening guards.
- [x] Inspect all owned diffs against captured baselines, including an independent review.
- [x] Exercise repaired synthetic journey in the running application. Record exact evidence and any live/platform limits in `docs/progress.md`.
