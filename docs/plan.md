# Quantix implementation plan

> For agentic workers: use the approved specification and assigned task brief. The primary developer coordinates independent work and performs integration/review. All development agents use gpt-6-astra/xhigh.

**Goal:** Deliver a local engineer-controlled Tender Office in complete working increments.

**Architecture:** Python domain/document/AI service, SQLite and local source objects, typed HTTP API, React frontend and thin Tauri Windows shell.

**Spec:** docs/spec.md. **Interfaces:** docs/contracts.md. **Execution ledger:** docs/progress.md.

## Global constraints

Source packages unchanged; no customer content or secrets in Git; real operations and honest coverage; explicit material approvals; maintained SDKs; readable engineer copy. User authorises autonomous local implementation. The fresh directory itself is the isolated project; initialise a rebuild branch here.

## Task 1 — Document extraction

- [ ] Write failing tests using generated PDF/DOCX/XLSX fixtures for page/cell provenance, actual VBA detection, formulas/errors, cancellation, unknown formats and size limits.
- [ ] Implement backend/quantix/documents.py with the extraction contracts in docs/contracts.md. Keep extraction pure with respect to domain storage.
- [ ] Run targeted tests, validate a private sample in memory, review output and licences.

## Task 2 — Durable local workspace

- [ ] Write failing tests for Tender creation, safe folder/zip import, duplicate/version identity, source retrieval, Tender isolation, FTS queries, interrupted jobs and approval enforcement.
- [ ] Implement backend/quantix/models.py, db.py, repository.py, intake.py, jobs.py and API routes using docs/contracts.md.
- [ ] Add authenticated loopback serving, native file selection and development launcher. Generate frontend API types.
- [ ] Import the private reference package into an ignored local application home; verify original hashes and exact cell/page references.

## Task 3 — Desktop interface

- [ ] Generate/inspect visual concept and record design tokens.
- [ ] Implement Manager, Files/source inspection, Work/decisions, Estimate and Settings against real API operations.
- [ ] Verify import, search, source preview, cancellation, error recovery and responsive layout in browser and native dev launch.

## Task 4 — Manager and work execution

- [ ] Inspect installed official SDK types. Write tests for tool Tender scope, structured source references, approvals and persisted work state with test doubles only in tests.
- [ ] Implement manager conversation, source-backed findings, dynamic proposed plans, approved tasks and independent review using SDK agents-as-tools.
- [ ] Persist events/usage/results and interruption state. Add configured live integration checks without pretending unconfigured runs succeeded.

## Task 5 — Estimating, research and deliverables

- [ ] Implement BOQ rows, Decimal rate build-ups, measured quantity proposals, tax treatments and engineer pricing decisions with real calculation tests.
- [ ] Add web research with dated citations, supplier comparison/RFQ drafts, scoped authorised delivery integration, and response registration.
- [ ] Generate source-linked Excel/Word deliverables and programme/register exports. Verify formulas/totals and render representative documents.

## Task 6 — Full workflow and recovery

- [ ] Complete revision impact, reusable approved knowledge, backups/recovery, resume paths and native startup/shutdown.
- [ ] Run complete verification and independent code review. Fix findings, recheck affected workflows, and record exact evidence and limitations.
- [ ] Provide a running application and concise test instructions. Continue remaining product work after each usable milestone.
