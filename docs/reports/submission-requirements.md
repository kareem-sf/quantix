# Submission requirement register

Implemented for the continuation request in development-only mode. No tests, lint, typechecks, browser checks or live verification were run for this increment, following the explicit user instruction.

## Scope and integration

`RequirementService(repo)` in `backend/quantix/tender_requirements.py` owns immutable requirement proposals and an append-only event history. The router is `requirement_routes.create_router(repo)`. Input and response types live in `requirement_models.py`. The frontend export is `SubmissionRequirements({tenderId})`, with its own stylesheet and the existing evidence picker and source drawer.

Root owns API inclusion, generated frontend bindings, Manager proposal publication, placement in the document workspace and final export integration. The service has no message sending or external submission action.

## Service contract

```python
service.propose(tender_id, values, origin="engineer", run_id=None)
service.create(tender_id, values)
service.list(tender_id, include_withdrawn=False, offset=0, limit=50)
service.get(tender_id, requirement_id)
service.decide(tender_id, requirement_id, values)
service.link_output(tender_id, requirement_id, values)
service.unlink_output(tender_id, requirement_id, output_id, values)
service.submission_basis(tender_id, requirement_ids, output_ids)
```

Proposal fields are `title`, `detail`, nonempty distinct `source_ids`, `deliverable_kind` and optional `due_date`. Deliverable kinds match generated outputs exactly: `boq_xlsx`, `client_boq`, `analysis_docx`, `technical_docx`, `registers_xlsx`, `comparison_xlsx`, `programme_xlsx`.

Manager proposals require a run belonging to the same Tender. Root must additionally restrict their source IDs to sources actually read by that run and publish within the job's final transaction. No Manager route can approve, link or mark a requirement satisfied.

## HTTP routes

| Method and route | Contract |
| --- | --- |
| GET/POST `/api/tenders/{id}/requirements` | List `RequirementRecord[]` / create from `RequirementProposal` |
| GET `.../requirements/{requirement_id}` | One complete record with source, link and decision history |
| POST `.../{requirement_id}/decision` | `RequirementDecision`: decision, engineer confirmation and rationale |
| POST `.../{requirement_id}/outputs` | `RequirementOutputLink`: output ID, engineer confirmation and rationale |
| POST `.../{requirement_id}/outputs/{output_id}/unlink` | `EngineerDecision` |

List parameters are `include_withdrawn`, `offset` and `limit` (1–100). Routes use the root application's session/origin controls and domain error handling.

## Authority and currentness

The proposal preserves the original source artifact, hash, version, exact locator and an evidence-content fingerprint. Currentness checks the preserved file bytes and size as well as the current artifact/evidence identity. Missing, revised or changed sources remain visible and block approval or a new completion review. Withdrawal remains possible when a source has changed.

Requirement content is immutable. Engineer actions append `requirement_events` and the shared `decisions` ledger; database triggers reject updates and deletions. Changing the title, source or stated due date therefore requires withdrawing and proposing the corrected requirement. Proposal creation does not alter Tender source revision or extraction/review coverage.

Approval establishes that the requirement belongs in the register. It does not mark the requirement satisfied. Linking requires a real, current generated document of the required kind in the same Tender, with checked bytes/manifest and an unblocked current working basis. The link preserves its filename, hash, basis and engineer rationale. Link changes invalidate completion review while retaining earlier decisions.

`satisfied` requires current linked documents and an explicit engineer review. `exception` records an explicit reviewed departure, with rationale. `reopen` returns completion review to pending. A review retains its exact source/link basis; later source or document changes invalidate its currentness. Linking never rewrites or regenerates a document and never asserts that its content fulfils the requirement without the engineer's review.

An optional due date remains the stated calendar date. Dates earlier than the current UTC calendar day produce a visible warning. The date alone is not a compliance verdict or an export blocker.

## Final export basis

`submission_basis(tender_id, requirement_ids, output_ids)` returns `requirements`, `blocking_reasons`, `warnings` and `fingerprint`.

Every selected requirement must remain approved and source-current with a current engineer completion review. A satisfied requirement requires all documents included in that review to be selected for export and remain current. A reviewed exception becomes a warning containing the requirement title and recorded rationale, for explicit acknowledgement in the final export flow. Due-date warnings remain visible.

The complete selected requirement records, linked output identities and decision history participate in the fingerprint. Root must include this basis in the frozen export preview/manifest and recompute it inside export approval. An empty selection does not establish that the full tender requirement set has been identified or reviewed.

## Development-only handoff

No verification outcome is claimed. Root integration and later permitted verification remain separate from these implementation notes.
