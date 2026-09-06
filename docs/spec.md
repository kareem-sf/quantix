# Quantix product specification

Approved 2026-09-06. The engineer authorised implementation and delegated remaining technical decisions to the development manager.

## Product

A local Windows Tender Office for one construction engineer. One persistent Tender Manager understands a package, presents a project analysis and proposed work plan, coordinates specialist work, and returns material decisions to the engineer. Routine work proceeds inside an approved plan. Requests remain flexible rather than being confined to a fixed sequence of feature modes.

## Required capabilities

1. Preserve complete original packages, paths, hashes, revisions, duplicate relationships, and individual exceptions. Read PDF, DOCX, XLSX and XLSM with exact source locations, formulas/cached values and visual page access. Identify DOC/DWG and provide supported local processing or an explicit coverage exception.
2. Build persistent project knowledge: evidence, buildings/areas/disciplines, document links, requirements, findings, assumptions, decisions, work and review history. Exact and semantic search retrieve source material; summaries do not replace evidence. Track registered, extracted, analysed and reviewed independently.
3. The manager proposes a project-specific plan; approval activates routine tasks. Facts and work stay scoped to the Tender. Specialists can retrieve further evidence and inspect sources. Material assumptions, quantity changes, commercial decisions and release require an engineer decision.
4. Use BOQ quantities by default. Support requested takeoffs at any scope, with separate measured proposals and attributable calculations. Do not imply that unverified visual interpretation is a checked takeoff.
5. Estimate using dated live market sources and rate build-ups. Preserve observed prices versus estimates, units, geography, currency, tax basis and conditions. Excluding/including VAT totals require an established tax treatment. Unknowns remain visible.
6. Research suppliers, prepare RFQs, and send/track them only under scoped explicit authorisation. Produce appropriate BOQs, cost build-ups, comparisons, technical documents, programmes and clarification/assumption/exclusion registers.
7. Keep source data and working records locally. Reuse preferences and explicitly approved knowledge across Tenders, with revalidation of dated information. Preserve privacy in logs and source control.
8. Provide a polished manager-led interface with inspectable files/work/estimates, clear progress and errors, cancellation, revision impact, restart recovery and backups.

## Reference acceptance

Private source folder (never commit its contents): `C:\Users\kareem\Desktop\(Arch-Civil) Tender Package - Rev03 27-06-2026\(Arch-Civil) Tender Package - Rev03 27-06-2026`.

Observed: 43 PDFs, 24 DWGs, 6 XLSX, 3 XLSM with VBA, 6 DOC, 6 DOCX, plus incidental system files. The same 583-page specification and vendor lists repeat across six areas. Civil BOQ headers can reverse quantity/unit relative to actual rows; one workbook contains #REF! formulas. Formats, names and counts are acceptance evidence, never product constants.

## Success

The first usable increment imports a real folder, persists it, organises documents and source evidence, provides exact search and source previews, and produces a truthful analysis and proposed work plan. Live AI requires a configured provider. Deterministic package findings must remain clearly identified as document-processing results.

Subsequent increments complete specialist execution, semantic retrieval, estimating/research, reviews, communications and deliverables. Each milestone is runnable and verified; partial milestones are not described as the completed product.
