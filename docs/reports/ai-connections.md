# AI connections implementation

Implemented 7 September 2026. This increment has not been tested. No tests, lint,
typechecks, browser checks, provider login, model discovery or live inference were
run during development. Provider SDK source was inspected directly.

## Owned interfaces

`AIConnectionService(repo)` implements the JSON methods in `docs/contracts.md`:
presets, list/get, create/update/delete, private credentials, models/save_model,
explicit asynchronous discover, mark_used and mark_error. `lease(id)` keeps a
connection stable for a whole execution; discovery takes an exclusive lease.
Changes to connection configuration or model metadata increment the connection
revision. Status updates alone do not invalidate approved route snapshots.

`async with model_for_route(route, connection, credentials) as binding` yields:

- `binding.model`: the Pydantic AI model instance.
- `binding.settings`: output limit, timeout and exact provider reasoning/routing settings.
- `binding.client`: private SDK client, also used by explicit catalog discovery.
- `binding.cloud_session`: private AWS session when applicable.

The root execution service owns Tender allowlists, capability requirements,
native search/tool selection, request budgets, response accounting, cancellation
and final publication. The factory does not run inference or authenticate at
import time. It closes owned clients on exit and disables SDK retries where the
constructed SDK exposes that setting; root remains responsible for any explicit
retry or fallback decision.

## Credentials and storage

Public connection metadata and manual/discovered models are in the workspace
SQLite database. The migration marker has its own `ai_connection_metadata` table
so backup filtering of ordinary settings cannot repeat the migration.

Credential values are stored in an OS-backed keyring, or only in process memory
when explicitly selected. Unknown/plaintext keyring backends are rejected. The
public schemas contain credential state, never credential values or references.
Supported private names are `api_key`, AWS access/secret/session values,
`service_account_json` and Azure `client_secret`. Azure tenant/client identifiers
and Google project/location are public settings. Environment authentication uses
only the explicitly named variable. Identity authentication uses the selected
cloud SDK's credential chain and does not imply that login has succeeded.

One-time migration preserves the previous model and copies a saved OpenAI key
into a new immutable keyring reference. The old key is left intact and cannot
mutate the new connection. An existing `OPENAI_API_KEY` remains an explicit
environment connection. No Tender gains approval automatically from migration.
Migration retries later if the OS keyring is unavailable.

Updates cannot silently carry credentials to another destination. Changed
provider/protocol/endpoint/identity scope requires replacement or explicit
credential clearing. Existing environment credentials need a separate connection
when reused at another destination. HTTP is allowed by default only on loopback;
remote HTTP requires an explicit setting. Endpoint URLs reject embedded
credentials, query parameters and fragments. API clients do not follow redirects
or inherit proxy settings implicitly. Provider errors saved on the public
connection record are generic to avoid recording request payloads or credentials.

## Catalog and protocols

Presets include OpenAI, Anthropic, Gemini API, Google Cloud/Vertex, xAI, DeepSeek,
Mistral, Kimi, Alibaba/Qwen, Z.ai, MiniMax, Cohere, OpenRouter, Bedrock, Foundry,
Ollama, LM Studio and a custom endpoint, plus the five official-client profiles
owned by the runtime service. Each preset separates protocol, authentication,
billing, public settings and official documentation.

Native SDKs construct Anthropic, Google, Bedrock, Mistral and Cohere models.
OpenAI SDK Chat/Responses clients use vendor-specific Pydantic AI profiles where
available. Foundry supports resource keys and Azure identity, including its
Anthropic SDK client. Native Bedrock uses AWS identity and Converse; compatible
Bedrock protocols require the explicit compatible endpoint and bearer key.
Google Cloud API-key mode uses Express access; project/location access uses
Google identity. MiniMax Chat preserves thinking through the default think-tag
format. Unsupported reasoning options raise an error instead of being mapped to
a nearby level by the SDK.

Capabilities default to unknown. Discovery imports only explicitly returned
metadata; manual declarations remain manual and are not overwritten by discovery.
An exact documented Astra entry carries its published context/output/reasoning
and tool/image/structured-output capabilities. It does not establish tested access.
Some catalogs only list models and cannot establish deployment access or feature
support; manual model entry is available throughout.

OpenRouter requires an upstream allowlist before inference, sets
`require_parameters=true`, and defaults provider fallback off. An enabled fallback
stays inside the explicit upstream allowlist. There is no automatic model fallback.
Its hosted-search engine and tool charges remain the execution service's explicit
decision. OpenRouter catalog prices are estimates; selected upstream prices can
differ.

Bundled `genai-prices` data is read without its network updater. Known direct
provider models can receive a conservative maximum tier/time token rate card,
identified as an offline package snapshot. Unknown models and cloud deployment
aliases require manual rates. Astra uses a separately documented conservative
Standard ceiling of $25/million input and $75/million output, with $0.01 per search,
covering the published long-context/cache-write token tiers. These are reservation
estimates, not invoices. Extra non-token pricing cannot be inferred as zero.

## Source references and integration limits

- [Pydantic AI provider source, pinned v2.40.0](https://github.com/pydantic/pydantic-ai/tree/v2.40.0/pydantic_ai_slim/pydantic_ai/providers)
- [Astra model documentation](https://developers.openai.com/api/docs/models/gpt-6-astra)
- [OpenAI pricing](https://developers.openai.com/api/docs/pricing)
- [Anthropic structured-output restrictions](https://platform.claude.com/docs/en/build-with-claude/structured-outputs)
- [Google API keys](https://ai.google.dev/gemini-api/docs/api-key)
- [Google Cloud authentication](https://docs.cloud.google.com/gemini-enterprise-agent-platform/models/start)
- [DeepSeek compatibility and silently ignored fields](https://api-docs.deepseek.com/guides/responses_api/)
- [Mistral models and capabilities](https://docs.mistral.ai/api/endpoint/models)
- [Kimi API protocols](https://platform.kimi.ai/docs/overview)
- [Qwen region/key pairing](https://www.alibabacloud.com/help/en/model-studio/get-api-key)
- [Qwen Token Plan entitlement](https://www.alibabacloud.com/help/en/model-studio/token-plan-overview)
- [Z.ai API surface](https://docs.z.ai/api-reference/llm/chat-completion)
- [MiniMax continuation](https://platform.minimax.io/docs/api-reference/text-openai-api)
- [Cohere compatibility limits](https://docs.cohere.com/docs/compatibility-api)
- [OpenRouter routing](https://openrouter.ai/docs/guides/routing/provider-selection)
- [Bedrock API/endpoint differences](https://docs.aws.amazon.com/bedrock/latest/userguide/apis.html)
- [Foundry deployments and authentication](https://learn.microsoft.com/en-us/azure/foundry/foundry-models/concepts/endpoints)
- [Ollama partial compatibility](https://docs.ollama.com/api/openai-compatibility)
- [LM Studio authentication](https://lmstudio.ai/docs/developer/core/authentication)

Search availability, strict JSON, image/PDF interpretation, subscription
entitlements, storage and geographic processing cannot be inferred from an API
format or provider brand. Localhost can still front an Ollama cloud model or a
remote MCP service. Native Mistral hosted search needs its separate Conversations
API; Kimi chat search requires its documented built-in continuation. These are
not silently substituted for generic web-search tools. The engineer's later
integration session must establish working access and task suitability.
