# Quantix MVP progress

Checkpoint: 2026-09-06. Specification: [spec.md](spec.md). Interfaces: [contracts.md](contracts.md).

## Current instruction

The engineer requested completion of the MVP with **no tests of any kind**. No unit tests, browser QA, lint, typechecks or live integration checks are to run until testing resumes with the engineer. Existing tests and earlier evidence are retained. New changes below are implemented but untested. API declaration generation is a build step, not a verification claim.

All delegated work uses gpt-6-astra with xhigh reasoning. Workers temporarily hit an account usage limit and subsequently resumed on the same configured model. The primary session model/speed settings cannot be independently changed or verified with the available controls; no claim that they were changed.

## Implemented MVP

- Local Python/FastAPI/SQLite workspace, React interface, authenticated loopback connection, Windows Tauri shell and Start-Quantix.cmd launcher.
- Original-preserving folder/ZIP import, versions/duplicates, PDF/Word/spreadsheet evidence, cell formulas/cache/VBA warnings, page previews and original downloads. Historical versions remain inspectable.
- Exact, meaning and combined search. Local multilingual embeddings; private Tender text is not uploaded for indexing. Incomplete/obsolete indexes are explicit.
- One Tender Manager, dynamic specialist plans, approved routine execution, saved results/events, cancellation/recovery and automatic Manager consolidation after completed plan tasks.
- Source-backed project map: buildings, areas, disciplines, work items and requirements; proposed/approved/withdrawn records, hierarchy, related documents/findings/BOQ, revision currentness and precise engineer review scopes. Imported folder groups remain distinct from approved project structure.
- Reusable engineer-approved notes with provenance, withdrawal and mandatory price/tax revalidation.
- BOQ candidate/source confirmation, Decimal rate build-ups, observed/estimated source conditions, VAT uncertainty and separate measured quantities. Broken supplied quantities can use an explicitly approved current measurement without rewriting the original record.
- Manual and agent-proposed calibrated PDF lengths/areas/counts. Agent calculations require inspected drawing regions and read supporting sources; their origin is never relabelled engineer-reviewed. Source changes invalidate linked quantities. Specialists can also propose sourced volume/group arithmetic with a quantity-specific BOQ basis.
- Dated market research and unit-rate proposals through the official SDK. Supplier investigations, RFQ drafting, exact-content engineer approval, SMTP delivery and IMAP/reply records. Newer delivery history survives restoring an older workspace.
- Draft BOQ/rate workbooks, analysis and technical Word documents, quotation comparisons, explicit-calendar programmes and clarification/assumption/risk/exclusion registers. Saved Manager programme proposals can populate the editor.
- Client-format XLSX/XLSM BOQ copies with explicit worksheet/header/row/column mappings, established pricing basis and separate measured-quantity mapping consent. Original package members, VBA and unrelated formulas remain preserved. Protected, encrypted, signed and unsupported mappings produce explicit errors.
- Routine draft generation inside an approved plan without fabricated engineer approvals. Job finalization keeps task snapshots stable and records output files in the run; rollback cleans unpublished generated files.
- Submission requirements proposed from Tender sources; explicit approval, document linking, satisfaction/exception/reopen decisions and revision invalidation. Frozen local ZIP exports check requirements by default, bind exact files/current bases and record approved scope/gaps. No external submission is implied.
- Backup/restore with original/output hashes, restart-only application and an honest recovery capture when current files are damaged. Native lifecycle and private OS credential storage.

## Earlier evidence (before the no-tests instruction)

The last complete gate before this code-only continuation passed **75 UI tests and 294 backend tests**, with one Windows privilege-dependent skip. TypeScript, Python lint and native debug checks passed at that earlier point. These results do not validate subsequent MVP additions.

The supplied reference Tender was imported: **104 files**, **15,656 source passages/rows**, 59 extracted, 5 requiring attention, 40 unsupported (24 CAD plus 16 incidental files), zero failed. All 104 original hashes were rechecked unchanged. There are 133 unconfirmed, unpriced BOQ candidates; no customer pricing or quantity decisions were made.

Local meaning indexing completed with 18,892 chunks / 2,728 unique. Earlier browser checks exercised Words/Meaning/Both, source inspection, persistent unsent messages, supplier forms and settings. Three printed drawing spans were compared read-only; discrepancies were 0.167%, 0.109% and 0.036%. Synthetic measurement save/link/cancel and representative Office rendering were exercised before the override.

Raw reference details, customer screenshots, runtime configuration and original acceptance evidence remain ignored under .quantix-dev or the local application home. They are not source-control content.

## Remaining for the joint testing session

- Exercise the new map, requirements, client BOQ, agent quantities/measurements, programme proposals, routine drafts and final export paths. Adapt retained test fixtures/contracts when testing is authorised again.
- Configure the provider privately in Settings and evaluate real source-backed Manager/specialist/research behavior. No live provider calls have been claimed or performed during development.
- Configure supplier mail and explicitly authorise any actual outgoing message; no supplier send or mailbox acceptance has been claimed.
- CAD originals are preserved and downloadable; native DWG interpretation/conversion and automatic OCR are not implemented. PDF companions and visual page tools are the current drawing path. Agent marks/arithmetic still need engineer review.
- The launcher is a local Windows development application, not a signed installer or a claim of production certification.

The planned MVP workflows are implemented for the engineer's next session. Full real-Tender engineering accuracy and live integrations remain unverified.
