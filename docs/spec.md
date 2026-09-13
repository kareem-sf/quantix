# Quantix product specification

Approved 2026-09-06. The engineer authorised implementation and delegated remaining technical decisions to the development manager.

## Adaptive office scope accepted for planning — 9 September 2026

The engineer accepted the [adaptive office scope](design/adaptive-office-scope.md): named AI staff with professional personalities, separate assignment contexts, Manager-coordinated work and artifact sharing, adaptive requests without mandatory workflows, skills/plugins/MCP, RAG, public research and reproducible engineering work. **Ollama and the external integration expansion formerly in research section 11 are excluded; BIM/IFC is deferred.** Existing approved model connections, local file handling and application mail remain preserved. The [master implementation plan](superpowers/plans/2026-09-09-adaptive-office-master.md) sequences the work; these capabilities are not claimed implemented by this planning record.

The latest staffing clarification supersedes the earlier illustrative roster: **only the Tender Manager exists by default**. The Manager generates each specialist's complete profile live from tasks and engineer requests, including names, roles, titles, personas, personality and responsibilities. No seeded specialist roster or fixed role-to-route mapping is permitted. The engineer fully customizes the Manager personality. The [live office brief](design/live-dynamic-office.md) requires visible real staff activity, messages and handoffs with purposeful motion. This is planned behavior, not an assertion of current implementation.

Delivery preferences are now confirmed: desktop and text first, push-to-talk afterward. Connected tablet access was removed from Quantix on 2026-09-13. Closing the main window keeps authorized work running with a visible tray indicator while the computer stays awake; explicit Quit stops work safely. The [coverage review](design/office-completeness-review.md) records product, UI and operational completeness for planning.

## Product

The normal application data home is `~/.quantix`, including databases, imported copies, generated work, private AI components/accounts, logs, caches, scratch work and supported desktop renderer state. Normal launch must not fall back to AppData or repository-local runtime records. OS-protected credentials, project source/build artifacts and user-chosen originals/exports remain outside this managed data tree. See [unified storage](unified-storage.md).

A local Tender Office for an individual construction engineer on Windows, macOS or Linux. One persistent Tender Manager understands a package, presents a project analysis and proposed work plan, coordinates specialist work, and returns material decisions to the engineer. Routine work proceeds inside an approved plan. Requests remain flexible rather than being confined to a fixed sequence of feature modes.

## Required capabilities

1. Preserve complete original packages, paths, hashes, revisions, duplicate relationships, and individual exceptions. Read PDF, DOCX, XLSX and XLSM with exact source locations, formulas/cached values and visual page access. Identify DOC/DWG and provide supported local processing or an explicit coverage exception.
2. Build persistent project knowledge: evidence, buildings/areas/disciplines, document links, requirements, findings, assumptions, decisions, work and review history. Exact and semantic search retrieve source material; summaries do not replace evidence. Track registered, extracted, analysed and reviewed independently.
3. The manager proposes a project-specific plan; approval activates routine tasks. Facts and work stay scoped to the Tender. Specialists can retrieve further evidence and inspect sources. Material assumptions, quantity changes, commercial decisions and release require an engineer decision.
4. Use BOQ quantities by default. Support requested takeoffs at any scope, with separate measured proposals and attributable calculations. Do not imply that unverified visual interpretation is a checked takeoff.
5. Estimate using dated live market sources and rate build-ups. Preserve observed prices versus estimates, units, geography, currency, tax basis and conditions. Excluding/including VAT totals require an established tax treatment. Unknowns remain visible.
6. Research suppliers, prepare RFQs, and send/track them only under scoped explicit authorisation. Produce appropriate BOQs, cost build-ups, comparisons, technical documents, programmes and clarification/assumption/exclusion registers.
7. Keep source data and working records locally. Reuse preferences and explicitly approved knowledge across Tenders, with revalidation of dated information. Preserve privacy in logs and source control.
8. Provide a polished manager-led interface with inspectable files/work/estimates, clear progress and errors, cancellation, revision impact, restart recovery and backups.
9. Offer simple account connections for OpenAI, Anthropic, Google, xAI and OpenAI-compatible BYOK/custom endpoints. Keep bundled direct API-key access for all five groups; offer eligible ChatGPT/Codex and Grok subscriptions through their official clients and authentication. Only expose subscription routes whose integration is documented and permitted; explain unavailable methods plainly. Preserve separate account identities, credentials and billing, with no automatic conversion or paid fallback. Tender-specific data permission, proposed AI teams, approved alternatives and budgets continue to govern work. See [the current connection contract](subscription-connections.md).

## Full agentic office

The approved [full agentic office implementation](superpowers/plans/2026-09-12-full-agentic-office.md) extends these requirements: reusable provider-independent professional definitions, exact Tender instances and execution bindings; AI SDK UI over durable jobs/events; reviewed Quantix tools (skills, plugins and external MCP connections were later removed at the engineer's request); deterministic workers, bounded Monty composition and qualified private Podman execution; evidence-aware research/memory; aggregate delegation and independent checking. Each run uses one engine. Native capabilities are identified separately by their actual model/client/runtime support, with shared Quantix capabilities available only through their own reviewed execution boundary.

Only the Tender Manager exists by default. New reviewed work proposals default to four specialists, twelve assignments, depth two and concurrency two; existing grants gain no new access. Code runs locally by default; hosted execution requires explicit destination/upload/spending scope. Approved decisions, assumptions, working notes and reusable company knowledge remain distinct from conversation history. Revisions mark dependent work for review. The 24-case synthetic benchmark and actual runtime/UI/live-engine acceptance records govern adoption; a passing synthetic test alone does not complete a capability.

This desktop/text increment excludes Ollama and business-service connector expansion. BIM/IFC and voice retain their deferred status. Its full backend/UI regression suites, frontend typecheck and actual UI journey checks are authorized; release packages and real commercial sending are excluded from verification.

## Reference acceptance

Private source folder (never commit its contents): `C:\Users\kareem\Desktop\(Arch-Civil) Tender Package - Rev03 27-06-2026\(Arch-Civil) Tender Package - Rev03 27-06-2026`.

Observed: 43 PDFs, 24 DWGs, 6 XLSX, 3 XLSM with VBA, 6 DOC, 6 DOCX, plus incidental system files. The same 583-page specification and vendor lists repeat across six areas. Civil BOQ headers can reverse quantity/unit relative to actual rows; one workbook contains #REF! formulas. Formats, names and counts are acceptance evidence, never product constants.

## Success

The first usable increment imports a real folder, persists it, organises documents and source evidence, provides exact search and source previews, and produces a truthful analysis and proposed work plan. Live AI requires a configured provider. Deterministic package findings must remain clearly identified as document-processing results.

Subsequent increments complete specialist execution, semantic retrieval, estimating/research, reviews, communications and deliverables. The approved 2026-09-09 [workspace redesign](design/workspace-redesign.md) governs current UI, conversation, queue and plan-approval work. It explicitly authorizes affected tests, frontend typecheck and visual journey verification, superseding the earlier deferral for this work. No release build. Partial milestones are not described as the completed product.

## Current brand identity — 12 September 2026

The supplied transparent v4 package in `design/brand/v4` governs Quantix's current logo and palette, superseding earlier navy/teal artwork directions. Preserve the complete original package. The application mapping, transparency rules and accessible semantic colors are documented in [v4 integration](../design/brand/v4-integration.md). The approved workspace layout, plain engineering language and authority boundaries still apply.
