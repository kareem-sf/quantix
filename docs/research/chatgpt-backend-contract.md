# ChatGPT Codex Backend API Contract — Research Spike

Date: 2026-08-22 · Task 10 (read-only spike) · No live calls made; no credentials handled.

Primary sources are OpenCode `dev` branch files (fetched raw from GitHub) plus the pinned
local schema file. Secondary web sources only corroborate; anything uncorroborated is marked
**ASSUMPTION**.

- [codex.ts] https://raw.githubusercontent.com/anomalyco/opencode/dev/packages/opencode/src/plugin/openai/codex.ts
- [transform.ts] https://raw.githubusercontent.com/anomalyco/opencode/dev/packages/opencode/src/provider/transform.ts
- [ws.ts] / [ws-pool.ts] / [README] https://raw.githubusercontent.com/anomalyco/opencode/dev/packages/opencode/src/plugin/openai/{ws.ts,ws-pool.ts,README.md}
- [retry.ts] .../packages/opencode/src/session/retry.ts · [processor.ts] .../packages/opencode/src/session/processor.ts
- [schemas] local: `src-tauri/runtime/codex_app_server_protocol.schemas.json` (pinned Codex app-server protocol)
- [ai-sdk-lm] https://raw.githubusercontent.com/vercel/ai/main/packages/openai/src/responses/openai-responses-language-model.ts
- [ai-sdk-opts] .../responses/openai-responses-language-model-options.ts · [ai-sdk-tools] .../responses/openai-responses-prepare-tools.ts

---

## 1. Request body — `POST https://chatgpt.com/backend-api/codex/responses`

Endpoint is hardcoded in OpenCode's plugin ([codex.ts:12]); requests to `/v1/responses` or
`/chat/completions` on provider "openai" are rewritten to it ([codex.ts:419-420]). The wire body
is the standard **OpenAI Responses API** shape, built by the AI SDK (`@ai-sdk/openai`, responses
path) and passed through unchanged — only headers and URL change in the plugin fetch wrapper.

| Field | Value / shape | Source |
|---|---|---|
| `model` | string model id, e.g. `"gpt-5.5"`. OpenCode sends whatever model id survived its filter (see §6). | [ai-sdk-lm:456] (`model: this.modelId`) |
| `instructions` | string system prompt (top-level field, not a message). OpenCode passes `openaiOptions.instructions`. Quantix maps its agent prompt here. | [ai-sdk-lm:471]; [ai-sdk-opts] `instructions` option |
| `input` | array of Responses items (below), not chat messages. Assistant reasoning items are replayed with `encrypted_content`; item `id`s are stripped before serialization when `store !== true` ("following Codex"). | [transform.ts:502-514] |
| `tools` | array of `{ type: "function", name, description, parameters: <JSON Schema>, strict? }`. Host-side tools are declared as plain function tools. | [ai-sdk-tools] `prepareFunctionTool()` |
| `store` | `false` — required for stateless OAuth multi-turn (no server-side storage). OpenCode sets `{ store: false }` as base options for every openai-provider small-model call. | [transform.ts:1327-1335]; AI SDK default is `true`, so must be sent explicitly [ai-sdk-lm:360] |
| `include` | `["reasoning.encrypted_content"]` — returns encrypted reasoning state needed to continue reasoning across turns with `store:false`. Attached to every reasoning variant for openai/azure/copilot providers. | [transform.ts:20-23, 916-976, 1300-1319, 1756-1761] |
| `stream` | `true` — all executor turns stream SSE. | [ws-pool.ts:61] gates WS on `body.stream`; HTTP path streams identically |
| `reasoning.effort` / `reasoning.summary` | nested object `reasoning: { effort, summary }`; effort from variant selection (§6), summary `"auto"` (OpenCode default; AI SDK defaults to `"detailed"` unless effort is `none`). | [transform.ts:1756-1761]; wire mapping [ai-sdk-lm:498-507], default summary [ai-sdk-lm:293-297] |
| `text.verbosity` | nested `text: { verbosity }`, values `low\|medium\|high` (schema enum confirms lowercase serialization). OpenCode sets `"low"` by default for gpt-5.x non-codex non-chat models. For codex models it is omitted → server default. | [transform.ts:1308-1315]; wire [ai-sdk-lm:446-463]; enum [schemas:22278-22286] |
| `prompt_cache_key` | set to session id for openai npm path — improves cache hit; harmless for backend. Optional for v0. | [transform.ts:1265-1271] |
| Forbidden fields | `max_output_tokens` / `max_tokens` / `metadata` are rejected by this backend; OpenCode explicitly undefines maxOutputTokens ("Match codex cli"), LiteLLM corroborates rejection. Do not send them. | [codex.ts:566-570]; https://docs.litellm.ai/docs/providers/chatgpt |

### Input item shapes (from pinned Codex protocol schemas, oneOf `v2/ResponseItem` at line 16408)

- **message**: `{ type: "message", role: "user"|"assistant"|..., content: [ContentItem...] , id? }`
  ([schemas:16408-16461]). Content items carry text via `input_text`/`output_text`
  ({ type, text }) ([schemas:6480-6520, 11107 area]).
- **reasoning** (assistant turn replay): `{ type: "reasoning", summary: [...], encrypted_content?: string|null, id? }`
  — encrypted_content round-trips the prior turn's reasoning ([schemas:16517-16563]).
- **function_call** (model → host): `{ type: "function_call", name, arguments: string (JSON), call_id?, id? }`
  ([schemas:16615-16670]). Arguments arrive as a JSON *string*.
- **function_call_output** (host → model): `{ type: "function_call_output", call_id, output }`,
  both required strings ([schemas:16743-16760]). This is how tool results return.
- Other variants exist (`agent_message`, `local_shell_call`, `custom_tool_call[_output]`,
  `tool_search_call`) but are not needed for the v0 executor ([schemas:16466-16862]).

## 2. SSE event inventory

Stream is standard Responses-API SSE: each event arrives as an SSE `data:` frame containing one
JSON object with a `type` discriminator (`data: {...}\n\n`). OpenCode's WS bridge re-emits exactly
these objects as SSE frames, confirming parity of event vocabulary between transports
([ws.ts:242-249]).

Events consumed by the reference client (AI SDK responses stream parser):

| Event | Payload highlights | Source |
|---|---|---|
| `response.created` | `{ response }` snapshot with `id` | [ai-sdk-lm:814] |
| `response.output_item.added` | `{ output_index, item }` wrapper (item has `id`, `type`) | [ai-sdk-lm:832] |
| `response.output_text.delta` | `{ item_id, delta }` incremental assistant text | [ai-sdk-lm:725] |
| `response.output_text.annotation.added` | annotations (ignorable for v0) | [ai-sdk-lm:1255] |
| `response.reasoning_summary_part.added` / `.done` | reasoning summary grouping | [ai-sdk-lm:1286,1299] |
| `response.reasoning_summary_text.delta` | `{ delta }` visible reasoning text | [ai-sdk-lm:1292] |
| `response.function_call_arguments.delta` / `.done` | streamed tool-call arguments string | [ai-sdk-lm:1225] |
| `response.output_item.done` | completed item incl. final `function_call` items | [ai-sdk-lm:980] |
| `response.completed` | full `response` object: `id`, `output[]`, `usage { input_tokens, input_tokens_details.cached_tokens, output_tokens, output_tokens_details.reasoning_tokens, total_tokens }` | [ai-sdk-lm:745, 2565-2575] |
| `response.incomplete` | terminal; `response.incomplete_details.reason` + usage | [ai-sdk-lm:745] |
| `response.failed` | terminal failure; `response.error`, usage may be present | [ai-sdk-lm:775, 2582-2600] |
| `error` | in-band error object `{ code, message }` | [ws-pool.ts:198-200] |

**Terminal detection**: stream ends on `response.completed` (success) or `response.failed` /
`response.incomplete` / `error` (failure); OpenCode then emits `data: [DONE]`.
[ws.ts:255-267], [ws-pool.ts:109]. An SSE connection that closes before a terminal event is an
error condition (retryable) [ws.ts:274-281].

## 3. Required headers

Set by OpenCode per request ([codex.ts:396-413, 556-564]):

| Header | Exact form | Notes |
|---|---|---|
| `Authorization` | `Bearer <access_token>` (lowercase key used by opencode; HTTP headers are case-insensitive) | token refreshed via `auth.openai.com/oauth/token` `grant_type=refresh_token` [codex.ts:135-149] |
| `ChatGPT-Account-Id` | account UUID extracted from JWT claims: `chatgpt_account_id` → nested `https://api.openai.com/auth.chatgpt_account_id` → `organizations[0].id` fallback | [codex.ts:38-78, 411-413]; Codex CLI sends PascalCase `ChatGPT-Account-ID` (https://github.com/openai/codex/blob/main/codex-rs/codex-api/src/auth.rs, cited by pi-mono issue #1828) — casing is irrelevant per HTTP spec |
| `originator` | `opencode` (design decision #8 for Quantix: `quantix`) | [codex.ts:558]; also sent in authorize params [codex.ts:99] |
| `User-Agent` | `opencode/<version> (<platform> <release>; <arch>)` e.g. `opencode/0.x (windows 10.0.26100; x64)` | [codex.ts:559] |
| `session-id` | hyphenated header name; OpenCode sets its internal session id (`ses_...`). Codex CLI uses a per-thread/conversation UUID. **Quantix recommendation:** stable UUIDv4 per agent-run thread. | [codex.ts:560]; WS pool keys affinity off it [ws-pool.ts:66-70] |
| `x-openai-internal-codex-residency` | only if JWT carries non-`no_constraint` `chatgpt_compute_residency` | [codex.ts:80-86, 421-424] |
| `Content-Type` | `application/json` (request); response `Content-Type: text/event-stream` | [ws.ts:339] |
| `OpenAI-Beta` | **not sent on the REST+SSE path by current OpenCode**. On WebSocket upgrades it is `openai-beta: responses_websockets=2026-02-06` ([ws.ts:11, 81]). Older clients/Codex CLI historically sent `responses=experimental` over SSE. ASSUMPTION (uncorroborated either way): harmless/optional on SSE; omit unless a live check proves otherwise. | |

Originator caveat: pi-mono issue #1828 (2026-03-05,
https://github.com/badlogic/pi-mono/issues/1828) reports the endpoint 403s unknown originators,
claiming a whitelist (`codex_cli_rs`, `codex_vscode`, `codex_sdk_ts`, `Codex ` prefix) in
codex-rs/core/src/default_client.rs:111-114. OpenCode ships `originator: opencode` successfully,
so the gate is evidently not a strict whitelist today (or UA factors in). Conflict noted;
recommendation: keep design decision #8 (`originator=quantix`) but make the value a single
constant and verify on the first Phase-2 live run.

## 4. Rate limits & usage surfaces

- **Usage counts** appear in-band in the `response.completed` (and `.incomplete`/`.failed`)
  payload under `response.usage`: `input_tokens`, `output_tokens`, `total_tokens`,
  `input_tokens_details.cached_tokens`, `output_tokens_details.reasoning_tokens`.
  OpenCode reads them from the SDK's step-finish event into its Usage/tokens model
  ([processor.ts, `step-finish` case]; [ai-sdk-lm:2565-2575]).
- **Rate limits**: no dedicated rate-limit header parsing exists in OpenCode's happy path.
  Retry logic reads `retry-after-ms` / `retry-after` **headers from error responses** (esp. 429),
  classifies `/rate limit|too many requests/i` messages, and backs off exponentially capped at 30s
  without headers ([retry.ts]). Subscription limits are plan-bound rolling windows (secondary:
  https://docs.litellm.ai/docs/providers/chatgpt; codex.danielvaughan.com article).
- In-band `rate_limit` snapshots (as seen in some Codex CLI telemetry) were not observed in the
  OpenCode SSE handling — treat any such event as ignorable-if-unknown. The parser must tolerate
  unknown `response.*`/other event types (AI SDK ignores unmatched types; our executor should too).

## 5. Model + reasoning catalogue

**Discoverable endpoint**: secondary sources indicate `GET https://chatgpt.com/backend-api/codex/models`
exists (Codex-style model catalog exposing id/context window/supported reasoning levels):
https://github.com/lobbystack/codex-model-manager (proxies it) and
https://pypi.org/project/codex-backend-sdk/ (`client.models.list()` with
`context_window`, `supported_reasoning_levels`). OpenCode does **not** call it — it filters
models.dev locally. We did not call it (hard rule). Treat it as **unverified but likely present**;
do not depend on it for v0.

**Built-in seed catalogue** (ship versioned in Quantix): ids from OpenCode's allowlist
[codex.ts:15-16], display names from models.dev api.json (openai provider), effort sets derived
from [transform.ts:574-582, 628-644]:

Rules recap: GPT‑5.1+ replaced `minimal` with `none`; GPT‑5.2+ adds `xhigh`; `-pro`: high-tier
only; codex ≥v3 adds `none`; `-chat`: medium only.

| api.id | Display name | Supported efforts | Context/output caps |
|---|---|---|---|
| `gpt-5.5` | GPT-5.5 | none, low, medium, high, xhigh | 400k ctx / 128k out (plugin override) [codex.ts:312-320] |
| `gpt-5.4` | GPT-5.4 | none, low, medium, high, xhigh | models.dev |
| `gpt-5.4-mini` | GPT-5.4 mini | none, low, medium, high, xhigh | models.dev |
| `gpt-5.3-codex-spark` | GPT-5.3 Codex Spark | none, low, medium, high, xhigh (codex ≥v3) | 128k ctx / 32k out |
| excluded | `gpt-5.5-pro`, `gpt-5.6` (and any `reasoningMode: pro`) | — | [codex.ts:16, 296-299] |

Forward-compat rule in OpenCode worth mirroring: ids matching `^gpt-(\d+\.\d+)` with version > 5.4
are allowed except explicit disallows ([codex.ts:300-301]) — i.e., new majors flow in without a
code change; encode that rule in the versioned catalogue rather than a frozen list alone.

## 6. WebSocket vs REST+SSE

- The WS transport is strictly opt-in: `options.experimentalWebSockets` constructs the pool;
  otherwise every request goes through normal `fetch` (HTTP+SSE) [codex.ts:276, 329-336].
  README confirms env-gating on prod (`OPENCODE_EXPERIMENTAL_WEBSOCKETS=true`) and that HTTP is
  the fallback for: missing session-id, title requests, busy socket, fallback mode, >5 stream
  failures [openai/README.md].
- WS requires the same body (`response.create` = request body minus `stream`/`background`)
  [ws.ts:311-313] and the same event vocabulary; terminals identical [ws.ts:255-267].
- Nothing in the auth loader or request rewriting requires WS.

**GO — REST+SSE is sufficient for v0** (plain POST to
`https://chatgpt.com/backend-api/codex/responses` with `stream:true`; no WebSocket dependency).

## Top risks / uncertainties for the executor build

1. **Originator gate**: possible first-party originator enforcement (pi-mono #1828 vs OpenCode's
   working `originator: opencode`). Verify `originator=quantix` on first live run; isolate the
   constant.
2. **Reasoning replay correctness**: multi-turn requires echoing `reasoning` items with their
   `encrypted_content` and stripping item ids; malformed replay may 400 silently-differently than
   public API. Test turn-2+ early.
3. **Undocumented surface drift**: no SLA; fields like `session-id`, residency header, and the
   models catalog could change without notice; catalogue should be versioned built-in data with a
   documented update path.
