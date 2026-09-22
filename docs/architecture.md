# Architecture

## Shape: a modular monolith

One developer, a local-first desktop app now and a hosted web version later. So Quantix is one Python service with
clear domain modules, one React interface and a thin desktop shell. No microservices, plugin system or
compatibility layers.

```
ui/        React + Vite + TypeScript, Tailwind + shadcn/ui, TanStack Query; API types generated from OpenAPI
desktop/   Tauri 2 shell: starts the service, owns the window and native file dialogs. No domain logic.
service/   Python 3.12, FastAPI, SQLAlchemy 2 + Alembic
  quantix/
    core/        settings, database, ids, event stream (SSE), the data home ~/.quantix
    ai/          connections: Pydantic AI for API keys; Codex/Grok official clients in a worker via a local MCP bridge
    documents/   import, hashing, PDF / Excel / Word readers, OCR, Arabic reading order, search
    office/      Manager and staff runtime, hiring and personas, portraits, tasks, messages, scheduler, gates
    boq/ takeoff/ estimate/ subcontract/ submission/
    company/     rate library, directory, past tenders, company rules
    api/         HTTP routes per module
  tests/
docs/
```

**Why:**

- Python has the strongest PDF, OCR, Excel and multi-provider agent libraries.
- SQLAlchemy and Alembic keep SQLite now and Postgres later as a configuration change.
- Server-sent events give live office updates to both the desktop and the web version.

Each domain module owns its models, its service functions and the agent tools that call them.

## Boundaries

- The UI talks only to the HTTP API on loopback, using a per-launch bearer token. It never sees credentials.
- Every record belongs to a tender, and every tender belongs to an owner, so accounts can be added later.
- Long work runs in a durable job ledger, not as fire-and-forget tasks. After a restart, interrupted work resumes
  or is shown as stopped.

## The office runtime

- **One runtime, many agents.** The Manager and staff differ only in persona and tools. Personas are generated
  by the Manager's `hire` tool.
- **Inboxes.** Engineer messages, direct messages, team-room posts, task assignments and answers land in an
  agent's inbox. A scheduler wakes agents with pending inbox items and runs one turn each: a tool loop with a step
  limit. API connections run several agents at once; a subscription client runs one at a time. Stop cancels
  everything on the tender.
- **Real conversation.** Agents talk only through `message`, `post_to_team` and `raise_concern`. The team room is
  exactly those records.
- **Proposals and gates.** Agents never write domain records. `propose(kind, data)` runs the owning module's
  validation, recomputes every number and stores the record as pending at its gate. Approval, by the engineer or by
  the office in Fully autonomous mode, publishes it inside one transaction.
- **Evidence.** A cited location must exist and must have been read in that agent's run.

## Quantity take-off

Sheets are rendered and their vector paths extracted. Agents detect or set the scale, find elements with vision,
and place geometry in page coordinates, snapped to vectors where they exist. Quantix computes the length, area or
count from the geometry and scale. Measurements link to BOQ items, and Quantix computes the comparison.

## Testing

Service tests use pytest. Agent behaviour is tested with a scripted model at the provider boundary (Pydantic AI
`FunctionModel`), so the real runtime, tools, gates and database are exercised. UI tests use Vitest and Testing
Library. CI runs tests, typecheck, Ruff and Clippy on every pull request.
