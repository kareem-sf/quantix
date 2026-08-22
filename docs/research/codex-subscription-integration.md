# Engineer-subscription Codex integration for Quantix

Research date: 2026-08-06

## Decision

Quantix can technically implement the requested **local, per-engineer Codex
connection without BYOK**. The documented integration is a locally running
Codex process controlled through **Codex app-server**, using app-server's
Codex-managed ChatGPT browser or device-code login. The engineer signs in with
their own ChatGPT account; Codex persists and refreshes that account's tokens,
and the work consumes that account's ChatGPT/Codex subscription allowance.

This is a sound basis for the Quantix v0 and private evaluation builds. It is
**not yet an unconditional public-production clearance**. OpenAI documents
app-server as the surface for deep integration in one's own product, but the
same page currently calls the app-server command and WebSocket transport
experimental and unsupported for production. The public documentation also
does not state that an independently distributed commercial application may
rely on every customer's personal Plus or Pro subscription as its product
entitlement. OpenAI must confirm that commercial/contractual point before a
public release is promised.

## What is explicitly supported

### Product-integration surface

[Codex app-server](https://developers.openai.com/codex/app-server) is explicitly
described as the interface for a deep integration inside one's own product. It
provides authentication, conversation history, approvals, streamed events,
thread lifecycle operations, sandbox configuration, account status, and
rate-limit information over JSON-RPC. This is the right documented surface for
a Quantix desktop UI that owns the login and approval experience.

The default transport is newline-delimited JSON over `stdio`. A local Quantix
backend can spawn one pinned Codex process and communicate over its standard
input and output. The WebSocket transport is experimental and unsupported; it
also introduces a second transport-authentication problem. Quantix v0 should
therefore use local `stdio`, not expose app-server on a network port.

The [TypeScript Codex SDK](https://developers.openai.com/codex/codex-sdk) is a
simpler, higher-level option for coding-focused jobs. It can start, continue,
stream, and resume local Codex threads. OpenAI says to run it server-side and
requires Node.js 18 or later. Its
[official source README](https://github.com/openai/codex/blob/a17da5e6e4a5a9b45396f0693b0a4d5b9df06318/sdk/typescript/README.md)
states that it wraps and spawns the Codex CLI, and specifically mentions
controlling its environment in sandboxed hosts such as Electron apps. The
published package depends on the same-version `@openai/codex` runtime. The SDK
does not expose the full documented account/login/rate-limit UI surface, so it
is not the best primary boundary for Quantix's signed-in desktop experience.

OpenAI also publishes a Python SDK, currently documented as beta, requiring
Python 3.10 or later and carrying a pinned Codex CLI runtime. Quantix does not
need both SDKs.

### ChatGPT sign-in and credential lifecycle

[OpenAI's Codex authentication documentation](https://learn.chatgpt.com/docs/auth)
states that **Sign in with ChatGPT** provides subscription access, while API-key
sign-in provides separately billed usage-based access. App-server exposes two
Codex-managed ChatGPT flows:

- browser login, where app-server returns an OpenAI `authUrl`, hosts the local
  callback, and announces completion; and
- device-code login, where Quantix displays OpenAI's verification URL and code
  while its frontend owns the ceremony.

In managed ChatGPT mode, Codex owns the OAuth flow, persists the tokens, and
refreshes them automatically. `account/read` reports the authenticated account
and plan and can force a refresh. `account/logout` clears the active account.

Codex can store cached credentials in `CODEX_HOME/auth.json` or in the
operating-system credential store. `cli_auth_credentials_store = "keyring"`
selects the latter; `auto` may fall back to the plaintext JSON file. The JSON
file contains access tokens and must be treated like a password. Quantix should
use the OS credential store, should not copy OpenAI tokens into its own database,
and should never log them.

App-server also has an externally managed `chatgptAuthTokens` mode, but OpenAI
marks it experimental. It makes the host responsible for acquiring and
refreshing raw access tokens. Quantix does not need it and should not use it.

### Session reuse

Authentication and work sessions are separate:

- Codex-managed login credentials are cached locally and automatically
  refreshed during use.
- Codex threads are persisted locally. The SDK source documents
  `~/.codex/sessions`, and app-server provides `thread/list`, `thread/read`,
  `thread/resume`, `thread/archive`, and related APIs over persisted rollout
  data. Quantix can store the returned thread ID on an Agent Profile or Tender
  Task and resume it after an application restart.

This supports one Codex thread per active Agent Profile. It does not mean the
thread is a durable Quantix business record: Quantix's own task, evidence,
decision, and deliverable records remain authoritative. Local Codex transcripts
also need an explicit retention policy because they may contain Tender Package
content.

### Usage attribution and limits

With ChatGPT-managed login, app-server reports the signed-in account and its
`planType`. It exposes:

- `account/rateLimits/read` and rate-limit update notifications, including the
  consumed percentage, quota-window duration, reset time, plan type when
  supplied, and reached-limit state; and
- `account/usage/read`, which can return account-level token-activity summaries
  and daily buckets.

This is evidence that runs use the authenticated engineer's Codex account and
its ChatGPT limits rather than a Quantix API key. It is not evidence of a
separate quota reserved for Quantix.

OpenAI's [Using Codex with your ChatGPT plan](https://help.openai.com/en/articles/11369540-using-codex-with-your-chatgpt-plan)
article likewise says Codex is included across ChatGPT plans, requires users to
sign in with their ChatGPT account, and applies the user's ChatGPT or
business-workspace terms to Codex use.

[Codex pricing and plan documentation](https://learn.chatgpt.com/docs/pricing)
says local messages and cloud chats share a five-hour allowance and that
additional weekly limits may apply. Consumption varies with model, task size,
context, reasoning, retrieval, tools, and caching, so even similar requests can
consume different amounts. The published message ranges are estimates and can
change. Quantix must read the live account limits, surface them to the engineer,
queue work when the account is exhausted, and never hard-code a promise of a
fixed number of Tender Office agents or tasks per subscription.

OpenAI publishes a configurable
`agents.max_concurrent_threads_per_session` setting for Codex-spawned
subagents, but does not publish a numeric account-wide concurrent-thread limit,
a per-app allowance, or a service-level agreement for multiple independent
app-server threads. That setting is not a subscription entitlement. Quantix
must treat concurrency as its own conservative scheduler policy and handle
rate-limit or overload responses rather than assuming unlimited parallelism.

## Distribution and runtime constraints

- The TypeScript SDK and Codex CLI sources are published under Apache-2.0; see
  the [SDK package metadata](https://github.com/openai/codex/blob/a17da5e6e4a5a9b45396f0693b0a4d5b9df06318/sdk/typescript/package.json)
  and [repository license](https://github.com/openai/codex/blob/a17da5e6e4a5a9b45396f0693b0a4d5b9df06318/LICENSE).
  That permits software redistribution subject to the license. It does not
  grant or define entitlement to the hosted Codex service.
- The current [official npm package metadata](https://registry.npmjs.org/@openai%2Fcodex/0.146.1)
  provides Codex binaries for x64 and arm64 Windows, macOS, and Linux. The SDK
  requires a local process and filesystem; it is not a browser library. In an
  Electron-style application, use only the trusted main/backend process, never
  the renderer.
- Pin the SDK/runtime version. App-server schemas are generated from the exact
  Codex version, experimental methods require an explicit opt-in, and some
  methods are expressly marked "do not call from production clients yet."
- Every app-server connection must initialize with `clientInfo`. OpenAI says
  the name identifies the integration in Compliance Logs and asks developers
  of new enterprise Codex integrations to contact OpenAI to join the known
  clients list.
- Authenticated OpenAI access and client-to-app-server transport security are
  separate. Keeping app-server on private `stdio` avoids exposing either a
  local control plane or a credential-bearing WebSocket endpoint.

## Assumptions Quantix must not make

1. **Do not assume a personal subscription can be pooled.** Each engineer must
   sign in to their own account. OpenAI's
   [Terms of Use](https://openai.com/policies/terms-of-use/) prohibit sharing
   credentials or making an account available to someone else, and OpenAI's
   [account-sharing policy](https://help.openai.com/en/articles/10471989-openai-account-sharing-policy)
   says an account is for the individual who created it.
2. **Do not assume technical support equals commercial entitlement.** The
   app-server docs authorize the technical shape of an in-product integration,
   but the public docs do not reconcile that path with the personal Terms of
   Use restriction on automatic/programmatic extraction of data or Output, nor
   do they explicitly approve resale of a product whose core AI service is each
   customer's Plus/Pro allowance. The
   [OpenAI Services Agreement](https://openai.com/policies/services-agreement/)
   expressly grants Customer Application integration rights for the API, but
   does not separately make that grant for ChatGPT subscription quotas. Obtain
   written OpenAI confirmation before public commercial distribution. This
   report makes no legal conclusion.
3. **Do not assume app-server currently has a production-support guarantee.**
   Its own page calls the app-server command and WebSocket transport
   experimental and unsupported for production workloads. Pinning a version
   and using `stdio` reduces change and network risk but does not erase that
   statement.
4. **Do not assume Quantix may impersonate or hide OpenAI authentication.** The
   managed flow returns OpenAI URLs and documents only `codex` or `chatgpt` as
   the hosted-success-page brand. Quantix should label the action truthfully as
   signing in to ChatGPT/Codex.
5. **Do not assume a user's plan always permits access.** Workspace membership,
   administrator policy, region, feature rollout, selected model, exhausted
   allowance, or a revoked session can deny work. Quantix needs a visible
   disconnected/limited state and resumable jobs.
6. **Do not assume one thread is isolated merely because it has a different
   Agent Profile.** File access, sandbox mode, writable roots, tools, MCP
   servers, and approval policy are configured runtime boundaries. Quantix must
   set them per task and keep original Tender Package evidence read-only.
7. **Do not assume Codex is validated as a construction professional.** OpenAI
   describes the SDK as serving coding-focused Codex threads. The official docs
   do not establish fitness, accuracy, regulatory compliance, or professional
   responsibility for tender management, estimating, contracts, drawings, or
   engineering decisions. Quantix's deterministic workflow, evidence links,
   independent review, and Tendering Manager approval remain mandatory.
8. **Do not assume subscription metering can be allocated per Agent Profile.**
   The documented service views are account-level. Quantix can track its own
   task and token telemetry, but official sources do not define a mapping from
   those values to the subscription meter or invoice.

## v0 integration boundary

For v0, use one pinned local Codex app-server process per signed-in Quantix
desktop user over `stdio`. Let app-server manage ChatGPT browser/device login,
token persistence, refresh, account state, and logout. Use stable thread,
turn, approval, sandbox, and account methods only. Store thread IDs beside
Quantix Agent Profiles, monitor the live rate-limit endpoint, and queue work
conservatively. Do not add BYOK, a Quantix cloud AI gateway, raw-token handling,
WebSocket transport, subscription pooling, or multi-provider routing.

Before a public commercial launch, ask OpenAI to confirm in writing:

- that a distributed third-party desktop product may use per-user ChatGPT
  Plus/Pro/Business Codex subscription access through app-server;
- which app-server/SDK surface and version OpenAI supports for production;
- whether Quantix must be registered as a known client beyond enterprise
  Compliance Logs; and
- which commercial terms govern this integration.

## Primary sources

- [Codex App Server](https://developers.openai.com/codex/app-server)
- [Codex SDK](https://developers.openai.com/codex/codex-sdk)
- [Codex authentication](https://learn.chatgpt.com/docs/auth)
- [Using Codex with your ChatGPT plan](https://help.openai.com/en/articles/11369540-using-codex-with-your-chatgpt-plan)
- [Codex pricing and usage limits](https://learn.chatgpt.com/docs/pricing)
- [Codex configuration reference](https://learn.chatgpt.com/docs/config-file/config-reference)
- [Open-source Codex components](https://learn.chatgpt.com/docs/open-source)
- [Official TypeScript SDK source](https://github.com/openai/codex/tree/a17da5e6e4a5a9b45396f0693b0a4d5b9df06318/sdk/typescript)
- [OpenAI Terms of Use](https://openai.com/policies/terms-of-use/)
- [OpenAI Services Agreement](https://openai.com/policies/services-agreement/)
- [OpenAI Account Sharing Policy](https://help.openai.com/en/articles/10471989-openai-account-sharing-policy)
