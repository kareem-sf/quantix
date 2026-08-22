# Quantix Codex-Only Beginner Connection — Design

Date: 2026-08-22
Status: Approved (user-approved in session)

## Goal

Quantix has one AI connection: the Engineer's ChatGPT account. A construction
engineer can connect it without understanding providers, API keys, OAuth,
ports, terminals, or model families.

## Product flow

The primary path is deliberately short:

1. The Engineer selects **Connect ChatGPT**.
2. Quantix binds an approved loopback callback before opening the system browser.
3. OpenAI owns authentication, account selection, MFA, and organization policy.
4. The callback exchanges and persists tokens before showing a branded success page.
5. Quantix detects completion and presents the connected account automatically.
6. Quantix prepares its recommended model and reasoning selection. The Engineer
   explicitly confirms that ChatGPT may receive future Tender content; model controls
   are available under an advanced disclosure and are not part of connection setup.

The fallback path is **Sign in on another device**, shown only after the normal
browser return path is unavailable or from a small troubleshooting disclosure. It
opens OpenAI's device page, displays one code with a copy action, waits in Quantix,
and supports cancellation. Device login is never the default and is never selected
silently after an ordinary provider error.

## Provider boundary

- `AiProviderKind` has exactly one value, `codex`, preserving the established
  persisted name for Quantix's ChatGPT execution adapter.
- Anthropic and Gemini adapters, commands, API-key forms, credential-vault access,
  schemas, cleanup targets, bindings, tests, and current product claims are deleted.
- The `keyring` dependency is deleted because Quantix no longer stores BYOK secrets.
- There is no provider router and no fallback to another provider, model, or account.
- Historical comparison research may mention third-party providers when it is clearly
  research rather than a statement of current Quantix capability.

## Browser authentication

The existing Quantix-owned browser PKCE flow remains the primary implementation.
It continues to use the externally registered loopback callbacks on ports 1455 then
1457 and the public Codex client identity already used by the direct-provider
architecture. The authorization request includes these exact behavioral markers:

- `codex_cli_simplified_flow=true`
- `originator=quantix`

Port numbers and holding process identifiers are internal diagnostics. User-facing
copy says only that the sign-in return step is blocked, asks the Engineer to close
other ChatGPT/Codex sign-in windows and retry, and offers device sign-in.

The callback page says **ChatGPT connected to Quantix** only after the token exchange
and local persistence succeed. Failure and cancellation pages use plain English and
never expose tokens, authorization codes, provider response bodies, PIDs, or OAuth
terminology.

## Device authentication fallback

The fallback mirrors OpenCode's current ChatGPT headless flow:

- Initiate with `POST https://auth.openai.com/api/accounts/deviceauth/usercode` and
  JSON `{ "client_id": "app_EMoamEEZ73f0CkXaXp7hrann" }`.
- Show `https://auth.openai.com/codex/device` and the returned `user_code`.
- Poll `POST https://auth.openai.com/api/accounts/deviceauth/token` using the returned
  `device_auth_id` and `user_code`, at the server-provided interval plus a three-second
  safety margin.
- Treat HTTP 403 and 404 as pending. Other non-success responses fail with redacted
  typed state.
- Exchange the returned authorization code and verifier at `/oauth/token` with
  redirect URI `https://auth.openai.com/deviceauth/callback`.
- Stop on success, cancellation, a provider error, or a 15-minute local timeout.

Device flow availability is externally controlled. If initiation is unavailable,
Quantix explains that the alternate sign-in method is unavailable and leaves the
primary browser method usable.

## Fresh persistence contract

Backward compatibility is intentionally not preserved.

- Installation schema version advances from 24 to 25 and accepts only `codex` in
  provider tables.
- Tender schema version advances from 35 to 36 because provider-bearing serialized
  contracts no longer accept Anthropic or Gemini.
- No migrations, aliases, fallback deserializers, or legacy cleanup jobs are added.
- Existing local Quantix application data and Quantix-owned AI credentials are
  removed after implementation so the next launch creates schema 25/36 data from
  an empty application home.

## UX language

- User-facing provider name: **ChatGPT**.
- Primary action: **Connect ChatGPT**.
- Fallback action: **Sign in on another device**.
- Pending browser message: **Finish signing in in your browser. Quantix will connect automatically.**
- Pending device message: **Enter this one-time code on the OpenAI page, then return to Quantix.**
- Connected status: **Connected** with the account label when available.
- Technical catalogue provenance remains available only under advanced model settings.

## Security and failure behavior

- Fresh PKCE and state values are generated for every browser attempt.
- Browser callback state is verified before token exchange.
- Only one login attempt may be active at a time, regardless of method.
- Login, refresh, disconnect, and persistence mutations remain serialized.
- Tokens and raw provider errors never enter UI state, logs, diagnostics, Tender
  Stores, backups, archives, exports, or generated artifacts.
- Disconnect cancels an active attempt and deletes `auth.json`.
- Public/commercial distribution remains blocked until the applicable OpenAI terms
  and authorization support Quantix's subscription-backed integration.

## Verification constraint

The user explicitly prohibited running tests. Implementation updates affected test
sources and generated declarations, but this session does not execute test, check,
verification, development-server, or production-build commands. Completion reporting
must state that limitation and must not claim that tests pass or the app was run.
