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
The contractor-side decision authority accountable for approving the tender office's plans, commitments, exceptions, and final outputs. The Tendering Manager does not perform routine analysis, drafting, document control, or production work.
_Avoid_: Tender analyst, proposal writer, chatbot operator

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

**Evidence**:
A traceable link from a requirement, deadline, risk, assumption, quantity, or claim to an exact location in a Source Artifact.
_Avoid_: Unsupported assertion, generic file citation

**Submission Package**:
The controlled set of tender files that has passed completeness and consistency validation and received the Tendering Manager's approval for external submission.
_Avoid_: Agent draft, unvalidated export

**Approval Gate**:
A workflow boundary that cannot advance until the Tendering Manager explicitly accepts or rejects the specified proposal, commitment, exception, or output.
_Avoid_: Agent self-approval, informal chat confirmation
