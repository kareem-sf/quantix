

Quantix: An Open-Source Agentic Tendering Office for Construction

Executive conclusion

Quantix is feasible, but it should not be designed as an autonomous chatbot that “does tenders.” It should be a local-first tender operating system in which a deterministic workflow controls deadlines, approvals, document versions, pricing calculations, permissions, and submission status, while AI agents perform bounded analytical and drafting work.



The strongest initial positioning is:



An open-source, evidence-driven virtual tender department for construction contractors that dynamically assembles specialist AI agents for each opportunity and guides the bid from tender receipt through submission, negotiation, award, and project handover.



A literal “build nothing from scratch” approach is impossible because the construction-specific domain model, workflow rules, evidence model, and user experience are the product’s unique value. However, approximately all generic infrastructure—desktop runtime, workflow engine, agent runtime, document parsing, PDF viewing, Word and Excel generation, databases, semantic search, tracing, and testing—can be assembled from established open-source projects.



Quantix should not claim to be the first AI tender product. Current commercial products already advertise functions such as RFP analysis, go/no-go recommendations, compliance checking, proposal drafting, vendor sourcing, and scope review. Vendor materials for ContraVault, TendersWorld, Civilnex, SimpleTender, BidSubs, and Tender X describe parts of this market.



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



Quantix is primarily a bidder-side system. It can prepare the contractor for award and support post-tender clarifications, negotiation, and contract handover, but it cannot “award” the contractor because the buyer or employer controls that decision.



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

Quantix should maintain a capability catalogue rather than a permanently fixed employee list.



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

On Tender creation, Quantix should generate a Construction Project fingerprint from the connected Tender Package and a short manager interview. The fingerprint should include:



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



Generic agent frameworks such as PydanticAI, the OpenAI Agents SDK, LangGraph and CrewAI are useful design references, but Quantix v0 does not install one. Their ordinary provider paths require API credentials or a custom Codex adapter, introduce another runtime and state model, and duplicate capabilities already supplied by Codex app-server. Codex app-server is the model-facing agent runtime; the genuine Rust Quantix Host is the deterministic Tender Office control plane.



The selected pattern for Quantix is:



The Engineer User, acting as Tendering Manager, remains the sole decision and approval authority.

The Tender Office Coordinator coordinates routine work inside the approved Work Plan but cannot make manager decisions.

Each specialist is a separate versioned Agent Profile with one bounded Codex thread, exact tasks, inputs, permissions, output contracts and independent review.

Inter-role handoff occurs through validated, registered records and Artifact Versions rather than shared mutable chat or workspaces.

Typed Rust workflow transitions, not an LLM or generic agent framework, decide legal state changes.

Agents return structured proposals; the Rust Host validates, reviews and publishes accepted versions.

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

Quantix should turn tender documents into structured, traceable objects rather than storing only chat messages.



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

The Open Contracting Data Standard can inform exchange entities such as tender, award, contract and implementation, but Quantix needs an extended bidder-side schema for requirements, estimate build-ups, assumptions, qualifications, work packages and proposal evidence.



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

The simplest durable implementation is a Tauri 2 desktop application with a genuine Rust Quantix Host, a React/TypeScript renderer, and a pinned OCR runtime installed and supervised by the Host. There is no Node Host sidecar.



Layer	Recommended component	Reason

Desktop runtime	tauri-apps/tauri	Cross-platform Rust Core with the operating system WebView, scoped capabilities and native packaging; MIT/Apache-2.0 licensed.

Packaging	Tauri CLI and updater	Native Windows, macOS and Linux bundles with mandatory signed updater artifacts and first-party release tooling.

UI	React, TypeScript and Vite	Familiar, strongly typed ecosystem suitable for Codex-generated components

Workflow	Typed Rust domain transitions	Pure transition functions plus persisted SQLite facts and Audit Events; add a state-machine crate only after repeated hierarchy is demonstrated.

Agent runtime	Pinned openai/codex app-server	Codex-managed ChatGPT login and threads behind the Quantix-owned AI Provider Interface; no BYOK or custom token handling.

Agent orchestration	Quantix Team Composer and typed Rust Tender Tasks	Project-specific Agent Profiles, work dependencies, permissions, EITL, evidence and publication stay in the Host; no generic agent framework or second orchestration runtime.

Local database	SQLite	Single-file local storage, transactions, indexing and straightforward backup

Full-text search	SQLite FTS5	Exact phrase retrieval over immutable Evidence locations remains available alongside semantic discovery.

Semantic search	fastembed-rs + asg017/sqlite-vec	Pinned multilingual E5 inference runs locally through Rust ONNX, while thresholded cosine vectors stay beside immutable Evidence in each Tender Store. sqlite-vec is explicitly pre-v1 and remains a version-pinned risk.

Document parsing	firecrawl/anydoc + RapidAI/RapidOCR	MIT-licensed local conversion of digital PDF, DOCX and XLSX to Markdown in-process, plus a small pinned RapidOCR ONNX runtime for scanned pages. RapidOCR model licences still require audit.

PDF display	mozilla/pdf.js	Mozilla-supported Apache-licensed PDF rendering for the evidence-review interface. 

DOCX generation	dolanmiu/docx	MIT-licensed Word generation and modification for Node.js and browser environments. 

XLSX handling	exceljs/exceljs	Reads, manipulates and writes Excel workbooks and JSON data. 

Validation	Serde, garde, ts-rs and JSON Schema	Strict Rust command decoding, Safety Limits, generated TypeScript DTOs and version-matched external output validation

Testing	Cargo test, Vitest, Playwright and Python pytest	Domain, contract, document and native cross-platform end-to-end testing

Observability	Rust tracing plus local Audit Events	Structured operational diagnostics remain separate from the canonical, tamper-evident Tender audit sequence.



Tauri 2 with a genuine Rust Host is the selected long-term desktop architecture. The React renderer is an untrusted presentation Module; the Rust Core owns all domain commands, EITL decisions, filesystem access, SQLite transactions, process supervision, recovery and updates. Quantix accepts the additional Rust-to-TypeScript binding and process-containment work in exchange for one durable native Host, declarative capability control and no Electron ABI or Chromium-distribution layer.



Tauri's Rust Core is privileged software. Quantix loads only bundled local UI code, applies a restrictive Content Security Policy and grants the main WebView only named, domain-shaped commands through a minimal capability manifest. The renderer receives no generic filesystem, SQL, shell, credential or updater permission.



Quantix does not store an OpenAI API key. The bundled Codex executable owns the Engineer User's ChatGPT authentication and exposes account state through app-server; Quantix never reads or exports Codex credentials. Any future provider requiring application-owned secrets needs a separate decision and an operating-system credential Adapter.

Codex app-server is pinned behind a generated-schema Adapter and may qualify a private engineer-operated v0. Public distribution remains blocked while OpenAI describes app-server as experimental and unsupported for production, and until applicable terms permit Quantix's intended third-party subscription-backed use. Technical risk acceptance cannot waive contractual authorization.



Repository structure

```text
quantix/
  apps/
    desktop/
      src/                 # React/TypeScript renderer
      src-tauri/
        src/               # Rust Quantix Host and deep Modules
        capabilities/      # least-authority WebView commands
        binaries/          # target-specific Codex and uv sidecars
  crates/
    domain/
    workflow/
    agents/
    tender-store/
    evidence/
    permissions/
    estimating/
    artifacts/
    integrations/
    testing/
  workflow-packs/
    core-construction/
    fidic/
    egypt/
  templates/
    bid-no-bid/
    compliance-matrix/
    technical-proposal/
    commercial-submission/
  fixtures/
    synthetic-tenders/
  docs/
    adr/
    domain/
    security/
  AGENTS.md
```

The boundaries should be strict:



domain contains pure business entities and rules.

workflow owns states, gates and transitions.

agents translates approved domain tasks into model runs.

permissions authorises every tool and data access.

evidence maps extracted content to source locations.

estimating performs deterministic quantities and arithmetic.

artifacts generates controlled outputs.

the renderer presents the product but contains no tendering logic or privileged mechanism.

the Rust Process Supervisor owns replaceable Codex and OCR Adapters behind narrow internal seams.

There should be no microservices, Kubernetes, cloud collaboration server, event broker or distributed database in the first product. One Tauri Rust Host, local SQLite Tender Stores, a content-addressed file store, the packaged Codex process and disposable OCR jobs are sufficient.



What to reuse and what not to reuse

Several GitHub repositories use names such as “Tender Management System,” but many are small educational CRUD applications and do not provide a mature construction bidding core. ProposalForce is a more relevant BSD-licensed RFP-management reference that supports proposal records and CSV/DOCX export, but it is Salesforce-oriented and should be studied for domain ideas rather than adopted as Quantix’s foundation.



Recently created generic multi-agent projects such as Multica, SwarmClaw, OpenSail, Paperclip, Horizons and open-multi-agent demonstrate useful concepts including agent teams, permissions, approval gates, budgets, audit logs and declarative workflows. Because several are new, they should initially be treated as architecture and UX references rather than foundational dependencies until maintenance, security, licence compatibility and upgrade stability are reviewed. 



Quantix should reuse their ideas, not combine their entire codebases. The foundation should remain a small set of mature libraries with a construction-specific domain layer owned by the Quantix project.



Phased delivery roadmap

Foundation

Create the repository, architecture boundaries, CI, packaging, licence inventory and one synthetic but realistic construction tender fixture.



Deliverables:



Domain vocabulary and entity schemas.

Project and document storage.

Tauri 2 shell with a least-authority React renderer and genuine Rust Host.

OCR worker adapter.

PDF evidence viewer.

Typed Rust Tender workflow transitions persisted as domain facts and Audit Events.

Pinned Codex app-server Adapter using the Engineer User's Codex login.

Structured logging and audit events.

Golden test fixture containing invitation, instructions, conditions, specifications, BOQ and addendum.

Architecture decision records.

Updated AGENTS.md with commands, module boundaries and acceptance rules.

Exit condition: the application installs on Windows, macOS and Linux, opens a sample project, imports documents and preserves page-level source references.



End-to-end vertical slice

Use only three AI agents:



Agent	Scope

Tender Office Coordinator	Coordinates the approved plan, dependencies and consolidation without manager authority

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

Exit condition: a user can complete this entire process without editing application data outside Quantix.



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

Exit condition: Quantix creates a complete, validated submission package while making every unresolved exception visible.



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

This is where Quantix becomes a community platform rather than one application.



Quality, security and open-source governance

Testing strategy

The most important quality metric is not how polished the proposal sounds. It is whether Quantix finds the requirements, preserves evidence, performs correct calculations and prevents unsafe actions.



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



Product acceptance is layered. Private v0 Qualification requires the complete deterministic suite, five consecutive clean live runs of the synthetic bilingual Acceptance Tender Fixture against the pinned Codex version, and a packaged Windows 11 end-to-end rehearsal. Public Release Gate additionally requires equivalent native packaged acceptance on Windows 11 x64, macOS 14+ Apple Silicon and Ubuntu 24.04 x64, plus production assurance and terms permitting the intended third-party subscription-backed Codex integration.



Every live release-candidate run must recover 100% of oracle-marked critical requirements, addenda, deadlines, forms and submission instructions; introduce zero unsupported critical requirements; account for 100% of BOQ rows; reproduce every deterministic calculation; attach approved provenance to every material claim; and recover at least 95% of non-critical oracle items. No average may hide a critical, calculation, evidence, permission, EITL, information-boundary, prompt-injection, integrity, recovery or security failure.



Pull requests use a deterministic fake app-server to exercise provider contracts, malformed messages, interruption, crash, restart and indeterminate outcomes on every supported CI platform without credentials. Live Codex evaluation is opted-in and release-candidate-only. Product acceptance enforces timeouts, bounded resources, no hangs, no orphaned processes and no partial canonical publication; stage timings establish an evidence-based performance baseline before a latency regression limit is introduced.



The implemented repository must expose separate deterministic verification, live-provider evaluation, native packaging-validation and aggregate release-acceptance entry points. Each qualifying attempt produces immutable Product Acceptance Runs and an attributable Product Acceptance Record binding exact fixture, oracle, source, application, Codex, OCR, model, schema, dependency, platform, test, evaluation, package, finding, exception, timing and artifact hashes.



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

Provider usage and elapsed time by Tender; missing monetary cost remains unknown rather than estimated from subscription usage.

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

Codex became generally available with CLI and SDK capabilities in October 2025, and OpenAI’s 2026 Codex application supports parallel agents and isolated worktrees. Those features are useful for separate module tasks, but merge order should remain controlled because Quantix’s domain schema and workflow are shared foundations.



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

A dependency policy should normally accept MIT, BSD, Apache-2.0 and similarly permissive licences. Copyleft, source-available, model-specific and non-commercial licences should receive explicit review before entering the distributed product. anydoc and RapidOCR are MIT/Apache-2.0 licensed, but the RapidOCR model assets require their own audit, so model files must be part of the dependency audit.



Locked v0 mission

Decision	Selected v0 boundary

Primary user	One Egyptian main-contractor Engineer User acting as Tendering Manager

Construction Project	One FIDIC-oriented employer-designed building Tender; authentic FIDIC text remains outside the public fixture

Tender language	English and Arabic source analysis and Tender-required output language

Input package	Connected directory or archive containing PDF, DOCX and XLSX; native DWG, Revit, BIM, Primavera and proprietary estimating integrations are later capabilities

Commercial depth	Mandatory evidence-linked Cost Estimating through an independently reviewed Priced Cost Baseline and EITL-approved Tender price; no LLM arithmetic or pricing autonomy

Deployment	Local single-user Tauri 2 desktop application with a genuine Rust Host and all Quantix-managed data under `~/.quantix`

AI Provider	One pinned Codex app-server Adapter using the Engineer User's Codex-managed ChatGPT subscription; no BYOK, routing, fallback or generic agent framework

Document processing	In-process anydoc Markdown conversion for digital documents plus a pinned local RapidOCR runtime installed through bundled `uv`; provider reasoning remains online

Company knowledge	Only explicitly approved, versioned company records and templates

External actions	Draft and approve inside Quantix only; no autonomous email, RFQ issue, portal upload or submission

Submission scope	Generate, validate, review and freeze an exact Submission Package; external submission remains outside v0

Product qualification	Private Windows engineer-operated v0 first; public support requires native Windows, macOS and Linux qualification plus a supported and permitted Codex integration

Project licence	Apache-2.0, with dependency and model licences audited before distribution

Product name	Quantix; Context is obsolete



These locked decisions produce the coherent first mission:



Quantix v0 should be a local single-user Tauri 2 application with a genuine Rust Host for English and Arabic construction tenders, initially focused on FIDIC-oriented building projects. It should import PDF/DOCX/XLSX packages, extract an evidence-linked compliance matrix, generate a bid/no-bid decision and Tender work plan through controlled agents, and export reviewed Word and Excel artifacts. No pricing autonomy, external communication, portal submission, cloud collaboration or speculative plugin system should enter the first end-to-end release.
