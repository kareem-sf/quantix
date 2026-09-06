# Architecture decisions

## Local service and desktop shell

Use a Python 3.12 service for project records, document processing and AI execution; React/TypeScript/Vite for the interface; Tauri 2 as the Windows shell. Rust owns desktop lifecycle and native dialogs, not a second copy of the domain. This keeps document and AI libraries in their established Python ecosystem.

FastAPI provides a typed HTTP boundary on loopback. Each launched session uses an unguessable bearer token; the renderer never receives provider credentials. The service uses SQLite transactions and a durable job ledger. Long jobs are not implemented as fire-and-forget HTTP background tasks. On restart interrupted jobs become resumable or explicitly interrupted; irreversible operations are never blindly replayed.

## Project storage

One application home contains a SQLite catalogue/domain database and content-addressed original objects. Every source occurrence retains Tender, relative path, version and hash. Extraction may be reused by identical content; source occurrences and Tender access remain distinct. Every database access to Tender knowledge is scoped by Tender ID.

Typed Pydantic models own API shapes. OpenAPI produces committed TypeScript declarations. The frontend only uses the API and native file selection, never database or arbitrary shell access.

## Documents

Use pypdfium2 for text/page geometry/rendering, openpyxl for spreadsheet cells/formulas/cached values, and python-docx for Word structure. PDFium work runs in one process lane or isolated processes, not concurrently across threads. Use native Word with macros disabled for legacy DOC only after a supported conversion test. DWG originals remain identified with explicit analysis coverage; PDF companions are separately identified documents.

OCR/conversion must be capability-tested and traceable. No result is marked reviewed merely because parsing succeeded. Raw cells, formulas and number formats are preserved before normalisation. Extraction artifacts and search indexes are reproducible derived data.

## Manager and specialists

Use the official OpenAI Agents SDK on Responses, initially gpt-6-astra/xhigh. Official SDK tools and structured outputs own the model loop. Tools operate on validated Tender-scoped service methods. OpenAI web search supplies dated source URLs; market estimates are not represented as binding quotations. SDK conversation history is separate from durable domain decisions and the job ledger.

Store API keys in the Windows credential store, with environment configuration available for local development. Disable external SDK tracing by default to avoid sending project traces to another service. No cached subscription credentials are repurposed as API keys.

## Validation sources

- https://pypdfium2.readthedocs.io/en/stable/python_api.html
- https://openpyxl.readthedocs.io/en/stable/api/openpyxl.reader.excel.html
- https://python-docx.readthedocs.io/en/latest/user/documents.html
- https://developers.openai.com/api/docs/guides/agents
- https://developers.openai.com/api/docs/guides/agents/orchestration
- https://developers.openai.com/api/docs/guides/tools-web-search
- https://www.sqlite.org/fts5.html
- https://v2.tauri.app/develop/sidecar/
