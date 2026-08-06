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

**Tendering Manager**:
The contractor-side decision authority accountable for approving the tender office's plans, commitments, exceptions, and final outputs. In the single-user v0, the authenticated Engineer User fills this role. The Tendering Manager does not perform routine analysis, drafting, document control, or production work.
_Avoid_: Tender analyst, proposal writer, chatbot operator

**Engineer User**:
The authenticated human engineer who operates Quantix, acts as the Tendering Manager in v0, and is the sole authority for every formal approval.
_Avoid_: AI engineer, Agent Profile, passive observer

**Engineer-in-the-Loop (EITL)**:
The control principle requiring an explicit, attributable Engineer User decision for every formal approval. AI agents may prepare, review, validate, and recommend, but cannot approve, infer approval from silence, or retain approval after a material change.
_Avoid_: Human-on-the-loop, automatic approval, agent approval

**Tender Office**:
The temporary organization assembled for one Tender, comprising the specialist roles needed to analyze, plan, price, review, control, and produce its submission under the Tendering Manager's decisions.
_Avoid_: Chatbot, fixed agent list

**Tender Office Coordinator**:
The AI role that coordinates the Tender Office's daily work, dependencies, deadlines, consolidation, and escalation without taking decisions reserved for the Tendering Manager.
_Avoid_: Tendering Manager Agent, autonomous manager

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
An invariant operation no Agent Profile can receive, including access to Secret data, direct Tender Store or security-control mutation, cross-workspace access, untrusted code execution, autonomous external action, approval, or concealment of audit history. The Engineer User may perform separately authorized workflow actions but cannot convert a Prohibited Action into agent authority.
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

**Team Composer**:
The controlled composition authority that maps verified Project Fingerprint signals and mandatory policy to Capability Demands, proposed Agent Profiles, Tender Tasks, reviewer assignments, and constraints for a Work Plan. It may use AI recommendations but cannot activate an Agent Profile or approve its own proposal.
_Avoid_: Autonomous staffing agent, fixed team list

**Bootstrap Team**:
The fixed restricted set of Tender Office Coordinator, Document Controller, Tender Analyst, and Independent Reviewer Agent Profiles authorized when the Engineer User creates a Tender. It may register and analyze the Tender Package, open the Query Register, and propose the Bid Decision Package and full Work Plan, but cannot perform Active Production or external actions.
_Avoid_: Full Tender Office, approved Work Plan

**Document Controller**:
The Agent Profile responsible for Source Artifact registration, revision and addendum control, controlled distribution, the Query Register record, and Submission Package document control. It preserves provenance and status without deciding technical, commercial, or contractual meaning.
_Avoid_: File uploader, technical reviewer

**Tender Analyst**:
The Agent Profile responsible for evidence-linked Tender requirements, deadlines, evaluation criteria, compliance conditions, information gaps, and Project Fingerprint inputs. It analyzes and proposes but does not make Tender Decisions or approve compliance.
_Avoid_: Tendering Manager, general-purpose analyst

**Cost Estimator**:
The Agent Profile responsible for developing evidence-linked quantities, rate inputs, quotations, cost build-ups, and Calculation Scenarios through the Calculation Engine. It cannot approve its own estimate, choose margin, or determine the Approved Tender Price.
_Avoid_: LLM calculator, pricing authority

**Independent Reviewer**:
The mandatory Agent Profile that reviews exact work produced by other profiles, raises Review Findings, and verifies evidence and output contracts without editing the reviewed target. Specialist work requires a separate reviewer with the relevant Capability when this profile is not qualified.
_Avoid_: Author self-check, AI approver

**Tender Package**:
The complete project directory supplied for a Tender, either as a connected directory or a compressed archive, containing every source artifact the Tender Office must register and assess.
_Avoid_: Single prompt attachment

**Tender Store**:
The Quantix-managed authoritative collection of one Tender's records and registered artifact versions under the Quantix application home. External directories, archives, chats, Codex threads, temporary files, and agent sandboxes are inputs or working material rather than the system of record.
_Avoid_: Chat history, live source directory, agent workspace

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

**Artifact**:
The stable logical identity of a controlled Tender Office output across its registered versions.
_Avoid_: Working file, individual version

**Artifact Version**:
An immutable, hashed version of an Artifact registered after validating its Tender Task output contract, with inherited Data Scopes and Data Classification and provenance linking its exact inputs, producing task, Agent Profile, Agent Run, tools, template, timestamp, and parent version.
_Avoid_: Working Artifact, mutable draft, latest file

**Agent Run**:
The immutable execution trace connecting one Tender Task and Agent Profile version to its Permission Grant, registered inputs and Data Views, provider thread and exposure, approved instructions, workspace manifest, Typed Tool calls, usage, outcome, errors, and produced Artifact Versions. It preserves auditable user-visible activity but not secrets or hidden model reasoning.
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
The mandatory Approval Gate at which the Tendering Manager chooses Proceed, Hold, or Decline before the full Tender Office begins production work.
_Avoid_: Agent go/no-go decision

**Project Fingerprint**:
The structured description of the Construction Project, procurement context, disciplines, deliverables, complexity, sensitivity, and required Tender Office capabilities derived during Fingerprinting.
_Avoid_: Tender summary, Agent Profile

**Bid Decision Package**:
The versioned, evidence-linked information presented to the Tendering Manager for a Bid Decision, including compliance, exposure, capacity, deadline viability, specialist needs, information gaps, and a recommendation.
_Avoid_: Agent recommendation alone

**Work Plan**:
The versioned organization of Agent Profiles, Workstreams, Tender Tasks, responsibilities, internal milestones, reviews, resource budgets, and data scopes approved by the Tendering Manager for one Tender.
_Avoid_: Chat plan, agent-generated to-do list

**Work Plan Proposal**:
A versioned candidate Work Plan that the Engineer User may add to, remove from, split, combine, rename, or adjust within Quantix invariants before approval. Each edit produces a newly validated proposal version and has no production authority until Work Plan Approval.
_Avoid_: Partially approved team, mutable approved plan

**Work Plan Approval**:
The Approval Gate after a Proceed Bid Decision at which the Tendering Manager approves, returns, or holds one exact Work Plan version. Only an approved version may activate the full production team and authorize Active Production.
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
The Quantix authority that validates exact numeric inputs, dimensions, units, currencies, rule versions, precision, and policy before computing and registering canonical calculated values. Codex may prepare inputs, request scenarios, and explain results but cannot supply authoritative arithmetic.
_Avoid_: LLM calculator, spreadsheet as system of record, agent-generated total

**Calculation**:
The stable logical identity of a derived numeric conclusion across its deterministic executions.
_Avoid_: Spreadsheet cell, prose total, individual execution

**Calculation Run**:
An immutable, hashed execution of an exact Calculation Rule version against exact input revisions, units, currencies, policies, scenario, and engine version. It preserves unrounded and contractual results, validation, provenance, and review state and inherits unresolved trust conditions from its inputs.
_Avoid_: Recalculated cell, overwritten result, LLM arithmetic

**Calculation Input**:
A typed numeric value admitted from a Verified canonical record, EITL-approved Assumption, non-Stale Calculation Run, approved scenario parameter, or attributable Engineer User entry. Missing, blank, unavailable, not-applicable, and explicit zero remain distinct.
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
A design- or safety-related calculated result produced by an approved discipline-specific Calculation Rule or verified external engineering tool. Without either, Quantix may register an external result as Evidence but cannot substitute Codex arithmetic.
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
The independently reviewed and Engineer User-approved expected cost of delivering the Construction Project, including direct costs, indirect costs, explicit allowances, and approved risk provisions, bound to an exact Calculation Manifest before final commercial pricing decisions.
_Avoid_: Tender Price, unreviewed estimate

**Approved Tender Price**:
The immutable, versioned customer-facing price and Calculation Manifest approved by the Engineer User after considering the Priced Cost Baseline, risk provision, overhead, financing, profit, discounts, and commercial adjustments.
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
The Approval Gate through which the Tendering Manager knowingly accepts a Major Review Finding or another explicit departure that the workflow permits to continue.
_Avoid_: Silent waiver, agent acceptance

**Coordinated Bid Baseline**:
The versioned, reconciled set of technical, programme, commercial, procurement, contractual, risk, and submission commitments approved before Package Production.
_Avoid_: Collection of latest drafts, unreviewed bid

**Baseline Approval**:
The Approval Gate at which the Tendering Manager accepts the Coordinated Bid Baseline after Integrated Review.
_Avoid_: Workstream completion, agent consolidation

**Package Production**:
The controlled transformation of the approved Coordinated Bid Baseline into the files, forms, schedules, appendices, envelopes, and naming structure required for the Submission Package.
_Avoid_: New drafting phase, commitment change

**Final Review**:
The independent verification that the produced Submission Package matches the Coordinated Bid Baseline, covers every mandatory requirement and addendum, preserves information boundaries, and satisfies its manifest and release rules.
_Avoid_: Production self-check, manager file-by-file drafting review

**Final Approval**:
The Approval Gate at which the Tendering Manager reviews the release summary and either returns the Submission Package for correction or freezes an immutable package version as Ready for Submission. Any later content change creates a new version, revokes Final Approval, and requires Final Review again.
_Avoid_: External submission, editable approval, agent release decision

**Evidence**:
A structured link from a canonical claim or record field to an exact Source Artifact Version and typed location, preserving the original excerpt, extraction provenance, confidence, verification status, and any non-authoritative translation. New source versions leave historical Evidence intact but can make it stale for current work.
_Avoid_: Unsupported assertion, generic file citation, translated text as authority

**Assumption**:
A first-class record of an unproven proposition needed for Tender work, including its evidence gap, owner, affected work, proposed treatment, confidence, status, and EITL decision when material. Approval permits controlled reliance but never converts an Assumption into a fact.
_Avoid_: Hidden premise, unsupported Evidence, approved fact

**Tender Decision**:
The canonical record of a formal Engineer User judgment, including the question, options, outcome, rationale, Evidence, affected exact records, conditions, expiry, identity, timestamp, and any superseded decision.
_Avoid_: Chat answer, agent recommendation, Audit Event

**Approval Record**:
The immutable result of an Approval Gate, binding an explicit Engineer User outcome to exact record revisions, artifact versions, hashes, Evidence, conditions, and exceptions. Material dependency changes invalidate rather than erase it.
_Avoid_: Chat confirmation, approval of latest, editable sign-off

**Submission Package**:
The controlled set of tender files that has passed completeness and consistency validation and received the Tendering Manager's approval for external submission.
_Avoid_: Agent draft, unvalidated export

**Approval Gate**:
A workflow boundary that cannot advance until the authenticated Engineer User, acting as Tendering Manager in v0, explicitly accepts, returns, or rejects the specified proposal, commitment, exception, or output. The approval records the engineer identity, timestamp, decision, object versions and hashes, evidence, comments, conditions, exceptions, and history.
_Avoid_: Agent self-approval, informal chat confirmation

**Ready for Submission**:
The successful terminal Tender state reached when the validated Submission Package has received the Tendering Manager's final approval.
_Avoid_: Submitted, complete

**Declined**:
The terminal Tender state produced when the Tendering Manager chooses Decline at the Bid Decision.
_Avoid_: Failed, cancelled

**Withdrawn**:
The terminal Tender state produced when the Tendering Manager stops a Tender after previously choosing Proceed.
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
