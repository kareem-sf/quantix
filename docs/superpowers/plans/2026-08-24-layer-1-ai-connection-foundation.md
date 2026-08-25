# Layer 1 AI Connection Foundation Implementation Plan

> **Superseded — do not execute.** ADR 0018 and the
> [SDK-first runtime design](../specs/2026-08-25-sdk-first-ai-runtime-cutover-design.md)
> replace its account-authentication and worker/runtime assumptions. A new plan
> will be written after the revised design is approved.

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the current ChatGPT-only, private-backend, per-Tender AI path with four Engineer-configured connection methods, one explicit global Active AI Configuration, a DPAPI-encrypted vault, supervised provider workers, normalized provider behavior, and no fallback.

**Architecture:** The Tauri Rust Host remains the only authority and writer. It drives a pinned official Codex app-server directly for account-backed execution and a run-scoped Pydantic AI Python worker for direct and compatible providers. Secrets are decrypted only in Host memory and sent over private supervised stdio to the exact assigned worker. The renderer receives credential-free projections. Every Agent Run captures the exact active connection revision and capability evidence before execution.

**Tech Stack:** Windows 11 x64, Rust 1.97.1, Tauri 2, SQLite, Windows DPAPI, `openidconnect` 4.0.1, ProcessKit, Tokio, official Codex app-server 0.149.1, Python 3.12.13 managed by uv 0.12.2, `pydantic-ai-slim[anthropic,google,openai,xai]==2.33.0`, React 19, TypeScript 7, Vitest, `ts-rs`.

**Spec:** `docs/superpowers/specs/2026-08-24-layer-1-ai-connection-foundation-design.md`, refining the approved master design's delivery layer 1.

## Global Constraints

- Quantix ships with zero selected connection, provider, model, reasoning level, or fallback.
- Support exactly these connection methods in Layer 1: Codex account login, direct provider key, custom OpenAI-compatible endpoint, and custom Anthropic-compatible endpoint.
- Direct provider keys support OpenAI, Anthropic, Google Gemini, and xAI. Compatible endpoints are protocol claims, not capability claims.
- Several connections may be saved, but exactly zero or one global Active AI Configuration exists. The Manager, workers, memory review, and later workshops cannot alter it.
- A model is always chosen explicitly. If a tested model exposes reasoning choices, the Engineer must choose one; if the probe proves reasoning is unsupported, the stored selection is explicitly `unsupported`. There is no `provider_default` choice.
- Reasoning choices come only from the adapter's current provider metadata/profile. Test validates the exact Engineer-selected model/reasoning pair; another effort remains untested. A compatible endpoint with no standard advertised reasoning control may be activated only with probe evidence recorded as `unsupported`, never an invented option.
- Editing execution-relevant connection data creates a new connection revision, clears its probe evidence, and makes any active reference to the old revision stale. Renaming alone does not change the execution revision. A verified same-account OAuth access/refresh rotation increments only `credential_generation`; it must not stale the active semantic revision or probe.
- Disabling or disconnecting the active connection leaves the exact active reference unavailable; it never selects another connection. Deleting an active connection or one used by a nonterminal run is rejected.
- Disabling affects future runs and lets an already executing run reach its bounded terminal. Disconnect removes the credential, cancels every nonterminal provider operation on that connection, invalidates its pending tool/access approvals, and rejects late tool results; uncertain side effects remain indeterminate.
- Active workers keep their immutable in-memory configuration snapshot. They may accept a verified same-account token rotation through `credential_generation`. A restart may use the latest credential generation for the exact recorded semantic revision/account; if that revision cannot be resolved, recovery blocks and never rewrites history to the current revision.
- All persistent secret material at rest exists only in `~/.quantix/ai-connections.vault`, encrypted by user-scoped DPAPI. Never use Credential Manager, a keyring, `auth.json`, environment variables, command-line arguments, machine-scoped DPAPI, or a user password.
- The account credential retains only the refresh token, current access token, expiry, and required account identifier. Parse then discard ID-token bytes after verified account/plan projection unless the pinned provider contract proves they are required for refresh.
- During setup, the renderer may hold a newly typed secret only in the active form until its one write-only command settles, then clears it. Existing/selected provider credentials are never returned. During execution, only the Rust Host and the single supervised worker for the granted operation may hold the selected credential in memory or private stdio. Long-lived credentials never reach renderer results, Tender Stores, diagnostics, logs, backups, archives, exports, process arguments, or process environments.
- A device-login verification URL and one-time user code may be returned only to the initiating UI while that login attempt is active. They are cleared on success, cancellation, timeout, disconnect, navigation, or restart and are never persisted or logged.
- The Codex adapter uses the official app-server integration intended for agents embedded in products. In 0.149.1, host-managed `chatgptAuthTokens` is explicitly marked unstable, OpenAI-internal-only, and “do not use”; `dynamicTools` is experimental. Quantix may prototype them privately only behind exact contract tests and must not ship account activation without written OpenAI approval for this use. Removal, denial, or drift makes the affected account capability incompatible and never triggers the old private backend or another provider.
- The general worker uses Pydantic AI only for provider normalization, streaming, structured output, and externally executed tools. It owns no Tender workflow, durable task, approval, memory, retry policy, fallback, or canonical state.
- General-provider SDK automatic retries, Pydantic tool/output retries, and every fallback model are disabled. Codex 0.149.1's reserved built-in account provider owns fixed bounded transport recovery (four request retries and five interrupted-stream retries) that cannot be overridden without replacing the sanctioned provider route; Quantix records this exact adapter limitation, adds no outer Codex retry, and treats absent usage as unknown.
- Layer 1 starts a fresh supervised worker scope per probe or Provider Turn; it adds no worker pool. Later pooling may reuse only scopes with identical user, connection revision, tools, workspace, and sandbox policy.
- Preserve the existing per-run request/token/tool/time/output budget and 32-tool-round hard ceiling across both workers. Crossing a ceiling interrupts the exact turn and never starts a replacement. Layer 2 adds semantic Loop Guard detection on top of these Layer 1 hard stops.
- The Windows Job Object proves descendant lifetime cleanup; it is not a filesystem or network sandbox. Exact run workspaces, cleared environments, provider client network policy, disabled native tools, Host grants, and fail-closed event handling provide the other boundaries.
- Custom endpoints require HTTPS except literal `127.0.0.1` or `[::1]` HTTP. Reject `localhost`, userinfo, fragments, embedded query strings, and connect-time resolutions containing unspecified, multicast, broadcast, link-local/metadata, IPv4-mapped, or mixed public/private address classes. Preserve the configured path prefix when joining API routes, pin its destination class from probe to turn, never follow redirects, and never return header/query values to the renderer.
- Compatible credentials are either standard bearer authorization or a dedicated named API-key header, matching the approved API-key/bearer contract. Authentication headers cannot also be entered as ordinary custom headers; all credential/header/query values stay write-only and encrypted. Unauthenticated compatible endpoints are outside Layer 1.
- Missing `/models` on a compatible endpoint is not failure when the Engineer supplied a model ID and the exact model probe succeeds.
- Installation schema moves from 25 to 26 and Tender schema from 45 to 46 at the cutover. There are no migrations, aliases, compatibility deserializers, or old-schema fallback paths.
- Preserve the immutable `agent_run_provider_bindings` evidence table. Delete the mutable per-Tender AI selection and rebind paths.
- Generated declarations under `src/bindings` are regenerated from Rust and never edited manually.
- No production build runs during normal implementation. Live-provider tests are opt-in and are not part of deterministic verification.

## Locked Framework Decision

Use the official Codex app-server directly for the account connection, and one isolated uv-locked Python environment for general providers:

```toml
[project]
name = "quantix-ai-worker"
version = "0.0.0"
requires-python = "==3.12.13"
dependencies = [
  "pydantic-ai-slim[anthropic,google,openai,xai]==2.33.0",
]

[build-system]
requires = ["uv_build==0.12.2"]
build-backend = "uv_build"

[tool.uv.build-backend]
module-root = ""
module-name = "quantix_ai_worker"
```

Why this is the smallest durable choice:

- OpenAI positions app-server, rather than the simplified SDK, for a Codex agent embedded in a product that needs open conversations, streamed events, interruption, tools, and approval handling.
- Stage the official npm package `@openai/codex` at version `0.149.1-win32-x64` by exact integrity `sha512-G3QXGAg7nyyhqOeooAMUekBCeHd8a1QByhKcVAFyzNBaI06t6Ft7nsF+1SzFS0spuIdU4YyMi5YD26ukADBQUQ==` and archive SHA-256 `513bde2e7a1fe31e9b7ab2c9ec1dc87e54eb93d3adc5ae579452a7f0c09e9ed2`; the prepared executable hash is recorded separately in runtime provenance.
- Pydantic AI supplies maintained OpenAI, Anthropic, Google, xAI, and compatible-provider adapters plus external/deferred tool execution. It runs as a disposable provider worker, not as Quantix's orchestrator.
- Quantix already provisions verified uv and Python 3.12.13. Vercel AI SDK would add a separate Node 22 runtime and a second packaging/update path without improving the Host boundary.
- Raw Rust HTTP adapters would make Quantix own six changing request, streaming, tool-delta, structured-output, and error protocols.

The lockfile, not loose transitive constraints, is authoritative. The researched runtime resolution includes OpenAI 3.3.1, Anthropic 1.0.0, Google Gen AI 2.19.0, xAI SDK 1.19.0, Pydantic 2.13.4, Pydantic Core 2.46.4, Pydantic Graph 2.33.0, HTTPX2 2.12.0, and Tiktoken 0.14.0; the checked-in final lock also pins the isolated build backend.

## Final File Structure

```text
src-tauri/src/
  ai/
    mod.rs                    # public Layer 1 domain surface
    codex_auth/
      mod.rs                  # connection-scoped login lifecycle
      authorize.rs            # browser PKCE request
      callback_server.rs      # bounded loopback callback
      crypto.rs               # PKCE/state generation
      device.rs               # explicit device-code fallback
      jwt.rs                  # account/plan claim extraction
      tokens.rs               # exchange and in-memory refresh
    connections.rs            # CRUD, probes, secret-free views, active selection
    contract.rs               # connection, capability, event, usage, failure DTOs
    codex_worker.rs           # strict app-server JSON-RPC adapter
    probe.rs                  # capability evidence and catalogue hashing
    runtime.rs                # managed worker environment and provenance
    vault.rs                  # versioned vault, lock, atomic replacement
    windows_dpapi.rs          # narrow CryptProtectData/CryptUnprotectData wrapper
    worker.rs                 # supervised JSONL client and exact adapter dispatch
  application_settings.rs     # general preferences and non-secret active reference only
  agent_runtime.rs            # Agent Run lifecycle consuming ai::contract
  process_supervisor.rs       # production conversation supervision
  setup.rs                    # exact schema 26 and vault/runtime ownership
  tender_store.rs             # exact schema 46; immutable run binding retained

src-tauri/runtime/ai/
  pyproject.toml
  uv.lock
  THIRD_PARTY_LICENSES.json
  quantix_ai_worker/
    __init__.py
    __main__.py
    general_adapter.py
    host_bridge.py
    model_factory.py
    probe.py
    protocol.py
  tests/
    __init__.py
    fakes.py
    test_general_adapter.py
    test_probe.py
    test_protocol.py

src-tauri/runtime/
  THIRD_PARTY_NOTICES.txt       # bundled Codex and Python distribution notices

src-tauri/tests/
  ai_connection_repository.rs
  ai_connection_vault.rs
  ai_worker_contract.rs
  fixtures/codex_app_server_protocol.schemas.json # regenerated by Codex 0.149.1
  fixtures/ai_worker/*.jsonl
  support/ai_worker_fixture.rs

src/
  AiConnectionsSettings.tsx
  AiConnectionsSettings.test.tsx
  AiConnectionForm.tsx
  AiConnectionList.tsx
  ActiveAiConfigurationControl.tsx
  aiSettingsCopy.ts
  applicationAiSelectionReadiness.ts
  ApplicationSettings.tsx
  quantixHost.ts

docs/
  adr/0017-run-multiple-ai-connections-through-one-active-configuration.md
  research/ai-worker-runtime-selection.md
```

Obsolete production files deleted at cutover:

```text
src-tauri/src/agent_backend/
src-tauri/src/agent_runtime/codex_actor.rs
src-tauri/src/agent_runtime/codex_protocol.rs
src-tauri/src/chatgpt_login.rs
src-tauri/src/chatgpt_oauth/
```

Layer 1 local layout:

```text
~/.quantix/
  ai-connections.vault
  ai-connections.vault.lock
  installation.sqlite
  runtimes/
    codex/0.149.1/vendor/        # exact verified relative runtime tree
    ai/                         # isolated uv environment
  runtime-work/
    codex/0123456789abcdef0123456789abcdef/ # example always-swept operation state
  tenders/11111111111111111111111111111111/runs/22222222222222222222222222222222/
    workspace/                  # exact permission-granted inputs/outputs
```

No worker writes credentials below `runtimes`, `runtime-work`, or a Tender. Tender
backup traversal never enters `runtime-work`.

## Plan Suite and Dependency Order

1. [`Layer 1A — Contract, Vault, and Active Configuration`](./2026-08-24-layer-1a-ai-contract-vault-active-configuration.md)
2. [`Layer 1B — Supervised AI Workers`](./2026-08-24-layer-1b-supervised-ai-workers.md)
3. [`Layer 1C — Runtime and Settings Cutover`](./2026-08-24-layer-1c-ai-runtime-and-settings-cutover.md)
4. [`Layer 1D — Security and Acceptance`](./2026-08-24-layer-1d-ai-security-and-acceptance.md)

The first two plans add tested foundations while the current application remains usable. Layer 1C performs one vertical cutover: new commands and UI are prepared first, then the runtime, schemas, generated contract, and mounted UI switch together while obsolete paths are deleted. Layer 1D proves the result and updates shipped-capability documentation.

- [ ] Complete and review Layer 1A; its targeted tests and repository checks pass.
- [ ] Complete and review Layer 1B; worker unit/contract/runtime tests and repository checks pass.
- [ ] Complete and review Layer 1C; the new end-to-end deterministic connection flow works and every legacy production path is absent.
- [ ] Complete and review Layer 1D; the security matrix, deterministic acceptance, documentation audit, and `npm run verify` pass.

## Cross-Store and Revision Rules

The vault is authoritative for connection revisions and test evidence. SQLite is authoritative only for the optional non-secret Active AI Configuration reference. No operation needs a distributed transaction:

The only allowed multi-store lock order is connection/account-auth gate → in-process vault mutex → cross-process vault file lock → installation SQLite immediate transaction → Tender SQLite immediate transaction. Settings reads use the same order when they need both stores. No path acquires an earlier lock while holding a later one.

1. Create/edit/test/enable/disable/disconnect mutates only the locked vault.
2. Activation holds the vault lock, validates the exact ready revision/model/reasoning/catalogue, then commits its reference to SQLite.
3. A crash before the SQLite commit leaves the prior active reference unchanged.
4. A later material vault edit makes the reference stale by revision/hash comparison and blocks new work.
5. Delete is allowed only after proving the connection is neither active nor referenced by a nonterminal run, so it cannot create an actionable dangling reference.
6. Startup projects stale/missing/disabled credentials as a typed unavailable state; it never clears the audit evidence or selects another connection.
7. Run preparation copies the exact selected secret into zeroizing Host memory and commits the credential-free run binding under the fixed order, then releases every store lock before spawning or writing to a worker.

## Worker Contract Invariants

- Protocol version is `1`; transport is newline-delimited UTF-8 JSON over private stdio.
- Codex app-server keeps its documented JSON-RPC protocol; `ai/codex_worker.rs` translates it directly into the same Quantix-owned semantic contract.
- Every frame has a bounded size, exact `request_id`, strict unknown-field rejection, and monotonic event `sequence`.
- Exactly one final frame of type `terminal` or `failure` is required; `failure` is terminal. Any frame after either final kind, duplicate sequence number, or gap is a protocol failure.
- General-provider secrets occur only in the first Host-to-worker operation frame. Codex external tokens occur only in the exact account login/refresh JSON-RPC exchange. Worker-to-Host projections and normalized events are structurally incapable of carrying credential fields.
- Allowed Host frames are one `initialize` handshake, exactly one primary `probe` or `turn_start`, zero or more correlated `tool_result` frames, optional `cancel`, and final `shutdown`.
- Quantix executes only a dynamic/external tool declared in the exact Provider Turn Request. The Host independently authorizes and executes it; a returned tool denial is ordinary model-visible data, not authority. If a provider exposes or invokes any built-in action, Quantix treats the turn as a security failure and interrupts it.
- Cancellation first asks the adapter to interrupt. If no terminal interruption arrives within the fixed grace period, ProcessKit terminates the complete Windows Job Object tree.
- Any provider-reported reroute/model substitution is a visible incompatibility failure. It is never accepted as the chosen model.

## Layer 1 Safety Limits

- At most 32 saved connections; display names are 1–120 UTF-8 bytes, may repeat, and never serve as identity.
- Endpoint URLs are at most 2,048 bytes; model IDs 1–256 bytes; one credential at most 16 KiB.
- At most 16 custom headers and 16 custom query parameters; names are 1–128 ASCII bytes and values at most 4 KiB each.
- Ordinary custom headers reject `Authorization`, `Proxy-Authorization`, `Host`, `Connection`, `Keep-Alive`, `Transfer-Encoding`, `TE`, `Trailer`, `Upgrade`, `Content-Length`, `Cookie`, `Set-Cookie`, `Forwarded`, every `Proxy-*`, and every `Sec-*` name. Dedicated credential handling owns authentication headers.
- Activation confirmation shows the canonical punycode scheme/host/port/path-prefix destination without userinfo, query values, or credential/header values.
- Decrypted vault JSON is at most 4 MiB and DPAPI ciphertext at most 8 MiB.
- A normalized model catalogue contains at most 500 entries. More entries or incomplete pagination makes discovery incomplete; an explicit model may still be probed without selecting a default.
- One connection test may make at most six model requests, send at most 4 KiB of synthetic probe input, request at most 1,024 output tokens total, and run for at most three minutes. The UI discloses possible usage before starting.
- One worker JSONL frame is at most 1 MiB, cumulative stdout per operation 16 MiB, and stderr 256 KiB. Smaller Agent Run budgets always win.
- Each Windows worker Job Object has a 1 GiB committed-memory ceiling. General workers allow at most two live processes; Codex workers allow at most four. Failure to enforce either limit blocks the operation.
- Browser login expires after five minutes; device-code login expires after fifteen minutes. Cancellation is available throughout.

## Completion Evidence

Layer 1 is complete only when all of the following are true:

- A clean setup contains no provider/model/reasoning selection.
- Each of the four methods can be created, tested, edited, disabled/disconnected, and deleted under its exact rules.
- A tested model can be selected explicitly as the one global Active AI Configuration.
- A failing active connection blocks new AI work without invoking another connection or model.
- An active run keeps revision A after the Engineer activates revision B; the next run captures B.
- Vault corruption, DPAPI failure, protocol drift, worker crash, timeout, cancellation, unsupported capability, and model reroute all fail closed with redacted typed errors.
- After the write-only submission settles, a sentinel credential is absent from plaintext vault bytes, SQLite, Tender Stores, logs, diagnostics, backups, archives, renderer DOM/state/returned data, process arguments, process environments, and worker output.
- The private ChatGPT backend URL, plaintext auth store, ChatGPT-only settings contract, and per-Tender selection/rebind implementation have zero production references.
- `npm run verify` passes from a clean working tree and a second binding export produces no diff.

## Primary Research References

- [OpenAI Codex SDK](https://learn.chatgpt.com/docs/codex-sdk)
- [OpenAI Codex app-server](https://learn.chatgpt.com/docs/app-server)
- [OpenAI Codex authentication](https://learn.chatgpt.com/docs/auth)
- [Pydantic AI model providers](https://pydantic.dev/docs/ai/models/overview/)
- [Pydantic AI deferred tools](https://pydantic.dev/docs/ai/tools-toolsets/deferred-tools/)
- [Microsoft CryptProtectData](https://learn.microsoft.com/windows/win32/api/dpapi/nf-dpapi-cryptprotectdata)
