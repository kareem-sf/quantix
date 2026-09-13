# Simple AI connections implementation contract

The subsequent [subscription connection contract](subscription-connections.md) supersedes the API-only provider scope. This document continues to describe the direct API branch; eligible ChatGPT/Codex and Grok subscriptions use their separate official-client branch.

The engineer requests a working, fast connection experience and explicitly approved API-key-only scope for OpenAI, Anthropic, Google, xAI and BYOK/custom, acknowledging separate API billing. Subscription sign-in is removed from new setup and execution. Do not delete Tender data, transfer subscription credentials to APIs, enable extra spending or silently change an approved account/model. Existing subscription records remain inspectable as retired connections; they cannot run or acquire API readiness.

## Direct runtime

Normal API connections use bundled Python dependencies, with no managed component download, native launcher or MCP subprocess. Declare the installed maintained versions: pydantic-ai-slim 2.40.0, openai 3.8.0, anthropic 1.4.0, google-genai 2.22.0. Reuse the existing Pydantic AI loop and SDK bindings for only the supported protocols. Preserve local source-tool validation, sequential tools, run/cancellation checks, image capability enforcement, validated output, web references and per-request budget callbacks. Core Office publication remains unchanged.

New module `quantix.ai_direct` exports `supports_direct(connection)->bool`, `direct_runtime_status(connection)->dict` and `DirectAPIService(repo)` with async `catalog(connection,credentials)`, `check(route,connection,credentials,*,before_request=None,on_response=None)`, `execute(route,connection,credentials,context,instruction,output_type,*,consult=None,before_request=None,on_response=None)`, and `close()`. Return shapes match the current host worker catalog/check/execute methods. `check` returns usage, actual_model, tools_supported, output_supported and checked_count. `execute` returns validated Pydantic output, usage and web_sources.

Supported direct profiles: openai (`openai_responses`, `openai_chat`), anthropic (`anthropic`), google (`google`), xai (`openai_responses`, `openai_chat`), custom/BYOK (`openai_chat`, `openai_responses`). API/environment authentication uses explicit profile credentials only. Official providers are metered; custom is unknown/metered, never a user-supplied local/subscription billing escape. Other saved profiles remain inspectable and cannot silently acquire a new route.

Runtime status reflects the installed bundled adapter/dependencies and has a stable API-adapter revision separate from all original-client worker fingerprints. A genuinely missing bundled dependency is an actionable installation error, not a fake prepared state. SDK clients are request scoped, explicitly authenticated, bounded, nonredirecting and have no hidden retry loops. Do not log provider bodies/keys/prompts.

## Connection and check

The main flow is provider → key and connection details → model → Check connection. Save credentials in the existing secure store. Authenticated model listing is a metadata operation; it does not prove model inference. Missing model listing on a custom endpoint must permit explicit manual model entry. Invalid credentials must produce a useful error, not a green connection status.

The direct model check is a generic random-value tool exchange, without Tender content, at most two model requests, at most 1024 output tokens per request, and a 90-second total deadline. A named input allowance of 16384 serialized-input bytes per request bounds the sample and is used consistently in preview and meter; fail before sending if its actual serialized prompt plus tool/schema overhead exceeds that allowance. Hide the output tool until the check tool has run; hide the check tool after its first call. Validate the unpredictable returned value and exactly one call.

Known prices use the existing fingerprint-bound separate setup meter. Unknown pricing must not block key/model setup forever: show an explicit unknown-cost notice and require consent before this bounded test. Preserve `maximum_cost_usd=null`; do not promise a monetary cap or charge zero. Record the consent and token/request limits in the separate check ledger. Passing this test never bypasses the existing Tender spending requirements.

Shared schema additions: `SetupAccount.supported:bool=true`; `SetupCheckPreview.requires_unknown_cost_consent:bool=false`; `SetupCheckPreview.max_input_tokens:int=16384` (a conservative byte-based input allowance, not a token measurement); `SetupAction.accept_unknown_cost:bool=false`. Retired profiles return supported=false. Preserve JSON backward-readable existing records and defaults. No unpriced request without a matching preview/fingerprint and explicit consent. Health AI setup revision becomes 6; new interface/launcher must not treat a revision-5 service as current.

Use live provider model discovery plus existing reviewed model/price metadata. Add only source-confirmed exact starter metadata where needed. Preserve requested and actual model identity. Normalize Google's documented `models/` prefix; accept aliases only through explicit provider-backed mapping, never arbitrary string prefixes or price-library matching. An unrecognized resolution is an actionable model-selection error.

## Setup integration and interface

Setup, direct catalog discovery, readiness and Office API dispatch use the direct service. No per-account installation action is needed for bundled APIs. Keep ready evidence tied to exact connection revision/model and direct-adapter version. A failed or stale check shows its actual next action and preserves fields/model selection. All old client install/login/discovery/execution entry points refuse retired profiles with clear guidance; no automatic account conversion.

The interface presents five provider groups, a compact account list and one focused editor. API key is write-only and masked, model selection/manual entry is simple, and advanced endpoint/protocol/rate controls are in More options. Separate subscription billing from API billing in the relevant choice. No four-stage preparation wizard for direct API access. Testing state has cancellation and an actionable error/reference. Do not bury the recovery action or expose the full old advanced-connection interface as a second competing UI.

## Ownership and verification

Runtime implementer owns new `ai_direct.py`, `ai_api_provider.py`, `ai_api_engine.py`, `ai_api_errors.py`, `ai_api_catalog.py`, dependency declarations/lock and packaging recipe. Setup implementer owns `ai_setup.py`, setup catalog/models/store/routes, `ai_catalog.py`, `ai_connections.py`, `ai_readiness.py`, `ai_execution.py`, `ai_routes.py` and narrow policy integration. UI implementer owns the AI connection/setup/editor React surface and styles. Primary owns architecture, docs, integration, generated frontend declarations and final verification. Implementation uses Luna/xhigh, review Astra/xhigh.

Use focused AI connection diagnostics with synthetic HTTP responses through real SDK serialization for all four provider families, key/catalog errors, the tool/schema exchange, cancellation, budget refusal and unknown-cost consent. Run live checks only with explicitly configured credentials and user-authorized billing. Do not claim providers live-verified from simulated responses. No broad suites, lint, typechecks or release builds. Storage remains under `~/.quantix` and existing dirty work is preserved.
