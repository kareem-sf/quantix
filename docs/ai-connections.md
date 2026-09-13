# AI connections in Quantix

Implementation checkpoint: 7 September 2026. This increment is implemented but untested at the engineer's request. No provider login, model request, browser check or release build was performed. This describes written integration, not exercised access to every provider/account combination.

## Set up your Tender Office

1. Reopen Quantix and go to **Settings → AI Connections**. Create a named connection for the account and access method you want. You can keep several connections to the same provider.
2. For a model API, enter your own key privately or select the supported cloud identity. For an original client, use **Install client** and its explicit sign-in action. Installation and sign-in are separate from confirmed model access.
3. Select **Discover models**, or add the exact model ID manually. Discovery retrieves metadata; it does not analyse a Tender. Confirm documented tool/image/reasoning capabilities where the provider does not publish them. Enter a dated price card where no suitable catalog estimate exists.
4. Open the Tender's **AI setup**. Choose which connections may receive its context, a Manager model, specialist defaults/role choices and permitted alternatives. Approve request limits and budgets for paid work.
5. Ask the Manager to analyse the package. Review the engineering work plan and AI team together before approving routine execution.
6. Follow work and AI usage. Stopping a request cannot undo provider processing already performed. Missing usage keeps a conservative paid allowance reserved; after work stops, reconcile it against your provider account.

Changing a key, endpoint, account setting or model record changes the connection revision. Review the Tender's data permissions again and refresh/reapprove the AI team as applicable. Updating its AI team does not silently change an approved engineering scope.

After restoring an older workspace, recorded spending can omit later work. Paid AI work pauses until you check provider accounts and explicitly review the entered budgets in Tender AI setup. Ordinary settings changes do not clear that pause. The notice about incomplete history remains after review, because renewed spending authority does not recover missing usage records.

The Tender budget is a limit including recorded spending and reservations. The interface shows the allowance left after deducting them. It is not all new credit; consider missing provider spending before approving that allowance.

## Available connection paths

| Path | Written support | Account and capability limits |
| --- | --- | --- |
| OpenAI | Responses and Chat APIs; your key or named environment key | API billing is separate from ChatGPT; native search requires an eligible route/model. |
| Anthropic | Native Messages API; your key | Tool, image, reasoning and hosted-search availability depend on the model. |
| Google | Gemini API, compatible Chat, Google Cloud/Vertex identity | Explicit cloud project/location or supported Express API-key access. |
| Microsoft | Azure/Foundry OpenAI and Anthropic endpoints | Deployment name and exact endpoint; API key or supported Azure identity. |
| AWS | Bedrock Converse through AWS identity; compatible endpoints through supported bearer access | Explicit region/profile or private AWS credentials. Model/inference-profile IDs and hosting destinations remain visible. |
| Other model APIs | xAI, DeepSeek, Mistral, Kimi, Alibaba Qwen, Z.ai, MiniMax and Cohere presets | Each preset lists documented protocols and product conditions. Protocol compatibility does not establish hosted-tool support. |
| Gateways | OpenRouter and custom endpoints speaking a supported protocol | OpenRouter requires selected upstream providers for Tender work, with upstream fallback off by default. Other gateways use their documented endpoint and key. |
| Local models | Ollama, LM Studio, custom local servers | Actual loaded model/context configuration determines capabilities. A local billing label does not disable a server's own cloud features. |
| Codex | Official Python SDK and original managed ChatGPT sign-in | Subscription-backed execution in this runtime adapter; OpenAI API connections cover metered work. |
| Copilot | Official SDK, original sign-in or supported GitHub token | Uses the account's Copilot entitlement. A GitHub token is not a model-provider BYOK key. |
| Gemini CLI | Original headless client and subscription sign-in | Exact model entered manually. Metered work uses Gemini API/Cloud. |
| Claude Agent SDK | Official SDK, Anthropic API key, bounded turns | Metered background Tender work. Discovery uses Anthropic's Models API. |
| Claude Code | Original attended client sign-in/handoff | Does not execute Quantix background Tender work using subscription credentials. |

Custom connections support OpenAI Chat, OpenAI Responses, Anthropic, Google, Mistral or Cohere protocols. A provider with a different protocol needs another adapter. Quantix does not convert arbitrary subscriptions or extract another app's tokens into a general API.

Native hosted web research is currently an API-route feature. Original-client Tender adapters reject hosted search and expose Quantix's scoped tools. Assign an API research specialist when needed. Mistral Conversations, Kimi-specific search continuations and other provider-specific hosted tools are not implemented by generic Chat access.

## Privacy, authority and cost

Public connection metadata lives in local SQLite. Keys use Windows Credential Manager, macOS Keychain or a supported Linux desktop secret store; session-only storage is available. Secret fields are write-only. Provider keys and private client account homes are excluded from workspace backups. Moving a backup to another machine requires reconnecting accounts.

Tender policies approve data destinations. Plan teams snapshot routes, connection revisions and model metadata. Execution records the requested route and provider-reported model identity where available. Quantix only attempts approved alternatives; gateway routing must also be constrained in the connection. Cloud region labels describe configuration, not a residency guarantee for global/cross-region deployments.

Metered or unknown-billing routes need a recorded price card and positive run/Tender budgets before a model request. Paid image inspection and hosted research also require a recorded context bound. Conservative allowances are reserved first; reported token usage supplies an estimate. Unknown usage remains unresolved until reconciled. Catalog rates retain source/date, may lag current account terms, and are not invoices. Discounts, special products, surcharges and gateway choices can differ. Provider-side spending limits complement Quantix budgets.

Hosted search has a requested call allowance and recorded per-call price. Enforcement varies by provider. Quantix pauses further work against recorded allowances/usage but cannot promise an external provider never exceeds an in-flight estimate. Opaque subscription clients expose less precise inference control than model APIs; Quantix retains work limits and cancellation without pretending every internal call is observable.

API execution uses Pydantic AI with native SDK clients. Original clients use separate adapters and a short-lived authenticated loopback MCP bridge. It exposes source reads/proposals, not unrestricted shell/file-writing tools or engineer approvals. Quantix validates findings and publishes domain changes outside the provider loop. Local semantic indexing remains independent of provider accounts.

## Original-client setup

SDK versions are pinned in `backend/pyproject.toml`. Codex and Claude use publisher-bundled runtimes. Explicit install actions prepare Copilot's runtime/account client or Gemini CLI in the connection's private directory. When required, setup downloads pinned Node from nodejs.org and checks the official SHA-256 manifest, without changing the engineer's system PATH.

Sign-in opens the original account flow only after the engineer selects it. Window exit is not confirmed authentication. Advanced executable, Node and terminal paths support managed installations. Credentials do not appear in user-visible command arguments. OS/processor packages and provider eligibility still need to be exercised on target machines.

## Implementation and remaining acceptance

Implemented: connection CRUD/credentials, discovery/manual metadata, API adapters, original-client adapters/setup, Tender policies/team approval, approved alternatives, usage/reservations/reconciliation, interface controls and generated API declarations. The single-provider loop has been replaced while preserving source validation and engineer-controlled publication.

The release-only service packager and native lifecycle are written for Windows, macOS and Linux. The manual packaging workflow has not been invoked. No installer was built or publisher-signed. See [desktop packaging](desktop-packaging.md).

Deferred acceptance: exercise accounts/SDKs, rendered interface, source-backed results, denied client tools, budget edges, fallback, interruption/restart, backups, revision approval and each OS package. Existing tests need contract updates when testing resumes. No new tests or checks were run for this increment.

## References

- [API implementation and official sources](reports/ai-connections.md)
- [Original-client implementation and official sources](reports/ai-runtimes.md)
- [Pydantic AI providers](https://ai.pydantic.dev/models/overview/)
- [Application contracts](contracts.md)
