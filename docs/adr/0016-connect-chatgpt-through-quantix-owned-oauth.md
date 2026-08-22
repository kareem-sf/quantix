---
status: accepted
---

# Connect ChatGPT through Quantix-owned OAuth

Quantix has exactly one AI connection: the Tendering Engineer's eligible
ChatGPT account. The trusted Rust Host owns the connection, stores its OAuth
tokens at `<Quantix Application Home>/auth.json`, and executes bounded Provider
Turns directly over HTTPS and server-sent events at
`https://chatgpt.com/backend-api/codex/responses`.

This decision supersedes ADR 0012 for provider count and authentication. It
also supersedes ADR 0004 and ADR 0008 for the ChatGPT execution architecture.
Quantix still owns Tender workflow, permissions, Typed Tools, evidence, audit,
interruption, validation, and Engineer-in-the-Loop controls; the connected
ChatGPT account owns none of those authorities.

## Consequences

- Browser PKCE is the primary sign-in path. The Rust Host binds its registered
  loopback callback before opening the system browser, trying ports 1455 then
  1457 internally. The authorization request contains
  `codex_cli_simplified_flow=true` and `originator=quantix`. Ports, process
  identifiers, OAuth terminology, and raw provider errors are never shown to
  the Engineer.
- The callback verifies state, exchanges the authorization code, validates the
  identity, and persists the connection before its success page says
  **ChatGPT connected to Quantix**. A failed or cancelled completion returns
  plain-language guidance without tokens, codes, or response bodies.
- **Sign in on another device** is an explicit troubleshooting fallback, never
  a silent alternative. Quantix initiates the device flow at
  `https://auth.openai.com/api/accounts/deviceauth/usercode`, shows the
  returned one-time code and `https://auth.openai.com/codex/device`, then polls
  `https://auth.openai.com/api/accounts/deviceauth/token`. It uses the
  server-provided interval plus three seconds, treats HTTP 403 and 404 as
  pending, allows cancellation, and stops after 15 minutes. The resulting
  authorization code is exchanged with redirect URI
  `https://auth.openai.com/deviceauth/callback`.
- Browser and device attempts share one active-login guard. Refresh, login,
  disconnect, and persistence mutations are serialized. Disconnect cancels an
  active attempt and deletes `auth.json`.
- ChatGPT access, refresh, and ID tokens are Secret Provider Credentials. They
  are kept only in `auth.json`, written atomically, and never enter Application
  Settings, `installation.sqlite`, a Tender Store, Provider Turn context,
  logs, diagnostics, backups, archives, exports, or generated artifacts.
- Quantix has no API-key connection, alternate AI provider, provider router,
  provider fallback, or multiple-account selection. The persisted provider
  value `codex` is an internal adapter name; user-facing copy says **ChatGPT**.
- A ready connection prepares its recommended model and reasoning selection
  without approving it. The Tendering Engineer explicitly approves ChatGPT as
  the destination for future Tender content. Advanced model settings are kept
  out of connection setup. Quantix never silently substitutes a model or
  reasoning setting.
- The direct adapter sends provider-native Responses requests with `store:
  false` and `stream: true`. Quantix validates every requested Typed Tool
  against its exact Permission Grant, runs permitted tools inside the Agent Run
  boundary, and validates candidate output before canonical Tender state can
  change.
- Production preparation, packaging, startup, readiness, updates, and
  shutdown do not stage, bundle, locate, launch, supervise, or version-gate a
  Codex executable. Legacy app-server-shaped fixtures may remain only as
  deterministic test infrastructure behind `runtime-fixture` and do not create
  a production dependency or compatibility promise.
- The browser authorization, device endpoints, subscription eligibility, and
  direct backend are externally controlled. Authentication, connectivity,
  malformed responses, rate limits, and acceptance uncertainty fail closed.
  Public or commercial claims remain blocked until applicable OpenAI terms and
  product authorization support Quantix's subscription-backed integration.

## Evidence

- [Approved beginner-connection design](../superpowers/specs/2026-08-22-codex-only-beginner-connection-design.md)
- [Current connection research](../research/multi-provider-auth-model-discovery.md)
- [OpenAI authentication documentation](https://learn.chatgpt.com/docs/auth)
- [OpenAI Codex browser-login source](https://github.com/openai/codex/blob/main/codex-rs/login/src/server.rs)
- [OpenAI Codex device-login source](https://github.com/openai/codex/blob/main/codex-rs/login/src/device_code_auth.rs)
- [Host-controlled run permissions](./0006-enforce-agent-access-through-host-owned-run-grants.md)
- [Tender-scoped AI execution](./0014-scope-ai-execution-and-asa-operations-per-tender.md)
- [Superseded provider-neutral decision](./0012-connect-provider-neutral-ai-without-silent-fallback.md)
