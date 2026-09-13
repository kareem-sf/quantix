# Dynamic Staff Creation and Live Office Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Initialize only an engineer-customizable Tender Manager, let it generate complete task-specific staff profiles live, execute isolated assignments and show actual work and communication in an animated shared-office view.

**Architecture:** Extend the existing provider execution and controlled publication paths with Manager profiles, generated staff, capability-bound assignments and durable office events. Staff names, roles, titles and personalities are generated data, never seeded defaults or routing keys. All staff retain the parent work authority and aggregate budget.

**Tech Stack:** Existing Python 3.12/FastAPI/Pydantic/SQLite/provider adapters, React/TypeScript/React Query and Tauri. Use accessible DOM, CSS/Web Animations and existing tests before adding a motion dependency. No new orchestration framework or model runtime.

**Spec:** [Accepted scope](<D:/AI Work/quantix/docs/design/adaptive-office-scope.md>), [live office design](<D:/AI Work/quantix/docs/design/live-dynamic-office.md>) and [master plan](<D:/AI Work/quantix/docs/superpowers/plans/2026-09-09-adaptive-office-master.md>).

## Global Constraints

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

Only the Manager is initialized. No staff_presets module, production specialist-name constants, role/title enum, title-to-route table or fallback employee template is permitted. Test names are fixture data only. Exclude Ollama, BIM and the external integration expansion from the accepted scope.

## Baseline and scope

Capture the starting status, owned diff and hashes before editing this actively changing workspace. Read current source and inspect concurrent changes before integration. Never restore an old baseline over another task or stage the entire repository.

Current [office.py](<D:/AI Work/quantix/backend/quantix/office.py>) constructs one OfficeContext and uses role-based consultation. [office_tools.py](<D:/AI Work/quantix/backend/quantix/office_tools.py>) owns source reads, [ai_execution.py](<D:/AI Work/quantix/backend/quantix/ai_execution.py>) dispatches direct/original-client work, [ai_policy.py](<D:/AI Work/quantix/backend/quantix/ai_policy.py>) owns routes/budgets and [jobs.py](<D:/AI Work/quantix/backend/quantix/jobs.py>) owns run lifecycle and terminal publication.

This first increment includes Manager-only startup, full personality customization, generated staff, minimal actual Manager-mediated exchanges and the live UI. Advanced dependency scheduling, review rooms and broader steering follow through the master plan. They are not prerequisites for seeing real dynamic staff here.

## File ownership

| New file | Responsibility |
|---|---|
| backend/quantix/manager_profile.py | Single Manager initialization, customization and immutable versions. |
| backend/quantix/staff_models.py | Generic professional-profile, assignment, message and snapshot DTOs. |
| backend/quantix/staff_store.py | Tender-scoped generated profiles, assignments and draft results. |
| backend/quantix/staff_generation.py | Manager-facing creation/revision tools and generation provenance. |
| backend/quantix/staff_routing.py | Approved capability/route binding independent of names/roles. |
| backend/quantix/staff_context.py | Separate assignment contexts, scoped notes and evidence receipts. |
| backend/quantix/staff_runtime.py | Real execution through the existing provider/meter boundaries. |
| backend/quantix/office_events.py | Durable events, internal messages, handoffs and cursor snapshots. |
| backend/quantix/staff_routes.py | Authenticated Manager, staff and live-office API. |
| src/features/LiveOffice.tsx | Dynamic shared work area and selection. |
| src/features/OfficeConversation.tsx | Actual staff exchanges and artifact links. |
| src/features/StaffDesk.tsx | Complete generated profile and attributable work. |
| src/features/ManagerPersonality.tsx | Freeform/structured Manager customization. |
| src/features/useOfficeEvents.ts | Authenticated feed and snapshot reconciliation. |
| src/styles/office.css | Layout, meaningful transitions and reduced motion. |
| src-tauri/src/tray.rs | Visible tray, reopen/current-work actions and explicit Quit. |

Modify relevant sections of office.py, office_tools.py, office_types.py, ai_policy.py, plan_review.py/plan_review_models.py, jobs.py, api.py, Manager.tsx, Work.tsx, Settings.tsx, api.ts and navigation/routes.ts. Add focused backend tests named below and adjacent frontend tests. Update contracts and generated declarations with public schema changes. Preserve provider authentication and current source/publication controls.

## Shared contracts

Define these types in staff_models.py and use them consistently:

| Type | Fields and ownership |
|---|---|
| Personality | description, traits, communication_style, problem_solving_style, collaboration_style, uncertainty_handling, initiative, explanation_style, language_preferences and working_habits; free text/lists. |
| ManagerProfile | Server id/version; engineer-editable display_name, title, persona, personality and working_preferences. |
| ManagerProfileEdit | expected_version plus all editable Manager fields; no account/grant fields. |
| StaffProfileDraft | Manager-generated display_name, role, title, specialisms, persona, personality, responsibilities, objectives, methods, deliverables, success_criteria, context_needs, requested_skill_ids, requested_tool_ids and creation_reason. |
| StaffWorkOrder | brief, goal, source_ids, expected_outputs and completion_checks, generated for the actual request. |
| ManagerCreationContext | Server-only tender_id, run_id, manager_profile_version and scope_id. Never taken from model arguments. |
| StaffProfile | Draft fields plus server-owned id, tender_id, version, creator/run lineage, created_at and lifecycle. |
| StaffAssignment | id, staff_id, staff_version, tender_id, root_run_id, work_order, route_binding_id, status and result_id. |
| StaffResult | id, assignment_id, office_output: OfficeOutput, authored_notes, source_ids_read, artifact_refs, created_at and currentness. All engineering output remains draft. |
| OfficeMessage | Server id/time plus authenticated sender_id, recipient_ids, assignment_id, kind, text, exact artifact_refs and reply_to. |
| OfficeEvent | event_id, sequence, tender_id, event_type, actor_id, assignment_id, record_ref and occurred_at. |
| OfficeSnapshot | ManagerProfile, generated staff, assignments, recent messages, cursor and recovery state. |

Input DTOs use extra="forbid". Bound names to 120 characters, roles/titles to 200, persona/personality sections to 2,000 each, professional lists to 20 entries of 500 characters, sources to 50 and briefs to 4,000. Bounds control resources; they do not define a permitted vocabulary. IDs, timestamps, grants and creator identity are assigned by the service.

ManagerProfileService(repo) exposes get() -> ManagerProfile and update(ManagerProfileEdit) -> ManagerProfile. Initialization creates exactly one Manager and no specialist.

StaffStore(repo) exposes list_staff(tender_id), get_staff(tender_id, staff_id, version=None), create_generated(context, profile, work_order, idempotency_key), create_assignment(context, staff_id, work_order, route_binding_id, idempotency_key), save_result(tender_id, assignment_id, result), detail(tender_id, staff_id) and recover_interrupted(). create_generated returns the generated StaffProfile and planned work-order identity; it neither starts execution nor grants capabilities.

Identical request/tool-call replay returns the same receipt; changed replay conflicts. Staff profiles belong to one Tender. A new Tender starts with no specialists even when another Tender has generated staff.

## Task 1: Manager-only startup and complete personality editing

**Files:** manager_profile.py, staff_models.py, staff_store.py, api.py and backend/tests/test_manager_profile.py.

**Produces:** single Manager/version store and an empty specialist store, separate from Tender context and provider authority.

- [ ] Add and run this initially failing persistence regression:

```python
from quantix.repository import Repository
from quantix.manager_profile import ManagerProfileService
from quantix.staff_store import StaffStore

def test_only_manager_exists_by_default(tmp_path):
    repo = Repository(tmp_path)
    manager = ManagerProfileService(repo).get()
    a = repo.create_tender("Synthetic office A")["id"]
    b = repo.create_tender("Synthetic office B")["id"]
    assert manager.display_name == "Tender Manager"
    assert StaffStore(repo).list_staff(a) == []
    assert StaffStore(repo).list_staff(b) == []
    assert ManagerProfileService(Repository(tmp_path)).get().id == manager.id
```

- [ ] Implement a neutral default Manager, all editable profile fields, immutable versions and optimistic update conflicts. Never reseed over user preferences or generate specialists during GET.
- [ ] Test arbitrary custom persona, name, language, initiative and communication styles across restart and across two Tenders. Assert Tender histories stay separate and no account/grant changes occur.
- [ ] Snapshot the Manager version for execution. Existing work retains it until a safe-boundary update is recorded; new work uses the saved version.
- [ ] Run backend/.venv/Scripts/python.exe -m pytest backend/tests/test_manager_profile.py -q and inspect the owned changes.

## Task 2: Generate complete specialist profiles through the Manager

**Files:** staff_generation.py, staff_store.py, staff_models.py, office.py and backend/tests/test_dynamic_staff.py.

**Produces:** Manager tools create_staff(profile: StaffProfileDraft, work_order: StaffWorkOrder) and revise_staff(staff_id, expected_version, profile). Their wrappers supply server-owned ManagerCreationContext and idempotency identity.

- [ ] Add schema tests with arbitrary invented roles not registered anywhere else. Assert their validity does not depend on name/title/role membership in a list.
- [ ] Test missing responsibilities, personality or success criteria produce specific validation errors and no partial employee. Reject forged creator, grant, ID or timestamp fields.
- [ ] Generate every profile field in the existing approved Manager loop using current task context and available capabilities. Do not fill missing fields from a hidden persona template or call an unapproved model.
- [ ] Save profile, Manager version, creation reason and planned work order atomically; publish staff_created only after commit.
- [ ] Represent unavailable requested capabilities as an explicit blocked requirement. They must not become callable tools. The Manager can revise its method within approved capabilities; application code must not invent a fallback employee.
- [ ] Test identical replay creates one profile, altered replay conflicts, foreign Tender references fail and another Tender remains staff-free.
- [ ] Run test_dynamic_staff.py and affected Manager/schema tests. Source-review production paths for seeded names and role-to-behavior branches.

## Task 3: Bind dynamic profiles to capabilities and approved routes

**Files:** staff_routing.py, ai_policy.py, plan_review.py, plan_review_models.py, backend/tests/test_staff_routing.py and affected plan-review tests.

**Produces:** bind_staff_route(context: ManagerCreationContext, staff_id: str, requested_capabilities: list[str]) -> RouteBinding, containing exact approved route/account/model revisions, tool/source scope, parent budget and delegation constraints.

- [ ] Test two unrelated role names can bind to the same valid capability requirements; a familiar title cannot gain an unapproved operation. Generated names and personas are never authority keys.
- [ ] Extend existing plan review with an explicit dynamic-delegation envelope: purpose, sources, capability set, exact permitted model routes and spending. Existing exact-task approvals do not gain this authority silently.
- [ ] Revalidate at approval and dispatch. Routine profile creation and work inside that envelope require no repeated name/role approval. A broader capability or route requires the existing explicit decision path.
- [ ] Assert unapproved account/tool/destination/budget requests result in zero provider calls and no partial grants.
- [ ] Preserve original-client readiness/account serial constraints, root request limits and uncertain reservations. Never reset the budget by creating a profile or child assignment.
- [ ] Update contracts/bindings and run test_staff_routing.py plus affected plan-review/budget regressions.

## Task 4: Separate working contexts and attributable results

**Files:** staff_context.py, staff_store.py, office_tools.py, office_types.py and backend/tests/test_staff_context.py.

**Produces:** build_staff_context(repo, assignment) -> OfficeContext with its own mutable state, staff/profile version and root run identity.

- [ ] Create two generated profiles/assignments through task 2's service and assert their seen_sources, item_bases and source_recipients are distinct objects.
- [ ] Register a synthetic source through Repository.register_artifact(); read it from one context; verify the other cannot cite it until actual inspection of the source or an attributable passed excerpt.
- [ ] Build context from the generated profile version, work order, scoped authored notes and permitted evidence. Do not inherit another actor's entire history or raw internal reasoning.
- [ ] Enforce Tender/tool scope before returning data from every read/list/document/semantic tool. Profile text and received notes cannot expand authority.
- [ ] Persist draft results, useful authored notes and actual evidence receipts using existing OfficeOutput/source validation. Never approve quantities/rates/plans from this path.
- [ ] Reject late results after cancellation, terminal root state, revoked authority or changed basis. Keep historical results with currentness labels.
- [ ] Run test_staff_context.py and affected source/publication tests.

## Task 5: Run generated staff and real office exchanges

**Files:** staff_runtime.py, office.py, jobs.py, office_events.py and backend/tests/test_staff_runtime.py.

**Produces:** async run_staff_assignment(repo, assignment, validated_route, leased_connection, credentials, before_request, on_response) -> StaffResult through ai_execution.execute_api.

- [ ] Mock only the provider boundary in a test; capture generated profile/context/route and verify the exact root meter callbacks and approved connection are used. Test production code contains no fixed staff resolver.
- [ ] Expose execute_staff(staff_id, work_order_id) to the Manager after route validation. Execution addresses generated IDs; names remain display data.
- [ ] Persist the actual Manager instruction and actual staff response/question as OfficeMessage records. Preserve exact artifact versions and recipients when the Manager relays a handoff.
- [ ] Support minimal Manager-mediated clarification/reply. Infer sender from the active actor context; internal office messaging does not authorize external supplier sending.
- [ ] Return concise findings and artifact/source references to the Manager. Do not union child source-read sets into its context; final material citations still require attributable inspection.
- [ ] Test stop, provider failure, unavailable capability, late output, repeated handoff and exhausted parent budget. Every card must reflect the actual terminal/waiting state.
- [ ] Run test_staff_runtime.py and affected office/SDK/worker tests, distinguishing synthetic responses from live acceptance.

## Task 6: Reliable authenticated snapshot and event feed

**Files:** office_events.py, staff_routes.py, api.py, backend/tests/test_office_events.py and backend/tests/test_staff_api.py.

**Produces:** GET/PATCH /api/manager-profile; GET /api/tenders/{tid}/office; GET /api/tenders/{tid}/staff/{id}; GET /api/tenders/{tid}/office/events?after={cursor}. The feed supports authenticated streaming and a bounded polling recovery response. There is no preset-based public staff creation endpoint.

- [ ] Test authentication, foreign Tender/staff/cursor rejection, and zero model calls or profile creation during read-only inspection.
- [ ] Persist unique event IDs and a per-Tender sequence. Commit state before releasing its event; return a consistent snapshot and cursor.
- [ ] Test no event is lost between snapshot and subscription, duplicated delivery is harmless and unavailable history produces an explicit snapshot-reset response.
- [ ] Use Authorization headers with streaming fetch/polling, never bearer tokens in URLs. Bound batches, messages and buffers.
- [ ] Mark abandoned assignments interrupted on restart without starting workers. Reconnection must not animate old creation events as new live work.
- [ ] Update contracts, run npm run bindings and verify the focused event/API tests before frontend integration.

## Task 7: Live workspace, staff desks and Manager customization UI

**Files:** the five frontend components/hook and office.css listed above, adjacent tests and existing Manager/Work/Settings/API/navigation integration.

**Consumes:** OfficeSnapshot, real OfficeEvent/OfficeMessage records, generated profiles, ManagerProfile and server-derived source links.

- [ ] Start UI tests with a Manager-only snapshot; apply synthetic staff_created/message_posted/artifact_shared events and assert cards, speakers and linked versions reflect the data. Fixture events never appear in production.
- [ ] Test arbitrary profile text, zero/one/many staff, disconnected state, duplicates, waiting, completion and failures. Assert no sample roster or fixed department arrangement is rendered.
- [ ] Build accessible DOM cards and optional SVG relationship cues. Open the live office from current Manager work and the Work subview; preserve drafts and source-return context.
- [ ] Render the complete generated profile and its creation reason, work, notes, capabilities and unmet needs. Use distinct illustrated AI portraits with names/roles, linked to each generated staff identity; validate the rendering/generation path and preserve a stable portrait artifact. A temporary initials fallback must not block execution or become a seeded roster. Distinguish completed staff drafts from accepted engineering records.
- [ ] Default to important exchanges, handoffs and questions, with expandable details and an All exchanges view over full retained messages. Render actual sender, recipients, task, reply and artifact links. Test that filtering does not discard messages. Preserve scroll/focus when the engineer is reading history; show new-message counts instead of forcing scroll.
- [ ] Implement freeform and optional structured Manager personality editing with version conflicts and preserved unsaved input. Custom values are supported; personality has no account/grant fields.
- [ ] Implement bounded staff-arrival, status, message, handoff and completion motion from the design brief. Support System/Reduced/Full preference, prefers-reduced-motion and hidden/offscreen suspension.
- [ ] Apply events monotonically and test reconciliation. The following helper rejects duplicates; the feed also detects gaps and requests a snapshot:

```typescript
export function shouldApplyOfficeEvent(
  lastSequence: number,
  event: { sequence: number },
): boolean {
  return event.sequence > lastSequence;
}
```

- [ ] Run the changed UI tests, npm run bindings and npm run check:ui. Verify source navigation and keyboard behavior before visual acceptance.

## Task 8: Continue work in the desktop tray

**Files:** src-tauri/src/tray.rs, src-tauri/src/main.rs, src-tauri/src/service.rs and src-tauri/Cargo.toml; add focused lifecycle tests and native acceptance cases. Review scripts/start-desktop.mjs and scripts/dev.mjs so the source launcher lifetime is also correct.

**Produces:** initialize_tray(app: &tauri::AppHandle) -> Result<(), String> and restore_main_window(app: &tauri::AppHandle) -> Result<(), String> in tray.rs, plus a deliberate native window-close handler. The tray contains Open Quantix, Current work and Quit. Keep current app/session state in the retained window/service.

- [ ] Add a lifecycle decision test proving window close and explicit Quit are different commands: window close must not invoke service::stop; explicit Quit must use the existing shutdown path. Add a no-tray case that does not hide the only accessible window.
- [ ] Enable the documented Tauri tray feature, create a persistent tray icon/menu and retain ownership for the application's lifetime. Validate the exact installed API against [Tauri's documentation](https://v2.tauri.app/learn/system-tray/).
- [ ] Handle the main window's CloseRequested event by preventing destruction and hiding the window only after tray initialization succeeds. Do not reinterpret all ExitRequested events as hide requests; explicit Quit and OS shutdown retain safe service-stop behavior.
- [ ] Update the existing single-instance reopen path to show a hidden window, unminimize and focus it. Preserve the selected Tender, unsent message, staff context and work state.
- [ ] Keep background work execution independent of renderer animation, polling and visibility. Pause hidden UI animation, not backend tasks. Do not introduce a wake lock or auto-start registration.
- [ ] Ensure original-client workers and the local service remain owned until explicit shutdown, including source-development launch. Handle failed service state truthfully and never stop unrelated processes.
- [ ] Verify Open Quantix restores the existing session, Current work navigates without starting work, and Quit gracefully cancels/stops owned work while preserving uncertain usage and recovery records.
- [ ] Run focused native lifecycle checks and normal development compilation, without release packaging. Visually verify the tray is present while the main window is hidden. If a target desktop cannot support its tray, keep the main window accessible with an actionable message.

## Task 9: Dynamic staffing, motion and restart acceptance

**Files:** integration tests/fixtures, affected backup/recovery tests, docs/progress.md and the increment's acceptance record.

- [ ] Verify one default Manager and zero specialists across two synthetic Tenders. Customize the Manager and verify identity/profile persistence with separate project contexts.
- [ ] Submit an unseen task and a different second task through a scripted provider boundary. Verify the Manager-generated profiles follow the task/capability context, with no assertions tied to production staff names.
- [ ] In the actual development app, inspect dynamic creation, execution, actual exchanges, artifact handoff, source attribution, blocked capability and failed work states.
- [ ] Restart/reconnect during work; verify preserved profile versions/results, truthful interruption and no duplicate staff/messages/publication.
- [ ] Check light/dark, 100%/125%/150% scaling, keyboard operation and reduced motion. Test event bursts and a larger synthetic team for readable, stable interaction.
- [ ] Close the native window during a synthetic task and verify continuation plus a visible tray indicator; reopen the same session, then explicitly Quit and verify safe shutdown. Check unavailable tray and suspend/resume without pretending work continued while asleep.
- [ ] Verify backup/restore preserves profiles/assignments/messages/events without renewing account readiness or spending authority.
- [ ] Run affected backend/UI tests and final frontend typecheck. Record any separately authorized live-provider exercise with synthetic content apart from mocked tests and visual observations.
- [ ] Review all changes and generated types, record exact outcomes and remaining limitations. No release package, real Tender approval or commercial sending for acceptance.

## Completion gate

- [ ] Only the Manager is initialized; no specialist preset/name/role routing exists.
- [ ] The engineer fully customizes Manager personality independently of billing/security authority.
- [ ] The Manager generates complete staff profiles for arbitrary supported tasks and preserves their history.
- [ ] Capabilities and routes are validated independently of persona/title.
- [ ] Assignment contexts, notes and evidence receipts remain separate.
- [ ] The live office displays real staff/messages/handoffs and meaningful motion.
- [ ] Reconnect/restart causes no invented activity, lost confirmed events or duplicate work.
- [ ] Source, scope, cancellation, publication and aggregate spending controls remain effective.
- [ ] Tests, typecheck and actual-app visual results are recorded accurately.
- [ ] Window close preserves work with a visible tray; reopening restores the session and explicit Quit stops safely.
- [ ] Desktop/text-first scope is preserved; tablet access and voice are not silently introduced.
- [ ] Ollama, BIM and excluded external integrations remain absent.

Continue with richer skills, scheduling and engineering capabilities after this gate. The first increment must already be dynamically staffed and visibly live; a fixed starter team is not an acceptable intermediate result.
