# Quantix Tendering

Quantix supports a construction contractor in preparing controlled, evidence-driven tender submissions with bounded AI assistance and human approval.

## Language

**Tender**:
The contractor's controlled bid effort for one procurement opportunity, from intake of the Tender Package through approval of the Submission Package.
_Avoid_: Project, job

**Construction Project**:
The physical works, services, and contractual commitments that the client is procuring through a Tender.
_Avoid_: Tender, workspace

**Quantix**:
The open-source tender operating system defined by this repository.
_Avoid_: Context (obsolete working name)

**Quantix Application Home**:
The Tendering Engineer's single local `~/.quantix` root containing every Quantix-managed Tender Store, non-secret setting, workspace, backup, archive, operational record, and the direct ChatGPT OAuth credential file `auth.json`. Connected Tender Packages remain outside it.
_Avoid_: General provider credential store, connected project directory, arbitrary application-data folders

**Quantix Setup**:
The first-run operation that establishes the Quantix Application Home and verifies the local capabilities required to begin Tender work. It neither performs provider login nor depends on unverified system runtimes; a later direct ChatGPT connection may create `auth.json` in the Application Home.
_Avoid_: Tender intake, provider login, installer

**Application Settings**:
The application-wide, non-secret preferences and operational facts governing Quantix appearance, accessibility, notifications, the ChatGPT connection, AI Execution Selection, data and storage, updates, and diagnostics independently of any one Tender. Changes save immediately, but an AI selection change affects only future Agent Runs.
_Avoid_: Tender setting, Provider Credential, hidden configuration file

**Tendering Engineer**:
The authenticated human engineer who operates Quantix and is the sole authority for every formal approval, commitment, exception, and final output in v0.
_Avoid_: Engineer User, Tendering Manager, AI engineer, Agent Profile, passive observer

**Engineer-in-the-Loop (EITL)**:
The control principle requiring an explicit, attributable Tendering Engineer decision for every formal approval. AI agents may prepare, review, validate, and recommend, but cannot approve, infer approval from silence, or retain approval after a material change.
_Avoid_: Human-on-the-loop, automatic approval, agent approval

**Tender Office**:
The temporary organization assembled for one Tender, comprising the specialist roles needed to analyze, plan, price, review, control, and produce its submission under the Tendering Engineer's decisions.
_Avoid_: Chatbot, fixed agent list

**Tender Office Conversation**:
An attributable, Engineer-visible sequence of messages within one Tender, comprising the durable Tendering Manager conversation, the shared team room, and focused task or review threads. Each message identifies its participant and time and may link exact Tender Tasks, Agent Runs, inputs, handoffs, Evidence, and outputs; conversation never grants authority, replaces a canonical record, exposes secrets or raw provider traffic, or claims to reveal hidden model reasoning.
_Avoid_: Provider Thread, approval record, raw event stream, chain of thought

**Tendering Manager Agent**:
The Tender-scoped lead AI Agent Profile that plans, delegates, and coordinates the Tender Office's work, dependencies, milestones, reviews, consolidation, questions, and escalation within an approved Work Plan. It cannot approve, expand its own authority, commit externally, or take decisions reserved for the Tendering Engineer.
_Avoid_: Tender Office Coordinator, approval authority, autonomous approver

**AI Provider**:
A connected AI execution service that performs bounded Agent Profile work and returns operational results to Quantix. It supplies intelligence but owns no Tender state, permission, workflow transition, decision, or approval authority.
_Avoid_: Tender Office, workflow engine, system of record

**Provider Connection**:
The credential-free Quantix view of Quantix's one configured AI Provider: the Engineer's ChatGPT account. It includes the stable identity, readiness, account label when available, capability state, and compatibility with required capabilities. Quantix connects it through Quantix-owned ChatGPT OAuth; it does not support API-key connections, provider routing, or fallback.
_Avoid_: Provider Credential, model router, fallback chain

**Provider Credential**:
A Secret authenticating the ChatGPT Provider Connection. Quantix stores direct ChatGPT OAuth tokens in `<Quantix Application Home>/auth.json`. No Provider Credential enters Application Settings, a Tender Store, provider-visible context, logs, diagnostics, backups, archives, exports, or generated artifacts.
_Avoid_: Provider Connection, API key record, Tender data

**Provider Capability Catalogue**:
The credential-free model and capability facts authorized by one ready Provider Connection, including provider-qualified model identities and exact supported reasoning choices. A catalogue may be reported live by the provider or supplied as an explicitly versioned built-in contract when Quantix has no validated live discovery dependency; the direct ChatGPT adapter uses built-in catalogue `chatgpt-direct-v1`. Only the adapter's current catalogue for a ready connection may authorize a new Agent Run, while a prior catalogue may explain an unavailable earlier selection.
_Avoid_: Unversioned model list, inferred compatibility table, stale authorization

**AI Execution Selection**:
The Tender-scoped choice of the ready ChatGPT Provider Connection, one ChatGPT model, and one reasoning setting for future Agent Runs in that Tender. Application Settings holds only the default copied into a newly created Tender. Every new Agent Run captures the effective selection and its Provider Capability Catalogue provenance; changing either default or Tender selection never rewrites active, queued, interrupted, or indeterminate work, and Quantix never silently substitutes a model or reasoning setting.
_Avoid_: Bare model name, automatic fallback, per-Agent provider routing, application-wide runtime selection

**AI Provider Contract**:
The versioned Quantix definition of the mandatory lifecycle, execution, tool, sandbox, event, usage, interruption, and failure capabilities an AI Provider must satisfy. Provider-specific protocols remain behind the contract, and incompatibility blocks provider work rather than weakening a requirement.
_Avoid_: Codex protocol mirror, optional feature catalogue, provider router

**Provider Thread**:
The AI Provider's externally managed conversational context dedicated to one Agent Profile Version within one Tender and known to Quantix only by an opaque reference. It may improve continuity but remains noncanonical, carries irreversible exposure, and grants no authority through its memory.
_Avoid_: Agent Profile, Tender Store, cross-Tender memory

**Provider Turn**:
One bounded AI Provider execution for exactly one Agent Run on a Provider Thread. Reconnecting may recover that same turn, but starting another turn creates another Agent Run.
_Avoid_: Tender Task, hidden retry, whole provider thread

**Provider Turn Request**:
The immutable execution envelope binding a Provider Turn to its Agent Run, exact Provider Connection, provider-qualified model, provider-native reasoning setting, Provider Thread, exact Agent Profile Version instructions, Tender Task objective, Data Views, output contract, Typed Tools, Permission Grant-derived constraints, resource budget, and required language.
_Avoid_: Free-form prompt, provider thread memory, mutable run configuration

**Provider Instruction Bundle**:
The ordered, versioned controls within a Provider Turn Request, comprising Quantix invariants, exact Agent Profile instructions, the Tender Task objective, input manifest, output and tool contracts, and escalation rules. Supplied Tender content remains untrusted data and cannot become an instruction source.
_Avoid_: System prompt, thread memory, document instructions

**Provider Turn Result**:
The normalized terminal outcome of one Provider Turn as Completed, Interrupted, Failed, or Indeterminate, including structured candidate output, staged-output manifest, usage, error, and opaque provider references. It is Agent Run evidence rather than a verified record or Artifact Version.
_Avoid_: Approved answer, Artifact Version, chat response

**Provider Control Request**:
An AI Provider's uniquely correlated request for the host to permit a tool, command, file, network, or user-input action during a Provider Turn. It grants nothing, is never an EITL Approval Gate or Access Request, and is answered only when Quantix independently proves the action already fits the current Permission Grant.
_Avoid_: Access Request, Approval Gate, provider-granted permission

**Provider Event**:
An attributable, normalized operational fact in a monotonic Agent Run sequence, such as a state change, Typed Tool call, file change, usage update, warning, interruption, or redacted failure. Delivery gaps remain explicit; raw protocol traffic, streamed deltas, credentials, and hidden reasoning are not Provider Events.
_Avoid_: Audit Event, chat transcript, raw provider payload

**Provider Failure**:
The normalized explanation of why a Provider Connection or Provider Turn could not proceed, including its stable category, retry safety, required user action, and redacted provider detail. Quantix workflow never depends directly on a provider-specific error code.
_Avoid_: Raw exception, agent explanation, Review Finding

**Waiting for AI Provider**:
The non-failure work state used when an exact selected Provider Connection, model, reasoning setting, credential, quota, or mandatory capability is temporarily unavailable before a Provider Turn is accepted. Tender records remain usable, Quantix may resume automatically when the same selection becomes ready, and changing the bound selection requires an explicit Tendering Engineer decision.
_Avoid_: Failed Agent Run, silent fallback, unavailable Tender

**Indeterminate Agent Run**:
An Agent Run whose Provider Turn outcome cannot be established after connection loss or failed interruption. Its workspace and partial outputs remain quarantined and its Tender Task remains Blocked until an attributable recovery decision starts a separate run or closes the uncertainty.
_Avoid_: Failed run, silent retry, successful partial result

**Provider Usage**:
The available attributable consumption and capacity information for a Provider Turn or Provider Connection, including token, context-window, elapsed-time, and rate-limit observations without inferred monetary cost. Missing measurements remain unknown rather than zero.
_Avoid_: API invoice, estimated subscription cost, resource budget

**Capability**:
A named Tender Office competence required by policy or the verified needs of a Tender and carried by one or more Agent Profiles. A Capability does not itself own a provider thread, Tender Task, permission, or approval authority.
_Avoid_: Agent Profile, job title, task

**Capability Demand**:
A project-specific need for a Capability classified as Policy-required, Tender-required, Risk-recommended, Manager-added, or a Capability Gap, with its triggering policy or Evidence and rationale. Policy-required and Tender-required demands are deterministic for the same exact Project Fingerprint, Capability Catalogue, and policy versions.
_Avoid_: Unexplained role suggestion, staffing guess

**Capability Catalogue**:
The versioned set of Capabilities Quantix can safely activate, including each Capability's supported tools, data scopes, output contracts, qualification constraints, and review requirements. Team Composer may specialize approved components into project-specific roles but cannot invent unsupported authority.
_Avoid_: Prompt library, unrestricted role generator

**Capability Gap**:
A visible record that a Tender requires competence, tooling, or qualified review not supported by the current Capability Catalogue. Affected work remains Blocked until the gap receives an EITL-approved external, limited, or newly supported treatment.
_Avoid_: Fictional expert, silent limitation

**Agent Profile**:
The Tender-scoped operational definition of one coherent Tender Office role, including its stable human-readable identity, discipline, seniority, capabilities, objective, behavioral work controls, permissions, constraints, output contract, review requirements, and resource budget. It may combine compatible Capabilities, but independence, permission, qualification, or workload boundaries require separate profiles.
_Avoid_: Fictional personality, character prompt, one all-purpose agent

**Agent Profile Version**:
An immutable revision of an Agent Profile's identity, instructions, Capabilities, permissions, constraints, output and review contracts, and resource budget. Tender Tasks and Agent Runs bind the exact version; a material profile change creates a proposed new version rather than altering active history.
_Avoid_: Mutable system prompt, latest agent

**Profile Status**:
The activation state of an Agent Profile: Proposed, Active, Suspended, or Retired. It is distinct from Verification Status; suspension stops new work without replacing the profile, while retirement preserves its registered history and archives its provider thread.
_Avoid_: Task state, verification state, deleted agent

**Role Archetype**:
A reusable approved set of defaults from which Team Composer may create a Tender-scoped Agent Profile. It carries no provider thread, Tender Task ownership, project data access, or approval authority on its own.
_Avoid_: Persistent cross-Tender agent, active employee

**Permission Policy**:
The versioned, Quantix-owned default-deny rules that decide whether an Agent Profile may access exact Tender data or use a tool for controlled work. Prompts and provider threads grant no authority; every denial fails closed and is auditable.
_Avoid_: Prompt permission, model-enforced security

**Permission Grant**:
An immutable, short-lived authorization for one Agent Run, binding an exact Agent Profile Version, Tender Task, Work Plan version, Data Scopes, Data Classifications, record versions or resolved record set, permitted actions, approved tools, private workspace, quotas, purpose, and expiry. Every operation is rechecked against current policy and state; a grant cannot be delegated, inherited, reused for another task, or expanded by a provider thread.
_Avoid_: Role-wide session access, prompt-granted permission

**Access Request**:
A structured request raised when a Tender Task cannot proceed within its current Permission Grant, identifying the exact additional data, action, tool, purpose, duration, and risk. It grants nothing and leaves affected work Blocked until denied, approved, or superseded.
_Avoid_: Agent self-expansion, informal permission request

**Access Approval**:
The EITL decision granting one exact, expiring access expansion within an already approved Agent Profile and Work Plan ceiling. Recurring or ceiling-changing access requires a Work Plan Amendment, and Secret data is never eligible.
_Avoid_: Permanent exception, secret disclosure

**Prohibited Action**:
An invariant operation no Agent Profile can receive, including access to Secret data, direct Tender Store or security-control mutation, cross-workspace access, untrusted code execution, autonomous external action, approval, or concealment of audit history. The Tendering Engineer may perform separately authorized workflow actions but cannot convert a Prohibited Action into agent authority.
_Avoid_: Overridable deny, high-risk permission

**Data Scope**:
A named business compartment of Tender or company information used to constrain access independently of its sensitivity, such as tender sources, commercial estimate, commercial markup, supplier quotes, legal advice, or company CVs. Access to one Data Scope never implies access to another.
_Avoid_: Folder permission, broad sensitive-data access

**Data Classification**:
The handling level attached to data independently of its Data Scope: Tender Internal, Sensitive, or Secret. Secret data such as credentials, tokens, encryption keys, signing material, and portal secrets is never model-visible or copied into provider threads, sandboxes, artifacts, or logs.
_Avoid_: Data Scope, public-by-default, model-visible credential

**Declassification**:
The controlled reduction of a registered output's inherited Data Classification after redaction, preserved provenance, independent verification, and any material EITL approval. An Agent Profile cannot declassify its own output or erase the classification of its inputs.
_Avoid_: Agent redaction claim, automatic sensitivity reduction

**Data View**:
A task-specific representation of exact registered data limited to the fields, rows, and content permitted by a Permission Grant. It preserves provenance and classification; redacted or derived content never silently replaces the authoritative Source Artifact Version used as Evidence.
_Avoid_: Full-record access, untracked redaction

**Agent Run Workspace**:
The fresh, private, disposable filesystem made available to one Agent Run, containing read-only exact inputs, a writable working area, and an output staging area under its Permission Grant. It has no direct access to the Tender Store, original Tender Package location, repository, user directories, operating-system secrets, or any other run workspace.
_Avoid_: Shared agent folder, Tender Store, persistent profile workspace

**Thread Exposure Set**:
The cumulative Data Scopes, Data Classifications, and exact inputs disclosed to one provider thread. Exposure is irreversible; a thread may be reused only when its prior exposure is compatible with the next Tender Task and output destination, otherwise Quantix archives it and starts a fresh thread.
_Avoid_: Current permission, erasable model memory

**Typed Tool**:
A versioned, host-controlled operation with defined Capability, input and output schemas, required Data Scopes and Data Classifications, side-effect class, resource limits, and audit behavior. An Agent Profile may invoke it only through an exact Permission Grant; arbitrary shells, executables, package installation, application control, and tool discovery are not Typed Tools.
_Avoid_: Unrestricted shell, arbitrary MCP tool, prompt-defined function

**Safety Limit**:
A non-overridable bound on input size, expansion, nesting, duration, memory, output, or storage consumption that protects the Tender and Tendering Engineer's device from unsafe work. Crossing it blocks the operation and cannot be converted into permission or approval.
_Avoid_: Resource budget, Permission Grant, manager override

**Team Composer**:
The controlled composition authority that maps verified Project Fingerprint signals and mandatory policy to Capability Demands, proposed Agent Profiles, Tender Tasks, reviewer assignments, and constraints for a Work Plan. It may use AI recommendations but cannot activate an Agent Profile or approve its own proposal.
_Avoid_: Autonomous staffing agent, fixed team list

**Bootstrap Team**:
The fixed restricted set of Tendering Manager Agent, Document Controller, Tender Analyst, and Independent Reviewer Agent Profiles authorized when the Tendering Engineer creates a Tender. It may register and analyze the Tender Package, open the Query Register, and propose the Bid Decision Package and full Work Plan, but cannot perform Active Production or external actions.
_Avoid_: Full Tender Office, approved Work Plan

**Document Controller**:
The Agent Profile responsible for Source Artifact registration, revision and addendum control, controlled distribution, the Query Register record, and Submission Package document control. It preserves provenance and status without deciding technical, commercial, or contractual meaning.
_Avoid_: File uploader, technical reviewer

**Tender Analyst**:
The Agent Profile responsible for evidence-linked Tender requirements, deadlines, evaluation criteria, compliance conditions, information gaps, and Project Fingerprint inputs. It analyzes and proposes but does not make Tender Decisions or approve compliance.
_Avoid_: Tendering Manager Agent, general-purpose analyst

**Cost Estimator**:
The Agent Profile responsible for developing evidence-linked quantities, rate inputs, quotations, cost build-ups, and Calculation Scenarios through the Calculation Engine. It cannot approve its own estimate, choose margin, or determine the Approved Tender Price.
_Avoid_: LLM calculator, pricing authority

**Independent Reviewer**:
The mandatory Agent Profile that reviews exact work produced by other profiles, raises Review Findings, and verifies evidence and output contracts without editing the reviewed target. Specialist work requires a separate reviewer with the relevant Capability when this profile is not qualified.
_Avoid_: Author self-check, AI approver

**Tender Package**:
The complete project directory supplied for a Tender, either as a connected directory or a compressed archive, containing every source artifact the Tender Office must register and assess.
_Avoid_: Single prompt attachment

**Connected Tender Package**:
The external directory or compressed archive selected as intake input. Quantix records its provenance and copies every registered source version into the Tender Store, after which the connection may disappear without invalidating canonical Tender work.
_Avoid_: Tender Store, live synchronized folder, editable source of truth

**Tender Store**:
The self-contained Quantix-managed authoritative collection of one Tender's records, retained revisions, and registered content under the Quantix Application Home. It can be verified, backed up, archived, transferred, or explicitly deleted as one unit; external inputs and working material never become the system of record by location alone.
_Avoid_: Chat history, live source directory, agent workspace

**Content Object**:
An immutable byte sequence copied into a Tender Store, addressed and reverified by its cryptographic digest. Source Artifact Versions and Artifact Versions reference Content Objects while preserving their distinct logical identities, provenance, filenames, and revision histories.
_Avoid_: Source Artifact, Artifact Version, mutable file, external path

**Tender Backup**:
A verified restorable copy of one complete Tender Store retained for recovery. It is neither a Submission Package nor evidence that any material was delivered externally.
_Avoid_: Portable Tender Archive, Delivery Export, live Tender Store

**Tender Recovery**:
The EITL-controlled replacement of an existing Tender Store from a verified Tender Backup after corruption, loss, or another declared recovery need. It never merges stores or silently overwrites an existing Tender.
_Avoid_: Portable Tender Archive import, automatic rollback, record-level correction

**Recovery Required**:
The read-only Tender state entered when the integrity of its Tender Store cannot be established. Canonical work, approvals, backups represented as valid, and Submission Package release remain blocked until Tender Recovery or explicit whole-Tender purge.
_Avoid_: Blocked task, Impact Assessment, automatic repair

**Tender Integrity Verification**:
A deterministic examination of one Tender Store's records, Audit Events, and registered content for internal consistency and digest agreement. It changes no Tender content, and any failure places the Tender in Recovery Required.
_Avoid_: Artifact review, Tender Recovery, confidence score

**Portable Tender Archive**:
A verified, self-contained transfer copy of one complete Tender Store that can be moved and restored without its original Quantix installation. It is distinct from an internal Tender Backup and from a client-facing Delivery Export.
_Avoid_: Tender Backup, Submission Package, Delivery Export, compressed Tender Package

**Archived Tender**:
A complete Tender placed in a reversible read-only lifecycle state after protected work reaches a safe terminal boundary. Its Tender Store and history remain intact, it opens through the ordinary Manager, Work, and Files surfaces with an Archived status, and it may return to active use through an attributable Tendering Engineer decision.
_Avoid_: Tender Backup, Portable Tender Archive, deleted Tender

**Tender Trash**:
The recoverable holding state for a complete Tender Store after an approved deletion request at a safe terminal boundary. Restoration and Permanent Tender Deletion are separate attributable Tendering Engineer decisions, and no automatic purge occurs.
_Avoid_: Archived Tender, operating-system recycle bin, permanent deletion

**Permanent Tender Deletion**:
The irreversible Tendering Engineer decision available only for a Tender in Tender Trash that removes every identifiable Tender-associated copy controlled by Quantix, including its Store, backups, portable archives, exports, run workspaces, staging, quarantine, and Tender-specific logs, while retaining only a Deletion Receipt. It neither claims to erase copies outside Quantix's control nor waits for provider-side context deletion before completing local deletion.
_Avoid_: Archive, move to Tender Trash, external-copy erasure promise

**Provider Cleanup Pending**:
The post-deletion status recorded when Permanent Tender Deletion has completed locally but deletion of one or more opaque Provider Threads has not yet been confirmed. Quantix retries that cleanup automatically without retaining Tender content or blocking the local deletion result.
_Avoid_: Recoverable Tender, failed local purge, credential revocation

**Deletion Receipt**:
The minimal installation-level record proving that an identified Tender underwent Permanent Tender Deletion through an attributable Tendering Engineer decision, including whether provider cleanup remains pending but none of the Tender's confidential content.
_Avoid_: Tender Backup, Audit Event, recoverable tombstone

**Source Artifact**:
The stable logical identity of a supplied Tender file across confirmed revisions. Files with uncertain revision relationships remain separate Source Artifacts until resolved.
_Avoid_: File path, content blob, editable working copy

**Source Artifact Version**:
An immutable, hashed capture of one Source Artifact at a particular revision, including its origin and package-relative path. Addenda, replacements, and changed source files create new versions rather than overwriting prior bytes.
_Avoid_: Latest file, live external file

**Tender Task**:
A controlled unit of Tender Office work with an owner Agent Profile version, objective, exact registered inputs, dependencies, deadline, output contract, review policy, permissions, resource budget, state, Agent Runs, and registered outputs. Only the Quantix workflow transitions it, and completion requires validated outputs, satisfied reviews, resolved dependencies, and required approvals.
_Avoid_: Informal chat request

**Working Artifact**:
Mutable, disposable material inside an Agent Run Workspace that has not passed a Tender Task's output contract and is not part of the Tender Store.
_Avoid_: Artifact Version, approved draft, system-of-record file

**Recovery Quarantine**:
The protected holding state for an indeterminate or failed Agent Run's workspace and candidate outputs until the Tendering Engineer resolves their disposition. Quarantined material is not canonical and cannot support downstream work or approval.
_Avoid_: Tender Store, completed Agent Run, automatic retry queue

**Artifact**:
The stable logical identity of a controlled Tender Office output across its registered versions.
_Avoid_: Working file, individual version

**Artifact Version**:
An immutable, hashed version of an Artifact registered after validating its Tender Task output contract, with inherited Data Scopes and Data Classification and provenance linking its exact inputs, producing task, Agent Profile, Agent Run, tools, template, timestamp, and parent version.
_Avoid_: Working Artifact, mutable draft, latest file

**Agent Run**:
The immutable execution trace connecting one Tender Task and Agent Profile version to its Permission Grant, registered inputs and Data Views, captured AI Execution Selection and catalogue provenance, exactly one Provider Turn, provider thread and exposure, approved instructions, attributable messages and handoffs, workspace manifest, Typed Tool calls, usage, outcome, errors, and produced Artifact Versions. It preserves auditable user-visible activity but not secrets, raw provider traffic, or hidden model reasoning.
_Avoid_: Chat as system of record, chain of thought

**Audit Event**:
An append-only, tamper-evident record of a material creation, revision, transition, access, decision, correction, denial, or failure in the Tender Store. Its per-Tender sequence and before/after references explain canonical history, but Audit Events are not replayed to reconstruct current Tender state.
_Avoid_: Editable log entry, chat transcript, event-sourced state

**Verification Status**:
The trust state of an evidence-bearing record: Proposed, Verified, Rejected, Stale, or Superseded. Registration proves structural validity and provenance, while verification determines whether the record may support controlled Tender work or an approval.
_Avoid_: Confidence score, approval status, database presence

**Stale**:
The status of a previously usable record, Review, or output whose exact dependency has materially changed and which must be revalidated before current use. Stale history remains preserved.
_Avoid_: Deleted, automatically corrected, failed

**Provenance Link**:
A typed dependency from a derived record or Artifact Version to an exact input revision or version, used to explain its origin and calculate targeted invalidation.
_Avoid_: Generic backlink, chat reference

**Named Version Reference**:
A workflow-controlled pointer such as Current Candidate, Reviewed Version, Approved Version, or Submission Version that resolves to one exact immutable Artifact Version. Moving the pointer is audited and never changes the version bound to an existing review or approval.
_Avoid_: Latest file, timestamp-selected version

**Intake**:
The first Tender state, in which Quantix inventories the connected Tender Package, records unreadable or unsupported Source Artifacts, identifies revisions and addenda, and proposes deadline candidates.
_Avoid_: File upload

**Bid Decision**:
The mandatory Approval Gate at which the Tendering Engineer chooses Proceed, Hold, or Decline before the full Tender Office begins production work.
_Avoid_: Agent go/no-go decision

**Project Fingerprint**:
The structured description of the Construction Project, procurement context, disciplines, deliverables, complexity, sensitivity, and required Tender Office capabilities derived during Fingerprinting.
_Avoid_: Tender summary, Agent Profile

**Bid Decision Package**:
The versioned, evidence-linked information presented to the Tendering Engineer for a Bid Decision, including compliance, exposure, capacity, deadline viability, specialist needs, information gaps, and a recommendation.
_Avoid_: Agent recommendation alone

**Work Plan**:
The versioned organization of Agent Profiles, Workstreams, Tender Tasks, responsibilities, internal milestones, reviews, resource budgets, and data scopes approved by the Tendering Engineer for one Tender.
_Avoid_: Chat plan, agent-generated to-do list

**Work Plan Proposal**:
A versioned candidate Work Plan that the Tendering Engineer may add to, remove from, split, combine, rename, or adjust within Quantix invariants before approval. Each edit produces a newly validated proposal version and has no production authority until Work Plan Approval.
_Avoid_: Partially approved team, mutable approved plan

**Work Plan Approval**:
The Approval Gate after a Proceed Bid Decision at which the Tendering Engineer approves, returns, or holds one exact Work Plan version. Only an approved version may activate the full production team and authorize Active Production.
_Avoid_: Partial team activation, agent staffing decision

**Work Plan Amendment**:
The EITL-controlled replacement of an approved Work Plan when team membership, Agent Profile Versions, capability coverage, permissions, resource envelopes, critical responsibilities, milestones, or reviewer independence must materially change. Routine coordination inside the approved envelope is audited but does not require an amendment.
_Avoid_: Silent team change, routine task scheduling

**Submission Deadline**:
The exact manager-approved timestamp and time zone for delivering the Submission Package, linked to its Evidence and superseded only through a controlled addendum change.
_Avoid_: Extracted date candidate, internal milestone

**Workstream**:
A concurrent area of related Tender Tasks performed during Active Production, such as technical, planning, commercial, procurement, contracts, or assurance work.
_Avoid_: Lifecycle state, independent project

**Cost Estimating Workstream**:
The mandatory Active Production Workstream that develops quantities, rate build-ups, quotations, direct and indirect costs, risk provisions, and pricing scenarios through deterministic calculations and independent review.
_Avoid_: LLM arithmetic, one-step price generation, final price decision

**Calculation Engine**:
The Quantix authority that validates exact numeric inputs, dimensions, units, currencies, rule versions, precision, and policy before computing and registering canonical calculated values. Agent Profiles may prepare inputs, request scenarios, and explain results but cannot supply authoritative arithmetic.
_Avoid_: LLM calculator, spreadsheet as system of record, agent-generated total

**Calculation**:
The stable logical identity of a derived numeric conclusion across its deterministic executions.
_Avoid_: Spreadsheet cell, prose total, individual execution

**Calculation Run**:
An immutable, hashed execution of an exact Calculation Rule version against exact input revisions, units, currencies, policies, scenario, and engine version. It preserves unrounded and contractual results, validation, provenance, and review state and inherits unresolved trust conditions from its inputs.
_Avoid_: Recalculated cell, overwritten result, LLM arithmetic

**Calculation Input**:
A typed numeric value admitted from a Verified canonical record, EITL-approved Assumption, non-Stale Calculation Run, approved scenario parameter, or attributable Tendering Engineer entry. Missing, blank, unavailable, not-applicable, and explicit zero remain distinct.
_Avoid_: Prompt number, implicit zero, unsourced spreadsheet value

**Calculation Rule**:
A versioned, deterministic formula and its applicability, input and output dimensions, validation, precision, rounding, and tests. Engineering rules also identify their governing standard and edition and applicability limits; every new or changed rule remains unavailable to Tender work until tested, independently reviewed, and approved through EITL, and retired versions remain historical only.
_Avoid_: Prompt formula, unreviewed spreadsheet formula, free-form arithmetic

**Calculation Scenario**:
A named, versioned alternative comprising exact Calculation Inputs, rules, policies, and an optional parent scenario. Scenario comparison is deterministic, and only an EITL decision may promote one into an approved baseline or Tender Price.
_Avoid_: Overwritten estimate, informal what-if, hidden option

**Calculation Adjustment**:
An explicit amount or factor introduced as a reviewed Calculation Input with its unit or currency, reason, scope, provenance, affected Calculation, and EITL approval. It replaces direct overrides and hidden balancing values.
_Avoid_: Manual total override, buried allowance, plug

**Calculation Manifest**:
The versioned dependency graph of exact Calculation Runs, inputs, rules, intermediate values, units, Exchange Rates, Rounding Policies, adjustments, assumptions, scenario, engine version, results, and hashes needed to reproduce and verify an approved numeric baseline.
_Avoid_: Spreadsheet total, current calculation state, narrative summary

**Exchange Rate**:
A versioned, evidence-linked currency conversion input recording its pair, direction, exact value, effective date, pricing date, rate type, source, and approval status. Original and converted monetary amounts remain visible together.
_Avoid_: Current rate, inferred currency, overwritten quote amount

**Rounding Policy**:
The versioned rules specifying where, how, and to what scale a quantity, rate, percentage, tax, currency amount, line value, or total is rounded. Calculation Runs retain their unrounded values and every applied contractual or display value.
_Avoid_: Default formatting, hidden precision loss, balancing adjustment

**Engineering Calculation**:
A design- or safety-related calculated result produced by an approved discipline-specific Calculation Rule or verified external engineering tool. Without either, Quantix may register an external result as Evidence but cannot substitute AI-generated arithmetic.
_Avoid_: AI engineering answer, unchecked formula, narrative estimate

**Query and RFI Control Workstream**:
The mandatory cross-lifecycle Workstream that registers, triages, coordinates, issues, tracks, interprets, and resolves Tender queries from Intake through Final Review under EITL control.
_Avoid_: One-time clarification phase, informal chat question, autonomous external communication

**Tender Query**:
An evidence-linked request for information, interpretation, or decision raised because a Tender requirement, scope item, document, quantity, responsibility, or submission instruction is missing, ambiguous, or contradictory.
_Avoid_: Unsupported agent assumption, untracked question

**External RFI**:
A coordinated Tender Query addressed to the employer, consultant, or authorized tender contact that requires EITL approval before issue and whose response is registered as a versioned Source Artifact.
_Avoid_: Internal task comment, automatically sent message

**Query Register**:
The controlled record of every Tender Query and External RFI, including evidence, ownership, affected work, priority, deadlines, status, issuance, responses, impact assessments, and approved resolution.
_Avoid_: Email inbox, informal question list

**Approved Query Treatment**:
The EITL decision that controls an unanswered or partially answered material Tender Query through an assumption, allowance, contingency, qualification, exclusion, exception, hold, or other explicit disposition.
_Avoid_: Silent assumption, agent closure

**Basis of Estimate**:
The versioned, evidence-linked definition of the scope, documents, pricing date, currencies, taxes, productivity assumptions, rate sources, escalation, design maturity, gaps, and allowances governing an estimate.
_Avoid_: Unstated assumptions, estimator prompt

**Cost Breakdown Structure**:
The controlled hierarchy linking Tender requirements and scope to work packages, cost codes, quantities, rate build-ups, and estimate lines.
_Avoid_: Unmapped spreadsheet rows, employer BOQ alone

**Priced Cost Baseline**:
The independently reviewed and Tendering Engineer-approved expected cost of delivering the Construction Project, including direct costs, indirect costs, explicit allowances, and approved risk provisions, bound to an exact Calculation Manifest before final commercial pricing decisions.
_Avoid_: Tender Price, unreviewed estimate

**Approved Tender Price**:
The immutable, versioned customer-facing price and Calculation Manifest approved by the Tendering Engineer after considering the Priced Cost Baseline, risk provision, overhead, financing, profit, discounts, and commercial adjustments.
_Avoid_: Expected cost, agent-selected price, editable total

**Ready for Integration**:
The Workstream status reached when its output contract validates, required Evidence and reviews exist, dependencies are resolved, assumptions and exceptions are explicit, and no critical blocker remains.
_Avoid_: Draft complete, agent finished

**Review**:
An independent assessment of an exact Artifact Version, record revision, calculation baseline, or Submission Package version under a versioned review policy. A changed target makes the Review historical and requires the affected checks to run again.
_Avoid_: Author self-check, review of latest

**Review Finding**:
An immutable problem statement raised by a Review, classified as Critical, Major, or Minor. Its disposition is appended and may close it only through verified correction, permitted Exception Approval, or explicit supersession by someone other than its author.
_Avoid_: Editable feedback, author self-closure

**Exception Approval**:
The Approval Gate through which the Tendering Engineer knowingly accepts a Major Review Finding or another explicit departure that the workflow permits to continue.
_Avoid_: Silent waiver, agent acceptance

**Coordinated Bid Baseline**:
The versioned, reconciled set of technical, programme, commercial, procurement, contractual, risk, and submission commitments approved before Package Production.
_Avoid_: Collection of latest drafts, unreviewed bid

**Baseline Approval**:
The Approval Gate at which the Tendering Engineer accepts the Coordinated Bid Baseline after Integrated Review.
_Avoid_: Workstream completion, agent consolidation

**Package Production**:
The controlled transformation of the approved Coordinated Bid Baseline into the files, forms, schedules, appendices, envelopes, and naming structure required for the Submission Package.
_Avoid_: New drafting phase, commitment change

**Final Review**:
The independent verification that the produced Submission Package matches the Coordinated Bid Baseline, covers every mandatory requirement and addendum, preserves information boundaries, and satisfies its manifest and release rules.
_Avoid_: Production self-check, manager file-by-file drafting review

**Final Review Plan**:
The versioned assignment of independent, qualified reviewers and exact review scopes for one Submission Package Version, derived from its Submission Sections, Capability requirements, risks, and Package Validation Policy. A coordinator may consolidate specialist results but cannot edit reviewed files, close their own findings, or override a specialist finding.
_Avoid_: One generic sign-off, producer self-review, informal review request

**Release Readiness Report**:
The evidence-linked decision view presented to the Tendering Engineer for Final Approval, identifying the exact package and manifest hashes, approved baselines, coverage, validations, execution, query treatments, assumptions, qualifications, exclusions, departures, findings, exceptions, information-boundary results, deadline, and changes since the reviewed version. It summarizes but does not replace its underlying records.
_Avoid_: Agent recommendation, unlinked dashboard score, approval record

**Final Approval**:
The atomic Approval Gate at which the Tendering Engineer reviews the Release Readiness Report and either returns the Submission Package for correction or freezes one exact immutable package version and manifest with zero open Critical Review Findings, only permitted Major Exception Approvals, and disclosed Minor findings. The transaction rechecks identity, deadline, addenda, staleness, hashes, validation, independent review, and exceptions; any later content change creates a new version, revokes approval, and requires Final Review again.
_Avoid_: External submission, editable approval, agent release decision

**Evidence**:
A structured link from a canonical claim or record field to an exact Source Artifact Version and typed location, preserving the original excerpt, extraction provenance, confidence, verification status, and any non-authoritative translation. New source versions leave historical Evidence intact but can make it stale for current work.
_Avoid_: Unsupported assertion, generic file citation, translated text as authority

**Semantic Evidence Search**:
A local, similarity-threshold retrieval over immutable Evidence locations. It returns exact Source Artifact Version and typed-location provenance with each ranked match; similarity is discovery support only and never changes Evidence authority, verification, or currentness.
_Avoid_: Autonomous fact selection, semantic result as Evidence, replacement for exact search

**Assumption**:
A first-class record of an unproven proposition needed for Tender work, including its evidence gap, owner, affected work, proposed treatment, confidence, status, and EITL decision when material. Approval permits controlled reliance but never converts an Assumption into a fact.
_Avoid_: Hidden premise, unsupported Evidence, approved fact

**Tender Decision**:
The canonical record of a formal Tendering Engineer judgment, including the question, options, outcome, rationale, Evidence, affected exact records, conditions, expiry, identity, timestamp, and any superseded decision.
_Avoid_: Chat answer, agent recommendation, Audit Event

**Approval Record**:
The immutable result of an Approval Gate, binding an explicit Tendering Engineer outcome to exact record revisions, artifact versions, hashes, Evidence, conditions, and exceptions. Material dependency changes invalidate rather than erase it.
_Avoid_: Chat confirmation, approval of latest, editable sign-off

**Submission Package**:
The stable logical identity of the controlled tender submission across its immutable Submission Package Versions. Only one exact version that passes Final Review and receives Final Approval may become Ready for Submission.
_Avoid_: Export folder, agent draft, collection of latest files

**Submission Package Version**:
An immutable domain object binding one approved Coordinated Bid Baseline to an exact Submission Manifest, required Submission Sections, and the content digests of every included file. Its root digest derives from the canonical manifest; any changed commitment invalidates affected baseline approval and returns the work through review.
_Avoid_: Mutable package folder, approved latest, archive as system of record

**Submission Manifest**:
The immutable, deterministically generated structured record for one Submission Package Version. It binds the approved Coordinated Bid Baseline and Calculation Manifest, Submission Deadline, Submission Sections, every item and its exact release properties and content digest, coverage, required execution, validation, Reviews, permitted exceptions, and approvals.
_Avoid_: Hand-edited file list, archive index, unhashed release note

**Submission Section**:
A typed part of a Submission Package Version representing a Tender-required envelope, volume, folder, language, alternative, or upload group, with deterministic membership and information-separation rules derived from verified requirements rather than a fixed technical-commercial layout.
_Avoid_: Arbitrary folder, hard-coded envelope structure

**Submission Manifest Item**:
The entry for one included file, binding its required purpose and package location to an exact approved Artifact Version or explicitly required unchanged Source Artifact Version, together with provenance, requirement coverage, Data Classification, and content digest. An unregistered file cannot enter a Submission Package Version.
_Avoid_: Loose file, path-only manifest row, mutable attachment

**Submission Coverage Matrix**:
The versioned proof that every mandatory requirement, deliverable, addendum instruction, form field, signature, evaluation response, and required file has an exact package item and location or an Evidence-supported permitted disposition. Final Approval requires complete disposition, and omission is never an implicit disposition.
_Avoid_: Informal checklist, percentage-only completeness score, silent omission

**Manual Verification**:
A controlled verification of one exact file hash against a versioned checklist by the Tendering Engineer or a qualified independent reviewer when automated inspection cannot establish a required property. It records verifier identity, capability, checks, Evidence, result, limitations, and timestamp; it is not an exception and cannot make an unverifiable mandatory file releasable.
_Avoid_: Visual glance, blanket waiver, self-attested agent output

**Package Validation Policy**:
The versioned union of Quantix release invariants and verified Tender-specific submission rules defining the deterministic, independent, and manual checks, severities, permitted dispositions, and release conditions for a Submission Package. AI recommendations cannot change the policy or declare a check passed.
_Avoid_: Prompt checklist, reviewer preference, package-specific waiver

**Package Validation Run**:
An immutable execution of one exact Package Validation Policy against one exact Submission Package Version and its files. It records validator and renderer versions, deterministic results, Manual Verifications, discovered inconsistencies, information-boundary checks, Review Findings, timestamps, and hashes. Every package version receives a new run; an item result may be reused only for the identical file hash, check version, policy, and context, while package-wide checks always rerun.
_Avoid_: Mutable validation status, agent confidence, validation of latest

**Release Copy**:
A Tender-shaped folder, archive, or split volume deterministically exported from one frozen Submission Package Version, reopened, and verified against its Submission Manifest. It is a reproducible delivery copy rather than the Tender system of record; later external edits make only that copy unverified, and producing it does not mean Quantix submitted it externally.
_Avoid_: Submission Package Version, submitted package, live export folder

**Acceptance Tender Fixture**:
The exact versioned, fully synthetic bilingual Egyptian building Tender Package, synthetic contractor library, and machine-readable Acceptance Oracle used to qualify Quantix v0. Its public contents are dedicated under CC0-1.0; authentic FIDIC text and real-company material belong only to a separate private licensed calibration lane.
_Avoid_: Customer Tender, demonstration folder, mutable sample data

**Acceptance Oracle**:
The machine-readable expected requirements, classifications, deadlines, addenda, forms, BOQ rows, calculations, evidence links, permitted assumptions, workflow outcomes, and package properties for one exact Acceptance Tender Fixture version.
_Avoid_: Agent answer key, prose checklist, mutable expected output

**Product Acceptance Run**:
An immutable execution of one exact product acceptance suite against an exact Quantix release candidate, Acceptance Tender Fixture, dependency and provider versions, and native platform. It records deterministic results, live-provider evaluations when required, safety and recovery results, metrics, timings, findings, artifacts, and hashes outside every Tender Store.
_Avoid_: Package Validation Run, ordinary test log, Tender approval

**Product Acceptance Record**:
The immutable aggregate of the Product Acceptance Runs and release evidence required for one Quantix release candidate, including fixture and binary hashes, application, AI Provider adapter and catalogue versions, OCR versions, platform results, evaluation metrics, known non-blocking findings, approved exceptions, and attributable release approval. It qualifies software; it does not approve a Tender or Submission Package.
_Avoid_: Release Readiness Report, Final Approval, CI dashboard

**Private v0 Qualification**:
The product gate allowing an engineer-operated, non-public Quantix v0 to proceed after deterministic verification, five consecutive qualifying live-provider runs through the direct ChatGPT connection, and a full packaged Windows 11 end-to-end Product Acceptance Run. It grants no public-production claim.
_Avoid_: Public Release Gate, prototype success, production support

**Public Release Gate**:
The additional product gate that blocks public Quantix distribution until the same native packaged acceptance passes on Windows 11 x64, macOS 14+ Apple Silicon, and Ubuntu 24.04 x64, and the direct ChatGPT integration has production assurance and terms permitting its intended third-party subscription-backed use. Technical risk acceptance cannot waive contractual authorization.
_Avoid_: Private v0 Qualification, Tender Final Approval, assumed entitlement

**Approval Gate**:
A workflow boundary that cannot advance until the authenticated Tendering Engineer explicitly accepts, returns, or rejects the specified proposal, commitment, exception, or output. The approval records the engineer identity, timestamp, decision, object versions and hashes, evidence, comments, conditions, exceptions, and history.
_Avoid_: Agent self-approval, informal chat confirmation

**Ready for Submission**:
The successful terminal Tender state reached when the validated Submission Package has received the Tendering Engineer's final approval.
_Avoid_: Submitted, complete

**Declined**:
The terminal Tender state produced when the Tendering Engineer chooses Decline at the Bid Decision.
_Avoid_: Failed, cancelled

**Withdrawn**:
The terminal Tender state produced when the Tendering Engineer stops a Tender after previously choosing Proceed.
_Avoid_: Declined, failed

**Expired**:
The terminal Tender state produced when the Submission Deadline passes before the Submission Package reaches Ready for Submission.
_Avoid_: Failed, timed-out task

**Change Assessment**:
A temporary Tender state entered when an addendum or revision may invalidate requirements, Tender Tasks, artifacts, deadlines, decisions, or approvals.
_Avoid_: Full restart, silent update

**Blocked**:
A status applied while a Tender cannot advance because a Tender Task, Source Artifact, decision, or required exception resolution is outstanding. Blocked is not a lifecycle state or terminal outcome.
_Avoid_: Failed
