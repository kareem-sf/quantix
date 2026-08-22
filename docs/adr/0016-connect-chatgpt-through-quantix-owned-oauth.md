---
status: accepted
---

# Connect ChatGPT through Quantix-owned OAuth

Quantix connects one eligible ChatGPT subscription through a browser PKCE flow owned by the trusted Rust Host, stores the resulting OAuth connection at `<Quantix Application Home>/auth.json`, and executes bounded Provider Turns directly over HTTPS and server-sent events at `https://chatgpt.com/backend-api/codex/responses`. The production application no longer delegates authentication or execution to a Codex CLI or app-server process.

This decision supersedes ADR 0004 as the ChatGPT execution architecture and ADR 0008 as the Codex-specific adapter decision. ADR 0008's general Quantix-owned AI Provider Contract boundary remains accepted through ADR 0012: Quantix still owns Tender workflow, permissions, Typed Tools, evidence, audit, interruption, validation, and EITL controls. This decision amends ADR 0012's OpenAI authentication, credential, catalogue, logout, execution, and compatibility consequences. It also amends ADR 0014 only by changing the adapter and catalogue provenance captured by each Tender and Agent Run; Tender-scoped selection and the prohibition on silent fallback are unchanged.

## Consequences

- The Rust Host owns ChatGPT browser login, OAuth state and PKCE verification, authorization-code exchange, refresh, readiness, and disconnect. It tries the registered loopback callback on port 1455 and then 1457, fails visibly before opening a browser when neither can bind, and does not add device-code login in this layer.
- ChatGPT access, refresh, and ID tokens are Secret Provider Credentials stored in the versioned `auth.json` file directly below the Quantix Application Home. Writes are atomic and refresh, login, and disconnect mutations are serialized. Disconnect deletes the file. Its contents never enter Application Settings, `installation.sqlite`, a Tender Store, Provider Turn context, logs, diagnostics, backups, archives, exports, or generated artifacts. The file relies on the signed-in operating-system account, restrictive Application Home permissions, and full-disk encryption where required; its plaintext JSON format is an explicit local-operability trade-off, not permission to expose or copy it.
- The direct ChatGPT adapter sends provider-native Responses requests and consumes the streamed SSE events behind the existing AI Provider Contract. Every request explicitly sends `store: false` and `stream: true`; encrypted reasoning state may be replayed only as needed to continue the same bounded tool loop. `store: false` requests that the response not be stored for server-side continuation, but does not make a claim about provider security, abuse-monitoring, or legal-retention systems.
- Quantix, not the model or provider, validates every requested Typed Tool against the exact Permission Grant, executes an allowed tool inside the Agent Run boundary, records attributable usage and denials, validates candidate output, and preserves interruption and Indeterminate Agent Run semantics. No backend response can mutate canonical Tender state directly.
- ChatGPT model discovery uses the stable built-in catalogue `chatgpt-direct-v1`, because the researched live models endpoint is not a validated dependency. This catalogue exposes `gpt-5.5`, `gpt-5.4`, `gpt-5.4-mini`, and `gpt-5.3-codex-spark`, each with `none`, `low`, `medium`, `high`, and `xhigh` reasoning and `medium` as the default. A model or reasoning change requires an explicit, tested catalogue-version change; model-name inference and silent forward compatibility are rejected.
- Production preparation, packaging, startup, readiness, updates, and shutdown neither stage, bundle, locate, launch, supervise, nor version-gate a Codex executable. `CODEX_HOME`, Codex-owned login, app-server threads, and app-server protocol compatibility are no longer production concepts.
- The `runtime-fixture` feature may retain deterministic legacy-shaped app-server fixtures and protocol schemas only where they provide existing acceptance coverage. They are test infrastructure, cannot be selected by a production build, and do not reinstate a Codex runtime dependency or compatibility promise.
- The ChatGPT backend and the public OAuth client registration remain externally controlled integration surfaces. Authentication, subscription eligibility, malformed streams, incompatible events, rate limits, connectivity loss, or acceptance uncertainty fail closed through typed Provider Failures. Quantix never falls back to a different provider, model, reasoning setting, or unaccepted turn.
- Public or commercial claims remain blocked until applicable OpenAI terms and product authorization support the intended direct subscription-backed integration. Passing technical acceptance cannot grant contractual authorization.

## Evidence

- [Approved direct-provider design](../superpowers/specs/2026-08-21-chatgpt-direct-provider-design.md)
- [ChatGPT backend contract research](../research/chatgpt-backend-contract.md)
- [Host-controlled run permissions](./0006-enforce-agent-access-through-host-owned-run-grants.md)
- [Provider-neutral AI without silent fallback](./0012-connect-provider-neutral-ai-without-silent-fallback.md)
- [Tender-scoped AI execution](./0014-scope-ai-execution-and-asa-operations-per-tender.md)
- [Superseded Codex thread decision](./0004-run-agent-profiles-through-host-controlled-codex-threads.md)
- [Superseded Codex adapter decision](./0008-keep-codex-behind-a-quantix-owned-ai-provider-contract.md)
