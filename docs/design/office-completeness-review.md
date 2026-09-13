# Tender Office scope and experience coverage review

The approved office design now has enough product direction to begin a concrete UI concept and the first implementation increment. The remaining technical work is to validate and implement the recorded behavior, not ask the engineer to choose every library or internal mechanism.

The engineer has confirmed desktop first, text first with push-to-talk afterward, and continued work in the background with a visible tray indicator when the main window closes. Tablet access is a later delivery extension. Closing a window, explicitly quitting, computer sleep and a crash must have distinct, truthful behavior.

This is a coverage review of the agreed scope and plans, not acceptance of implemented software or a guarantee that no issue will emerge during development. Refer to the [accepted scope](<D:/AI Work/quantix/docs/design/adaptive-office-scope.md>), [live office design](<D:/AI Work/quantix/docs/design/live-dynamic-office.md>) and [implementation programme](<D:/AI Work/quantix/docs/superpowers/plans/2026-09-09-adaptive-office-master.md>).

## Coverage matrix

Agreed means the product direction is established. Implementation detail means it can be resolved within that direction without another routine product question. Candidate means it is not automatically part of the first increment.

| ID | Area | Decision and required behavior | Status / delivery |
|---|---|---|---|
| C01 | Product model | One adaptive Tender Office accepts engineering outcomes; no mandatory fixed workflow menu. | Agreed; stages 1–3 |
| C02 | Startup | Only the Tender Manager exists by default; an empty Tender contains no seeded specialists or fake activity. | Agreed; stage 1 |
| C03 | Staff generation | The Manager generates names, roles, titles, personas, personality, responsibilities, objectives and context needs live. | Agreed; stage 1 |
| C04 | Staff identity | Generated staff persist within their Tender and may be reused, revised, idled or archived by the Manager as needed. | Agreed; stages 1/3 |
| C05 | Manager personalization | Full freeform/structured personality customization; stable Manager identity across projects and separate Tender contexts. | Agreed; stage 1 |
| C06 | Novel requests | Discover applicable capabilities, compose a plan, complete supported work and identify specific missing inputs or capabilities. | Agreed; stages 1–3 |
| C07 | Context and memory | Separate assignment histories, notes and source receipts; retrieve only relevant permitted material. | Agreed; stages 1/5 |
| C08 | Shared records | One authoritative Tender store; drafts and private working notes cannot silently change accepted values. | Agreed; all stages |
| C09 | Collaboration | Manager-authorized internal messages and exact artifact handoffs, with attributable sender, recipient and task. | Agreed; stages 1/3 |
| C10 | Independent checking | Checks reproduce sources/calculations; disagreements remain visible; agent agreement is not engineer approval. | Agreed; stages 1/3/6 |
| C11 | Interruptions and replanning | Acknowledge new instructions, preserve work, apply changes at safe boundaries and invalidate affected outputs. | Agreed; stage 4 |
| C12 | Model accounts | Preserve the five approved direct groups and supported original-client subscriptions; no silent model/billing fallback. | Agreed; existing core/all stages |
| C13 | Resource limits | Dynamic staffing within aggregate parent limits; no per-child allowance reset, fabricated usage or automatic paid portrait requirement. | Agreed; stages 1/3/7 |
| C14 | Documents | Preserve originals, paths, hashes and revisions; show unreadable/partial formats; reprocess supported derivatives. | Agreed; existing core/stage 5 |
| C15 | RAG | Exact, semantic and structured retrieval with original locators, correct revision and claim support. | Agreed; stage 5 |
| C16 | Source changes | Trace source/decision dependencies to affected quantities, rates, findings and outputs without deleting accepted history. | Agreed; stage 5 |
| C17 | Quantities | Supplied BOQ remains the default basis; measurements are attributable proposals with units, calibration and method. | Agreed; existing core/stage 6 |
| C18 | Calculations | Reproducible inputs, formulas/methods, units, rounding, independent arithmetic checks and result identity. | Agreed; stage 6 |
| C19 | Market research | Public search with dated, geographically/product-appropriate evidence and explicit commercial terms; no excluded dedicated feeds. | Agreed; stage 6 |
| C20 | Supplier work | Local RFQ preparation and imported quotation comparison; preserve separately authorized existing mail behavior. | Agreed; stage 6 |
| C21 | Outputs and release | Reviewable documents/programmes, client format preservation, requirement coverage, package manifest and separate release/sending authority. | Agreed; stage 6 |
| C22 | Skills | Versioned reusable methods and reference access; instructions cannot grant tools or permissions. | Agreed; stage 2 |
| C23 | Plugins and MCP | Curated capabilities with identity, scopes, compatibility, disable/rollback and no excluded connector presets. | Agreed; stages 2/7 |
| C24 | Generated Python | Reviewed tools first; arbitrary generated code requires a validated isolation boundary and still uses the same authority. | Agreed; stages 6/7 |
| C25 | Opening layout | Manager conversation beside the current document/result, with the live office expandable; no empty filler panels. | Agreed; stage 1 |
| C26 | Staff visuals | Distinct illustrated AI portraits, names and roles; stable assets tied to generated identity, no fixed employee cast. | Agreed; stage 1; renderer validated during implementation |
| C27 | Office conversation | Important exchanges, handoffs and questions by default; full retained exchanges expandable and filterable. | Agreed; stage 1 |
| C28 | Motion/accessibility | Animate actual events; support reduced motion, keyboard, readable labels, themes and scaling; never steal focus. | Agreed; stages 1/8 |
| C29 | Interaction continuity | Preserve drafts, selections, drawing zoom and source-return context across views; manual inspection remains available. | Agreed; stages 1/8 |
| C30 | Language and project settings | Preserve English UI and multilingual Manager behavior, including Arabic; set units, currency, geography and applicable rules per Tender. | Agreed baseline; stages 5/6/8 |
| C31 | Empty/error states | Distinguish no data, unavailable service, missing capability, waiting, failure, interruption and stale records. | Agreed; all stages |
| C32 | Background desktop work | Closing the main window keeps the app/service and authorized work alive with a visible tray indicator; reopening returns to the same session. | Newly confirmed; stage 1 |
| C33 | Quit, sleep and crashes | Explicit Quit stops owned work safely; sleep/crash do not promise continuous execution; recovery preserves uncertainty and checkpoints. | Required lifecycle detail; stages 1/4 |
| C34 | Devices | First usable version is desktop. Preserve responsive design; actual connected tablet access follows separately. | Newly confirmed; later tablet delivery |
| C35 | Voice | First usable version is text. Push-to-talk follows afterward; full realtime voice is not part of the current increment. | Newly confirmed; later capability |
| C36 | Retention and backups | Preserve staff/work/decision history; caches and logs are bounded; archive is not delete; restore must not renew authority or repeat side effects. | Agreed plus implementation detail; stages 1/4/8 |
| C37 | Privacy and provenance | Keep managed data under the application home, credentials in supported stores, Tender permissions enforced and diagnostics sanitized. | Agreed; all stages |
| C38 | Performance and upgrades | Test realistic sources and event bursts; keep selected work stable; validate library/model/skill changes against the benchmark. | Implementation detail; all stages |
| C39 | Quality and delivery | Affected tests, typecheck and real-app visual journeys; separate synthetic/live evidence; no release packages now. | Agreed; every increment |
| C40 | Additional UX ideas | Task-specific surfaces, living brief, return briefing and other new suggestions remain candidate enhancements until deliberately included. | Candidate backlog; not a hidden first-stage expansion |

## Implementation defaults that do not need another product interview

- No repeated approval merely because the Manager invents a new name or role. Authority comes from the approved scope and capabilities.
- A quick question can be answered without creating staff. Team size reflects the work and available resources, not a target number of faces.
- A staff profile does not select credentials or billing by its job title. Missing execution capability remains visible.
- A source-read receipt belongs to the actor that inspected the material. A received message is not proof of whole-document review.
- A changed name, portrait or communication style does not silently rewrite prior messages or change account authority.
- Model/tool activity is recorded separately from authored staff conversation. The live office never invents discussion to look busy.
- Local inspection, expanding the office, reopening a desk or replaying retained history does not start an AI call.
- Portraits remain stable after creation. A failed or pending portrait does not block engineering execution.
- Thread and message filtering affects presentation only; it must not erase retained work history.
- Generated UI content uses validated components and safe rendering; no untrusted scripts or arbitrary application commands.
- Meaningful errors and decisions remain visible even when routine notifications are quiet.
- A late result from stopped/replaced work cannot overwrite a newer accepted state.
- Search observations, quotations, estimates and accepted rates remain distinguishable.
- Monetary totals, VAT treatment and unit conversions require explicit bases; unknowns do not become zero.
- New software capabilities do not automatically revive excluded integrations, add paid routes or gain access to private files.
- Review and export preserve clear internal-versus-client content choices through actual selected outputs.
- No automatic starts on OS login, system service installation, wake-from-sleep behavior or cloud transfer is implied by background-to-tray work.

## Technical findings resolved by documentation review

**Illustrated portraits can use local rendering.** DiceBear documents browser/Node rendering and deterministic seed-based avatars. Its Notionists illustration style is documented under CC0; individual styles have their own licenses. This is a feasible candidate for the prototype and implementation validation, not a locked visual style or installed dependency. A local library would avoid sending staff/Tender data to an avatar HTTP service. The generated identity and appearance data would drive the image; no fixed roster is required. [DiceBear JavaScript](https://www.dicebear.com/integrations/javascript/), [Notionists](https://www.dicebear.com/styles/notionists/)

**Tray behavior has an existing platform path.** Tauri documents system-tray APIs. Quantix's current main.rs handles ExitRequested by stopping the service, and its Cargo configuration does not currently enable the tray feature. The plan must add deliberate close-to-background handling and a persistent tray entry before promising this behavior. Keep explicit Quit and service shutdown separate. [Tauri system tray](https://v2.tauri.app/learn/system-tray/), [current desktop lifecycle](<D:/AI Work/quantix/src-tauri/src/main.rs>), [service lifecycle](<D:/AI Work/quantix/src-tauri/src/service.rs>)

These are documentation/source findings, not evidence that portraits or tray continuation have been implemented or tested in Quantix.

## Coverage gaps corrected in the plan

1. Replaced the previous seeded-team foundation with Manager-only initialization and full dynamic profile generation.
2. Removed name/role-based routing as a source of authority.
3. Added full Manager personality editing to the first increment.
4. Added actual live-office events, conversation and handoffs to the first increment rather than deferring all collaboration visibility.
5. Recorded the chosen balanced layout, illustrated staff and important-exchanges default.
6. Added close-to-background/tray behavior and its native shutdown/reopen acceptance cases.
7. Explicitly deferred connected tablet delivery and voice input from the first usable version.
8. Kept exclusions and candidate ideas visibly separate from implementation commitments.

## What comes next

No further blocking product questions remain for the first increment. A concrete visual concept should show the approved workspace in three connected states: Manager alone; Manager-created staff working and exchanging artifacts; a result with evidence awaiting an engineer decision. Use synthetic data and mark it as a concept. Review the interaction, readability and motion through that journey before treating the UI as visually accepted.

Implementation may still expose real technical constraints. Resolve them against the approved goals and report material tradeoffs; do not reopen settled preferences or add another broad questionnaire without a specific reason.
