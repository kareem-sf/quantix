# Architecture decisions

## Local service and desktop shell

The [9 September project review](project-review-2026-09-09.md) records source findings and the focused diagnostics/AI repair. The subsequent [unified storage contract](unified-storage.md) establishes `~/.quantix` as the normal home for all Quantix-managed data, including `logs/`, AI installations/profiles, runtime state and temporary work. It supersedes the earlier split storage layout.

Use a Python 3.12 service for project records, document processing and AI execution; React/TypeScript/Vite for the interface; Tauri 2 for Windows, macOS and Linux. Rust owns desktop lifecycle and native dialogs, not a second copy of the domain. Release-only native packaging embeds the service; installers remain unbuilt for this increment.

FastAPI provides a typed HTTP boundary on loopback. Each launched session uses an unguessable bearer token; the renderer never receives provider credentials. The service uses SQLite transactions and a durable job ledger. Long jobs are not implemented as fire-and-forget HTTP background tasks. On restart interrupted jobs become resumable or explicitly interrupted; irreversible operations are never blindly replayed.

## Project storage

One application home contains a SQLite catalogue/domain database and content-addressed original objects. Every source occurrence retains Tender, relative path, version and hash. Extraction may be reused by identical content; source occurrences and Tender access remain distinct. Every database access to Tender knowledge is scoped by Tender ID.

Typed Pydantic models own API shapes. OpenAPI produces committed TypeScript declarations. The frontend only uses the API and native file selection, never database or arbitrary shell access.

## Documents

Use pypdfium2 for text/page geometry/rendering, openpyxl for spreadsheet cells/formulas/cached values, and python-docx for Word structure. PDFium work runs in one process lane or isolated processes, not concurrently across threads. Use native Word with macros disabled for legacy DOC only after a supported conversion test. DWG originals remain identified with explicit analysis coverage; PDF companions are separately identified documents.

OCR/conversion must be capability-tested and traceable. No result is marked reviewed merely because parsing succeeded. Raw cells, formulas and number formats are preserved before normalisation. Extraction artifacts and search indexes are reproducible derived data.

## Manager and specialists

The guided on-demand design now supersedes the earlier in-process provider layout below. Provider SDKs and client binaries live in selected, private component environments; the core retains source, budget and publication authority. See [AI setup architecture](ai-setup-architecture.md) for the current implementation and [provider support](ai-provider-support.md) for dated integration limits.

Use Pydantic AI with request-scoped native provider SDKs for direct model APIs. Official original-client runtimes are separate adapters with a scoped local MCP bridge. Provider-neutral tools call Tender-scoped service methods; Quantix validates evidence and publishes proposals outside the model loop. Hosted search URLs come from native provider metadata, not generated prose. SDK history remains separate from domain decisions and the job ledger.

Named profiles record access/billing method, endpoint, model capabilities and dated rates. Keys use an OS secret store or explicit session-only memory; selected cloud identity/environment access is supported. Tender policies approve destinations and budgets. Plans snapshot AI teams and connection revisions; alternatives must already be approved. Budget reservations persist through interruption. External SDK tracing is disabled; original client sign-ins remain in private runtime homes. No cached subscription credentials are repurposed as API keys. Details and trade-offs: [AI connections](ai-connections.md).

## Validation sources

- https://pypdfium2.readthedocs.io/en/stable/python_api.html
- https://openpyxl.readthedocs.io/en/stable/api/openpyxl.reader.excel.html
- https://python-docx.readthedocs.io/en/latest/user/documents.html
- https://developers.openai.com/api/docs/guides/agents
- https://developers.openai.com/api/docs/guides/agents/orchestration
- https://developers.openai.com/api/docs/guides/tools-web-search
- https://www.sqlite.org/fts5.html
- https://v2.tauri.app/develop/sidecar/


## MVP completion decisions

Project structure and review scopes use ordinary SQLite records, source snapshots and append-only decisions. They do not require a graph database. Submission requirements use the same source/decision pattern, with output links and explicit satisfied/exception reviews.

Client-format workbook export edits targeted OOXML members in a new copy so VBA, drawings and unsupported Excel objects can remain intact. The workflow rejects ambiguous targets instead of silently re-saving or repairing the source workbook. Final release records a checked local ZIP, its selected requirement coverage and engineer-approved scope; transmission is separate.

Agent-produced drafts use approved plan authority rather than synthesizing an engineer decision. Their source snapshots and originating task remain stable through finalization. The primary service owns both publication and rollback cleanup. Calibrated and general quantity proposals retain author origin and remain separate from pricing approval.

Development verification is active: backend tests, UI tests, typecheck, ruff and clippy run on every change set (see README commands). Modules exist only when production code imports them; synthetic acceptance drivers exercise product services, never test-only stand-ins.
