# Complete execution monitoring

The accepted design retains observable execution details with the Tender and provides both an expanded chat timeline and a dedicated inspector. Model-provided explanations are labelled thinking summaries; they are distinct from source receipts, validated findings, and engineer approvals.

## Provider basis checked on 13 September 2026

- [OpenAI reasoning](https://developers.openai.com/api/docs/guides/reasoning): explicit summary opt-in; summaries do not expose raw reasoning tokens.
- [Codex App Server](https://learn.chatgpt.com/docs/app-server): item lifecycle, MCP tool arguments/results, summary text and section events.
- [Claude thinking](https://platform.claude.com/docs/en/build-with-claude/thinking): documented summarized display and streaming blocks; preserve existing thinking configuration.
- [Gemini thinking](https://ai.google.dev/gemini-api/docs/generate-content/thinking): include-thoughts enables public thought summaries, separate from signature continuity.
- [xAI reasoning](https://docs.x.ai/developers/model-capabilities/text/reasoning): Grok 4.6 explicitly exposes summarized reasoning through documented event fields. That mapping is not generalized to other models or custom endpoints.

The installed Pydantic AI adapters and official original-client bindings were inspected before changing request settings. Unsupported profiles retain their approved execution settings and report unavailable summary capability. Original-client process/turn content is observable; unreported internal model prompts and requests remain unknown.

## Implementation boundaries

The database is the durable execution record; React Query owns bounded observation windows. Full details are paged separately. Closing observation cannot stop a job. Redaction occurs before both modern payload persistence and retained legacy draft copies. A monitoring failure prevents further operation admission; failed monitoring cannot roll back Stop or recovery revocation.

Verification uses synthetic sources, providers and approvals. Real provider/account acceptance is separate and is not established by synthetic tests. No release package is built.

## Verification results

- Combined affected backend run: **254 passed, 1 skipped**. The skip is the existing Windows symlink-creation test. This covers activity persistence, providers, source tools, jobs/API, staff transactions, original clients and backups.
- Subsequent focused regressions: **37 passed** for activity, provider capture and SSE; **6 passed** for office concurrency. These overlap the combined run and are not added to its total. Explicit provider-tool failed, denied and interrupted outcomes remain failed, blocked and interrupted in the timeline.
- Frontend: **22 passed** across the activity, Manager, Work and modal suites; frontend typecheck and focused Ruff checks passed.
- The actual React app connected to the isolated API completed a composer submission through real job/routing/budget/tool/validation/publication code, substituting only a synthetic Pydantic AI provider. The browser inspected exact tool inputs and source results, searched history, closed/reopened the inspector and reloaded completed messages.
- Chrome browser checks passed in light/dark at 100%, 125%, 150% and 200% effective display scaling (viewport plus device-scale-factor emulation), including a full-width narrow-screen inspector and horizontal-overflow checks. No page errors were reported. These are browser render checks of the real application, not native Windows DPI or production-provider acceptance.

Reproduction helpers: `scripts/verify_activity_server.py` and `scripts/verify-activity-ui.mjs`. The synthetic server uses port 18765; its separate UI uses port 1421. The normal app on port 1420 was left alone. Reports, fixture data and screenshots are retained beneath `~/.quantix/runtime/verification/activity-2026-09-13`; no synthetic credentials or private runtime database are committed.

Review regressions cover failure fencing across rollback, Stop/recovery surviving monitoring failure, retaining a result returned after Stop without publishing it, sanitizing both modern and legacy records, historical filter consistency, valid empty-history resets, model output navigation, native tool visibility, restored-detail cache invalidation, and resuming live observation after browsing older work.
