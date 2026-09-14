# Quantix product specification

Quantix is a local tendering team for an individual construction engineer on Windows, macOS or Linux. The Tender Manager leads a team of AI staff that does the tender work: it reads the package, takes off and checks quantities, prices, plans and drafts. The engineer reviews, approves and steers. Material assumptions, quantity changes, commercial decisions and release are always the engineer's.

Current as of 14 September 2026. Interfaces are in [contracts](contracts.md); decisions in [architecture](architecture.md); status in [progress](progress.md).

## The team

- One Tender Manager per Tender conversation, with a personality the engineer customises.
- Staff are hired by the Manager for the actual work, with generated names, roles, specialisms, backgrounds and working styles. There is no default roster and no fixed role list.
- The Manager assigns work, answers staff questions and combines their results. Staff work in parallel on API accounts and one at a time on a subscription client.
- The engineer sees every staff member, assignment, question, result and usage in the Team tab, and can steer the running Manager.
- Work runs under the Tender's approved AI account, request allowance and spending limits. There is no per-assignment approval.

## Required capabilities

1. Preserve complete original packages, paths, hashes, revisions, duplicate relationships and individual exceptions. Read PDF, DOCX, XLSX and XLSM with exact source locations, formulas/cached values and visual page access. Identify DOC/DWG and report an explicit coverage exception where they cannot be read.
2. Build persistent project knowledge: evidence, buildings/areas/disciplines, document links, requirements, findings, assumptions, decisions, work and review history. Exact and meaning search retrieve source material; summaries do not replace evidence. Track registered, extracted, analysed and reviewed coverage separately.
3. The Manager proposes a project-specific plan; approval starts it. Every fact cites evidence read in the run. Findings, plans, quantities, rates, requirements and drafts are proposals until the engineer decides.
4. Use BOQ quantities by default. **Quantity takeoff is agentic:** vision-capable staff take quantities from the drawings, Quantix compares each line with the BOQ, and the engineer reviews quantities that differ, work the BOQ is missing and BOQ items the drawings do not show. There is no manual measuring canvas. Takeoff results are never presented as checked quantities.
5. Estimate with dated sources and rate build-ups. Keep observed prices separate from estimates, with units, geography, currency, tax basis and conditions. Totals including or excluding VAT need an established tax treatment. Unknowns stay visible.
6. Prepare supplier quotation requests as drafts the engineer sends from their own mail program, and record replies. Produce BOQs, cost build-ups, comparisons, technical documents, programmes, registers and client-format BOQ copies as drafts; final export is a separate engineer decision.
7. Keep source data and working records locally under `~/.quantix`. Reuse preferences and explicitly approved knowledge across Tenders, with revalidation of dated information. Keep private content out of logs and source control.
8. Provide a plain-language, Manager-led interface with inspectable documents, team work, estimates and submissions, clear progress and errors, cancellation, revision impact, restart recovery and backups.
9. Offer AI connections for OpenAI, Anthropic, Google, xAI and one OpenAI-compatible custom endpoint by API key, and ChatGPT/Codex and Grok subscriptions through their official clients. Keep account identities, credentials and billing separate, with no automatic conversion or paid fallback. See [subscription connections](subscription-connections.md).

## Not in scope

Supplier email sending/sync and watchers are a future feature. Also excluded: a code sandbox or browser automation, benchmark gating, voice input, reusable agent definitions, public research libraries, Ollama and local models, BIM/IFC, native DWG interpretation and release packaging.

## Reference acceptance

Private source folder (never commit its contents): `C:\Users\kareem\Desktop\(Arch-Civil) Tender Package - Rev03 27-06-2026\(Arch-Civil) Tender Package - Rev03 27-06-2026`.

Observed: 43 PDFs, 24 DWGs, 6 XLSX, 3 XLSM with VBA, 6 DOC, 6 DOCX, plus incidental system files. The same 583-page specification and vendor lists repeat across six areas. Civil BOQ headers can reverse quantity/unit relative to actual rows; one workbook contains #REF! formulas. Formats, names and counts are acceptance evidence, never product constants.

## Brand

The transparent v4 package in `design/brand/v4` governs the logo and palette. The application mapping and accessible semantic colours are in [v4 integration](../design/brand/v4-integration.md).
