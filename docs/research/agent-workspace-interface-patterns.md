# Agent workspace interface patterns

Status: initial comparative research for Quantix product discovery  
Evidence snapshot: 2026-08-14  
Primary user: a Tendering Engineer operating a single-user local desktop workspace  
Source policy: first-party product documentation and canonical GitHub repositories only

## How to read this report

This report separates two kinds of statements:

- **Evidence fact** — directly supported by a first-party page or canonical repository linked next to the claim.
- **Design inference** — a proposed consequence for Quantix. It is not established user research and should be challenged in the product interview and a runnable prototype.

The research compares product interaction models, not benchmark scores or marketing reach. Repository stars are deliberately omitted because they do not establish usability. Availability and identity were checked on the evidence-snapshot date; fast-moving beta products may change.

## Executive conclusion

**Evidence fact:** The strongest current agent workspaces converge on a small set of structural ideas:

1. Start from a **project, folder, repository, or durable workspace**, not from a global control panel.
2. Treat each delegated outcome as a **persistent task/session** with a visible state.
3. Stream an intelligible **work trace** — messages, tool calls, file changes, approvals, blockers, and outputs — while keeping raw diagnostics subordinate.
4. Permit the user to **steer, pause, stop, approve, reject, or resume** without losing the task's history.
5. Make files and other deliverables **first-class artifacts**, previewable beside the conversation that produced them.
6. Let multiple tasks run concurrently, while presenting a compact **attention queue** rather than forcing the user to watch every token.
7. Preserve recovery through durable history, checkpoints, event logs, branches/worktrees, or replayable state.

Claude Cowork is the clearest benchmark for outcome delegation and background work; Agent Zero for watching the agent operate real browser/desktop/file surfaces; Block Buzz for agents as first-class participants in shared rooms and one searchable activity/audit substrate; GitHub Copilot app and Cline Kanban for concurrent isolated workstreams; and OpenHands Agent Canvas for a control center spanning multiple agent backends.

**Design inference:** Quantix should become a **Tender Workspace with an observable Agent Office**, not a tender-domain form catalogue and not a generic chat clone. A useful working name for its primary surface is the **Tender Workroom**. Conversation and activity are the collaboration surface. They are not the Tender system of record. Requirements, decisions, approved estimates, queries, baselines, packages, evidence bindings, and approvals remain authoritative domain records.

The recommended shell is:

- a brief launch state that runs healthy checks silently and explains only blocked or degraded capabilities;
- direct return to the last Tender Workspace, or a small recent-Tenders home when no Tender is active;
- a persistent workspace sidebar for Tender areas and conversations/tasks;
- a primary work surface that can show conversation plus live agent activity;
- a contextual artifact/evidence inspector;
- a compact cross-workspace activity and attention center for running, waiting, failed, and completed work;
- explicit, local approval prompts tied to the exact proposed action or record version.

### Quantix baseline and constraints

**Evidence fact:** The current renderer makes Application Home setup, runtime readiness, and update status the opening experience, then mounts the Tender workspace ([`App`](../../src/App.tsx)). An open Tender renders nearly every domain subsystem in one long surface; the existing Agent Run panel already exposes profile identity, state, interrupt, recovery, permissions, output, and normalized events, but only as one subsystem ([`TenderWorkspace`](../../src/TenderWorkspace.tsx), [`AgentRunOffice`](../../src/AgentRunOffice.tsx)). The earlier research therefore recommended an exception-led Control Room, focused workbenches, one decision inbox, and a persistent Run Center ([Engineer tender workflow and UX direction](./engineer-tender-workflow-ui-ux.md)).

**Design inference:** The new workspace direction changes the center of gravity rather than discarding that work. The Tender Workroom becomes the place to delegate, observe, steer, inspect, and review; the Control Room becomes its `Now` and attention layer.

**Evidence fact:** Quantix also imposes stronger controls than the products reviewed here: the Engineer alone formally approves ([ADR 0001](../adr/0001-control-the-tender-lifecycle-with-eitl-gates.md)); chat and Agent Runs are noncanonical traces ([ADR 0002](../adr/0002-keep-chats-outside-the-tender-system-of-record.md)); calculations remain deterministic ([ADR 0003](../adr/0003-make-deterministic-calculations-canonical.md)); each Agent Profile has an isolated thread/workspace and inter-role handoff occurs only through registered output ([ADR 0004](../adr/0004-run-agent-profiles-through-host-controlled-codex-threads.md)); independent roles and exact task ownership are enforced ([ADR 0005](../adr/0005-compose-tender-teams-through-controlled-capability-demands.md)); and per-run access is minimum-disclosure and Host-controlled ([ADR 0006](../adr/0006-enforce-agent-access-through-host-owned-run-grants.md)). Local v0 runs no hidden background Tender Office; shutdown interrupts active work and quarantines partial output ([ADR 0008](../adr/0008-keep-codex-behind-a-quantix-owned-ai-provider-contract.md)).

**Design inference:** "See agents chatting" should therefore mean a truthful conversation-shaped timeline of actual assignments, questions, answers, registered handoffs, reviews, findings, and recovery events. Quantix must not fabricate agent banter or imply profiles share chat history. "Background" means continuing while the Engineer navigates elsewhere in the open app; it cannot promise cloud-style work after exit in local v0.

## 1. Identity and availability audit

### Claude Cowork, not “Claude Work”

**Evidence fact:** The comparable Anthropic product is **Claude Cowork**. Anthropic describes Cowork as bringing Claude Code's agentic architecture to multi-step knowledge work. Chat and Cowork share one home; the user selects Cowork in the composer, reviews the approach, watches progress, can steer during execution, and receives finished files. Cowork can coordinate parallel subagents and, for remote sessions, continue after the laptop closes. It is currently available on paid Pro, Max, Team, and Enterprise plans; desktop supports macOS and Windows, with Linux beta documented separately, while web/mobile remote Cowork are beta and rolling out. Local file, browser, and computer access still require a connected desktop client ([Cowork getting started](https://support.claude.com/en/articles/13345190-get-started-with-claude-cowork), [cross-surface behavior](https://support.claude.com/en/articles/15520349-use-claude-cowork-on-web-desktop-and-mobile), [desktop availability](https://support.claude.com/en/articles/10065433-install-claude-desktop)).

**Evidence fact:** Cowork first appeared as a research preview in January 2026. “Claude for Work” is instead Anthropic's organizational plan/administration umbrella, not the agent-workspace mode ([Anthropic Labs announcement](https://www.anthropic.com/news/introducing-anthropic-labs), [Claude for Work collection](https://support.anthropic.com/en/collections/9387370-claude-for-work-team-enterprise-plans)).

### Agent Zero

**Evidence fact:** The canonical project is [`agent0ai/agent-zero`](https://github.com/agent0ai/agent-zero), not similarly named mirrors. It is an active MIT-licensed agent framework/workbench that packages a Linux environment, Web UI, projects, memory, files, browser and desktop canvases, tasks, plugins, and hierarchical delegation. The Web UI is explicitly described as a place to watch and steer work; its composer exposes attachments, pause, nudge, compaction, context, and history. Projects scope workspace files, memory, secrets, and instructions ([official documentation](https://www.agent-zero.ai/p/docs/), [canonical usage guide](https://github.com/agent0ai/agent-zero/blob/main/docs/guides/usage.md), [subagent guide](https://www.agent-zero.ai/p/docs/subagents/)).

**Evidence fact:** Agent Zero v2.9 was released on August 12, 2026, and its separate Launcher provides packaged setup for Windows, macOS, and Linux ([v2.9 release](https://github.com/agent0ai/agent-zero/releases/tag/v2.9), [Launcher releases](https://github.com/agent0ai/a0-launcher/releases)).

### Buzz

**Evidence fact:** The high-confidence match for the user's “Buzz” is Block's canonical [`block/buzz`](https://github.com/block/buzz). It is a self-hostable human-agent workspace built around communities, rooms/channels, threads, canvases, workflows, Git events, and a signed event log. Humans and agents use the same identity and participation model; agents have their own keys, memberships, messages, activity, and audit trail. No other canonical exact-name GitHub result matched the user's agent-workspace description.

**Evidence fact:** Buzz distinguishes shipped behavior from aspiration. Its README says channels, threads, DMs, canvases, search, audit log, Tauri desktop app, agent CLI/ACP harness, YAML workflow triggers, and Git events work today. Workflow approval glue and mobile clients are still being wired, the Windows package is unsigned alpha, and the project explicitly says it is not finished. The current desktop release at the snapshot is v0.5.11, released August 12, 2026 ([canonical README and maturity table](https://github.com/block/buzz), [v0.5.11 release](https://github.com/block/buzz/releases/tag/desktop-v0.5.11)).

**Design inference:** Buzz is strong conceptual evidence for “agents are colleagues in the room,” but not evidence that its complete product or security/approval model is mature enough to copy.

### OpenHands / Agent Canvas

**Evidence fact:** [`OpenHands/OpenHands`](https://github.com/OpenHands/OpenHands) remains the active canonical repository. Its current README brands the user-facing control center **Agent Canvas**: a self-hosted, always-on interface that can run OpenHands, Claude Code, Codex, Gemini, or another ACP-compatible agent across local, Docker, VM, cloud, and enterprise backends. The older separate `OpenHands/agent-canvas` repository is archived; that does not mean the current product is abandoned.

**Evidence fact:** The current control-center model supports conversations and scheduled/event-triggered automations across multiple backends. OpenHands' conversation model has durable states including idle, running, paused, waiting for confirmation, finished, error, and stuck; conversation persistence stores the complete event log, configuration, execution state, tool outputs, statistics, workspace context, activated skills, and agent state. Pause/resume and mid-run messages are explicit operations ([current canonical repository](https://github.com/OpenHands/OpenHands), [conversation persistence](https://docs.openhands.dev/sdk/guides/convo-persistence), [pause and resume](https://docs.openhands.dev/sdk/guides/convo-pause-and-resume), [conversation status API](https://docs.openhands.dev/sdk/api-reference/openhands.sdk.conversation)).

### Kortix / Suna

**Evidence fact:** [`kortix-ai/suna`](https://github.com/kortix-ai/suna) is active but has evolved beyond the earlier “Suna AI employee” framing. The current repository presents **Kortix** as a company AI command center/operating system: a shared persistent machine where agents use the same filesystem, databases, credentials, memory, and history. Its current command-center surface includes agents, reusable skills, connectors, encrypted scoped secrets, channels, triggers, and memory; work can be on-demand, human-assisted, or scheduled/triggered. The agent runtime is OpenCode.

**Design inference:** Kortix is useful evidence for a durable shared operational substrate. Its “maximum context and openness” philosophy is not automatically appropriate for Quantix, where exact Tender scoping, least privilege, independent review, and fail-closed approvals are product invariants.

### OpenCode

**Evidence fact:** The active canonical project is [`anomalyco/opencode`](https://github.com/anomalyco/opencode). OpenCode is available as a TUI, IDE extension, and beta desktop app. Its basic workspace is the current project directory. Sessions are persistent and switchable; the TUI supports details, export, undo/redo of both a conversation turn and associated file changes, attention signals, and optional visibility of reasoning blocks ([official introduction](https://opencode.ai/docs), [TUI/session controls](https://opencode.ai/docs/tui/)).

**Evidence fact:** OpenCode distinguishes primary agents and subagents. Built-in `build` and read-only `plan` agents expose mode through identity, while custom agents can define model, tools, and permissions. Permissions consistently resolve to `allow`, `ask`, or `deny`, including tool- and pattern-specific rules ([agents](https://opencode.ai/docs/agents/), [permissions](https://opencode.ai/docs/permissions/)).

**Evidence fact:** Official documentation establishes subagent invocation, not a mature multi-pane subagent observability UI. A canonical open issue explicitly describes subagents as running silently in-process and proposes observable panes; treat that limitation as a reported current gap rather than a guaranteed architecture contract ([observability request](https://github.com/anomalyco/opencode/issues/6929)).

### Cline

**Evidence fact:** [`cline/cline`](https://github.com/cline/cline) currently spans VS Code, JetBrains, CLI, SDK, and a web-based **Kanban** for many parallel agents. In the IDE, one task is a self-contained persistent session containing conversation, changes, commands, decisions, time, cost, and token use. The task timeline displays proposed edits and tool actions, and terminal output remains visible while long-running processes continue in the background ([task model](https://docs.cline.bot/core-workflows/task-management), [canonical repository](https://github.com/cline/cline)).

**Evidence fact:** Cline's core loop uses Plan and Act modes and supports per-action approval or auto-approval. It creates checkpoints after tool use in a shadow Git repository, allowing restoration of code only, task/conversation only, or both without polluting the user's Git history. Its current Kanban product gives each parallel card a worktree, auto-commit behavior, and dependency chains ([IDE workflow](https://docs.cline.bot/usage/ide), [checkpoints](https://docs.cline.bot/core-workflows/checkpoints), [canonical repository](https://github.com/cline/cline)).

### Roo Code

**Evidence fact:** Roo Code is **not an active current benchmark**. The canonical [`RooCodeInc/Roo-Code`](https://github.com/RooCodeInc/Roo-Code) repository is archived, and its README says the extension was shut down on May 15, 2026. Its earlier Code, Architect, Ask, Debug, and custom modes remain historical interaction ideas, but the product should not be described as currently available.

### goose

**Evidence fact:** The active canonical repository is [`aaif-goose/goose`](https://github.com/aaif-goose/goose), now under the Agentic AI Foundation. goose is a native general-purpose agent available as desktop, CLI, and API on macOS, Linux, and Windows. The first-run desktop flow configures a model provider and then opens a session. Extensions can render interactive UI inside the desktop app, while recipes package repeatable workflows ([official product site](https://block.github.io/goose/), [canonical repository](https://github.com/aaif-goose/goose)).

**Evidence fact:** goose supports parallel subagents, session history, permissions, cancellation, and streaming agent text/tool-call status through ACP. Its own 2026 roadmap, however, describes clear task/progress tracking for multi-agent work as an area still being built. The transport/runtime is more observability-capable than the finished end-user orchestration UI ([ACP and subagent behavior](https://github.com/aaif-goose/goose/blob/main/CUSTOM_DISTROS.md), [2026 roadmap](https://github.com/aaif-goose/goose/discussions/6973)).

### Two additional top-tier benchmarks

#### GitHub Copilot app

**Evidence fact:** GitHub's current Copilot desktop app is explicitly a parallel agent-workspace product. The user starts a session by selecting a project, choosing local repository/new worktree/cloud sandbox, selecting Interactive/Plan/Autopilot, model, and reasoning effort, and providing a task. Active sessions are grouped by repository. Each isolated session gets its own branch/worktree; quick chats support discussion without creating one ([app overview](https://docs.github.com/en/copilot/concepts/agents/github-copilot-app), [agent sessions](https://docs.github.com/en/copilot/how-tos/github-copilot-app/agent-sessions)).

**Evidence fact:** GitHub's agents panel shows live session progress, reasoning/tool logs, token use, and session length. Users can steer after the current tool call, stop while preserving pushed commits, archive, share read-only session traces, and query prior sessions. A session connects execution, commits, review, CI, and PR lifecycle ([manage sessions](https://docs.github.com/en/enterprise-cloud@latest/copilot/how-tos/copilot-on-github/use-copilot-agents/manage-and-track-agents)).

#### ChatGPT desktop app / Codex / Work

**Evidence fact:** OpenAI's current desktop app describes itself as a command center for complex work: it keeps projects and long-running conversations visible, opens documents, spreadsheets, images, and other files in the same workspace, and can use browser, desktop apps, plugins, and scheduled tasks. Onboarding is install, sign in, choose a chat/project/folder, then choose Chat, Work, or Codex and state the desired result. A Quick Chat supports questions without creating a full workstream ([official OpenAI documentation](https://learn.chatgpt.com/docs/app)).

**Design inference:** This is strong evidence for one coherent shell with different work modes, but Quantix should not expose generic Chat/Work/Codex choices. Its modes should be domain meaningful: understand a Tender, coordinate work, resolve an exception, review an artifact, or make an approval decision.

## 2. Cross-product interaction matrix

The table records what first-party sources establish. “Limited” means the capability exists but the reviewed source does not establish a strong end-user surface for it.

| Product | Workspace/project model | Task/conversation model | Agent visibility and live activity | Steering and approval | Artifacts and recovery | Background/parallel coordination |
| --- | --- | --- | --- | --- | --- | --- |
| Claude Cowork | Projects and connected local folders | Outcome-oriented Cowork session beside ordinary chat | Plan/progress, approach, parallel subagents; individual subagent presentation is not fully documented | Mid-run steer; explicit permission for destructive file deletion | Preview/download created files; sessions and files follow the account | Remote sessions, scheduled tasks, parallel workstreams |
| Agent Zero | Project scopes files, memory, secrets, instructions | Chat plus tasks/automation | Named profiles/subagents; live browser and Linux desktop canvases; streamed action trace | Pause, nudge, user interaction, configurable tool access | Files, history, memory, backup, live document cowork | Hierarchical subagents and scheduler |
| Block Buzz | Community -> rooms/channels/projects | Shared threads, workflows, events | Agents are signed members with identities, presence, messages, and audit history | Human reactions/reviews; workflow approval glue still incomplete | Canvases, media, patches, searchable signed event log | Agents can orchestrate agents; trigger workflows; project is alpha/incomplete |
| OpenHands Agent Canvas | Multiple projects and agent backends | Persistent conversation plus automation | Event stream and explicit execution states; control center spans agents/backends | Message, pause/resume, stop, confirmation state | Complete event log, workspace context, tool outputs, restored conversation | Always-on local/remote/cloud backends; scheduled/webhook automations |
| Kortix | Shared persistent company machine | On-demand, human-assisted, automated sessions | Agent/workforce model; exact end-user live subagent visualization not established | Scoped agents/secrets; human-assisted checkpoints | Shared files, memory, history, generated deliverables | 24/7 agents, triggers, orchestration |
| OpenCode | Current project directory | Persistent sessions | Primary agent identity and inline tool trace; subagent live visibility limited | Allow/ask/deny permissions; attention requests | Export, undo/redo conversation and file changes, resume sessions | Subagents exist; observable parallel UI is immature |
| Cline | IDE project/workspace; Kanban card per worktree | Self-contained task | Inline messages, commands, diffs, terminal output; Kanban shows parallel cards | Plan/Act, approve/reject, scoped auto-approve | Files/diffs, shadow-Git checkpoints, task history | Background processes; Kanban agents, dependencies, isolated worktrees |
| Roo Code | Historical IDE workspace | Historical mode-based task | Historical inline activity | Historical mode/tool approvals | Historical checkpoints | Product shut down; do not treat as current |
| goose | Session working directory and extensions | Persistent desktop/CLI session or recipe | Streaming message/tool events; subagent UX is still maturing | Permission request and cancellation through ACP | Session load/resume; extension-rendered interactive UI | Parallel delegates and reusable recipes; clearer progress UI on roadmap |
| GitHub Copilot app | Repository grouped sessions; branch/worktree/sandbox per task | Agent session or branchless Quick Chat | Live logs with tools, progress, duration, token usage | Interactive/Plan/Autopilot, steer, stop | Branch/commits/PR/CI; searchable and shareable history | Multiple isolated sessions in parallel |
| ChatGPT/Codex/Work | Chat, project, or folder in one desktop shell | Quick chat or durable long-running work | Persistent conversations and parallel work surfaces | Interactive steering and permissions | Same-workspace previews for documents and other files | Scheduled and long-running work; parallel chats |

## 3. Patterns Quantix should adopt

### 3.1 Startup: silent health, loud blockers

**Evidence fact:** Mature products keep first-run provider/authentication/folder setup separate from ordinary sessions. After setup, the normal entry is a recent project, workspace, or session. Agent Zero and goose use guided provider onboarding; OpenHands and GitHub Copilot lead with project/repository selection; ChatGPT returns to chats/projects/folders.

**Design inference:** Quantix should implement two launch paths:

1. **First run or broken setup:** a focused setup/recovery surface with only the checks and actions required to become usable.
2. **Healthy returning run:** a short branded launch state while checks execute silently, then direct entry to the last Tender Workspace. Do not replay a setup dashboard.

The splash must not be a theatrical delay. If startup exceeds a short threshold, show bounded truthful stages such as `Opening application home`, `Checking agent runtime`, and `Restoring workspace`. Healthy checks disappear. Only degraded or blocking results persist, with a concrete action and an option to enter read-only work where safe.

### 3.2 Tender is the workspace; task is the unit of delegated work

**Evidence fact:** Projects/folders/repos provide durable context; tasks/sessions isolate a goal and its history. The strongest parallel products do not make one eternal conversation carry the whole project.

**Design inference:** A Quantix Tender Workspace should contain many focused conversations and Agent Runs. Each task needs:

- a goal stated in Tender language;
- one accountable Agent Profile and visibly named collaborators/reviewers;
- scope: Tender records, source documents, tools, filesystem areas, and budget;
- status: queued, running, needs input, awaiting approval, blocked, failed, stopped, completed, superseded;
- a compact live activity summary;
- outputs and proposed record changes;
- exact recovery/retry semantics;
- links to every canonical record or evidence item affected.

### 3.3 Show work at three levels of detail

**Evidence fact:** Current products balance simple progress summaries with expandable tool/file logs. Products that expose only chat hide material actions; products that show raw event firehoses become difficult to supervise.

**Design inference:** Quantix needs progressive disclosure:

1. **Status strip:** who is working, current objective, current step, elapsed time, and whether attention is needed.
2. **Activity timeline:** meaningful events such as “reviewing Addendum 03,” “found 4 changed requirements,” “waiting for clarification,” or “proposed BOQ reconciliation.”
3. **Technical trace:** exact tool calls, command output, tokens, model/runtime facts, retries, IDs, hashes, and logs.

Do not expose private chain-of-thought. Show concise action rationale, plan, evidence, and outcomes that the Engineer can audit.

### 3.4 Make agents visible identities without turning work into theater

**Evidence fact:** Buzz gives agents the same room/member identity primitives as people; Agent Zero uses named profiles and hierarchical agents; OpenCode and Cline expose mode/role; Claude coordinates parallel subagents while keeping the user's requested outcome central.

**Design inference:** Every active Quantix Agent should have a stable human-readable identity consisting of:

- role name, such as `Requirements Analyst` or `Independent Cost Reviewer`;
- state and current assignment;
- permissions/scope;
- parent task and any child agents;
- independence constraints where relevant;
- output ownership and review responsibility.

Use a compact agent roster and expandable activity, not animated avatars or simulated typing. The goal is accountability and intervention, not anthropomorphic decoration.

### 3.5 Conversation is the control surface; records are the truth

**Evidence fact:** Buzz's unified event log makes conversation and approvals searchable, while Cline and GitHub tie session activity to diffs, commits, and review. These products still distinguish the execution trace from the resulting artifact.

**Design inference:** In Quantix:

- chat can request, clarify, steer, explain, and summarize;
- activity can show who did what and when;
- an Agent can propose a requirement, risk, query, estimate change, review finding, or package;
- only the typed domain record and exact evidence/version binding becomes canonical;
- a chat message, emoji, or “looks good” can never become formal approval;
- formal approvals remain explicit actions displaying the exact bound version and consequences.

This preserves Quantix's control model while making the process feel live and collaborative.

### 3.6 One attention queue across all work

**Evidence fact:** GitHub's agents panel centralizes active sessions and attention; OpenHands has explicit waiting/error/stuck states; Cline Kanban exposes dependency chains; remote Cowork notifies users when work needs input or completes.

**Design inference:** Quantix should have one Engineer attention queue ordered by urgency and Tender deadline, containing only actionable states:

- approval required;
- question/clarification required;
- blocked by access, source evidence, dependency, or budget;
- independent finding awaiting disposition;
- failed or stuck work needing recovery;
- completed deliverable awaiting review;
- material addendum/change impact.

Healthy running work belongs in a compact activity center, not the attention queue.

### 3.7 Artifacts beside the conversation

**Evidence fact:** ChatGPT/Work and Claude Cowork preview finished files in the workspace; Agent Zero exposes files, browser, desktop, and document cowork; Cline puts diffs next to the task.

**Design inference:** Quantix should use a three-pane pattern when screen size permits:

```text
Tender / tasks          Conversation + activity          Artifact / evidence
-----------------       -----------------------          -------------------
Overview                Engineer request                 Source document
Attention               Agent plan                       Extracted passage
Requirements            Live work events                 Proposed record diff
Estimate                Questions / steering             BOQ / estimate table
Reviews                 Completion summary               Review / approval view
```

The right pane is contextual, not permanent clutter. It should open the exact source, proposed record, calculation, comparison, or deliverable involved in the selected message/activity event.

### 3.8 Recovery must be designed before autonomy

**Evidence fact:** Cline uses checkpoints, OpenCode uses undo/redo, OpenHands persists full conversation state, GitHub isolates work in branches/worktrees, and Buzz keeps a signed event log.

**Design inference:** Every Quantix Agent Run should answer:

- What survives app restart?
- Can the Engineer resume, retry from a safe boundary, or branch a new attempt?
- Which proposed changes were never accepted?
- Which canonical records changed, and under what explicit approval?
- If the run failed midway, what is still trustworthy?

Retry must not silently duplicate work or mutate the authoritative Tender. A recovered run should show its relationship to the previous attempt.

### 3.9 Permission requests should be rare, local, and consequential

**Evidence fact:** OpenCode's allow/ask/deny model is predictable; Cline previews exact actions; Claude reserves explicit deletion permission for destructive actions; GitHub offers distinct autonomy modes.

**Design inference:** Quantix should not ask the Engineer to approve routine reads or every harmless tool call. Permission policy should be set at task start and interrupt only for:

- expanding scope to new Tender data, folders, integrations, or external network targets;
- spending beyond the approved budget;
- writing outside the run's proposal area;
- changing or binding canonical Tender records;
- destructive or irreversible local actions;
- formal approvals, releases, exports, archive/trash/purge actions;
- any action explicitly prohibited by the Tender's governance policy.

Each prompt must say what will happen, what exact object/version is affected, why it is needed, and what happens if rejected.

## 4. What not to copy

- **Do not copy a generic chat-first home.** A Tendering Engineer needs deadline, change, risk, decisions, and work status immediately.
- **Do not copy Buzz's “one event type for everything” into the domain model.** It is valuable for activity and search, but Tender requirements, estimates, approvals, evidence, and packages need typed semantics.
- **Do not copy Kortix's broad shared-access philosophy.** Quantix should retain explicit Tender scope and least privilege.
- **Do not expose subagent activity only as nested raw logs.** The Engineer needs named responsibility, current assignment, output, and intervention points.
- **Do not force one worktree/branch metaphor onto non-code Tender work.** Use versioned proposal sets and exact record bindings appropriate to Quantix.
- **Do not make autonomy a single global “YOLO” switch.** Autonomy belongs to a task, scope, tool/action class, and budget.
- **Do not equate visible reasoning with trust.** Trust comes from exact evidence, deterministic calculations where required, attributable actions, independent review, and explicit approval.
- **Do not treat Roo Code as a current product benchmark.** It is archived and shut down.
- **Do not treat Buzz's unfinished approval workflow as validated practice.** Borrow its participant and event-log concepts, not its maturity assumptions.
- **Do not add a splash animation that hides indefinite initialization.** Startup must remain truthful, cancelable where possible, and actionable when blocked.

## 5. Recommended first prototype questions

The next phase should not redesign every screen. A small runnable prototype should answer these questions with the Tendering Engineer:

1. On launch, can the Engineer understand whether Quantix is opening normally without seeing infrastructure detail?
2. On entering a Tender, can the Engineer answer within ten seconds: what changed, what is running, what needs me, and what is due next?
3. Can the Engineer start one delegated outcome, see which Agent(s) are working, inspect meaningful live activity, and steer it?
4. Can the Engineer open a source/artifact beside that activity without losing the conversation?
5. Can the Engineer distinguish ordinary Agent conversation from a formal proposal, review, and approval?
6. Can the Engineer supervise three concurrent tasks without opening each one?
7. Can the Engineer recover a stopped/failed task and understand what remains trustworthy?

A suitable tracer scenario is:

> Open a newly imported Tender, ask Quantix to establish the deadline/compliance baseline, watch a Requirements Analyst and independent reviewer work, answer one clarification, inspect exact source evidence beside a proposed requirement, approve the typed proposal set, then return to the Tender overview and see the next action.

That slice exercises startup, workspace orientation, visible agents, chat, activity, evidence, artifacts, attention, approval, and recovery without requiring the entire Tender lifecycle to be redesigned at once.

## 6. Research limitations

- This is desk research, not observation of Tendering Engineers using these products.
- Product documentation establishes supported concepts but may omit UX friction, latency, and failure behavior.
- Claude Cowork, ChatGPT Work/Codex, GitHub Copilot app, OpenCode Desktop, Kortix, Agent Canvas, and Buzz are changing quickly; exact screenshots and labels should be rechecked immediately before visual prototyping.
- First-party sources do not establish that all products expose individual parallel subagents equally well. Where evidence was absent, the report says so.
- “Buzz” is resolved with high confidence to Block's public repository because it exactly matches the requested agent-workspace context. If the user meant a private or different product, its URL is required.
- The report identifies interface patterns; it does not decide Quantix's final information architecture, visual language, or autonomy policy. Those belong to the upcoming grilling and prototype.

## Primary source index

- Anthropic: [Cowork getting started](https://support.claude.com/en/articles/13345190-get-started-with-claude-cowork), [cross-surface behavior](https://support.claude.com/en/articles/15520349-use-claude-cowork-on-web-desktop-and-mobile), [desktop install/availability](https://support.claude.com/en/articles/10065433-install-claude-desktop)
- Agent Zero: [canonical repository](https://github.com/agent0ai/agent-zero), [official docs](https://www.agent-zero.ai/p/docs/), [usage guide](https://github.com/agent0ai/agent-zero/blob/main/docs/guides/usage.md)
- Buzz: [canonical repository and maturity table](https://github.com/block/buzz), [current release](https://github.com/block/buzz/releases/tag/desktop-v0.5.11)
- OpenHands: [canonical Agent Canvas repository](https://github.com/OpenHands/OpenHands), [conversation persistence](https://docs.openhands.dev/sdk/guides/convo-persistence), [pause/resume](https://docs.openhands.dev/sdk/guides/convo-pause-and-resume)
- Kortix/Suna: [canonical repository](https://github.com/kortix-ai/suna)
- OpenCode: [official docs](https://opencode.ai/docs), [TUI](https://opencode.ai/docs/tui/), [agents](https://opencode.ai/docs/agents/), [permissions](https://opencode.ai/docs/permissions/)
- Cline: [canonical repository](https://github.com/cline/cline), [tasks](https://docs.cline.bot/core-workflows/task-management), [checkpoints](https://docs.cline.bot/core-workflows/checkpoints)
- Roo Code: [archived canonical repository and shutdown notice](https://github.com/RooCodeInc/Roo-Code)
- goose: [official site](https://block.github.io/goose/), [canonical repository](https://github.com/aaif-goose/goose), [ACP/subagent integration](https://github.com/aaif-goose/goose/blob/main/CUSTOM_DISTROS.md)
- GitHub Copilot app: [overview](https://docs.github.com/en/copilot/concepts/agents/github-copilot-app), [agent sessions](https://docs.github.com/en/copilot/how-tos/github-copilot-app/agent-sessions), [manage sessions](https://docs.github.com/en/enterprise-cloud@latest/copilot/how-tos/copilot-on-github/use-copilot-agents/manage-and-track-agents)
- OpenAI: [ChatGPT desktop app](https://learn.chatgpt.com/docs/app)
