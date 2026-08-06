

Context: An Open-Source Agentic Tendering Office for Construction

Executive conclusion

Context is feasible, but it should not be designed as an autonomous chatbot that “does tenders.” It should be a local-first tender operating system in which a deterministic workflow controls deadlines, approvals, document versions, pricing calculations, permissions, and submission status, while AI agents perform bounded analytical and drafting work.



The strongest initial positioning is:



An open-source, evidence-driven virtual tender department for construction contractors that dynamically assembles specialist AI agents for each opportunity and guides the bid from tender receipt through submission, negotiation, award, and project handover.



A literal “build nothing from scratch” approach is impossible because the construction-specific domain model, workflow rules, evidence model, and user experience are the product’s unique value. However, approximately all generic infrastructure—desktop runtime, workflow engine, agent runtime, document parsing, PDF viewing, Word and Excel generation, databases, semantic search, tracing, and testing—can be assembled from established open-source projects.



Context should not claim to be the first AI tender product. Current commercial products already advertise functions such as RFP analysis, go/no-go recommendations, compliance checking, proposal drafting, vendor sourcing, and scope review. Vendor materials for ContraVault, TendersWorld, Civilnex, SimpleTender, BidSubs, and Tender X describe parts of this market. 



The defensible uniqueness is the combination of:



Open-source and local-first operation.

Construction-contractor rather than generic sales-proposal workflows.

A dynamically generated tender team rather than a fixed chatbot list.

Arabic-English and FIDIC-oriented regional workflow packs.

Evidence links from every requirement, risk, assumption, and proposal claim back to the source document.

Human approval gates for commercial, contractual, legal, and submission decisions.

User-owned agent definitions, tender playbooks, templates, and company knowledge.

Vendor-neutral model support over time.

Full lifecycle coverage from opportunity intake to award handover, rather than proposal writing alone.

The recommended first product is much smaller: import one tender package, extract its deadlines and requirements, build a reviewed compliance matrix, generate a bid/no-bid memorandum, create a tender work plan, and produce one evidence-linked draft report. Starting with that complete vertical slice follows the supplied AGENTS.md principles far better than attempting estimation, procurement, planning, legal review, document generation, collaboration, and integrations simultaneously.



Construction tendering operating model

What tendering actually covers

RICS describes tendering as the process used to establish the contract price and addresses tender strategy, tender documentation, communications, queries, amendments, technical and commercial compliance, pricing examination, qualifications, abnormal tenders, clarifications, tender reporting, and contract execution. FIDIC’s procurement guidance similarly covers project strategy, preparation, prequalification, tendering, evaluation, award, and standard procurement documents. 



The World Bank Procurement Framework treats procurement as a configurable process that must be tailored to the project while preserving principles such as value for money and integrity. The Open Contracting Data Standard models tender, award, contract, and implementation as distinct stages; notably, an award is not the same object as the resulting executed contract. 



Context is primarily a bidder-side system. It can prepare the contractor for award and support post-tender clarifications, negotiation, and contract handover, but it cannot “award” the contractor because the buyer or employer controls that decision.



A practical contractor-side lifecycle is:



Stage	Main work	Required controlled outputs

Opportunity intake	Register opportunity, client, route, contract form, deadline, strategic fit and document package	Opportunity record, tender calendar, document register

Project fingerprint	Identify project type, disciplines, geography, value band, procurement route, contract conditions and submission requirements	Structured project profile

Initial compliance	Find mandatory conditions, forms, qualifications, bonds, insurances, certifications and disqualification risks	Mandatory requirements register

Bid/no-bid	Evaluate strategic fit, delivery capacity, financial exposure, competition, client risk, resource availability and probable margin	Decision memorandum and approval

Tender planning	Create work breakdown, responsibilities, internal deadlines, reviews and information requests	Tender programme and responsibility matrix

Document interrogation	Extract requirements, clauses, evaluation criteria, deliverables, conflicts and missing information	Compliance matrix, clause register, clarification log

Technical solution	Develop methodology, design approach, construction sequence, logistics, HSE, quality, sustainability and project controls	Technical response sections and supporting schedules

Programme and resources	Build procurement, design, mobilisation and construction programmes and resource assumptions	Programme narrative, milestones, resource plan

Commercial build-up	Normalise BOQ, build rates, preliminaries, indirect costs, escalation, contingency, taxes, cash flow and markup	Estimate, assumptions, exclusions and pricing workbook

Supply-chain engagement	Prepare work packages, issue RFQs, receive quotes, normalise offers and track coverage	Supplier register, quote comparison and coverage report

Contract review	Identify amendments, liabilities, indemnities, delay exposure, securities, payment conditions and departures	Contract-risk register and qualifications

Integrated review	Reconcile technical, programme, commercial and contractual assumptions	Coordinated bid baseline

Proposal production	Compile forms, methodology, CVs, schedules, pricing and appendices	Controlled submission set

Governance reviews	Conduct completeness, technical, commercial, legal and executive reviews	Review findings and approvals

Submission	Validate format, filenames, signatures, portal constraints, package completeness and deadline	Submission manifest, hashes and receipt

Post-submission	Manage clarifications, interviews, revised offers, value engineering and negotiation	Clarification responses and revision history

Award and handover	Compare final offer with awarded terms and transfer commitments, risks and assumptions to delivery	Award review and project handover pack

Lessons learned	Record scoring feedback, lost/won reasons, reusable content and estimating outcomes	Tender knowledge record



Public procurement evaluation guidance repeatedly stresses initial completeness and compliance checks, transparent price-and-quality assessment, competent evaluators, independent or moderated scoring, documented decisions, and investigation of abnormally low pricing. Construction frameworks may also evaluate programme, stakeholder management, supply-chain capability, risk, design, carbon and social value rather than merely the lowest price. 



Tendering Manager responsibilities

The Tendering Manager is not simply the person who writes the proposal. The manager owns the temporary tender organisation and is accountable for:



Maintaining the single source of truth for documents, addenda, deadlines and decisions.

Running the bid/no-bid process.

Understanding how the tender will be evaluated.

Creating the tender strategy, work breakdown, responsibilities and internal programme.

Coordinating estimating, engineering, planning, procurement, contracts, HSE, quality, finance and management.

Controlling clarification questions and tender communications.

Ensuring every mandatory requirement is answered.

Reconciling technical assumptions with programme, quantities, pricing and contractual commitments.

Escalating critical commercial or legal exposure.

Organising reviews and obtaining formal approvals.

Protecting confidential information.

Authorising the final submission and preserving its audit trail.

Managing post-tender clarifications and transferring the awarded commitments to the project team.

RICS describes quantity-surveying and commercial functions as covering lifecycle costs, financial management, tender documents and contractor pricing procedures, which confirms that these responsibilities require specialist support rather than being absorbed entirely by the Tendering Manager. 



Dynamic tender team

Context should maintain a capability catalogue rather than a permanently fixed employee list.



Capability	Typical agent	Main responsibility	Usually human-approved

Orchestration	Tendering Manager	Strategy, delegation, deadlines, consolidation and escalation	Bid/no-bid, final submission

Intake and control	Document Controller	Register files, revisions, addenda and deliverables	Superseding or deleting records

Requirements	Compliance Analyst	Extract requirements and maintain the compliance matrix	Mandatory/non-compliant classification

Technical	Technical Proposal Lead	Methodology and technical narrative	Engineering commitments

Estimating	Estimator	Quantities, rate build-ups and estimate structure	Rates, productivity and totals

Commercial	Quantity Surveyor	Commercial assumptions, exclusions and cash-flow implications	Markup, qualifications and final price

Planning	Planning Engineer	Tender programme, sequencing, milestones and resources	Contractual programme commitments

Procurement	Supply-Chain Coordinator	Work packages, RFQs, quote comparisons and gaps	External RFQs and supplier selection

Contracts	Contracts Analyst	Conditions, amendments, liabilities and departures	Legal position and contractual departures

Risk	Risk and Opportunity Analyst	Integrated risk, opportunity and contingency review	Contingency release and risk acceptance

Quality and HSE	QA/HSE Specialist	Required plans, certifications and method controls	Safety-critical content

Sustainability	Sustainability Specialist	Carbon, environmental and social-value responses	Formal commitments

Design and BIM	Discipline or BIM Specialist	Design obligations, deliverables and information requirements	Design responsibility

Submission	Proposal and Production Specialist	Templates, forms, appendices, formatting and packaging	Final package

Independent assurance	Red-Team Reviewer	Search for omissions, contradictions and unsupported claims	Closing critical findings

Award transition	Handover Specialist	Convert the successful bid baseline into a delivery handover	Final contract comparison



Only capabilities required by the tender should become active agents. A small maintenance tender might need five agents. A design-and-build infrastructure tender might require estimating, civil, structural, MEP, BIM, planning, logistics, procurement, environmental, contractual, financial and stakeholder specialists.



Adaptive agent operating model

What should be dynamic

On project creation, Context should generate a project fingerprint from the uploaded package and a short manager interview. The fingerprint should include:



Project sector, scope and location.

Employer, consultant and procurement authority.

Procurement route and contract form.

Design responsibility.

Disciplines and work packages.

Submission deadline and intermediate milestones.

Evaluation criteria and weightings.

Mandatory eligibility conditions.

Requested deliverables and file formats.

Available BOQ, drawings, specifications and models.

Tender value and complexity indicators.

Key commercial and contractual exposures.

Required languages.

Data sensitivity.

Current information quality and unresolved uncertainty.

A Team Composer then maps this fingerprint to required capabilities, creates agent profiles and proposes a work plan. The manager approves the team before substantial work begins.



The system should not make everything dynamic. The following invariants should be hard-coded because they protect reliability:



Every critical conclusion must have evidence or be explicitly identified as an assumption.

Pricing arithmetic is performed by deterministic code, never free-form LLM calculation.

External communications require approval.

Agents receive minimum necessary data access.

No agent may alter approved prices, contractual positions or final files silently.

Every decision and artifact version is auditable.

Mandatory gates cannot be skipped by an agent.

A project cannot be marked submitted without package validation and human confirmation.

This distinction resolves the conflict between “fully dynamic” and “safe enough for real tenders”: team composition, instructions, task routing and workflow extensions are dynamic; governance, evidence, calculation and approval rules are invariant.



Agent profile instead of fictional personality

“Personality” should be operationally useful. Each AgentProfile should contain:



text

Copy

identity

capabilities

objective

success criteria

professional stance

communication style

skepticism level

risk tolerance

speed-versus-completeness preference

escalation thresholds

allowed tools

allowed data scopes

prohibited actions

required output schemas

review requirements

model and token budget

memory policy

For example, the Red-Team Reviewer should be sceptical, omission-focused and unable to modify the submission. The Proposal Lead may prioritise clarity and persuasion but must not invent experience. The Estimator should be conservative regarding missing quantities and prohibited from deciding margin.



OpenAI’s Agents SDK supports agents, tools, agents-as-tools, handoffs, dynamic instructions, guardrails, sessions, human intervention and tracing. Its documentation distinguishes a manager pattern—in which one agent retains control and invokes specialists—from handoffs that transfer control to another agent. Dynamic instruction functions can adapt behaviour from runtime context. 



The recommended pattern for Context is:



The Tendering Manager Agent remains the controller.

Specialist agents are normally exposed as bounded tools.

Handoffs are used only when the human deliberately enters a specialist workspace.

A deterministic workflow engine, not the LLM, decides legal state transitions.

Agents return structured proposals; application services validate and apply them.

Privacy and permissions

Privacy should be implemented through data scopes and tool capabilities, not merely prompt instructions.



Example scopes include:



text

Copy

project.public

project.tender\_documents

company.approved\_experience

company.cvs

commercial.estimate

commercial.markup

commercial.supplier\_quotes

contracts.legal\_advice

finance.confidential

credentials.external\_portals

The default is deny. An agent may receive only the records needed for its task. The Technical Proposal Agent may see approved project references but not confidential salaries or the final markup. A supplier-comparison agent may see quotes but not other bidders’ private data from unrelated projects. Agents should never receive raw API keys or portal credentials.



Guardrails are necessary but insufficient by themselves. OpenAI’s documentation notes that agent input and output guardrails apply at specific agent boundaries and that custom tools may require their own tool guardrails; therefore permissions must also be enforced in ordinary application code. 



Evidence-first information model

Context should turn tender documents into structured, traceable objects rather than storing only chat messages.



Core entities should include:



text

Copy

Project

SourceDocument

DocumentVersion

PageOrSection

Requirement

EvaluationCriterion

Clause

Deliverable

Deadline

BOQItem

WorkPackage

Assumption

Clarification

Risk

Opportunity

Decision

Task

AgentProfile

Capability

ApprovalGate

EvidenceLink

Artifact

ArtifactVersion

SubmissionManifest

Every extracted requirement should preserve:



Source file and version.

Page, section, sheet, cell or drawing reference.

Exact supporting excerpt within copyright-safe internal storage.

Extraction method and confidence.

Extracting agent or parser.

Human verification status.

Relationships to responses, tasks and deliverables.

The Open Contracting Data Standard can inform exchange entities such as tender, award, contract and implementation, but Context needs an extended bidder-side schema for requirements, estimate build-ups, assumptions, qualifications, work packages and proposal evidence. 



A high-level runtime should operate as follows:



text

Copy

Tender documents

&#x20;     ↓

Secure ingestion and parsing

&#x20;     ↓

Structured project evidence store

&#x20;     ↓

Project fingerprint

&#x20;     ↓

Team Composer → proposed agents and permissions

&#x20;     ↓

Deterministic tender workflow

&#x20;     ↓

Manager Agent ↔ bounded specialist agents

&#x20;     ↓

Human approval gates

&#x20;     ↓

Versioned Word, Excel and PDF artifacts

&#x20;     ↓

Submission manifest and award handover

Architecture and open-source reuse

Recommended stack

The simplest durable implementation is a TypeScript-oriented monorepo with an Electron desktop shell and a separately packaged Python document-processing worker.



Layer	Recommended component	Reason

Desktop runtime	electron/electron	Cross-platform Windows, macOS and Linux runtime using JavaScript, HTML, CSS, Node.js and Chromium; MIT licensed. 

Packaging	electron/forge	Established Electron packaging and publishing tooling maintained under the Electron organisation. 

UI	React, TypeScript and Vite	Familiar, strongly typed ecosystem suitable for Codex-generated components

Workflow	statelyai/xstate	MIT-licensed TypeScript state machines and actors with zero dependencies, intended for predictable application and workflow orchestration. 

Agent runtime	openai/openai-agents-js	TypeScript agent primitives, tools, manager/handoff patterns, guardrails, sessions, human-in-loop and tracing. 

Local database	SQLite	Single-file local storage, transactions, indexing and straightforward backup

Full-text search	SQLite FTS5	Exact search should work before semantic retrieval is added

Semantic search	asg017/sqlite-vec, later	Tiny cross-platform SQLite vector extension with MIT/Apache licensing, but it is explicitly pre-v1 and may introduce breaking changes. 

Document parsing	docling-project/docling	MIT-licensed local parsing for PDF, DOCX, PPTX, XLSX, email and images, including layout, table and OCR support. Individual model licences still require audit. 

PDF display	mozilla/pdf.js	Mozilla-supported Apache-licensed PDF rendering for the evidence-review interface. 

DOCX generation	dolanmiu/docx	MIT-licensed Word generation and modification for Node.js and browser environments. 

XLSX handling	exceljs/exceljs	Reads, manipulates and writes Excel workbooks and JSON data. 

Validation	Zod and JSON Schema	Shared runtime and static validation for agent outputs, configuration and IPC

Testing	Vitest, Playwright and Python pytest	Unit, contract, document and cross-platform end-to-end testing

Observability	Agent SDK tracing plus local audit log	Agent tracing can capture runs, agents, generations, tools, guardrails and handoffs. 



Electron is recommended over Tauri for the first long-term architecture because Context already needs a Node/TypeScript agent runtime and a Python document worker. Electron keeps the application layer in one primary language and avoids adding Rust plus sidecar orchestration. Tauri remains a reasonable alternative where smaller binaries and OS-level capability configuration outweigh the additional integration complexity; Tauri supports scoped capability files, secret storage through Stronghold and bundled sidecars. 



Electron must be treated as privileged software. Its official security guidance recommends context isolation, renderer sandboxing, restrictive Content Security Policy, validation of IPC senders and avoidance of unsafe remote content. Context should load only bundled local UI code, expose a small typed preload API and perform file, database, network and model operations outside the renderer. 



API keys should use the operating system’s credential facilities. Electron’s safeStorage provides OS-backed encryption, but its Linux documentation warns that some environments can fall back to a weak basic\_text backend; Context should detect that state and refuse to represent the secret as securely stored. 



Repository structure

text

Copy

context/

&#x20; apps/

&#x20;   desktop/

&#x20;     main/

&#x20;     preload/

&#x20;     renderer/

&#x20; packages/

&#x20;   domain/

&#x20;   workflow/

&#x20;   agents/

&#x20;   storage/

&#x20;   evidence/

&#x20;   permissions/

&#x20;   estimating/

&#x20;   artifacts/

&#x20;   integrations/

&#x20;   testing/

&#x20; workers/

&#x20;   docling/

&#x20; workflow-packs/

&#x20;   core-construction/

&#x20;   fidic/

&#x20;   egypt/

&#x20; templates/

&#x20;   bid-no-bid/

&#x20;   compliance-matrix/

&#x20;   technical-proposal/

&#x20;   commercial-submission/

&#x20; fixtures/

&#x20;   synthetic-tenders/

&#x20; docs/

&#x20;   architecture-decisions/

&#x20;   domain/

&#x20;   security/

&#x20; AGENTS.md

The boundaries should be strict:



domain contains pure business entities and rules.

workflow owns states, gates and transitions.

agents translates approved domain tasks into model runs.

permissions authorises every tool and data access.

evidence maps extracted content to source locations.

estimating performs deterministic quantities and arithmetic.

artifacts generates controlled outputs.

desktop presents the product but contains no tendering logic.

workers/docling is replaceable through a narrow document-conversion interface.

There should be no microservices, Kubernetes, cloud collaboration server, event broker or distributed database in the first product. A local SQLite database, a content-addressed project file store, a TypeScript application process and one Python ingestion worker are sufficient.



What to reuse and what not to reuse

Several GitHub repositories use names such as “Tender Management System,” but many are small educational CRUD applications and do not provide a mature construction bidding core. ProposalForce is a more relevant BSD-licensed RFP-management reference that supports proposal records and CSV/DOCX export, but it is Salesforce-oriented and should be studied for domain ideas rather than adopted as Context’s foundation. 



Recently created generic multi-agent projects such as Multica, SwarmClaw, OpenSail, Paperclip, Horizons and open-multi-agent demonstrate useful concepts including agent teams, permissions, approval gates, budgets, audit logs and declarative workflows. Because several are new, they should initially be treated as architecture and UX references rather than foundational dependencies until maintenance, security, licence compatibility and upgrade stability are reviewed. 



Context should reuse their ideas, not combine their entire codebases. The foundation should remain a small set of mature libraries with a construction-specific domain layer owned by the Context project.



Phased delivery roadmap

Foundation

Create the repository, architecture boundaries, CI, packaging, licence inventory and one synthetic but realistic construction tender fixture.



Deliverables:



Domain vocabulary and entity schemas.

Project and document storage.

Electron shell with sandboxed renderer.

Docling worker adapter.

PDF evidence viewer.

XState tender workflow.

Agent SDK adapter.

Structured logging and audit events.

Golden test fixture containing invitation, instructions, conditions, specifications, BOQ and addendum.

Architecture decision records.

Updated AGENTS.md with commands, module boundaries and acceptance rules.

Exit condition: the application installs on Windows, macOS and Linux, opens a sample project, imports documents and preserves page-level source references.



End-to-end vertical slice

Use only three AI agents:



Agent	Scope

Tendering Manager	Creates the project plan and consolidates results

Tender Analyst	Extracts deadlines, requirements, evaluation criteria and risks

Independent Reviewer	Searches for omissions and unsupported conclusions



The application should:



Import one tender package.

Build a document register.

Extract tender metadata and deadlines.

Produce a requirements and compliance matrix.

Allow every extraction to be reviewed beside the source page.

Generate a bid/no-bid memorandum.

Create tasks and internal milestones.

Export the memorandum and compliance matrix to DOCX/XLSX.

Preserve all evidence and review decisions.

Exit condition: a user can complete this entire process without editing application data outside Context.



Dynamic tender office

Add project fingerprinting, the capability catalogue, Team Composer and specialist agents.



Capabilities introduced:



Dynamic agent profiles.

Project-specific tools and data scopes.

Human-editable team proposal.

Technical, planning, commercial, procurement, contracts and HSE specialists.

Manager-as-controller orchestration.

Agent budgets and concurrency limits.

Approval gates.

Requirement-to-task and requirement-to-response traceability.

Append-only action log.

Reusable company knowledge restricted to approved content.

Exit condition: two materially different tenders generate different proposed teams and workflows without changing application code.



Commercial and supply-chain bid

Add the deterministic commercial core:



BOQ and schedule-of-rates import.

Unit and currency normalisation.

Rate build-ups.

Labour, plant, material and subcontract components.

Preliminaries and indirect costs.

Escalation, taxes, contingency and markup.

Work-package generation.

Supplier and subcontractor RFQ packages.

Quote normalisation and comparison.

Coverage and missing-quote analysis.

Technical-commercial assumption reconciliation.

Approval-controlled final price.

The LLM may classify BOQ descriptions, identify possible omissions and draft RFQs. It must not become the system of record for numerical calculations.



Exit condition: every tender price is reproducible from stored inputs, formulas and approved adjustments, with no arithmetic dependent on model-generated prose.



Submission factory

Add controlled proposal production:



Company template management.

Requirement-linked response sections.

CV and experience selection from approved libraries.

Method-statement assembly.

Programme and schedule attachments.

Word and Excel generation.

PDF compilation where licensing and fidelity permit.

Filename and folder rules.

Form-completion checks.

Cross-document consistency checks.

Signature and approval tracking.

Submission manifest, file hashes and receipt storage.

Red-team and final executive reviews.

Exit condition: Context creates a complete, validated submission package while making every unresolved exception visible.



Award and organisational learning

Add:



Post-tender clarification management.

Interview and presentation preparation.

Best-and-final-offer revisions.

Negotiation registers.

Comparison of submitted, negotiated and awarded positions.

Award/no-award reason analysis.

Handover of scope, assumptions, rates, programme, risks and commitments.

Reusable lessons and approved answer-library updates.

Actual-versus-tender feedback from completed projects.

Exit condition: successful tenders transfer a controlled baseline to delivery, while lost tenders improve future decisions without automatically contaminating approved company knowledge.



Community ecosystem

Only after the local single-user product works:



Workflow-pack SDK.

Agent capability-pack SDK.

Template marketplace.

Contract-form packs.

Jurisdiction packs.

Arabic-English terminology packs.

Optional local-model support.

Optional team server and role-based collaboration.

Connectors for email, document management, ERP, scheduling, BIM and procurement systems.

OCDS import/export where applicable.

MCP integration with explicit permissions.

Public benchmark tenders and evaluation suite.

This is where Context becomes a community platform rather than one application.



Quality, security and open-source governance

Testing strategy

The most important quality metric is not how polished the proposal sounds. It is whether Context finds the requirements, preserves evidence, performs correct calculations and prevents unsafe actions.



Test category	Required checks

Domain unit tests	Workflow transitions, deadlines, currencies, totals, markups, weightings and approval rules

Document golden tests	Expected clauses, dates, forms, BOQ columns and addenda extracted from fixed tender packs

Agent contract tests	Every agent response validates against its schema

Evidence tests	Every requirement and factual output retains valid source references

Permission tests	Agents cannot read unauthorised commercial, HR, legal or credential data

Prompt-injection tests	Tender text cannot override system permissions or request unrelated tool actions

File-security tests	Corrupt files, oversized archives, nested archives, malicious links and macro-bearing documents

Consistency tests	Price, programme, quantity, methodology and contract assumptions agree across outputs

Artifact tests	DOCX/XLSX structure, formulas, page references, filenames and package manifests

Regression evaluations	Compare each release against fixed expected outputs and reviewed baselines

Cross-platform tests	Installation, update, import, processing and export on Windows, macOS and Linux



Recommended product metrics are:



Recall of mandatory requirements.

False critical-requirement rate.

Deadline extraction accuracy.

BOQ coverage.

Quote coverage by work package.

Numerical accuracy.

Percentage of proposal claims with approved evidence.

Contradictions found before submission.

Critical review findings remaining at submission.

Human acceptance or correction rate by agent and task type.

Model cost per tender.

Processing time by stage.

Win/loss reasons, without presenting win rate as an AI-only outcome.

Approval gates

Human approval must remain mandatory for:



Bid/no-bid.

External clarification questions.

Issuing supplier RFQs.

Productivity and major estimating assumptions.

Contingency and markup.

Contract departures.

Use of unapproved company experience or CV data.

Safety-critical commitments.

Final programme commitments.

Final price.

Submission.

Negotiated revisions.

Acceptance of award terms.

This is consistent with Codex’s own operating model: OpenAI states that Codex can edit files, run tests and propose changes, while users review the evidence and manually validate generated code before integration. OpenAI also documents AGENTS.md as the repository mechanism for navigation guidance, test commands and project practices. 



Codex development workflow

Every Codex task should be an independently reviewable vertical change:



text

Copy

Problem

User-visible behaviour

Out-of-scope behaviour

Domain rules

Affected modules

Required tests

Acceptance commands

Security considerations

Documentation changes

Codex should not receive tasks such as “build the estimating module.” It should receive tasks such as:



Add BOQ XLSX import for one supported worksheet layout, persist normalised items, display validation errors, and include fixture, unit, integration and end-to-end tests.



The root AGENTS.md should preserve the supplied principles and add:



Exact install, lint, type-check, test and packaging commands.

Dependency approval policy.

Allowed module dependency direction.

“No business logic in UI components.”

“No unvalidated agent output enters the database.”

“No LLM arithmetic becomes an approved commercial result.”

“No external side effect without a permission check.”

“Every extracted fact requires evidence metadata.”

“Delete obsolete paths rather than adding adapters.”

“A task is incomplete until its tests pass.”

“Do not add packages before checking existing dependency APIs and types.”

Codex became generally available with CLI and SDK capabilities in October 2025, and OpenAI’s 2026 Codex application supports parallel agents and isolated worktrees. Those features are useful for separate module tasks, but merge order should remain controlled because Context’s domain schema and workflow are shared foundations. 



Open-source policy

Apache-2.0 is the recommended project licence. It permits use, modification and distribution and includes an explicit contributor patent grant, which is useful for an extensible industry platform. 



The repository should also contain:



LICENSE

NOTICE

CONTRIBUTING.md

CODE\_OF\_CONDUCT.md

SECURITY.md

GOVERNANCE.md

TRADEMARKS.md

Dependency licence report.

Software bill of materials for each release.

Signed release artifacts.

Security reporting process.

Developer Certificate of Origin sign-off rather than an initially complex CLA.

Automated dependency, vulnerability and secret scanning.

A dependency policy should normally accept MIT, BSD, Apache-2.0 and similarly permissive licences. Copyleft, source-available, model-specific and non-commercial licences should receive explicit review before entering the distributed product. Docling’s code is MIT-licensed, but its own repository advises checking the licences of individual models, so model assets must be part of the dependency audit. 



Decisions to lock before Codex starts

Decision	Recommended default	Question to resolve

Primary market	Main and specialist construction contractors	Is the first user a main contractor, subcontractor, consultant or employer?

Initial geography	Egypt and Gulf region	Which country’s practices and portals should the first fixture represent?

Contract family	FIDIC-based projects	Which FIDIC editions and local amendments are most common in the target company?

Project types	Buildings first	Buildings, infrastructure, MEP, industrial, fit-out or mixed?

Tender language	English and Arabic	Must the first release extract and generate both languages or only display Arabic sources?

Input package	PDF, DOCX and XLSX	Are drawings, Primavera schedules, BIM models and emails required in the first usable release?

Commercial depth	Requirements and assumptions before detailed estimating	Is full BOQ rate build-up required for the first live pilot?

Deployment	Local single-user desktop	Must multiple tender staff collaborate concurrently from the first release?

Model provider	OpenAI first behind a narrow provider interface	Is cloud processing acceptable for confidential tender documents?

Offline requirement	Local document parsing; model use initially online	Is fully offline inference a mandatory contractual requirement?

Company knowledge	Explicitly approved library only	What source systems hold CVs, project references, productivity rates and templates?

External actions	Draft-only with human approval	Should Context ever send emails, upload portals or contact vendors automatically?

Submission scope	Generate and validate package, not autonomously submit	Are specific e-tendering portals required?

Licensing	Apache-2.0	Is commercial hosting of Context by third parties acceptable?

Product name	Context as working name	Is the name legally and commercially available in the target software categories?



The recommended defaults produce a coherent first mission:



Context v0 should be a local single-user Electron application for English and Arabic construction tenders, initially focused on FIDIC-oriented building projects. It should import PDF/DOCX/XLSX packages, extract an evidence-linked compliance matrix, generate a bid/no-bid decision and tender work plan through three controlled agents, and export reviewed Word and Excel artifacts. No pricing autonomy, external communication, portal submission, cloud collaboration or speculative plugin system should enter the first end-to-end release.

