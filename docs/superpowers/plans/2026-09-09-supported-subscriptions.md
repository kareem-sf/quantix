# Supported subscriptions implementation plan

> For agentic workers: use subagent-driven-development in this session with bounded ownership and primary integration review.

**Goal:** Add official ChatGPT and Grok subscription connections beside direct APIs, preserving the simple setup and all source/spending protections.
**Architecture:** Keep the direct SDK facade unchanged. Route only exact approved subscription profiles through the retained original-client worker lifecycle; use a focused subscription setup helper for differing authentication, metadata and check limits.
**Stack:** Existing Tauri/React/FastAPI, pinned openai-codex and Grok clients, current native process host and secure account homes.
**Spec:** docs/subscription-connections.md.

## Global constraints

Implementation uses gpt-5.6-luna/xhigh; research/review uses gpt-6-astra/xhigh. Baseline owned files before edits and preserve all prior dirty work. All runtime data stays in ~/.quantix. No broad suites, lint, typechecks, broad browser QA or release builds. Targeted connection diagnostics and live checks are authorized; user completes provider authentication. No credential or billing conversion.

## Task 1: Original-client startup and runtime correctness

Files: backend/ai_worker/quantix_ai_worker/{codex,accounts,grok_auth,common,runtime_catalog}.py; narrow backend/quantix/ai_worker_client.py check plumbing; targeted diagnostic under .superpowers/sdd/2026-09-09-subscriptions.

- [x] Reproduce isolated Codex startup before login, create the private CODEX_HOME and verify original-client account response without a real credential.
- [x] Inspect sanitized worker environment and initialization failures. Keep original managed auth and useful safe errors; never expose provider bodies or credentials.
- [x] Decouple Grok cached authentication from billing queries. Preserve account ownership, cancellation and fresh pre-inference allowance guard.
- [x] Align check limits with the setup contract: API 2 requests/1,024 output/16,384 input unchanged; subscription counts only where actually enforceable. Confirm nonce tool and validated submission remain required.
- [x] Run and retain targeted startup/protocol diagnostics; report exact command and evidence.

## Task 2: Setup, readiness and Office integration

Files: backend/quantix/ai_setup*.py, ai_connections.py, ai_catalog.py, ai_readiness.py, ai_execution.py, ai_runtimes.py, ai_routes.py, narrow ai_policy.py/office.py/models.py, scripts/running-workspace.mjs; focused setup regressions.

- [x] Add exact is_subscription_profile/is_supported_profile predicates and five groups with supported methods; keep unrelated profiles retired and direct paths unchanged.
- [x] Add agreed schema fields and revision 7. Restore subscription component/auth lifecycle in a focused helper; honor account revision leases and invalidate proofs on deliberate account changes.
- [x] Add fresh async subscription preview/check preflight and explicit refresh_usage recovery; exclude refreshed timestamp from authority fingerprint.
- [x] Separate original-client meter limits from direct API limits. Preserve paid-extras blocks and useful next actions; no invented charge cap.
- [x] Integrate supported Office dispatch and restored Grok nested-consult deferral, account deletion and shutdown cleanup.
- [x] Run focused setup tests covering both direct and subscription methods, stale usage recovery, failed sign-in recovery, retired profile rejection and model/version-bound readiness.

## Task 3: Minimal account choice and recovery UI

Files: src/features/AISetup.tsx and a small subscription editor if needed, AIAdvancedConnections.tsx, TenderAI.tsx, src/styles/ai.css, src/App.tsx.

- [x] Offer subscription/API choice only for OpenAI and xAI. Show source-backed plain-language notes for other providers; preserve their API option.
- [x] Keep the direct form and add one-action subscription guidance for preparation, original sign-in, model choice, check, cancellation and refresh usage. Put details in More options.
- [x] Show supported subscriptions in Tender choices; keep unsupported historical profiles read-only. Never infer free usage or use direct numeric check caps for subscription clients.
- [x] Apply revision 7 banner/gating and generated schema integration. Inspect error recovery and nullable preview fields.

## Primary integration and acceptance

- [x] Review all owned-file diffs and independent source-review findings.
- [x] Regenerate bindings and run focused direct/subscription diagnostics. Keep synthetic, original-client metadata and actual inference evidence separate.
- [x] Reopen the idle app through its normal development launcher after verifying no active work; check live rev7 contract and persistent diagnostics.
- [ ] Use Computer for focused setup acceptance where available; leave actual authentication to the engineer. Record any pending live checks honestly in docs/progress.md and provider guide.


Acceptance note: native capture reached the locked Windows desktop, so UI input and fresh ChatGPT sign-in remain pending the engineer. Reopening and real service preparation were completed. Grok reused its saved account and passed the actual four-round generic check; Codex startup/account metadata is working and requests sign-in. See docs/progress.md for the separated verification record.
