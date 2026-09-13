# Diagnostics and AI Repair Implementation Plan

> For agentic workers: use superpowers:subagent-driven-development for bounded tasks, with primary-owned integration and review.

**Goal:** Make current connection failures diagnosable and simplify the first settings interaction.

**Architecture:** Standard-library local JSONL diagnostics across service/worker boundaries, a small authenticated renderer sink and request references. Preserve existing domain authority and provider isolation.

**Tech stack:** Python 3.12, FastAPI, React, Tauri 2; no telemetry service or new runtime dependency.

**Spec:** [Diagnostic design](../../diagnostics-design.md).

## Global constraints

- Implementers use gpt-5.6-luna/xhigh; architecture/review use gpt-6-astra/xhigh.
- Preserve supplied files and current dirty work; use non-overlapping file ownership.
- No broad suites, lint, typechecks, general browser QA or release builds. Current request authorizes focused native UI investigation and connection diagnostics.
- Source references for claims; distinguish source review from live observations.
- Update shared contracts and generated frontend declarations together.
- Existing approval covers local implementation and reversible repairs; no routine approval gate or speculative infrastructure.

## Task 1: Service and worker diagnostics

Ownership: new backend/quantix/diagnostics.py and diagnostic models/routes, backend/quantix/{__main__,api,ai_worker_client,ai_setup,ai_components,ai_component_manifest,jobs}.py, backend/ai_worker/quantix_ai_worker/{server,grok}.py and a writer copy supplied by preparation. Do not change Grok command/model/tool policy without primary direction.

- [x] Build a bounded JSONL writer, safe exception structure, rotation, retention and failure status as defined in the design.
- [x] Initialize before service imports/startup, capture lifecycle, handled/unhandled request outcomes, jobs, installer exit/classification, setup transitions and worker operations. Propagate operation references.
- [x] Add authenticated typed status and strict renderer event endpoints; include router in API. Use route templates, never raw paths/query strings.
- [x] Copy canonical writer into managed worker and include it in fingerprint and receipt. Pass explicit log destination and context. Log Grok safe stream terminal/round/exit information.
- [x] Preserve specific already-sanitized setup errors; give unexpected failures an error reference. Keep raw SDK logging disabled.
- [x] Primary source review and targeted diagnostic probes, with checks documented separately.

## Task 2: Renderer and settings

Ownership: src/api.ts, src/main.tsx, new src/diagnostics.ts and src/components/AppErrorBoundary.tsx and src/features/Diagnostics.tsx, src/features/Settings.tsx, src/components/ui.tsx, narrowly needed styles. Consume typed endpoints after Task 1 contract is settled; no backend edits.

- [x] Attach safe renderer failure reporting with recursion/rate protection; retain backend request references.
- [x] Install a simple React recovery boundary.
- [x] Keep AI accounts first; put mail, reusable notes, recovery and technical details into clearly named disclosure sections while keeping state and capability intact.
- [x] Display recording status, actual folder and simple support instructions inside technical details.
- [x] Address only clearly established shared modal focus defects if the audit confirms them.
- [x] Primary inspects diff; native focused observation confirms settings/connection behavior when feasible.

## Task 3: Evidence-driven Grok repair

Ownership: assigned after Task 1 completes; default worker grok.py/grok_common.py/grok_auth.py plus narrowly required control boundary. Do not overlap logging implementer.

- [x] Inspect failed generic check metadata and pinned CLI primary sources.
- [x] Delegate minimal repair for the confirmed cause, preserving subscription-only and approved tools/model limits.
- [x] Prepare through existing managed installer, rerun one generic live check, inspect readiness and logs.
- [x] If external/account state blocks success, report exact observed blocker and next action without repeated blind retries.

## Task 4: Project review and handoff

- [x] Independently audit backend domain, AI architecture and frontend workflows.
- [x] Primary reconciles findings against current code and writes one prioritized project review, roadmap and decision questions.
- [x] Document changed files, diagnostic location, observed check result, remaining limitations and deferred verification in docs/progress.md and README.md.

## Execution record

Preflight: Tasks 1 and 2 share HTTP contract only; Task 1 owns schemas and Task 2 consumes generated declarations. Task 3 shares worker files with Task 1, so runs afterward. Task 4 report files have distinct owners. Each task preserves the no-broad-checks override. Work stays in the running user's dirty workspace because the request is to investigate that exact application; review uses a per-turn baseline, not HEAD alone.

First native Grok check on this turn: failed after authentication/discovery, with generic account/model/work-limits message. No Tender operation or spending preference was changed.

Completed: service/worker diagnostics, Node/native startup logging, renderer/settings simplification and the source-proven Grok repair. Independent source reviews approved the corrected scope. Eighteen focused Grok tests, separate diagnostic probes, API binding generation and normal native development launch are recorded in [acceptance evidence](../../reports/2026-09-09-acceptance.md).

Final live outcome: Grok 4.5 passed its generic access check in four rounds; native UI and saved state show Ready. Logging is active under `C:\Users\kareem\.quantix\logs`. Account/sign-in/spending choices and existing Tender records were preserved. No full Tender execution, broad suite, lint, typecheck or release build was run. Source review findings outside this increment remain in [the prioritized project review](../../project-review-2026-09-09.md).
