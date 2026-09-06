# Quantix rebuild ledger — docs/plan.md

## 2026-09-06

- Approved full product direction and local shared project-knowledge design recorded in docs/spec.md.
- Workspace was empty. Node 26.7.0, npm 12.0.2, Rust/Cargo 1.98.0 and Python are available.
- Primary session model cannot be verified or changed through the available controls. All new delegated agents explicitly use gpt-6-astra/xhigh.
- AI SDK research completed by validate_ai_runtime. Official OpenAI Agents Python/Responses chosen; API credential required. No provider calls performed. User asked for a local configuration path if available; independent work continues.
- Document research completed by validate_document_stack. Bundled PDFium/openpyxl/python-docx successfully read representative sources without changing them. Word installed; legacy conversion not yet exercised. No CAD converter found in standard locations.
- Visual concept task desktop_visual_design is running, owns docs/design only.
- Architecture and tasks recorded before implementation.

## Interface review

| Tasks | Shared boundary | Ruling |
|---|---|---|
| Documents / intake | ExtractionResult and parser functions | Defined in contracts; parser has no database writes. |
| Workspace / manager | Repository read/write operations | Defined before manager implementation; source IDs validated by Tender. |
| API / frontend | OpenAPI models and REST routes | Python source of truth; generated TypeScript committed. |
| Jobs / AI | Persisted state and cancellation | Domain jobs own lifecycle; SDK owns model/tool loop. |
| Estimates / outputs | Decimal values and approved quantities | Outputs consume checked records; no model arithmetic as authority. |

Ruling: independently owned implementation files may be developed in parallel as explicitly authorised by the user; integration/review remain with the primary agent.
Ruling: work directly in the new empty requested directory on a rebuild branch; no second copy or legacy worktree is needed.

## Next

Define contracts, initialise repository/dependencies, implement tests and the document/workspace foundation.

## Foundation progress

- Task 1 document implementation active in validate_document_stack. Core tests pass; genuine legacy Word conversion succeeded with original hash preserved. Awaiting final review report.
- Task 2 repository/intake implementation added. Twelve targeted tests pass covering persistence, source isolation, duplicate/revision behaviour, explicit approvals, cancellation, unsafe ZIP paths and unchanged originals.
- Task 3 concept reviewed and accepted by root; desktop_visual_design resumed to implement src/** (excluding generated bindings). Root owns package/config files.
- Task 4 office worker implemented by validate_ai_runtime; sixteen local tests pass including actual SDK orchestration with a test-only model. No live provider requests. Root reviewing and integrating.
- Python dependencies installed into backend/.venv. openai-agents 0.22.0, openai 3.8.0, FastAPI 0.141.1. Node dependency compatibility corrected to TypeScript 5.9.3 and jsdom 30.0.1 after registry/peer inspection.
- Root next: authenticated API, job lifecycle, generated bindings and native/development launch, followed by estimates/outputs and full integration review.
