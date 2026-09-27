# Architecture

## Shape: a modular monolith

One developer, a local-first desktop app now and a hosted web version later. So Quantix is one Python service with
clear domain modules, one React interface and a thin desktop shell. No microservices, plugin system or
compatibility layers.

```
ui/        React + Vite + TypeScript, Tailwind + shadcn/ui, TanStack Query; API types generated from OpenAPI
desktop/   Tauri 2 shell: owns the window and native file dialogs. No domain logic.
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

- AI connections and their keys are kept in `~/.quantix/auth.json`, and office settings (office mode, the office's
  AI) in `~/.quantix/settings.json`: plain JSON files the engineer can read. The service never returns a key.
- The UI talks only to the HTTP API on loopback. Each launch of the service gets a fresh bearer token. In
  development the Vite server starts the service on a free port and forwards `/api` to it with the token, so the
  browser preview and the desktop window use the same path and the UI never holds the token or a key.
  Release packaging, which embeds the service in the desktop app, comes later.
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
- **Proposals, review and gates.** Agents never write domain records. Staff propose through each module's tools,
  which validate and recompute every number, and the record starts as `proposed`: in the Tender Manager's review
  queue (`quantix/review`). The Manager has no production tools. He accepts a record (`reviewed`, waiting at the
  engineer's gate; or `office_approved` in Fully autonomous mode) or sends it back (`rejected`, with the correction
  posted in the team room naming who made it). New proposals wake him. The engineer approves or sends back reviewed
  records, and can reopen approved ones. The statuses and review fields live in `quantix/core/review.py`.
- **Checks.** `quantix/review/checks.py` computes findings from the records whenever they are asked for; they are
  never stored. Each finding is a blocker or a warning, with its message and the pages or records it rests on. The
  Manager can't accept a record with a blocker. A warning needs his reason, kept in `acceptances` under the
  finding's key, which names the check and the records, so a changed record is checked again. The thresholds are
  named constants that each message states: plausibility checks, not standards.
- **Evidence.** A cited location must exist and must have been read in that agent's run.

## Quantity take-off

Sheets are rendered and their vector paths extracted. Agents detect or set the scale, find elements with vision,
and place geometry in page coordinates, snapped to vectors where they exist. Quantix computes the length, area or
count from the geometry and scale. Measurements link to BOQ items, and Quantix computes the comparison.

## Testing

Service tests use pytest. Agent behaviour is tested with a scripted model at the provider boundary (Pydantic AI
`FunctionModel`), so the real runtime, tools, gates and database are exercised. UI tests use Vitest and Testing
Library. CI runs tests, typecheck, Ruff and Clippy on every pull request.
