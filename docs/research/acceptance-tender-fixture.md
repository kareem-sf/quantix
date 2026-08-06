# Acceptance Tender fixture and licensing

Research date: 2026-08-06  
Decision ticket: [Define the acceptance Tender fixture and its licensing](https://github.com/kareem-sf/quantix/issues/3)

## Decision

Quantix v0 should be accepted against one **fully synthetic, internally consistent, bilingual Egyptian employer-designed building Tender**. The public fixture must be authored specifically for Quantix, contain no real people or organisations, and be dedicated under `CC0-1.0` at the fixture root.

The fixture should be **FIDIC-oriented, not a FIDIC reproduction**. It may describe the scenario as employer-designed construction and exercise the same kinds of contractor-side commercial and contractual risks, but it must not copy, translate, paraphrase clause-by-clause, scan, OCR, or redistribute any FIDIC publication, editable form, sample communication, logo, or trade dress. The public fixture should contain an original fictional contract family and clearly state that it is not a FIDIC contract and is not suitable for real procurement.

If authentic FIDIC behaviour later needs calibration, run a separate **private licensed overlay** outside the repository and public CI. That overlay is optional evidence, never a prerequisite for the public v0 acceptance suite.

The canonical fixture should be a directory. Tests should also create a reproducible ZIP of that directory at runtime and run the same ingestion assertions against both inputs; do not commit a duplicate ZIP.

## Facts established from primary sources

### FIDIC fit and restrictions

- FIDIC describes the 2017 Red Book, reprinted in 2022 with amendments, as the contract for building and engineering works designed by the Employer. That makes an employer-designed building the right *scenario shape* for the requested FIDIC orientation. The product contains General Conditions, guidance for Particular Conditions, and tender/contract forms. ([FIDIC product page](https://fidic.org/books/construction-contract-2nd-ed-2017-red-book-reprinted-2022-amendments))
- FIDIC states that it owns the copyright in its publications; reproduction, translation, adaptation, storage, or communication requires prior written permission. It also says purchase does not transfer authors' rights. ([FIDIC IP, copyright and trademark policy](https://www.fidic.org/copyright-0))
- The practitioner electronic-product terms allow tightly bounded project use during the licence term, but say General Conditions cannot be modified or incorporated electronically, and say FIDIC documents cannot be copied, reproduced, published, or distributed commercially. Editable Particular Conditions and forms remain subject to the purchased product's stated project and term restrictions. ([FIDIC IP, copyright and trademark policy](https://www.fidic.org/copyright-0))
- FIDIC's licence guidance expressly marks copying General Conditions as not allowed and distinguishes internal tender distribution from public distribution. ([FIDIC licence guidance](https://fidic.org/sites/default/files/fidic_licence_agreements.pdf))

**Consequence:** public availability, purchase, or access to a FIDIC file is not permission to commit it to an open-source test fixture. An independently authored fictional contract is the safe public acceptance source.

### Egyptian tender-document texture

- Egypt's General Authority for Government Services (GAGS) identifies Law 182 of 2018 as the governing public-contracting framework for the entities it oversees and describes its role in procurement controls, works contracts, contractor registration, and the government procurement portal. ([GAGS procurement role](https://www.gags.gov.eg/Home/Purchasesindex))
- The official GAGS owner guide says the tender booklet exposes general and special conditions, qualifications, technical specifications, evaluation criteria, temporary/final securities, and a draft contract. It also describes contractor evidence such as registration, past experience, proposed personnel, materials, programme, and—in works tenders—Egyptian Federation for Construction and Building Contractors membership and a local-component undertaking. ([GAGS owner guide, pp. 16–18](https://assets.mof.gov.eg/files/d1370200-4d23-11ec-96fc-a7f429c97fa6.pdf))
- The same guide describes separate technical and financial envelopes and formal completeness/technical evaluation before financial opening. ([GAGS owner guide, pp. 17 and 22–25](https://assets.mof.gov.eg/files/d1370200-4d23-11ec-96fc-a7f429c97fa6.pdf))
- GAGS lists a 2024 circular commencing use of standard tender-condition and contract patterns for works contracts. Its training programme also treats specifications, tender documentation, technical/financial evaluation, works contracts, programmes, and price-adjustment calculations as distinct procurement concerns. ([GAGS circular listing](https://www.gags.gov.eg/Book/index/1?page=4), [GAGS training programme](https://www.gags.gov.eg/Home/Targets))
- Egypt operates an official public e-procurement system, but access to a record or document does not itself establish a redistribution licence. ([Egypt Public e-Procurement System](https://eps-gags.gov.eg/))

**Consequence:** the synthetic Tender should contain Egyptian-style procedural, qualification, technical, financial, security, programme, addendum, and submission material. These sources support the *coverage categories*; their words, layouts, logos, seals, and example data should not be copied.

### Open formats and fixture licensing

- buildingSMART defines IFC as an open international standard for sharing BIM data and documents its clear-text STEP serialization. A small original IFC model is therefore a suitable redistributable BIM input. ([buildingSMART IFC 4.3 documentation](https://standards.buildingsmart.org/IFC/RELEASE/IFC4_3/HTML/content/introduction.htm))
- Creative Commons says CC0 allows a rights holder to waive copyright and related rights to the greatest extent allowed and lets reusers copy, modify, and distribute the work, including commercially. ([CC0 legal code](https://creativecommons.org/publicdomain/zero/1.0/legalcode.en), [CC0 deed](https://creativecommons.org/publicdomain/zero/1.0/))
- SPDX's canonical identifier for Creative Commons Zero v1.0 Universal is `CC0-1.0`. ([SPDX License List](https://spdx.org/licenses/))
- If generated Arabic PDFs need an embeddable font, the official Noto Arabic repository uses the SIL Open Font License 1.1, which permits embedding and states that documents created with the font do not inherit the font licence. ([Noto Arabic OFL text](https://raw.githubusercontent.com/notofonts/arabic/main/OFL.txt))

## Recommended fixture scenario

Use a fictional Tender named **Nile Civic Learning Centre — Main Works Tender**:

- fictional Egyptian employer and consultant;
- employer-designed, mid-rise public-use building in Greater Cairo;
- Arabic and English source documents, with an explicit language-precedence rule and at least one deliberate translation mismatch to detect;
- architectural, structural, civil, mechanical, electrical, fire/life-safety, fit-out, external-works, and limited contractor-design obligations;
- technical and financial envelopes, measurable evaluation criteria, bid security, programme commitments, quality/HSE/sustainability requirements, BOQ pricing, and specialist supply-chain packages;
- two addenda, including one revised submission deadline and one superseded drawing/BOQ item;
- no real procurement authority, project, address, registration number, bank account, signature, logo, seal, personal data, or supplier.

The fixture must say prominently:

> Synthetic Quantix acceptance data. No real organisation or procurement. Not legal, engineering, commercial, or FIDIC guidance.

This is a deliberately broad golden fixture. It proves the complete pre-submission Tender Office once. Smaller focused fixtures should test individual parsers and failure modes later; they should not replace this end-to-end pack.

## Complete fixture bundle

The bundle has three boundaries. Only the first two are ingested as project inputs; the oracle is test-only.

```text
fixtures/synthetic-tenders/nile-civic-learning-centre/
├── LICENSE
├── README.md
├── PROVENANCE.md
├── fixture-manifest.json
├── tender-package/
├── contractor-library/
└── oracle/
```

### 1. Tender Package

| Family | Minimum synthetic files | Capability exercised |
| --- | --- | --- |
| Notice and control | bilingual invitation PDF; tender notice PDF; package index XLSX | intake, identity, deadlines, document control |
| Instructions | English instructions PDF; Arabic instructions PDF; evaluation criteria XLSX; submission checklist DOCX | eligibility, two-envelope rules, scoring, mandatory deliverables, bilingual precedence |
| Contract | original fictional General Conditions PDF; Contract Data PDF; original Special Conditions PDF; draft agreement DOCX; synthetic security forms DOCX | contract-risk review, securities, payments, delay, insurance, departures; no FIDIC text |
| Scope and specifications | overall scope PDF; architectural/finishes PDF; structural/civil PDF; MEP/fire PDF; QA/HSE PDF; sustainability/social-value PDF | technical response, design responsibility, quality, safety, sustainability |
| Drawings and BIM | drawing register XLSX; at least five vector PDFs across architectural, structural, mechanical, electrical and site disciplines; one small original IFC model | drawing revisions, metadata, cross-document references, BIM registration and limited extraction |
| Commercial | measured BOQ XLSX with formulas and units; pricing instructions PDF; provisional-sum/allowance schedule XLSX | deterministic quantities, rates, currency, tax, totals, contingency boundaries |
| Programme and resources | contractual milestones XLSX; phasing/logistics PDF; resource requirements XLSX | programme, dependencies, resources, contractual dates |
| Forms and schedules | form of tender DOCX; compliance schedule XLSX; proposed staff/experience forms DOCX; plant/material schedules XLSX | forms completion, qualifications, experience, proposed personnel and materials |
| Site information | searchable geotechnical PDF; one scanned Arabic site note requiring OCR; utilities PDF; a few entirely generated site images | evidence extraction across text, OCR, tables and images |
| Addenda | two bilingual addendum PDFs; revised drawing register XLSX; one revised drawing PDF; one revised BOQ XLSX | versioning, supersession, changed deadline, conflict reconciliation |

The original fictional General Conditions should cover ordinary construction-contract topics—roles, communications, time, payment, variations, quality, safety, insurance, claims, suspension, termination, and dispute escalation—but use an independently designed structure, terminology, numbering, and wording. It must not present itself as compatible with, derived from, or endorsed by FIDIC.

### 2. Contractor Reference Pack

The Tender Office cannot produce a credible full submission from employer documents alone. Supply a separate synthetic contractor library with explicit data scopes:

| Family | Minimum synthetic files | Capability exercised |
| --- | --- | --- |
| Corporate eligibility | company profile DOCX; fictional registrations/certifications schedule XLSX; authority matrix PDF | eligibility and approval checks without real credentials |
| Experience and personnel | approved project-reference register XLSX; three synthetic project sheets PDF; key-personnel CV pack DOCX | evidence-backed experience and staffing, no invented claims |
| Commercial inputs | labour/plant/material rate library XLSX; productivity assumptions XLSX; preliminaries template XLSX | deterministic estimate build-up and controlled assumptions |
| Supply chain | work-package register XLSX; three synthetic supplier quotations PDF/XLSX with different inclusions, currencies, validity dates, and exclusions | RFQ drafting, quote normalization, coverage and exception analysis |
| Production assets | bilingual proposal template DOCX; approved boilerplate register XLSX; synthetic logo SVG | controlled proposal generation and language selection |

The library should include at least two access classes—general approved experience and restricted commercial data—so tests can prove that a technical Agent Profile cannot read markup or confidential rates.

### 3. Test oracle

The oracle is not shown to agents. It should be compact, machine-readable, and stable:

- `artifacts.json`: expected path, media type, language, revision, issuer, hash, superseded-by link, and ingest disposition for every file;
- `facts.json`: deadlines, eligibility conditions, deliverables, evaluation weights, securities, contract particulars, and exact evidence locations;
- `requirements.json`: mandatory/optional classification and expected source references;
- `contradictions.json`: every planted mismatch and its acceptable resolution or escalation;
- `calculations.json`: BOQ quantities, formulas, currencies, tax treatment, totals, quote-normalization results, and tolerances;
- `team.json`: required capabilities, permitted scopes/tools, and the reasons each role is active;
- `submission.json`: required technical/financial output files, naming rules, unresolved exceptions, and expected manifest entries;
- `approvals.json`: actions that must remain pending until the Tendering Manager approves them.

The oracle should grade structured facts, evidence locations, calculations, workflow state, permissions, approvals, and package completeness. It should not require model prose to match a single golden paragraph.

## Planted cases the fixture must contain

The pack is useful only if it forces the Tender Office to reconcile information rather than summarize documents. Include at least these authored cases:

1. Addendum 01 moves the submission deadline; the original invitation remains in the package.
2. Addendum 02 supersedes one drawing and changes one BOQ quantity.
3. An Arabic requirement and its English rendering differ materially; the stated precedence rule determines the governing text.
4. A specification, drawing note, and BOQ description disagree on one fire-rated element.
5. A milestone in the instructions conflicts with an uncorrected date in the programme schedule.
6. One mandatory form is referenced but absent, requiring a clarification rather than invention.
7. One specialist package has no quote; another quote excludes installation; a third uses a different currency and validity date.
8. A contractor-designed element creates a design-responsibility and insurance issue.
9. A high-risk BOQ formula is deliberately wrong in the source workbook; Quantix must preserve the source and calculate a controlled correction rather than silently alter it.
10. One file contains a harmless prompt-injection instruction inside ordinary tender text; it must be treated as untrusted source content and never override permissions or workflow.
11. The final package requires separate technical and financial envelopes; restricted price data must not leak into the technical submission.
12. One critical claim lacks approved contractor evidence and must remain an assumption/open exception.

Keep corrupt-file, archive-bomb, path-traversal, macro, and malware samples in a separate security corpus. Mixing destructive or malformed artifacts into the business golden fixture would make end-to-end failures harder to diagnose.

## Required Tender Office capabilities

The acceptance Tender should activate every pre-submission role needed by the current destination:

| Agent Profile | Why this fixture requires it | Minimum reviewable output |
| --- | --- | --- |
| Tender Office Coordinator | owns the task graph, dependencies, deadlines, consolidation, and escalation | approved team proposal, Tender work plan, decision/exception queue |
| Document Controller | addenda and revised drawings/BOQ create a real revision problem | document register, supersession graph, immutable hashes |
| Compliance Analyst | bilingual instructions, eligibility, forms, and scoring are distributed across files | evidence-linked compliance matrix and missing-item register |
| Technical Proposal Lead | multidisciplinary employer design needs a coordinated construction response | methodology and technical response with source links |
| Estimator | measured BOQ and rate inputs require reproducible arithmetic | priced estimate with formula lineage and unresolved inputs |
| Quantity Surveyor / Commercial Analyst | qualifications, exclusions, allowances, tax, cash flow, and markup need control | commercial assumptions and qualifications; manager-controlled price |
| Planning Engineer | milestones, phasing, logistics, and resources must reconcile | tender programme narrative, milestones and resource plan |
| Supply-Chain Coordinator | specialist packages and uneven quotes create coverage gaps | package register, RFQ drafts, quote comparison and coverage report |
| Contracts Analyst | original conditions and particulars contain risk allocations and departures | contract-risk/departure register; no legal commitment without approval |
| Risk and Opportunity Analyst | technical, programme, commercial, and contractual risks interact | integrated risk/opportunity register and proposed contingency basis |
| QA/HSE Specialist | explicit quality, safety, and method controls apply | QA/HSE response and safety-critical escalations |
| Sustainability Specialist | energy, waste, local-content, and social-value criteria apply | evidence-linked sustainability response and commitment list |
| Design/BIM Specialist | contractor design, drawing revisions, and IFC metadata apply | design-responsibility matrix and drawing/model review findings |
| Proposal and Production Specialist | controlled bilingual forms and two envelopes must be assembled | validated Submission Package and manifest |
| Red-Team Reviewer | planted omissions, conflicts, assumptions, and leakage risks require independent challenge | findings register and closure status without write access to approved outputs |

The Tendering Manager is the human decision authority, not another autonomous Agent Profile. The fixture must require manager Approval Gates for team composition, work plan, bid/no-bid, clarification/RFQ release, engineering commitments, estimate assumptions, markup/final price, contractual departures, safety-critical commitments, exceptions, and final Submission Package.

Do not activate an Award/Handover role: post-submission clarification, negotiation, award handover, and organisational learning are outside the v0 destination.

## End-to-end acceptance result

Given either the canonical directory or its runtime-generated ZIP, the same fixture should prove that Quantix can:

1. register, hash, classify, and preserve every Source Artifact without silently ignoring unsupported content;
2. construct the document/version register and apply addenda without altering originals;
3. fingerprint the Construction Project and propose the required bounded team;
4. build and execute a dependency-aware Tender Task graph while keeping chat out of the system of record;
5. produce evidence-linked deadlines, requirements, compliance, clarification, design, programme, procurement, commercial, contract, risk, QA/HSE, and sustainability records;
6. perform all quantities, rates, currencies, formulas, totals, tax, contingency, and markup through deterministic code;
7. enforce least-privilege file/tool scopes and keep commercial data out of the technical envelope;
8. stop at every mandatory Approval Gate;
9. produce reviewed bilingual technical and financial deliverables, a bid/no-bid memorandum, and a validated Submission Package with filenames, versions, hashes, and unresolved exceptions;
10. surface every planted critical case in the oracle without inventing absent evidence or claiming native DWG/Revit editing.

## Licensing and provenance implementation

1. Put the complete CC0 legal text in the fixture's `LICENSE` file and use the SPDX expression `CC0-1.0` in `fixture-manifest.json`.
2. State exactly what the dedication covers: all original tender documents, data, drawings, IFC content, images, names, and oracle data in this fixture.
3. Keep the project's software licence separate. A repository-level software licence does not make third-party source documents redistributable, and the fixture's CC0 dedication should not change the licence of application code.
4. Include `PROVENANCE.md` listing every generator, font, and external reference consulted. Every committed content file should have `origin: synthetic` and no upstream copyrighted source.
5. Generate all images, drawings, numbers, names, addresses, signatures, seals, and logos. Do not anonymize a real Tender; de-identification does not clear copyright or confidentiality.
6. If Noto Arabic is embedded in generated documents, record the font name/version and OFL source in provenance. Do not relabel the font software itself as CC0.
7. Add a CI allowlist: fixture content may contain only project-authored files plus explicitly reviewed third-party assets with licence and attribution metadata. Fail CI on unknown provenance, real-looking personal identifiers, forbidden third-party branding, or files that claim to be authentic FIDIC material. Any deeper comparison against licensed FIDIC content belongs in the private calibration lane.
8. Preserve source files for document generation where practical, then derive PDFs deterministically so future edits remain auditable.

## Material that must not be copied

- any FIDIC General Conditions, Particular Conditions templates, annexes, forms, sample communications, guides, educational/practitioner PDFs, translations, scans, OCR output, screenshots, clause-by-clause paraphrases, or extracted embeddings;
- FIDIC logos, book-cover styling, trademarks used as branding, or language implying FIDIC approval, compatibility, authenticity, or endorsement;
- Egyptian Ministry/GAGS model tender or contract wording, page design, stamps, seals, logos, or bulk extracts;
- real notices or Tender Packages downloaded from the Egyptian procurement portal unless a specific, verified licence grants redistribution of that exact material;
- real employer, contractor, consultant, employee, CV, project-reference, supplier-quote, rate, bank, registration, tax, signature, credential, site-photo, or geolocation data;
- copyrighted Egyptian codes, standards, catalogues, proprietary CAD/BIM files, or manufacturer literature without an explicit compatible licence.

Short bibliographic names and links may identify sources. They should inform the authored scenario, not become fixture content.

## Source gaps and follow-up

These are findings about the evidence boundary, not reasons to delay the public fixture:

1. **No open redistribution licence was found for the Egyptian GAGS/MoF model works documents or procurement-portal Tender files reviewed.** Public access is therefore treated as reference access only. Before reusing any exact government form or text, obtain written permission or authoritative Egyptian copyright advice.
2. **No primary source reviewed established which FIDIC edition/amendment pattern is most common among Egyptian building main contractors.** The 2017 Red Book reprinted 2022 is recommended because FIDIC describes it as employer-designed building/engineering works, not because Egyptian market prevalence was proven. Validate prevalence with the target contractor before a live workflow pack.
3. **No primary source reviewed proved that the proposed fictional combination of Egyptian public-procurement procedures and FIDIC-style risk allocation is legally correct for a real Egyptian public authority.** The fixture is a software test simulation, not a model procurement. Obtain Egyptian procurement and construction-law review before claiming jurisdictional compliance.
4. **Egyptian code/standard text and proprietary DWG/Revit/Primavera samples remain unlicensed for this repository.** Use original IFC/PDF/XLSX surrogates in public CI and later validate proprietary formats with privately supplied, permissioned samples.
5. **CC0 can waive only rights held by the affirmer.** This is why the fixture should avoid third-party content rather than attempting to clear a mixed real-world package after the fact.

## Acceptance-fixture decision in one line

Build one original CC0 bilingual Egyptian employer-designed building Tender plus a synthetic contractor library and machine-readable oracle; use it as the public directory/ZIP release gate, and keep all authentic FIDIC or real-company material in an optional private licensed calibration lane.
