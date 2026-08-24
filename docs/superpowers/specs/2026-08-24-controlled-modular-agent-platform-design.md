# Quantix Controlled Modular Agent Platform — Design

Date: 2026-08-24

Status: Approved in the design session

Scope: Master architecture; implementation requires separate layer specifications and plans

## 1. Goal

Quantix becomes a closed, local-first Windows tendering application that can run a
dynamic team of AI employees for a beginner construction engineer. Quantix reuses
maintained frameworks, libraries, protocols, and research patterns selectively. It
does not embed OpenClaw, Goose, Hermes, Grok Build, Agent Zero, or another complete
agent product.

The Engineer sees one integrated Quantix application. The trusted Rust Host owns
Tender state, evidence, permissions, approvals, memory, agent identity, messaging,
budgets, audit, recovery, and publication. The selected AI provider supplies
reasoning and generation but cannot acquire authority or write canonical Tender
state directly.

The product grows in working layers. Each layer must leave a usable end-to-end
product before the next layer starts.

## 2. Approved Product Decisions

- Quantix uses a **controlled modular hybrid** architecture.
- All managed execution is local on the Engineer's Windows computer. Online AI,
  web search, and connected services remain explicit network boundaries.
- Quantix ships with no selected AI provider, model, reasoning level, or fallback.
- The Engineer configures connections and selects one global **Active AI
  Configuration**. It powers the Tendering Manager and every AI employee.
- The first release supports four connection methods: approved account login,
  direct provider key, custom OpenAI-compatible endpoint, and custom
  Anthropic-compatible endpoint.
- Multiple connections may be saved, but only one is active. Quantix and the
  Manager never switch providers automatically.
- Each Tender has one Tendering Manager. The Manager is the only fixed role; all
  specialist employees are created dynamically.
- AI employees have full Egyptian Arabic identities and professional profiles.
- The Engineer works Manager-first but can inspect, message, redirect, pause, or
  stop any employee.
- Safe internal work is automatic. Consequential external actions require exact
  Engineer approval.
- Memory is layered into company, Tender, agent, and temporary scopes.
- Agents can communicate directly through a governed, durable Tender message bus.
- Quantix runs in an optional Windows tray mode while the computer remains on.
- Smart parallelism and a Host-owned Loop Guard prevent runaway agent and usage
  loops.
- The Manager may discover, compose, and propose skills and tools. New executable
  tools enter a controlled workshop, start Tender-only, and may be promoted only
  after testing and approval.
- Quantix includes a staged Adaptive Self-Improvement system. Release one enables
  output refinement, structured lessons, and governed skill evolution. Live
  production code mutation and model-weight adaptation remain disabled.
- The UI supports Arabic and English. Arabic uses a complete right-to-left layout.
- There is no public marketplace, arbitrary end-user plugin installation, or
  developer configuration surface.

## 3. Superseded Decisions and Documentation Consequences

This master design supersedes the approved
[`2026-08-22-codex-only-beginner-connection-design.md`](./2026-08-22-codex-only-beginner-connection-design.md).
That design's one-connection, ChatGPT-only, recommended-model, and plaintext
`auth.json` consequences are obsolete.

Implementation must create a replacement ADR that:

- supersedes [`ADR 0016`](../../adr/0016-connect-chatgpt-through-quantix-owned-oauth.md)
  for provider count, authentication, credential persistence, and direct private
  backend execution;
- supersedes the AI-selection consequence of
  [`ADR 0014`](../../adr/0014-scope-ai-execution-and-asa-operations-per-tender.md):
  selection is now global and Engineer-owned, not copied or overridden per Tender;
- supersedes the Bootstrap Team, static-specialization, and no-fictional-employee
  consequences of
  [`ADR 0005`](../../adr/0005-compose-tender-teams-through-controlled-capability-demands.md):
  one fixed Manager now creates versioned Egyptian Arabic product identities for
  temporary specialist agents without granting them independent authority;
- supersedes only the cross-platform release consequences of
  [`ADR 0009`](../../adr/0009-run-one-local-host-over-self-contained-tender-stores.md)
  and [`ADR 0010`](../../adr/0010-qualify-v0-through-layered-product-acceptance.md):
  release one is Windows-only, while the one Tauri/Rust Host, single-writer,
  `~/.quantix`, and layered acceptance boundaries remain accepted;
- leaves [`ADR 0012`](../../adr/0012-connect-provider-neutral-ai-without-silent-fallback.md)
  historical rather than reviving its obsolete implementation details; and
- preserves the enduring Host-owned permission, evidence, audit, validation,
  retention, and no-silent-fallback principles.

[`ADR 0002`](../../adr/0002-keep-chats-outside-the-tender-system-of-record.md)
remains accepted: conversation and memory can influence proposals but never become
canonical Tender facts silently. [`ADR 0006`](../../adr/0006-enforce-agent-access-through-host-owned-run-grants.md)
also remains accepted and applies to every provider, dynamic employee, memory read,
tool/workshop action, and Improvement Lab run.

The provider-specific, per-Tender-selection, Bootstrap Team, no-portrait,
no-cross-Tender-company-memory, and one-language assumptions in
[`docs/product/agentic-tender-workspace.md`](../../product/agentic-tender-workspace.md)
must be updated in the relevant implementation layers. The Manager remains
Tender-scoped; only approved company memory crosses Tender boundaries. No
compatibility adapter, legacy deserializer, migration, fallback, or dual path is
added. Obsolete paths and fresh-development data are removed when the replacement is
implemented.

The existing
[`2026-08-22-codex-only-beginner-connection.md`](../plans/2026-08-22-codex-only-beginner-connection.md)
and
[`2026-08-21-chatgpt-direct-provider.md`](../plans/2026-08-21-chatgpt-direct-provider.md)
implementation plans are historical and must not be executed or incrementally
extended. Layer one receives a fresh plan from this design.

## 4. Core Architecture

### 4.1 Engineer Interface

The Tauri WebView loads only bundled Quantix UI. It receives credential-free,
typed views and invokes named Host commands. It never receives filesystem, SQL,
generic shell, raw provider protocol, or secret-store authority.

The primary interaction is the Tendering Manager workspace. Team, Tasks, Evidence,
Outputs, Approvals, and Activity remain inspectable without exposing hidden model
reasoning or developer internals.

### 4.2 Trusted Rust Host

The Rust Host is the sole writer for `~/.quantix` and the sole authority for:

- Tender and application state;
- dynamic employee identity and lifecycle;
- permission envelopes and per-run grants;
- tool admission and side-effect approval;
- evidence, validation, calculations, and publication;
- memory scopes, provenance, invalidation, and curation;
- durable tasks, messages, schedules, budgets, and Loop Guard state;
- worker supervision, cancellation, recovery, and updates; and
- redacted diagnostics and audit receipts.

No prompt, provider setting, skill, tool, or agent message can expand Host authority.

### 4.3 AI Runtime Workers

Provider protocols run in replaceable, supervised local workers behind one
Quantix-owned AI Runtime Contract. The Host does not reimplement provider SDKs. It
uses official SDKs or maintained compatibility libraries in isolated workers.

The contract normalizes:

- connection health and model discovery;
- start, continue, stream, steer, cancel, and terminal result;
- tool calls and approval requests;
- structured output and validation failure;
- usage, rate limits, latency, and provider errors; and
- declared capabilities such as tools, images, reasoning, structured output,
  context size, and streaming.

One Codex worker uses the official Codex SDK for account-backed Codex execution. A
general provider worker handles direct API-key and compatible endpoint protocols.
Different run grants use isolated worker scopes; workers may be pooled only when
their user, connection, tools, skills, workspace, and sandbox policy are identical.

### 4.4 Capability Workers

Trusted deterministic Rust libraries may execute in the Host when they do not
process active or executable untrusted content. Document, browser, schedule, BIM,
CAD, office, generated-tool, and other higher-risk engines run as supervised local
workers with restricted inputs, outputs, filesystem, network, time, memory, and
process trees.

### 4.5 Local Data

Quantix-managed data stays under `~/.quantix`. Tender data, vectors, memory,
employees, messages, tasks, approvals, audit, runtimes, models, logs, and backups
remain local. Only the minimum selected work context crosses an explicitly approved
AI, web, portal, or connector boundary.

## 5. AI Connections and Settings

### 5.1 Connection Methods

The first release supports:

1. **Account login.** Only public, provider-approved third-party commercial flows
   are allowed. ChatGPT/Codex is the initial account-backed connector and remains a
   commercial-release gate until OpenAI approves the Quantix client/integration.
   Claude.ai, Gemini CLI, Grok consumer, or other private/first-party session tokens
   are never copied or reverse engineered.
2. **Direct provider key.** The supported direct catalogue initially covers OpenAI,
   Anthropic, Google Gemini, and xAI. Other providers use a compatible endpoint until
   a first-party adapter proves necessary.
3. **Custom OpenAI-compatible endpoint.** The Engineer supplies a name, HTTPS base
   URL (or an explicit local loopback URL), API key or bearer token, model ID, and
   optional headers/query parameters. Quantix probes the claimed capabilities.
4. **Custom Anthropic-compatible endpoint.** The Engineer supplies the same
   connection facts for an Anthropic Messages-compatible service.

Provider-compatible does not mean capability-equivalent. Quantix records what the
tested model actually supports and fails closed when a required capability is
missing.

### 5.2 No Factory Default

Quantix chooses no provider or model. Before the first AI run, the Engineer must:

1. create or sign into a connection;
2. test it;
3. select a model and supported reasoning setting; and
4. mark the complete selection as the Active AI Configuration.

Several connections may be saved. The active configuration powers the Manager,
employees, memory/skill review, and tool workshop. The Manager cannot change it.
Changing it affects future runs only; active and historical runs retain their exact
connection revision, provider, endpoint identity, model, reasoning, adapter version,
and data destination. Failure pauses AI work and asks the Engineer; there is no
silent fallback.

### 5.3 Settings UX

The settings flow is **AI Settings → Connections → Test → Select Active**. The UI
supports add, test, edit, disable, delete, and disconnect without exposing raw
protocol or environment-variable terminology. A custom connection reveals only the
fields required for its chosen protocol.

## 6. Credential Persistence

Quantix keeps ordinary non-secret application settings in
`~/.quantix/installation.sqlite`. All AI connection configuration and secret material
are stored in one versioned `~/.quantix/ai-connections.vault`.

On Windows, the Host serializes the complete vault payload and protects it directly
with user-scoped DPAPI (`CryptProtectData`) on every save. This is not Windows
Credential Manager: it creates no external credential entries, settings surface, or
extra password prompt. The vault remains one Quantix-owned file. Quantix must never
use the machine-scoped DPAPI flag.

Vault writes use a cross-process mutation lock, same-directory temporary file,
flush, `sync_all`, Windows `ReplaceFileW`/write-through replacement, and bounded
retry for antivirus or sharing contention. Corrupt or undecryptable state fails
closed and is never interpreted as an intentionally empty store.

The renderer may submit a secret once through a narrow command but never receives it
back. Secrets never enter `installation.sqlite`, Tender Stores, logs, diagnostics,
crash reports, provider transcripts, backups, archives, exports, or generated
artifacts. Disconnect deletes the exact connection secret and invalidates dependent
approvals.

Tender and application backups exclude the vault. Restoring on another computer
requires the Engineer to sign in again or re-enter keys. Quantix provides no portable
credential export in the first release.

## 7. Dynamic AI Employees

### 7.1 Manager and Creation

Every Tender has one Manager with a complete Egyptian Arabic AI employee identity.
The role boundary is fixed; the persona is a versioned employee profile. The Manager
analyzes the Tender task and creates only the specialists needed for current work.
There is no static worker roster.

Each employee profile includes:

- Arabic human-style name, avatar, AI label, and employee number;
- Arabic job title, department, mission, responsibilities, expertise, simulated
  background, working style, languages, and communication style;
- Manager, collaborators, assignment, priority, deadline, success checks, and status;
- exact skills, tools, connections, allowed data, memory scope, powers, prohibited
  actions, time/usage budget, and profile version; and
- performance, corrections, outputs, messages, and lifecycle history.

Profiles never claim real human identity, employment, certification, or credentials.

### 7.2 Authority and Actions

The Manager assigns powers only from the intersection of the Engineer-approved
Tender authority envelope, the task, the capability catalogue, and current Host
policy. It cannot mint authority.

Read, retrieval, public research, calculation, drafting, internal tasks, and
candidate outputs may run automatically within the grant. Sending, uploading,
publishing, deleting, changing final prices, permissions, purchases, legal terms, or
Tender submission require an exact Engineer approval.

### 7.3 Communication and Engineer Access

Employees can message, challenge, request help, and hand off work directly through a
durable Tender message bus. The Manager can observe, steer, stop, or redirect the
conversation. A message may carry only data allowed to both participants.

The Engineer works Manager-first but can open any employee profile, inspect exact
work and permissions, chat directly, change the assignment, pause, or stop it.

### 7.4 Lifecycle and Parallelism

Employees start automatically within safe approved limits and are temporary by
default. They move through preparing, working, waiting, blocked, review, completed,
failed, and archived states. A proven employee may be promoted into a reusable
company employee only after review.

The Manager uses smart parallelism within Host-issued concurrency, time, iteration,
tool, search, and provider-usage budgets. Child work consumes the parent budget. In
release one, only the Manager creates employees. Controlled subagent creation and
recursive swarms remain documented future experiments and are disabled.

## 8. Loop Guard

The Rust Host independently detects:

- repeated tool plus canonical arguments and unchanged result;
- repeated provider or tool errors;
- repeated searches and source sets;
- semantically repeated plans with no new evidence, state, or artifact;
- repeated messages, handoffs, delegation goals, and A→B→A cycles;
- agent-creation cycles; and
- work that consumes budget without a progress receipt.

Progress means new evidence, a verified calculation, a new artifact, a durable task
transition, or a concrete blocker. Planning or paraphrasing alone is not progress.

The Loop Guard requests one changed strategy, then cancels the agent and descendants
if repetition continues. It preserves work, reports a structured blocker to the
Manager, and asks the Engineer whether to change instructions or budget. Agents and
providers cannot disable or configure the guard.

## 9. Evidence, Retrieval, Memory, and Skills

### 9.1 Evidence Is Not Memory

Original documents, emails, drawings, spreadsheets, connector objects, and web
captures remain immutable, versioned evidence with source hash and precise page,
paragraph, table, cell, message, URL, selector, or retrieval-time location. Answers,
memories, calculations, and deliverables reference those locations. A changed source
invalidates dependent candidates.

### 9.2 Search

Quantix combines SQLite full-text keyword search with local dense-vector search and
rank fusion. Exact search covers clauses, codes, units, and amounts; vector search
covers meaning across Arabic and English. The current 384-dimensional
`multilingual-e5-small` model remains the first benchmark candidate because it is
multilingual, retrieval-trained, and derived from multilingual MiniLM. It is kept
only if the Arabic/English construction corpus proves it. Indexes are rebuildable and
never authoritative.

### 9.3 Layered Memory

- **Company memory:** Engineer-approved stable policies and preferences.
- **Tender memory:** sourced facts and decisions for one Tender.
- **Agent memory:** role-specific lessons and working preferences.
- **Temporary memory:** task notes that expire after completion.
- **Conversation history:** complete local history searched on demand, not injected as
  permanent memory automatically.

Every permanent memory has source, scope, owner, confidence, creation and review
dates, expiry, supersession, and edit/delete history. Web, email, document, and tool
content cannot silently become a permanent instruction. The Engineer can inspect,
edit, approve, reject, or delete memory and skills.

## 10. Capability Catalogue and Controlled Tool Workshop

### 10.1 Capability Types

The internal catalogue contains tools, skills, connections, tool-connector endpoints,
and specialist engines. Each manifest records source, exact version/hash, license,
runtime, Windows support, input/output schemas, permissions, data classes, network,
side effects, resource limits, sandbox profile, tests, and promotion status.

The Host validates every capability call. A capability cannot expand authority,
access unrelated files or credentials, bypass approval, or open undeclared network
destinations.

### 10.2 Selective Framework Reuse

Quantix may directly depend on, selectively fork, wrap, or adapt small modules from
established projects. A component is admitted only when it removes more complexity
than it adds, has an acceptable license, is maintained, works on Windows, and remains
replaceable behind a Quantix contract. Complete external agent products and public
marketplaces are excluded.

### 10.3 Workshop Lifecycle

When a capability is missing, the Manager:

1. searches the installed catalogue;
2. searches trusted official sources and repositories;
3. prefers a maintained ready skill, tool, library, or framework component;
4. records license, maintenance, Windows, dependency, and security findings;
5. copies only required sources into quarantine and never executes internet code
   directly;
6. builds/adapts inside an isolated workspace with fake data, no secrets, and denied
   network unless the test explicitly requires it;
7. runs schemas, unit, integration, security, resource, and capability tests;
8. shows a plain-English purpose, access, risk, provenance, and result report;
9. requires Engineer approval and activates the tool for one Tender only; and
10. monitors outcomes and proposes permanent company promotion only after successful
    use and stronger held-out tests.

Every version has a parent, hash, diff, test receipt, approval, rollback pointer, and
archive-not-delete history.

## 11. Web, Browser, and Connections

Public web research is automatic within task budgets. Static fetch is preferred;
managed local browser automation is used for JavaScript-heavy sites. Agents may read
and download from Engineer-approved authenticated Tender portals. Passwords,
passkeys, MFA codes, CAPTCHAs, payments, and legal confirmations require human
takeover. Upload, send, delete, permission change, and final submission require exact
approval.

Connections are built-in, curated Quantix integrations—not a marketplace. They use
the service's stable API when available and request the minimum delegated scope.
Microsoft Graph provides selected SharePoint/OneDrive files and Outlook mail/calendar
as a first-wave connection; Teams-backed files follow their SharePoint source. Change
tracking uses stored incremental checkpoints. Writes and sends are always previewed.

## 12. End-to-End Tender Flow

The product flow is:

**Import → Understand → Plan → Create Team → Work → Validate → Engineer Approval →
Deliver → Improvement Review**

1. The Engineer creates a Tender and adds local or connected sources and deadlines.
2. Local workers extract text, tables, OCR, structure, provenance, and revisions.
3. The Manager reviews the Tender stage, deliverables, evidence, deadlines,
   instructions, active AI configuration, and capabilities.
4. The Manager creates the minimum dynamic team and a governed work plan.
5. Employees retrieve evidence, research, calculate, analyze compliance/BOQ/risk,
   draft, communicate, and hand off work.
6. Deterministic rules, citation checks, required-field validation, independent
   employee review, and Manager review treat outputs as candidates.
7. The Engineer approves consequential decisions and external actions.
8. Quantix publishes evidence-linked outputs and immutable approval records.
9. The Improvement Lab reviews the completed trajectory separately; the live Tender
   result is never rewritten by that review.

## 13. Quantix Improvement Lab

### 13.1 Two Loops

The fast work loop is **Work → Validate → Correct → Deliver**. The slow improvement
loop is **Collect evidence → Identify lesson → Create candidate → Test → Compare →
Approve → Trial → Promote or Roll back**.

The durable improvement belongs to Quantix, not the selected provider. Switching the
Active AI Configuration preserves Quantix memory, skills, tools, workflows, and
evaluation history.

### 13.2 Release-One Levels

Release one enables:

1. output self-refinement driven by deterministic validator feedback;
2. Reflexion/ExpeL-style typed, source-linked lesson candidates; and
3. Voyager/OpenClaw-style governed skill proposals and lifecycle management.

The Lab may propose memory, skills, prompt sections, tool descriptions, workflows,
and Tender-specific tools. Permanent skills and company memory require Engineer
approval. Candidates never write live assets directly.

### 13.3 Later Levels and Prohibitions

GEPA/DSPy-style prompt, tool-description, and workflow optimization may run later as
offline, held-out-evaluated work. Production self-editing of the Rust Host, permission
policy, security code, evaluator, provider credentials, or release build is forbidden.
SICA/DGM/AlphaEvolve-style code evolution remains an isolated development experiment.
SEAL/ERL-style model-weight adaptation remains research-only.

Every candidate records baseline and parent hashes, evidence, exact diff, train/
validation/hidden-test results, quality, cost, latency, security findings, trial,
approval, monitoring, and rollback. The candidate cannot read or modify the hidden
tests or evaluator. Provenance gates prevent untrusted input from becoming a delayed
memory, skill, scheduled job, or external action.

## 14. Reliability, Safety, and Recovery

- Quantix never silently changes provider, model, connection, tool, source,
  permission, or approval target.
- Only clearly transient network/provider/tool failures receive bounded jittered
  retry. External or uncertain side effects never auto-retry.
- Every task and run has durable idempotent state. Restart classifies it as safe to
  continue, completed, failed, blocked, or uncertain.
- An external action with uncertain outcome requires Engineer review before another
  attempt.
- Worker processes receive restricted workspaces, handles, network, time, memory,
  output, and process-tree limits and are killed as a group.
- Web, email, document, connector, tool, memory, and agent-message origins propagate
  into dependent candidates and approval packets.
- Approval binds the exact action kind, target, arguments, data/source versions,
  agent, grant, and expiry. Material change invalidates it.
- Errors explain what happened, what was preserved, what was prevented, what may
  have changed externally, and the next safe action.
- Logs remain local and exclude credentials, raw provider traffic, prompt/response
  bodies, Tender content by default, and hidden reasoning.
- The Engineer has Pause All and Stop All controls.

## 15. Bilingual Beginner UX

First setup asks the Engineer to select Arabic or English, add/test a connection,
select the Active AI Configuration, review data destinations, choose controlled
browser access, and choose tray startup. It never exposes ports, environment
variables, terminals, framework names, or raw protocol errors.

The Tender workspace provides Manager, Team, Tasks, Evidence, Outputs, Approvals, and
Activity. Employee cards expose identity, assignment, progress, powers, skills,
tools, memory scope, budget, messages, outputs, performance, and controls.

Settings provides Language, AI Connections, Active AI Configuration, Data and
Storage, Web and Connected Services, Memory and Improvement Lab, Background Work,
Updates, and Diagnostics. There is no marketplace or developer panel.

Arabic mode is fully right-to-left; English mode is left-to-right. Employee identity
remains Egyptian Arabic in both. Source documents retain their original language and
technical terms can show the original value. Language changes never alter evidence.

Tray notifications are limited to approval, completion, blocker, Loop Guard stop,
connection failure, and approaching deadline. The tray exposes Open, Pause All,
Resume Approved Work, Attention Needed, and Exit Completely.

## 16. Verification and Release Gates

### 16.1 Deterministic and Contract Tests

Provider-free tests cover permissions, approvals, calculations, state, memory,
employees, messages, Loop Guard, crash recovery, idempotency, secret exclusion,
backup, and restore. Every provider/connection adapter must prove authentication,
discovery, streaming, tools, structured output, cancellation, usage, and typed
errors. Unsupported capabilities are explicit.

### 16.2 Tender Benchmark Library

Versioned Arabic and English fixtures contain digital/scanned PDFs, BOQs, emails,
addenda, drawings, missing/conflicting facts, prompt injections, poisoned sources,
and exact expected calculations/citations/decisions. Measures include parser/OCR,
retrieval, citations, calculations, compliance coverage, unsupported claims,
workflow completion, handoffs, latency, memory, usage, and Loop Guard precision.

### 16.3 Improvement Evaluation

Candidates run against train, validation, and immutable hidden held-out sets,
security probes, cost/latency budgets, and no-regression thresholds. The improver
cannot access evaluator assets. The current approved artifact remains the champion
until a reviewed challenger proves improvement.

### 16.4 Windows Acceptance

Clean private Windows runs must complete Import → Understand → Create Team → Work →
Validate → Approve → Export → Restart → Recover. Release is blocked by permission
bypass, secret leakage, uncertain calculation, escaped loop budget, accepted
capability regression, missing provenance/license evidence, or recovery that can
repeat an external action.

## 17. Delivery Layers

Each layer receives a focused specification, implementation plan, review, and
end-to-end acceptance before the next begins:

1. **AI connection foundation:** four methods, user-selected active configuration,
   encrypted vault, capability probing, normalized events, no fallback.
2. **Dynamic AI team:** Manager, Egyptian employee personas, grants, messaging,
   lifecycle, smart parallelism, Loop Guard, Manager-first UX.
3. **Memory and evidence:** layered memory, hybrid retrieval, source-linked facts,
   searchable history, invalidation, Arabic/English benchmarks.
4. **Capabilities and connections:** catalogue, controlled browser/web, curated tool
   connectors, Microsoft 365, construction workers, worker isolation.
5. **Controlled Tool Workshop:** discovery, quarantine, license/security checks,
   sandboxed build, Tender-only trial, prove-then-promote, rollback.
6. **Quantix Improvement Lab:** levels 1–3 first; offline level 4 later; production
   levels 5–6 disabled.
7. **Release hardening:** complete benchmarks, private Windows acceptance, security,
   provider matrix, recovery, backup, bilingual UX, and license evidence.

Production builds remain explicit release-stage operations, not ordinary development
verification.

## 18. Research Basis

The design selectively adopts ideas rather than whole systems:

- [OpenAI Codex SDK](https://learn.chatgpt.com/docs/codex-sdk) for supported embedded
  Codex control.
- [Vercel AI SDK provider architecture](https://ai-sdk.dev/docs/foundations/providers-and-models)
  for direct and compatible provider normalization patterns.
- [OpenClaw embedding and multi-agent design](https://docs.openclaw.ai/gateway/embedding)
  for replaceable runtime supervision and agent isolation.
- [Goose custom distributions](https://goose-docs.ai/docs/guides/custom-distributions/)
  for native application layering and selective packaging.
- [Hermes Agent memory](https://github.com/NousResearch/hermes-agent/blob/main/website/docs/user-guide/features/memory.md)
  and [self-evolution](https://github.com/NousResearch/hermes-agent-self-evolution)
  for bounded learning and offline evolution.
- [Agent Zero memory](https://github.com/agent0ai/agent-zero/blob/main/plugins/_memory/README.md)
  for scoped memory, reindexing, and human curation.
- [Self-Refine](https://arxiv.org/abs/2303.17651),
  [Reflexion](https://arxiv.org/abs/2303.11366),
  [ExpeL](https://arxiv.org/abs/2308.10144), and
  [Voyager](https://voyager.minedojo.org/) for non-parametric improvement.
- [GEPA](https://arxiv.org/abs/2507.19457) for trace-aware offline prompt and
  workflow evolution.
- [OpenClaw Skill Workshop](https://docs.openclaw.ai/tools/skill-workshop) for
  proposal, hash, quarantine, apply, and rollback lifecycle.
- [Sleeper Channels and Provenance Gates](https://arxiv.org/abs/2605.13471),
  [memory poisoning research](https://arxiv.org/abs/2606.04329), and
  [reward hacking in self-refinement](https://arxiv.org/abs/2407.04549) for the
  persistent-state threat model.
- [Microsoft DPAPI](https://learn.microsoft.com/en-us/windows/win32/api/dpapi/nf-dpapi-cryptprotectdata)
  for invisible user-scoped vault protection.

## 19. Master Acceptance Criteria

The architecture is realized only when:

1. A beginner can configure any of the four connection methods, test it, and choose
   one active provider/model without developer concepts or a product-selected
   default.
2. Secrets exist only in the user-scoped encrypted vault and never cross the
   credential-free renderer, Tender, log, diagnostic, backup, or export boundaries.
3. A Tender Manager dynamically creates a minimal Egyptian Arabic team whose powers,
   data, tools, memory, budgets, and messages are Host-enforced and inspectable.
4. Agents collaborate and recover without uncontrolled spawning, usage loops,
   permission expansion, silent routing, or duplicate side effects.
5. Every important claim, calculation, memory, action, and deliverable is traceable
   to source, tool/rule version, agent, provider/model, validation, and approval.
6. The Tool Workshop can prefer a ready licensed capability, prove it safely on one
   Tender, and promote or roll it back without enabling a public marketplace.
7. The Improvement Lab can demonstrate a held-out, source-backed improvement and
   cannot alter the live baseline, evaluator, Host, or security policy.
8. Arabic and English users can complete the full Tender loop on clean Windows
   installations with truthful failure and restart recovery.

No implementation work is authorized by this document alone. Each delivery layer
requires its own approved plan and verification evidence.
