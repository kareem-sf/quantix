# Shared implementation contracts

These are the agreed boundaries between modules. JSON properties use snake_case. IDs are opaque UUID hex strings; timestamps are UTC ISO strings. Decimal quantities and money cross the API as strings. Pydantic models own every API shape; `npm run bindings` regenerates `src/bindings/api.ts`, and schemas and generated declarations change together.

## Tender Manager and team — 14 September 2026

Design: [tender team runtime](design/tender-team-runtime.md).

**Runs.** Every engineer message queues one `manager` run (`JobManager.submit_message`); there is no separate routing pass. `office.run_manager(repo, tender_id, run_id, instruction)` runs up to `MAX_MANAGER_TURNS = 6` Manager turns in that run, under the Tender's AI route, request allowance and budget meter, and returns a `PreparedOfficeResult`. The job owner calls `publish_prepared(repo, prepared)` inside the same `repo.atomic()` transaction as the terminal run state. No `await` occurs inside authority or publication transactions.

**Answer and proposals.** The model returns `ManagerAnswer {summary, source_ids, findings}`. Every other record is staged during the run with the `propose(kind, items, replace=false)` tool; `proposal_format(kind)` returns that kind's item schema and rules. Kinds: `plan`, `takeoff`, `boq_item_proposals`, `quantity_proposals`, `unit_rate_proposals`, `price_proposals`, `web_findings`, `quote_drafts`, `submission_requirements`, `project_map_nodes`, `programme_proposal`, `draft_documents`. List kinds append unless `replace` is true; `plan` and `programme_proposal` always replace. Staged records live in `OfficeContext.proposals`. `propose` validates structure and evidence when called (`office.validate_proposals` with web sources deferred); `compose(answer, context)` builds the internal `OfficeOutput = ManagerAnswer + OfficeProposals`, and the full checks, including web-search URLs and the working-brief freshness rule, run again in the model loop and in `prepare_result`/`validate_prepared`. A local client's closing prose without a submitted result becomes a summary-only `ManagerAnswer`.

**Evidence.** Cite only evidence read with a tool in the same run (`OfficeContext.seen_sources`). A staff member's citations are not the Manager's reading. BOQ-linked proposals require the item basis captured by `inspect_estimate` in the run (`OfficeContext.item_bases`).

**Team.** `TeamService` owns `team_staff` and `team_assignments`. The Manager's `team_tools(repo, tender_id, run_id)` are `list_team` (staff plus `available_models`), `hire_staff` (a generated `StaffDraft {name, role, specialisms, background, working_style}`; duplicate active names are refused), `assign_work(staff_id, title, brief, expected_result, source_ids?, model?)`, `read_assignment` and `answer_staff`. Staff contexts cannot call them. Assignments are `queued|running|waiting|completed|failed|cancelled`. After each Manager turn, `team_runtime.run_queued` runs the run's queued assignments: up to three at once on API accounts, one at a time per subscription account. Each assignment is one staff turn on its stored connection/model with `STAFF_INSTRUCTIONS`, the source, calculation and record tools, and `StaffOutput {kind: completed|question, summary, findings, source_ids, question}`. Outcomes reach the next Manager turn as `team_updates_not_source_inspection` (`outcome_view`). Stop cancels open assignments; startup marks queued/running assignments cancelled.

**Staff records.** Staff may stage `takeoff`, `boq_item_proposals`, `quantity_proposals`, `submission_requirements` and `project_map_nodes`. `save_staff_records` validates and saves them in one transaction when the assignment completes and stores the counts in `AssignmentResult.saved_records`. A failed save fails that assignment. A question saves nothing.

**Steering.** `POST /api/tenders/{tid}/runs/{run_id}/steering` accepts `InstructionRevisionRequest {kind: question|constraint|replace|urgent|cancel, content, selection, idempotency_key}` and returns `InstructionAdmission`. Admitted text joins the next Manager turn with a never-rewrite-published-results guard; `cancel` stops the run.

**Team API.** `GET /api/tenders/{tid}/team` returns `TeamView {staff, assignments}` (newest 200 assignments). `GET .../team/assignments/{id}` returns one `Assignment`. `POST .../team/staff/{id}/retire` retires a colleague.

## Quantity takeoff — 14 September 2026

`TakeoffLineProposal {description, location, unit, quantity?, method?: dimensions|schedule|scaled|counted, working, source_ids, boq_item_id?}`. A line needs a quantity, or a `boq_item_id` for a BOQ item not found on the drawings; a quantity needs a method. `TakeoffService.validate` requires the sources to have been read and a matched BOQ item to have been read with `inspect_estimate` in the run.

`takeoff.compare` computes the comparison; the model never supplies it. Units are normalised (for example m³, cum and m3). Results: `not_in_boq` (no item), `not_on_drawings` (item, no quantity), `unit_differs`, `no_boq_quantity`, `matches` (within 2% of the supplied BOQ quantity) or `differs`, with `difference` and `difference_percent`. Saved lines (`takeoff_lines`) keep the BOQ snapshot, author, assignment and the item's quantity fingerprint; `is_current` is false when a cited source revision or the BOQ item basis changed.

`GET /api/tenders/{tid}/takeoff` returns `TakeoffLine[]`, newest first. `POST .../takeoff/{line_id}/review` accepts `TakeoffReview {decision: accepted|rejected, note}` and appends a `takeoff_line` decision. Review changes no BOQ quantity. The Manager lists lines with `inspect_tender_records(record_type="takeoff")`; `trace_change_impact` includes takeoff lines.

`calculate_drawing_measurement` scales a length, area or count from viewed drawing regions using the original PDF page size and a printed calibration; nothing is stored. `GET /api/tenders/{tid}/artifacts/{artifact_id}/measurement-page?page=` returns `MeasurementPage` (page size and count) for the document viewer.

## Package analysis and Tender authority — 13 September 2026

Creating a Tender grants no AI destination, route, paid allowance or provider-managed extras. The engineer selects and confirms AI for that Tender through its AI setup. Import can complete local analysis while AI mapping waits for this decision.

Automatic identity results are proposals, retained in the run result/package map and labelled for source review. They can supply a provisional AI name but cannot update engineer-maintained Tender profile fields or create engineer decisions. Staff read the same package map as the Manager.

Document brief cache version 2 binds reuse to the Tender, full current artifact metadata, document IDs/paths and every opening excerpt supplied to the model. Relationships must name another document in that model batch. Identical file bytes in another Tender or changed extraction context require a fresh brief. Older content-only cache entries remain stored but are not reused.

## Source BOQ rows, requirements and links — 13 September 2026

`SourceBoqProposal.replaces_item_id` is optional for a new row and names the exact earlier source-BOQ row when proposing its replacement. It is preserved in the immutable proposal basis and is never inferred from a repeated row label.

`EstimateView.retired_source_rows` keeps earlier source-BOQ rows visible after revisions. They block complete totals until a source proposal explicitly naming `replaces_item_id` is confirmed, or the engineer records that the old row is no longer required. Replacements must use a newer revision of the same source file and identify the exact old row, so repeated row labels on separate pages cannot collapse. Confirmed replacement chains transfer pending scope to their latest unresolved row. `POST /tenders/{tid}/estimate/source-rows/{id}/exclude` takes `SourceRowExclusion {engineer_confirmed:true,rationale,current_artifact_id}`; the last field must match the revision displayed during review. The immutable `boq_source_exclusions` basis joins the immutable decision ledger, so a later file revision requires renewed review. Old originals and BOQ records remain preserved.

Source BOQ proposals use `POST /api/tenders/{tid}/estimate/source-rows` with `SourceBoqProposal {source_id,row_reference,source_excerpt,description,unit,quantity}`. An exact current preserved passage must contain the supplied excerpt, quantity and unit. The operation creates an **unconfirmed** `EstimateItem`; existing explicit source, quantity, price, tax and release decisions remain authoritative. The `boq_item_proposals` proposal kind publishes the same unconfirmed rows from inspected evidence. Saved identity is source ID plus row reference, so one page may support multiple rows; altered retries conflict. Source proposals remain separate from calculated quantity proposals. `EstimateItem` adds optional original proposal/excerpt/reference/origin/run fields. Normal refresh retains current source proposals and retires rows whose original source revision is superseded.

Database schema version 2 replaces the obsolete unique `(tender_id,source_id)` BOQ key with `(tender_id,source_id,row_key)`; Excel rows retain an empty key and every prior item ID and foreign-key relationship survives. The versioned startup migration snapshots the database, commits atomically after foreign-key validation and restores on failure. Backup schema 1–4 are readable; a new manifest records the actual database schema and inspection verifies it.

`RequirementProposal` adds `source_quote`, `applicability` (`unconditional|conditional|unknown`), `condition`, and `exceptions`. New Manager proposals must quote an actual inspected clause and explicitly preserve known conditions/exceptions. The lexical guard includes the enclosing source line to catch omitted prefixes; it is not a guarantee of complete semantic interpretation. Old payloads remain immutable and project unknown applicability with review warnings. `RequirementDecision.applicability_reviewed` is explicit non-autosaved engineer acknowledgement for unknown/conditional/exception-qualified requirements. Its receipt is retained in the decision ledger; unreviewed qualification blocks final package review.

`ResultLink.kind` adds `requirement`, `boq_item`, `work_product`, and `calculation`. Links join exact owning-run records/events to existing same-Tender records; IDs in generated prose do not establish links.

`WorkBrief.progress_current` and `latest_work_run_id` are derived read-only projections. Later completed material work makes an older brief explicitly out of date without rewriting its steps or inventing completion. Material Manager work requires a newly saved brief before publication; greetings and plain assignments do not. A missing update is a correctable model error.

Analysis DOCX uses the latest Manager answer that cites sources from a completed `manager` run. Replies without sources and failed runs do not replace the analysis. No attributable analysis yields an explicit note.

## Conversation, pending instructions and plans

- GET `/tenders/{id}/messages?limit=50` returns `MessagePage {items,next_cursor}` with the newest bounded engineer/Manager dialogue in chronological order. The opaque cursor fetches strictly older rows and is scoped to its Tender. `Message.result_links` are derived only from exact persisted run relations.
- POST `/tenders/{id}/messages` accepts `MessageRequest {content,idempotency_key?,action?:review_documents}` and returns `MessageSubmission {outcome:immediate,run}` or `{outcome:pending,pending}`.
- GET `/tenders/{id}/pending-message` returns the one current `PendingInstruction|null`. PATCH accepts `{pending_id,expected_revision,content,action?}`; DELETE accepts `{pending_id,expected_revision}`. POST `/pending-message/confirm` sends held text after fresh checks. Submission receipt, edits and consume are atomic; readiness, credentials, route permission and spending are rechecked before consume. Failed/cancelled/interrupted work holds the waiting instruction. Startup and restore never start it automatically.
- GET `/tenders/{id}/plans` lists plans. POST `/tenders/{id}/plans/{plan}/approve` accepts `ApprovalRequest {rationale}`, approves the proposed plan and queues one Manager run that carries it out with the team. POST `/tenders/{id}/tasks/{task}/run` asks the Manager to carry out one approved task.
- Resume of a stopped `manager` run queues a new `manager` run with the same instruction, without duplicating the engineer message or its receipt.

## Source and submission navigation

GET `/tenders/{id}/artifacts/{artifact}/evidence` adds optional `sheet` (exact worksheet name) and `cell_range` (bounded A1 range). Existing offset/limit apply after filtering; response remains `Evidence[]`. Range intersection returns original saved source rows rather than fabricating clipped cells.

`SubmissionPreview.blockers` defaults to an empty list and supplements preserved `blocking_reasons`. Each blocker includes `code`, `message`, and `target {kind:requirement|output|package,record_id,output_ids}`.

HashRouter routes use `/tenders/{id}/{manager|documents|work|estimate|submission}` with `view`, `record`, and validated local `origin`/Settings `return` context. The Estimate section views are `boq`, `takeoff`, `proposals` and `quotes`. Source URLs retain artifact ID/version/hash/page/sheet/range. Nonsecret form drafts are scoped by Tender/form/version. Credentials and final/quantity/export consent controls are excluded from autosave.

The Manager screen's right workspace (`RightWorkspace`, `useWorkspaceState`) has a home launcher, visited Documents/Team/Reviews/Activity tabs and split/hidden/expanded modes. Only the layout preference is persisted. Each Tender keys its workbench instance so transient records cannot carry into a different Tender.

## Manager activity and send availability

The Manager composer reflects the single Tender work lane: any queued/running import, search-index or Manager run makes a new message wait as the pending instruction, while its draft remains editable. The server remains authoritative for races.

Trusted source-tool and worker lifecycle events may update `Run.detail` through `Repository.update_active_run_detail(run_id, detail)`. Updates are atomic and apply only to queued/running runs; they never change terminal status, invent percentage completion or claim reviewed coverage. Provider failures retain only allowlisted categories/status codes and fixed messages.

## Complete execution monitoring — 13 September 2026

`ActivityRecorder` appends immutable `kind=activity` run events with server-created operation, parent, actor/assignment, category, phase, timestamps, provider/model and call correlation. Full permitted JSON lives in `run_activity_payloads` in the same transaction; `run_activity_operations` retains operation identity. Payloads remain for the Tender lifetime. No private content enters source control or generic diagnostic logs.

The shared tool dispatcher (`tool_policy.dispatch`) records preparation, invocation, result or failure for direct, MCP and nested tools. A tool result returned after Stop remains inspectable but is not delivered to the model. A read-only call with identical arguments may run twice per Manager turn; further identical calls return a correctable refusal until a mutating tool succeeds. `search_sources` stops once, with a correctable message, after three consecutive searches that return only passages already shown.

- `GET /api/tenders/{tid}/runs/{rid}/activity?after=&before=&limit=&q=&actor_id=&category=&errors_only=` returns `RunActivityPage`. Cursors bind run, event ID and content fingerprint.
- `GET /api/tenders/{tid}/runs/{rid}/activity/{event_id}?offset=&limit=` returns `RunActivityDetail` with paginated payload characters and redaction/unavailable fields.
- `GET /api/tenders/{tid}/runs/{rid}/chat-stream` observes events with AI SDK UI v7-compatible SSE. It cannot submit, retry, approve or cancel. Observer disconnect leaves work running.

Model request capture retains the content Quantix controls, effective settings/tool definitions, public output and available usage. Account internals, authentication headers, credentials, reasoning signatures and encrypted blocks are excluded before persistence. Native request capture is labelled `Content supplied by Quantix` and does not claim to expose an original client's entire internal prompt.

## Manager working brief — 13 September 2026

The Tender Manager keeps one versioned working brief per Tender so multi-step work continues across turns. It is a progress record, never source evidence or an approval.

`WorkBrief` holds `outcome`, `status` (`in_progress` | `waiting_for_engineer` | `complete`), `next_step`, `done_when`, `steps` (`title`, `state`: `to_do` | `in_progress` | `done` | `blocked`, `note`), `settled` points (`text`, `source_ids`), `open_questions` (`text`, `owner`: `engineer` | `manager` | `colleague`, `affects`) and `work_product_ids`, plus server fields. The serialized brief is capped at 12,000 characters.

`save_work_brief` is a Manager-only mutating tool under a trusted invocation ID. Each save replaces the whole brief as the next version and requires `expected_version` (0 when none exists). A settled point may cite a source read in the current run or one the previous version already carried and that is still current. Work-product IDs must exist in the Tender. A revised source marks the brief `needs_review`. The Manager prompt carries `manager_work_brief`. GET `/api/tenders/{tender_id}/work-brief` returns `WorkBriefState {brief: WorkBrief | null}`; the Team tab shows it as "Current work".

Every tool's argument-schema failure is a `ToolArgumentError` naming the failing fields without echoing their values, so the model corrects the call. An argument naming a server-owned identity (`tender_id`, `run_id`, actor or invocation fields) remains a refusal.

## Evidence, revision and package checks — 13 September 2026

These read-only tools never mark a source as read for citation, approve, export or send. Manager and staff can use all of them.

- `inspect_extraction_coverage(offset, limit, exceptions_only=true)` returns Tender totals and per-document status, page and segment counts, pages without text, OCR pages and warnings. Extracted text is not analysed or reviewed coverage.
- `compare_source_versions(artifact_id, previous_version=null, offset, limit)` compares two versions of the same relative path by locator. Graphic-only drawing changes are not detected.
- `trace_change_impact(artifact_id)` lists records that still reference earlier versions of the same file: findings, tasks, BOQ rows, quantity and rate proposals, takeoff lines, submission requirements, project map items, generated documents, work products and working briefs.
- `check_estimate_coverage(offset, limit)` summarises unpriced, unconfirmed, unknown-VAT and unresolved-quantity rows, possible duplicates, pending proposals and completeness.
- `rehearse_submission(output_ids=null)` runs the submission preview without exporting.

**Codex boundary.** Codex's built-in `list_mcp_resources` and `list_mcp_resource_templates` return names only. Any other MCP tool outside the `quantix` server stops the run and names the tool and server.

## Work products and calculations

Work products, the Tender profile/calendar, calculations and company assets are versioned Tender-scoped records. `GET/PATCH /api/tenders/{tid}/profile`, `POST /api/tenders/{tid}/calendar`, `GET/POST /api/tenders/{tid}/work-products`, `GET .../work-products/{id}/versions[/{version}[/rows]]`, `POST /api/tenders/{tid}/calculations`, `GET .../calculations/{id}` and `POST /api/company-assets` use the authenticated session. Views cannot mint approvals.

`calculate_engineering`, `check_engineering_calculation` and `save_work_product` save attributable drafts under trusted invocation IDs, rechecking active work inside `authority_guard` plus the database transaction. Decimal/Pint calculations retain inputs, units, method fingerprint, assumptions and rounding. `save_work_product` creates a new product by default; revising needs both `product_id` and `expected_version`. Work products cite tender evidence IDs read in the run; web URLs are refused. Source-version dependencies mark affected records `needs_review` without rewriting them.

## Factory reset (2026-09-09)

[Factory reset](design/factory-reset.md) defines a deliberate Settings **Reset and close** action. It erases the owned application home and its home-scoped protected credentials. It never accepts a deletion path from the renderer, erases original/export locations outside the home, or silently retains an external backup.

- GET `/reset/preview` returns `ResetPreview {supported,home,tender_count,artifact_count,account_count,backup_count,blockers,fingerprint}`.
- GET `/reset/status` returns `ResetStatus|null` with `reset_id,state,detail,credentials_cleared,fingerprint`.
- POST `/reset` accepts `ResetRequest {fingerprint,confirmation:"RESET"}` and returns `ResetReceipt {reset_id,state,detail}`. Repeating an accepted fingerprint returns its original receipt. A stale preview or active work blocks confirmation.
- Health advertises `factory_reset` only with a supported cleanup adapter and adds `reset_pending`. The native `reset_support` command must independently succeed before the renderer can confirm reset.
- Once confirmation is durably admitted, domain requests are blocked. Health/reset/diagnostic-status/shutdown remain available.

`pending-reset.json` (format 1) retains the owned home, reset ID, fingerprint, confirmation time, credential-target metadata, credentials-cleared state and a fixed public detail. Public `state` is `cleaning_credentials|credential_error|ready|deleting|failed`. Native `finish_reset(reset_id)` accepts only the matching normal-home journal with cleared credentials. An early native helper deletes without following links and removes the journal only after complete cleanup. Startup recovery precedes opening home logs or desktop WebView files.

## AI connections

**Supported routes.** Direct API keys for OpenAI, Anthropic, Google, xAI and one OpenAI-compatible custom endpoint, through bundled SDKs. Subscriptions only through the official ChatGPT/Codex and Grok clients, per [subscription connections](subscription-connections.md). SetupMethod and SetupAccount carry `access_kind` (`api_key|subscription`); SetupService adds nullable `subscription_note` and `subscription_docs_url`. SetupCheckPreview adds `subscription_check=false` and nullable `limit_description`, and numeric request/input/output limits are nullable for subscription clients. Unsupported historical records return `SetupAccount.supported=false` and cannot install, sign in, discover, check or run. `SetupCheckPreview` adds `requires_unknown_cost_consent` and `max_input_tokens`; `SetupAction` adds `accept_unknown_cost`. An unpriced generic test requires fingerprint-bound consent and a two-request/1024-output limit.

**Connections.** `AIConnectionService(repo)` provides presets, list/get/create/update/delete, private `credentials(id)`, `models(id)`, `save_model`, `discover`, `mark_used` and `mark_error`. A new root begins with no connection profiles; constructors never recreate accounts from legacy keys or ambient environment variables. Secrets stay in the OS keyring or explicit session-only memory, never in SQLite or the renderer. Root API: GET `/ai/providers`; GET/POST `/ai/connections`; GET/PUT/DELETE `/ai/connections/{id}`; GET/POST `/ai/connections/{id}/models`; POST `/ai/connections/{id}/discover`; runtime actions GET `.../runtime`, POST `.../install`, `.../login`, `.../logout`.

**Tender policy and spending.** GET/PUT `/tenders/{id}/ai-policy` uses `TenderAIInput/Record`: allowed connections, Manager and specialist routes and positive budgets for metered routes. `AIPolicyService.routes_for(tender_id, role=None)` is read-only. `BudgetMeter.before_request(input_allowance, output_limit, requests=1)` returns a usage-record ID; `on_response(usage, reservation_id)` keeps the reservation when usage is incomplete. GET `/tenders/{id}/ai-usage` returns records; POST `.../ai-usage/{id}/reconcile` requires `AIReconcile` with engineer confirmation and stopped work. `authority_guard()` is acquired before an outer SQLite transaction that reads or changes AI authority and is never held across provider execution. A detected restore pauses metered requests until an explicit budget review.

**Execution.** `async execute_api(route, connection, credentials, context, instruction, output_type, *, definitions, system_instructions, before_request, on_response, validate_output) -> {output, usage, web_sources}`. Direct APIs use Pydantic AI with provider SDKs; Codex and Grok run in an isolated worker behind a scoped MCP bridge exposing only Quantix tools and `quantix_submit_result`. `ai_turn.run_turn` leases the account, checks readiness, meters every request and redacts credentials from errors. Hosted web search is the only native tool.

**Guided setup.** Public contracts are in `ai_setup_models.py`: GET `/ai/setup/services`; GET/POST `/ai/setup/accounts`; GET `/ai/setup/accounts/{id}`; PATCH `.../configure`; POST `.../actions`; GET `.../check-preview`; GET `.../checks`; POST `/tenders/{id}/ai-setup` with `SimpleTenderAIInput`. GET/POST `/tenders/{id}/ai-thinking` reads and changes the Manager's reasoning level while no work runs. Readiness is per connection/model in `ai_setup_model_checks` and bound to the checked connection revision and software version; execution requires the requested model's current proof. `AIComponentService` owns private versioned software and leases; `AIWorkerClient` owns the MCP worker sessions. A changed worker fingerprint needs Prepare and Check again.

**Grok subscription.** Protocol `grok_build` uses `client_login` and subscription billing. Component `client-grok` pins Grok 1.0.13 behind standard MCP. `connection.settings.allow_provider_managed_extras` is a strict boolean, default false, changed only with the `set_subscription_extras` action. `sign_in_method=browser|device_code` is optional on sign-in. `SubscriptionUsage` is nullable on SetupAccount/RuntimeStatus; account-wide usage is never attributed as exact Tender spending. Fresh included-only evidence is required before each check and before execution. Every active Grok route using extras needs a matching `provider_managed_extras` approval for the account revision. Grok serializes its per-account configuration through an OS lock.

**Native browser handoff.** `open_external_url` and `prepare_sign_in_browser` accept an HTTP(S) URL without embedded credentials and open it through the OS browser association. They expose no arbitrary executable or file-opening capability.

## Unified application storage and startup

The normal application root is `~/.quantix`; see [unified storage](unified-storage.md). Database, domain directories, private AI components/accounts, caches, logs, scratch work and supported WebView state belong to that root. Development and native service discovery use `<root>/runtime/connection.json`; schema generation uses `<root>/runtime/openapi.json`.

The `connection_info` IPC payload is `{base_url, token}`. The backend is the sole publisher of `connection.json`: write a complete private temporary file, flush it and replace the record, with bounded retries for Windows sharing conflicts. Frontend `connect(signal?)` resolves only after native discovery and an authenticated, timeout-bound GET `/health`. Startup retries are reads only and never replay a write, approval, message or AI request.

## Local diagnostics (9 September 2026)

Diagnostic files live in `<application root>/logs`; workers receive this path explicitly. Each process uses a rotating JSONL file (5 MiB, two backups); retention is 14 days / about 100 MiB. Logging failure is non-fatal and visible in diagnostic status.

Authenticated GET `/api/diagnostics` returns `DiagnosticsStatus`. Authenticated POST `/api/diagnostics/events` accepts `DiagnosticEventInput` with a fixed event/error-type vocabulary, bounded line/column and an optional 32-character request ID, and returns `DiagnosticEventAck`. Extra properties are forbidden and renderer events are limited to 60 per minute. API responses carry `X-Quantix-Request-Id`. Credentials, query strings, file names/content, messages, provider bodies, raw stderr and auth URLs never enter diagnostics.

## Document worker

`backend/quantix/documents.py` exports:

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

Cancellation raises `InterruptedError`; unsupported and parse errors return explicit status. No database writes. PDFium calls share a module lock. Spreadsheet rows are one segment each, with `cells` metadata (coordinate, value, cached_value, formula, number_format, data_type). VBA presence is identified without execution. Never use PyMuPDF.

## Local repository

`Repository(home: Path)` in `quantix/repository.py` provides tenders, overview, artifacts (with versions and hashes), search, evidence, findings, messages, plans, tasks, runs and run events. Run kinds are `import`, `manager`, `index`, `identify` and `analysis`; status `queued|running|completed|failed|cancelled|interrupted`. Errors: `KeyError` for absent or scoped-out objects; `ValueError` for invalid inputs or business decisions. `Repository.atomic()` groups nested operations into one SQLite transaction; never await inside it. Its connection is thread-local.

## HTTP API conventions

All `/api` requests require `Authorization: Bearer <local session token>`. Origin is restricted to application and development origins. Errors are `{detail: string}`. API responses default to `Cache-Control: no-store`.

- GET `/health`: version, provider readiness, `workspace_revision`, `ai_setup_revision`, `reset_pending` and `capabilities` (including `team`, `takeoff`, `estimates`, `outputs`, `quotations`, `knowledge`, `submissions`, `project_map`, `work_products`, `meaning_search`).
- GET/POST `/tenders`; GET `/tenders/{id}` returns the overview.
- POST `/tenders/{id}/imports` with `{source_path}` returns a run.
- GET `/tenders/{id}/artifacts`; GET `.../artifacts/{artifact}`; GET `.../artifacts/{artifact}/evidence`; GET `.../artifacts/{artifact}/preview?page=1` (PNG); GET `.../artifacts/{artifact}/original` (hash-verified download); GET `/tenders/{id}/evidence/{source_id}`.
- POST `/tenders/{id}/extractions/reprocess` with `ReprocessRequest` returns `ReprocessResult`; the original file hash is unchanged and publication advances `retrieval_generation`.
- GET `/tenders/{id}/search?q=&mode=auto|words|meaning|combined&...` returns `RetrievalResponse`. Strict `meaning` and `combined` refuse an unavailable, preparing or stale index with HTTP 409. GET `search-status`, POST `search-index`, POST `search-index/cancel`. The agent tool `search_sources` uses the same `retrieve()` path.
- GET `/tenders/{id}/findings`; POST `.../findings/{finding}/decision` with `{decision:accept|reject|resolve,rationale}`.
- GET `/tenders/{id}/runs`; GET `/runs/{id}`; GET `/runs/{id}/events`; POST `/runs/{id}/cancel`; POST `/runs/{id}/resume`; POST `/tenders/{id}/work/stop`.
- GET/PATCH `/settings`.

## Estimates and draft outputs

`EstimateService(repo)` provides `refresh(tender_id)` and `view(tender_id)`. Root calls refresh after successful imports.

- GET `/tenders/{id}/estimate` -> `EstimateView`: items, per-currency totals, completeness, counts, blocking reasons and coverage note.
- POST `/tenders/{id}/estimate/refresh`.
- PATCH `/tenders/{id}/estimate/items/{item_id}` with `ItemUpdate`.
- POST `/tenders/{id}/estimate/items/{item_id}/quantity-proposals` with `QuantityRequest`.
- POST `/tenders/{id}/estimate/quantity-proposals/{proposal_id}/approve` with `EngineerDecision`.
- GET/POST `/tenders/{id}/outputs`; GET `.../outputs/{output_id}/download`.

Commercial decisions and output generation require `engineer_confirmed:true` and a nonblank `rationale`. Agents have no access to these engineer routes.

`ItemUpdate` optional fields: `confirm_source`, `quantity_cell`, `unit_rate` or `components`, `currency`, `tax_basis` (`excluding_vat|including_vat|unknown`), `vat_percent`, `provenance {basis:observed|estimated,observed_on,source_ids,urls,geography,conditions}`. `effective_quantity` uses the supplied quantity until the engineer approves a separate quantity proposal. Agent quantity proposals keep their BOQ item fingerprint, which is checked again before approval. Totals distinguish `priced_subtotal_ex_vat` from nullable `total_ex_vat` and `total_inc_vat`; missing rates, unconfirmed rows, ambiguous quantities or missing tax information block completeness. Values are grouped by currency.

`OutputRequest.kind` is `boq_xlsx|analysis_docx|technical_docx|registers_xlsx|comparison_xlsx|programme_xlsx|client_boq`. Programme output requires an explicit `ConstructionProgramme`. `OutputRecord` status is always draft. Files and JSON manifests live under `repo.home/outputs`; source workbooks are never edited.

Client-format BOQ copies (`client_boq`) require `ClientBoqInput {artifact_id,mappings,currency,tax_basis,mapping_reviewed:true,quantity_mapping_approved}`. Quantities are preserved by default; writing other quantities needs approved quantity proposals and explicit mapping consent. Copies keep XLSX/XLSM format, original OOXML members and VBA; only mapped cells and necessary recalculation settings change.

Routine drafts (`draft_documents` proposals) are generated from saved work inside an approved plan after the run publishes, by `OutputService.generate_from_office`. GET `/tenders/{id}/programme-proposals` returns saved programme proposals with sources and currentness.

## Meaning search

`SemanticService(repo)` stores its derived index under the application home. `status`, `index` and `search` preserve original evidence with `metadata.semantic_match`. Scope and document kind apply before scoring. Ranking version `rrf-2-features` adds identifier/phrase/unit features after reciprocal-rank fusion. `RetrievalCoverage.unsupported_answer` distinguishes no match and weak meaning-only results from an unavailable index. The shared `retrieve()` path collapses identical content-hash/locator occurrences and lists other copies. Missing or stale indexes never silently become exact-word results.

## Commercial proposals and supplier requests

GET `/tenders/{id}/estimate/rate-proposals` returns `RateProposalRecord[]`. POST `.../rate-proposals/{proposal_id}/approve` accepts `RateApproval`. The immutable payload and item/source basis are rechecked at publication and approval. Rates never change quantity selection.

- GET/POST `/tenders/{id}/quotes` list and create drafts using `DraftInput {to,cc,subject,body,attachment_ids,source_ids}`; PATCH `.../quotes/{quote_id}` revises a draft.
- GET `.../{quote_id}/preview` returns `QuotePreview` with the exact content fingerprint, attachment hashes and currentness.
- GET `.../{quote_id}/eml` downloads a mail draft for the engineer to send from their own mail program. Quantix does not send mail.
- GET/POST `.../{quote_id}/replies` inspect and register reply records with preserved source evidence. Receipt is not commercial acceptance.

## Backups

GET/POST `/backups` lists/creates verified archives; GET `/backups/{id}/download` downloads one. POST `/backups/inspect` returns checked archive identity and coverage. POST `/backups/restore` binds `{path,expected_sha256,engineer_confirmed,rationale}` to the inspected archive and stages a journal. GET `/backups/pending` returns `RestoreReady|null`; GET `/backups/latest` returns `RestoreOutcome|null`. The CLI holds a single-workspace FileLock and applies a pending restore before creating any repository or API. Restores validate payload paths/hashes, keep a before-restore snapshot and install the database last. POST `/shutdown` requests owned-service shutdown.

## Approved reusable knowledge

`KnowledgeService(repo)` owns engineer-approved reusable notes (categories preference, method, reference, price, tax), not Tender facts. Creation requires title, content, category, `engineer_confirmed:true` and rationale; optional source Tender/source IDs must resolve there. Price and tax notes always require fresh validation before commercial use. GET/POST `/knowledge`, GET `/knowledge/{id}`, POST `/knowledge/{id}/withdraw`. Source revisions can mark a note `needs_recheck` without rewriting it. The tools `list_reusable_notes` and `read_reusable_note` label these as approved notes, never current Tender evidence.

## Project map and review coverage

`ProjectMapService(repo)` owns source-linked nodes and scoped engineer review records. GET `/tenders/{id}/project-map`; POST `.../nodes` with kind (`building|area|discipline|work_item|requirement`), title, detail, source_ids and optional parent_id; POST `.../nodes/{node}/decision` with approve|withdraw. POST `.../reviews` records artifact/page/locator review scopes. Coverage distinguishes registered files, extracted files/evidence, cited evidence, current review scopes and documents reviewed in full. Agent nodes are published only in the owning job's finalization, or when a staff assignment completes.

## Submission requirements and exports

`RequirementService` owns source-backed immutable requirements and append-only `requirement_events`. GET/POST `/tenders/{id}/requirements`; GET `.../requirements/{id}`; POST `.../{id}/decision` (approve|withdraw|satisfied|exception|reopen); POST `.../{id}/outputs` links a generated output; POST `.../{id}/outputs/{output_id}/unlink`. Origin is engineer or manager.

`SubmissionService(repo)` freezes selected drafts into a local ZIP: GET `/tenders/{id}/submissions`, POST `.../submissions/preview`, POST `.../submissions` with `SubmissionApproval`, GET `.../submissions/{id}/download`. Approval binds the preview fingerprint, checked files and bases, final review, acknowledged scope, displayed gaps and rationale. Approval never transmits externally or certifies full Tender compliance.

## Removed on 14 September 2026

At the engineer's request: the dynamic-office delegation stack (grants, envelopes, route bindings, ownership, checkpoints, handoffs, office messages, notebooks and their routes), the AI team approval, the no-tools routing classifier, the code sandbox and local code runtime, public research and working memory, reusable agent definitions, the benchmark suite and adoption gate, supplier mail sending/sync and watchers, push-to-talk voice, the manual measurement canvas and saved measurement records, and the Copilot, Gemini CLI and Claude Code routes. Existing databases keep their now-unused tables; nothing reads or writes them. Earlier, on 13 September, external MCP connections, plugins and method packages were removed; the internal Quantix MCP bridge used by the Codex and Grok clients remains.
