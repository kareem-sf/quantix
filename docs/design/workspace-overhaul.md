# Workspace overhaul: chat work log, right panel, left sidebar

Status: proposed, 2026-09-14. Every decision below comes from interviews with the product owner. This document replaces `work-log.md`.

**Who it's for.** A construction engineer running a tender, not a developer. Every screen shows real, captured work in plain native English: no invented progress, no backend jargon, no provider or model details.

**What it covers:**

1. [Chat work log](#1-chat-work-log): what the Tender Manager is doing, live.
2. [Right panel](#2-right-panel): Plan, Team, Documents, Activity.
3. [Left sidebar and pages](#3-left-sidebar-and-pages): tenders, Documents, Estimate, Submission.
4. [Data changes](#4-data-changes), [Removed](#5-removed), [Build order](#6-build-order).

---

## 1. Chat work log

### Problem

- **"Tender Manager is planning" and "Deciding the next step" are placeholders.** They appear whenever the app waits for the AI. No planning is captured.
- **Step titles are fixed labels per tool.** "Checking approved company knowledge" appears whatever the tool returned.
- **The AI's words are thrown away.** DeepSeek sends `reasoning_content`, but the `openai_chat` route drops it. The run on 2026-09-14 made 37 tool calls with no text.
- **The same run appears twice:** once in the planning list, and again in a "Work in progress / Work activity" box whose history drawer is full of backend events.

### Decisions

| Topic | Decision |
|---|---|
| Words | All three layers: the AI's short note, its raw thinking, and facts from what each tool actually did. |
| Stacking | Thinking streams live. When the AI acts, the thinking folds into "Thought for 12s", the note becomes the block's main text, and the facts sit under it. |
| Note voice | First person, like a colleague. One or two sentences in plain native English, even for Arabic documents. |
| Housekeeping steps | Each one gets its own line, showing its actual result. |
| Expand / collapse | The current block is open and older blocks are collapsed. Any block can be opened or closed at any time. |
| Live state | Stream whatever is happening. Never show invented text. |
| Expanded fact | What was found, plus "Open page". The exact search terms and locators sit behind a small "Details" toggle. |
| Staff work | Nested under the handoff, in the same style. |
| Finished | The log folds into one summary line above the answer. It can be reopened. |
| Stopped early | A plain reason, what was kept, what was not done (when known), and one action button. |
| Timings | The total time and the time per step are kept. |

### Layout

```
You: review the package

▾ Working · 7m 12s                                   [Stop]
  ▸ I'll get up to speed on the package first.       0:15
  ▸ The systems schedule and specs set the scope,     2:40
    so I'm reading those in full.
  ▾ I'll check the visit schedule for the date,       0:42
    then the clarifications in case it moved.
    ▸ Thought for 12s
    ├ Read 8-موعد الزيارة الميدانية 1.pdf · pages 1–2          0.3s
    │   Visit on 21 Sep at 10:00, KSAU-HS site office.
    │   Open page 1 · Details
    └ Searched the clarifications · nothing found        0.2s
  ● Thinking…
    The schedule says 21 Sep but the header mentions a
    second session, so I should confirm which one app█
```

### Blocks

A **block** is one AI turn.

- **Heading:** the AI's note. If there's no note, the heading is built from the facts ("Read 2 documents and searched once"). It's never an invented intention.
- **Thinking:** streams under the heading, then folds to "Thought for Ns". It's only shown when the provider sent it.
- **Facts:** one line per tool call, in order, each with its own time.

### Live streaming

Things stream in the order they happen: thinking tokens, then note tokens, then each fact. A fact appears as its tool starts ("Reading Systems.pdf…") and changes to its result when the tool finishes.

If the provider streams nothing yet, the live line says **"Waiting for the Tender Manager's reply · 0:12"**.

### Fact lines

Each tool gets a backend formatter that turns its real arguments and result into one line plus an expanded view.

| Tool | Line | Expanded |
|---|---|---|
| `read_whole_document` | Read **{file}** · pages {a–b} | Key lines found · Open page |
| `read_source` | Read a passage in **{file}** · page {n} | The passage · Open page |
| `search_sources` | Searched the documents · {n} matches in {m} files (or "nothing found") | Matching files and pages · Details: the search terms |
| `view_document_page` | Looked at **{file}** · page {n} | Open page (region highlighted) |
| `read_package_map` | Checked the package overview · {project}, {n} groups | Groups |
| `list_documents` | Checked the document list · {n} documents | File names |
| `inspect_extraction_coverage` | Checked which pages could be read · {n} read, {k} unreadable | Unreadable files |
| `inspect_estimate` | Checked the estimate · {n} items, {k} priced (or "no estimate yet") | Items |
| `inspect_submission_requirements` | Checked submission requirements · {n} saved (or "none saved yet") | Requirements |
| `inspect_project_map` | Checked the project breakdown · {n} items | Items |
| `list_work_products` | Checked earlier drafts · {names} (or "none yet") | Drafts with versions |
| `read_work_product` | Read the earlier draft **{name}** v{n} | Rows read |
| `list_reusable_notes` | Checked company guidance · {n} notes (or "none saved yet") | Note titles |
| `list_team` | Checked the team · {names} (or "only the Tender Manager") | Roles |
| `hire_staff` | Hired **{name}**, {role} | Profile |
| `assign_work` | Asked **{name}** to {task} | Nested staff log |
| `save_work_product` | Saved draft **{name}** v{n} | Rows saved |
| `save_work_brief` | Updated the plan · {what changed} | Plan items changed |
| `calculate_engineering` | Calculated {what} · {result} | Inputs and working |
| `propose` | Prepared {n} {record kind} for your review | The records |
| `proposal_format` | *(not shown: internal format lookup)* | |

A failed call reads as a plain fact: "Tried to read {file}, but pages 3–9 couldn't be read." The AI's corrected retry of a rejected call merges into the same line.

### Staff work

`assign_work` becomes its own block: "Asked Layla Haddad (Specifications Engineer) to list the specified systems". Expanding it shows Layla's blocks, indented, ending with "Layla's result: …" and her time.

### When the job ends

- **Finished:** the log folds into "Worked for 7m 12s · read 23 documents · searched 6 times · asked Layla Haddad for the systems list". The counts come from the facts.
- **Stopped early:**

  ```
  ■ Stopped: the spending limit for this tender was reached.
    Kept: 23 documents read, Layla's systems list.
    Not done: the site-visit terms and the review.
    [Raise limit]  [Continue]
  ```

  "Not done" comes from the live plan's unfinished items. It is omitted if there is no plan.

  | Cause | Reason shown |
  |---|---|
  | Spending limit | "the spending limit for this tender was reached" |
  | Provider cut off its reply | "the AI service dropped its reply" |
  | You pressed Stop | "you stopped it" |
  | App restarted | "Quantix restarted" |

---

## 2. Right panel

### Problem

- **The panel opens to an empty launcher screen.**
- **Team** shows large profile cards with skill chips, then an "All work" list of big cards that are all Cancelled.
- **Reviews** is mostly empty-state text ("has not proposed a work plan yet", "No findings…").
- **Activity** has one card per attempt at the same request, six for "review the package". Each has its own Resume button and raw error text in red.
- **Documents** has cut-off names and "Version 1 · Current · Read" on every row.

### Structure

The panel has four tabs and opens straight to **Plan**. There is no launcher screen.

```
[Plan]  Team  Documents  Activity                    ⤢  ✕
```

Reviews merges into Plan as "Waiting for you". The existing hide, expand and resize behaviour is kept.

### Plan

The Tender Manager keeps a live plan and the engineer approves it.

- **Plan first.** For any real engineering job, the Tender Manager writes the plan before starting work. Questions about the tender, like "who are you?", don't get a plan.
- **Live updates.** It updates each item as work moves: owner, state and what was found.
- **Approval.** You approve the plan, or ask for changes in the chat. The panel has no inline editing.
- **Plan changes.** A change to an approved plan shows what changed and waits for approval again.

```
Plan · review the package · approved 3:47 PM

✓ Map the package                         Tender Manager
  20 documents in 6 groups · 1 unreadable
● Confirm site-visit terms                 Layla Haddad
  Found 21 Sep 10:00; checking clarifications
○ Take off BOQ quantities                  Khalid Bin Salem
○ Contract risk review                     Noura Al-Qahtani

Waiting for you (3)
• Approve the plan
• Visit date: the schedule says 21 Sep, clarification 02 says 23 Sep
• Question from Khalid: use BOQ units or drawing units?
```

**Waiting for you** collects everything that is genuinely waiting on the engineer and blocking or pending work:

- plan approvals and plan changes
- findings and conflicts that need a decision
- proposed quantities, rates and estimate lines
- submission requirements to approve
- questions from the Tender Manager or staff

Each item opens the place where you decide it. Nothing that is merely informational is listed.

### Team

The Team tab answers one question: who's doing what now.

```
Team

Tender Manager                            working
  Checking the site-visit terms

Layla Haddad · Specifications Engineer
  ✓ Listed 9 specified systems · 3m

Khalid Bin Salem · Quantity Surveyor
  Idle · last job stopped

Past work (7) ▸
```

- **Rows.** One row per person: name, role, and their current action in plain English (from the live work log), or their last result.
- **Profile.** Skills and profile open on click.
- **Past work.** Cancelled and finished assignments sit behind "Past work".

### Documents

Documents are grouped by discipline. **The AI chooses the groups from the actual content; they are not hard-coded.**

```
Documents · 20

▾ Drawings (7)
  FIRE STATION-ELEC.pdf
  Electrical layouts and schedules · 14 pp
  FIRE STATION-MECH-1.pdf
  Mechanical and plumbing layouts · 9 pp
▾ Bill of quantities (1)
  1-جدول الكميات.pdf
  Priced BOQ schedule (Arabic) · 17 pp
▸ Site visit (2)
▸ Clarifications (2)
⚠ 1 file couldn't be read
```

- **When grouping happens.** Package analysis assigns each file to a group and writes a short English description. It runs when a package is imported and again when files change.
- **Your changes win.** You can move a file to another group, and later analysis keeps your change.
- **Row contents.** The original file name, then the English description and page count.
- **Problems only.** Rows show warnings only for problems (unreadable pages, a newer revision). "Version 1 · Current · Read" is removed.
- **Opening a file** opens the existing inline viewer.

### Activity

Activity lists every job on the tender, newest first, grouped by day. Resumes of the same request are grouped as one job.

```
Today
■ review the package                    stopped · 31m
  The AI service dropped its reply.
  Read 23 documents · hired 7 staff
  [Continue]                     6 attempts ▸

✓ Who are you?                          done · 10s

Yesterday
✓ Analysing tender package              done · 4m
```

- **Row contents.** The request, the latest status, the total time across attempts, one plain reason, the summary line, and one Continue button on the latest attempt.
- **Grouping.** Attempts are linked through the existing `resumed_from` events. Earlier attempts sit inside the job.
- **Opening a job** shows its full work log in the chat style.
- **Technical log.** A "Technical log" section, collapsed by default, is the only place raw events, the provider, the model and request IDs appear.

---

## 3. Left sidebar and pages

### Sidebar

The sidebar follows the tender's stages.

```
Quantix
+ New tender

TENDERS
▾ New Fire Station · KSAU-HS      due 21 Oct · 3
    ● Manager                     working
      Documents                   20 · 1 issue
      Estimate                    not started
      Submission                  3 of 11 ready
  Warehouse Riyadh                due 3 Oct

⚙ Settings
```

- **Tender row:** a short English name (full title on hover), the due date (highlighted when close), a live working dot (also shown when you're viewing another tender), and a count of items waiting for you.
- **Under each tender:** Manager, Documents, Estimate and Submission, each with its live state on the right.
- **Work is removed.** Its plan, decisions and activity live in the right panel.
- **AI and spending** moves to Settings.

### Page text

Each page has at most one short plain-English line under its title. Rules are enforced by the app, not explained in paragraphs. Empty states say what to do next in one sentence, with one action.

```
Submission
What the tender asks you to submit.

No requirements yet.
[Ask the Tender Manager to find them]
```

### Documents page

The page uses the same AI groups and English descriptions as the right panel, full width, with the viewer beside the list.

```
Documents · 20 · 1 issue                      [Search all text]

▾ Drawings (7)            │  FIRE STATION-ELEC.pdf
  ELEC    Electrical 14pp │  ┌───────────────────┐
  MECH-1  Mechanical 9pp  │  │      page 3       │
▾ BOQ (1)                 │  └───────────────────┘
  جدول الكميات    17pp     │  ◀ 3 / 14 ▶
▸ Site visit (2)          │
```

- **Search** covers all extracted text.
- **Filters, the reading map and revisions** collapse under "More". They are kept, not deleted.
- **Viewer tools** stay: versions, pages, sheets, zoom, download and measurement.

### Estimate page

One BOQ table replaces the BOQ, Takeoff, Proposals and Supplier quotations tabs.

```
Estimate · BOQ                         SAR 0 · 0 of 214 priced

Item  Description          Qty        Rate    Status
3.1   Excavation           1,240 m³   —       ● proposed
      from Drawing C-02 p.3                   [Accept] [Reject]
3.2   Blinding concrete    86 m³      —       ○ not started
28.4  Fire alarm panel     1 no       quote   ● waiting
      Edwards EST4 · 2 quotes requested
```

- **Each row** shows quantity (with its takeoff source), rate (with its source or quote), amount and status.
- **AI proposals** appear inline as rows waiting for your OK.
- **As built (2026-09-15):** the app joins `/estimate`, `/takeoff` and `/estimate/rate-proposals` in `src/features/estimate/model.ts`; no new backend query was needed.
  - Status is one of Waiting for you, Check the source, Not priced or Priced.
  - A single drawing quantity waiting for a decision gets Accept and Reject in the row. Rate and quantity proposals open the row review, because approving them needs a note and confirmation.
  - Drawing lines with no BOQ row are listed under the table as "On the drawings, not in the BOQ".
  - Supplier quotes are request emails that are not linked to BOQ rows, so they open in a side panel from the header, not from the rate cell.
  - Old `?view=proposals|takeoff|quotes` links still work.
- **Supplier quotes** open from the rate cell.
- **Takeoff details** open from the quantity cell.

### Submission page

One checklist replaces the Requirements, Documents and Package tabs.

```
Submission · 3 of 11 ready

✓ Bid bond (1%)                Guarantee.pdf
  from Conditions §4.2
● Site visit certificate       drafted · review
  from نموذج الزيارة p.1
○ Priced BOQ                   missing
○ Technical schedule           missing

[Build submission package]      (8 missing)
```

- **Each item** shows its source clause, the file that satisfies it, and its status: missing, drafted or approved.
- **Build submission package** is enabled only when every item is approved or has a recorded exception.

---

## Components

Beautiful UI (beautifului.dev, MIT) primitives are used instead of custom builds wherever one fits. The owner supplies the source. Sample data and invented signals, such as confidence meters, are never carried over: every value shown comes from real Quantix records.

| Beautiful UI component | Used for |
|---|---|
| 02 Thinking | Chat work log blocks: streamed thinking that folds to "Thought for Ns", steps, searches |
| 05 Tool Chips | Fact lines under each block, and the finished summary line ("23 reads, 6 searches") |
| 01 Loading State | Live "Waiting for the Tender Manager's reply · 0:12" line |
| 03 Streaming Text | The Manager's answer, with inline document citations and follow-ups |
| 10 Context Cards | An expanded fact's "what it found": the passage with its file and page |
| 06 Task Rows | Plan items (owner, state, what was found), Team "now" rows, staff blocks |
| 04 Approval Card | "Waiting for you": questions from the Tender Manager or staff, with options |
| 09 Recommendation Card | Plan approval and plan changes: Accept, or ask for changes |
| 11 Diff Table | Plan revisions, and proposed quantities and rates to accept or reject |
| 13 Filter Table | Activity jobs and the Submission checklist, filtered by status |
| 12 Records Table | The Estimate BOQ table |
| 14 Sidebar Nav | Left sidebar: tenders and stages |
| 15 Search | Document search and quick jump |
| 08 Prompt Bar | Composer: @ to cite a document, / for common tender tasks |

Not used: 07 Chat (the Manager chat already exists), 16 Flowchart, 17 Insight Cards, 18 Code Block, 19 Fine-tune Card, 20 Selection Actions, 21 Agent Screen.

### Received source

The owner supplied the original source, saved unmodified in `docs/design/beautiful-ui/` as reference only. That folder sits outside `src`, so it isn't compiled or linted. Adapted components will live in `src/components/beautiful/`.

| File | Size | Adapted file |
|---|---|---|
| 01-loading-state.tsx | 4.9 KB | `live-status.tsx` |
| 02-thinking.tsx | 11.4 KB | `work-block.tsx` |
| 03-streaming-text.tsx | 10.3 KB | `answer-stream.tsx` |
| 04-approval-card.tsx | 16.5 KB | `question-card.tsx` |
| 05-tool-chips.tsx | 14.0 KB | `fact-list.tsx` |
| 06-task-rows.tsx | 10.0 KB | `task-rows.tsx` |
| 08-prompt-bar.tsx | 30.5 KB | `composer.tsx` |
| 09-recommendation-card.tsx | 5.8 KB | `approval-card.tsx` |
| 10-context-cards.tsx | 4.2 KB | `passage-cards.tsx` |
| 11-diff-table.tsx | 11.4 KB | `change-table.tsx` |
| 12-records-table.tsx | 59.5 KB | `boq-table.tsx` |
| 13-filter-table.tsx | 6.0 KB | `status-table.tsx` |
| 14-sidebar-nav.tsx | 18.3 KB | `app-sidebar.tsx` |
| 15-search.tsx | 4.4 KB | `document-search.tsx` |

### Adaptation rules

1. **Real state only.** The demos animate on fixed timers: `useSequence(STAGES)`, `useTick(TICKS)`, `STEP_MS`, the autoplay in Prompt Bar, and the staged reveal in Diff Table. That is exactly the invented progress this overhaul removes. Every timer is deleted; each component becomes controlled by props fed from live run events and records. Entrance animations still play when real data arrives.
2. **No sample data.** Delete every default `ROWS`, `CHUNKS`, `QUESTIONS`, `OPTIONS`, `SOURCES`, `ITEMS` and label constant; the props become required.
3. **No invented signals.** Remove the Recommendation Card confidence meter ("High confidence", signal bars), the Records Table "Connection strength" average, and "Show confidence".
4. **No provider details.** Remove the Prompt Bar model picker, brand logos (Figma, Slack, Gmail), "Connect" rows and the rainbow `glimm` sweep. The @ menu lists tender documents; the / menu lists real Tender Manager tasks.
5. **Nothing that does nothing.** Drop or wire up the decorative controls: Streaming Text's copy/retry/thumbs icons all read `aria-label="Action"`, and Records Table's "Add calculation", "Go calculate" and AI property popover do nothing real. Diff chips in Tool Chips become document chips (file and pages read), not code diffs.
6. **Quantix tokens.** A token bridge in `src/index.css` maps the Beautiful UI names onto the existing shadcn theme, so light and dark both work. There is no second palette.

   | Beautiful UI | Quantix theme |
   |---|---|
   | `ink`, `ink-2`, `ink-3` | `foreground`, a mid-tone, `muted-foreground` |
   | `surface`, `canvas`, `inset`, `field` | `card`, `background`, `muted` |
   | `line`, `line-strong` | `border`, `input` |
   | `hover`, `hover-2` | `accent` at two strengths |
   | `green`, `red`, `orange`, `*-tint` | the existing success, destructive and warning colours |
   | `rounded-control`, `-chip`, `-card` | `radius-md`, `radius-sm`, `radius-lg` |

7. **Icons.** Replace `@central-icons-react/*` with `lucide-react`, which is already installed. Central Icons is a separately licensed set.
8. **Buttons.** Use the existing shadcn `Button`, mapping the variants `accent`, `primary`, `secondary`, `ghost` and `success`.
9. **Motion.** Keep durations and easings (they match `--expo-out`), and honour `prefers-reduced-motion` in every component, not only Approval Card.

### Missing pieces

The pasted code imports or relies on things that weren't included:

| Missing | Used by | Plan |
|---|---|---|
| `@/components/primitives/GlideMenu` | Approval Card, Sidebar Nav, Records Table, Search | Needs the source, or a small rewrite: one highlight that glides to the hovered `[data-menu-row]` |
| `@/components/atoms/Button`, `EntityChip`, `ValuePill` | Approval Card, Recommendation Card, Diff Table | Replace with shadcn `Button` and `Badge` |
| Global CSS: tokens, keyframes (`fade-in`, `fade-up`, `pop-in`, `shimmer-text`, `pixel-on`, `eq-bounce`) | All | Needs the source, or recreate in `index.css` |
| `primitive-card-bar/pad/footer`, `primitive-table-cell`, `primitive-icon-button` | Cards and tables | Needs the source, or recreate |
| `records-*` classes, about 45 of them | Records Table | Needs the source; the table is unstyled without it |
| `sidebar-*` classes | Sidebar Nav | Needs the source; collapse behaviour depends on them |
| `filter-status-*` classes | Filter Table | Recreate from the status colours |
| `glimm` package | Prompt Bar | Not needed (rule 4) |

## 4. Data changes

| Need | Today | Change |
|---|---|---|
| Thinking | Dropped on `openai_chat` | Set the Pydantic AI profile `openai_chat_thinking_field` for connections that return reasoning. Stream `ThinkingPart` deltas as activity events. |
| Notes | None | Add an instruction to write one or two first-person sentences before each set of actions. Capture `TextPart` alongside tool calls as a `note` event. |
| Facts | Raw payloads only | A per-tool formatter stores `{line, found, open, details}` on each tool's completion event. |
| Live plan | `WorkBrief` steps (title, state, note), saved only when the AI chooses | Add `owner` to `BriefStep`. Require a plan before real engineering work. Update steps as work moves. Add approval of a plan revision. |
| Waiting for you | Spread across Reviews, Estimate, Submission and staff questions | One backend query that collects pending plan approvals, findings, proposals, requirements and open questions owned by the engineer. |
| Document groups | Fixed "Area" field | Package analysis writes `group` and `description_en` per document. An engineer override is stored separately and wins. |
| Short English name and due date | Long Arabic title; deadline not surfaced | Package analysis proposes a short English name. The due date comes from the tender calendar's submission deadline. |
| Job grouping | One run per attempt | Group runs by their `resumed_from` chain. |
| Team "now" | Assignment cards | Derived from each person's latest work log block. |
| Estimate table | Four separate views | One query joining BOQ rows, takeoff lines, rate proposals and quotes. |
| Submission checklist | Three tabs | Requirements joined to their satisfying documents and statuses. |

**Cost:** the notes add roughly 30–60 output tokens per AI turn. Thinking tokens are already generated and billed; they're just discarded today. Document grouping adds one short pass per package analysis.

## 5. Removed

- **Chat placeholders:** "Tender Manager is planning", "Deciding the next step", "Working out what this request needs".
- **Chat activity box:** "Work in progress / Work activity", "Inspect activity" and "View chronological history" in the chat.
- **Backend wording:** "Open code and native work history", "Content supplied by Quantix", "Using an approved AI connection", thinking-stream notices.
- **Provider details:** names, models, costs, tokens and request counts in the Manager workspace. Spending lives in Settings.
- **Right panel:** the launcher screen and the separate Reviews tab.
- **Left sidebar:** the Work page. Its content moves to the right panel, and AI and spending moves to Settings.
- **Tab sets:** Estimate's four tabs and Submission's three tabs.
- **Page text:** long caution paragraphs on every page.

## 6. Build order

Each step ships working and tested before the next starts.

1. **Component foundation:** the token bridge and keyframes in `index.css`, GlideMenu, and the 14 adapted components in `src/components/beautiful/`, each controlled, timer-free, with tests and checked in light and dark.
2. **Backend capture:** thinking, notes, fact formatters, and staff grouping of activity.
3. **Chat work log:** replaces `ManagerPlanning` and the inline `LiveRunStream`.
4. **Live plan:** the owner field, plan-first instruction, revision approval, and the right panel **Plan** tab with **Waiting for you**.
5. **Right panel:** **Team**, **Activity** (job grouping) and **Documents** (AI groups, descriptions, overrides).
6. **Left sidebar:** tender rows (short name, due date, working dot, waiting count), stage states, removal of Work, AI and spending moved to Settings.
7. **Pages:** Documents (groups and viewer), Estimate (one table), Submission (one checklist), page text trimmed.
8. **Cleanup and live check:** delete the old components and labels, then run one real job in the desktop app within the test budget.

## Not included

- English translation of Arabic passages.
- Costs inside the work log.
- Inline editing of the plan in the panel.
