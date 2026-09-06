# Shared implementation contracts

These are the agreed boundaries for independently owned modules. JSON properties use snake_case. IDs are opaque UUID hex strings; timestamps are UTC ISO strings. Decimal quantities/money cross the API as strings.

## Document worker (owned by document implementer)

`backend/quantix/documents.py` exports dataclasses:

```python
@dataclass
class Segment:
    locator: str
    text: str
    page: int | None = None
    sheet: str | None = None
    cell_range: str | None = None
    kind: str = "text"
    metadata: dict = field(default_factory=dict)

@dataclass
class Extraction:
    kind: str  # pdf, spreadsheet, word, cad, other
    status: str  # extracted, needs_attention, unsupported, failed
    segments: list[Segment] = field(default_factory=list)
    warnings: list[dict] = field(default_factory=list)  # code, message, locator(optional)
    metadata: dict = field(default_factory=dict)

def extract_document(path: Path, original_name: str, cancelled: Callable[[], bool] | None = None) -> Extraction: ...
def render_pdf_page(path: Path, page: int, max_width: int = 1600) -> bytes: ... # 1-based, PNG
```

Cancellation raises `InterruptedError`; unsupported and parse errors return explicit status. No database writes. Enforce reasonable file/page/row/output limits and report partial coverage. PDFium calls in extraction/render share a module lock so no concurrent calls occur. Preserve spreadsheet cell address, raw value, cached value, formula, number format, error and hidden row/sheet context in segment metadata. A row is one segment; metadata `cells` is a list of dictionaries with keys coordinate, value, cached_value, formula, number_format, data_type. Values must be JSON serialisable. Metadata identifies VBA presence without execution. DOCX locators use paragraph/table structure. Preserve source bytes. Never use PyMuPDF. Tests under backend/tests/test_documents.py; synthetic fixtures generated in temp directories.

## Local repository (owned by primary)

`Repository(home: Path)` in quantix/repository.py provides:

- `list_tenders() -> list[dict]`, `create_tender(name: str) -> dict`, `get_tender(tender_id) -> dict`.
- `overview(tender_id) -> dict`: tender, artifacts count, evidence count, coverage {registered,extracted,needs_attention,unsupported,failed}, areas list, findings list, current plan, active runs, boq_count.
- `list_artifacts(tender_id, current_only=True) -> list[dict]`: id,tender_id,relative_path,name,version,content_hash,size,kind,status,area,metadata,warnings,is_current,created_at.
- `search(tender_id, query, limit=20, *, area=None, status=None) -> list[dict]`: id,artifact_id,artifact_name,relative_path,locator,text,page,sheet,cell_range,kind,score. Filters apply before ranking/limits; only current versions participate.
- `get_evidence(tender_id,evidence_id) -> dict`, `artifact_evidence(tender_id,artifact_id,offset=0,limit=50) -> list[dict]`. Scope must be verified.
- `list_findings(tender_id) -> list[dict]`, `add_finding(tender_id, title, detail, kind, source_ids, origin='agent', run_id=None) -> dict`. Kinds requirement,risk,question,assumption,observation; state proposed/accepted/rejected/resolved. Source IDs checked in Tender.
- `messages(tender_id) -> list[dict]`, `add_message(tender_id,role,content,source_ids=None,run_id=None) -> dict`. Roles engineer,manager,system; rows id,content,source_ids,created_at.
- `create_plan(tender_id,title,tasks:list[dict],run_id=None) -> dict`: each task title,description,role,source_ids. Plan id,title,version,status(proposed/approved/superseded),tasks.
- `list_tasks(tender_id) -> list[dict]`; task id,plan_id,title,description,role,status,source_ids,result,run_id.
- `create_run(tender_id,kind,instruction='') -> dict`, `update_run(run_id,**fields)`, `get_run(run_id)`, `event(run_id,kind,message,data=None)`, `run_events(run_id)`. Run kind import/manager/task/research; status queued/running/completed/failed/cancelled/interrupted; progress integer 0..100, detail, result, error, usage, timestamps. Persist before work.

Errors: `KeyError` for absent/scoped-out objects; `ValueError` for invalid inputs/business decisions. Domain methods perform transactional writes.

`Repository.atomic()` is a synchronous context manager grouping nested repository operations into one SQLite transaction; use it around publishing an office result. Never await inside it. Its connection is thread-local and is not shared with background workers.

## AI worker (implementation after installed SDK inspection)

`quantix/office.py` exports async `run_manager(...)` and `run_specialist(...)`, returning `PreparedOfficeResult` from office_types.py. They use the official Agents SDK, Tender-scoped tools, `gpt-6-astra`/`xhigh`, bounded turns, no external tracing, and full validated engineer instructions/preferences. The SDK prepares output; it does not publish domain results. The job owner calls `publish_prepared(repo, prepared)` inside the same `repo.atomic()` transaction as terminal run/task state. Publication rechecks actually-read current sources and commercial proposal bases. No await occurs inside this transaction. Cancellation cannot leave a published reply with an unfinished job.

`PreparedOfficeResult` retains output, usage, read source IDs, consulted web URLs, captured item fingerprints and locally tracked recipient evidence. `OfficeOutput` includes proposed findings/plans plus optional `quote_drafts: DraftInput[]` and `unit_rate_proposals: UnitRateProposalInput[]`. Agents can propose but cannot approve sources, rates, quantities or supplier sends. `Repository.approved_scope(tender_id, plan_id)` supplies the immutable engineer scope/rationale to specialists. Historical evidence and full stored records remain inspectable with explicit version/state and bounded pagination.

## HTTP API

All `/api` requests require `Authorization: Bearer <local session token>`. Origin restricted to application/dev origins. Errors `{detail: string}`. API base `http://127.0.0.1:<port>/api`. Dates/decimals as above.

- GET `/health`: version,provider_ready,model,home (local display only).
- GET/POST `/tenders`; POST body `{name}`.
- GET `/tenders/{id}` returns overview.
- POST `/tenders/{id}/imports` body `{source_path}` returns run; POST accepts directory/zip only.
- GET `/tenders/{id}/artifacts?include_history=false`; GET `/tenders/{id}/artifacts/{artifact}/evidence?offset=0&limit=50`.
- GET `/tenders/{id}/artifacts/{artifact}/preview?page=1` PNG; GET `/tenders/{id}/evidence/{source_id}` evidence object.
- GET `/tenders/{id}/search?q=...&mode=words|meaning|combined&area=...&status=...` source hits. GET `search-status`; POST `search-index` starts a durable index run. Meaning-search unavailability is visible and never disguised as keyword results.
- GET `/tenders/{id}/messages`; POST body `{content}` returns manager run.
- GET `/tenders/{id}/findings`; POST `/tenders/{id}/findings/{finding}/decision` body `{decision:accept|reject|resolve,rationale}`.
- GET `/tenders/{id}/plans`; POST `/tenders/{id}/plans/{plan}/approve` body `{rationale}` starts approved tasks.
- GET `/tenders/{id}/tasks`; POST `/tenders/{id}/tasks/{task}/run` returns run (requires approved plan).
- GET `/tenders/{id}/runs`; GET `/runs/{id}`; GET `/runs/{id}/events`; POST `/runs/{id}/cancel`; POST `/runs/{id}/resume` only for safe resumable jobs.
- GET/PATCH `/settings` returns public connection status, model, default_currency and preferences. PATCH accepts api_key (write-only), model,default_currency; secrets go into credential store.
- Estimate/output routes are added by task 5 with generated types, never invented in the renderer.

Frontend connection uses Tauri invoke `connection_info` in desktop mode; Vite supplies API base/token for authorised local browser development. Native `choose_package` selects directory or zip through dialog; browser development accepts a source path in the import form.

## Estimates and draft outputs

`EstimateService(repo)` in `quantix/estimates.py` provides `refresh(tender_id) -> EstimateView` and `view(tender_id) -> EstimateView` dictionaries. Root calls refresh after successful imports. `quantix/estimate_routes.py:create_router(repo)` returns an APIRouter with `/api` prefix; root includes it under its existing bearer/origin enforcement and exception handlers. Typed schemas live in `estimate_models.py` and must be included when frontend types are generated.

- GET `/tenders/{id}/estimate` -> EstimateView: items, per-currency totals, complete, refresh_required, unpriced_count, unconfirmed_count, unknown_vat_count, unresolved_quantity_count, blocking_reasons, coverage_note.
- POST `/tenders/{id}/estimate/refresh` no body -> EstimateView (routine source-candidate refresh).
- PATCH `/tenders/{id}/estimate/items/{item_id}` body ItemUpdate -> EstimateItem.
- POST `/tenders/{id}/estimate/items/{item_id}/quantity-proposals` body QuantityRequest -> QuantityProposal.
- POST `/tenders/{id}/estimate/quantity-proposals/{proposal_id}/approve` body EngineerDecision -> EstimateView.
- GET `/tenders/{id}/outputs` -> list[OutputRecord].
- POST `/tenders/{id}/outputs` body OutputRequest -> OutputRecord.
- GET `/tenders/{id}/outputs/{output_id}/download` -> XLSX or DOCX attachment.

Estimate commercial decisions and output generation require `engineer_confirmed:true` and nonblank `rationale`; routine source refresh does not. Office agents have no access to these engineer mutation routes/tools.

ItemUpdate optional fields: `confirm_source`, `quantity_cell` (select an actual numeric source cell), `unit_rate` OR `components`, `currency` (ISO-style three capitals), `tax_basis` (`excluding_vat|including_vat|unknown`), `vat_percent` (0–100), `provenance`. Decimal values are strings with up to 12 integer and 6 fractional digits. A component is `{name,quantity,unit_rate,unit}`. Provenance is `{basis:observed|estimated,observed_on:YYYY-MM-DD,source_ids:[],urls:[],geography,conditions}`. Observed rates require a source ID or HTTP(S) URL; dates cannot be future. Human-provided URLs are recorded, not claimed independently verified. Explicit null clears a supplied rate/VAT field. Changing a direct rate clears its previous components.

EstimateItem preserves source_id, artifact_id, source document/sheet/locator, quantity/unit cell addresses, quantity_candidates, supplied_quantity, confirmed state and interpretation issues. effective_quantity uses supplied_quantity until an engineer approves a separate measurement. QuantityRequest adds `{quantity,calculation,source_ids}` to EngineerDecision; quantity proposals keep proposed/approved/superseded states. New source revisions never inherit previous rate or quantity approvals.

Totals distinguish `priced_subtotal_ex_vat` from nullable `total_ex_vat` and `total_inc_vat`. Missing rates, unconfirmed rows, ambiguous quantities or missing tax information block completeness. Values are grouped by currency and never silently combined. BOQ extraction is candidate identification and does not claim complete source coverage.

OutputRequest adds `kind:boq_xlsx|analysis_docx|technical_docx|registers_xlsx|comparison_xlsx|programme_xlsx` to EngineerDecision. Technical output requires `task_id` identifying a completed saved specialist result. Programme output requires an explicit `ConstructionProgramme` with dates, working weekdays, holidays, activity durations/dependencies, sources and assumptions. Whole working days and zero-lag finish-to-start links are supported; no resource levelling is claimed. OutputRecord includes id,tender_id,kind,filename,status(always draft),created_at,size,sha256,source_ids,pricing_complete,blocking_reasons,metadata. Files and matching JSON manifests live under `repo.home/outputs`; source workbooks are never edited. Excel Summary contains Decimal-calculated snapshot totals; Excel recomputes the auditable BOQ and component formulas when opened. A consolidated workbook is not described as a client-format priced copy.

Storage owned by the service: boq_items (active=1 for current candidates), estimate_state, quantity_proposals, generated_outputs; engineer actions also append the repository decisions ledger. Repository overview must count only active BOQ items. Imports invalidate estimate completeness until refresh.

## Meaning search

`SemanticService(repo)` stores its derived index separately under the application home. `status(tender_id)` reports fingerprints and coverage; `index(tender_id, cancelled=None, progress=None)` builds locally and preserves prior completed state on interruption. `search(tender_id, query, limit=20, *, area=None, status=None, collapse_duplicates=False)` returns original evidence with `metadata.semantic_match={start,end,text,model}`. The HTTP search collapses identical content-hash/locator occurrences before limiting; register/history and scoped service reads retain every occurrence. E5 model revision and extractor fingerprint are pinned; missing/stale indexes never silently become exact-word results.

## Commercial proposals

GET `/tenders/{id}/estimate/rate-proposals` returns `RateProposalRecord[]`. POST `/tenders/{id}/estimate/rate-proposals/{proposal_id}/approve` accepts `RateApproval` (engineer_confirmed, rationale, optional confirm_source=false). Immutable payload and item/source basis are rechecked at publication and approval. Rates never implicitly change quantity selection. See `estimate_models.py` and `docs/reports/office-business.md` for the precise shared types and read-only office tools.

## Supplier requests and replies

- GET/POST `/tenders/{id}/quotes`: list/create drafts using `DraftInput` (to,cc,subject,body,attachment_ids,source_ids).
- PATCH `/tenders/{id}/quotes/{quote_id}`: revise draft; prior approval no longer authorises changed content.
- GET `.../{quote_id}/preview` returns `QuotePreview`, including sender, exact content fingerprint, attachment byte hashes, currentness, SMTP readiness and restore_reconciliation_required.
- GET `.../{quote_id}/eml` downloads a mail draft; it does not send.
- POST `.../{quote_id}/approve` and POST `.../{quote_id}/send` accept `SendDecision` (fingerprint,engineer_confirmed,rationale,optional restore_reconciliation). The send route requires prior approval of that exact fingerprint. Agents cannot invoke these actions.
- GET/POST `.../{quote_id}/replies`: inspect/register reply records and their preserved source evidence. Receipt is not commercial acceptance.
- GET/PATCH `/mail/settings`: public account settings and write-only credentials. POST `/mail/sync` reads a bounded saved mailbox batch only when explicitly invoked.

Main-database quote status and append-only `mail-delivery.sqlite` history are distinct. Newer sending/sent/uncertain/partial history blocks automatic retry even when an older main database is restored. A restored draft lacking trustworthy newer history needs explicit engineer reconciliation. Credentials are stored in the OS credential store; they are never returned to the renderer or archive.

## Backups and native lifecycle

GET/POST `/backups` lists/creates verified archives; GET `/backups/{id}/download` downloads one. POST `/backups/inspect` accepts a local path and returns checked archive identity and coverage. POST `/backups/restore` binds `{path,expected_sha256,engineer_confirmed,rationale}` to the inspected archive. It stages a journal rather than replacing a live database. GET `/backups/pending` returns `RestoreReady|null`, allowing a full renderer reload to reconstruct the pending notice.

The CLI holds a single-workspace FileLock and calls `apply_pending_restore(home)` before creating any repository/API. Restores validate payload paths/hashes, preserve a before-restore snapshot and install the database last. Newer delivery history is not rolled back. POST `/shutdown` requests owned-service shutdown; Tauri `close_quantix` closes the native shell. Imported originals are never edited by restoration or output generation.

## Approved reusable knowledge

`KnowledgeService(repo)` owns `knowledge.py`, `knowledge_models.py`, `knowledge_routes.py`; root integrates router and office retrieval. Local records are explicitly engineer-approved reusable notes, not imported Tender facts. Categories: preference, method, reference, price, tax. Creation requires title, content, category, engineer_confirmed:true, rationale; optional source_tender_id/source_ids identify the original supporting Tender and must resolve there. Optional verified_on/recheck_after dates retain their meaning; price/tax always require fresh validation before commercial use regardless of dates. No automatic cross-Tender fact promotion.

GET/POST `/knowledge`, GET `/knowledge/{id}`, POST `/knowledge/{id}/withdraw` (engineer decision) are typed. Notes retain immutable approved text, category, provenance source hashes/version and approval rationale; withdrawal changes state with a recorded decision. Source revisions can mark a note needs_recheck without rewriting approved text. List returns state/currentness/revalidation flags explicitly. Reusable knowledge is inside the workspace database/backups; provider keys and private source content remain excluded from source control. Frontend and model-read tool must label these as approved reusable notes, never as current Tender evidence or independently current prices/tax rules.

## Calibrated drawing measurements

PDF measurements remain proposals, separate from supplied BOQ quantities. A measurement stores the exact Tender artifact/version/hash/page; two calibration points and their known printed length in metres; normalized top-left page coordinates; mode length/area/count; shape points; calculation/quantity/unit; author origin and scope label. Authoritative PDF page dimensions must be read from the preserved original so unequal page width/height does not distort geometry. Count needs no length calibration. Invalid, degenerate or self-intersecting shapes are rejected. These records never imply automated object recognition or a checked entire-drawing takeoff.

The interface must let the engineer inspect the original, calibrate against a stated dimension, measure and review the result. Stored measurements preserve their source/calibration and become stale on source revision. Linking a measurement to a BOQ item creates a separate quantity proposal; supplied quantities stay unchanged until explicit engineer approval. Full-project takeoff may collect many scoped measurements and specialist proposals; there is no fixed restriction on the Tender work scope.


## Original-file inspection

GET `/tenders/{id}/artifacts/{artifact_id}` returns the exact version's Artifact, including historical versions. GET `.../{artifact_id}/original` returns a downloaded copy of hash-verified original bytes. Both routes require the same session authentication and Tender scope as source search. Download does not execute macros or open a native application automatically.

## Measurement service

`MeasurementService(repo)` exposes page, calculate, create, list, get and link. `measurement_routes.create_router(repo)` provides GET `/tenders/{id}/artifacts/{artifact_id}/measurement-page?page=1`, POST `/tenders/{id}/measurements/calculate`, GET/POST `/tenders/{id}/measurements`, GET `.../{measurement_id}`, and POST `.../{measurement_id}/link`. API schemas are MeasurementInput/Create/Page/Calculation/Record/Link. Derived evidence retains `kind=measurement`, engineer origin, proposal status and original version/hash; these labels reach office source tools. Link is idempotent per measurement/item and creates a separately approved quantity proposal. An explicitly approved current measurement can establish pricing quantity where a source BOQ formula is broken; the unresolved supplied quantity and warning remain unchanged and source confirmation is still explicit.

## Reviewed local submission exports

`SubmissionService(repo)` and `submission_routes.create_router(repo)` freeze selected existing drafts into a local ZIP. GET `/tenders/{id}/submissions`, POST `.../submissions/preview` with output_ids, POST `.../submissions` with SubmissionApproval, and GET `.../submissions/{id}/download` are typed. Approval binds exact preview fingerprint, checked files/source/working-record bases, final_review_confirmed, acknowledged_scope, every displayed gap and rationale. Approval never transmits externally or certifies full Tender compliance. Files remain unchanged and their frozen manifest records approval scope. Workspace backups include all supported draft formats and frozen submission ZIPs/manifests.

GET `/backups/latest` returns `RestoreOutcome|null` with recorded completion time, previous intact backup ID or damaged-state recovery-capture path and detail. Completion must be recorded, never inferred from file timestamps. Damaged-source recovery captures preserve available data with actual/expected hashes and are explicitly not restorable backups.

## Project map and review coverage

`ProjectMapService(repo)` owns durable source-linked nodes and explicitly scoped engineer review records. GET `/tenders/{id}/project-map` returns nodes,review_scopes,coverage; POST `.../nodes` accepts kind (building|area|discipline|work_item|requirement),title,detail,source_ids and optional parent_id; POST `.../nodes/{node}/decision` accepts approve|withdraw,engineer_confirmed:true,rationale. Node content/parent is immutable; approval history and source currentness are separate. Nodes proposed by the Manager can be published only within the owning job's atomic finalization. Their source IDs must have been read in that run. Nodes never grant commercial authority.

POST `.../reviews` records artifact_id,scope_type (artifact|page|locator),scope_label,optional page/locator,engineer_confirmed:true,rationale,whole_document_reviewed. A whole-document record requires whole_document_reviewed:true; page/locator records forbid it. Currentness follows the exact original/source version. Coverage distinguishes registered files, extracted files/evidence, evidence cited in findings, current explicit review scopes, and documents reviewed in full. Directory areas stay identified as imported structure rather than approved project entities.


## MVP office completion

OfficeOutput now includes project_map_nodes, submission_requirements, drawing_measurements, quantity_proposals, programme_proposal and draft_documents. All local source references must be read in the run and current at publication. PreparedOfficeResult retains approved_plan_id; publication rechecks the original approved scope. A specialist's consultation is read-only and the owning run publishes its selected structured results.

AgentMeasurementProposal retains geometry, scope and supporting source IDs. office_measurement validates that the actual drawing regions were viewed and the page/dimension sources were read; MeasurementService.propose_agent saves origin=agent and null engineer review fields. Engineer-marked records retain their real review. BOQ linking and pricing-quantity approval remain separate. AgentQuantityProposal supports sourced volume/group arithmetic; its read item basis and source references are checked before publication, and its quantity-specific fingerprint is checked again before approval.

DraftDocumentProposal contains only one of the six routine draft kinds, task_id and programme; it has no engineer approval fields. OutputService.generate_from_office(tender_id,proposal,run_id,plan_id) uses an existing approved plan. The job publishes its task/run results before generating drafts in the same transaction; generated_outputs are added only to the run so a technical document's saved task result does not change. Uncommitted draft files are removed if finalization fails. Client-format copies are excluded from this automatic path.

GET /tenders/{id}/programme-proposals returns ProgrammeProposalRecord[] with saved programme, originating run, sources and currentness. POST /tenders/{id}/work/stop requests cancellation of all active Tender runs. After all tasks in an approved plan complete, the Manager consolidates their saved results within that scope.

## Client-format BOQ copies

OutputRequest.kind=client_boq requires ClientBoqInput: artifact_id,mappings,currency,tax_basis,mapping_reviewed:true,quantity_mapping_approved. Mappings specify sheet,source_ids of actual column headers,item_ids,rate_column,amount_column,optional quantity_column. Quantities are preserved by default. Writing measured quantities needs separately approved measurements plus explicit mapping consent. Copies retain XLSX/XLSM format and original OOXML members/VBA; only mapped cells, necessary dimension bounds and recalculation settings change. Unrelated source formulas remain as supplied. Metadata retains prior/updated cells, exact original versions and incomplete-scope/recalculation notes. Original files are never rewritten.

## Submission requirement register

RequirementService owns source-backed immutable requirements and append-only requirement_events. GET/POST /tenders/{id}/requirements list/propose; GET /requirements/{id} reads a record; POST /{id}/decision records approve|withdraw|satisfied|exception|reopen with EngineerDecision; POST /{id}/outputs links a real generated output; POST /{id}/outputs/{output_id}/unlink removes that link with a recorded decision. Origin is engineer or manager. Sources, document hashes and completion-review bases stay separate. Due dates warn when past but do not alone establish noncompliance.

SubmissionSelection accepts requirement_ids:null (default: all active registered requirements) or an explicit list for a scoped export. SubmissionPreview/SubmissionRecord include the selected requirements and IDs. RequirementService.submission_basis checks approval, current sources, reviewed satisfaction or explicit exception, current linked outputs and their inclusion in the selected package. Omitted requirements and reviewed exceptions are warnings requiring explicit acknowledgement in the final fingerprint-bound decision.
