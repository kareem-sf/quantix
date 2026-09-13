# Native execution evidence — 13 September 2026

This note records the implementation boundary for work package E and approved provider-hosted code. It does not claim live inference or platform isolation qualification from synthetic tests.

## Inspected interfaces

- The installed Pydantic AI 2.40.0 implementation supplies `CodeExecutionTool`, `UploadedFile`, native tool call/return parts, typed streaming events and `NativeOutput`. The direct runtime retains its normal Pydantic AI loop and OpenAI SDK 3.8.0.
- The pinned Codex Python SDK 0.147.0 exposes `AsyncCodex.thread_start`, `thread_resume` and `AsyncThread.turn`. Agent message and reasoning-summary deltas have documented app-server method identities; these delta classes are not all re-exported from `openai_codex.types`. Native raw reasoning/signatures are not display data. See [Codex SDK](https://learn.chatgpt.com/docs/codex-sdk) and [app-server integration](https://learn.chatgpt.com/docs/app-server).
- Grok Build remains pinned to 1.0.13. The inspected [publisher headless guide at commit 72a61251](https://github.com/xai-org/grok-build/blob/72a61251/crates/codegen/xai-grok-pager/docs/user-guide/14-headless-mode.md) documents exact session resume, `streaming-json` text events, internal `thought` events, model-turn limits, strict tool filtering and `Agent` denial. Only UUID session IDs are resumed; titles are never matched. Internal thought events are not presented as reasoning summaries.
- Codex's inspected turn signature and [configuration reference](https://learn.chatgpt.com/docs/config-file/config-reference) expose no hard output-token or inference-count limit. Existing subscription allowance semantics remain explicit; this implementation does not invent those controls.

## Bound sessions and original-client capabilities

Quantix records each original-client session and turn against the Tender, pinned profile version, exact account revision/model/runtime and source/tool/settings fingerprints. Explicit work Resume can reuse a matching prior native session. A changed scope, model, profile or runtime cannot continue incompatible history. Reused history is labelled historical in the client instruction and does not establish fresh source-read receipts.

Original-client shell, native file mutation, native subagents and unaccounted native research remain disabled. Actual supplied client text, Codex summaries and allowlisted tool activity are projected through private worker control. Actor/assignment identity comes from the host context. Shared Quantix code and research tools retain their own reviewed permissions and readiness requirements; unavailable native powers are not presented as executed work.

## Hosted OpenAI code

The first supported hosted-code destination is the explicit `https://api.openai.com/v1` Responses endpoint. The [exact GPT-6 Astra model page](https://developers.openai.com/api/docs/models/gpt-6-astra) documents Code Interpreter support; other models require independent capability evidence.

[OpenAI's tool reference](https://developers.openai.com/api/docs/guides/tools-code-interpreter) documents auto containers, file IDs, container-file citations and content downloads. The [Responses reference](https://developers.openai.com/api/reference/cli/resources/responses/methods/create) documents `max_tool_calls`. The supported native request supplies a bounded total call limit, a 1 GB container and outbound networking disabled. Pydantic AI's portable code-tool type does not expose network policy; a small [public HTTPX transport wrapper](https://www.python-httpx.org/advanced/transports/) applies the documented OpenAI container fields while preserving the SDK-generated tool schemas.

The [current pricing page](https://developers.openai.com/api/docs/pricing#tools), inspected on 12 September UTC, lists USD 0.03 for a full 1 GB / 20-minute container session and separate eligible minute billing. The application uses a recorded, dated full-session rate no lower than that inspected floor. It does not claim an invoice amount. Each work review separately selects native tools, optional exact originals and a charge allowance. Zero uploaded originals is valid for arithmetic or values already supplied in the instruction.

Inference and hosted-code reservations use the same root `ai_usage` ledger. Reservations remain held throughout the hosted environment; verified completed usage is settled after confirmed cleanup. A missing response, cancellation or unconfirmed cleanup retains uncertainty. The application bounds each hosted execution to 15 minutes and deletes its known owned resources. This is application admission and lifecycle control, not a promise of provider invoice ceilings or remote termination during network failures. Unknown resource outcomes receive durable cleanup receipts and block unsafe automatic retries until an engineer reviews them. Cleanup review does not release monetary reservations.

Only selected current artifact versions/hashes are uploaded, after budget admission. Supplied originals are unchanged. Generated files are private, unreviewed artifacts with their input basis and execution identity; upload, generation and file receipt do not mark any source inspected or reviewed. Provider file references must belong to observed native code calls. There is no arbitrary download URL or host-path execution surface.

## Verification basis

Focused tests use synthetic repository records, original-client protocol fixtures, real local MCP exchanges and the actual Pydantic AI/OpenAI SDK stack over HTTPX MockTransport. Coverage includes explicit upload scope, no-file arithmetic, request limits, private generated artifacts, cleanup uncertainty, source changes, route-setting identity, session compatibility, original-client text boundaries, classification limits and shared research admission. Separate live account, native platform and end-to-end engineering acceptance gates remain open until observed by the primary verification task.
