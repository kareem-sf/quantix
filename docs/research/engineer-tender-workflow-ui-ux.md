# Engineer tender workflow and Quantix UX direction

Status: discovery research for a product-level UI/UX redesign  
Evidence snapshot: 2026-08-14  
Scope: single-user Quantix v0 for an Engineer User acting as Tendering Manager at an Egyptian main contractor

## Reading this report

This report uses two explicit kinds of statements:

- **Evidence fact** — directly supported by Quantix's accepted domain documents, inspected renderer code, or a cited primary/first-party external source.
- **Design inference** — a proposed consequence for Quantix. It is not presented as user research or an established fact and must be challenged through the upcoming interview and a runnable prototype.

The external work was desk research against standards bodies, government design systems, professional associations, and first-party product documentation. No tender engineer has yet been observed using Quantix in a real bid. That missing field evidence matters: this report can identify a strong design hypothesis, but it cannot validate the Engineer User's actual priorities, vocabulary, tolerance for density, or daily work rhythm.

## Executive conclusion

**Evidence fact:** Quantix has a coherent domain concept: a controlled tender lifecycle, exact evidence and immutable versions, bounded AI work, independent review, and explicit Engineer-in-the-Loop approval. The accepted lifecycle is `Intake -> Fingerprinting -> Initial Compliance -> Bid Decision -> Tender Planning -> Active Production -> Integrated Review -> Package Production -> Final Review -> Ready for Submission` ([ADR 0001](../adr/0001-control-the-tender-lifecycle-with-eitl-gates.md)). Chats and Agent Runs are intentionally not the system of record; material records, reviews, and approvals bind exact versions ([ADR 0002](../adr/0002-keep-chats-outside-the-tender-system-of-record.md)).

**Evidence fact:** The current renderer does not make that lifecycle the primary interaction model. After a Tender is opened, it mounts intake, Agent Runs, the Document Register, source relationships, Decision Cockpit, Change Assessment, Tender Records, Query Register, External RFIs, Bid Decision, Tender Office, BOQ calculation, Basis of Estimate, pricing, coordinated baseline, package production, final review, and retention in one long detail column ([current `TenderWorkspace`](../../src/TenderWorkspace.tsx#L486)). The 13 largest mounted workspace components contain about 12,000 source lines, 141 button elements, and 21 forms. Those counts describe the implementation surface, not the number visible in every state, but they show that the screen is organized around subsystem completeness rather than the Engineer User's immediate job.

**Design inference:** The domain should stay; the interaction concept should change. Quantix should feel like an **exception-led Tender Control Room**, not a catalogue followed by an expanding wall of control panels. Every returning session should answer three questions before showing implementation detail:

1. Where is this Tender in its lifecycle?
2. What needs the Engineer's attention now, and why?
3. What changed or became unsafe since the last review?

The recommended product shell is:

- a small Tender portfolio/home;
- a lifecycle-oriented Tender Overview;
- one prioritized action and decision inbox;
- a shallow, stable navigation structure for focused workbenches;
- a persistent background-run and recovery center;
- exact evidence, audit, IDs, hashes, and technical detail available in context rather than continuously occupying the primary path.

No visual redesign should be implemented directly from this report. The next responsible phase is a stateful product interview, followed by a small runnable prototype of the highest-risk interaction flows. Quantix explicitly skipped its earlier decision-cockpit prototype; the issue was closed without replacement prototype code ([issue #15](https://github.com/kareem-sf/quantix/issues/15)). The present reaction to the implemented UI is evidence that this unresolved design risk has now materialized.

## 1. What the current product is and how an Engineer can use it

### 1.1 The intended operating role

**Evidence fact:** The Engineer User operates Quantix, fills the Tendering Manager role in v0, and is the sole formal approval authority. The Tendering Manager is accountable for plans, commitments, exceptions, and final outputs but is not intended to perform routine analysis, drafting, document control, or production work ([Quantix domain language](../../CONTEXT.md)). AI Agent Profiles may prepare, analyze, calculate, coordinate, review, and recommend, but cannot approve or infer approval ([ADR 0001](../adr/0001-control-the-tender-lifecycle-with-eitl-gates.md)).

**Design inference:** The primary UI should optimize for supervision, exception resolution, and decisions. It should not require the Engineer to understand or manually visit every subsystem in internal execution order. Expert access to records and controls remains necessary, but it is a secondary path.

### 1.2 The current as-built operating path

An Engineer can use the current application end to end as follows:

1. **Establish the local application.** Quantix runs Setup checks, shows Application Home readiness, then presents provider/runtime readiness and update state before enabling new Tender work ([`App`](../../src/App.tsx#L106)).
2. **Create or reopen a Tender Store.** The Tender Catalogue lists local Tenders and opens one into the detail column. Backup, recovery, archive, and trash functions are attached to the catalogue and selected Tender ([`TenderWorkspace`](../../src/TenderWorkspace.tsx#L378)).
3. **Import the Tender Package.** The Engineer selects a directory or ZIP. Quantix copies accepted content into its canonical store, opens the Query Register, and records intake exceptions ([`TenderWorkspace`](../../src/TenderWorkspace.tsx#L544)).
4. **Parse and inspect documents.** The Engineer starts document parsing, inspects exact extracted evidence, confirms addendum/replacement relationships, and invokes Bootstrap Agent Profile work. The Document Register and Tender Records expose the resulting versions and trust states.
5. **Review extracted Tender knowledge.** Proposed requirements, deadlines, risks, project characteristics, assumptions, contradictions, and gaps are checked against exact source evidence. Missing support remains visible rather than being silently completed.
6. **Make the Bid Decision.** The Bid Decision Package and compliance matrix support Proceed, Hold, or Decline. Proceed is required before the full Tender Office can start.
7. **Compose and approve the Tender Office.** Quantix proposes capability demands, Agent Profiles, Workstreams, tasks, permissions, budgets, and independent reviews. The Engineer revises and approves an exact Work Plan, then separately activates production ([ADR 0005](../adr/0005-compose-tender-teams-through-controlled-capability-demands.md)).
8. **Supervise production and queries.** The Engineer monitors Agent Runs and production tasks, addresses access or capability blocks, handles independent findings, controls Tender Queries and External RFIs, and reacts to addenda through Change Assessment.
9. **Build and approve the price.** BOQ calculations remain deterministic; the Engineer controls scenarios, the Basis of Estimate, the Priced Cost Baseline, commercial adjustments, and Final Price.
10. **Integrate, package, and review.** Approved Workstreams become a Coordinated Bid Baseline, then a manifest-bound Submission Package. Validation, independent section review, manual exact-hash checks, findings, and release readiness precede Final Approval.
11. **Export, retain, or remove.** Final Approval binds an exact package and permits a verified Release Copy. Quantix does not submit externally. The Engineer may later archive, back up, restore, trash, or purge the Tender under explicit controls.

That is a defensible product workflow. The main UX problem is that the screen does not guide the Engineer through it.

### 1.3 What the current UI gets right

**Evidence fact:** The renderer already contains valuable interaction foundations:

- real HTML headings, forms, buttons, lists, status messages, and labelled regions;
- visible focus styling in important workspaces;
- an actual list/detail structure inside the Decision Cockpit;
- exact evidence navigation instead of unsupported prose summaries;
- explicit approval actions and rationale fields;
- immutable version, finding, dependency, and provenance details;
- fail-closed error copy such as “Quantix did not change the Tender”;
- no chat interface pretending to be the canonical workflow.

**Design inference:** These are reusable behaviors, not a reason to retain the current page composition. The redesign should relocate and prioritize them rather than discard them.

### 1.4 Why the current concept feels wrong

| Current behavior | Consequence for the Engineer | Design inference |
| --- | --- | --- |
| Setup, runtime, updates, catalogue, and Tender work are vertically stacked in the same application flow. | Infrastructure readiness competes continuously with the Tender's business goal. | Collapse healthy infrastructure into a compact global status area; expand only warnings or required action. |
| An opened Tender renders nearly every domain subsystem in sequence in one `<aside>`. | The Engineer must scroll, remember the domain order, and infer which panel is relevant. | Use persistent shallow navigation and a lifecycle overview; render one focused workspace at a time. |
| The UI reflects implementation modules: Agent Runs, records, queries, RFIs, Bid Decision, office, BOQ, estimate, pricing, baseline, package, final review. | The product asks the user to orchestrate the architecture rather than accomplish a bid outcome. | Make “next required action,” “blocked work,” and “decision due” primary. Keep module names as destinations, not the default experience. |
| Formal decisions exist both in domain-specific panels and in a buried Decision Cockpit. | The user cannot know which location is authoritative or whether another decision is waiting elsewhere. | Make one decision inbox authoritative; deep-link from it to focused decision pages and underlying domain context. |
| Technical identifiers, hashes, revision facts, and detailed control forms often appear near primary actions. | Strong auditability becomes visual noise and may obscure the decision itself. | Show human-readable exact-version labels first; disclose full IDs, hashes, manifests, and audit details on demand. |
| Disabled or unavailable work remains present throughout the page. | The user sees many controls they cannot yet use and must diagnose prerequisites repeatedly. | Replace premature forms with a task status, prerequisite explanation, and link to the blocking action. |
| Long-running work is represented inside local panels and frequent polling refreshes. | Runs can be lost spatially when the user moves elsewhere, and the whole Tender can feel busy. | Add a persistent, non-modal Run Center with durable state, progress, cancellation semantics, and recovery actions. |

This is not merely a styling problem. New colors, cards, icons, or an accordion around the current panels would preserve the wrong interaction model.

## 2. The real-world job to be done

### 2.1 Primary job statement

**Design inference:**

> When my company receives a complex construction Tender Package under a hard deadline, help me establish exactly what is required, decide whether and how to bid, coordinate specialist work, build a defensible cost and price, resolve gaps and changes, and freeze a compliant submission—without losing evidence, responsibility, or control to AI.

The desired outcome is not “run agents” or “fill every Quantix record.” It is **reach an attributable, evidence-grounded submission decision before the deadline with commercial and technical exposure understood**.

### 2.2 External workflow evidence

**Evidence fact:** APMP's official Winning Business Ecosystem describes a bid/proposal stage that begins with reviewing the request, instructions, evaluation criteria, and deadlines; creating a compliance matrix; planning milestones and responsibilities; developing content; conducting iterative reviews; aligning pricing; and performing final formatting, quality, and compliance checks before submission ([APMP Winning Business Ecosystem](https://apmp.org/Web/Web/About-Us/Winning-Business-Ecosystem.aspx?hkey=78f94982-b424-49fe-b276-f890f0ab744c)). It also places a formal bid/no-bid decision before pursuit.

**Evidence fact:** FIDIC's official Tendering Procedure describes a systematic international construction process including project strategy, tender documents, site visit, Tenderer queries, addenda, submission, evaluation, and award. FIDIC states that the procedure should adapt to project size, complexity, and employer or financier procedures ([FIDIC Tendering Procedure](https://fidic.org/books/fidic-tendering-procedure-2nd-ed-1994)). Quantix is contractor-side and stops before external submission, so FIDIC's employer-side evaluation and award steps are context rather than product scope.

**Evidence fact:** The World Bank's current procurement regulations require written clarification and simultaneous written distribution of clarifications and addenda to recorded bidders. Its standard Works bidding document expects bidders to examine the instructions, forms, terms, and specifications and treats material deviations, reservations, and omissions as questions of responsiveness ([World Bank Procurement Regulations, September 2025](https://thedocs.worldbank.org/en/doc/c84273d1b230aeb2b0b8134de5dc8cd7-0290012025/original/Procurement-Regulations-7th-Edition-Sep-2025.pdf), [World Bank standard Works bidding document](https://documents1.worldbank.org/curated/en/883401552630990554/pdf/135308-WP-PUBLIC-Works.pdf)).

**Evidence fact:** AACE's Basis of Estimate guidance treats the basis as a required estimate-package document whose reader should be able to understand scope, pricing basis, allowances, assumptions, exclusions, risks, opportunities, deviations, and pertinent agreements. It also distinguishes preparation, review, and approval ([AACE 106R-19 sample](https://web.aacei.org/docs/default-source/toc/toc_106r-19.pdf)).

**Evidence fact:** The official Oracle Unifier bidder workflow reduces the external bidder experience to opening the request, optionally requesting clarification, preparing the response, and submitting it, while the requestor uses separate review, invitation, comparison, and award surfaces ([Oracle Request for Bid workflow](https://docs.oracle.com/en/industries/construction-engineering/primavera-unifier/26/udesigner/requestforbidrfb-77699a.html)). It is not a model for Quantix's internal contractor office, but it demonstrates that role- and task-specific surfaces can conceal a much larger process model.

**Design inference:** These sources reinforce four durable layers for Quantix:

1. **Understand the opportunity** — intake, requirements, evidence, compliance, deadlines, risks, queries.
2. **Commit deliberately** — bid/no-bid, plan, capability, scope, strategy, and resource decisions.
3. **Produce and coordinate** — parallel technical, commercial, cost, query, and review work under change control.
4. **Assure and freeze** — integration, compliance, package validation, exact-version approval, and export.

The UI does not need one navigation item for every lifecycle state. It needs a clear lifecycle model plus stable work areas that persist across stages.

### 2.3 The Engineer's repeating loop

**Design inference:** Most sessions after intake should use the same loop:

1. Open a Tender and see deadline, phase, material change, active work, and release health.
2. Take the highest-priority attributable action from an inbox.
3. Inspect a short decision/task summary.
4. Drill into exact evidence, versions, calculations, or review findings only as needed.
5. Decide, correct, return, approve, or explicitly defer.
6. See the downstream consequence and the next safe action.
7. Leave long-running work in the background and return later.

This repeatable loop should be learned once and reused at Bid Decision, Work Plan Approval, access decisions, query treatments, cost baseline approval, Final Price, baseline approval, finding exceptions, Manual Verification, and Final Approval.

## 3. Recommended information architecture

### 3.1 Application level

**Evidence fact:** Microsoft recommends a consistent top-level navigation experience for apps with multiple categories, a shallow hierarchy, and ideally no more than two navigation levels ([Windows NavigationView guidance](https://learn.microsoft.com/en-us/windows/apps/develop/ui/controls/navigationview)). Its list/details pattern is intended for finding, prioritizing, and moving quickly between items and their details in wide desktop windows, with an adaptive stacked presentation when narrower ([Windows list/details pattern](https://learn.microsoft.com/en-us/windows/apps/develop/ui/controls/list-details)). GOV.UK recommends task lists for long transactions involving multiple tasks across sessions, with related actions grouped into tasks and a small, user-tested set of statuses ([Complete multiple tasks](https://design-system.service.gov.uk/patterns/complete-multiple-tasks/)).

**Design inference:** Use three levels, never a page-length accumulation of all levels:

```text
Quantix
├─ Home / Tender portfolio
│  ├─ Needs attention across Tenders
│  ├─ Recent Tenders
│  └─ Runtime and recovery exceptions
└─ Selected Tender
   ├─ Overview
   ├─ Requirements & evidence
   ├─ Queries & changes
   ├─ Work & reviews
   ├─ Cost & pricing
   ├─ Submission
   └─ Records & audit
```

Global utilities:

- **Decisions** — authoritative inbox for all pending EITL decisions.
- **Runs** — persistent queue/history for parsing and Agent Runs, including recovery.
- **Search** — searches canonical records and evidence, not provider chats.
- **Application health** — setup, provider, update, backup, and recovery exceptions; healthy state remains compact.

The seven Tender destinations are work areas, not lifecycle steps. The lifecycle appears as status and context in Overview. Cross-lifecycle work such as Queries and Changes remains easy to access without being mistaken for a phase.

### 3.2 Tender Overview

**Design inference:** The Overview is the default returning-session page and contains four ordered regions:

1. **Tender header** — name, client/opportunity reference if available, approved submission deadline and time zone, current lifecycle phase, current package/addendum basis.
2. **Needs your attention** — at most the highest-value actionable items first: approvals, blocking evidence gaps, stale decisions, overdue queries, indeterminate runs, and expiring deadline risks. Each item states why it matters and what it blocks.
3. **Lifecycle and work health** — a compact lifecycle rail plus phase-specific tasks. Sequential stages and parallel Workstreams are displayed differently. A linear step indicator must not imply that parallel production tasks are sequential; USWDS explicitly distinguishes sequential process lists from non-sequential content ([USWDS process-list guidance](https://designsystem.digital.gov/components/process-list/)).
4. **Active work and changes** — current background runs, material addenda/change assessments, and a short “since your last review” summary.

Use a minimal visible status vocabulary until user research proves more is needed:

- Not started
- Ready
- In progress
- Needs attention
- Blocked
- Complete
- Stale

Domain-specific terminal states such as Declined, Withdrawn, Expired, Recovery Required, and Ready for Submission remain exact and visible. Color is supplementary; the text is authoritative.

### 3.3 Focused workbenches

**Design inference:** Each destination should use a stable list/detail/context pattern:

- **Left or upper list:** filterable requirements, queries, tasks, cost items, package items, or records.
- **Main detail:** the selected item's human-readable state, content, findings, and available action.
- **Context drawer or adjacent evidence pane:** exact source location, provenance chain, version history, dependencies, and audit facts.

This is especially important for evidence work. A reviewer should be able to select a requirement, see its proposed structured value, open the exact Arabic or English source location without losing the record, and then Verify, Correct, Mark unsupported, Raise Query, or create an attributable Assumption. A generic “edit” action is too weak because these choices have different trust and workflow consequences.

### 3.4 Progressive disclosure rules

**Evidence fact:** GOV.UK warns that hiding a large application inside accordions is often inferior to simplifying content, splitting it across pages, or providing navigation; nested accordions should be avoided ([GOV.UK accordion guidance](https://design-system.service.gov.uk/components/accordion/)). USWDS recommends a summary box for only three to five essential facts from a longer page ([USWDS summary box](https://designsystem.digital.gov/components/summary-box/)).

**Design inference:** Apply three disclosure layers:

1. **Act:** the task or decision, its urgency, blockers, consequence, and one primary action.
2. **Understand:** the relevant evidence, change, calculation, dependencies, findings, and alternatives.
3. **Audit:** full IDs, hashes, manifests, provider events, permission grants, raw version history, and immutable audit details.

Do not hide information required to make the decision, a non-waivable blocker, the fact that content is stale, or the consequence of an irreversible action. Progressive disclosure is for depth, not for concealing risk.

### 3.5 One authoritative decision experience

**Evidence fact:** GOV.UK recommends a check-answers step immediately before confirmation for small and medium transactions, and section-level review for very large ones ([Check answers](https://design-system.service.gov.uk/patterns/check-answers/)). Quantix approvals bind exact versions and must show evidence, change, rationale, and consequence.

**Design inference:** Elevate the existing Decision Cockpit concept into a first-class destination and use one consistent decision page:

1. **Decision header:** plain-language decision, target object, exact version, owner, deadline, and current state.
2. **Recommended disposition:** explicitly labelled as deterministic policy, AI proposal, independent review, or prior Engineer decision.
3. **What changed:** a material diff from the prior reviewed/approved version, including invalidations.
4. **What blocks this:** missing evidence, open findings, unresolved queries, stale dependencies, or capability limitations.
5. **Decision basis:** three to five decisive facts, then expandable exact evidence and calculations.
6. **Consequences:** what each available action permits, blocks, invalidates, or schedules.
7. **Action:** Approve, Return, Hold, Reject/Decline, or the exact domain vocabulary; rationale and required conditions appear in a persistent action bar.
8. **Receipt:** after submission, show the immutable approval/decision record, version/hash binding, timestamp, and next workflow action.

An approval button must never coexist ambiguously with another approval control elsewhere. Domain workbenches link into this decision page and return to the originating context afterward.

## 4. Traceability, review, and evidence UX

### 4.1 Preserve provenance without making the UI an audit log

**Evidence fact:** NIST's AI RMF says documentation can improve transparency, human review, and accountability; it calls for defined human-oversight roles, documented operating scope and limitations, validation, safe failure, and interpretation of AI output in context ([NIST AI RMF Core](https://airc.nist.gov/airmf-resources/airmf/5-sec-core/)). Quantix already exceeds a generic AI interface by owning exact evidence references, immutable versions, independent review, and EITL decisions.

**Evidence fact:** W3C PROV represents provenance through entities, activities, and agents and includes derivation, attribution, primary-source, revision, generation, and time relations ([PROV-O](https://www.w3.org/TR/prov-o/), [PROV Overview](https://www.w3.org/TR/2013/NOTE-prov-overview-20130430/)). Quantix's Source Artifacts, Artifact Versions, Agent Runs, reviews, decisions, and provenance links already form a domain-specific application of this general model.

**Design inference:** Every material claim should expose a compact provenance sentence:

```text
AI-proposed from Addendum 02, page 14 · independently verified · current
```

The sentence links to:

- the exact source version and location;
- original text and any visibly derived translation;
- extraction/author run and output version;
- independent review result and findings;
- Engineer decision or approved assumption;
- later change/invalidation history.

The first line communicates trust. The provenance drawer proves it. Full digests and opaque identifiers belong in the audit layer unless resolving an exact-version ambiguity.

### 4.2 Trust classes should be visible and actionable

**Design inference:** Use the existing domain distinctions consistently across every workbench:

| Trust state | Meaning in the UI | Typical Engineer action |
| --- | --- | --- |
| Deterministic fact | Computed or validated by Quantix-owned rules | Inspect inputs/rule if surprising |
| AI proposal | Candidate, not truth | Verify, correct, return, or raise a gap |
| Independently verified | Exact version reviewed against evidence | Use subject to currentness and approval gates |
| Engineer-approved assumption | Deliberate decision under uncertainty | Track consequence and invalidate when evidence changes |
| Missing or contradictory | No safe conclusion | Raise/resolve Query, add treatment, or block affected work |
| Stale | Supporting basis changed | Reassess only affected work |

Avoid a generic “AI confidence” badge. Confidence is meaningful only if calibrated for the exact task and shown in a way that changes the user's safe action.

### 4.3 Review is a workflow, not a badge

**Evidence fact:** APMP's Body of Knowledge recommends early review-team formation, reviewers qualified for the review type, standardized review templates, and interim content reviews that identify completeness, gaps, inaccurate assumptions, and improvements ([APMP Bid & Proposal Writing Body of Knowledge](https://www.apmp.org/assets/BoK-BW-M-v2.pdf)). Oracle's first-party workflow model separates creation, review, revision, and terminal approval/rejection, recording each action ([Oracle workflow overview](https://docs.oracle.com/en/industries/construction-engineering/primavera-unifier/26/user-help/aboutworkflows-73172a.html)).

**Design inference:** A review page should show:

- exact target and author;
- reviewer qualification and independence;
- review criteria and scope;
- evidence available at review time;
- findings grouped by blocking consequence, not decorative severity color;
- remediation version and verification status;
- who may dispose each finding and why.

The reviewer should never edit the reviewed target. “Return for revision” creates a clear new author action, and successor versions preserve the negative review rather than visually replacing it.

## 5. Human-AI trust and control

### 5.1 Set the correct mental model before the first run

**Evidence fact:** Microsoft's validated Human-AI Interaction guidelines begin with making clear what the system can do and how well it can do it; they also call for efficient correction, support when the AI is wrong, granular feedback, consequences of actions, global controls, and notification of changes ([Microsoft Human-AI guidelines](https://www.microsoft.com/en-us/haxtoolkit/ai-guidelines/)). Google's PAIR guidance says the goal is calibrated trust rather than complete trust and recommends explaining relevant data sources, limitations, and action-linked causes ([PAIR Explainability and Trust](https://pair.withgoogle.com/guidebook-v2/chapter/explainability-trust/)).

**Design inference:** First-run and first-Tender education should explain, briefly and concretely:

- Quantix registers and protects Tender evidence and workflow state.
- AI Agent Profiles propose and review bounded work; they do not approve or submit.
- deterministic code owns arithmetic and hard workflow checks.
- the Engineer owns all formal decisions.
- source gaps and provider uncertainty can block work.
- Quantix stops at a verified Release Copy; the Engineer submits externally.

Do this through one short guided first Tender and contextual explanations, not a long product manifesto repeated above every screen.

### 5.2 Explain each AI result at the point of use

**Design inference:** An AI-produced record or artifact should answer:

- What task was the Agent Profile asked to perform?
- Which exact inputs and data scopes were available?
- Which source evidence supports this result?
- What could not be established?
- Has a qualified independent reviewer checked this exact version?
- What action can the Engineer safely take now?

Do not expose hidden reasoning. Show attributable inputs, outputs, limitations, events, and review evidence already owned by Quantix.

### 5.3 Correct through domain actions, not chat

**Evidence fact:** Google's guidance for graceful AI failure recommends explaining why a result could not be given and providing alternative paths; its feedback guidance emphasizes user control and clear consequences ([PAIR Errors and Graceful Failure](https://pair.withgoogle.com/guidebook-v2/chapter/errors-failing/), [PAIR Feedback and Control](https://pair.withgoogle.com/guidebook-v2/chapter/feedback-controls/)).

**Design inference:** Corrections should be typed, attributable product actions such as:

- Correct extracted value
- Reject unsupported claim
- Link better evidence
- Raise Tender Query
- Approve Assumption
- Return artifact for remediation
- Start a separate attributable retry

A free-form note may accompany the action, but it does not define the workflow effect. Provider chat remains outside the Tender system of record.

## 6. Long-running work, errors, and recovery

### 6.1 A persistent Run Center

**Evidence fact:** Windows guidance treats progress as feedback for long-running work, recommends determinate progress when duration is knowable, indeterminate progress when it is not, and non-modal progress when users can continue working ([Windows progress controls](https://learn.microsoft.com/en-us/windows/apps/develop/ui/controls/progress-controls)). Older but still relevant Windows desktop guidance recommends allowing long work to continue in the background, providing a halt action, distinguishing Cancel from Stop according to side effects, and not abandoning a task after a recoverable error ([Windows progress-dialog guidance](https://learn.microsoft.com/en-us/windows/win32/uxguide/win-dialog-box)).

**Design inference:** Parsing, extraction, Agent Runs, production tasks, validation, backup, recovery, archive, and export should appear in a global Run Center that survives navigation. Each entry shows:

- action verb and human-readable subject;
- Tender and responsible Agent Profile or Host operation;
- Queued, Running, Needs attention, Completed, Failed, Interrupted, Indeterminate, Stopped, or Cancelled;
- current stage and real progress only when measurable;
- elapsed time and last durable event, not invented time remaining;
- whether the user may continue other work;
- Cancel when rollback returns to the prior state, Stop when partial noncanonical work remains;
- exact output or recovery action when terminal.

The current detail panel may show local progress too, but it is a view of the same durable run—not a separate spinner.

Persist a readable event summary after completion. Microsoft recommends persistent output or logs when users may need detail after a long operation has finished ([Visual Studio notifications and progress guidance](https://learn.microsoft.com/en-us/visualstudio/extensibility/ux-guidelines/notifications-and-progress-for-visual-studio?view=visualstudio)). This does not mean exposing raw provider protocol; it means retaining Quantix's safe normalized events and terminal facts.

### 6.2 Recovery-specific interaction

**Design inference:** Terminal outcomes need different UX:

| Outcome | Required explanation | Safe actions |
| --- | --- | --- |
| Completed | What was registered and what still needs review | Open result; dismiss notification |
| Failed | Cause category, whether canonical state changed, what was preserved | Correct input; start a new attributable run |
| Interrupted | Last known terminal event and whether exact resumption is provable | Resume exact turn if proven; otherwise start a separate run |
| Indeterminate | Why outcome cannot be established and what is quarantined | Inspect evidence; choose explicit recovery; never silent retry |
| Safety/permission blocked | Exact limit or permission boundary and affected task | Reduce work; resolve an eligible access request; no override of a Prohibited Action |
| Recovery Required | Integrity failure and read-only scope | Verify backup; prepare and approve recovery or purge |

Every error should state: **what happened, what Quantix changed, what Quantix did not change, and what the Engineer can safely do next**. Microsoft's error-writing guidance similarly emphasizes the problem, consequence, and realistic action without jargon or blame ([Windows writing style](https://learn.microsoft.com/en-us/windows/apps/design/style/writing-style)). “Try again” alone is not enough for attributable work.

## 7. Accessibility and bilingual operation

### 7.1 Conformance target and keyboard model

**Evidence fact:** W3C recommends WCAG 2.2 as the current conformance target. Relevant requirements include meaningful sequence, visible and unobscured focus, minimum target size, predictable interaction, labels/instructions, error identification, and programmatically announced status messages ([WCAG 2.2](https://www.w3.org/TR/WCAG22/)). W3C notes that focus order must preserve meaning and operation so keyboard users can form a consistent mental model ([Understanding Focus Order](https://www.w3.org/WAI/WCAG22/Understanding/focus-order)).

**Design inference:** Target WCAG 2.2 AA for the Tauri WebView UI and verify it with native assistive technology on every supported platform. Specifically:

- all functions work by keyboard without a pointer;
- focus order matches visible order and returns sensibly after drawers, evidence viewers, and dialogs close;
- sticky headers/action bars never obscure focused controls;
- selected navigation and lifecycle states are exposed programmatically and not only by color;
- primary actions have comfortably large targets; 24 by 24 CSS pixels is the WCAG AA floor, not an aspirational desktop size;
- reduced motion, 200–400% zoom/reflow, high contrast, and Windows text scaling are included in manual qualification;
- healthy background updates do not steal focus.

The current `aria-live="polite"` on the entire selected Tender detail should be reviewed: announcing a large changing region can become noisy. Prefer targeted status regions for the action that changed.

### 7.2 Forms and errors

**Evidence fact:** W3C recommends short forms, explicit labels, grouped related controls, instructions before they are needed, and notifications that identify the error and explain how to correct it ([WAI Forms Tutorial](https://www.w3.org/WAI/tutorials/forms/), [WAI User Notification](https://www.w3.org/WAI/tutorials/forms/notifications/)).

**Design inference:** Replace large always-visible configuration forms with task-specific forms. Show an error summary at the top linking to invalid fields, inline corrective text, preserved user input, and an explicit statement when the Tender was not changed. Required rationale should explain why it is required and what record it will enter.

### 7.3 Dense tables and evidence grids

**Evidence fact:** Accessible data tables need structural header/data associations and generally benefit from captions and summaries. W3C recommends breaking overly complex tables into simpler topic-specific tables where possible ([WAI Tables Tutorial](https://www.w3.org/WAI/tutorials/tables/), [WAI table tips](https://www.w3.org/WAI/tutorials/tables/tips/)).

**Evidence fact:** WAI-ARIA distinguishes a readable semantic table from an interactive grid. A grid creates a composite widget with managed focus and arrow-key interaction and should not be introduced merely because the content is tabular ([WAI-ARIA Grid Pattern](https://www.w3.org/WAI/ARIA/apg/patterns/grid/)).

**Design inference:** Use tables for comparison tasks—BOQ rows, compliance coverage, scenario comparison, package manifest—not for general layout. Provide sticky visual headers without breaking semantic associations, captions, row headers, keyboard-reachable actions, column customization for expert density, and a list/detail alternative at narrow widths or high zoom.

### 7.4 Arabic and mixed-direction evidence

**Evidence fact:** W3C internationalization guidance recommends declaring base direction and tightly isolating opposite-direction phrases. When runtime text direction is unknown, `dir="auto"` or `<bdi>` usually supplies the correct base direction and prevents spillover into surrounding text ([W3C inline bidi guidance](https://www.w3.org/International/articles/inline-bidi-markup/index.en.html), [structural RTL guidance](https://www.w3.org/International/questions/qa-html-dir)).

**Design inference:** Quantix should:

- preserve and show the source language explicitly;
- apply `lang` and `dir` at the smallest coherent content block;
- isolate filenames, paths, clause identifiers, currencies, dates, and hashes embedded in Arabic;
- keep original and derived translation visually paired but never interchangeable;
- test Arabic/English mixed tables, evidence quotations, search hits, PDF locations, and form entry with actual native fonts and screen readers;
- keep application navigation direction independent from the direction of an individual source excerpt.

## 8. Useful first-party product precedents

These sources describe their vendors' products, not neutral usability studies. They are useful only as evidence of established workflow patterns.

### Procore Tender Management

**Evidence fact:** Procore's enhanced Tender Management centers named Tender Packages and Tender Forms, then bidder invitation, side-by-side tender leveling, award, and contract conversion ([Procore Tender Management](https://en-gb.support.procore.com/products/online/user-guide/project-level/bidding/tutorials/about-bid-management-enhanced-experience)).

**Design inference:** The useful precedent is object-centered navigation and task-specific comparison, not Procore's owner/GC procurement scope. Quantix should similarly center the current Tender, exact package/version, and comparison task rather than display every control in one workspace.

### Oracle Primavera Unifier

**Evidence fact:** Unifier exposes project/shell navigation and routes business-process records through creation, review, revision, approval/rejection, and recorded actions ([Oracle workflow overview](https://docs.oracle.com/en/industries/construction-engineering/primavera-unifier/26/user-help/aboutworkflows-73172a.html)). Its Request for Bid feature presents different requestor and bidder forms ([Oracle RFB](https://docs.oracle.com/en/industries/construction-engineering/primavera-unifier/26/udesigner/requestforbidrfb-77699a.html)).

**Design inference:** The useful precedent is separating the stable project context, business-process record, current step, and available action. Quantix can do this with far less configuration and terminology because v0 has one role and an opinionated domain.

### Autodesk Construction Cloud file packages

**Evidence fact:** Autodesk file packages explicitly distinguish packages that follow current file versions from packages fixed to selected versions, expose package IDs and lists, and support locking before downstream workflows ([Autodesk File Packages](https://help.autodesk.com/cloudhelp/ENU/Docs-Files/files/File_Packages_Docs.html), [Create Packages](https://help.autodesk.com/cloudhelp/ENU/Docs-Files/files/file-packages/Create_Packages.html)).

**Design inference:** Quantix's stronger immutable-version model should be made similarly legible in ordinary language: “follows current approved version,” “fixed to v3,” “stale because Addendum 02 changed its basis,” and “frozen by Final Approval.” Users should not have to infer these states from hashes.

Autodesk's first-party construction takeoff and estimating material also describes a contractor-side chain from current drawings/models to quantities, then material, labor, equipment, subcontractor cost, markup, estimate, and proposal, with revision-aware comparison and quantity audit trails ([Autodesk construction takeoff](https://construction.autodesk.com/workflows/construction-takeoff/), [Autodesk cost estimating](https://construction.autodesk.com/workflows/cost-estimating/)). **Design inference:** This supports keeping Quantix's source baseline, BOQ, Basis of Estimate, scenarios, and proposal price visibly connected rather than distributing them across apparently unrelated panels.

## 9. Prototype and validation plan

### 9.1 Prototype question

**Design inference:** The prototype should answer one question:

> Can an Engineer understand a Tender's state, find the next consequential action, inspect enough exact evidence to trust or correct it, and recover from interrupted AI work without learning Quantix's internal module graph?

This is a runnable interaction question, so conversation alone is insufficient.

### 9.2 Smallest prototype scope

Use one realistic bilingual synthetic Tender state and prototype these connected flows:

1. **First orientation:** create/open a Tender, understand what Quantix and AI do, and import the Tender Package.
2. **Returning session:** identify the deadline, current phase, material change, active work, and next Engineer action within seconds.
3. **Evidence review:** inspect an AI-proposed deadline or requirement beside its exact Arabic/English source, then Verify, Correct, or Raise Query.
4. **Formal decision:** review a Bid Decision or Work Plan pack, inspect changes/blockers/evidence, and record an explicit version-bound decision.
5. **Run recovery:** let a background Agent Run continue while navigating, then handle Failed and Indeterminate outcomes safely.
6. **Change impact:** register an addendum and understand which requirements, price, decisions, and package items became stale.
7. **Final assurance:** review release blockers, exact package coverage, Manual Verification, and the consequence of Final Approval.

The prototype does not need working parsing, AI, storage, or every domain screen. It needs credible state transitions and enough realistic information density to test navigation and judgment.

### 9.3 Evaluation tasks

Observe the Engineer attempting these tasks without coaching:

- Explain in their own words what Quantix is responsible for and what remains their responsibility.
- Find what must be done next and why the Tender cannot advance.
- Determine whether a proposed critical deadline is supported by the correct source version.
- Identify what changed after an addendum and what remains valid.
- Explain the difference between an AI proposal, independent verification, and Engineer approval.
- Stop or recover a long-running job without assuming partial output was accepted.
- Make a formal decision and identify the exact object/version it affected.
- Determine whether the package is ready for external submission and what Quantix has not done.

Record task success, wrong turns, time to first correct action, evidence-inspection behavior, misunderstood status terms, missed blockers, confidence before and after evidence review, and qualitative reactions. Do not optimize only for speed: safe refusal and deliberate evidence inspection are valid outcomes in high-stakes steps.

## 10. Interview frontier for the next grilling phase

Desk research cannot settle these decisions. They should be resolved with the product owner in rounds, beginning with the real work rather than colors or components.

### Round A — the actual user and day

- Is the v0 user primarily a tender engineer doing hands-on analysis, a tendering manager supervising specialists, or one person switching between both modes?
- How many simultaneous Tenders are normal, and how often does the user return to each one?
- What usually triggers opening the tool: a new invitation, a morning status check, an addendum, a management decision, or a deadline emergency?
- What tools, files, spreadsheets, emails, and review rituals are used today?

### Round B — control and delegation

- Which work should Quantix start automatically after an approved gate, and which work should always require an explicit “Run” action?
- Does the Engineer want to see Agent Profiles as a team, as task assignees, or only when a capability/permission/review problem occurs?
- Which failures demand immediate interruption, and which can wait in the attention queue?
- What level of provider/run detail builds confidence versus becoming technical noise?

### Round C — decision packs

- At Bid Decision, Work Plan, price, baseline, and Final Approval, what are the three to five facts the Engineer personally checks first?
- Does the Engineer compare versions, scenarios, or alternatives side by side, and at which gates?
- What rationale, conditions, and signatures are required by real company practice?
- Which decisions should support deliberate defer/hold, and what deadline consequence should be shown?

### Round D — evidence and documents

- Does the Engineer think first in documents, clauses, requirements, disciplines, deliverables, BOQ rows, or submission sections?
- Should the source document remain continuously visible during review, or open only on demand?
- How are Arabic and English normally mixed? Are translations reviewed, and by whom?
- Which document formats and large-table interactions dominate the actual tender?

### Round E — density and navigation

- Does the Engineer prefer a management summary by default with an expert-density toggle, or consistently dense tables?
- Which seven-or-fewer destinations match their mental model?
- Should Decisions and Runs be global destinations, persistent drawers, or both?
- What should the first screen show when everything is healthy and no action is due?

### Round F — visual character

- Should Quantix feel like an engineering control room, a document review studio, a project management tool, or a restrained professional desktop utility?
- Which current products feel trustworthy or irritating, and why?
- What color, typography, spacing, and motion choices suit extended daily use rather than a demo?

## 11. Product principles to carry into a specification

1. **Organize around the Tender and Engineer action, not renderer/backend modules.**
2. **Make the lifecycle visible but do not force parallel work into a false wizard.**
3. **Default to exceptions and next actions; preserve expert exploration.**
4. **Use one authoritative decision inbox and one consistent decision interaction.**
5. **Keep exact evidence one action away and in context.**
6. **State the trust class, currentness, and review status of every material conclusion.**
7. **Make AI capability, limits, inputs, and correction paths concrete.**
8. **Run long work in the background with durable status and explicit recovery.**
9. **Show consequences before high-impact actions and immutable receipts afterward.**
10. **Treat Arabic/English direction, keyboard operation, zoom, screen readers, and high contrast as core product behavior.**
11. **Keep healthy infrastructure quiet; make unsafe infrastructure impossible to miss.**
12. **Prototype the critical flows with realistic density before rebuilding the renderer.**

## 12. What should happen next

1. Continue in the same discovery context with a stateful `/grill-with-docs` interview, using the frontier above but asking one decision at a time.
2. Record agreed vocabulary and durable product decisions in `CONTEXT.md` and ADRs only when they become clear; do not encode unsettled visual preferences as architecture.
3. At the first question that requires seeing and using the interaction, create a narrow `/handoff` to a `/prototype` branch/directory.
4. Test the prototype against the seven tasks above with the product owner first, then at least one practicing tender engineer who was not involved in its design.
5. Bring validated findings back, write the redesign specification, split it into tracer-bullet tickets, and only then use `/implement`.

The immediate next step is not another corrective UI commit. It is to determine which Engineer is being designed for and what they expect to see in the first 30 seconds of a real Tender session.
