# Quantix audit — agent-architect 0.9.0 (2026-09-14)

Skill: `anyaa-labs/agent-skills` → `agent-architect` (AUDIT mode). Commit `61b531e` on `main`, with a dirty
working tree (82 changed or new files, some still being edited by a parallel session). Every figure below was
measured on this tree.

## System map

```
Agents found: 3 roles
  Conversation router — classifies each message, no tools — prompt ~0.6K tokens + context — conversation.py
  Tender Manager      — main agent — 55 tools — instructions ~2.2K tokens + OfficeOutput schema ~4K
                        + tool schemas ~8.5K + per-turn JSON context — office.py
  Generated staff     — Manager-created colleagues run on queued assignments — source tools + child assignment
                        — staff_*.py, office_concurrency.py
Models detected: Gemini 3.8 Flash (google, KNOWN 2026-09-08), gpt-5.3-codex-spark via Codex (openai, KNOWN),
  Anthropic / xAI / OpenAI-compatible keys supported (KNOWN)
Orchestration: router → manager → dynamic workers (hierarchy with queued concurrent children)
Memory: present (SQLite; typed but spread over 6 stores: work brief, working memory, staff notebooks,
  reusable notes + decision ledger, standing preferences, manager profile)
Runtime: custom loop on pydantic-ai 2.40 (direct APIs) + separate worker process driving the Codex/Grok
  clients through a loopback MCP bridge + an on-demand "AI components" installer
Sandbox: present — Podman VM for Python analysis, Monty subprocess for tool code, networkless browser VM
Reasoning state: preserved (SDK-managed; trim_tool_history only replaces ToolReturnPart content)
Modalities: text, image input (page view), push-to-talk transcription, OCR
MCP/tools: 1 first-party MCP bridge, 55 Manager tools — loadout: all-in-context
Residency: absent (N/A)
Agent identity: OS-keyring API keys and private client profiles; single local engineer; server-side tool
  fence + budget meters can refuse actions (external refusal point present)
Eval infrastructure: partial — 36K lines of tests, 24 synthetic goldens, retrieval baseline; none in CI
Tool count: 55 (Manager) — staff get the source-tool subset
Estimated cost per Manager invocation: ~USD 0.05–0.30 at Quantix's conservative Gemini 3.8 Flash card
  (live measured 2026-09-12: USD 0.09–0.19 per planning run; 13K → 158K input tokens over five steps)
```

**Size.** Backend has 247 modules and 62K lines. The AI worker is 4K lines. Backend tests are 36K lines.
Frontend TS/TSX is 83K lines (18K of that is generated bindings), plus 9K lines of CSS and 3K of Rust.

**What the bloat is not.** 244 of 247 backend modules are imported from `quantix.api` / `__main__`, and the
UI calls 180 of 199 API paths. This is not a dead-code problem.

**What it is.** Complete features that the product brief does not need, wired end to end, with a heavy
authority layer around each. The local database has **129 tables, 97 of them empty**. All staff, delegation,
assignment, handoff, notebook, watcher, correspondence, benchmark and graph tables are empty. Both Manager
runs recorded since the 2026-09-13 reset failed. On 2026-09-12, 26 of 43 runs failed.

## Scores

| # | Dimension | Score | Summary |
|---|---|---|---|
| 1 | Prompt Architecture (1.5x) | 4 | One 158-line paragraph covers about 15 workflows (supplier quotes, programmes, drawing takeoffs, staff hiring…) on every request. No stop list and no escalation ladder. |
| 2 | Tool Design | 5 | Good per-tool hygiene (argument errors return to the model, excerpt limits, `already_returned`). But 55 tools with overlapping reads and saves. |
| 3 | Context Management | 5 | Cache marker, excerpt caps and history trim are good. The AI connection catalog, 40 findings, the plan and 16 messages go in on every turn. |
| 4 | Multi-Agent Orchestration | 3 | Router plus generated staff with no evidence that one Manager fails. About 20K lines of coordination, and zero recorded staff executions. |
| 5 | Eval Infrastructure | 5 | Real deterministic goldens (no LLM judge) and a retrieval baseline, but no CI and no trace-level eval of live Manager runs. |
| 6 | Production Readiness (1.5x) | 4 | Budgets, reservations, timeouts and cancellation exist, but the core path fails more often than it succeeds. No partial result when a run stops. |
| 7 | Model Awareness | 6 | Gemini schema adapter, thinking levels, per-provider caching, Codex tool-search fix. One prompt for every provider. |
| 8 | Agent Security | 7 | Evidence-as-data rule, citation validation, exact-content approval before any send, sandboxed code, keyring credentials. |
| 9 | Memory Architecture | 5 | Provenance and approval on reusable notes, but six overlapping stores for one Manager. |
| 10 | Harness Architecture (1.5x) | 5 | Durable runs, events and trace. Two runtimes, a component installer and a worker fingerprint make every change fragile. |
| 11 | Multimodal Architecture | 6 | Image tools gated on model support; transcripts are editable drafts that are never sent automatically. |
| 12 | Sovereignty & Residency | N/A | No residency constraint. |
| 13 | Agent Identity & Authorization | 7 | Server-side fence and budget meters refuse actions outside a grant. Over-built for one local user, but correct. |

**Overall maturity: 5.5 / 10 — prototype quality.** Weighted: (6+5+5+3+5+6+6+10.5+5+7.5+6+7)/14.

## Findings

### Critical

- **[CRITICAL] (confidence 9/10) `backend/quantix/office.py:71-85`, `staff_*.py`, `office_concurrency.py`,
  `office_ownership.py`, `office_handoffs.py`, `office_messages.py`, `staff_routing.py`, `plan_review.py` — generated
  staff added without proving that a single Manager fails (Iron Law violation).**
  Current: the Manager can `create_staff`, `plan_staff_work` and `execute_staff`. Children drain concurrently
  under ownership fences, checkpoints, delegation grants, route bindings and aggregate reservations.
  About 19K backend lines and 15 Manager tools serve this. The DB holds zero staff assignments.
  Fix: remove generated staff and delegation. The Manager does the work itself with the source tools, and a
  plan stays a list of engineer-visible steps. If a context-isolation need is later proven, reintroduce one
  bounded sub-call tool with no persistence of its own.
  Why: Simplicity Ratchet; Multi-agent 1.1. The complexity has cost live runs (citation guard, budget
  exhaustion, silent Codex turns) without any measured quality gain.

- **[CRITICAL] (confidence 8/10) `backend/quantix/office_types.py` + `office.py:251-294` — one 15-field
  OfficeOutput (16.5K-char schema) is forced on every Manager response, including a one-fact question.**
  Current: summary, findings, plan, web_findings, price_proposals, quote_drafts, unit_rate_proposals,
  project_map_nodes, submission_requirements, programme_proposal, drawing_measurements, draft_documents,
  quantity_proposals and boq_item_proposals are all in one schema, validated together after the loop.
  Fix: the answer schema becomes `summary`, `source_ids` and `findings`. Each proposal kind becomes a save tool
  the model calls only when the request needs it (for example `propose_boq_rows`, `draft_supplier_request`),
  and each tool is validated on its own call.
  Why: Tool Contract (I/O matched to the decision); Context is Calories. A failed check on one proposal kind
  currently fails the whole paid run.

- **[CRITICAL] (confidence 8/10) Production readiness — the core Manager path does not reliably complete.**
  Current: 2 of 2 Manager runs failed on the current DB; 26 of 43 runs failed on 2026-09-12. The recorded
  causes were argument refusals, the citation guard, Codex tool deferral, schema bounds and budget holds, each
  patched separately.
  Fix: shrink the surface first (findings 1, 2 and 4), then gate on a small live-trace eval of 10 real questions
  with source-ID checks.
  Why: Failure Mode Cartography; Recovery Ladder.

### Important

- **[IMPORTANT] (confidence 8/10) `backend/quantix/office.py:473-600` — 55 tool schemas (~34K chars) in context
  on every request.** Overlaps: `search_sources` / `search_semantic_sources`; `read_source` / `read_document` /
  `read_whole_document` / `view_document_page`; `save_work_brief` / `save_working_memory` / `save_work_product`;
  `fetch_public_url` / `fetch_rendered_public_url`; `execute_tool_code` / `execute_python_analysis`.
  Fix: after staff removal, merge down to about 15 task-shaped tools: search, read (passage | document | page),
  package map, estimate, records, calculate, web search/fetch, brief.
  Why: Tool Loadout Beats Tool Hoarding.
- **[IMPORTANT] (confidence 7/10) `backend/quantix/jobs.py:587-637`, `conversation.py` — a separate no-tools
  classifier call runs before every engineering request.** It cannot see documents, and on Codex a local
  client needs up to 6 requests for it. The router adds latency and a misroute path. It pays off only because
  the Manager prefix is about 15K tokens. Fix: once the Manager payload is small, let the Manager answer small
  talk directly and delete the router. Why: Multi-agent 2.4; Harness Expiry Date.
- **[IMPORTANT] (confidence 8/10) `office.py:25-157` — the prompt carries instructions for every capability on
  every turn.** Examples: programme scheduling, drawing takeoffs, client BOQ mapping, supplier recipients,
  reusable-note tax rules and AI team routes. Fix: a short core prompt (identity, evidence rule, not-found
  rule, stop and escalate) with capability rules moved into the descriptions of the tools that need them.
  Why: Prompt architecture 1.1/1.2/2.3; Context is Calories.
- **[IMPORTANT] (confidence 8/10) `office.py:185-239` — just-in-case context.** The AI connection catalog,
  40 findings × 2,000 chars, 32 plan tasks and 16 messages × 3,000 chars are sent whether or not the request
  needs them. Fix: 6 messages plus the work brief; findings and plan through `read_tender_record`.
  Why: Context mgmt 2.1.
- **[IMPORTANT] (confidence 7/10) Harness — two AI runtimes plus a software installer.** They are
  `ai_api_engine` (pydantic-ai) and `ai_worker` + `ai_worker_client` + `ai_runtime_mcp` + `ai_components` +
  `ai_component_manifest`, about 9K lines of AI plumbing. Any edit under `backend/ai_worker` invalidates the
  installed component and forces Prepare + Check. Fix: keep the direct API path as the only runtime unless the
  free Codex subscription route is worth this cost to you (product decision). Why: Harness 2.3/2.5.
- **[IMPORTANT] (confidence 7/10) Memory — six stores for one agent.** The work brief, working memory
  (`research_tools`), staff notebooks, reusable notes, standing preferences and the manager profile each have
  their own tools and routes. Fix: keep the work brief (per-tender progress) and reusable notes (approved
  cross-tender guidance); fold working memory into the brief; staff notebooks go with staff.
  Why: Memory Type Discipline.
- **[IMPORTANT] (confidence 9/10) Eval infrastructure — no test runs in CI.**
  `.github/workflows/desktop-packages.yml` only builds packages; 36K lines of backend tests and the UI tests
  run only by hand. Fix: add a `pytest -q` + `npm run test:ui` + `tsc` workflow. Why: Eval 2.1.
- **[IMPORTANT] (confidence 9/10) Current tree — `backend/tests/test_office.py::test_manager_prepares_source_bound_proposals_without_publishing`
  fails** with `MemoryRepository has no attribute search_keyword` after the uncommitted `retrieval_service.py`
  rewrite. Fix: add the method to the test repository or route the test through the real repository.

### Minor

- **[MINOR] (confidence 9/10) `office.py:546-552`** — `routes_for(...)[:1]` feeds a `for index, route` loop that
  logs `"fallback": index > 0`. The fallback branch is unreachable (by design: no automatic paid fallback).
  Remove the loop.
- **[MINOR] (confidence 9/10) Frontend** — 34 production files (4.5K lines) are never imported from
  `src/main.tsx`, mostly unused shadcn parts. `@ai-sdk/react`, `ai` and `date-fns` have no importers;
  `embla-carousel-react`, `input-otp`, `react-day-picker`, `recharts`, `cmdk` and `react-resizable-panels` are
  used only by those unused files.
- **[MINOR] (confidence 9/10) `backend/quantix/office_scheduler.py`** — imported only by tests.
- **[MINOR] (confidence 8/10) `ai_gemini_schema.py`, the `codex.py` discovery sentence, `trim_tool_history`** —
  model-compensating code with no `RE-EVALUATE ON MODEL UPGRADE` marker.

## Failure mode maps

**Tender Manager**
- HALLUCINATION: cited IDs must have been read this run, checked in-run and before publication. Summary prose is
  not checked against the passages. **PARTIAL.**
- REFUSAL: the router can answer "not available" without seeing documents; the prompt forbids it, but nothing
  structural stops it. **PARTIAL.**
- LOOP: `FRUITLESS_SEARCH_LIMIT=3`, `already_returned`, a request cap from the tender allowance and a 15-minute
  timeout. **HANDLED.**
- ABANDONMENT: a stopped run saves nothing except an earlier brief; the engineer sees a sanitized error and no
  partial answer. **PARTIAL.**
- STALE BELIEF: `work_progress.require_current_brief` freshness check; reusable notes carry withdrawn and recheck
  flags. **PARTIAL.**

**Conversation router** — HALLUCINATION PARTIAL (misclassification); REFUSAL PARTIAL; LOOP N/A (single pass);
ABANDONMENT N/A (single turn).

**Generated staff** — HALLUCINATION PARTIAL (the Manager must re-read cited sources); REFUSAL PARTIAL
(`answer_staff_question`); LOOP HANDLED (shared allowance); ABANDONMENT PARTIAL (checkpoints). Never observed
running.

UNHANDLED: 0.

## Model upgrade candidates

1. Conversation router — absorbable: yes, once the Manager prefix is small and cached.
2. `ai_gemini_schema` bound stripping — partial: re-test when Gemini accepts JSON-schema bounds in `validated` mode.
3. `trim_tool_history` — partial: re-test against provider-side context management.
4. The Codex "search for the quantix tools by name" instruction — re-test when Codex stops deferring MCP tools.

## Top 3 recommendations

1. **One Manager, no generated staff** — removes about 20K backend + 6K UI lines and 15 tools. Effort L.
2. **Small answer schema plus proposal save tools; short core prompt; about 15 tools; no router** — fewer
   failed paid runs and a much smaller per-request payload. Effort M.
3. **Delete what nothing uses now, and put the test suites in CI** — 34 UI files, 9 npm packages,
   `office_scheduler`, and the fix for the failing office test. Effort S.

## Engineer decisions (2026-09-14)

- **The team stays.** "Quantix is a tendering team, not a tendering manager." The Manager hires staff with
  live-generated profiles, delegates work to staff and an AI team, and the engineer can see all of it.
  Recommendation 1 is therefore replaced by *simplify the team so it actually runs*.
- **AI routes kept:** API keys (OpenAI, Anthropic, Google, xAI, one OpenAI-compatible custom endpoint),
  ChatGPT/Codex subscription, Grok subscription. Removed: Copilot, Gemini CLI, Claude Code handoff and 16 other
  API presets.
- **Deleted:** code sandbox (Podman VM, hosted code, Monty tool code, research browser VM), benchmarks and
  the adoption gate, supplier email and watchers (a future feature), voice input, manual drawing measurement.
- **New:** agentic drawing takeoff. The connected vision model measures from drawings, cross-checks the BOQ
  and lists items shown on drawings but missing from the BOQ. The engineer reviews, approves and steers.
- **Git:** checkpoint commit on a cleanup branch, one commit per step, then merge to `main`, push, PRs and CI.

## Execution plan

| Step | Work | Size |
|---|---|---|
| 0 | Checkpoint commit of the current tree on `cleanup/2026-09-14`; record baseline test results | S |
| 1 | Delete unused UI files and npm packages, `office_scheduler`; fix the failing office test | S |
| 2 | Delete the code sandbox, benchmarks and adoption gate, mail and watchers, voice, manual measurement — backend, UI, tests, DB tables, docs | M |
| 3 | Trim AI routes to the kept set; remove the Copilot, Gemini CLI and Claude Code worker clients | M |
| 4 | Manager harness: short core prompt, answer schema (`summary`, `source_ids`, `findings`) plus proposal save tools, about 20 tools, no just-in-case context, router folded in | L |
| 5 | Team, simplified: `hire_staff` (generated profile), `delegate(staff, brief, sources)` queues work, children run after the Manager turn with the tender route (serial on Codex/Grok, parallel on APIs), results return on the next turn, one Office view of staff, assignments, results and cost. Removes delegation grants, route bindings, ownership fences, checkpoints, handoffs, coordination and notebooks | L |
| 6 | Agentic takeoff: vision staff reads drawing pages, returns quantities with page evidence, cross-checks BOQ rows, lists missing items as proposals | L |
| 7 | CI workflow (pytest, UI tests, tsc, ruff); merge to `main`; push; PRs | S |

Each step ends with backend tests, UI tests and `tsc` green, followed by one commit.

Iron Law violations: 2 (generated staff, conversation router). The generated-staff finding is overruled by the
engineer's product decision; the implementation-size findings still stand.
Cognitive patterns applied: Simplicity Ratchet, Context is Calories, Tool Contract, Tool Loadout, Failure Mode
Cartography, Harness Expiry Date, Memory Type Discipline, Cost as Architecture, Recovery Ladder.

## Outcome (2026-09-14)

| Step | Result | Commit |
| --- | --- | --- |
| 0 | Checkpoint of the working tree | `19ee3de` |
| 1 | Unused UI files and npm packages removed; pytest-xdist added | `f7eb5b3` |
| 2 | Benchmarks, voice, mail and watchers removed | `7364792`, `18716b4`, `10c52da` |
| 3 | AI routes trimmed to the kept set | `a7be0aa` |
| 5 | Tender team runtime replaces the delegation stack; Team tab | `963a5cc`, `783c6ba` |
| 4 | Manager harness: 4.0k-character prompt, three-field answer, `propose` tools, no router | `a7dd4ab` |
| 6 | Agentic takeoff; manual measurement removed | `2d6988b` |
| 7 | Docs, formatting, CI, merge and push | this branch |

Steps 4 and 5 ran in the order 5 then 4 because the harness depends on the new team tools. The Manager still has
35 tools, not the target of about 20: removing the duplicate search and document readers was offset by the two
proposal tools. Iron Law violations resolved: the conversation router is gone; generated staff remain by the
engineer's decision.
