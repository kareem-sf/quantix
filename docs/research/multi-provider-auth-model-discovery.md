# ChatGPT-only connection decision research

Research date: 2026-08-22

## Decision

Quantix connects exactly one AI service: the Tendering Engineer's ChatGPT
account. The normal path is a browser-first, Quantix-owned PKCE flow. An
explicit device-code flow is available only when the Engineer chooses
troubleshooting help or the browser return path is unavailable.

This replaces the earlier multi-provider research in this file. It does not
describe any current Quantix API-key, provider-routing, credential-vault, or
alternative-provider capability.

## Engineer flow

1. Select **Connect ChatGPT**.
2. Quantix binds the internal loopback callback and opens the system browser.
3. OpenAI handles account sign-in, MFA, account choice, and organization
   policy. Quantix's authorization request identifies the application with
   `originator=quantix` and requests the simplified Codex authorization flow
   with `codex_cli_simplified_flow=true`.
4. Quantix verifies the callback, persists the tokens, and only then shows
   **ChatGPT connected to Quantix**.
5. If return is blocked, or if the Engineer opens troubleshooting, Quantix
   offers **Sign in on another device**. It presents one copyable code and the
   OpenAI device page, waits, and can be cancelled.

The device route is not automatic: a browser error does not silently switch
the Engineer to a different connection method.

## Integration boundary

The browser callback tries registered local ports 1455 and 1457 before opening
the browser. Those ports are an internal implementation detail, not part of
the Engineer-facing product language.

The device fallback follows the currently published behavior in the OpenAI
Codex source and the independently maintained OpenCode implementation:

- initiate with `POST /api/accounts/deviceauth/usercode`;
- display `https://auth.openai.com/codex/device` and the returned code;
- poll `POST /api/accounts/deviceauth/token` using the returned device
  authorization identifier and user code;
- treat HTTP 403 and 404 as pending, use the server interval plus three
  seconds, support cancellation, and stop after 15 minutes;
- exchange the resulting authorization code using the device callback URI.

These endpoints and the public OAuth client are external integration surfaces,
not a promise that every account, network, or future service configuration can
use them. Quantix keeps the browser method available when device initiation is
unavailable and reports only plain-language, redacted failures.

## Security and product consequences

- Tokens remain only in `<Quantix Application Home>/auth.json`; they never
  enter Tender data, logs, diagnostics, backups, archives, exports, generated
  artifacts, or renderer state.
- One active login attempt is allowed at a time. Login, refresh, disconnect,
  and persistence changes are serialized. Disconnect cancels an active attempt
  and deletes `auth.json`.
- Quantix has no API-key connection, provider router, fallback provider, or
  multiple-account selector. The serialized adapter value `codex` is an
  internal name; user-facing language is **ChatGPT**.
- Model and reasoning selection is prepared after connection and stays under
  advanced settings. Sending future Tender content requires the Engineer's
  explicit approval.
- Direct execution remains behind Quantix's Provider Contract. Quantix retains
  permissions, evidence, workflow, validation, audit, and approval authority.

## Dated primary sources

Sources checked on 2026-08-22:

- [OpenAI: Authentication documentation](https://learn.chatgpt.com/docs/auth)
- [OpenAI Codex: browser authorization construction](https://github.com/openai/codex/blob/main/codex-rs/login/src/server.rs)
- [OpenAI Codex: device-code authorization implementation](https://github.com/openai/codex/blob/main/codex-rs/login/src/device_code_auth.rs)
- [OpenCode: ChatGPT device-flow implementation](https://github.com/anomalyco/opencode/blob/dev/packages/opencode/src/plugin/codex.ts)

OpenAI's sources are the authority for the integration. OpenCode is an
implementation comparison that informed the fallback interaction, not an
authorization or product-policy source.

## Commercial boundary

Technical behavior does not establish authorization to distribute a
subscription-backed integration. Quantix must not make public or commercial
compatibility claims until the applicable OpenAI terms and product
authorization support them.
