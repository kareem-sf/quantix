# Specification: Manager-led Tender Workspace UX

Status: approved product specification
Decision basis: completed `ask-matt` / `grill-with-docs` design session
Primary actor: Tendering Engineer

## Problem Statement

The current Quantix renderer exposes the system's modules, records, controls, status, and Agent activity as many simultaneous panels. A Tendering Engineer must understand Quantix's internal architecture before they can understand what to do. Dense navigation, small text, dashboards, permanent inspectors, competing actions, and raw status make the product visually difficult, operationally confusing, and unsuitable for a first-time user.

The Engineer does not want to operate an AI control room. They want a professional Tender workspace led by one Tendering Manager Agent. The Manager must understand the Tender, ask only necessary questions, prepare the complete Work Plan and delegation, present the exact plan for approval, coordinate approved work, bring material decisions back to the Engineer, and make every Agent's attributable context and collaboration inspectable without forcing all of that information onto the default screen.

Quantix must preserve its controlled lifecycle, exact Evidence, immutable versions, permissions, independent review, provenance, recovery, and Engineer-in-the-Loop authority. Simplification cannot weaken those controls; it must reveal them only when they help the Engineer complete the current task.

## Solution

Replace the module-catalogue renderer with a Manager-led Tender Workspace.

Quantix resumes the last active Tender directly in its durable Tendering Manager conversation. With no Tender, it presents one action: `Start a Tender`. A collapsible sidebar keeps the work list limited to active Tenders, with `Archived & Trash` below that list and one application Settings control at the bottom. Inside the selected Tender, three quiet destinations—`Manager`, `Work`, and `Files`—organize the workspace. Formal decisions return through the Manager rather than requiring a separate inbox.

The default Manager surface contains the latest meaningful message, one current action, a plain composer, and a quiet live-team control. The Manager presents one decision at a time, prepares the entire Work Plan, and routes formal approval through a focused exact-plan review. After approval, all unblocked work starts automatically within the approved boundary.

`Team working` opens a full shared team room with meaningful named communication, handoffs, blockers, and outputs. Selecting an Agent identity opens that Agent's focused workroom with `Conversation`, `Context`, `Activity`, and `Outputs`. The complete operational record remains accessible, including exact instructions, inputs, permissions, messages, actions, handoffs, outputs, and history, while credentials, unsafe raw provider traffic, and hidden model reasoning remain excluded.

The visual system is calm, readable, minimalist, and beginner-oriented. Secondary evidence, plan detail, technical activity, version history, audit facts, and failure diagnostics open only through clearly labeled focused views or disclosures.

## User Stories

1. As a Tendering Engineer, I want Quantix to resume my last active Tender, so that I can continue without navigating a home dashboard.
2. As a Tendering Engineer with no existing Tender, I want one `Start a Tender` action, so that the first step is unmistakable.
3. As a Tendering Engineer, I want the Tendering Manager conversation to dominate the selected Tender, so that I always know who is coordinating the work.
4. As a Tendering Engineer, I want one current question, decision, or result in focus, so that unrelated information does not compete for my attention.
5. As a Tendering Engineer, I want the Manager to explain what happens after my action, so that I can act confidently.
6. As a Tendering Engineer, I want the Tender sidebar to contain only my Tenders, so that system modules do not become navigation noise.
7. As a Tendering Engineer, I want each Tender row to show only its name, phase, and whether it needs me, so that I can scan several Tenders quickly.
8. As a Tendering Engineer, I want Agents to keep working safely when I switch Tenders, so that parallel Tender work does not require supervision.
9. As a Tendering Engineer, I want every Tender to have one durable Manager conversation, so that decisions and summaries are not fragmented across arbitrary chats.
10. As a Tendering Engineer, I want focused task and review threads to return material conclusions to the Manager, so that the main conversation remains coherent.
11. As a Tendering Engineer, I want `Manager`, `Work`, and `Files` as the only persistent Tender destinations, so that navigation remains shallow and predictable.
12. As a Tendering Engineer, I want formal decisions routed through the Manager, so that I do not have to monitor a separate approval inbox.
13. As a Tendering Engineer, I want to select a Tender Package and let Quantix derive available facts, so that I do not complete an unnecessary setup form.
14. As a Tendering Engineer, I want the Manager to ask only for genuinely missing information, so that intake uses my time responsibly.
15. As a Tendering Engineer, I want a truthful intake status line, so that I understand what Quantix is doing without watching a technical event stream.
16. As a Tendering Engineer, I want to switch Tenders while intake continues, so that one long review does not block other work.
17. As a Tendering Engineer, I want the Manager to bring me back when a question or result is ready, so that I do not poll the workspace.
18. As a Tendering Engineer, I want the Manager to prepare the complete Work Plan and delegation, so that I do not manually orchestrate specialist Agents.
19. As a Tendering Engineer, I want a concise conversational plan summary, so that I understand the proposal before opening its detail.
20. As a Tendering Engineer, I want formal plan approval to happen only in a focused review of the exact version, so that an abbreviated chat summary cannot become approval.
21. As a Tendering Engineer, I want the plan overview to explain outcome, deadline, major risks, team, duration, and expected decisions, so that I can assess it quickly.
22. As a Tendering Engineer, I want routine plan detail collapsed into focused sections, so that exceptional and unresolved items remain prominent.
23. As a Tendering Engineer, I want plan sections for work and sequence, team, access and limits, independent review, and assumptions, so that every material control is inspectable.
24. As a Tendering Engineer, I want to request a change against the exact plan section, so that the Manager understands what must change.
25. As a Tendering Engineer, I want the Manager to create a successor plan version with a clear difference, so that controlled history is never silently edited.
26. As a Tendering Engineer, I want `Approve and proceed` to start all unblocked approved work, so that I do not start each task manually.
27. As a Tendering Engineer, I want the Manager to coordinate sequencing and handoffs inside the approved boundary, so that routine delegation does not interrupt me.
28. As a Tendering Engineer, I want the Manager to propose an amendment when scope or authority changes, so that Agents cannot silently exceed the approved plan.
29. As a Tendering Engineer, I want Work grouped by `Needs you`, `Working`, and `Done`, so that I can understand progress without a Kanban board or Gantt chart.
30. As a Tendering Engineer, I want each Work row to state the outcome, responsible specialist, and plain-language status, so that the list is understandable without opening it.
31. As a Tendering Engineer, I want a waiting Work item to explain its dependency, so that `Waiting` is actionable rather than opaque.
32. As a Tendering Engineer, I want selecting Work to open its focused thread, Evidence, context, and output, so that detail remains available on demand.
33. As a Tendering Engineer, I want to pause or stop a selected task with its downstream impact explained, so that I retain meaningful control.
34. As a Tendering Engineer, I want scope redirection to go through the Manager and a plan amendment, so that task controls cannot bypass approval.
35. As a Tendering Engineer, I want six consistent work states—Waiting, Working, Needs you, Paused, Done, and Failed—so that status language remains stable.
36. As a Tendering Engineer, I want non-working states to explain their cause and next step, so that status is not merely decorative.
37. As a Tendering Engineer, I want `Team working` available near the Tender title, so that I can inspect live collaboration without keeping it on screen.
38. As a Tendering Engineer, I want the shared team room to replace the Manager view temporarily, so that live collaboration has enough space without creating a permanent split pane.
39. As a Tendering Engineer, I want meaningful Agent messages, questions, findings, handoffs, blockers, and outputs shown live, so that I can observe real collaboration.
40. As a Tendering Engineer, I want routine coordination to remain visually quiet, so that material communication is easy to identify.
41. As a Tendering Engineer, I want consequential messages labeled as Question, Finding, Handoff, Blocker, or Output, so that the room remains readable and traceable.
42. As a Tendering Engineer, I want consequential messages linked to their exact task, Agent Run, Evidence, or Artifact Version, so that conversation can be verified.
43. As a Tendering Engineer, I want older team conversation grouped by work period or milestone, so that history remains accessible without becoming an endless undifferentiated stream.
44. As a Tendering Engineer, I want filters for Needs you, Handoffs, Outputs, and All messages, so that I can find the type of collaboration I need.
45. As a Tendering Engineer, I want to speak naturally in the team room and ask any specialist a question, so that collaboration feels direct.
46. As a Tendering Engineer, I want the Manager to coordinate work instructions from the room, so that direct conversation does not fragment authority.
47. As a Tendering Engineer, I want every visible Agent identity to be selectable, so that transparency is consistently discoverable.
48. As a Tendering Engineer, I want an Agent workroom to open on Conversation, so that I first see what the Agent communicated and received.
49. As a Tendering Engineer, I want separate Context, Activity, and Outputs tabs, so that complete transparency does not mean simultaneous data overload.
50. As a Tendering Engineer, I want Context to show the exact approved instructions, objective, input versions, Data Views, thread exposure, and Permission Grant for the selected Agent Run, so that I can prove what the Agent actually knew.
51. As a Tendering Engineer, I want the Agent's broader permission ceiling shown separately from granted context, so that possible access is not confused with actual access.
52. As a Tendering Engineer, I want prior Agent Runs and exact context differences available, so that changed instructions, inputs, permissions, and exposure remain traceable.
53. As a Tendering Engineer, I want Activity to show meaningful actions first, so that technical telemetry does not dominate the workroom.
54. As a Tendering Engineer, I want an optional technical activity view with exact Typed Tool calls, timestamps, retries, failures, and resource observations, so that I can audit execution when necessary.
55. As a Tendering Engineer, I want Outputs to show produced files, structured results, Evidence, citations, and preserved versions, so that Agent work can be verified.
56. As a Tendering Engineer, I want Agent identities expressed as professional roles and responsibilities, so that Quantix does not simulate fictional employees.
57. As a Tendering Engineer, I want complete messages and meaningful progress updates rather than token streaming, so that live work remains readable.
58. As a Tendering Engineer, I want a subtle working indicator while an Agent is composing, so that I know work continues without distracting animation.
59. As a Tendering Engineer, I want completed work returned through the Manager with the exact output attached, so that I do not monitor Files or a review inbox.
60. As a Tendering Engineer, I want formal output review in a focused workspace, so that output, Evidence, differences, findings, and consequences are assessed together.
61. As a Tendering Engineer, I want the current Artifact Version shown first with History and comparison available, so that prior work remains preserved without clutter.
62. As a Tendering Engineer, I want Tender documents separated from Quantix work, so that authoritative sources never become mixed with generated outputs.
63. As a Tendering Engineer, I want the original Tender Package structure preserved inside Tender documents, so that the received package remains recognizable.
64. As a Tendering Engineer, I want a focused document viewer with highlighted citations, annotations, versions, and return-to-origin behaviour, so that source review does not make me lose my place.
65. As a Tendering Engineer, I want short source excerpts and exact citations in Manager questions, so that I can understand the issue before opening the full source.
66. As a Tendering Engineer, I want conflicting sources presented together in focused Evidence review, so that I can make an informed decision.
67. As a Tendering Engineer, I want Addenda announced by the Manager with changed requirements and invalidated work summarized, so that revisions cannot pass unnoticed.
68. As a Tendering Engineer, I want prior source versions and outputs preserved after an Addendum, so that change history remains defensible.
69. As a Tendering Engineer, I want calculations presented as structured controlled tables with inputs, formulas, assumptions, scenarios, and differences, so that commercial results are not untraceable chat arithmetic.
70. As a Tendering Engineer, I want the Manager to explain calculation outcomes in plain language, so that structured commercial work remains approachable.
71. As a Tendering Engineer, I want the submission deadline visible quietly in the Tender header, so that the controlling date remains available without countdown theatrics.
72. As a Tendering Engineer, I want schedule risk escalated only when it becomes material, so that ordinary deadlines do not create ambient urgency.
73. As a Tendering Engineer, I want one clearly labeled Needs your decision message, so that formal attention is unmistakable.
74. As a Tendering Engineer, I want one restrained sidebar indicator and an operating-system notification only when Quantix is unfocused, so that attention does not become notification noise.
75. As a Tendering Engineer, I want one decision at a time with only inseparable details grouped, so that large forms do not obscure the actual choice.
76. As a Tendering Engineer, I want the Manager to state how many later decisions remain without displaying them all, so that I can anticipate work without losing focus.
77. As a Tendering Engineer, I want Manager messages to lead with the outcome or action in concise prose, so that explanations are easy to scan.
78. As a Tendering Engineer, I want structured action cards only for decisions, approvals, blockers, plans, and deliverables, so that ordinary conversation stays calm.
79. As a Tendering Engineer, I want a plain composer with text, Tools & Context, my Tender's provider and model selection, and Send, so that I can control future AI work without leaving the Manager.
80. As a Tendering Engineer, I want attachments, search, and governed actions behind Tools & Context, so that expert capability does not burden the beginner surface or imply unrestricted access.
81. As a Tendering Engineer, I want a sent message corrected through a visible successor after work has begun, so that history cannot be rewritten silently.
82. As a Tendering Engineer, I want Search this Tender in the Tender workspace header and through Tools & Context, with an optional keyboard shortcut, so that all search remains scoped and easy to reach.
83. As a Tendering Engineer, I want search results grouped by conversation, Work, Files, Evidence, and Agents, so that different record types remain understandable.
84. As a Tendering Engineer, I want an attachment registered and classified before Agent exposure, so that attaching a file does not silently grant access.
85. As a Tendering Engineer, I want Quantix to explain which task or Agent will receive an attachment, so that disclosure remains intentional.
86. As a Tendering Engineer, I want a minimal Tender header containing name, deadline, Team working, and Search this Tender, while lifecycle actions live on the Tender row, so that orientation does not become a dashboard.
87. As a Tendering Engineer, I want a responsive workspace that collapses navigation and opens focused views instead of squeezing panes, so that text remains readable in smaller windows.
88. As a Tendering Engineer, I want calm neutral surfaces, restrained blue emphasis, subtle boundaries, and generous whitespace, so that Quantix feels professional rather than like a control room.
89. As a Tendering Engineer, I want body text sized for comfortable reading and primary controls large enough to use confidently, so that beginner usability is not sacrificed for density.
90. As a Tendering Engineer, I want Quantix to follow the operating-system theme, so that appearance is consistent without adding a task-bar preference.
91. As a Tendering Engineer, I want icons from an established coherent set and text on critical actions, so that controls are recognizable.
92. As a Tendering Engineer, I want only short functional transitions and reduced-motion support, so that movement communicates state rather than decoration.
93. As a Tendering Engineer, I want startup checks to remain silent when healthy, so that infrastructure does not precede my Tender work.
94. As a Tendering Engineer, I want a truthful startup stage if checks take longer, so that waiting is understandable without fabricated progress.
95. As a Tendering Engineer, I want to learn Quantix through my first real Tender rather than a product tour, so that onboarding produces useful work.
96. As a Tendering Engineer, I want readable Tender access preserved when the AI Provider is unavailable, so that provider failure does not hide canonical work.
97. As a Tendering Engineer, I want affected Agent work paused with one plain-language recovery action, so that operational failures remain manageable.
98. As a Tendering Engineer, I want technical failure detail available only when requested, so that diagnostics do not overwhelm the main workflow.
99. As a Tendering Engineer, I want interrupted work resumed only when Quantix proves continuity of the same Agent Run, so that recovery never invents success.
100. As a Tendering Engineer, I want an indeterminate outcome isolated to the affected task with safe Manager-presented choices, so that unrelated work can continue.
101. As a Tendering Engineer, I want a human-readable Tender history with optional technical detail and controlled export, so that complete auditability remains accessible.
102. As a Tendering Engineer, I want Settings limited to provider, storage, notifications, accessibility, appearance, updates, and diagnostics, so that Tender work does not leak into configuration.
103. As a keyboard user, I want every workflow fully operable with visible focus, so that Quantix does not require a pointer.
104. As a screen-reader user, I want meaningful structure, labels, and status announcements, so that the workspace remains understandable nonvisually.
105. As a low-vision user, I want the workspace to remain usable at 200% zoom and high contrast, so that content does not overlap or disappear.
106. As a motion-sensitive user, I want reduced-motion preferences honored, so that functional updates do not cause discomfort.
107. As a first-time Tendering Engineer, I want the interface validated with people unfamiliar with it, so that familiarity from the design team does not hide usability failures.
108. As a Tendering Engineer, I want Rename, Archive, and Move to Trash in each Tender row's one menu, so that management is discoverable without becoming another permanent workspace panel.
109. As a Tendering Engineer, I want Archive to become available only at a proven safe terminal boundary, so that protected work cannot be made read-only mid-operation.
110. As a Tendering Engineer, I want Archived Tenders separated from my active list but openable through the same Manager, Work, and Files surfaces with a clear read-only banner and Restore action, so that healthy history is not presented as recovery failure.
111. As a Tendering Engineer, I want Delete to move a safe Tender into recoverable Tender Trash, so that an ordinary delete action is not immediately irreversible.
112. As a Tendering Engineer, I want `Archived & Trash` to show Archived Tenders, trashed Tenders, restore actions, and deletion receipts without occupying a fourth Tender tab, so that lifecycle management remains findable but quiet.
113. As a Tendering Engineer, I want Permanent Tender Deletion available only from Tender Trash with an exact consequence review and explicit confirmation, so that every Quantix-controlled Tender copy is removed deliberately.
114. As a Tendering Engineer, I want local deletion to complete even when provider-thread cleanup is temporarily unavailable, with a minimal receipt and visible cleanup status, so that an external provider cannot hold local confidential data hostage.
114a. As a Tendering Engineer, I want a damaged Tender to have a recovery-specific Move to Trash action, so that I can contain it without Quantix opening an incompatible Store.
114b. As a Tendering Engineer, I want to permanently delete a recovery-required Tender with a rationale and exact-name confirmation, so that the destructive decision is explicit while the original Tender Package remains untouched.
114c. As a Tendering Engineer, I want restoring a damaged Tender to preserve its `Needs recovery` state, so that restoration never falsely suggests that the Store was repaired.
114d. As a Tendering Engineer, I want incomplete provider-reference discovery reported honestly in the Deletion Receipt, so that local deletion can finish without hiding the need for any external provider review.
115. As a Tendering Engineer, I want one application Settings control fixed at the bottom of the sidebar, so that global configuration is distinct from Tender work.
116. As a Tendering Engineer, I want Settings organized as General, AI & Models, Data & Storage, Updates, and About & Diagnostics with technical detail collapsed, so that beginner choices remain clear.
117. As a Tendering Engineer, I want Settings to prepare the default ChatGPT model and reasoning for new Tenders while requiring my explicit approval before Tender content is sent, so that connection setup stays simple and data destination remains intentional.
118. As a Tendering Engineer, I want one **Connect ChatGPT** action that opens my browser and completes automatically, so that I do not have to understand providers, ports, OAuth, or terminals.
119. As a Tendering Engineer, I want **Sign in on another device** offered only when the normal return path is unavailable or when I ask for help, so that the ordinary browser path remains the simple default.
120. As a Tendering Engineer, I want the alternate sign-in to show one copyable code and an OpenAI page, wait for me, and let me cancel, so that I can finish sign-in from another device without technical setup.
121. As a Tendering Engineer, I want ChatGPT tokens excluded from every Tender record, log, diagnostic, archive, and export, so that connection credentials remain separate from Tender information.
122. As a Tendering Engineer, I want advanced model settings kept out of connection setup, so that a model choice never blocks a first connection.
123. As a Tendering Engineer, I want every Agent Run to retain the ChatGPT model, reasoning setting, and catalogue provenance it started with, so that later selection changes do not rewrite ongoing or historical work.
124. As a Tendering Engineer, I want an unavailable model or reasoning choice kept visible with a compatible recommendation, so that capability drift is explicit and requires my confirmation.
125. As a Tendering Engineer, I want Quantix never to switch models or reasoning depth silently, so that AI behavior remains intentional.
126. As a Tendering Engineer, I want a ChatGPT connection problem not to disable local Tender work, so that expected offline operation is not recorded as failed work.
127. As a Tendering Engineer, I want a concise disclosure before I approve ChatGPT for future Tender content, so that I understand the data destination.
128. As a Tendering Engineer, I want Disconnect to cancel an active sign-in and remove local authentication safely, so that the next use requires a deliberate fresh connection.
129. As a Tendering Engineer, I want Quantix to warn me before the Tender Package picker when the ChatGPT default is incomplete, while allowing me to continue with local work, so that missing AI setup is intentional rather than a surprise.
130. As a Tendering Engineer, I want About & Diagnostics to explain each health finding and offer only safe typed repairs after I approve their impact, so that Repair Required is actionable without granting Quantix autonomous repair authority.

## Implementation Decisions

- Replace the obsolete renderer path rather than preserving compatibility with the panel-based module catalogue.
- Preserve the existing domain glossary, Engineer-in-the-Loop authority, immutable versions, Evidence, permissions, provenance, independent review, recovery, and formal approval gates.
- Introduce one workspace-level projection/gateway as the renderer's read seam. It supplies the Tender catalogue, selected Tender orientation, Manager conversation, current action, Work groups, Files collections, live team state, focused review references, and Agent workroom projections.
- Keep authoritative mutations behind existing domain-specific Host commands. The workspace gateway does not become an alternative workflow engine or approval authority.
- Persist Tender Office Conversation as attributable messages in the durable Manager conversation, shared team room, and focused task or review threads. Messages may link exact Tender Tasks, Agent Runs, Evidence, and Artifact Versions but never replace canonical records or formal approval.
- Represent consequential conversation messages with lightweight kinds: Question, Finding, Handoff, Blocker, and Output. Routine messages remain untyped conversational content.
- Support live workspace refresh through normalized attributable events or snapshot refresh, without rendering raw provider protocol or token deltas.
- Project Agent workrooms from exact Agent Profile Versions, Tender Tasks, Agent Runs, Provider Turn Requests, Data Views, Permission Grants, Provider Events, outputs, messages, and handoffs already controlled by the Host.
- Make the exact per-Run context the authoritative Context view. Permission ceilings, requested-but-ungranted access, and prior Thread Exposure remain separate concepts.
- Use current-first version presentation with immutable history and comparison for plans, messages corrected after execution, contexts, sources, calculations, and outputs.
- Separate Tender documents from Quantix work while retaining original package-relative source structure and exact provenance.
- Use controlled calculation records for estimates and scenarios; conversational explanation never becomes the calculation source of truth.
- Formal Work Plan and output approval actions bind exact immutable versions and are available only in focused review views.
- Starting, pausing, stopping, recovering, amending, approving, and returning work must surface the exact domain consequence before dispatching the existing Host command.
- Place Rename, Archive, and Move to Trash in each Tender sidebar row's Quantix menu. Make the same menu available through the row ellipsis, keyboard menu key, and secondary click. Place separate Archived and Trash collections behind one `Archived & Trash` sidebar destination; do not add retention to Manager, Work, Team, or Files.
- Open a healthy Archived Tender read-only through the ordinary Tender workspace with an Archived banner and Restore action. Do not project Archive as Recovery Required or label it as corruption.
- Reuse the Host's existing archive, restore, trash, restore-from-trash, purge, and Deletion Receipt authorities. Extend Permanent Tender Deletion to remove every identifiable Tender-associated Quantix-controlled copy and track asynchronous provider cleanup; do not create a parallel renderer lifecycle.
- Recovery-required Tenders use typed Host trash and purge commands that validate identity and Application Home ownership without opening the damaged Store. The renderer shows the catalogue name when available, requires a rationale for recovery deletion, and requires exact-name confirmation for permanent deletion. The original Tender Package is never treated as a Quantix-controlled copy.
- Restore of a recovery-required Tender restores its files and identity but projects `Needs recovery` until an explicit repair succeeds. Provider-reference discovery may be `Pending` or `Incomplete`; either state is visible in the receipt and never blocks local deletion.
- Disable Archive and Delete unless the Host proves the Tender is at the exact safe terminal boundary. The renderer never infers safety from visible activity alone.
- Place one application Settings control at the bottom of the sidebar. Settings replaces the Tender content area with a focused application-wide view and is not a fourth Tender destination.
- Persist non-secret Application Settings at installation scope. Keep ChatGPT authentication only in Quantix-owned `auth.json`; do not expose tokens to the renderer or copy them into Tender data.
- Implement exactly one AI adapter: ChatGPT through Quantix-owned OAuth and direct HTTPS/SSE execution. Do not support API keys, alternative providers, routing, or provider fallback.
- Maintain one ChatGPT Provider Connection. Application Settings holds the default AI Execution Selection copied into a new Tender; every Tender owns an optional independent selection for its future Agent Runs. Do not add per-Agent selection, multiple accounts, routing, or automatic fallback.
- Use the ready ChatGPT connection's versioned catalogue and expose model and reasoning choices only in advanced settings.
- Apply Tender provider, model, and reasoning changes atomically and capture the exact effective selection and catalogue provenance in every new Agent Run. Existing and already queued runs remain pinned; changing the application default never rewrites an existing Tender.
- Treat stale catalogues as explanation only. Capability or authentication drift pauses affected work as Waiting for AI Provider, preserves non-AI access, proposes a live compatible replacement, and requires explicit confirmation before changing selection.
- Disclose the ChatGPT data destination before the Engineer approves future Tender content. Disconnect cancels an active sign-in and deletes Quantix's local `auth.json` without claiming an external-account action.
- Diagnose setup, runtime, provider, integrity, update, and recovery health automatically through the redacted Quantix Doctor. Healthy checks remain quiet; findings state exact cause and impact, and only an Engineer-commanded typed action may repair local state.
- Use a professional neutral visual system with one restrained accent, comfortable reading typography, clear hierarchy, established icons, visible focus, and reduced motion.
- Follow the operating-system appearance preference; keep appearance controls in Settings.
- Do not implement dashboard cards, permanent evidence inspectors, agent strips, Gantt charts, dependency graphs, tiny telemetry, decorative status motion, fictional Agent portraits, or an expert-density mode.
- Treat the approved workspace and control concepts as production design references while keeping all interaction, text, controls, and data code-native. Do not ship raster mockups as application UI.
- Freeze the current development prototype as rejected exploratory work. It is not an implementation foundation and must be removed when the approved replacement begins.
- Keep generated Rust-owned TypeScript declarations committed and generated from their Rust DTOs when the workspace projection introduces or changes Host contracts.

## Testing Decisions

- Test externally observable Engineer behaviour, not component structure, CSS selectors, internal hook state, or implementation details.
- Use the complete Manager-led workspace as the highest renderer seam. Substitute the workspace gateway and drive workflows through accessible names and visible outcomes.
- Cover the full tracer workflow: launch, resume or start a Tender, select a Tender Package, observe truthful intake, answer the Manager, review the exact Work Plan, request a section change, compare the successor version, approve, observe automatic delegation, open Team working, inspect an Agent's Conversation and Context, review Evidence, pause affected work, and receive a result through the Manager.
- Cover navigation and state preservation across Tenders, focused reviews, shared team room, Agent workrooms, document views, search, and return-to-origin focus.
- Cover every formal action's exact-version binding, stale-state refusal, visible consequence, and post-action refresh through the authoritative Host command.
- Cover complete operational transparency: exact per-Run inputs, permissions, instructions, messages, handoffs, meaningful activities, failures, outputs, citations, and history without exposing secrets or hidden reasoning.
- Cover provider unavailable, independent connection failures, offline/read-only access, Waiting for AI Provider, long startup, Addendum invalidation, interrupted and indeterminate Agent Runs, update/recovery blockers, and partial-task isolation.
- Cover live provider connection and catalogue projections, credential-free renderer boundaries, atomic AI Execution Selection, capability removal, invalid-selection confirmation, no silent fallback, and exact per-Run selection retention.
- Cover Archive and Trash discovery, safe-terminal refusal, Archived read-only access, restore, recoverable deletion, recovery-required trash and purge, exact Permanent Tender Deletion consequences, Deletion Receipts, and Provider Cleanup Pending or Incomplete without duplicating Host lifecycle authority. Verify that receipts contain no Tender content, sensitive paths, or provider thread references.
- Cover keyboard-only operation, focus movement and restoration, accessible dialogs and disclosures, status announcements, 200% zoom, high contrast, reduced motion, and no color-only meaning.
- Reuse the current renderer precedent of mocking the Host adapter at the module boundary, but consolidate story coverage at the workspace level rather than reproducing one isolated test suite per visual panel.
- Test Rust workspace projections and commands through the public Quantix Host interface, preserving current deterministic Host-level patterns and exact canonical records.
- Do not test private model reasoning, raw provider protocol, pixel-perfect screenshots, generated-image similarity, or arbitrary internal event ordering.
- Before production implementation, validate the low-fidelity clickable prototype with at least five practicing Tendering Engineers unfamiliar with the interface. Test starting a Tender, answering the Manager, reviewing a plan, inspecting an Agent, following a handoff, finding Evidence, and correcting a decision.
- Treat confusion about the current action, formal approval, Agent authority, evidence origin, or return navigation as a failed usability outcome requiring redesign before implementation.

## Out of Scope

- Exposing, reconstructing, storing, or claiming access to private chain-of-thought or hidden model reasoning.
- Displaying credentials, Secret data, unsafe raw provider traffic, token streams, or unrestricted system logs.
- Allowing an Agent to approve, infer approval, expand its own permission, communicate externally, release, or submit.
- A global Manager or cross-Tender Agent memory. Each Tender and Agent Profile remains isolated; approved Company Knowledge follows its existing controlled path.
- A permanent dashboard, control-room layout, module catalogue, technical inspector, Agent monitor, review inbox, or separate decisions inbox.
- Arbitrary user-created chats, unrestricted direct Agent delegation, or silent editing of controlled plans and records.
- Mobile or web-specific layouts. The specification is for the native desktop workspace, with responsive behaviour inside its supported window sizes.
- Any AI connection other than the one ChatGPT account; API-key access; multiple ChatGPT accounts; per-Agent provider selection; automatic routing or fallback; local models; and generic OpenAI-compatible endpoints.
- Replacing canonical Tender lifecycle logic, approval semantics, Evidence, calculation, review, recovery, release, or retention rules already implemented by the Host.
- Generated portraits, decorative illustrations, or generated raster controls in the shipped application.
- Automated test execution, release verification, or production builds during the specification phase.

## Further Notes

- The canonical terms are defined in `CONTEXT.md`, including Tender, Tendering Engineer, Tendering Manager Agent, Tender Office Conversation, Tender Task, Agent Run, Evidence, Work Plan, Permission Grant, Data View, and Artifact Version.
- The Manager-first orchestration authority is recorded by ADR 0011 and remains unchanged.
- Existing primary-source research on beginner workspaces, Tendering Engineer workflow, and agent-workspace interface patterns supports the progressive-disclosure direction but does not replace usability validation.
- The current renderer and rejected prototype remain useful evidence of what not to preserve: simultaneous panels, status density, tiny text, internal-module navigation, and decorative live telemetry.
- Ticket decomposition must use tracer-bullet slices that keep the application working end to end. The first slice should prove the approved workspace seam and the launch-to-Manager path before adding Agent transparency, focused reviews, and remaining capability projections.
- Production implementation must remove obsolete paths rather than retaining compatibility modes, variant switchers, or parallel workspace architectures.
