# Adaptive Tender Office: accepted scope

The engineer accepted the adaptive office direction, named AI staff, skills, plugins, RAG, tool calling and public market research discussed on 9 September 2026. This document consolidates that direction for implementation planning. It is an extension of the [product specification](<D:/AI Work/quantix/docs/spec.md>); existing source, spending, publication and storage safeguards remain authoritative. The [research](<D:/AI Work/quantix/docs/reports/2026-09-09-agentic-tender-office-research.md>) and [office concept](<D:/AI Work/quantix/docs/reports/2026-09-09-adaptive-ai-office-concept.md>) provide rationale and examples.

## Included and excluded scope

**Included:** an adaptive local workspace; one primary Tender Manager; AI staff generated live by the Manager, with complete task-specific profiles and consistent professional personalities; separate assignment contexts; persistent notes and work artifacts; Manager-authorized delegation and communication; skills and optional reusable workflows; tool and plugin infrastructure; scoped MCP; evidence retrieval; public web and market research; Python engineering calculations; document/2D drawing workflows; estimates, procurement preparation, programmes and submissions; checks and recovery.

**Excluded:** Ollama and the external integration expansion formerly in research section 11. This excludes new Microsoft 365, Google Workspace, Autodesk/Procore/Speckle/CostX, live scheduling-system, supplier-system, ERP/accounting, dedicated tender-portal feeds/APIs, EC3 and company-automation connectors. Do not reintroduce these through a plugin or MCP preset.

**Deferred:** BIM/IFC, model-based takeoff and BIM connectors. Keep their previous research as historical reference only.

**Preserved:** the five approved direct model-provider groups and supported original-client ChatGPT/Codex and Grok subscription routes. Existing mail behavior and local import/export remain available. The exclusions remove integration expansion; they do not authorize deletion of existing account records or working application capabilities. Public web research remains included separately from dedicated portal/data-feed connectors. Local embeddings and OCR are not excluded by the Ollama decision.

## Delivery choices confirmed

The first usable version is desktop and text-based. Connected tablet access follows afterward; preserve responsive layouts without claiming remote access. Push-to-talk is a later capability. While the computer remains awake, closing the main window keeps authorized work running in the background with a visible tray indicator. Explicit Quit stops work safely; sleep/crash/restart use truthful recovery. No automatic OS startup, wake lock or cloud service is implied.

## Required outcomes

| ID | Requirement | Observable acceptance |
|---|---|---|
| AO01 | The Tender Manager accepts freeform engineer outcomes without requiring a saved workflow. | An unseen request can produce a new plan and useful artifacts using permitted tools. |
| AO02 | Saved workflows and skills are reusable methods. | Applicable steps are reused; irrelevant steps are omitted; new combinations are possible. |
| AO03 | Only the Tender Manager exists by default; it generates every other staff profile live from the work. | A fresh office has zero specialists. Names, roles, titles, personas, personality, responsibilities, methods, requested capabilities and assignments are model-generated; no production roster, role enum or name-to-behavior mapping exists. |
| AO04 | Every assignment has its own context, notes and evidence receipts. | A colleague cannot inherit another's source-read or review status accidentally. |
| AO05 | The Manager coordinates focused agents, subagents and workers. | Work is assigned by responsibility and dependencies; unused staff do not consume model calls. |
| AO06 | Colleagues exchange exact data and artifacts through Manager-authorized coordination. | Handoffs retain artifact versions, units, authorship, acknowledgement and source identity. |
| AO07 | Plans adapt to findings and engineer instructions. | Work can be stopped, narrowed or revised at a safe boundary without losing completed artifacts. |
| AO08 | Evidence retrieval combines exact, semantic and structured methods. | Answers use applicable sources with exact locators and explicit gaps. |
| AO09 | Source and decision changes identify dependent work. | A changed drawing or accepted decision marks affected calculations and outputs for review. |
| AO10 | Public market research preserves commercial context. | Observations retain source, product, date, location, currency, unit, supply/tax basis and validity. |
| AO11 | Engineering calculations are reproducible. | Units, inputs, formula/method version, rounding and result can be inspected and recalculated. |
| AO12 | Skills/plugins/tools are governed capabilities. | Installation and execution have versioned identity, scopes, checks and rollback; no excluded presets. |
| AO13 | Recovery preserves progress and avoids duplicate effects. | A stopped or interrupted task resumes/reassigns from valid checkpoints without duplicate publication or sending. |
| AO14 | Checking is separate from authorship. | A reviewer can reproduce a result, record disagreement and leave a material decision unresolved. |
| AO15 | Staff expertise spans the included Tender lifecycle. | Document control, scope, quantities, estimating, public research, supplier comparison, planning and submission produce connected outputs. |
| AO16 | The interface stays engineering-friendly. | Balanced Manager conversation beside the current document/result, with expandable live office; one primary next action; optional staff desks/detail; source navigation, Arabic replies, themes and scaling work. |
| AO17 | Proactive work is explicit and bounded. | Approved local watchers react to meaningful changes; availability and missed checks remain visible. |
| AO18 | Approved knowledge improves the office. | A checked method or lesson can be proposed for versioned reuse without copying private Tender knowledge indiscriminately. |
| AO19 | The engineer fully customizes the Tender Manager personality. | Name, persona, tone, language preferences, working habits, initiative, explanation and collaboration style are editable; the same Manager identity serves every project with separate Tender context. |
| AO20 | A live shared-office UI shows the actual dynamic team and collaboration. | Distinct illustrated AI portraits with names/roles; important exchanges, handoffs and questions by default, with all retained conversations expandable. Committed events drive motion, readable states, reduced-motion behavior and reconnect recovery. |
| AO21 | Closing the main window preserves active desktop work with a visible tray indicator. | Reopening returns to the same session; explicit Quit stops owned work safely; unavailable tray, sleep, crashes and restart do not leave invisible or falsely active work. |

## Architecture decisions for planning

Keep Python 3.12, FastAPI/Pydantic, SQLite, React/TypeScript/Vite and Tauri. Continue using the existing provider execution boundary; introduce no new orchestration framework in the first increment. Current documentation features in Pydantic AI Harness are candidates to validate, not assumed installed APIs.

Separate staff identity from task execution. Stable staff profiles are not continuously running models. Each assignment reconstructs its context from the profile, current brief, selected evidence, notes and relevant decisions. Named AI personas must not imply invented human credentials or experience.

Only the Tender Manager is initialized. It has one stable office identity across projects, an engineer-editable full personality profile and separate context/history per Tender. Every specialist is generated by the Manager after it inspects the engineer's request, current work and available capabilities. There are no seeded specialists, staff-name constants, production persona templates or closed role/title lists. Previous named examples are illustrative only and must never become application defaults.

The Manager generates the complete professional staff profile: name, role, title, specialisms, persona, personality, communication/problem-solving/collaboration styles, working habits, responsibilities, objectives, success criteria, deliverables, task brief, context needs and requested skills/tools. The service supplies internal IDs, timestamps, creator/run lineage and validated permission bindings. These system fields are not personality defaults and cannot be minted by model output. Unknown capabilities produce an explicit unmet need, not a fabricated tool or fallback employee.

Generated staff persist as Tender history and may be reused or adapted by the Manager when relevant to another task in that Tender. A new Tender begins with the Manager alone. Previous staff are not automatically copied or activated across projects. Finished staff may become idle or archived; do not discard their evidence and artifacts or silently reset personality between calls. Profile changes create versions.

Manager personalization is separate from source/billing/commercial authority. Edits affect subsequent work or a safe-boundary instruction revision, preserve older execution snapshots and cannot change model accounts or grant new permissions. Provide freeform customization plus focused optional fields; do not restrict personality to a fixed set of presets.

The live office is required in the first increment. Its first version includes dynamic cards, actual Manager/staff exchanges, assignments, artifact links, lifecycle feedback and motion. Advanced review rooms and broader scheduling can follow, but an empty static staff directory cannot satisfy the first increment. See the [live office design brief](<D:/AI Work/quantix/docs/design/live-dynamic-office.md>).

Keep one authoritative Tender domain store and one controlled publication path. Draft staff artifacts, received messages and notes do not grant permission to change accepted records. Independent histories also require scoped data/tool access; history isolation alone is insufficient.

Keep one logical parent work scope for aggregate model/search spending. All descendants share its limits. Per-task records improve attribution but must not reset run allowances or hide usage. Respect existing serial constraints on PDFium and original-client accounts; parallelism is enabled only for independent permitted work.

Use a local coordination service and durable records for messages and handoffs. It routes exact artifacts without invoking the Manager model for every delivery. The Manager handles planning, prioritization, exceptions and material disagreements. Acknowledged, read, checked and accepted are distinct states.

A newly composed task may execute without another approval only when its source scope, allowed operations, data destinations, route, spending and output authority are covered by the existing approved work scope. The Manager cannot broaden that scope itself. Future approval reviews must display the allowed delegation envelope explicitly; historical exact-task approvals are not silently expanded.

The responsiveness increment deliberately evolves the current single pending-instruction contract. Until its API, UI and recovery behavior are implemented together, preserve the existing queue semantics. Do not remove the busy guard early and create concurrent untracked instructions.

No arbitrary generated Python is enabled by the staff increment. Reviewed deterministic tools come first. An isolated generated-code capability is a separate acceptance boundary, with default-denied host/network/credential access and a validated supported-platform implementation.

## Non-negotiable implementation constraints

- The primary agent owns architecture, integration, review, and verification. Inspect all changes.
- Build complete working increments; no compatibility layers, fake production behaviour, or speculative infrastructure.
- All product copy uses plain construction-engineering language. The main contact is the Tender Manager.
- Preserve supplied Tender files. Do not commit customer documents, API keys, private extracted content, or runtime databases.
- Use source references for factual findings. Keep imported, extracted, analysed, and reviewed coverage distinct.
- Read docs/contracts.md before changing a shared interface. Update API schemas and generated frontend types together.
- The normal Quantix application home is `~/.quantix`.
- No `await` is allowed inside authority/publication database transactions, including schema initialization.
- Do not build release packages.
- Use synthetic data for approvals and other acceptance mutations; approval of the real Tender and commercial sending remain the engineer's decisions.

Use affected backend/UI tests, frontend typecheck and real-app visual journeys, including light/dark and scaling, for the implementation. Record synthetic provider tests, live provider acceptance and native UI observations separately. Do not use real Tender approval or actual supplier sending as a test shortcut.

## Completion boundary

The programme is complete when the included lifecycle can handle representative familiar and unfamiliar requests with attributable staff work, valid evidence, checked calculations, useful outputs, responsive control and recovery. A staff directory, prompt-based persona or successful single provider check alone does not satisfy this scope.

The first implementation plan establishes Manager-only initialization, full Manager customization, live generation of complete staff profiles, isolated execution, attributable results and an event-driven shared-office view with actual messages and handoffs. It preserves existing authority through capability-based route binding rather than fixed job titles. Broader scheduling, advanced discussions and remaining lifecycle extensions follow as explicit stages in the master plan.

The [completeness review](<D:/AI Work/quantix/docs/design/office-completeness-review.md>) maps the agreed behavior across 40 product, UI and operational areas and records the remaining implementation defaults.
