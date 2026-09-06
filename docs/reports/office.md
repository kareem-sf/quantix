# Tender Office worker

Implemented 2026-09-06. Scope: the AI worker increment, not the completed Tender Office product.

## Implementation

- `backend/quantix/office.py` exports the contracted asynchronous `run_manager` and `run_specialist` functions. The root job runner owns run status, scheduling, cancellation, errors, recovery, and task approval checks.
- `office_types.py` defines strict Pydantic output proposals. Findings, dynamically chosen task roles, work plans, web research and price references are structured data. There are no approval, supplier-send, quantity-change or release tools.
- `office_tools.py` provides read-only, Tender-scoped evidence search, source reading, document reading and document listing. No tool accepts a Tender identifier, credential or operating-system path. Runtime repository/client dependencies do not enter prompts. Exact source IDs and locators survive; spreadsheet formulas and cached values are retained. Truncated evidence is marked partial.
- `office_research.py` records provider-returned citation URLs, consulted web sources, retrieval timestamps, request usage and native search call counts. Observed and estimated price references retain date, currency, unit, geography, tax basis, validity and conditions. Every price remains proposed; no estimate is adopted automatically.

The Tender Manager receives bounded prior conversation, existing findings and their engineer states, the current plan, extraction coverage and project areas. It can request up to three task-specific consultations through the SDK's `Agent.as_tool(parameters=..., input_builder=...)` API. Each consultation receives an explicit role, brief and Tender evidence. A specialist has no delegation tool. Root runs allow 12 turns; consultations allow 8. Outputs become proposed findings/plans and a manager message through the repository contract. Specialist results and research are preserved in local run events.

## SDK validation

Inspected the installed `openai-agents==0.22.0` and `openai==3.8.0` Python APIs before implementation:

- Private `AsyncOpenAI` client plus `OpenAIProvider(openai_client=..., use_responses=True)`; the client is closed after each worker invocation.
- Every agent uses `model='gpt-6-astra'` and `ModelSettings(reasoning=Reasoning(effort='xhigh'))`. The worker rejects silent model substitution.
- Native `WebSearchTool(external_web_access=True)` converts to the supported `web_search` Responses tool. `response_include=['web_search_call.action.sources']` preserves consulted sources.
- `RunConfig(tracing_disabled=True, trace_include_sensitive_data=False)` disables the SDK's external trace exporter. `store=False` avoids Responses conversation storage; this does not claim zero provider retention.
- `RunHooks.on_llm_end` records each response once. SDK nested agents share usage accounting; a real SDK-loop test verifies that nested usage is counted once.
- `Runner.run` is awaited, so task cancellation propagates. There is no background provider run or untracked loop. Only the root may resume/retry a recorded job. Repository history is the chosen local conversation strategy; no second SQLiteSession history is maintained.
- SDK `RunState.to_json` / `RunState.from_json` and approval APIs were inspected. They are not used for this read-only worker because it has no external side-effect tools requiring paused approvals.

Official sources: [Agents SDK](https://developers.openai.com/api/docs/guides/agents), [agent definitions](https://developers.openai.com/api/docs/guides/agents/define-agents), [manager orchestration](https://developers.openai.com/api/docs/guides/agents/orchestration), [running agents](https://developers.openai.com/api/docs/guides/agents/running-agents), [web search](https://developers.openai.com/api/docs/guides/tools-web-search), [GPT-6 Astra](https://developers.openai.com/api/docs/models/gpt-6-astra), [authentication](https://learn.chatgpt.com/docs/auth).

## Source and decision boundaries

All output references are validated before domain writes. A local source ID must both belong to this Tender and have been supplied through this run's evidence context/tools. Requirement, risk and observation findings require supporting evidence. A model cannot invent approval fields, source IDs or web citation URLs. Web URLs in prose are also checked against provider-returned web sources. Observed prices require a nonfuture observation date. Blank proposals are invalid.

These checks establish reference integrity; they cannot prove every model interpretation or numeric reading correct. Engineer review remains necessary. Native search does not establish that a supplier quotation is binding. Cost is explicitly unavailable rather than calculated from incomplete billing assumptions; aggregate and per-request token usage are returned.

Coverage is not promoted to reviewed by this worker. Existing accepted/rejected findings and approved plan states remain visible. No supplied Tender file is modified. Research source URLs and text may themselves contain private query information; events and worker outputs belong in the local runtime database, never source control.

## Verification and remaining integration

Ran:

```powershell
backend/.venv/Scripts/python.exe -m pytest backend/tests/test_office.py backend/tests/test_office_sdk.py -q
backend/.venv/Scripts/ruff.exe check backend/quantix/office.py backend/quantix/office_tools.py backend/quantix/office_types.py backend/quantix/office_research.py backend/tests/test_office.py backend/tests/test_office_sdk.py
```

Result: 16 tests passed; lint passed. Behavioral coverage includes source-bound persistence, fabricated/unread sources, strict tools, scoped runs, cancellation, existing decisions/plans, web citation preservation, invented URLs, observed price proposals, malformed output, dynamic consultations, formula context, partial extraction, and the real SDK nested tool loop against a temporary real Repository.

No credentials were inspected or stored by this task. No live provider request was sent. Root still needs the authorised credential/live quality check, API/UI integration and full milestone gate. Repository methods are individually transactional; the current contract does not provide one transaction spanning all proposal writes. All output is validated first, but a storage failure between writes can leave a partial batch. Root should assess atomic batch persistence at integration.
