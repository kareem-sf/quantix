# AI provider support — 9 September 2026

Quantix offers five provider groups. An API key and a subscription are separate account methods; neither silently converts into the other.

| Provider | API key | Subscription inside Quantix | Basis |
| --- | --- | --- | --- |
| OpenAI | Responses / Chat Completions | Eligible ChatGPT access through original Codex | [Official SDK](https://learn.chatgpt.com/docs/codex-sdk), [app-server authentication](https://learn.chatgpt.com/docs/app-server) |
| xAI | OpenAI-compatible API | Eligible Grok access through original Grok Build | [Official integration announcement](https://x.ai/news/grok-build-cli), [ACP/headless guide](https://docs.x.ai/build/cli/headless-scripting) |
| Anthropic | Native Messages API | Not offered for Quantix's own sign-in and Tender interface | [Current hosting and credential rules](https://code.claude.com/docs/en/legal-and-compliance) |
| Google | Native Gemini API | Not offered for personal subscriptions here | [Gemini CLI transition](https://developers.googleblog.com/an-important-update-transitioning-gemini-cli-to-antigravity-cli/), [Antigravity terms](https://antigravity.google/terms) |
| Custom / BYOK | Explicit OpenAI-compatible endpoint and key | No generic subscription-token import | Use the endpoint owner's documented API. |

Anthropic conditionally permits hosting unmodified Claude Code with end-user authentication. That is different from permission for a product's own subscription sign-in/Agent SDK integration; this is not a claim that every third-party Claude use is prohibited. Google's current transition announcement conflicts with older Gemini CLI subscription pages, while the successor's consumer terms restrict third-party access. The [detailed dated research](reports/2026-09-09-subscription-options.md) records those distinctions and sources.

Direct APIs use bundled SDKs. Supported subscriptions use privately prepared, original vendor clients that own their authentication and token refresh. Source tools, current model readiness, Tender data permission, cancellation and spending authority remain enforced in both paths. Other historical connection profiles remain inspectable and removable but cannot run.

A configured key or successful sign-in does not establish that every model is eligible. Complete the actual model check in Quantix. No unlimited usage, automatic API fallback or zero charge from missing telemetry is promised. See [the setup guide](ai-setup-guide.md), [the implementation contract](subscription-connections.md) and [progress](progress.md) for executed verification and pending acceptance.

## Agentic capability bounds — 13 September 2026

Reusable professional definitions remain independent of the selected provider. Quantix pins effective instructions, settings, skills, tools, source scope and execution identity for each run. The capability view distinguishes provider/client features from shared Quantix tools and reports unsupported combinations before their execution.

OpenAI Responses native calls use the lower approved search/native total; hosted code also requires exact upload and spending scope. Anthropic supports individually bounded native search or page fetching, but the combined native search/fetch configuration is refused where a shared total cannot be enforced. Google URL Context ignores the portable tool's domain/content/call restrictions, and Google/xAI native search lacks an established individual-call bound for the reviewed request. Those combinations remain unavailable; no setting is silently discarded and no account/billing route changes automatically. Shared Quantix URL reading and an explicitly approved bounded research route remain separate alternatives. See [native execution evidence](reports/2026-09-13-native-execution-evidence.md) and [the final acceptance record](reports/2026-09-13-full-agentic-office-acceptance.md).

Original-client host shell/file/subagent powers remain individually identified and restricted until their execution boundary and resource accounting can be verified. Shared Quantix tools, approved delegation and isolated code supply their supported counterparts. Local Monty is available; full Python, local stdio and rendered-browser execution require the qualified private runtime. This computer still requires WSL2 setup. Live model/client and native-platform acceptance remain open.
