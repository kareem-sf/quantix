---
status: accepted
amends:
  - 0017-run-multiple-ai-connections-through-one-active-configuration.md
supersedes:
  - ../superpowers/specs/2026-08-21-chatgpt-direct-provider-design.md
---

# Adopt SDK-first AI runtime boundaries

Quantix replaces provider-specific custom protocols with official provider SDKs
and runtimes while retaining the Rust Host as the only workflow, permission,
approval, budget, audit, recovery, and canonical-state authority.

This decision amends ADR 0017 only where that ADR requires Quantix-owned ChatGPT
OAuth, experimental `chatgptAuthTokens`, vault-only Codex credentials, a
credential-free disposable `CODEX_HOME`, or the release gate tied to that
experimental token seam. ADR 0017 remains accepted for multiple Engineer-created
connections, one optional explicit global Active AI Configuration, immutable
Agent Run binding, no defaults, no silent fallback, the encrypted general-provider
vault, and Host authority.

## Decision

### Managed ChatGPT/Codex

The account-backed route is official Codex app-server `0.149.1` over supervised
stdio JSON-RPC. App-server owns documented managed ChatGPT browser/device login,
token persistence/refresh, account state, model discovery, and upstream transport.
Quantix does not construct a private ChatGPT execution URL and does not implement
ChatGPT OAuth, JWT/token parsing, refresh, or `chatgptAuthTokens` injection.

Codex runs with an isolated persistent home at the validated absolute path
`<Application Home>\codex` and file credential storage. `auth.json` and the
Quantix-generated `config.toml` are the only approved persistent content. This is
an explicit exception to ADR 0017's vault-only credential rule and the user's
requirement to avoid Credential Manager/keyring storage; the files remain under
`~/.quantix` with restrictive current-user permissions and are excluded from every
backup, archive, export, Tender store, log, diagnostic bundle, and renderer view.

Layer 1 supports one managed ChatGPT account. Explicit account replacement changes
the connection revision and invalidates prior probe/active approval. Normal logout
uses app-server; direct credential deletion is stopped-process recovery only.

App-server receives an empty staging cwd and a deny-by-default protocol/config
allowlist. Provider-native filesystem/process/web/app/plugin/memory/subagent/
collaboration surfaces are denied. The current same-user Windows boundary cannot
prove read isolation; account mode remains private-development only until an
OS-enforced filesystem boundary or approved equivalent is separately proven.

### General providers

One disposable Python `3.12.13` worker uses Pydantic AI `2.33.0` over the pinned
official OpenAI, Anthropic, Google Gen AI, and xAI Python SDKs. Compatible endpoints
use the matching official SDK with a Quantix-injected hardened transport. No SDK,
Pydantic, gateway, provider, or model fallback/retry/default may bypass the Host.

### MCP and retrieval

The official Rust `rmcp = "3.1.4"` SDK and MCP protocol `2026-07-28` are the only MCP
implementation boundary. The initial scope is Host-as-client, stdio/local or
guarded remote tool servers, tools only. Provider-native MCP authority and a Codex
MCP bridge require separate approval.

Existing FastEmbed, SQLite FTS, and `sqlite-vec` retrieval remain Host-owned,
rebuildable, and non-canonical. Provider workers receive only selected evidence,
not database/index paths.

## Consequences

- `chatgpt-direct-v1`, the private ChatGPT backend, custom ChatGPT OAuth, and custom
  provider stream parsers are removed without aliases or compatibility fallbacks.
- General-provider keys and compatible-endpoint secrets remain in the DPAPI-
  encrypted `~/.quantix/ai-connections.vault`; Codex managed auth remains only in
  its isolated Codex home.
- Every SDK client is explicitly constructed with ambient credentials/proxies and
  automatic retries disabled. Networked turns are not automatically retried after
  dispatch begins.
- Agent Profiles, Arabic personas, Manager grants, tool approvals, communication,
  memory authority, loop limits, and future swarm ceilings remain Quantix records.
- Provider agent frameworks may be disposable specialists but never a competing
  durable or canonical authority.
- The existing Layer 1 implementation plans are obsolete and must be replaced
  after the SDK-first design is reviewed.

## Release gate

Managed ChatGPT login is documented and does not require the experimental external-
token release gate from ADR 0017. Public account activation is nevertheless blocked
until the exact Codex distribution/terms are approved, OS-enforced filesystem
isolation is proven, every built-in action is denied, and the experimental dynamic-
tool bridge is replaced by an approved Host MCP bridge or explicitly authorized.

## Design evidence

- [SDK-first AI Runtime Cutover Design](../superpowers/specs/2026-08-25-sdk-first-ai-runtime-cutover-design.md)
- [OpenAI Codex app-server](https://learn.chatgpt.com/docs/app-server)
- [OpenAI Codex configuration schema](https://github.com/openai/codex/blob/main/codex-rs/core/config.schema.json)
- [Pydantic AI provider model](https://pydantic.dev/docs/ai/models/overview/)
- [Official MCP SDKs](https://modelcontextprotocol.io/docs/sdk)
