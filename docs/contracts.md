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
- `list_artifacts(tender_id) -> list[dict]`: id,tender_id,relative_path,name,version,content_hash,size,kind,status,area,metadata,warnings,is_current,created_at.
- `search(tender_id, query, limit=20) -> list[dict]`: id,artifact_id,artifact_name,relative_path,locator,text,page,sheet,cell_range,kind,score.
- `get_evidence(tender_id,evidence_id) -> dict`, `artifact_evidence(tender_id,artifact_id,offset=0,limit=50) -> list[dict]`. Scope must be verified.
- `list_findings(tender_id) -> list[dict]`, `add_finding(tender_id, title, detail, kind, source_ids, origin='agent', run_id=None) -> dict`. Kinds requirement,risk,question,assumption,observation; state proposed/accepted/rejected/resolved. Source IDs checked in Tender.
- `messages(tender_id) -> list[dict]`, `add_message(tender_id,role,content,source_ids=None,run_id=None) -> dict`. Roles engineer,manager,system; rows id,content,source_ids,created_at.
- `create_plan(tender_id,title,tasks:list[dict],run_id=None) -> dict`: each task title,description,role,source_ids. Plan id,title,version,status(proposed/approved/superseded),tasks.
- `list_tasks(tender_id) -> list[dict]`; task id,plan_id,title,description,role,status,source_ids,result,run_id.
- `create_run(tender_id,kind,instruction='') -> dict`, `update_run(run_id,**fields)`, `get_run(run_id)`, `event(run_id,kind,message,data=None)`, `run_events(run_id)`. Run kind import/manager/task/research; status queued/running/completed/failed/cancelled/interrupted; progress integer 0..100, detail, result, error, usage, timestamps. Persist before work.

Errors: `KeyError` for absent/scoped-out objects; `ValueError` for invalid inputs/business decisions. Domain methods perform transactional writes.

`Repository.atomic()` is a synchronous context manager grouping nested repository operations into one SQLite transaction; use it around publishing an office result. Never await inside it. Its connection is thread-local and is not shared with background workers.

## AI worker (implementation after installed SDK inspection)

`quantix/office.py` exports `async run_manager(repo: Repository,tender_id: str,run_id: str,instruction: str,api_key: str,model: str='gpt-6-astra') -> dict` and `async run_specialist(repo,tender_id,run_id,task:dict,api_key,model='gpt-6-astra') -> dict`. They use official Agents SDK, local tools calling repository APIs, xhigh reasoning, bounded turns, and no external tracing. They persist manager replies, validated findings and proposed plans using repository methods; root job runner owns terminal lifecycle/cancellation/errors. Actual supplier sending and price/quantity approval cannot be authorised by model text. Structured return includes summary,source_ids,usage. Source references must resolve; rejected invalid output is a visible failure.

## HTTP API

All `/api` requests require `Authorization: Bearer <local session token>`. Origin restricted to application/dev origins. Errors `{detail: string}`. API base `http://127.0.0.1:<port>/api`. Dates/decimals as above.

- GET `/health`: version,provider_ready,model,home (local display only).
- GET/POST `/tenders`; POST body `{name}`.
- GET `/tenders/{id}` returns overview.
- POST `/tenders/{id}/imports` body `{source_path}` returns run; POST accepts directory/zip only.
- GET `/tenders/{id}/artifacts`; GET `/tenders/{id}/artifacts/{artifact}/evidence?offset=0&limit=50`.
- GET `/tenders/{id}/artifacts/{artifact}/preview?page=1` PNG; GET `/tenders/{id}/evidence/{source_id}` evidence object.
- GET `/tenders/{id}/search?q=...` source hits.
- GET `/tenders/{id}/messages`; POST body `{content}` returns manager run.
- GET `/tenders/{id}/findings`; POST `/tenders/{id}/findings/{finding}/decision` body `{decision:accept|reject|resolve,rationale}`.
- GET `/tenders/{id}/plans`; POST `/tenders/{id}/plans/{plan}/approve` body `{rationale}` starts approved tasks.
- GET `/tenders/{id}/tasks`; POST `/tenders/{id}/tasks/{task}/run` returns run (requires approved plan).
- GET `/tenders/{id}/runs`; GET `/runs/{id}`; GET `/runs/{id}/events`; POST `/runs/{id}/cancel`; POST `/runs/{id}/resume` only for safe resumable jobs.
- GET/PATCH `/settings` returns public connection status, model, default_currency and preferences. PATCH accepts api_key (write-only), model,default_currency; secrets go into credential store.
- Estimate/output routes are added by task 5 with generated types, never invented in the renderer.

Frontend connection uses Tauri invoke `connection_info` in desktop mode; Vite supplies API base/token for authorised local browser development. Native `choose_package` selects directory or zip through dialog; browser development accepts a source path in the import form.
