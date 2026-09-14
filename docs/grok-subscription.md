# Grok subscription integration — 8 September 2026

Quantix adds official Grok subscription access alongside its existing xAI API-key connection. The engineer chooses the access method explicitly. No existing API account, credential, selected model, approval or usage record is converted automatically.

## Supported route

The adapter hosts official Grok Build 1.0.13. ACP performs browser account sign-in/status, model listing and read-only billing operations; original `grok login --device-auth` offers the alternative device-code flow, with only its anchored public URL/code display exposed. Headless `streaming-json` performs Tender work. xAI explicitly documents hosting its client in other applications, and its OpenCode/Hermes announcements establish subscription use in third-party agents. Actual account entitlement and model availability are determined by the provider, not hard-coded plan names.

Software is shared by OS user and component version. Each account's original-client home is separate under the Quantix data directory. No browser cookies or another application's credential cache are imported. The official client owns its login and refresh. `x.ai/models/list` supplies model choices; authentication, catalog access and a passed per-model check remain distinct.

Grok 1.0.13 opens its browser unconditionally during interactive browser/device authorization; it does not expose a supported no-browser toggle while retaining the ACP URL callback. These login subprocesses now receive the desktop's normal HOME/USERPROFILE/APPDATA/LOCALAPPDATA/XDG profile paths, with GROK_HOME still explicitly private. Ordinary workers, saved-account metadata and Tender work retain their isolated environments. Codex already returns its URL for the normal desktop opener. No browser executable, profile directory argument, OS association or browser preference is rewritten. Browser profile selection follows the user's normal browser settings.

Before starting Grok sign-in, the native interface opens the provider's public landing page through a desktop-owned browser command. This starts the normal browser outside the AI worker's lifetime before the original client tries to open its authorization URL. Windows uses the normal URL association and waits for input-idle when the OS returns a process handle; a successful no-handle/delegated handoff is accepted without claiming the page loaded. The engineer may see one preliminary provider tab. Sign-in does not begin if browser preparation reports failure. Both guided setup and advanced client controls use this path. Client process ownership remains unchanged; browser selection and cookies are not managed by the model worker.

The native packages contain a Brotli-compressed executable. Quantix checks publisher SHA-512 integrity and uses its own bounded native decoder; there is no npm postinstall, Node requirement, PATH mutation or system Python discovery. See [installation details](reports/grok-components.md) for pins, platform coverage, receipts and cancellation.

## Spending decisions

The default is **Use my subscription allowance only**. The worker reads `x.ai/billing` and `x.ai/auto-topup-rule`; the core separately validates the normalized evidence. Safe included-only work needs an active unexhausted usage period, explicitly zero prepaid balance and on-demand cap, and a successfully read disabled top-up rule. Missing fields are unknown, except defaults explicitly defined by xAI's typed contract. The remote `onDemandEnabled` feature flag is not proof that account spending is off.

**Allow provider-managed extras** is optional. It permits the original client to follow the account's existing Grok spending settings for Tender work. Quantix does not buy credits or change those settings. The engineer must approve the exact account revision for each Tender; this route cannot promise an independently enforced Tender monetary cap. Restoring older history retains the spending-review pause.

A paid connection check still needs a defensible maximum charge. No subscription-specific charge ceiling has been established for this adapter, so checks requiring paid extras remain unavailable. A previously passed, unchanged model check can survive the dedicated spending-preference change; the Tender's spending authority must still be renewed. Account-wide credit percentages and provider-reported nominal model costs are not exact Tender charges. Provider cost and completeness observations are saved separately from estimated billable cost.

Billing can change in Grok while work is running. Fresh preflight observes current state; it does not create an immutable provider-side spending lock. Unknown costs are never shown as zero, and failed access never switches to an API key.

## Execution and limitations

The original client receives only Quantix's authenticated Tender MCP server. Its native shell, filesystem, web and subagent tools are disabled, along with inherited hooks, skills and external MCP discovery. The model submits through Quantix's structured proposal tool. Source validation, approvals, budgets and final publication remain in the core.

The adapter pins the exact model, reasoning choice when supported, per-model completion limit, main-turn limit and timeout. Auxiliary model features and automatic retries are disabled. A turn limit is not advertised as a complete count of every potential provider-side operation; partial usage stays explicit. Missing or mismatched actual model identity, malformed output or an unsuccessful terminal event withholds publication.

Grok's per-model configuration is account-wide. A core queue serializes billing preflight and execution, with cancellable OS locks preserving cross-process ownership. Other accounts remain independent. A Manager's consultation on any busy Grok account returns a truthful deferral to a separate work-plan task, preventing reciprocal consultation cycles. Cancellation closes the source bridge and stops only owned client descendants through nested native owners. Windows process ownership does not imply a Windows filesystem sandbox.

Deliberate reauthentication and confirmed sign-out advance the Grok account revision. Even after a new successful model check, earlier Tender grants and extras approvals cannot silently transfer to another signed-in account. Routine original-client token refresh does not change that authority.

Native Grok web/X search is not connected in this increment. Image capability remains unknown unless supported by trustworthy model/interface metadata. The unmodified client can fetch xAI's own authenticated support bundles; disabling their discovery/use does not establish a zero-download guarantee for those client-owned assets. Remote session sharing and unrelated inherited capabilities are disabled where the client supports that configuration.

## Update and later acceptance

Health advertises AI setup revision 5. The current application stays running during development unless the engineer authorizes reopening. Frontend HMR can display the update notice; new backend code takes effect when Quantix is reopened through its launcher. Existing client authorization remains in its private account directory across these updates.

The original implementation deferred checks. The engineer subsequently authorized targeted setup diagnostics and generic live account checks; those observations are recorded in git history (the 9 September connection and permission/round-limit repairs). Broad suites, lint, typechecks and release builds remain deferred. Do not treat the narrower results as full provider acceptance.

Later joint acceptance must still cover fresh installation without developer tools; sign-in, cancellation, expiry and reconnect; missing/expired billing observations; both spending preferences and stale-approval rejection; restored history; parallel tasks on one account; separate accounts; interrupted downloads/work, repair and restart; and unchanged API-key connections.

## Primary sources

- [Official client integration](https://docs.x.ai/build/cli/headless-scripting), [enterprise authentication and policy](https://docs.x.ai/build/enterprise).
- [Grok in OpenCode](https://x.ai/news/grok-opencode), [Grok in Hermes](https://x.ai/news/grok-hermes), [terminal-client eligibility](https://x.ai/news/grok-build-cli).
- [Billing and automatic top-up contract](https://github.com/xai-org/grok-build/blob/main/crates/codegen/xai-grok-shell/src/extensions/billing.rs), [consumer usage and account linking](https://docs.x.ai/grok/faq), [Console API billing](https://docs.x.ai/console/billing).
- [Headless output and tool contract](https://github.com/xai-org/grok-build/blob/main/crates/codegen/xai-grok-pager/docs/user-guide/14-headless-mode.md), [configuration reference](https://github.com/xai-org/grok-build/blob/main/crates/codegen/xai-grok-pager/docs/user-guide/26-config-reference.md).
