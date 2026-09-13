# Quantix — Full Agentic Tender Office

Approved for implementation by the engineer on 12 September 2026. This execution record implements the complete plan approved in the current task, alongside docs/spec.md, docs/contracts.md and the existing adaptive-office master programme. Existing unfinished acceptance gates remain open until observed.

## Architecture and required outcome

Pydantic AI and official provider SDKs execute the five direct API groups. Original Codex and Grok clients execute supported subscriptions. AI SDK UI consumes durable Quantix chat/events. Python/FastAPI/SQLite remain authoritative for Tender data, permissions, budgets, assignments and publication. Each run has one execution engine. Shared tools, versioned SKILL.md packages, governed plugins and official MCP transports serve both engine types. No API key/OAuth conversion or automatic billing fallback.

Provider-independent definitions capture persona, instructions, skills, requested capabilities, output requirements and generation preferences. Tender instances, source permissions and exact account/model/runtime bindings remain separate. Versioned capabilities distinguish native support, Quantix-provided support, setup requirements, limitations and unknown observations. Old approvals gain no new authority.

## Required increments

- A — Agent library: create, generate from brief, edit, duplicate, version, retire, reuse; configurable Manager and specialists; model, temperature/sampling, thinking, output limits/mode and native-tool controls; validate incompatible settings; pin effective run configuration.
- B — Chat/workbench: AI SDK UI streaming over server-owned jobs/events; readable responses, tools, reasoning summaries when supplied, sources, artifacts and questions; queued follow-up, steering, stop/retry/resume; reconnect without replay; inspectable staff/worker desks.
- C — Tools/skills/plugins/MCP: one versioned typed invocation fence, progressive skills and resources, isolated declared scripts, agent-proposed draft methods, complete plugin lifecycle, local stdio and remote Streamable HTTP MCP discovery/resources/prompts/auth/health/reconnect/cancel, exact version grants.
- D — Deterministic workers and code: decimal/unit/rate/spreadsheet/document/extraction/chart operations with provenance; Monty bounded tool composition; dedicated Podman machine and disposable rootless Python containers (WSL2/AppleHV/QEMU); setup/repair/remove; selected inputs; resource and isolation limits; optional approved provider-hosted execution; reproducible receipts.
- E — Full official-client tasks: managed sessions bound to Tender/profile/account/runtime/scope; individually verified native capabilities; shared sandbox where native isolation is insufficient; native/Quantix tool origin and activity; accounted subagents or Quantix delegation; compatible-session resume; honest subscription telemetry.
- F — Research/grounding/memory: approved search, URL context/fetch, isolated public browser reading, citations and actual retrieved passages; structured market observations; hybrid/bilingual retrieval; distinct notes/assumptions/decisions/company knowledge; source dependency and revision-impact invalidation; explicit cross-Tender promotion.
- G — Delegation/review/recovery: human/Manager-created agents, dependency graphs, parallel/nested work, questions/messages/handoffs, aggregate root budgets and limits, client serialization/deadlock prevention, independent recomputation/evidence checking, disagreements, safe checkpoints and uncertain effects, native lifecycle/backups/reset.

## Defaults and limits

Only the Manager is initialized. Newly reviewed proposals default to four specialists, twelve assignments, depth two and concurrency two; existing grants retain their exact limits. Maximum depth/concurrency remains eight and account serialization can reduce concurrency. No default monetary authority. Local code by default; hosted code requires approved destination/spending. All managed data stays under ~/.quantix except OS-protected credentials and selected originals/exports. Preserve supplied files and all pre-existing work. No release packaging, real Tender approval, commercial test sending, Ollama or excluded business connectors. BIM/IFC/tablet/voice retain deferred status.

## Acceptance

Every increment includes actual API/runtime/UI/failure recovery and tests. Required journeys: one QS definition across API/Codex/Grok; effective/rejected settings; parallel and child assignment limits; skill script/plugin/MCP through both execution paths; BOQ plus research/calculation/cited artifact; source revision impact; independent planted-error review; interruption/resume without duplicate effects. Qualify host/credential/path/network/process/resource isolation on each claimed platform. Run affected tests and final backend/UI suites, generated types/typecheck, and real desktop light/dark/narrow/scaling/keyboard journeys.

Build 24 synthetic Pydantic Evals cases for BOQ, specification conflict, revisions, markets, supplier comparisons and integrated work, with explicit expected facts/evidence/numerics. Measure completion, evidence precision/recall, accuracy, latency, requests/tokens and attributable cost. Live evaluations repeat three times per configured engine/model and retain failures. Missing credentials/platforms leave live acceptance open; mocks and unavailable fallbacks never prove completion.

## Execution ledger

Ruling: implement on the existing `rebuild` checkout with per-file baselines under the Quantix development cache. Most of the current application is uncommitted; a HEAD-only worktree would omit the implementation being extended. Do not commit or revert unrelated work.

Ruling: the user explicitly approved all affected testing for this plan; historical test deferrals do not apply. Development agents use disjoint ownership; primary owns integration and inspects every change.

| Work package | State | Owner / evidence |
| --- | --- | --- |
| A1 reusable definitions and agent-library experience | Local gates passed | Versioned API/Settings/tools/bindings; actual synthetic browser create/edit/history and guided validation; live cross-engine reuse remains open |
| A2 provider capabilities and effective generation settings | Local gates passed | Explicit settings and provider combinations validated, lower effective native caps enforced, unsupported limits explained; no silent billing fallback |
| B durable chat stream and AI SDK UI | Local gates passed | Authoritative observer plus actual metered SDK streaming, staff drafts, reconnect/reset behavior and unchanged job admission |
| C shared tool/skill/plugin/MCP execution | Local gates passed | Exact references/policies, progressive schemas, actual declared callbacks, scoped transport/cleanup and uncertain-result reconciliation; real isolated stdio qualification open |
| D local deterministic/code runtime and setup | Platform acceptance open | Actual Monty, exact unit arithmetic, calculation/code inspectors and receipt/download checks; private Podman installed; WSL/full-container/platform qualification open |
| E original-client capabilities/session integration | Live acceptance open | Bound native sessions, activity, artifacts/cleanup and individual capabilities; SDK wire fixtures pass; live accounts untested |
| F research, grounding and evidence-aware memory | Local gates passed | Actual public retrieval/citation and multilingual vectors; revision warnings, scoped notes, cancellation draining, older-record access and explicit promotion; real rendered-browser qualification open |
| G delegation, review and recovery | Local gates passed | Reviewed 4/12/2/2 controls, aggregate scheduling, actual transactional checkpoints and verified resume without duplicate assignments/provider calls/spend |
| H benchmarks and integrated acceptance | Live/platform gates open | 24 cases/42 immutable originals, actual probes, three-repeat live driver, strict independent-review scoring and enforced adoption/report integrity; live allowance pending |

## Integration notes — 13 September 2026

- New delegation proposal depth/concurrency fields default to one when absent from a historical saved selection; new unsaved proposals use the approved 4/12/2/2 defaults. The scheduler revalidates the actual saved grant and cannot accept a larger caller-provided concurrency cap. Paging includes queued work after the first 200 historical assignments.
- The UI-message SSE observer is read-only and scoped to Tender/run. Disconnect does not cancel a job. Only saved Manager messages become final text. Unreviewed deltas, supplied reasoning summaries, retries and staff assignment text are separated by typed event identity.
- GenerationSettings additions preserve historical serialized AIRoute order/default shape. Explicit non-default settings affect the binding fingerprint. Source/library checks cannot establish live provider capability or actual billing.
- API SDK UI packages are installed (ai 7.0.99, @ai-sdk/react 4.0.102); backend Monty 0.0.23 and developer Pydantic Evals 2.40.0 are installed. New routes and generated frontend bindings are integrated. No new global release or live acceptance claim.
- Podman private executable was installed with verified official hash under ~/.quantix. WSL remains absent. Production full Python and plugin stdio require qualified isolation; no same-user host Python/stdio fallback is allowed.

## Final local gate — 13 September 2026

Full backend: **1,096 passed, one existing platform skip** (769.81 s). Final full UI: **330 passed in 75 files** (202.45 s). Frontend typecheck passed; OpenAPI/generated frontend declarations are refreshed. Four real filesystem-watcher tests passed after correcting metadata-triggered development reloads. Retained failed discovery runs and final evidence are recorded in [the acceptance report](../../reports/2026-09-13-full-agentic-office-acceptance.md). No release build, commit, real Tender approval, account authentication change or commercial send.

Actual browser checks used a synthetic Tender: imported BOQ originals, immutable comparison versions, source inspection/return, revised-source warning, exact calculation values/assumptions/hashes, public fetch/citation and actual local Monty output 42. Light/dark and 600/1440-pixel layouts and keyboard navigation were observed. Native window/scaling/tray/Quit acceptance, real isolated Python/stdio/browser execution and live API/Codex/Grok repetitions remain open. Missing provider limits remain explicit restrictions; they are not implemented as unenforceable promises.
