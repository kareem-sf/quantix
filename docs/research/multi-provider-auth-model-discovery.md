# Multi-provider authentication and model discovery

Research date: 2026-08-15

## Question

How can Quantix be an AI-powered tendering workspace that supports a user's
ChatGPT/Codex account as well as Anthropic and Gemini bring-your-own-key
(BYOK), while loading models and reasoning controls from the providers rather
than presenting a stale hardcoded catalog?

## Short answer

Quantix should treat AI access as a set of provider connections, not as “the
Codex runtime.” The evidence supports three different first-party integration
paths:

1. **OpenAI account connection:** supervise the official `codex app-server`
   and let it own ChatGPT browser/device-code login, refresh, account state,
   turns, and its live model catalog. Do not implement a private
   `chatgpt.com/backend-api` client in Quantix.
2. **Anthropic BYOK:** call the Claude API with the Engineer's Anthropic API
   key. The current Models API provides a live model list and machine-readable
   capability flags for supported effort levels and thinking modes.
3. **Google Gemini BYOK:** call the Gemini Developer API with the Engineer's
   Gemini API key. Its Models API provides a live model list and a `thinking`
   capability flag, but it does **not** advertise the exact valid thinking
   levels or numeric budget ranges per model.

Consequently, “never hardcode models” is achievable for all three providers.
“Never hardcode effort options” is achievable for Codex and current Anthropic,
but not for Gemini using its documented model-discovery endpoint. With that
strict requirement, Quantix can offer Gemini **Provider default / Automatic**
and only add explicit Gemini choices if Google later exposes them through a
machine-readable capability API.

## What Hermes and Agent Zero actually do

These projects are useful implementation evidence, but neither project is a
provider contract or a legal authorization for another product.

### Hermes Agent

Hermes has a provider registry with distinct transports and credentials for
OpenAI Codex, the OpenAI API, Anthropic, Gemini, OpenRouter, local servers, and
other providers. Its public provider documentation distinguishes ChatGPT OAuth
for OpenAI Codex from API-key access to the OpenAI API, and supports direct
Anthropic and Gemini providers. It also warns that auxiliary features may use
a different provider from the main model. [Hermes provider documentation](https://github.com/hermes-agent-org/hermes/blob/main/website/docs/integrations/providers.md)

For Codex, Hermes normally maintains an OAuth token store in
`~/.hermes/auth.json`. The source explicitly says this session is separate from
the Codex CLI and VS Code extension because rotating refresh tokens can collide.
Hermes may offer to import `~/.codex/auth.json`, but recommends a separate login
and subsequently owns its copied session. Its default Codex transport calls the
ChatGPT Codex backend with the OAuth bearer; a newer, explicit opt-in mode can
instead hand an OpenAI/Codex turn to `codex app-server`. [Hermes authentication source](https://github.com/NousResearch/hermes-agent/blob/main/hermes_cli/auth.py), [Hermes runtime-provider source](https://github.com/NousResearch/hermes-agent/blob/main/hermes_cli/runtime_provider.py)

Hermes does not demonstrate a universally live capability catalog. Its model
picker contains curated provider lists and provider-specific fetching/caches;
its Codex list is derived from a maintained Codex model module. This is a
pragmatic compatibility system, not proof that every provider exposes the same
discovery data. [Hermes model catalog source](https://github.com/NousResearch/hermes-agent/blob/main/hermes_cli/models.py)

### Agent Zero

Agent Zero's account-connection plugin says that OAuth tokens are
password-equivalent, stores them in an Agent-Zero-owned file, and explicitly
warns against pointing that file at a rotating Codex CLI credential store. Its
Codex connection uses browser or device-code OAuth, refreshes its own tokens,
and exposes a local OpenAI-compatible wrapper. [Agent Zero OAuth documentation](https://github.com/agent0ai/agent-zero/blob/main/plugins/_oauth/README.md), [Agent Zero Codex provider](https://github.com/agent0ai/agent-zero/blob/main/plugins/_oauth/helpers/providers/codex.py)

Agent Zero calls the Codex backend `/models` route and preserves fields such as
`default_reasoning_level` and `supported_reasoning_levels`. Its ordinary
provider registry separately points Anthropic and Google BYOK configurations at
their first-party model-list endpoints. [Agent Zero Codex bridge](https://github.com/agent0ai/agent-zero/blob/main/plugins/_oauth/helpers/codex.py), [Agent Zero provider catalog](https://github.com/agent0ai/agent-zero/blob/main/conf/model_providers.yaml)

Agent Zero also has a Google OAuth connection, but that source is clear that it
uses a user-supplied Google Cloud OAuth client and Gemini API billing/quota; it
does not reuse Gemini CLI, Google AI Pro, or Google AI Ultra subscription quota.
That is a different product from the Gemini API-key BYOK path requested for
Quantix. [Agent Zero Gemini account provider](https://github.com/agent0ai/agent-zero/blob/main/plugins/_oauth/helpers/providers/gemini_api.py)

### Lesson from both projects

The transferable pattern is the separation of:

- connection/authentication state;
- a provider-native transport;
- provider-native model discovery;
- per-run model configuration;
- provider-specific recovery and quota states.

The unsafe pattern to copy is direct dependence on undocumented ChatGPT backend
routes and a shared refresh-token file. Quantix has an official integration
surface available through `codex app-server`, so it does not need that coupling.

## Provider facts

### OpenAI Codex / ChatGPT account

The official Codex app-server supports these account operations:

- `account/read` for current account state;
- `account/login/start` with `type: "chatgpt"` for browser login;
- `account/login/start` with `type: "chatgptDeviceCode"` for device-code login;
- `account/login/cancel`, `account/logout`, and account-update notifications;
- Codex-managed persistence and refresh of ChatGPT OAuth credentials.

It also supports API-key login, but that is a separate billing/auth mode from a
ChatGPT account. [Official Codex app-server authentication](https://github.com/openai/codex/blob/main/codex-rs/app-server/README.md#auth-endpoints)

The official `model/list` method is paginated and returns an ordered catalog.
Each entry includes an identifier, display name, description,
`supportedReasoningEfforts`, `defaultReasoningEffort`, accepted input
modalities, default status, and optional upgrade metadata. Therefore both the
model picker and its reasoning-effort picker can be generated from the active
Codex process instead of a Quantix enum. [Official Codex model-list documentation](https://github.com/openai/codex/blob/main/codex-rs/docs/codex_mcp_interface.md#models)

The same interface exposes thread/turn methods and streaming notifications, so
the Codex provider can power real Manager/Agent work and live activity; it is
not merely a login helper. However, the official documentation labels this
interface **experimental** and says methods, fields, and event shapes may
change. Quantix must version-gate the protocol, generate/commit its bindings,
and fail closed when required capabilities disappear. [Official Codex interface stability notice](https://github.com/openai/codex/blob/main/codex-rs/docs/codex_mcp_interface.md#compatibility-and-stability)

OpenAI's source defines `CODEX_HOME` as the Codex configuration root and stores
authentication under it. A Quantix-supervised process can therefore use a
Quantix-owned Codex home rather than reading or mutating the Engineer's normal
CLI session. [Official Codex home/auth source](https://github.com/openai/codex/blob/main/codex-rs/utils/home-dir/src/lib.rs)

#### Product and legal uncertainty

The official app-server and SDK document third-party client mechanics, which is
strong product evidence that client applications are an intended use. The
public sources reviewed here do not, however, contain a specific statement that
a separately distributed commercial desktop product may market itself as using
an end user's consumer ChatGPT subscription. OpenAI's consumer terms prohibit
credential sharing and automatic/programmatic extraction of data or output;
business terms separately allow API-based customer applications and prohibit
sharing individual login credentials. A local single-user client using the
official managed login is not the same as sharing credentials, but the exact
commercial-distribution boundary is not resolved by these pages. Quantix should
obtain provider/legal confirmation before publicly promising “use your ChatGPT
subscription,” and should describe the feature narrowly as “Connect your
OpenAI account through the official Codex login” until confirmed. [OpenAI Terms of Use](https://openai.com/policies/row-terms-of-use/), [OpenAI Services Agreement](https://openai.com/policies/services-agreement/)

### Anthropic BYOK

The Claude API accepts an API key in `x-api-key`; SDKs also read
`ANTHROPIC_API_KEY`. Anthropic characterizes API keys as suitable for local
development and single-tenant servers where the application controls secret
storage, and instructs applications to store them in a secrets manager, rotate
them, and revoke suspected leaks. Quantix can therefore support true Anthropic
BYOK without relying on Claude Pro/Max or Claude Code credentials. [Anthropic authentication documentation](https://platform.claude.com/docs/en/manage-claude/authentication)

`GET /v1/models` lists the models available to that API account. The current
`ModelInfo` schema includes limits and `capabilities`; its effort capability has
machine-readable support flags for `low`, `medium`, `high`, `xhigh`, and `max`,
and its thinking capability reports whether `enabled` and `adaptive` modes are
supported. Exact supported effort choices can therefore be populated live for
each returned model. The catalog does not expose a per-model default-effort
field or numeric manual-thinking budget ranges in the documented schema.
[Anthropic Models API](https://platform.claude.com/docs/en/api/models), [Anthropic list-models endpoint](https://platform.claude.com/docs/en/api/models/list)

The request syntax is provider-native. Current models may use adaptive thinking
with `output_config.effort`; older/manual models use
`thinking: {type: "enabled", budget_tokens: ...}`. Some newer models reject
manual budgets. A single cross-provider `reasoning_effort` field cannot safely
be forwarded unchanged. [Anthropic effort documentation](https://platform.claude.com/docs/en/build-with-claude/effort), [Anthropic thinking documentation](https://platform.claude.com/docs/en/build-with-claude/extended-thinking)

For the beginner UI, the live effort flags are sufficient to show only valid
named effort choices. “Provider default” must remain a distinct choice because
the catalog does not advertise a default. Manual numeric thinking budgets are
an advanced, different control and should not be inferred from an effort name.

### Google Gemini BYOK

Gemini Developer API requests use an opaque API key in the `x-goog-api-key`
header; the SDKs read `GEMINI_API_KEY` or `GOOGLE_API_KEY`. Google's current key
documentation distinguishes standard keys from service-account-bound
authorization keys. New AI Studio keys default to authorization keys, and the
page announces rejection of standard keys from September 2026. Quantix should
accept the user's key without trying to infer its type and surface authentication
failure as a connection problem. [Gemini API authentication reference](https://ai.google.dev/api), [Gemini API-key documentation](https://ai.google.dev/gemini-api/docs/api-key)

`models.list` is paginated and returns identifiers, display names, descriptions,
input/output token limits, supported generation methods, and a boolean
`thinking` field. This is enough to build the live model picker and determine
whether a model has some thinking capability. [Gemini Models API](https://ai.google.dev/api/models)

It is **not** enough to build an exact live effort picker. Gemini 3 and later
use `thinkingLevel`; Gemini 2.5 uses `thinkingBudget`. The generation API defines
the possible level enum, while the thinking guide documents model-specific
subsets, defaults, and numeric ranges. Those exact subsets/ranges are not fields
in the model-list response. [Gemini ThinkingConfig reference](https://ai.google.dev/api/generate-content#thinkingconfig), [Gemini thinking guide](https://ai.google.dev/gemini-api/docs/generate-content/thinking)

Accordingly, without a Quantix-owned compatibility table, an undocumented
endpoint, or speculative error probing, the only non-hardcoded Gemini reasoning
selection is **Provider default / Automatic**. The UI can still say that the
selected live model supports thinking when `Model.thinking` is true.

Google's Gemini API terms expressly cover developers building API clients for
professional or business purposes, subject to region, age, use, and other
restrictions. Quantix must still present the applicable terms and avoid implying
that a Gemini API key consumes Google AI Pro/Ultra subscription quota.
[Gemini API Additional Terms](https://ai.google.dev/gemini-api/terms)

## Implications for the Quantix design

### Provider boundary

Use three adapters behind one small provider interface, while keeping request
translation provider-native:

| Provider connection | Credential owner | Catalog source | Named effort source | Turn transport |
| --- | --- | --- | --- | --- |
| OpenAI account | supervised Codex app-server in Quantix Application Home | app-server `model/list` | app-server model metadata | app-server thread/turn RPC |
| Anthropic BYOK | Quantix secret store | Claude `GET /v1/models` | live `capabilities.effort` flags | Claude Messages API |
| Gemini BYOK | Quantix secret store | Gemini `models.list` | not available exactly; provider default only | Gemini `generateContent` API |

Do not normalize transports by pretending every provider is OpenAI-compatible.
Normalize only Quantix's own domain events: connection state, catalog snapshot,
run request, streamed text/tool/activity events, usage, completion, and failure.

### Domain records

A model selection is a provider-qualified identity, never a bare model string:

```text
ProviderConnectionId + ProviderModelId + ProviderReasoningSelection
```

`ProviderReasoningSelection` should be a tagged provider-native value:

- `provider_default`;
- `codex_effort(<advertised value>)`;
- `anthropic_effort(<advertised value>)`;
- a future Gemini value only when backed by a machine-readable provider
  capability.

Every new Agent Run should snapshot the provider connection identity, model ID,
reasoning selection, catalog revision/fetch time, and adapter/runtime version.
The API key or OAuth token must never be copied into a Tender Store, run record,
conversation, diagnostic bundle, or log.

### Settings behavior

The beginner-facing AI settings can be:

1. **Connections:** Connect OpenAI account; add/test/remove Anthropic API key;
   add/test/remove Gemini API key.
2. **Default model:** group the live model catalog by connected provider.
3. **Reasoning:** populate choices from that selected model's live capability
   record. For Gemini, show Automatic until a live exact source exists.
4. **Connection details:** last successful refresh, catalog timestamp, account
   or masked-key label, quota/readiness state, and a repair/reconnect action.

“Connect” and “Test key” must validate credentials against the provider before
marking a connection ready. Settings can cache the last successful catalog for
explanation, but a stale catalog must not authorize a new Agent Run.

### Offline and changed-capability behavior

- Quantix opens and all non-AI Tender work remains available when every
  provider is offline.
- Each connection has its own state: disconnected, checking, ready,
  authentication required, temporarily unavailable, quota limited, or
  incompatible.
- A failed provider must not disable other ready providers.
- If a selected model or reasoning value disappears on refresh, keep the old
  selection visible as unavailable, pause new runs that depend on it, and ask
  the Engineer to choose from the fresh catalog. Never silently switch provider,
  model, or reasoning depth.
- Existing/running runs retain their captured provider configuration. Retries
  use the same configuration unless the Engineer explicitly approves a new run.
- A provider rejection caused by model/capability drift triggers one catalog
  refresh and then fails closed with an actionable explanation.

## Decisions this research does not settle

1. Whether OpenAI will contractually approve Quantix's public/commercial use of
   ChatGPT-account-backed Codex access. Official app-server support establishes
   a technical path, not a bespoke distribution grant.
2. Which operating-system secret-store library Quantix should use for Anthropic
   and Gemini keys.
3. Whether Quantix should later support Anthropic Workload Identity Federation,
   Google OAuth/service-account connections, the OpenAI API, or local models.
   They should not be smuggled into the first multi-provider slice.
4. Whether the product requirement should relax “never hardcode effort” for
   Gemini. Without such a relaxation or a new Google capability endpoint,
   explicit Gemini effort choices cannot be delivered truthfully.

## Evidence-based conclusion

Quantix can become genuinely AI-powered without making Codex the product's
identity. The smallest honest first layer is one official account-backed
provider (OpenAI through supervised Codex app-server) plus two direct BYOK
providers (Anthropic and Gemini), with live model catalogs and provider-native
reasoning controls. The architecture must make capability provenance visible:
Codex and Anthropic can offer live named effort choices today; Gemini can offer
live models and live thinking support, but only provider-default reasoning under
the stated no-hardcoding rule.
