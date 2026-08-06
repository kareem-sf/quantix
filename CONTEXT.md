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

**Agent Profile**:
The operational definition of a Tender Office role, including its capabilities, objective, professional stance, permissions, constraints, output contract, review requirements, and resource budget.
_Avoid_: Fictional personality, character prompt

**Tender Package**:
The complete project directory supplied for a Tender, either as a connected directory or a compressed archive, containing every source artifact the Tender Office must register and assess.
_Avoid_: Single prompt attachment

**Source Artifact**:
An immutable, versioned file registered from the Tender Package that can support evidence or require action.
_Avoid_: Editable working copy

**Tender Task**:
A controlled unit of Tender Office work with an owner, objective, inputs, dependencies, deadline, output contract, status, and review requirement.
_Avoid_: Informal chat request

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
The manager-approved organization of Agent Profiles, Workstreams, Tender Tasks, responsibilities, internal milestones, reviews, resource budgets, and data scopes for one Tender.
_Avoid_: Chat plan, agent-generated to-do list

**Submission Deadline**:
The exact manager-approved timestamp and time zone for delivering the Submission Package, linked to its Evidence and superseded only through a controlled addendum change.
_Avoid_: Extracted date candidate, internal milestone

**Workstream**:
A concurrent area of related Tender Tasks performed during Active Production, such as technical, planning, commercial, procurement, contracts, or assurance work.
_Avoid_: Lifecycle state, independent project

**Cost Estimating Workstream**:
The mandatory Active Production Workstream that develops quantities, rate build-ups, quotations, direct and indirect costs, risk provisions, and pricing scenarios through deterministic calculations and independent review.
_Avoid_: LLM arithmetic, one-step price generation, final price decision

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
The independently reviewed and Engineer User-approved expected cost of delivering the Construction Project, including direct costs, indirect costs, explicit allowances, and approved risk provisions, before final commercial pricing decisions.
_Avoid_: Tender Price, unreviewed estimate

**Approved Tender Price**:
The immutable, versioned customer-facing price approved by the Engineer User after considering the Priced Cost Baseline, risk provision, overhead, financing, profit, discounts, and commercial adjustments.
_Avoid_: Expected cost, agent-selected price, editable total

**Ready for Integration**:
The Workstream status reached when its output contract validates, required Evidence and reviews exist, dependencies are resolved, assumptions and exceptions are explicit, and no critical blocker remains.
_Avoid_: Draft complete, agent finished

**Review Finding**:
An independently raised problem in a Tender output, classified as Critical, Major, or Minor and tracked until corrected or given an allowed disposition.
_Avoid_: Author self-review, informal feedback

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
A traceable link from a requirement, deadline, risk, assumption, quantity, or claim to an exact location in a Source Artifact.
_Avoid_: Unsupported assertion, generic file citation

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
