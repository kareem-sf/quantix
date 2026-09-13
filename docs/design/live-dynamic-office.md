# Live dynamic office and Manager customization

This brief implements the engineer's staffing clarification: only the Tender Manager exists by default. It creates the complete staff profile for each required assignment, and the engineer can fully customize the Manager's personality. The office must visibly reflect the staff and work that actually exist. This is an implementation design brief; no live staff feature is claimed implemented here.

## Product behavior

A new installation and a new Tender show the Manager alone. There is no empty grid of suggested employees, pre-created departments or fixed specialist roster. When the engineer submits work, the Manager decides whether it can answer directly or needs staff. It generates the required names, roles, titles, personas, personality, responsibilities, methods, expected outputs and requested capabilities.

The service validates the generated profile and its authority before committing it. A committed staff-created event adds the person to the live office. The UI explains why the Manager created that role and links it to the actual assignment. The number and mix of staff come from the task and approved resource limits, not a layout that expects three or four employees.

Staff identity persists within the Tender after creation. A later assignment can reuse or adapt an appropriate colleague, preserving profile versions and history. Completed colleagues move to idle/history; a new Tender does not automatically import them. The same default Manager identity is available in every Tender, with separately scoped project context.

## Workspace arrangement

**Selected layout:** a balanced workspace with the Manager conversation beside the current document or result, and the live office expandable. The engineer selected this during the UI discussion. When no useful work artifact exists, let the Manager use the space instead of showing empty panels. Preserve the same Tender and selected artifact when expanding or closing the office.

Keep the established app navigation and the Tender Manager as the primary conversation. Add an obvious Open live office action beside current work and an Office subview under Work. Opening it preserves the Manager draft, selected Tender, source context and return location.

The live office has three coordinated views of the same records:

| View | Purpose | Content |
|---|---|---|
| Shared work area | Understand who is doing what and how tasks relate | Manager anchor, dynamic staff cards, actual assignments and selected relationship lines. |
| Office conversation | Read real exchanges and decisions | Manager instructions, staff responses, questions, handoffs and review disagreements, labelled by sender and task. |
| Staff desk | Inspect one colleague's complete profile and work | Generated profile, creation reason, current brief, tools/skills, source receipts, notes, artifacts and history. |

Use ordinary accessible DOM cards/list items with an optional SVG relationship layer. The text/list representation is complete by itself. A heavy 3D office or physics simulation is not required to deliver a lively working experience. Match the existing restrained construction-engineering visual system; this feature does not replace the app's brand or navigation.

On a laptop, show the shared work area and a selected conversation/desk panel with a clear reading order. On narrower layouts, switch between Work area, Conversation and Selected staff, preserving selection. Avoid several competing scroll areas or a permanently expanded technical rail.

Keep the Manager's identity and a return-to-Manager action visible. Opening a staff desk does not accidentally create a second independent engineer instruction channel. If the engineer addresses a staff member, the message is recorded and routed through the Manager's coordination scope.

## Staff cards

**Selected representation:** distinct illustrated AI portraits with names and roles. Maintain one coherent illustration style while giving each live-generated staff identity a recognizable appearance. Portraits are associated with generated staff records; no default specialist cast is bundled or created to populate the UI.

Every card is populated from a committed generated profile and actual assignment state. Show name, job title, AI label, current task, current state and one useful action. Expand to see the full persona, working methods, responsibilities, context needs and permitted capabilities.

Portrait creation/rendering must use a validated local method or an explicitly permitted generation capability within the task's data and spending scope. Staff execution does not wait for a portrait: initials may appear as a temporary loading/failure fallback, with the reason and recovery available. Store a stable portrait artifact per generated identity/version so reopening the workspace does not regenerate it. Do not substitute photorealistic people or a hardcoded employee roster.

State language is concrete: Created for this task, Waiting to start, Reading sources, Calculating, Waiting for a reply, Preparing a draft, Checking, Needs your decision, Completed, Failed, Interrupted or Idle. Use a state only when its execution event establishes it. A generic model call can show Working; it cannot claim Calculating or Checking without that evidence.

Do not generate percentages from elapsed time. Progress counts should come from actual known units of work. A colleague's completed draft is not an engineer-approved quantity, rate or submission.

## Motion design

Motion communicates staffing and collaboration. The focal event is the Manager creating a colleague for an assignment: the new card appears, connects briefly to the originating work and settles into the workspace. Content and controls must be immediately usable; animation never delays execution or review.

| Actual event | Visible response | Motion target |
|---|---|---|
| staff_created | Add the generated staff card and its assignment reason | Short arrival/position transition, approximately 180–260 ms. |
| assignment_started | Update the card's actual state and task | Local state change, approximately 120–180 ms. |
| message_posted | Add a real message to the selected conversation; show sender context | Small message reveal; no invented speech or simulated typing. |
| artifact_shared | Show sender, recipient and the exact artifact link | One brief directional handoff cue, approximately 200–300 ms. |
| review_requested | Indicate the reviewer and the result being checked | Highlight the relationship once; keep a persistent text label. |
| waiting_on | Identify the unmet dependency | Stable waiting state; no endless motion suggesting progress. |
| assignment_completed | Replace work state with result access | Brief acknowledgement followed by a quiet completed state. |
| assignment_failed/interrupted | Show actionable failure/recovery context | Immediate visible state; no celebratory transition. |
| scope_changed | Mark affected work and revised instructions | Explain the change with text and a bounded transition. |

Respect prefers-reduced-motion and provide an app-level motion preference: System, Reduced or Full. Reduced mode removes spatial travel and repeated movement while retaining clear text/state changes. Pause offscreen/hidden-tab animation. Keep sound off by default.

Animate only affected elements, use transform/opacity or measured layout transitions, and batch bursts of events. Cap visual notifications without dropping the underlying events. Keep selected/focused cards stable rather than reordering them on every update. Larger teams can be grouped by the actual task structure, never by hardcoded departments.

## Real conversations and handoffs

**Selected default:** show important exchanges, handoffs and questions, with details expandable. Include material review disagreements, scope changes and required decisions. Keep routine tool events separate from conversation. A visible All exchanges control opens the full retained conversation, with staff/task filters and exact artifact references. Default filtering changes presentation only; it must not discard messages or remove their accessibility.

The Office conversation shows messages that were actually produced or submitted by the participating agents. Tool activity is a separate event category. An OCR completion must not be rewritten into a quotation attributed to a staff member who did not say it.

Every message retains sender, recipient or work group, assignment, created time, source/result references and reply relationship. Clicking a shared calculation opens that exact artifact version. Receiving a handoff does not automatically mark the recipient as having inspected all its sources.

Show new-message counts when the engineer is reading older content. Do not steal scroll position or keyboard focus. Filtering by staff, assignment, questions or artifacts changes presentation, not the stored history. Advanced review rooms can later group the same underlying messages around a concrete issue.

If actual provider output is streamed, mark incomplete content as a draft and render it safely. Never simulate talking bubbles by inventing text or stretching a completed response into fake progress. A historical replay is visibly labelled Replay and never changes records or appears live.

## Manager personality editor

Provide Customize your Manager from the Manager profile and Settings. Offer a freeform description of the desired behavior plus optional editable fields for display name, persona, personality traits, tone, language preferences, explanation depth, initiative, uncertainty handling, decision presentation, problem-solving approach and collaboration style. Custom values are supported; preset chips may be convenience inputs only, never the full allowed vocabulary.

The Manager is the engineer's permanent contact across projects. Global personality changes create a new profile version; old tasks retain the version they ran under. Tender-specific instructions remain scoped to that Tender and do not overwrite global preferences or move private data into another project.

Personality changes govern communication and working approach. They do not grant spending, change the connected model or remove source/commercial authority. Preview, if provided, must be labelled a sample response and must not inspect private Tender data or launch staff work. Running work uses the existing version until a safe-boundary update is applied and recorded.

## Desktop lifetime and delivery scope

The first usable version is desktop and text-based; connected tablet access and push-to-talk follow later. Closing the main window keeps authorized work running while the computer stays awake, with a visible tray indicator. The tray provides Open Quantix, Current work and Quit. Reopening returns to the same Tender, draft, selected artifact and office session.

Explicit Quit stops owned work through the normal safe shutdown path and retains interrupted/uncertain state for recovery. Sleep or a crash cannot be represented as uninterrupted work. The app does not keep the computer awake or start automatically on OS login without a separate setting and authorization. If the tray cannot be initialized, keep the main window accessible and explain the problem rather than leave hidden work with no visible control.

## Transport and recovery

Persist office events with a Tender-scoped monotonic sequence and unique event ID. Fetch a current snapshot and its cursor, then subscribe after that cursor. Use authenticated streaming fetch or a tested polling fallback through the existing loopback service; never place the session bearer token in a URL.

Reconnection requests events after the last confirmed cursor. Deduplicate repeated events. A cursor outside retained history triggers a fresh snapshot, visibly distinguishing unavailable history from zero activity. The client must not animate a replay as new live work.

When the service disconnects, keep the last known view and show its stale/disconnected state. Do not change an old Running state into an assertion of current activity. On restart, use persisted interrupted/recovery status. Inspecting the office or editing animation preferences never starts a model call.

## Acceptance scenarios

1. Fresh root: one Manager and zero specialist profiles, assignments or generated messages.
2. Fresh Tender: Manager alone even when another Tender has generated staff.
3. Novel request: generated names and roles appear without matching any production roster or role enum.
4. Profile completeness: the created colleague has all required professional fields and a task-based creation reason.
5. Different requests: staffing follows the required work, without a fixed count or fixed job-title sequence.
6. Real handoff: sender/recipient and artifact version in the animation match persisted records.
7. Context isolation: a shared message does not fabricate another agent's source-read/review status.
8. Restart/reconnect: no duplicate staff, duplicate messages, replayed creation effects or invented success.
9. Manager editing: arbitrary valid custom personality text persists across projects, while Tender contexts remain separate.
10. Accessibility: keyboard navigation, screen-reader labels, reduced motion, light/dark and 100%/125%/150% scaling preserve all essential information.
11. Large/fast event set: cards remain readable, selected work stays stable and event bursts do not freeze the interface.
12. Paused or failed work: live visual feedback remains truthful and recovery is discoverable.
13. Window close: active work continues with a visible tray indicator; reopening restores the same session; explicit Quit stops safely.
14. No tray, suspend/resume and crash: the app remains accessible or reports recoverable interrupted state, without claiming continuous operation.

The first implementation includes the core work area, conversation, generated profile desk, Manager editor and event-based feedback. Advanced meeting layouts, historical replay and richer visual portraits can follow after that complete behavior is verified.
