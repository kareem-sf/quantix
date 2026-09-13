# AI setup architecture

## Subscription methods alongside direct APIs — 9 September 2026

The engineer subsequently requested subscription choices again. The [subscription contract](subscription-connections.md) adds eligible ChatGPT/Codex and Grok original-client methods beside the direct SDK branch. The same provider card lets the engineer choose the access method; account credentials, readiness proofs and billing remain distinct. Other subscription methods stay unavailable with an official-source explanation. See [provider research](reports/2026-09-09-subscription-options.md) and [progress](progress.md) for evidence and acceptance status.

## Current direct API design — 9 September 2026

The engineer approved API-key-only setup for OpenAI, Anthropic, Google, xAI and OpenAI-compatible Custom. The [direct API contract](ai-connection-simplification.md) supersedes the historical client/installer design below. Bundled SDK adapters perform model discovery, the small connection check and Tender execution in the local service. Scoped local source tools, per-request spending reservations, cancellation and validated Office publication remain under Quantix control. Subscription profiles are retired without converting their credentials or authority.

The interface offers provider, key, model and check with advanced details in More options. Known-price checks use the separate setup ledger; unknown-price checks require explicit consent to bounded requests while retaining an unknown cost. Readiness binds the account revision, exact model and direct adapter fingerprint. Health revision 6 distinguishes this contract. See [progress](progress.md) for executed checks and remaining live acceptance.

## Historical isolated provider design (superseded)

Implementation record: 7 September 2026. Source review and dependency/schema generation only; no tests, client installation, account login, inference or release build was run for this increment.

## Responsibilities

The frozen core retains SQLite, original files, source tools, estimates, engineer decisions, cost reservations and publication. Provider packages are no longer mandatory core dependencies. The worker source is bundled as data alongside a reviewed component manifest and a small native process owner; provider SDKs and client binaries are downloaded only when that component is selected.

`AIComponentService` manages immutable per-user installations shared across workspaces. Account/configuration homes remain private per workspace/connection. Complete hash locks are resolved ahead of use; user-side preparation installs binary wheels only and does not resolve arbitrary package versions or build native dependencies. uv and CPython are downloaded from pinned manifests. Where Node/npm is needed, the reviewed Node archive and npm lock are used. Python, registry, PATH and shell configuration belonging to other applications are not changed.

Each candidate environment is created at its final version path. Receipts record hashes; activation switches an atomic pointer after preparation completes. Status reads critical presence/receipt identity, while installation/worker launch perform full integrity checks. Cross-process component leases prevent removal of active versions. Repairs retain old versions until their workers have closed. Shared runtimes and caches remain when one provider's downloaded software is removed.

The native `quantix-ai-host` owns child processes in a Windows Job Object or POSIX process group, sanitizes frozen-process loader state, and terminates descendants when its parent input closes. The core does not change its own process-wide DLL search state. Provider credentials travel through private pipes, never command arguments.

## Execution and authority

The selected worker runs the complete existing Pydantic AI loop or official client adapter. Standard MCP stdio carries initialize, catalog, account and execution requests. A scoped core MCP endpoint exposes only Tender reads/calculations/proposal submission. A separate endpoint/token exposes budget reservation, usage recording and limited progress events to trusted worker code; those operations never appear in the model tool list.

Core publication still validates source use, currentness and engineer-approved scope. The account/model sent to a worker is explicit; reported identity is retained and mismatches are withheld. Automatic local/provider gateway fallback is disabled in the new workflow. Failures pause work for the engineer's decision.

## Setup state and costs

`AISetupService` owns durable account setup records and short-lived background tasks. Its stages distinguish missing software, preparation, credentials/sign-in, discovery, model choice, checking, ready, attention and cancellation. Reopening converts an unfinished setup to an actionable state; it never invents a completed sign-in/check.

Normal setup uses service/access-method cards backed by explicit provider presets. Advanced custom/cloud details use the existing validated connection contract. Models and documented capabilities/pricing are enriched automatically. Suggestions prefer reviewed quality-oriented models available to the account and explain when comparative quality is unknown. Existing selected models are not silently replaced.

Connection checks contain no Tender data and use a nonce tool plus a structured reply. Direct API checks allow at most two model requests; original-client checks allow at most three bounded turns. Paid checks require a fingerprint-bound conservative allowance based on a recorded price. Requests reserve through the core before execution, and setup charges are stored in `ai_setup_checks`, separately from Tender spending. Unknown/unfinished usage remains uncertain; no zero-charge claim is made from missing telemetry.

Ready binds the successful check to the account revision, selected model and installed component version. Account renaming alone does not change its routing revision. Tender activation uses one explicit button decision, a named account/model/destination and one paid allowance, deriving internal safety bounds without exposing token configuration. Restore-history budget safeguards remain in force.

Readiness is stored per model, so checking one model cannot authorize another. A new check supersedes that model's prior proof; confirmed sign-out or a new sign-in clears the account's proofs. Other valid checked models are retained when selecting a new model. Fresh original-client workers can confirm saved sign-in through supported account/metadata operations without opening another login page. Network uncertainty remains Needs attention. Refreshes with unchanged meaningful model information preserve checks; a date refresh alone does not reset authority.

## Interface and deployment

The `/ai/setup` routes expose service cards, accounts, configure/actions and check previews. `/tenders/{id}/ai-setup` creates the simple Tender policy through the existing authority service. Backend Pydantic schemas generate frontend declarations. Existing advanced APIs remain available; historical accounts, keys, plans and usage are preserved. An unmatched AI route on an outdated service gets a restart message; an actually deleted account retains its ordinary not-found message.

Source-development startup builds the native process owner as an application launch step. Release packaging embeds that owner and component/worker assets in the core service. No release artifacts have been built or signed. The engineer's machine never needs development tools once a packaged distribution is prepared; the current source launcher still needs its documented development environment.

The guided API including Grok advertises Health.ai_setup_revision=3. The source launcher reuses a compatible service, gracefully closes an older idle one during a future engineer-triggered launch, or keeps running work open with a clear update notice. Development never restarts the current app to activate new backend code. The Grok increment remains untested.

The [Grok integration record](grok-subscription.md) describes the native component, original-client account boundary, read-only subscription metadata, optional provider-managed extras, revision-bound Tender consent and remaining limitations. Provider-reported model costs are stored separately from estimated billable Tender cost.

## References

- [Detailed component implementation and platform locks](reports/managed-ai-components.md)
- [Provider support and official sources](ai-provider-support.md)
- [uv managed Python](https://docs.astral.sh/uv/concepts/python-versions/), [environment installation](https://docs.astral.sh/uv/pip/environments/)
- [PyInstaller subprocess and loader behavior](https://pyinstaller.org/en/stable/common-issues-and-pitfalls.html)
- [MCP stdio transport](https://modelcontextprotocol.io/specification/2026-07-28/basic/transports/stdio)
