# Subscription integration options — 9 September 2026

Research scope: OpenAI, xAI, Google and Anthropic subscription access for an individual engineer using Quantix locally. These notes reflect official pages opened on 9 September 2026. They distinguish a supported client interface, subscription entitlement, billing treatment, and permission for a third-party product; one does not establish the others.

The engineer's current request adds subscription options alongside APIs and supersedes the earlier API-only product decision for this investigation. This report changes no connection behaviour. Existing API-key connections must remain available with their own credentials and billing authority.

## Decision matrix

| Provider | Subscription use in official software | Third-party Quantix route | Recommendation |
| --- | --- | --- | --- |
| OpenAI | Codex supports managed ChatGPT sign-in separately from API-key access. Eligibility and limits belong to the signed-in plan/workspace. | The official SDK/app-server is documented for embedding Codex and its authentication in a custom application. | Use the original Codex client through its Python SDK. Let it manage sign-in and token refresh; do not extract tokens into the direct OpenAI API adapter. |
| xAI | Grok Build supports account sign-in. Its launch names SuperGrok and X Premium Plus; other integration announcements describe different eligible tiers. | Official Grok Build headless/ACP integration is explicitly documented for custom bots and agent orchestration applications. | Proceed with an original-client adapter after the existing implementation is reviewed and targeted setup diagnostics pass. Determine actual entitlement from the account. |
| Google | Google now directs individual/free/AI Pro/Ultra Gemini CLI users to Antigravity. Older Gemini CLI pages still describe those subscriptions. | Gemini CLI has documented ACP, but OAuth harvesting/piggybacking is expressly restricted. Antigravity's consumer terms expressly restrict third-party service access; its SDK documents API/Vertex credentials. | Keep Google's direct API route. Do not promise a personal subscription provider inside Quantix from the older Gemini CLI documents. |
| Anthropic | Subscription use of original Claude Code and personal Agent SDK/`claude -p` usage exists. | Hosting the unmodified client is expressly allowed subject to conditions. Own-product subscription login/credential routing is restricted, and the Agent SDK requires prior approval for product subscription login/rate limits. Quantix's own Tender interface is not explicitly addressed by the hosted-client exception. | Keep Anthropic API active. A Claude subscription connection in Quantix remains unresolved; do not label all third-party hosting prohibited. |

## xAI: supported original-client integration

The [Grok Build announcement](https://x.ai/news/grok-build-cli) explicitly combines subscriber access with headless scripts and ACP-based custom bots/agent orchestration. This is affirmative support beyond merely documenting a sign-in command. The [headless/ACP guide](https://docs.x.ai/build/cli/headless-scripting) specifies:

- `grok -p` with `--output-format json` or `streaming-json` for scripting.
- `grok agent stdio` for JSON-RPC ACP. Its example authenticates with a locally cached login or an explicit API key, then uses `session/new`, `session/prompt` and streamed `session/update` messages.
- `--no-auto-update` for automated runs.

The [enterprise guide](https://docs.x.ai/build/enterprise) documents browser login, `grok login --device-auth`, external auth providers and API keys. It also documents credential precedence, so isolation must prevent an inherited key from unexpectedly changing the selected billing route. The original client should own its authentication and token refresh.

xAI also explicitly supports subscriptions in [OpenCode](https://x.ai/news/grok-opencode) and [Hermes](https://x.ai/news/grok-hermes). Hermes is described as a persistent personal agent with memory and messaging integrations, establishing that sanctioned subscription integrations extend beyond code editing. Those named integrations do not by themselves establish permission to copy their OAuth clients or reuse arbitrary private backend endpoints; Grok Build's documented interface is the concrete Quantix route.

The existing [historical Grok integration record](../grok-subscription.md) describes pinned Grok Build 1.0.13 with ACP setup/billing and headless execution. This research reaffirms that route's documentation basis, not its current correctness. Preserve separate account/model validation, Tender data grants, spending approval and cancellation. Do not treat subscription allowance, API credit and provider-managed extras as interchangeable or promise unlimited/included usage merely because login succeeds.

## Google: documentation conflict and successor restrictions

The current [Gemini CLI authentication page](https://geminicli.com/docs/get-started/authentication/) still names Google AI Pro/Ultra and states that headless mode can use previously cached authentication. Its [ACP guide](https://geminicli.com/docs/cli/acp-mode/) documents `gemini --acp` over stdio for programmatic IDE/developer-tool integration. Google also published an official [Zed integration](https://developers.googleblog.com/en/gemini-cli-is-now-integrated-into-zed/). It would therefore be inaccurate to claim that every external client controlling Gemini CLI is forbidden.

However, Google's [19 May migration announcement](https://developers.googleblog.com/an-important-update-transitioning-gemini-cli-to-antigravity-cli/) explicitly says individual/free/AI Pro/Ultra Gemini CLI requests stop on **18 June 2026**. Standard/Enterprise Code Assist licences and paid API-key access remain supported. The opened announcement contains no reversal notice. This conflicts with the older subscription statements still present on Gemini CLI pages; live entitlement was not checked.

The [Gemini CLI FAQ](https://geminicli.com/docs/resources/faq/) expressly rejects third-party software harvesting or piggybacking on its OAuth to access backend services and directs third-party agents to Vertex AI or AI Studio API keys. That restriction is specifically about access/authentication; it is not evidence that all documented ACP integrations are disallowed.

For the successor, [Antigravity plans](https://antigravity.google/docs/plans) include CLI access and higher Pro/Ultra quotas, with optional AI-credit overages. But clause 6 of the [Antigravity additional terms](https://antigravity.google/terms) expressly treats access through third-party software/services, exemplified by OpenClaw using Antigravity OAuth, as a breach. Enterprise/Cloud access is governed separately under the opening paragraph. The [official Antigravity SDK](https://antigravity.google/docs/sdk/overview) provides a local Python runtime for custom agents and documents `GEMINI_API_KEY` or Vertex/Gemini Enterprise Agent Platform authentication. No subscription-backed Quantix integration permission was established from these sources.

The historical [Gemini CLI SDK README](https://github.com/google-gemini/gemini-cli/blob/main/packages/sdk/README.md) establishes a programmatic package, but does not establish a consumer subscription entitlement or override the service restrictions. Do not replace the old worker with that SDK merely to preserve subscription access.

## Anthropic: hosted client exception, unresolved Tender UI

The current [legal/compliance page](https://code.claude.com/docs/en/legal-and-compliance), sections “Can customers offer Claude Code in their products?” and “Authentication and credential use”, permits hosting/preinstalling the published, unmodified binary under Commercial Terms. Built-in authentication methods must remain available; users authenticate and pay directly. It also forbids a product's own Claude.ai login and credential/session-token intermediation, while expressly preserving end-user sign-in to the hosted original binary. These are distinct routes.

The [Agent SDK overview](https://code.claude.com/docs/en/agent-sdk/overview) still requires prior approval for developers offering subscription login/rate limits in their products and directs them to API authentication. The hosted-client exception does not explicitly explain whether Quantix may present its own login controls and Tender interface while orchestrating `claude -p`/MCP and consuming results. Preserving all client auth choices alone does not resolve that boundary. Enable this only after a clearly supported design or provider clarification; do not describe it as categorically illegal.

Technical availability is broader than that conclusion: [authentication](https://code.claude.com/docs/en/authentication) documents subscription login and `claude setup-token` for personal scripts/CI. [Programmatic usage](https://code.claude.com/docs/en/headless) documents `-p`, structured output and tool control; `--bare` does not use subscription OAuth. A working token or subprocess is not sufficient product authorization.

The [16 June subscription help update](https://support.claude.com/en/articles/15036540-use-the-claude-agent-sdk-with-your-claude-plan) pauses the previously announced SDK monthly-credit change: Agent SDK, `claude -p` and third-party app usage continue drawing from subscription usage limits. The older credit table below its update is explicitly historical and must not be implemented as current billing. This billing statement does not remove the separate developer-authentication restrictions.

## Implementation consequence and evidence limits

Prioritize xAI's documented original-client route alongside the separate OpenAI assessment. Keep all existing API adapters and never silently convert a retired connection or switch billing after subscription failure. Present unsupported/unresolved options truthfully rather than offering a login that suggests Tender execution is supported.

Research only: no credentials were read, no inference was sent, and no subscription account's current entitlement, quota or spending settings were checked. No tests, lint, typechecks, browser QA or builds were run. Source/document inspection is not runtime verification.

## OpenAI: original Codex app-server integration

The [Codex SDK guide](https://learn.chatgpt.com/docs/codex-sdk) explicitly describes application integration and the stable Python library, which controls the local app-server using a pinned CLI dependency. The [app-server guide](https://learn.chatgpt.com/docs/app-server) documents custom-client authentication and its managed ChatGPT browser/device flows. The [authentication guide](https://learn.chatgpt.com/docs/auth) distinguishes subscription sign-in from separately billed API keys.

Quantix will use the SDK's original-client authentication and account/model metadata. It will not implement its own OAuth client, import another application's token cache, reuse subscription tokens as API keys, or treat a saved login as proof of selected-model execution. Account/workspace eligibility and provider limits still apply; the SDK's one turn is not a documented hard internal inference/token cap.
