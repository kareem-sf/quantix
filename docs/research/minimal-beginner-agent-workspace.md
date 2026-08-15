# Minimal beginner agent workspace

Status: product-direction research  
Evidence snapshot: 2026-08-14  
Primary user: a first-time Tendering Engineer  
Source policy: first-party design systems, accessibility standards, and official product documentation only

## Executive decision

Quantix should not open as an orchestration dashboard. It should open as a calm conversation with the Tendering Manager Agent and make one next action unmistakable.

The rejected prototype made the product's internal model visible all at once: three persistent panes, a global header full of status, nested navigation sections, pinned metrics, conversation tabs, decision cards, blocking questions, agent telemetry, an evidence inspector, and a bottom activity strip. That is not merely a styling problem. It asks a new Engineer to understand the product's vocabulary and information architecture before doing useful tender work.

The replacement should use **progressive disclosure, not simultaneous visibility**:

- one main surface: the Manager conversation;
- one current question, decision, or deliverable in focus;
- one primary action;
- no persistent inspector, agent strip, dashboard, or technical trace;
- a compact, plain-language work indicator that opens details on request;
- contextual source and agent activity panels that appear only when invoked;
- a short launch transition followed by immediate work, with healthy checks silent.

This is a design inference from the evidence below, not a claim that a vendor's interface has been usability-tested for tendering engineers. The first prototype must still be tested with a practicing Tendering Engineer who has not seen its design.

## What the primary sources agree on

### 1. Put the user's purpose before the product's structure

Apple's current design principles say to include only what is necessary, establish a clear hierarchy, use familiar concepts, and get people directly to the task or content. Apple explicitly distinguishes simplicity from visual minimalism: the goal is a focused and useful experience in which secondary things fall away, not an empty-looking interface. [Apple Human Interface Guidelines: Design principles](https://developer.apple.com/design/human-interface-guidelines/design-principles)

Microsoft's current Windows guidance similarly calls for predictable navigation, clear affordances, consistent typography and hierarchy, and concise language that reduces cognitive load. Its writing guidance says to lead with the important point, use familiar language, and tell people what to do next without unnecessary information. [Windows design guidelines](https://learn.microsoft.com/en-us/windows/apps/design/guidelines-overview), [Windows writing style](https://learn.microsoft.com/en-us/windows/apps/design/style/writing-style)

W3C's cognitive-accessibility guidance explains why this matters beyond taste: clearly structured content, consistent labels, predictable behavior, short blocks of plain language, and the ability to suppress continually changing content help people focus, understand, and resume after interruption. [W3C: Cognitive and learning disabilities](https://www.w3.org/WAI/people-use-web/abilities-barriers/cognitive/), [W3C: Use clear and understandable content](https://www.w3.org/WAI/WCAG2/supplemental/objectives/o3-clear-content/)

**Quantix consequence:** The first screen should answer one human question: **“What do I need to do now?”** It should not attempt to answer “What exists in Quantix?” or “What is every agent doing?” at the same time.

### 2. Reveal detail only when it becomes relevant

Apple recommends hiding details until they are relevant, keeping likely actions visible, and hiding advanced functionality by default. It warns that multiple disclosure controls in one view add complexity and can be confusing. [Apple HIG: Disclosure controls](https://developer.apple.com/design/human-interface-guidelines/disclosure-controls)

Microsoft describes progressive disclosure as a way to simplify the baseline experience and reduce perceived clutter. It recommends showing contextual commands only when they apply, reducing the visual weight of secondary controls, and revealing follow-up steps after prerequisites. It also warns that hidden features need an obvious, predictable affordance. [Microsoft: Progressive disclosure controls](https://learn.microsoft.com/en-us/windows/win32/uxguide/ctrl-progressive-disclosure-controls)

Material Design 3's supporting-pane layout gives the primary content most of the window and uses a secondary pane for supporting content. Its layered layouts are specifically presented as a way to focus on a temporary task. [Material Design 3: Canonical layout examples](https://m3.material.io/foundations/layout/canonical-examples/overview)

**Quantix consequence:** Evidence, plan detail, agent rosters, and technical activity should be contextual drawers, sheets, or focused work views. They must not occupy permanent columns. A “Show work” or “View source” control must clearly say what will open and retain a stable location.

### 3. Ask one thing at a time and expose one main action

The GOV.UK Design System recommends starting with one question per page because it helps people understand the question and focus on the answer. Each question should have a specific heading, only necessary supporting text, and a clear continue action. It also recommends that a service start point give only enough information to understand the service and use one button for the principal action; a secondary call to action should be a link. [GOV.UK: Question pages](https://design-system.service.gov.uk/patterns/question-pages/), [GOV.UK: Start using a service](https://design-system.service.gov.uk/patterns/start-using-a-service/)

Microsoft's button guidance recommends exposing only one or two buttons at a time and using concise, specific labels that describe the action. [Microsoft: Buttons](https://learn.microsoft.com/en-us/windows/apps/develop/ui/controls/buttons)

**Quantix consequence:** The Manager should present one blocking question or one approval summary at a time. For example:

> I reviewed the Tender package. Before I prepare the work plan, who carries the six-year latent-defect insurance?

The primary action can be `Answer Manager` or, after the Engineer responds, `Review work plan`. “Open brief,” “Review clauses,” “Record treatment,” “Inspect exact plan,” “Approve,” “Return,” and “Compare source” must not all compete on the same screen.

### 4. Use onboarding to complete real work, not teach the interface

Apple says onboarding should be fast and optional, teach through interaction, and provide contextual tips near the current task rather than requiring people to memorize a tour. It recommends postponing nonessential setup and providing reasonable defaults so people can begin immediately. [Apple HIG: Onboarding](https://developer.apple.com/design/human-interface-guidelines/onboarding)

Apple also says launch should feel immediate, restore the previous state, and avoid using a launch screen as a branding or advertising interstitial. The launch surface should closely resemble the first app screen to avoid a jarring transition. [Apple HIG: Launching](https://developer.apple.com/design/human-interface-guidelines/launching)

**Quantix consequence:** Do not show a feature tour, infrastructure checklist, agent roster tutorial, or “here are your workstreams” walkthrough. A new Engineer learns Quantix by importing one Tender and answering the Manager's first real question. A returning Engineer resumes the exact Manager conversation and current action. Healthy startup checks remain invisible; only an actual blocker replaces the workspace.

### 5. Make reading comfortable before making information dense

Windows recommends one UI typeface, a consistent type ramp, and a clear hierarchy. Its reference Windows body style is 14 effective pixels with a 20-pixel line height, while caption text is 12/16; italic is excluded from its type system because it can reduce legibility, particularly for people with dyslexia. [Microsoft: Typography in Windows apps](https://learn.microsoft.com/en-us/windows/apps/design/signature-experiences/typography)

The GOV.UK layout guidance starts from a single column and limits ordinary reading width so lines generally stay under about 75 characters. Its current type scale begins at 16 pixels for body content and uses a consistent vertical rhythm to improve scanning. [GOV.UK: Layout](https://design-system.service.gov.uk/styles/layout/), [GOV.UK: Type scale](https://design-system.service.gov.uk/styles/type-scale/)

W3C recommends short blocks, whitespace, clear foreground/background separation, simple language, and distinct instructions. [W3C: Use clear and understandable content](https://www.w3.org/WAI/WCAG2/supplemental/objectives/o3-clear-content/)

**Quantix consequence:** The Manager conversation should use at least a comfortable 15–16 pixel body size with generous line height and a reading measure around 60–75 characters. Twelve pixels is metadata, not ordinary prose. Eight-to-ten-pixel activity copy, tiny all-caps labels, truncated rows, and dense multi-column cards are disallowed in the default workspace.

### 6. Keep navigation shallow and subordinate to the current task

Microsoft's `NavigationView` guidance recommends shallow hierarchy—two levels is its ideal for usability and comprehension—and offers a minimal overlay mode when content needs the space. It recommends top navigation when there are five or fewer equally important top-level destinations. [Microsoft: NavigationView](https://learn.microsoft.com/en-us/windows/apps/develop/ui/controls/navigationview)

Apple says each submenu adds interface complexity and hides its contents. [Apple HIG: Menus](https://developer.apple.com/design/human-interface-guidelines/menus)

**Quantix consequence:** Default navigation should contain no more than four plain-language destinations, with no nested workstream tree:

1. `Manager`
2. `Work`
3. `Files`
4. `Decisions`

The optional Tender Office room belongs behind `Work` or a contextual control in the Manager conversation. Internal module names, records/audit, recovery, permissions, and technical settings stay out of primary navigation unless the Engineer's current task requires them.

### 7. Show live agent work as calm awareness, not a second job

Microsoft says background work of only modest interest can be represented by text rather than a permanently visible progress control. A line of text may be more useful than a spinner because it explains what is happening. Its info badge pattern is intentionally non-intrusive: it attracts attention to an area and then lets the person return to their flow. [Microsoft: Progress controls](https://learn.microsoft.com/en-us/windows/apps/develop/ui/controls/progress-controls), [Microsoft: Info badge](https://learn.microsoft.com/en-us/windows/apps/develop/ui/controls/info-badge)

OpenAI's Codex app uses separate task threads organized by project and lets the user switch among parallel work, review results in the originating thread, and comment on changes. Its later mobile guidance identifies the important human check-ins as answering a question, reviewing a finding, changing direction, approving the next step, or adding an idea. [OpenAI: Introducing the Codex app](https://openai.com/index/introducing-the-codex-app/), [OpenAI: Work with Codex from anywhere](https://openai.com/index/work-with-codex-from-anywhere/)

Anthropic's official Cowork documentation says the agent creates a plan, can coordinate parallel workstreams, shows progress, accepts mid-task steering, and delivers previewable outputs. Its normal start is still just selecting Cowork, describing the desired result, reviewing the approach, and letting it run. [Anthropic: Get started with Claude Cowork](https://support.claude.com/en/articles/13345190-get-started-with-claude-cowork)

These vendor pages establish product interaction precedents, not independent usability evidence.

**Quantix consequence:** Default live visibility should be one quiet sentence such as `3 specialists are working · 1 question for you`. Selecting it opens an activity drawer with meaningful assignments, blockers, handoffs, and completed outputs. It must not show a permanent row of agent cards, running dots, elapsed times, tool calls, model facts, hashes, or raw event streams. The Manager summarizes and routes what matters; the Engineer does not supervise telemetry.

### 8. Use plain empty states and explicit next steps

Apple says an empty state should welcome and educate in context, but must provide a clear next step because a blank screen without an obvious action is daunting. Crucial guidance should not live only in temporary empty-state content. [Apple HIG: Writing](https://developer.apple.com/design/human-interface-guidelines/writing)

**Quantix consequence:** A workspace with no Tender shows a short sentence and one button:

> Start with the Tender package you received. Quantix will review it and your Tendering Manager will ask only what is needed.

Primary action: `Choose Tender package`  
Secondary link: `Open a recent Tender`

Do not show empty tables, zero-count cards, disabled workstream navigation, agent placeholders, setup status, or a catalogue of features.

### 9. Preserve accessibility while simplifying

WCAG 2.2 requires descriptive headings and labels, visible keyboard focus, and pointer targets of at least 24 by 24 CSS pixels except for defined exceptions. Status changes that do not receive focus need programmatic announcement under the status-message criterion. [W3C: WCAG 2.2](https://www.w3.org/TR/WCAG22/)

WAI's disclosure pattern requires a real button, keyboard activation, and an accurate `aria-expanded` state. [WAI-ARIA Authoring Practices: Disclosure pattern](https://www.w3.org/WAI/ARIA/apg/patterns/disclosure/)

Material Design 3 recommends applying interaction states consistently and using two visual indicators for state rather than relying on one signal. [Material Design 3: Interaction states](https://m3.material.io/foundations/interaction/states/overview)

**Quantix consequence:** Simplification must not produce mystery icons or hover-only actions. Every drawer has a labeled trigger, every selection uses text/icon plus color or shape, focus remains visible and unobscured, background updates announce only meaningful state changes, and the entire first workflow is keyboard operable.

## What Quantix must remove from the rejected prototype

| Remove from the default workspace | Why it must go | Where the capability belongs |
| --- | --- | --- |
| Persistent three-pane layout | Divides attention before a task requires comparison | Single Manager surface; contextual evidence drawer or focused review view |
| Permanent evidence inspector | Shows source metadata when no source is being reviewed | Open from `View source`; close returns focus to the trigger |
| Bottom agent activity strip | Creates continuous motion, repeated status, and another navigation layer | One calm work-status sentence; details in `Show work` drawer |
| Top-bar deadline, global search, attention count, theme control, and avatar competing together | Makes the header a dashboard instead of orientation | Tender name only by default; the few necessary utilities in one compact menu |
| Pinned four-metric Tender brief | Repeats status outside the current decision | Manager writes a two- or three-sentence returning summary |
| Separate Manager/Office tabs visible at all times | Forces a collaboration-model choice before the user has a reason | Manager is home; Office opens when the Manager convenes it or the Engineer chooses `Join team room` |
| Workstream tree with agent names and statuses | Exposes internal delegation before the Engineer needs to inspect it | `Work` contains a simple task list; agent identity appears inside a selected task |
| Persistent approval card plus blocking-question card plus conversation | Presents multiple priorities and actions simultaneously | Manager surfaces the single current question or decision in the conversation |
| Search shortcut and expert commands (`@ Agent`, `/ Action`) in the beginner composer | Teaches syntax and possibility space before need | Plain composer with attachment; advanced commands revealed through one `+` menu |
| Clause/document/record/notes inspector tabs | Exposes storage model instead of the review task | `View source`, `View proposed change`, and `Why this matters` as task-specific actions |
| Provenance timeline, host-policy events, hashes, identifiers, and runtime details | Converts trust into technical noise | `Technical details` disclosure inside an evidence or run record |
| Theme toggle in the primary task bar | Adds a preference decision to every screen | Follow the operating system; preference remains in Settings |
| Variant switcher and competing layout concepts | Makes the prototype itself part of the product | Delete after choosing one coherent direction |
| Tiny all-caps eyebrows, 8–10 px body copy, truncated status rows | Fails ordinary reading before any domain complexity begins | 15–16 px body copy, clear sentence-case headings, restrained metadata |
| Decorative “active” dots and pulsing presence throughout | Produces ambient urgency without a required human action | Motion only for a newly actionable or blocking state, honoring reduced motion |

The capability is not being deleted merely because its default presentation is removed. Quantix still needs exact evidence, agent responsibility, activity, provenance, approvals, and recovery. The rule is that each appears **at the moment it helps the Engineer complete the current task**.

## The first-time Tendering Engineer journey

### 1. Launch

- Show the same neutral background and Quantix mark used by the workspace for only as long as necessary.
- Run healthy store, runtime, provider, integrity, and recovery checks silently.
- If a genuine blocker exists, explain it in plain language with one recovery action; otherwise open the workspace immediately.

### 2. Empty workspace

The screen contains:

- `Quantix`
- heading: `Start your first Tender`
- one-sentence explanation of the outcome;
- primary button: `Choose Tender package`
- secondary link: `Open a recent Tender`, only when recent work exists.

Nothing else is required. No agent roster, lifecycle, settings, dashboard, sample data, feature tour, zero counters, or system status.

### 3. Quiet intake

After file selection, keep the Manager surface visible. Show one line near the conversation composer:

> Reviewing 18 files. You can keep working.

Do not display fabricated percentages. If a useful first result is ready, replace the line with it while safe background intake continues.

### 4. Manager orientation

The Manager says, in short prose:

> I found the submission deadline and reviewed the package structure. I need one answer before I prepare the work plan.

Then show one question. A `View source` link opens the relevant clause only if the Engineer wants it. The composer is already focused and labeled for the answer.

### 5. Plan review

After required answers, the Manager presents:

- the outcome in one sentence;
- three to five work areas in a simple ordered list;
- the deadline or most material risk;
- what Quantix will do automatically;
- what still requires the Engineer;
- primary button: `Approve and proceed`;
- secondary link: `Review full plan`;
- secondary button or link: `Request changes`.

The full dependency graph, budgets, permissions, reviewer separation, and technical scope remain available in the focused plan review, not in the conversational summary.

### 6. Work in progress

The conversation stays calm. Under the tender name, show a non-animated line such as:

> 3 specialists working · Requirements review needs you

Selecting the sentence opens a drawer grouped into only:

- `Needs you`
- `Working`
- `Done`

Completed items are visually quiet. New blockers or approval requests receive emphasis. GOV.UK similarly recommends keeping the number of task statuses as small as possible and visually quieting completed items so attention stays on required work. [GOV.UK: Complete multiple tasks](https://design-system.service.gov.uk/patterns/complete-multiple-tasks/)

### 7. Contextual review

Selecting `View source`, a deliverable, or a decision opens a temporary side sheet or full review view. It contains only the information required for that review and one clear action. Closing it restores the Manager conversation exactly where the Engineer left it.

## Proposed default screen

```text
┌────────────────────────────────────────────────────────────────────┐
│ Quantix                         North Coast Medical Campus        ⋯ │
├────────────────────────────────────────────────────────────────────┤
│                                                                    │
│  Tendering Manager                                                 │
│                                                                    │
│  I reviewed the Tender package. Before I prepare the work plan,    │
│  I need to confirm who carries the six-year latent-defect          │
│  insurance.                                                        │
│                                                                    │
│  View source                                                       │
│                                                                    │
│  ┌──────────────────────────────────────────────────────────────┐  │
│  │ Type your answer…                                            │  │
│  └──────────────────────────────────────────────────────────────┘  │
│                                                     Answer Manager │
│                                                                    │
│  3 specialists working · 1 question for you        Show work       │
│                                                                    │
└────────────────────────────────────────────────────────────────────┘
```

This sketch intentionally omits a sidebar in the first-use state. After the first plan exists, a slim navigation rail can appear with `Manager`, `Work`, `Files`, and `Decisions`. It should collapse at narrower widths and must not reduce the conversation below a readable measure.

## Visual and interaction rules for the next prototype

1. Use a light, quiet neutral canvas with one restrained accent color. Reserve amber/red for actionable risk or failure.
2. One visual focal point per state. If the Manager asks a question, no other card may look equally urgent.
3. One persistent main column. Secondary material is hidden until invoked.
4. Body text is 15–16 px minimum in the conversational workspace; metadata is at least 12 px and never carries the main meaning.
5. Keep prose to roughly 60–75 characters per line.
6. Use sentence case. Eliminate technical eyebrows and unexplained acronyms.
7. Prefer verbs that describe the Engineer's outcome: `Answer Manager`, `Review work plan`, `Approve and proceed`, `View source`.
8. Use no more than four primary navigation destinations and no more than two hierarchy levels.
9. No animated agent avatars, pulsing status field, decorative activity stream, dashboard cards, or default technical trace.
10. No icon-only control unless the platform convention is unmistakable and an accessible name is supplied; beginner-critical actions use text labels.
11. Let the Engineer dismiss every contextual panel and return focus/context correctly.
12. Follow the operating-system theme by default; keep visual preferences out of the task flow.

## Prototype acceptance questions

The next concept succeeds only if a first-time Tendering Engineer can answer these without coaching:

- What does Quantix want me to do right now?
- Who am I talking to?
- What happens after I answer?
- Is work happening, and does any of it need me?
- How do I see the source without losing my place?
- Which action is the formal approval?
- How do I return or correct something?

Observe time to first correct action, wrong turns, whether the Engineer notices the one work-status line, whether they can open and close evidence, and whether they confuse a conversation reply with formal approval. Also ask them to describe the screen in their own words. If they describe “panels,” “agent cards,” “statuses,” or “a dashboard” before describing the Tender problem and current question, the design is still exposing the system instead of supporting the work.

## Bottom line

Quantix should feel like a capable Tendering Manager waiting at a clear desk—not a control room asking the Engineer to monitor every instrument. Agent activity, evidence, provenance, tasks, and approvals remain available and trustworthy, but they earn screen space only when the Engineer needs them.
