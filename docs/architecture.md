# Architecture decisions

## Local service and desktop shell

The [unified storage contract](unified-storage.md) establishes `~/.quantix` as the home for all Quantix-managed data, including `logs/`, AI installations/profiles, runtime state and temporary work.

Use a Python 3.12 service for project records, document processing and AI execution; React/TypeScript/Vite for the interface; Tauri 2 for Windows, macOS and Linux. Rust owns desktop lifecycle and native dialogs, not a second copy of the domain. Release-only native packaging embeds the service; installers remain unbuilt for this increment.

FastAPI provides a typed HTTP boundary on loopback. Each launched session uses an unguessable bearer token; the renderer never receives provider credentials. The service uses SQLite transactions and a durable job ledger. Long jobs are not implemented as fire-and-forget HTTP background tasks. On restart interrupted jobs become resumable or explicitly interrupted; irreversible operations are never blindly replayed.

## Project storage

One application home contains a SQLite catalogue/domain database and content-addressed original objects. Every source occurrence retains Tender, relative path, version and hash. Extraction may be reused by identical content; source occurrences and Tender access remain distinct. Every database access to Tender knowledge is scoped by Tender ID.

Typed Pydantic models own API shapes. OpenAPI produces committed TypeScript declarations. The frontend only uses the API and native file selection, never database or arbitrary shell access.

## Documents

Use pypdfium2 for text/page geometry/rendering, openpyxl for spreadsheet cells/formulas/cached values, and python-docx for Word structure. PDFium work runs in one process lane or isolated processes, not concurrently across threads. Use native Word with macros disabled for legacy DOC only after a supported conversion test. DWG originals remain identified with explicit analysis coverage; PDF companions are separately identified documents.

OCR/conversion must be capability-tested and traceable. No result is marked reviewed merely because parsing succeeded. Raw cells, formulas and number formats are preserved before normalisation. Extraction artifacts and search indexes are reproducible derived data.

## Manager and team

The Tender Manager and its staff are described in [tender team runtime](design/tender-team-runtime.md). One Manager run holds the whole piece of work: Manager turns, then the staff assignments they queued, then another Manager turn with the outcomes, up to six Manager turns. Everything runs under one Tender AI route, request allowance and budget meter; there are no per-assignment approvals, grants or route bindings.

The model answers with a summary, cited source IDs and findings. Every other record is staged with the `propose` tool and saved only when the run finishes, or for staff when their assignment completes, after the same checks run again. Quantix, not the model, validates evidence, computes takeoff comparisons and publishes domain records, inside one SQLite transaction with the run's terminal state.

Direct model APIs use Pydantic AI with request-scoped native provider SDKs in the service. ChatGPT/Codex and Grok subscriptions run their official clients in an isolated worker process that reaches the Tender only through a scoped local MCP bridge. Provider-neutral tools call Tender-scoped service methods. Hosted search URLs come from native provider metadata, not generated prose. Keys use the OS secret store or session-only memory; subscription sign-ins stay in private client homes and are never reused as API keys. See [AI setup architecture](ai-setup-architecture.md) and [provider support](ai-provider-support.md).

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

Agent-produced drafts use approved plan authority rather than synthesizing an engineer decision. Their source snapshots and originating task remain stable through finalization. The primary service owns both publication and rollback cleanup. Takeoff lines and quantity proposals retain their author and remain separate from BOQ quantities and pricing approval.

Backend tests, UI tests, typecheck, Ruff and formatting checks run in CI on every pull request (see README commands). Modules exist only when production code imports them; synthetic acceptance drivers exercise product services, never test-only stand-ins.
