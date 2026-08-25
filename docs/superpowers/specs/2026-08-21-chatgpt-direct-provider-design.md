# Quantix Direct ChatGPT Provider — Design

Date: 2026-08-21
Status: Superseded by ADR 0018 and the 2026-08-25 SDK-first design; historical evidence only

## Goal

Quantix connects to the Engineer's ChatGPT subscription directly, exactly like OpenCode
does: Quantix owns the OAuth login, the tokens, and the execution calls. The bundled
Codex CLI is removed entirely. Quantix and Codex are fully separated; they never mix.

## Verified external facts

From OpenCode's implementation (`packages/opencode/src/plugin/openai/codex.ts`,
anomalyco/opencode) and OpenAI's Codex CLI:

- OAuth issuer: `https://auth.openai.com`
- Client ID: `app_EMoamEEZ73f0CkXaXp7hrann` (public Codex CLI client)
- Browser PKCE flow: local loopback server, callback `http://localhost:1455/auth/callback`
- Registered fallback callback port: 1457 (`localhost:1457/auth/callback`)
- Authorize params: `response_type=code`, scope `openid profile email offline_access`,
  PKCE S256 challenge, random `state`, `id_token_add_organizations=true`, `originator`
- Token endpoints: `POST {issuer}/oauth/token`
  (`grant_type=authorization_code`; refresh via `grant_type=refresh_token`)
- Account identity from JWT claims (`chatgpt_account_id`, nested auth claim, org fallback)
- Backend API: `https://chatgpt.com/backend-api/codex/responses`
- Device-login flow exists (`auth.openai.com/codex/device`) — deferred, not built now.

## Decisions

| # | Decision |
|---|---|
| 1 | Full replacement: drop bundled Codex CLI; Quantix calls the backend API directly |
| 2 | Tokens stored by Quantix at `<application home>/auth.json` (plaintext, chosen for debuggability) |
| 3 | Old delegated logins (browser + device-code via codex binary): deleted |
| 4 | Callback ports: bind 1455 → fall back to 1457 → fail with PID-level actionable error |
| 5 | Device-code method: deferred |
| 6 | Sequencing: phased construction on an isolated branch; single clean landing on main |
| 7 | AGENTS.md gains a standing simplicity-first rule |
| 8 | `originator=quantix` |

## Architecture after landing

- New host module `chatgpt_oauth`: PKCE, loopback responder, authorize URL, token
  exchange/refresh, JWT claims parsing, branded success/error pages, token store.
- New executor inside the agent runtime implementing today's Provider Turn lifecycle
  over direct HTTPS + SSE to the backend endpoint: Permission Grants, Agent Run
  Workspaces, Typed Tools, candidate validation, interruption/indeterminate semantics,
  usage capture. Tool approvals move fully host-side (model emits function_call →
  grant validated → Typed Tool executes → output returned).
- Deterministic lane: HTTP client sits behind a seam filled by scripted responses
  under the `runtime-fixture` feature.
- Deleted: `codex_actor.rs`, `codex_protocol.rs`, Codex binary staging, CODEX_HOME
  plumbing, `@openai/codex` dependency, `smoke:codex`, runtime-readiness Codex checks.
- Docs: new ADR superseding parts of ADR 0004/0008 and amending 0012/0014; CONTEXT.md
  glossary updated (Provider Credential may reside in Application Home auth.json);
  AGENTS.md simplicity rule added.

## Phases (built on one branch, landed together on main)

1. **Phase 1** — Quantix-owned OAuth connection end to end (module, store, settings UI,
   tests). Agent runs stay non-functional on the branch until Phase 2 lands.
2. **Spike** — probe backend contract (request/SSE shapes, function-call items, rate
   limits, model/reasoning catalogue source, WS vs SSE). Reported before executor code.
3. **Phase 2** — direct backend executor replacing the Codex runtime.
4. **Phase 3** — delete all Codex CLI legacy; docs/governance updates.

## Error handling

- Port conflicts detected before opening the browser; error names holding processes.
- State mismatch / missing code / provider error → branded error page + typed failure.
- Refresh failure with invalid_grant → connection returns to AuthenticationRequired.
- No silent fallbacks anywhere; every failure maps to existing typed error codes.

## Verification

Each phase ends with `npm run verify` green on the branch. Final landing requires full
verify + renderer build. Live-provider checks remain explicit opt-in commands.
