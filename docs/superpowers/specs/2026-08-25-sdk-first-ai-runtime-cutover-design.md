# SDK-First AI Runtime Cutover Design

Date: 2026-08-25  
Status: Approved in principle; pending written-spec review  

Supersedes:

- the account-authentication, provider-runtime, and release-gate consequences of
  ADR 0017, as recorded by ADR 0018;
- `2026-08-21-chatgpt-direct-provider-design.md` in full;
- the account-authentication and worker-runtime decisions in
  `2026-08-24-layer-1-ai-connection-foundation-design.md`; and
- the implementation assumptions in the existing Layer 1 and Layer 1B plans.

Retains the provider-selection, vault, compatible-endpoint, immutable-run,
permission, budget, audit, and canonical-state decisions from the Layer 1 design
unless this document changes them explicitly.

## 1. Outcome

Quantix removes custom provider protocols and uses maintained SDK/runtime
boundaries for model-facing work. The product remains one integrated Windows
desktop application for a non-developer Engineer; workers, runtimes, and package
management are internal implementation details and never become a marketplace.

The supported connection methods remain:

1. managed ChatGPT/Codex account login;
2. direct OpenAI, Anthropic, Google Gemini, or xAI API key;
3. custom OpenAI-compatible endpoint with explicit base URL, credential, and
   model ID; and
4. custom Anthropic-compatible endpoint with explicit base URL, credential, and
   model ID.

Quantix supplies no provider, connection, model, reasoning, embedding, or fallback
default. Testing a connection never activates it. The Engineer explicitly selects
the exact active connection/model/reasoning tuple in Settings.

## 2. Core Boundary

The Rust Host remains the only authority for:

- connection metadata and the one optional Active AI Configuration;
- Agent Profile/persona CRUD, versioning, Arabic identity, and Manager grants;
- Agent Run creation, immutable provider binding, and lifecycle;
- tool definitions, data views, permissions, approvals, execution, and
  idempotency;
- model, token, byte, tool-round, time, process, and output budgets;
- loop, repetition, and non-progress detection;
- Tender evidence, retrieval provenance, candidate validation, and canonical
  writes;
- cancellation, recovery, diagnostics, audit, and user-visible status; and
- future controlled subagent and recursive-swarm ceilings.

SDKs and provider runtimes supply intelligence and normalize provider protocols.
They never become a workflow database, canonical writer, permission authority,
fallback router, employee directory, or independent memory owner.

## 3. Runtime Architecture

```text
Tauri/React renderer (no secrets, no provider calls)
        |
        v
Rust Quantix Host (only authority and canonical writer)
        |-- supervised stdio JSON-RPC -> official Codex app-server
        |       `-- managed ChatGPT account route
        |
        |-- private bounded JSONL -> disposable Python AI worker
        |       `-- Pydantic AI -> official provider SDKs
        |
        |-- official Rust rmcp client boundary
        |       `-- approved local or remote MCP tools
        |
        `-- FastEmbed + SQLite FTS + sqlite-vec
                `-- local Arabic/English retrieval
```

The Host creates all processes, owns their Windows Job Objects, limits their
lifetimes and communication, validates every frame, and destroys the complete
process tree after completion or cancellation.

This is not an OS filesystem sandbox. A same-user process can potentially read
other same-user files even when Quantix supplies no such path. Private development
relies on empty staging roots, sanitized environments, strict protocol policy, and
code-path tests. Public release remains blocked until an OS-enforced filesystem
boundary (for example an approved AppContainer/restricted-token plus ACL design)
is separately specified and proven, or the approved provider integration supplies
an equivalent boundary.

Implementation is split into three ordered slices:

1. Codex managed-auth/runtime cutover and one successful selected-model run;
2. the general-provider worker and compatible endpoints; and
3. the Host-owned MCP client lane.

Dynamic Agent Profile CRUD and recursive swarms are not implemented by this
cutover; this design only fixes their future authority boundary.

### 3.1 Managed ChatGPT/Codex Lane

Quantix pins and verifies official `@openai/codex` app-server `0.149.1` and its
generated protocol schema. The private `chatgpt-direct-v1` adapter, private
ChatGPT backend URL, custom OAuth implementation, handcrafted Responses request,
and custom SSE parser are removed with no compatibility alias or fallback.

App-server is used because it is OpenAI's product-embedding interface and owns:

- managed ChatGPT browser login and device-code login;
- token persistence and refresh;
- account, plan, model, reasoning, and rate-limit discovery;
- thread/turn lifecycle, streaming, interruption, and provider errors; and
- the upstream service route as an opaque implementation detail.

Browser login sends `account/login/start` with `type: "chatgpt"`. Device-code
login sends `type: "chatgptDeviceCode"`. Both are explicit Engineer choices;
neither silently replaces the other. `chatgptAuthTokens` is not used. Quantix
does not parse, refresh, return, or persist ChatGPT tokens itself.

App-server runs with:

- `CODEX_HOME` set to the validated absolute Windows path
  `<Application Home>\codex` (normally `%USERPROFILE%\.quantix\codex`);
- `cli_auth_credentials_store="file"`;
- a version-pinned, Quantix-written `config.toml`; and
- a generated, contract-tested deny-by-default feature and protocol policy.

Only `auth.json` and Quantix's `config.toml` are approved persistent content in
that Codex home. Codex may create bounded lock/temp files while atomically updating
them; those files and every session/database/log/cache path are transient,
non-authoritative runtime state and are swept at safe process boundaries using an
exact version-pinned layout. The Codex home, sessions, raw provider state, and
credentials are excluded from Tender backups, portable archives, diagnostics
bundles, and exports.

The Codex credential file is intentionally not in Credential Manager or a
keyring. Quantix validates that the Codex home and files are ordinary local paths
under Application Home, not links/junctions, and have the same restrictive
current-user DACL required for Application Home. Codex owns the file format and
updates. Normal disconnect uses `account/logout`. Direct deletion is recovery-only,
runs only with app-server stopped and the account/auth gate held, and removes only
the revalidated exact credential after user approval.

Layer 1 supports exactly one managed ChatGPT account because one isolated Codex
home owns one primary identity. Other API-key connections remain multiple. An
Engineer-confirmed login to a different ChatGPT account cancels pending login,
blocks new Codex runs, lets already accepted runs reach a governed terminal or be
interrupted, increments the Codex connection execution revision, clears probe and
active-selection approval, and verifies the new account fingerprint before use.
One global Codex account/auth gate serializes login, refresh, logout, account
replacement, config publication, run startup, and home sweeping.

The pinned app-server policy is exact rather than “best effort”:

| Surface | Allowed policy |
| --- | --- |
| Handshake | one `initialize`, one `initialized`; experimental API only when the reviewed tool bridge requires it |
| Account | `account/read`, explicit browser/device `account/login/start`, login cancel/completed, `account/logout`, account/rate-limit updates |
| Discovery | `model/list`; account readiness comes only from `account/read` and documented account notifications |
| Thread | `thread/start` only; exact model, `ephemeral: true`, empty staging cwd, `approvalPolicy: "never"`, read-only sandbox, provider fallback disabled |
| Turn | `turn/start`, exact selected effort and output schema, and `turn/interrupt`; no resume/fork/steer/compact/rollback/injection |
| Output | agent text/reasoning deltas, usage, thread/turn status, and reviewed dynamic-tool call events only |
| Denied | shell/exec/file/patch/web/image/app/plugin/MCP/memory/goal/environment/subagent/collaboration/config-write requests or any unknown method/item type |

The generated 0.149.1 schema must accept `ephemeral: true` and the explicit
provider-fallback denial; otherwise the adapter is incompatible. Where the pinned
runtime exposes a real pre-execution control, Quantix disables memories,
compaction, goals, environments, subagents/multi-agent/collaboration,
skills/plugins/apps, MCP, browser/computer/web/image/code-mode tools,
analytics/feedback, and ambient provider configuration. Any denied or unknown
server request/event terminates the turn before Host execution and records a
redacted protocol-drift failure. Core shell/file behavior is governed by the
test-only limitation below, not claimed disabled by this configuration.

This policy does not prove pre-execution suppression of Codex core shell/file
tools. In 0.149.1, `approvalPolicy: "never"` suppresses prompts rather than
guaranteeing denial, and read-only sandboxing can still allow reads. Therefore the
Codex lane remains test-only and may run live only in a dedicated sanitized Windows
test account or VM with no unrelated sensitive files. A built-in action event
quarantines the run before any Quantix canonical publication, but Quantix does not
claim the internal action was prevented. Codex account mode is incompatible for
ordinary/public Tender work until an exact pre-execution core-tool disable control
or the separately approved OS filesystem sandbox exists.

Codex receives an empty staged working directory and no Tender filesystem path.
Any built-in shell, file, patch, web, app, plugin, collaboration, memory, or
unapproved MCP action is rejected and fails the turn. During private development,
Host-declared dynamic tools may bridge tool calls under the pinned experimental
contract. Public release requires replacing that experimental bridge with the
Host-owned MCP bridge or obtaining explicit approval for the pinned seam.

Codex's documented internal same-request transport recovery is an adapter fact,
not Quantix retry authority. Quantix adds no outer Codex retry and records usage
or attempts that app-server does not expose as unknown.

### 3.2 General Provider Lane

One disposable Python `3.12.13` worker uses the exact uv-locked environment:

- `pydantic-ai-slim[anthropic,google,openai,xai]==2.33.0`;
- official OpenAI Python SDK `3.3.1`;
- official Anthropic Python SDK `1.0.0`;
- official `google-genai` Python SDK `2.19.0`; and
- official xAI Python SDK `1.19.0`.

Pydantic AI provides the common model, streaming, structured-output, toolset,
usage, and error surface while wrapping the vendor SDKs. Direct routes use native
provider models. Compatible routes use the matching official SDK with an explicit
base URL and a capability profile proven by a live bounded probe.

The exact route mapping is:

| Connection route | Pydantic model/provider surface |
| --- | --- |
| Direct OpenAI | `OpenAIResponsesModel` with an explicitly constructed official `AsyncOpenAI` client |
| OpenAI-compatible | `OpenAIChatModel` over Chat Completions; Responses is separate and requires its own exact probe |
| Direct Anthropic | `AnthropicModel` with an explicitly constructed official `AsyncAnthropic` client |
| Anthropic-compatible | `AnthropicModel` with the same Messages contract and hardened custom base URL |
| Gemini | `GoogleModel` with the official `google-genai` Developer API client and the selected in-memory API key |
| xAI | `XaiModel` with an explicitly preconfigured official xAI `AsyncClient` |

The worker is installed non-editably and launched as the exact venv
`python.exe -I -m quantix_ai_worker`. The Host supplies a minimal environment
allowlist; it removes provider credentials, proxy variables, `PYTHONPATH`, user
site settings, `.netrc`/home discovery, cloud ADC/profile variables, CA overrides,
and SDK telemetry. Every SDK client is constructed explicitly with the selected
in-memory credential and `trust_env=false` equivalent. The worker's cwd is an
empty staging directory, and it receives no Quantix database or Tender path.

The private JSONL protocol is retained and versioned. It has one initialize
handshake; exactly one probe or turn; monotonic bounded sequence numbers; typed
model/tool/usage/error frames; correlated Host tool result or denial frames;
optional cancel; byte/frame/aggregate limits; a control lane that remains usable
after operation cancellation; exactly one terminal frame; and final shutdown.
Malformed, duplicate, oversized, out-of-order, late, or post-terminal output fails
closed. Raw stderr is bounded, redacted, and never persisted.

The Host serializes the approved Agent Profile, task, evidence handles, grants,
and budgets. The disposable worker constructs the Pydantic agent. Pydantic may
manage the provider-facing tool loop, but every tool call is deferred to the Host.
The worker cannot issue the next model request until the Host has authorized or
denied the call, reserved idempotency/quota, executed it if allowed, persisted the
result, and reconciled cumulative usage. The hard tool-round ceiling remains 32.
The worker exits after the operation and owns no durable session.

All official-SDK automatic retries are disabled: OpenAI and Anthropic use
`max_retries=0`; Google is configured for one total attempt; xAI receives a
preconfigured `AsyncClient` with `grpc.enable_retries=0` and the SDK's default
retry service config removed;
Pydantic model/output/tool retries are zero. `FallbackModel`, LiteLLM fallback
routing, provider/model substitution, and implicit model defaults are forbidden.
Request-count tests cover every route. Quantix performs no automatic networked
Provider Turn retry. Serialization/policy failures before dispatch may be retried
locally; every failure after connection or write begins is terminal or
indeterminate unless the provider documents an idempotency guarantee used by that
exact request.

### 3.3 MCP Lane

Slice 3 pins official Tier-1 Rust SDK `rmcp = "3.1.4"` and MCP protocol
`2026-07-28`. Its initial topology is Host-as-client for Engineer-installed tool
servers. Quantix does not expose an MCP server or Codex MCP bridge in this cutover.
The Rust Host owns server configuration, process supervision, OAuth references,
tool discovery, schema validation, approval, execution, timeout, result limits,
and audit.

Only the MCP `tools` capability is enabled initially. Resources, prompts,
sampling, elicitation, roots, tasks, subscriptions, and server-initiated
multi-round trips are rejected until separately designed. Local desktop
integrations use stdio. Remote integrations require HTTPS,
bounded Streamable HTTP, explicit authentication, destination validation, and an
Engineer-created connection. Provider-native MCP execution is disabled unless an
exact capability explicitly returns every call through the Host approval boundary.

MCP connection metadata lives in installation SQLite; API keys/OAuth material lives
in the DPAPI vault. Nothing is written to Codex home. Version negotiation accepts
only protocol `2026-07-28`; every older, newer, or unknown version/capability fails
closed. Public Codex tool bridging remains blocked and gets a
separate design before replacing private-development dynamic tools.

There is no in-product public marketplace. Quantix may bundle or internally
configure reviewed MCP servers, but the Engineer sees only installed, approved
connections and capabilities.

### 3.4 Local Retrieval Lane

Existing Rust `fastembed`, SQLite full-text search, and `sqlite-vec` remain the
local retrieval implementation. `multilingual-e5-small` remains a benchmark
candidate for Arabic/English Tender retrieval, not an unquestioned permanent
default. Embedding model selection is explicit and versioned; indexes are
rebuildable and never canonical.

Python and Codex worker protocols receive no retrieval-store path or mutation
method; private-development tests reject every sanctioned attempt. This is not an
OS isolation claim. Workers receive only Host-selected evidence handles and bounded
evidence text. Qdrant, LanceDB, remote
embeddings, or rerankers are added only after representative Arabic/English
construction-tender evaluations prove a material benefit.

## 4. Connection, Credential, and Canonical State

The canonical split is:

- installation SQLite: preferences, non-secret connection summaries, and the
  optional Active AI Configuration reference;
- `~/.quantix/ai-connections.vault`: complete DPAPI-encrypted direct-provider and
  compatible-endpoint and MCP configurations and secrets;
- `~/.quantix/codex/auth.json`: Codex-owned managed ChatGPT credential only;
- Tender SQLite: immutable Agent Run provider binding, events, outcomes, usage,
  permissions, and canonical Tender work; and
- provider workers: no authoritative persistent state.

Quantix never copies Codex tokens into its vault or copies API keys into Codex
home. The renderer receives only redacted readiness, account label/plan when
provided, model/capability metadata, and user-action states.

The Codex account/auth gate is ordered before the existing vault/installation/
Tender locks. App-server and workers start only after store locks are released.
Logout, account replacement, and recovery reject stale in-memory snapshots and
cancel any pending login before publishing a new readiness state.

Changing provider, endpoint, account, credential placement, model, reasoning, or
capability catalogue invalidates the affected approval according to the existing
execution-revision and credential-generation rules. A running Agent Run remains
bound to the exact immutable configuration captured at creation.

## 5. Discovery and Capability Qualification

No provider/model list is hardcoded as product truth.

- Codex uses `model/list` and account/readiness RPCs.
- Direct providers use their official SDK discovery APIs when available.
- A compatible endpoint may omit model listing only when the Engineer supplies an
  exact model ID and it passes qualification.

Only the Engineer-selected connection/model/reasoning/capability tuple must pass a
bounded, disclosed,
possibly billable probe before activation. Qualification records supported,
unsupported, or unknown for:

- authentication and exact destination;
- requested/reported model identity;
- streaming and cancellation;
- tool calling and strict argument validation;
- the exact activation-selected output mode (native JSON Schema, strict tool,
  prompted JSON, or unsupported), with no automatic downgrade;
- reasoning controls;
- usage and request identifiers;
- context/input limits; and
- provider-specific native tools selected by the Engineer.

SDK presence never implies capability parity. A custom compatible endpoint is
qualified feature by feature and fails closed on reroute, protocol drift, or
unknown required behavior.

Compatible endpoints use the official SDK with an injected, provider-neutral
`QuantixGuardedTransport`; passing `base_url` alone is forbidden. The transport
preserves the configured path prefix, sets the exact selected authentication,
disables redirects/proxies/ambient credentials/retries, classifies every DNS
answer and connected address, rejects private/metadata/mixed or destination-class
drift, permits plaintext HTTP only for literal loopback, preserves TLS hostname
verification, and bounds connect/write/read/overall time plus compressed and
decompressed bytes. Discovery, probe, streaming, tool continuation, and ordinary
turn requests must all use the same injected transport. The Host independently
revalidates the destination fingerprint before creating the worker.

Token and monetary budgets are distinct. Quantix ships a versioned pricing
snapshot with provider source URL, currency, unit, effective date, and fetch time;
the exact snapshot identity is recorded on the run. Unknown or stale pricing never
counts as zero: it disables or blocks a monetary budget until the Engineer supplies
and approves compatible-endpoint pricing, while token/tool/time limits still apply.

## 6. Dynamic Agents and Tools

This cutover does not implement Agent Profile CRUD. It preserves the approved Layer
2 contract: future Agent identities are Quantix records, not Python classes or a
static source-code roster. The Manager will create and version complete Arabic
employee profiles, including name, role, biography, communication style,
expertise, instructions, limits, tools, data access, budgets, and escalation
policy. At run time, the Host will serialize the approved profile and exact grants,
and the disposable worker will materialize the ephemeral SDK agent.

Agents communicate through Host-recorded messages and handoffs. Provider-native
handoffs or subagents are treated as proposals and cannot create durable employees,
expand permissions, choose another model, or bypass the Manager. Controlled
subagent creation and recursive swarms remain a later layer with explicit depth,
breadth, cost, time, tool, and data ceilings.

Provider-native web search, deep research, file search, code execution, and other
hosted tools are excluded from this cutover because they can execute provider-side
without per-call Host approval. They require a separate capability design covering
request/call ceilings, evidence and citation capture, data destination, usage,
remote cancellation limits, and indeterminate outcomes before the Engineer or
Manager can enable them.

## 7. Failure, Retry, and Loop Safety

The Host normalizes SDK/runtime errors into typed redacted failures while retaining
safe metadata such as provider request ID, HTTP status class, retry-after value,
model identity, usage, and whether the provider accepted the turn.

An accepted or partially streamed turn with an uncertain terminal outcome is never
retried automatically. It becomes indeterminate and requires the existing recovery
decision. A connection/DNS/TLS/write/read failure is not proof of provider
non-acceptance after dispatch begins. Repeated equivalent tool calls, state hashes,
outputs, failed plans, or non-progress cycles trigger the Quantix loop detector.
Tool-round, token, cost, time, and retry budgets are cumulative across every
continuation.

No local SDK worker, local framework, MCP child process, or Host-authorized
continuation may continue after the Host cancels or exhausts a budget. Quantix
cannot prove that already accepted remote provider or hosted-tool work stopped;
late remote results are rejected and the run becomes indeterminate.

## 8. Framework Placement

The following are not Quantix's core authority:

- OpenAI Agents SDK;
- Claude Agent SDK;
- Google Agent Development Kit;
- Microsoft Agent Framework;
- LangGraph;
- Vercel AI SDK; and
- LiteLLM proxy/router.

They may later run as explicitly installed, disposable, non-authoritative
specialists behind the same Host contract. Microsoft Agent Framework and Pydantic
AI Harness are candidates for a future controlled-swarm evaluation. Their sessions,
memory, tools, approvals, and retries never become canonical without a separately
approved design.

## 9. Removal and Supersession

The cutover removes, rather than preserves:

- `chatgpt-direct-v1` adapter and catalogue identities;
- the private ChatGPT backend URL and direct HTTP/SSE executor;
- Quantix-owned ChatGPT OAuth, refresh, JWT, and token-store code;
- host-managed `chatgptAuthTokens` injection;
- custom provider request/stream parsers replaced by official SDKs;
- hardcoded provider model catalogues and `provider_default` behavior;
- obsolete plaintext application-home `auth.json`; and
- compatibility deserializers, aliases, or fallback paths for these designs.

The 2026-08-21 direct-provider spec becomes historical evidence only. The existing
Layer 1 implementation plans must not be executed after this design is approved;
a new implementation plan will replace them.

## 10. Acceptance

Completion requires deterministic and live evidence for all of the following:

1. The exact Codex executable, package, generated schema, and hashes are pinned and
   verified before execution.
2. Managed browser and device-code login work through app-server fixtures and an
   opted-in live Windows run; restart reuses the isolated Codex credential.
3. No Quantix OAuth/token parser, `chatgptAuthTokens`, private ChatGPT URL, or
   `chatgpt-direct-v1` production reference remains.
4. The same small Test Project completes one real Codex Agent Run using the
   currently discovered, explicitly selected model/reasoning pair that satisfies
   the required capability profile, and records that exact identity before
   publishing a validated Manager outcome.
5. Disconnect/logout and corrupt-auth recovery remove only the validated Codex
   credential; no secret reaches SQLite, Tender data, renderer state, logs,
   diagnostics, backups, archives, argv, environment, or worker output.
6. OpenAI, Anthropic, Gemini, xAI, OpenAI-compatible, and Anthropic-compatible
   routes use pinned official SDKs through the worker, with exact request-count
   and retry-disabled contract tests.
7. All seven routes preserve explicit selection, immutable run binding, cumulative
   budgets, reroute detection, cancellation, and zero provider/model fallback.
8. Codex/Python workers receive no Quantix SQLite, source-package, retrieval-index,
   or arbitrary Tender path through any sanctioned code path; private acceptance
   demonstrates empty staging, sanitized environments, protocol denial, and file-
   access attempts failing where current sandbox/ACL policy can enforce them. This
   evidence is not represented as same-user OS isolation.
9. Only Host tool implementations execute after Host approval; duplicate calls are
   idempotent and every denial, approval, result, and timeout is attributable. A
   Codex built-in action quarantines the run before canonical publication, and the
   evidence explicitly does not claim pre-execution suppression inside Codex.
10. The official Rust MCP client passes pinned protocol/transport tests; malicious
    servers, oversized frames, protocol drift, and every non-tool capability fail
    closed, and no MCP call bypasses Host policy.
11. FastEmbed/SQLite retrieval remains intact and rebuildable, with no canonical
    state owned by an embedding/vector library.
12. Full repository verification and private Windows acceptance pass without
    production builds being run during ordinary development.

Additional deterministic acceptance covers:

- ephemeral Codex threads and no persisted Tender content in Codex home;
- ambient API keys, proxies, `.netrc`, ADC/cloud profiles, and CA overrides being
  absent or ignored;
- concurrent login/run/logout/account replacement and stale snapshots;
- config/schema drift and every unexpected app-server method/item type;
- crash-time transient-home sweeping without deleting valid managed auth;
- hostile, oversized, duplicate, out-of-order, late, and post-terminal worker IPC;
- exact OpenAI/Anthropic/Google/xAI request counts, including xAI gRPC attempts;
- remote hosted-tool cancellation after acceptance becoming indeterminate; and
- pricing absent/stale/unknown behavior never bypassing a monetary ceiling.

## 11. Release Boundary

Managed ChatGPT login is a documented app-server capability and does not inherit
the old experimental external-token gate. ADR 0018 supersedes that part of ADR
0017. Account-backed release still requires license, terms, distribution,
OS-enforced filesystem isolation, built-in-tool denial, and private-data handling
review for the exact pinned Codex runtime.

Any remaining experimental dynamic-tool seam remains private-development only.
This cutover does not claim a public Codex MCP bridge; public account activation
therefore remains blocked until that bridge receives a separate approved design
or explicit written approval covers the exact dynamic-tool seam. General-provider
routes have their own provider terms, data-destination disclosures, and live
qualification gates.

## 12. Primary Sources

- [OpenAI Codex app-server](https://learn.chatgpt.com/docs/app-server)
- [OpenAI Codex SDK](https://learn.chatgpt.com/docs/codex-sdk)
- [OpenAI SDKs](https://developers.openai.com/api/docs/libraries)
- [OpenAI Responses API](https://developers.openai.com/api/reference/resources/responses/methods/create)
- [Official Codex configuration schema](https://github.com/openai/codex/blob/main/codex-rs/core/config.schema.json)
- [Pydantic AI providers](https://pydantic.dev/docs/ai/models/overview/)
- [Pydantic AI MCP](https://pydantic.dev/docs/ai/mcp/overview/)
- [Anthropic Python SDK](https://platform.claude.com/docs/en/cli-sdks-libraries/sdks/python)
- [Anthropic structured outputs](https://platform.claude.com/docs/en/build-with-claude/structured-outputs)
- [Google Gen AI Python SDK](https://github.com/googleapis/python-genai)
- [xAI Python SDK](https://github.com/xai-org/xai-sdk-python)
- [Official MCP SDKs](https://modelcontextprotocol.io/docs/sdk)
- [Official Rust MCP SDK](https://github.com/modelcontextprotocol/rust-sdk)
- [FastEmbed](https://qdrant.tech/documentation/fastembed/)
- [sqlite-vec](https://github.com/asg017/sqlite-vec)
- [Microsoft Agent Framework](https://learn.microsoft.com/en-us/agent-framework/overview/)
