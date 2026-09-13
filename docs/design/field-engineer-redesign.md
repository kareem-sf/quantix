# Quantix: field-engineer UI/UX review and redesign brief

Status: proposed direction for review, not an approved replacement implementation.
Date: 2026-09-09.
Method: dual independent assessments (A: /root/design_assessment; B: /root/ui_evidence). Assessment A completed before detector findings entered synthesis.

## Main recommendation

Make Quantix a guided engineering workspace. Lead with the next meaningful engineering action, open its evidence beside the decision, and keep the Tender Manager available for explanations and flexible requests. Preserve the full Tender Office capability and engineer authority.

This is a workflow redesign. Changing fonts and colors alone would leave the main usability problems intact.

## Confirmed audience and working context

- Primary user: construction field engineer.
- Devices: both laptop and tablet, confirmed by the user.
- Concept approach: interactive layout built directly in code, confirmed by the user.
- Product: an engineer-controlled local Tender Office, as specified in docs/spec.md.
- The present request does not establish a site diary, inspection management, construction progress tracking, or collaboration product.
- Tablet operating systems, connectivity, synchronization and deployment are unresolved. A responsive layout does not establish actual tablet access to the desktop service.

## Evidence and limits

Reviewed the live shell and Settings at http://127.0.0.1:1420, source for the principal features, styles and routes, the product specification, and previous approved Manager/plan-review concept images.

The live app initially showed a missing documents.css import error. Source was changing during review and that stylesheet subsequently existed. The persistent service-unavailable/compatibility state prevented populated Tender journeys. Treat the import failure as a transient development observation, not a confirmed persistent defect.

Live findings below concern the visible shell, recovery and Settings. Populated workflow findings are source-based and are not proof of successfully executed journeys. Earlier concept images are visual references, not current application screenshots. Real AI execution, approvals, mail, exports, dark-mode operation and tablet behavior of the production app were not tested.

## Provisional design health

Scores are reviewer judgements, not measured user performance or a WCAG certification.

| Heuristic | Score / 4 | Main reason |
| --- | ---: | --- |
| System status | 2 | Failure, checking and first-use states compete. |
| Match to engineering work | 3 | Good Tender/BOQ/source vocabulary; infrastructure still intrudes. |
| User control | 3 | Stop, cancellation, drafts and explicit approvals exist in source. |
| Consistency | 2 | Differing overlay, creation and review patterns. |
| Error prevention | 3 | Source checks, approval fingerprints and conflict handling exist. |
| Recognition over recall | 2 | Evidence and decisions are often separated. |
| Efficiency | 2 | Filters and keyboard support exist; repeated row review is costly. |
| Focus and simplicity | 2 | Persistent setup prompts and competing content weaken priority. |
| Error recovery | 2 | Plain errors lack a clear, consistent repair path. |
| Contextual guidance | 2 | Instructions exist but do not consistently guide the next task. |
| **Total** | **23/40** | **Provisional, source-led assessment.** |

## What to preserve

1. Evidence and coverage truth: imported, extracted, analysed and engineer-reviewed mean different things.
2. Engineer control: source-backed proposals, original BOQ quantities, commercial authority, explicit release and sending.
3. Useful accessibility and recovery foundations: focus handling, keyboard behavior, drafts, cancellation and return context.

Evidence: src/features/Files.tsx:269; src/features/PlanReview.tsx:44,94,125; src/components/ui.tsx:103; src/features/Composer.tsx:58.

## Priority findings

### P1 — An unavailable office looks like an empty office

Observed service error, checking/compatibility guidance and first-use onboarding together. The sidebar New tender control is disabled while the central one appears available and its guarded handler does nothing.

Impact: a returning engineer can think work disappeared or that the app ignored a click.

Redesign: mutually exclusive connecting, unavailable, incompatible, ready-empty and ready-populated states. Only show Create your first tender after a successful empty-list response. Give unavailable state one truthful recovery action and preserve the selected Tender context. Do not promise offline records unless a verified local read path exists.

Evidence: src/App.tsx:146,188,203,276,736.
Relevant Impeccable follow-up: harden and onboard.

### P1 — The main screen does not reliably answer what to do next

The same import summary and Finish AI setup action appear for every non-empty package. They compete with conversation, active work, the plan and pending instruction.

Impact: the engineer must interpret application state and choose among software operations before doing engineering work.

Redesign: one state-aware action: add documents, connect an account, review proposed scope, resolve a decision, continue approved work, or review outputs. Readiness prompts appear only when relevant. Completed import information moves into document history.

Evidence: src/features/Manager.tsx:142,144,149,171.
Relevant follow-up: distill and onboard.

### P1 — Tablet support needs a different composition

The desktop sidebar consumes material width around tablet sizes. Source review found document columns needing about 848 px while the 1024 px layout leaves roughly 712 px; the register hides overflow. Estimate tables require 850 px and scroll, without a sticky identifying column in the reviewed styles. Settings summaries measured 20 px high; several other controls measured 34–36 px.

Impact: essential status, row identity or actions can leave the useful view; small targets make touch review harder. The document overflow is a source-backed risk, not a populated-tablet reproduction.

Redesign: respond to the space available to each component. Collapse navigation before it crowds the task. Use explicit labels in stacked document rows, retain item identity in wide tables, and use a touch target design goal of at least 44 × 44 CSS px for important controls.

Evidence: src/styles.css:199,211,661,700; src/styles/documents.css:1,11,117,157; src/styles/estimate.css:58,86,203; src/features/Files.tsx:263.
Relevant follow-up: adapt and layout.

### P1 — Evidence checking breaks the decision context

Estimate editing asks the engineer to verify a row, but a citation opens a separate source modal and makes the earlier decision surface inert. No visible next/previous-row review flow exists in the inspected editor.

Impact: repeated checking depends on memory and repeated navigation.

Redesign: an evidence-and-decision workspace. On laptop, show both together. On tablet, preserve the item identity and working values when switching between source and decision; use a paired view when there is sufficient space. Offer Save and review next only after an explicit item decision, never as implicit batch approval.

Evidence: src/features/EstimateEditor.tsx:27,70; src/features/Sources.tsx:241; src/components/ui.tsx:129.
Relevant follow-up: shape and layout.

### P1 — Plan approval should lead with engineering scope

Below 850 px, the AI sidebar is ordered ahead of scope/tasks. The visible spending statement is vague while details are collapsed. Request changes calls the back handler rather than beginning an amendment with the plan context.

Impact: an engineer sees administration before understanding what they are approving; an action label promises more than its behavior.

Redesign: show the requested work, source coverage, deliverables, important assumptions and effect of approval first. Follow with a compact factual account/model/data/spending summary. A true Request changes action carries the plan identity and requested amendment to the Manager. Detailed controls remain available.

Evidence: src/styles/workspace.css:258; src/features/PlanReview.tsx:120,125,140.
Relevant follow-up: clarify and shape.

## Cognitive load and users at risk

- Returning engineer: needs a clear resumption point; stale import/setup prompts make them reconstruct context.
- Tablet engineer: needs larger targets, readable sources and stable item identity; desktop density makes these harder.
- Laptop estimator: needs efficient review progression; repeated modals and row reopening accumulate work.
- First-time engineer: needs a truthful first task; startup failure must not be presented as missing Tenders.

Five top-level destinations are not inherently excessive. The larger issue is competing tasks within each destination and losing context while following a source.

The emotional low points are startup uncertainty and repeated verification detours. The key trust moment is approval: preserve control, but make the engineering consequences easy to understand.

## Proposed information architecture

| Workspace | Primary question | Intended behavior |
| --- | --- | --- |
| Overview | What needs my attention? | A single prioritized next action, current work, and direct links to relevant records. |
| Documents | Which document and revision do I need? | Search/register, visible revision and coverage, direct source inspection and measurement entry. |
| Review & decisions | What do I need to check or authorize? | Findings, proposals, plan review, evidence beside decisions, decision history. |
| Quantities & costs | What quantity and rate are we using? | BOQ, original/working/proposed values, measurement proposals, rate build-ups and supplier quotations. |
| Submission | What is ready to issue? | Requirements, drafts, programmes, unresolved blockers and reviewed export packages. |

Tender Manager is continuously reachable, with a full conversation available when needed. Flexible requests remain supported. The concept illustrates contextual help with scripted example replies only; it is not connected to AI.

Work plans, tasks, project map, activity, supplier requests/replies and all output types remain reachable through the corresponding workspace. Settings retains AI accounts, spending/data setup, working preferences, reusable notes, mail, backups and technical details. These secondary areas are design scope, not implemented in the prototype.

## Two starting points to compare

### Drawing and decision

A marked drawing is the working surface. The current item, source revision, calculation and decision appear together. The first viewport proves Quantix helps engineering judgement.

Best for: reviewing a known drawing, quantity or finding.
Risk: less useful when opening a Tender with no current review or needing a broad status picture.

### Action-list overview

A practical work list presents the next engineering action with a short reason, followed by other relevant Tender work. The selected task opens the same drawing-and-decision workspace.

Best for: returning to work, onboarding and an interrupted field engineer.
Risk: familiar workflow-software pattern; it must earn its space with accurate state rather than become a dashboard of decorative metrics.

Recommendation: action-list overview for the Tender home, drawing-and-decision for detailed review. Both are represented in the interactive concept. The user has not yet approved this change from the prior chat-first home.

## Visual and interaction direction

Engineering clarity expressed through a drawing-review workspace: readable sans-serif text, strong item identity, warm neutral surfaces, restrained green active controls, tabular quantities and explicit status wording. Keep the Quantix name. This is a proposed direction; it does not replace DESIGN.md or the previous approved design contract.

- Light and dark use the same hierarchy and semantic meanings.
- Use approximately 16 px body text; reserve smaller text for genuinely secondary information.
- Put document title, revision and page beside the evidence.
- Use icon plus label where an icon adds recognition; avoid unexplained icon-only primary navigation.
- Keep controls stable and action labels concrete: Review quantity change, Keep current quantity, Request changes, Review package.
- Reserve animation for state continuity; respect reduced motion.
- Empty, loading, failed, stopped, changed-source and conflict states are first-class designs.
- Do not use a single percentage to imply imported material is checked engineering work.

Accessibility design basis:
- W3C WCAG 2.2 target-size minimum is 24 × 24 CSS px with exceptions; the proposed 44 px touch goal is deliberately larger, not a claim that 44 is the AA minimum: https://www.w3.org/WAI/WCAG22/Understanding/target-size-minimum.html
- Normal text contrast target at least 4.5:1, and 3:1 for qualifying large text: https://www.w3.org/WAI/WCAG22/Understanding/contrast-minimum.html
- Reflow ordinary interface content; accommodate essential two-dimensional drawings/tables without forcing the whole page to scroll sideways: https://www.w3.org/WAI/WCAG22/Understanding/reflow.html

## Concept scope and interaction

The interactive mockup contains synthetic West Yard workshop data and a schematic slab drawing. It includes Overview, Documents, a quantity review, Quantities & costs, and a Submission checklist.

Example: 24 × 18 × 0.20 = 86.4 m³ proposed against an 80.0 m³ original BOQ value. These are illustration values, not customer data or a checked engineering calculation. The example omits deductions and requires source review. A local checkbox and decision action demonstrate the intended approval boundary.

Navigation, document search/filtering, source-to-BOQ navigation, example decision recording, next-action update, scripted Manager guidance and appearance switching are implemented locally in the concept. No source files, Tender records, credentials, services, AI work, mail or exports are mutated.

The prototype is a reviewable slice of the proposed system, not a full redesign implementation or acceptance of the revised product architecture.

Concept verification: inspected the initial laptop view at 1280 × 720, tablet portrait at 768 × 1024, and a 1024 × 768 view. Checked light and dark appearance. Document search for S-101 returned the matching drawing. The example approval remained disabled until the source-check checkbox was selected. Recording the example changed the working value to 86.4 while preserving the original 80.0; returning to Overview then offered the missing-rate task. The reviewed tablet and laptop containers had no horizontal overflow. The concept browser reported no warning/error entries during the inspected run. One browser-control timeout was recovered using the current accessibility state. Actual touch hardware, screen-reader use, all small-width states, production journeys and 125/150% zoom remain unverified. No application tests were run because application code was not changed.

## Delivery increments after direction is settled

1. Shell and recovery: navigation, typography, touch targets, theme, honest service/empty states.
2. Guided home and setup: state-derived next action, contextual Manager and readiness repair.
3. Evidence review: document register/viewer, revision context, quantity review and item progression.
4. Engineering work: plan/decisions, scope map, activity and stop/recovery behavior.
5. Estimate and suppliers: BOQ/working/proposed quantities, rate details, quotations and correspondence.
6. Submission and settings: requirement repair loops, draft review, package readiness and explicit release.

For every affected increment: preserve existing capabilities and data, read docs/contracts.md before shared API changes, update schemas and generated types together, run relevant backend/UI tests and frontend typecheck, then verify real app journeys with synthetic acceptance data. Include light/dark, laptop/tablet widths and 100/125/150% scaling. Do not build release packages.

Tablet delivery is a separate architectural choice. Reusing the interface in a securely connected browser or shipping a tablet package both have implications for file access, credentials, offline behavior and synchronization. None is silently assumed by this visual proposal.

## Detector synthesis

The incumbent src scan produced 15 warnings: 14 side-tab occurrences and one overused-font warning for Inter. These are heuristic prompts, not 15 confirmed usability bugs. Active navigation, citations, warnings and pending-state indicators legitimately need visual distinction. Consolidate the callout system rather than mechanically delete every marker. Inter's popularity does not itself make it inappropriate for engineering software.

Locations: src/styles/ai.css:307; correspondence.css:85; deliverables.css:93,185; estimate.css:2; features.css:52,493,645,875; knowledge.css:123; rate-proposals.css:62; workspace.css:145; src/styles.css:928,1022. Font warning: src/styles.css:5. Paths without src/styles prefix in this sentence are within that directory.

A separate detector pass over the newly authored concept returned no findings. That result is not an accessibility certification or proof of production readiness.

## Decision still to settle

Use the action-list overview as the default Tender home, with the evidence-and-decision workspace for review and the Tender Manager always reachable, or retain conversation as the first screen. Current recommendation is the former, specifically for engineers returning to work on both laptop and tablet.
