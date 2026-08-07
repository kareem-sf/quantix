# Agent-framework selection for Quantix v0

**Evidence snapshot:** 2026-08-07
**Question:** Should Quantix adopt PydanticAI or another ready-made agent framework while keeping the accepted Tauri 2/Rust Host, Codex subscription authentication, and EITL design?

## Recommendation

Do **not** add PydanticAI, the OpenAI Agents SDK, LangGraph, CrewAI, Microsoft Agent Framework, AutoGen, or Rig as a Quantix v0 runtime dependency.

Use the pinned **Codex app-server itself as the ready-made agent runtime**. Keep the Rust Quantix Host as the deterministic tender-office orchestrator: it creates Agent Profiles and Codex threads, schedules exact Tender Tasks, materializes minimum-disclosure workspaces, enforces Permission Grants and EITL gates, validates proposed outputs, and publishes accepted facts and artifacts. This is the boundary already recorded in [ADR 0004](../adr/0004-run-agent-profiles-through-host-controlled-codex-threads.md), [ADR 0006](../adr/0006-enforce-agent-access-through-host-owned-run-grants.md), and [ADR 0008](../adr/0008-keep-codex-behind-a-quantix-owned-ai-provider-contract.md).

This recommendation does not mean rebuilding a generic agent framework. Codex already supplies the model-facing agent loop, ChatGPT subscription login, threads, turns, streaming events, approvals, and sandboxed tool execution. OpenAI explicitly positions app-server for deep product integrations requiring authentication, conversation history, approvals, and streamed agent events; it supports local JSONL over stdio and generates version-exact JSON Schema or TypeScript protocol definitions. App-server also exposes account methods that report the active authentication mode and ChatGPT plan. [Codex app-server](https://learn.chatgpt.com/docs/app-server), [Codex authentication](https://learn.chatgpt.com/docs/auth)

The unavoidable custom work is Quantix's product: tender roles, Capability Demands, Work Plans, evidence, permissions, task dependencies, EITL decisions, audit, and recovery. A generic framework would move those rules into another runtime and another state model; it would not delete them.

## Pydantic is not PydanticAI

- **Pydantic** is a Python validation and serialization library. `BaseModel` validates Python data, serializes it, and generates JSON Schema; it does not run agents or orchestrate a team. [Pydantic models](https://docs.pydantic.dev/latest/concepts/models/), [Pydantic JSON Schema](https://docs.pydantic.dev/latest/concepts/json_schema/)
- **PydanticAI** is a separate Python agent framework. Its `Agent` owns an LLM model, instructions, tools, dependencies, and a structured output type. It supports agent delegation, programmatic handoffs, graph control flow, deferred tool approvals, and integrations with durable-execution systems. [PydanticAI agents](https://pydantic.dev/docs/ai/core-concepts/agent/), [multi-agent patterns](https://pydantic.dev/docs/ai/guides/multi-agent-applications/), [deferred tools](https://pydantic.dev/docs/ai/tools-toolsets/deferred-tools), [durable execution](https://pydantic.dev/docs/ai/capabilities/durable_execution/overview/)

PydanticAI is a good choice for a Python application that calls ordinary model APIs. It is not the most suitable choice for the accepted Quantix architecture:

1. It requires Python 3.10+ and would add a long-lived Python orchestration runtime beside the genuine Rust Host. Quantix currently needs Python only for disposable Docling CLI jobs. [PydanticAI installation](https://pydantic.dev/docs/ai/overview/install/)
2. Its documented OpenAI provider calls the OpenAI Responses or Chat Completions API and is configured with an OpenAI Platform API key. OpenAI-compatible providers likewise use HTTP base URLs and keys. That is not the Engineer User's ChatGPT-authenticated Codex session. [PydanticAI OpenAI model](https://pydantic.dev/docs/ai/models/openai/)
3. A non-HTTP provider requires implementing PydanticAI's `Model` abstraction. Adapting Codex app-server would therefore still be Quantix-owned code, and it would translate a full agent/thread/approval protocol into an abstraction designed around model requests and responses. [PydanticAI custom models](https://pydantic.dev/docs/ai/models/overview/#custom-models)
4. PydanticAI's production durability comes through Temporal, DBOS, Prefect, Restate, or other external runtimes. Quantix already has one local source of truth, one Rust workflow boundary, and crash rules tied to Tender Store transactions; adding another journal/checkpoint model would create competing authority. [PydanticAI durable execution](https://pydantic.dev/docs/ai/capabilities/durable_execution/overview/)

Therefore, Quantix should not add either Pydantic or PydanticAI to its Host. Strict Rust DTOs and external agent-output contracts remain covered by Serde, `garde`, and `jsonschema` as accepted in [ADR 0009](../adr/0009-run-one-local-host-over-self-contained-tender-stores.md).

## The decisive compatibility question

Quantix's fixed provider is not simply “OpenAI.” It is the Engineer User's existing **Codex-managed ChatGPT subscription session**, used through a pinned `codex app-server` child process. Codex supports both ChatGPT subscription login and API-key login, but they are distinct billing, policy, and credential paths. [Codex authentication](https://learn.chatgpt.com/docs/auth)

None of the compared frameworks documents a native model/provider adapter for Codex app-server and its ChatGPT-session authentication:

- PydanticAI's OpenAI integration uses the OpenAI APIs and an API key. [PydanticAI OpenAI model](https://pydantic.dev/docs/ai/models/openai/)
- The OpenAI Agents SDK uses the Responses API by default; its default and custom OpenAI-compatible clients are API clients. [Agents SDK models](https://openai.github.io/openai-agents-python/models/)
- LangGraph supplies orchestration and persistence rather than a Codex client; its documented `ChatOpenAI` integration requires an OpenAI Platform API key. [LangGraph overview](https://docs.langchain.com/oss/python/langgraph/overview), [ChatOpenAI setup](https://docs.langchain.com/oss/python/integrations/chat/openai)
- CrewAI's documented OpenAI `LLM` takes `OPENAI_API_KEY`; a nonstandard backend requires a custom `BaseLLM`. [CrewAI LLM documentation](https://github.com/crewaiinc/crewai/blob/main/docs/edge/en/concepts/llms.mdx), [custom LLM](https://github.com/crewaiinc/crewai/blob/main/docs/edge/en/learn/custom-llm.mdx)
- Microsoft Agent Framework lists OpenAI API, Azure OpenAI, Foundry, Anthropic, Ollama, GitHub Copilot, and custom providers, but not Codex app-server. [Agent Framework providers](https://learn.microsoft.com/en-us/agent-framework/agents/providers/)
- Rig's built-in agents sit over completion-model/provider clients. Its extension path requires a custom provider/`CompletionModel`; it does not document Codex app-server support. [Rig provider concepts](https://docs.rig.rs/docs/concepts/provider_clients), [custom provider guide](https://docs.rig.rs/guides/extension/write_your_own_provider)

There is one important near-match: OpenAI documents running `codex mcp-server` as a tool used by an OpenAI Agents SDK workflow. However, the outer Agents SDK agents in that official guide still require an `OPENAI_API_KEY`; Codex is a nested MCP tool, not the Agents SDK's subscription-authenticated model backend. This introduces two agent layers and does not satisfy Quantix's no-BYOK requirement. [Use Codex with the Agents SDK](https://learn.chatgpt.com/docs/mcp-server)

## Candidate comparison

Repository popularity is recorded because it is a useful maintenance signal, but stars do not overcome a runtime or authentication mismatch. Counts below are a 2026-08-07 snapshot from each official GitHub repository.

| Candidate | Approx. stars | Useful ready-made capability | Quantix v0 mismatch | Verdict |
| --- | ---: | --- | --- | --- |
| [PydanticAI](https://github.com/pydantic/pydantic-ai) | 19.1k | Excellent typed outputs, tools, delegation, deferred approvals, provider abstraction | Python sidecar; API-oriented models; no Codex app-server adapter; durability adds another runtime | Do not adopt |
| [OpenAI Agents SDK for Python](https://github.com/openai/openai-agents-python) | 28.4k | Lightweight agent loop, agents-as-tools, handoffs, sessions, guardrails, serialized HITL state | Python sidecar; outer agents use API credentials; duplicates Codex's own agent loop | Do not adopt; retain as pattern reference |
| [LangGraph](https://github.com/langchain-ai/langgraph) | 39.1k | Strong graph runtime, checkpoints, durable execution, interrupts, parallel routing | Python/JS runtime and checkpoint authority duplicate the Rust Host and Tender Store; no Codex client | Do not adopt |
| [CrewAI](https://github.com/crewaiinc/crewai) | 56.7k | Familiar roles/tasks/crews, hierarchical manager, flows, persistence, human feedback | Python sidecar; API/custom LLM contract; high-level autonomous hierarchy conflicts with Host-controlled authority | Do not adopt; study configuration vocabulary only |
| [Microsoft Agent Framework](https://github.com/microsoft/agent-framework) | 12.6k | Broadest workflow fit: sequential/concurrent/handoff/group orchestration, checkpointing, HITL, providers | Full framework is Python/.NET; Go is separately previewed; no Rust or Codex app-server provider; another state/runtime seam | Do not adopt |
| [AutoGen](https://github.com/microsoft/autogen) | 60.3k | Historically important multi-agent patterns | Microsoft marks it maintenance mode and directs new projects to Agent Framework | Reject for new work |
| [Rig](https://github.com/0xPlaygrounds/rig) | 8.2k | Most credible Rust-native model/tool abstraction; 20+ providers | API/completion abstraction, not Codex app-server; no matching durable EITL workflow; project warns of continuing breaking changes | Do not adopt in v0; reassess for future API providers only |

### OpenAI Agents SDK

The SDK's manager pattern is conceptually close to Quantix: a manager can invoke specialist agents as tools while retaining control, and tools or nested agents can pause a run for approval and later resume from serialized `RunState`. [Agents as tools](https://openai.github.io/openai-agents-python/tools/), [human in the loop](https://openai.github.io/openai-agents-python/human_in_the_loop/)

It is still the wrong runtime dependency. Quantix's Tendering Manager is not allowed to become the canonical scheduler or approval authority; the Rust Host owns those facts. Running an Agents SDK manager outside Codex would also require an API-backed outer model. The SDK is valuable as design evidence for manager-versus-handoff semantics, not as an installed Quantix component.

### LangGraph

LangGraph is explicitly a low-level orchestration runtime rather than a high-level model abstraction. Its strengths are durable execution, state checkpoints, human interrupts, and fault recovery. [LangGraph overview](https://docs.langchain.com/oss/python/langgraph/overview), [persistence](https://docs.langchain.com/oss/python/langgraph/persistence), [human in the loop](https://docs.langchain.com/oss/python/langchain/human-in-the-loop)

Those strengths overlap almost exactly with responsibilities already assigned to the Rust Host and Tender Store. Making a LangGraph checkpoint canonical would conflict with Quantix's transactional task, approval, audit, and recovery facts; making it noncanonical would retain the custom Rust rules while adding a duplicate graph runtime. It removes no required Quantix interface.

### CrewAI

CrewAI most directly resembles the user's mental model of a Tendering Manager and named specialist employees. Its hierarchical process accepts a `manager_agent` or `manager_llm`, and its Flows can persist and pause/resume for human feedback. [hierarchical process](https://github.com/crewaiinc/crewai/blob/main/docs/edge/en/concepts/processes.mdx), [human feedback in flows](https://github.com/crewaiinc/crewai/blob/main/docs/edge/en/learn/human-feedback-in-flows.mdx)

That resemblance is not enough. Quantix's “personalities” are versioned Agent Profiles with exact capabilities, data scopes, prohibited actions, outputs, and reviewers. Team composition and manager proposals may be AI-assisted, but authority remains in deterministic Host rules and EITL decisions. CrewAI's high-level crew state would be a second orchestration model, and its LLM layer does not consume Codex app-server subscription sessions.

### Microsoft Agent Framework and AutoGen

Microsoft Agent Framework is the strongest generic workflow feature match: it provides explicit workflow graphs, sequential/concurrent/handoff/group patterns, superstep checkpointing, and human request/response gates. Its official repository describes full Python and .NET implementations; the newer Go implementation is in public preview, and no Rust implementation is documented. [Agent Framework repository](https://github.com/microsoft/agent-framework), [workflow model](https://learn.microsoft.com/en-us/agent-framework/workflows/workflows), [HITL](https://learn.microsoft.com/en-us/agent-framework/workflows/human-in-the-loop), [checkpoints](https://learn.microsoft.com/en-us/agent-framework/user-guide/workflows/checkpoints)

Adopting it would therefore add Python or .NET beside Rust and would still require a custom Codex adapter. AutoGen is not an alternative: Microsoft marks AutoGen as maintenance mode and recommends Agent Framework for new projects. [AutoGen repository](https://github.com/microsoft/autogen)

### Rig and other Rust options

Rig is the only Rust agent library in this comparison with meaningful adoption, current activity, broad provider integrations, and a documented custom-provider extension point. It is attractive if Quantix later adds ordinary API-based providers directly from Rust. Its own repository currently warns that planned features will bring breaking changes. [Rig repository](https://github.com/0xPlaygrounds/rig)

Rig does not remove the v0 Codex adapter. Codex app-server is a stateful agent harness with threads, turns, approvals, control requests, and streamed events—not a completion endpoint. Implementing Rig's custom `CompletionModel` over app-server would either discard those native semantics or reproduce them behind a mismatched interface. Quantix should revisit Rig only after a second provider proves that a shared API-level model/tool loop is genuinely needed.

## Recommended v0 agent boundary

```text
React/TypeScript renderer
        |
        | named, typed Tauri commands
        v
Rust Quantix Host
        |-- Team Composer: deterministic coverage + Codex proposals + EITL approval
        |-- Work Plan / Tender Task scheduler: canonical Rust + SQLite facts
        |-- Permission and EITL engine: host-owned, default deny
        |-- Agent Runtime Module
        |      `-- Codex adapter -> one pinned codex app-server
        |                         -> one persistent Codex thread per Agent Profile
        |                         -> one turn per Agent Run
        `-- output validation -> Proposed records/artifacts -> review/approval/publication
```

Reuse directly:

- Codex's login, model-facing agent loop, threads, turns, streaming events, native sandbox, and protocol schemas. [Codex app-server](https://learn.chatgpt.com/docs/app-server)
- Tokio and ProcessKit for supervised process I/O and containment; Serde, `garde`, and `jsonschema` for contracts; `rusqlite` for canonical workflow facts, as selected in [ADR 0009](../adr/0009-run-one-local-host-over-self-contained-tender-stores.md).
- Official Docling CLI for document conversion; do not turn the Docling Python environment into a general Quantix Python application host.

Quantix must own:

- Capability Catalogue, Agent Profile versions, Team Composer policy, Work Plan versions, Tender Tasks, review separation, Permission Grants, EITL Approval Gates, evidence, audit, and recovery.
- A narrow, generated-schema Codex adapter that maps only readiness, thread lifecycle, one turn, interruption, events, and shutdown into the existing AI Provider Contract.

This is less custom code than adopting a generic framework because it keeps exactly one agent loop, one workflow authority, one canonical store, and one provider adapter.

## Revisit triggers

Re-open the framework decision only if one of these becomes true:

1. OpenAI publishes a production-supported Rust app-server client or another supported Codex integration that preserves ChatGPT subscription authentication and the required thread/approval semantics.
2. Quantix adds a second real provider whose production requirements cannot fit the existing semantic AI Provider Contract without a shared model/tool runtime.
3. Repeated implemented workflow hierarchy proves that typed Rust transition functions are materially duplicating a maintained Rust dependency.
4. Quantix deliberately changes the Host architecture to Python/.NET or deliberately adopts API-key/BYOK model access.

Until then, adding PydanticAI or another agent framework would violate the project's simplest-durable-system rule rather than satisfy its reuse rule.

## Release caveat

OpenAI currently says the app-server command and WebSocket transport are experimental and unsupported for production workloads. Quantix uses local stdio, not WebSocket, but the command-level support caveat remains a public-release gate already recorded in ADR 0009. A private engineer-operated v0 may continue with exact version pinning and fail-closed compatibility checks; public distribution requires OpenAI production assurance or a separately approved risk decision. [Codex app-server](https://learn.chatgpt.com/docs/app-server)
