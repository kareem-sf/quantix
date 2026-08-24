---
status: accepted
supersedes:
  - 0016-connect-chatgpt-through-quantix-owned-oauth
  - 0014-scope-ai-execution-and-asa-operations-per-tender#ai-selection
  - 0009-run-one-local-host-over-self-contained-tender-stores#provider-runtime
  - 0010-qualify-v0-through-layered-product-acceptance#provider-evidence
---

# Run multiple AI connections through one active configuration

Quantix stores several Engineer-configured AI connections but uses exactly zero
or one global Active AI Configuration. The Engineer explicitly tests and selects
the exact connection revision, provider, model, reasoning state, runtime, and
capability catalogue. Quantix supplies no connection, provider, model, reasoning,
or fallback default.

The Rust Host remains the only workflow, permission, approval, tool-execution,
budget, persistence, validation, recovery, audit, and canonical-state authority.
Providers supply intelligence only. Each Agent Run immutably captures the Active
AI Configuration when the run is created; later activation changes affect only
future runs.

## Decision

### Account-backed Codex

The account-backed product integration is the official Codex app-server 0.149.1,
driven directly by the Rust Host over supervised stdio JSON-RPC. The Codex Python
SDK is not Quantix's production security boundary. App-server exposes the
lifecycle, streaming, interruption, account, tool, and approval protocol that the
Host must police without placing a second framework between those control events
and Quantix.

Quantix may privately prototype the 0.149.1 `chatgptAuthTokens` seam solely to
pass Host-owned, encrypted-vault-backed access tokens into that exact pinned
app-server. The generated schema describes the seam as unstable,
OpenAI-internal-only, and “do not use.” Account activation is therefore
non-shippable without written OpenAI approval for the Quantix client and this
integration. Existing browser/device OAuth and token-refresh implementation moves
behind `ai::codex_auth` at cutover; no legacy authentication path remains.

Quantix never constructs or calls the private ChatGPT execution backend. The
official Codex runtime owns its upstream service route as an opaque implementation
detail. Removal, denial, or schema drift of the experimental seam makes only the
account connection incompatible. It never invokes the removed private adapter or
another connection.

The Host supplies only an empty staged workspace and Host-declared dynamic tools.
Any built-in command, patch, file, web, MCP, app, permission, or collaboration
request is a security failure. The pinned runtime's read-only sandbox cannot
restrict readable roots, so account mode also remains non-shippable until the
approved integration satisfies the required filesystem boundary.

Codex's reserved account provider owns fixed bounded transport recovery: four
request retries and five interrupted-stream retries. Quantix cannot override
these without replacing the sanctioned provider route, adds no outer retry, and
records unavailable internal retry usage as unknown.

### General providers

Direct OpenAI, Anthropic, Google Gemini, and xAI keys and custom OpenAI- or
Anthropic-compatible endpoints run through a disposable Python 3.12.13 worker
using `pydantic-ai-slim[anthropic,google,openai,xai]==2.33.0`.

Pydantic AI is adopted only inside that general-provider worker. The newly
approved second-provider condition satisfies the revisit trigger that made the
old v0 rejection obsolete. Pydantic AI supplies the model-facing stream,
structured-output, provider normalization, and external-tool loop; it is not a
Host workflow, orchestration, approval, memory, retry, fallback, durability, or
canonical-state engine. External tools always return to the Host for permission
and execution.

Provider SDK retries and Pydantic tool/output retries are disabled. The Host may
authorize one bounded same-revision, same-model transient retry only when no
uncertain side effect exists. `FallbackModel` and every cross-connection or
cross-model fallback are forbidden.

### Credentials and vault

All complete connection configuration and persistent secret material **at rest**
exists only in the current-user-DPAPI-encrypted
`~/.quantix/ai-connections.vault`. During an authorized operation, the exact Rust
Host and assigned worker may hold the selected secret in memory and private IPC.
Secrets never persist in renderer state, SQLite, Tender Stores, logs,
diagnostics, backups, archives, exports, process arguments, process environments,
`auth.json`, a keyring, or Credential Manager.

## Supersession scope

- ADR 0016 is superseded in full. Its browser/device OAuth validation work moves
  behind `ai::codex_auth`, while its direct private-backend execution and
  plaintext `auth.json` paths are removed.
- ADR 0014 remains accepted for Tender-scoped work, immutable Agent Run binding,
  Host authority, and fail-closed execution. Only its per-Tender AI-selection
  consequences are superseded by one optional global Active AI Configuration.
- ADR 0004 and ADR 0008 retain their Host-owned permission, candidate-validation,
  run-boundary, and canonical-state principles. Only their former Codex process,
  persistent-thread, login, and runtime consequences are superseded.
- ADR 0009 retains the Rust Host, single writer, Tender Stores, process
  supervision, and native qualification consequences. Its direct-HTTPS ChatGPT
  and cross-platform general-worker consequences are superseded; Layer 1 targets
  Windows 11 x64 and uses the two worker boundaries above.
- ADR 0010 retains layered deterministic, live-provider, native-package, and
  public-release gates. Its direct-ChatGPT-only fields are replaced by exact
  connection revision, model, catalogue, adapter/runtime, and route evidence.
- ADR 0012's no-silent-fallback and Host-authority principles remain binding; its
  historical catalogue is not revived as a compatibility contract.
- ADR 0005's fixed Bootstrap Team is superseded by the approved controlled modular
  agent-platform design, but remains the implementation fact until Layer 2.
  Layer 1 does not create a compatibility roster.

## Consequences

- A connection is not activated by creation, discovery, or testing. Activation
  is a separate attributable Engineer action after the exact model/reasoning pair
  passes its bounded probe.
- Editing execution identity increments the connection revision and invalidates
  probe evidence. Verified same-account token rotation changes only credential
  generation and preserves the semantic revision.
- A disabled, disconnected, stale, incompatible, or failed active connection
  blocks new AI work. Quantix never chooses a substitute.
- Codex app-server schemas and the Python worker environment are exact-version,
  generated/locked, contract-tested runtime inputs rather than provider defaults.
- No runtime behavior changes until the Layer 1 cutover installs these boundaries
  and removes the superseded paths.

## Rejected alternatives

- **Codex Python SDK as the account boundary:** rejected because it adds a wrapper
  while app-server is the product-integration protocol whose account, lifecycle,
  tool, approval, and event surfaces the Host must validate directly.
- **Private ChatGPT backend calls:** rejected because Quantix must not construct,
  depend on, or claim support for OpenAI's private execution route.
- **Pydantic AI in the Rust Host or as a durable workflow engine:** rejected
  because it would create competing orchestration and state authority.
- **Vercel AI SDK worker:** rejected because it adds a Node 22 runtime and a second
  packaging/update path without improving the Host boundary.
- **Raw Rust provider adapters:** rejected because Quantix would own six changing
  request, streaming, tool-delta, structured-output, and error protocols.
- **Per-Tender selection, default selection, compatibility roster, or fallback
  chain:** rejected because each would obscure the Engineer's one explicit global
  choice or preserve obsolete Layer 1 behavior.

## Release gate

Account login and account-backed execution remain private-development capability
until OpenAI approves the Quantix client/integration in writing, the pinned
experimental seams remain contract-compatible, and the required filesystem
boundary is demonstrated. Protocol removal or denial disables only the account
connection. Public release still requires the layered gates preserved from ADR
0010; technical qualification cannot waive provider authorization.

## Primary-source evidence

- [OpenAI: Codex as a platform](https://developers.openai.com/blog/codex-as-a-platform)
- [OpenAI Codex app-server](https://learn.chatgpt.com/docs/app-server)
- [OpenAI Codex SDK](https://learn.chatgpt.com/docs/codex-sdk)
- [OpenAI Codex authentication](https://learn.chatgpt.com/docs/auth)
- [OpenAI Codex source and Apache-2.0 license](https://github.com/openai/codex)
- [Pydantic AI model providers](https://pydantic.dev/docs/ai/models/overview/)
- [Pydantic AI deferred tools](https://pydantic.dev/docs/ai/tools-toolsets/deferred-tools/)
- [Pydantic AI retries](https://pydantic.dev/docs/ai/retries/)
- [Vercel AI SDK providers and models](https://ai-sdk.dev/docs/foundations/providers-and-models)
- [Layer 1 AI connection foundation design](../superpowers/specs/2026-08-24-layer-1-ai-connection-foundation-design.md)
- [AI worker runtime selection](../research/ai-worker-runtime-selection.md)
