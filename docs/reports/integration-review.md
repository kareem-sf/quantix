# Integrated foundation review

Reviewed 2026-09-06 against `docs/spec.md`, `docs/architecture.md` and `docs/contracts.md`. The initial read-only review found the four defects below. The primary agent accepted them and authorized the bounded fixes recorded in the resolution section. These changes do not complete the separately documented later milestones.

## Resolution and verification

All four reproduced defects are fixed in the working tree:

- **Approval scope:** `Repository.approved_scope(tender_id, plan_id)` returns the approved plan's original Tender revision and full saved rationale. Manager/task contexts pass that scope and its binding conditions into the model instructions, including nested consultations and resumed tasks. The obsolete `current_plan` fixture/prompt key was removed in favor of `plan`.
- **Revision control:** a new `plan_basis` table captures the source-package revision at proposal. Approval requires that revision to remain unchanged. Already approved tasks remain runnable after unrelated document additions; superseded cited inputs and invalidated task states block execution. New findings/plans/messages cannot cite superseded evidence as current. Historical reads remain available with explicit `version` and `is_current` in the model-facing source payload. Both result citations and recorded read-source dependencies invalidate completed task work when revised.
- **Atomic publication:** SDK workers now return a `PreparedOfficeResult`, with validated output, usage, read evidence IDs and provider-returned web-source records. The job runner publishes the office records, run result/status, task result/status and completion event in one synchronous SQLite transaction. Failed run/task completion writes roll the complete publication back. Cancellation before final publication leaves no manager proposals. Queued cancellation and immediate shutdown now save cancelled/interrupted terminal states even when the coroutine has not started.
- **VAT:** both net and gross round at line boundaries from the unrounded applicable basis. VAT-inclusive gross amounts use the original inclusive rate. Generated workbooks preserve that rate and its tax basis in two additional BOQ columns, and their gross formulas use the correct basis.

**Verification:** the new regression file initially produced 13 expected failures and one passing normal-money control before implementation. The final combined repository/API/intake/office/SDK/visual-source/estimate/output/transaction suite passes **75 tests**, including binding limits in nested consultation and resumed-task instructions. Ruff passes for every changed source/test file. The existing Starlette/AnyIO deprecation warning remains.

Actual Microsoft Excel recalculation was exercised on temporary synthetic exports, with an owned hidden Excel instance closed afterward:

| Rate basis | Supplied rate | Quantity | Net line | Gross line and Summary |
|---|---:|---:|---:|---:|
| Including VAT, 14% | 0.04 | 1 | 0.04 | 0.04 |
| Including VAT, 14% | 19.99 | 3 | 52.61 | 59.97 |
| Including VAT, 14% | 11.40 | 12.5 | 125.00 | 142.50 |
| Excluding VAT, 14% | 10.10 | 12.5 | 126.25 | 143.93 |

No provider calls, customer-data changes, release packaging or commits were performed. Root-owned visual-source functionality remains covered by the combined tests. HTTP response models are unchanged; internal worker/repository contracts changed as described above. The read-dependency invalidation record covers evidence read in the successful task run; this is evidence provenance, not a claim of complete document review.

The following sections preserve the original findings and reproductions for the audit record.

## P1 — Approval limits are not passed to the approved workers

**Locations:** `src/features/Work.tsx:73`; `backend/quantix/repository.py:419`; `backend/quantix/jobs.py:91`; `backend/quantix/office.py:42`.

The approval dialog explicitly invites the engineer to record “any limits on the work”. The approval note is saved only in the decisions ledger. Task activation and the worker prompt never read it: workers receive the original task description, the plan, recent messages and evidence, but no approval conditions. A specialist therefore cannot comply with a new scope limit entered at the actual approval step.

**Reproduced locally:** create a proposed task, approve with the note `LIMIT-731: Review only this source. Do not perform web research.`, then build the assigned task's `_prompt`. The note is absent from the prompt, and the approved task starts normally. No provider request was needed. The omitted scope is observed; whether a particular model would perform the excluded work was not tested.

**Correction:** make approval conditions part of the durable approved work scope and pass them to every task and nested consultation. Preserve them on resume. If conditions require a revised plan, keep tasks inactive until that revision is approved. Add a behavior test proving that a condition entered through the approval API reaches the specialist's effective instructions.

## P1 — Superseded evidence can regain an apparently current approval

**Locations:** `backend/quantix/repository.py:112`, `:205`, `:233`, `:385`; `backend/quantix/office_tools.py:36`, `:64`; `backend/quantix/office.py:74`.

Revision invalidation checks only tasks in `ready` or `completed`, so a task awaiting plan approval survives a source revision. `approve_plan` then unconditionally sets that plan's tasks to `ready`, and neither starting the task nor reading its supplied evidence checks that the artifact is current. The source tool also omits the revision/current state. Separately, `_check_sources` accepts historical evidence when creating a new finding, and that finding starts with `is_stale=false`, allowing the existing stale-finding decision gate to be bypassed.

**Reproduced locally:** register `spec.pdf` with C30, propose a task citing it, then register a C40 revision at the same path. The old plan still approves, its task transitions to `running`, and its worker prompt contains C30 without a stale label. Adding a new requirement finding against the C30 evidence after the revision produces `is_stale=false`; `decide_finding(..., 'accept', ...)` accepts it.

**Correction:** retain historical evidence for inspection, but check current source dependencies when approving/starting work and publishing or accepting current findings. Invalidate proposed tasks as well as ready/completed tasks. Include artifact version/current state in source reads. Revision dependency checks should also include evidence retrieved into `task.result.source_ids`, since a specialist can read sources beyond the task's original seed list. The latter gap was inspected in code, not separately executed.

## P2 — Office publication and job completion are separate commits

**Locations:** `backend/quantix/office.py:90`, `:203`; `backend/quantix/jobs.py:158`, `:167`, `:190`, `:217`.

The office transaction commits findings, plans, the manager reply and its publication event before the job runner saves the durable result and completed status. The task status/result is a further write. A storage failure or process exit between these commits leaves published work attached to a failed/interrupted run with no stored result. Resume invokes the provider again and republishes the result; plans can be duplicated or superseded despite the earlier output already being visible.

**Reproduced with a local test worker:** use the real `_persist` with a synthetic `OfficeOutput`, and inject `OSError` only on `repo.update_run(status='completed')`. The resulting run is `failed`, its `result` is `{}`, and one manager reply is already committed. Resume without the injected error produces a second manager reply. No live provider, real key or customer data was used. The analogous process-exit window follows from the same transaction boundaries; a process was not forcibly terminated for this review.

**Correction:** publish the validated office records, durable run result/status and task result/status in one synchronous transaction owned by the finalization path, with no await inside it. Alternatively, persist a publication marker/result keyed by run ID and make restart recovery recognize it without re-executing the model. Test failure at terminal publication and successful resume/recovery without duplicate domain records.

## P2 — Reapplying VAT to a rounded net line changes a VAT-inclusive price

**Locations:** `backend/quantix/estimates.py:462`, `:469`, `:471`; `backend/quantix/outputs.py:128`, `:131`.

For `tax_basis='including_vat'`, the service divides the supplied rate by the VAT factor, rounds the net line to two decimals, and calculates the gross line from that rounded net. This can change the gross amount implied by the engineer's supplied VAT-inclusive rate. The workbook follows the same calculation, so agreement between the workbook and server does not detect the error.

**Reproduced locally:** confirm one item with quantity `1`, unit rate `0.04`, tax basis `including_vat`, VAT `14`, currency `EGP` and valid dated provenance. The service returns net `0.04` and gross `0.05`. The entered gross line is `1 × 0.04 = 0.04`. This is a rounding defect, independent of which VAT percentage the engineer establishes.

**Correction:** for VAT-inclusive input, derive the gross line directly from quantity × the supplied inclusive rate and calculate the net/tax split under an explicit rounding rule. Keep the workbook formulas consistent with that basis. Add rounding-boundary examples for inclusive prices, rather than testing only a rate that divides exactly by the VAT factor.

## Evidence and limits

- Inspected repository/SQLite and FTS queries, intake and job lifecycle, API authentication/settings, office prompts and source tools, estimate/output services, frontend request/forms/source drawers and Tauri connection/launcher configuration.
- Ran `backend/.venv/Scripts/python.exe -m pytest backend/tests/test_repository.py backend/tests/test_api.py backend/tests/test_transactions.py backend/tests/test_estimates.py backend/tests/test_outputs.py -q`: **30 passed**, one Starlette/AnyIO deprecation warning.
- Ran the synthetic reproductions above using temporary local repositories. Main evidence lookup/search queries are Tender scoped, current FTS search excludes superseded artifacts, estimate source revisions do not inherit row approvals/rates, and output downloads check Tender ownership and content hashes in the inspected paths.
- Did not read `.quantix-dev/connection.json`, customer documents or customer runtime data; did not make provider calls, change source files or commit changes. Frontend and native behavior was reviewed from code, not exercised in a fresh browser/native session.
- Excluded in-progress visual source and semantic API work from completeness judgments. The primary agent's already identified queued-cancellation defect is not repeated here.
