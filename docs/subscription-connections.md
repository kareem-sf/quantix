# Subscription connections alongside direct APIs

The engineer's new request supersedes the API-only restriction. Keep the five provider groups and their working direct API paths. Add eligible ChatGPT/Codex and Grok subscriptions through the original vendor clients. API keys, subscription authentication, model lists and billing remain separate account methods. No credential copying or automatic payment-route change.

## Supported choices

- OpenAI: ChatGPT subscription through the official openai-codex Python SDK/app-server and its own managed browser/device sign-in, or existing direct API key.
- xAI: Grok subscription through original Grok Build and its documented ACP/headless integration, or existing direct API key.
- Anthropic: keep direct API. Hosting unmodified Claude Code has a conditional exception, but the current own-app/Agent SDK subscription-login restriction does not establish permission for Quantix's Tender interface. State unavailable here, not that all third-party Claude hosting is prohibited.
- Google: keep direct Gemini API. Official Gemini CLI retirement announcement and remaining older pages conflict; Antigravity terms restrict third-party access. Do not revive the old CLI or repurpose OAuth credentials.
- Custom: explicit API credentials only; no generic subscription-token field.

Official sources: https://learn.chatgpt.com/docs/codex-sdk ; https://learn.chatgpt.com/docs/app-server ; https://x.ai/news/grok-build-cli ; docs/reports/2026-09-09-subscription-options.md.

## Integration contract

Create a shared supported-profile predicate: existing is_direct_profile remains unchanged; is_subscription_profile accepts ONLY matching codex/codex or grok_build/grok_build with auth_type=client_login and billing=subscription; is_supported_profile is their union. Keep direct Runtime status/fingerprint/service separate. Other historical profiles stay retired. Existing codex/Grok accounts become inspectable supported accounts again with their existing identity, settings and model selection. Every readiness proof still requires exact account revision, model and actual installed worker fingerprint; deliberate login/sign-out invalidates prior authority for both clients.

Keep AISetupService as the coordinator; put subscription-only preparation/authentication/catalog/check operations in a focused helper if that keeps responsibilities small. Direct code must keep working. Reuse existing managed components/native process ownership and MCP source/control boundaries for subscriptions only; never install a worker to use an API key. Keep installed version changes, cancellation and final cleanup tied to leases. Worker checks retain unpredictable nonce and validated output. Direct check request/input/output limits remain exactly as before.

Fix reproduced Codex startup: create private CODEX_HOME before starting SDK; root's fresh-account probe otherwise returned TransportClosedError before authentication. Inspect real isolated worker environment rather than guessing account() shape (its GetAccountResponse is correct). Preserve actionable sanitized startup/auth/network errors with log references, always expose explicit sign-in recovery. Auth discovery must not depend on unrelated billing metadata succeeding.

Grok check preview must refresh stale usage metadata through an explicit async preflight before presenting a current result, and revalidate immediately before dispatch. Do not invalidate the preview solely because fetched_at changed. Keep the worker's fresh subscription-only guard and account locks. If allowance cannot be confirmed, expose Refresh usage and official usage settings beside the reason. Do not silently enable paid extras. Preserve existing extras/Tender approvals and report boundaries honestly.

Add defaulted SetupMethod.access_kind and SetupAccount.access_kind ('api_key' or 'subscription'); SetupService.subscription_note and subscription_docs_url (optional) explain unsupported choices. Add SetupCheckPreview.limit_description optional string and subscription_check bool=false. For subscription previews max_requests/max_output_tokens/max_input_tokens may be null: Codex internal inference/token limits are not exposed and cannot be described as a two-request API check. UI must use the subscription description rather than print null numeric caps. Generic subscription checks are bounded by the existing core deadline, bridge tool calls and Grok's actual five rounds; the exact Codex internal inference count remains unknown. Use a subscription-specific check meter or explicit mode so full-context reservations do not trip the direct 16,384-byte check limit. No invented dollar cap or zero charge from missing telemetry.

GET check-preview may await current subscription metadata through a new async wrapper while preserving the direct synchronous preview helper used by focused tests. Expose refresh_usage as a setup action if needed. Login/logout/model actions retain leases and compare-and-swap publication. Status failures keep key/model fields and an explicit next action. Auth is completed by the engineer in the vendor-owned page.

Office chooses direct SDK for direct profiles and the original worker for eligible subscriptions. Keep ready checks, per-Tender approvals, source validation and publication; restore same-account Grok consultation deferral to prevent nested account-lock deadlocks. Never route retired profiles or automatically select a paid API fallback. Close pending account workers on deletion/shutdown.

## Interface

Provider card -> choose 'Use my subscription' or 'API key' where supported -> connect -> choose model -> check. Preparing needed software is a simple progress state, not a complicated four-stage wizard. Preserve the direct key form. Put detailed account/billing/model/software controls in More options. Subscription editor shows one main next action, browser/device sign-in alternatives, cancel, model selection, check and refresh usage recovery. Existing unsupported records remain in Older connections. Current supported subscriptions appear in normal accounts and Tender selection.

Health revision becomes 7; backend health, launcher reuse threshold and frontend update banner all agree. Regenerate frontend declarations. All application-managed data stays beneath ~/.quantix, with existing OS-protected credential exception.

## Work and verification

- Runtime implementer owns worker codex.py/accounts.py/grok_auth.py/common.py/runtime_catalog.py and focused runtime diagnostics, plus ai_worker_client.py check limit plumbing if necessary. Preserve pinned dependencies unless a reproduced issue requires a validated upgrade.
- Setup implementer owns ai_setup*, ai_connections, ai_catalog, ai_readiness, ai_execution, ai_runtimes, ai_routes, narrow ai_policy/office dispatch, models.Health and running-workspace threshold. Coordinate limits with runtime owner.
- UI implementer owns AISetup and focused subscription editor, AIAdvancedConnections/TenderAI predicate/copy and styles, App revision banner.
- Primary owns architecture, docs, schema generation, integration and final verification. All implementation uses gpt-5.6-luna/xhigh; independent research/review gpt-6-astra/xhigh.
- Baseline owned files into .superpowers/sdd/2026-09-09-subscriptions/baseline before editing. Do not revert pre-existing dirty work. No broad suites/lint/typechecks/browser QA/release builds. Narrow AI setup/runtime tests, original-client startup/account diagnostics and engineer-authorized live generic checks are allowed. Report local synthetic checks, real client metadata and real inference separately.
