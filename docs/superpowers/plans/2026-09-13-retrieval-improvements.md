# Quantix Retrieval Improvements Implementation Plan

> **For agentic workers:** Use `superpowers:executing-plans` to implement and verify this plan increment by increment. This plan does not request subagent delegation.

**Goal:** Make the existing local embeddings consistently useful for source-grounded Tender questions and saved-work reuse, with clear preparation, exact scope and truthful coverage.

**Architecture:** Keep E5/FastEmbed/ONNX and SQLite. Centralize scoped retrieval; publish immutable extraction versions into canonical evidence; add managed background preparation and typed adapters for saved records. The index remains a rebuildable projection of authoritative services.

**Tech Stack:** Existing Python 3.12/FastAPI/Pydantic/SQLite/NumPy/FastEmbed backend and React/TypeScript/TanStack Query frontend. Keep the installed dependency versions until a measured need justifies a change.

**Spec/design:** [Embedding and retrieval analysis](../../reports/2026-09-13-embedding-retrieval-analysis.md), especially findings R01–R30, architecture, contracts and acceptance. Existing [project specification](../../spec.md) and [contracts](../../contracts.md) remain authoritative.

**Status:** Ready as a proposed execution plan. No task below is marked implemented by the analysis. User authorization covers the direction, local architecture/implementation and reversible verification; do not ask routine permission again. Live account/model/spending choices and real engineer decisions retain their existing boundaries.

## Global constraints

- Preserve supplied Tender originals; private records, models, caches, logs and temporary verification work stay beneath `~/.quantix`, with isolated roots for synthetic checks.
- Do not grant new source/tool/account/spending scope to existing reviewed staff bindings. Recheck the original services at candidate selection, text release and publication.
- Keep Tender evidence, working material, decisions, approved knowledge and public passages distinct. Embedding/search success is not analysis, review or approval.
- Keep strict Words/Meaning/Both semantics explicit. An ordinary automatic mode may report a visible Words-only state; it must never pretend that semantic search succeeded.
- No hosted embeddings, paid reranking, provider fallback, account authentication changes, Ollama or excluded integration expansion.
- Update API schemas, generated frontend bindings and contracts in the same increment. Remove obsolete internal duplicate logic after migrating its callers; do not add permanent compatibility layers.
- Inspect the current dirty working tree before editing and preserve unrelated work. Recheck source locations because adjacent development may continue.
- Run affected backend/UI tests, frontend typecheck and actual visual journeys for delivered behavior. Do not build release packages or perform real Tender approval/commercial sending.

## Task 1: Establish a representative retrieval baseline

**Findings:** R11, R13, R20, R30. **Depends on:** none.

**Files:** Extend `scripts/benchmark_multilingual_retrieval.py`; add `backend/tests/retrieval/` synthetic fixtures and benchmark tests. Retain the current six-query case as a smoke subset. Record evidence under `docs/reports/` and runtime output under the isolated Quantix home.

- [x] Define a versioned corpus/label manifest with at least 80 synthetic engineering questions and a distinct held-out set. Include every category in the analysis acceptance section.
- [x] Label source spans and multiple valid answers, absent answers, source revision, permitted scope and required surrounding context.
- [x] Add tests of the evaluator itself: absent ranks, multiple relevant passages, forbidden/stale hits and changed corpus/configuration hashes cannot produce a passing result.
- [x] Measure current keyword, semantic and combined results and the bilingual-boost ablation without changing the production ranker.
- [x] Record cold/warm latency separately, model/corpus/configuration hashes, machine context and unavailable metrics. Include initial indexing and changed-only reindexing.
- [x] Set review thresholds from the measured baseline before tuning. Critical permission/provenance failures are always blocking; do not waive them with average relevance.

**Check:** `backend/.venv/Scripts/python.exe -m pytest backend/tests/retrieval -q` — 17 passed. Real-model baseline: `scripts/benchmark_multilingual_retrieval.py --suite baseline` on the existing local E5 and synthetic labels. Evidence: [retrieval baseline](../../reports/2026-09-13-retrieval-baseline.md).

**Deliverable:** Reproducible baseline and an evaluator that cannot confuse fixture mechanics with language or live-agent quality. Completed 13 September 2026. Production ranking unchanged.

## Task 2: Centralize retrieval and fix scope/filter/duplicate ordering

**Findings:** R07–R10, R12, R14, R18. **Depends on:** Task 1 labels/evaluator.

**Files:** Add `backend/quantix/retrieval_models.py`, `retrieval_service.py`, `retrieval_ranking.py`; modify `repository.py`, `semantic.py`, `api.py`, `office_tools.py`, `office_business.py`, `tool_policy.py` where required, and `docs/contracts.md`. Add `backend/tests/test_retrieval_scope.py`, `test_retrieval_ranking.py`, `test_retrieval_api.py`; extend existing semantic/office tests.

- [x] Define `RetrievalRequest` and `RetrievalResponse` as specified in the analysis: `auto | words | meaning | combined`, Tender-evidence default collection, typed hits, actual mode, generation, matched spans, coverage, limitations and continuation. Migrate `/api/tenders/{id}/search` and its array-response consumers together; derive agent scope on the server.
- [x] Write failing tests for a permitted result below many forbidden hits; empty permitted scope; revision/revocation during a search; two permitted duplicate occurrences; and foreign Tender exclusion.
- [x] Write the reproduced duplicate-window test and document-kind-before-limit test. Include equal ranking ties and one source containing several relevant spans.
- [x] Pass validated artifact/version scope into both SQL candidate paths before rank/limit/group operations. Revalidate results before releasing their text.
- [x] Replace the fixed duplicate overfetch heuristic with distinct result selection/continuation under a hard work ceiling. Report truncation without claiming no further matches exist.
- [x] Move current reciprocal-rank fusion out of the HTTP route. Preserve per-channel provenance and deterministic tie breaking; do not combine raw FTS and cosine scores numerically.
- [x] Migrate HTTP and both agent search paths to the common service. Update all affected callers/bindings together. Preserve existing capability scope and fingerprint behavior.
- [x] Run the new tests plus existing semantic, API and office source tests; compare baseline relevance and critical slices.

**Check:** Focused backend retrieval/semantic/office/API suites passed (86 tests in the combined run including repository/usage). `npm run bindings` and `npm run check:ui` passed. Affected UI suites 29 passed. Labeled combined recall@10 0.944 → 0.986; held-out combined recall@10 0.9375 (at the Task 1 floor). Absent-answer critical failures remain 8/8 (R13, not this increment).

**Deliverable:** The same permitted search behavior through UI and agents, without duplicate or post-filter starvation. Completed 13 September 2026.

## Task 3: Publish versioned reprocessing into canonical source evidence

**Findings:** R02, R06, R22, R28. **Depends on:** Task 2 source identity contract.

**Files:** Modify `extraction_adapters.py`, `extraction_models.py`, `documents.py`, `repository.py`, `db.py`, `later_routes.py`, `evidence_tools.py`, `office_tools.py`, `research_dependencies.py`, `semantic.py` and relevant reviewed-source models/context validation. Add `backend/quantix/extraction_publication.py` if needed to keep responsibilities clear. Extend extraction/dependency/source-scope tests.

- [x] Define immutable extraction identity and an active-extraction pointer for each exact artifact. Existing original version/hash stays unchanged. Old evidence remains addressable with truthful superseded-extraction status.
- [x] Write tests demonstrating that reprocessed text currently cannot be found. Add shared-content-hash copies in two Tenders to verify that publication targets exact artifacts rather than silently adopting results everywhere.
- [x] Persist extraction reader/version/configuration, observed coverage/exceptions and original-hash basis with the target artifact identity.
- [x] Atomically publish successful page derivatives to canonical evidence/FTS plus the active extraction and desired retrieval generation. A partial reprocess must retain explicitly selected prior page evidence or mark uncovered pages; it cannot erase readable pages accidentally.
- [x] Update canonical current-evidence reads, tool validation, dependency tracking and source-view state. An old extraction's source ID cannot be represented as current solely because the original artifact is still current.
- [x] Reconcile extraction changes with reviewed execution bases, source receipts, checkpoints and existing grant fingerprints. Historical bindings must not silently gain newly available evidence or tools.
- [x] Test cancellation/failure before and during publication, unchanged original hashes, same-byte improved extraction, old-source inspection, revised-source invalidation and a fully unreadable page becoming searchable.
- [x] Update schemas/contracts/bindings and verify actual synthetic import → reprocess → search → source-open behavior.

**Check:** `test_extraction_publication` plus later-routes, identity, import-search, semantic, evidence-package, work-product dependencies, staff-context and checkpoints: 78 passed in the combined run (the nested PDFium spawn test needed a 30s join; it is not a lock). `npm run bindings` and `npm run check:ui` passed. Sources/Files/DocumentSearch UI **18 passed**.

**Deliverable:** Improved reading changes what Quantix can find while retaining exact original and extraction history. Completed 13 September 2026.

## Task 4: Build contextual passages and engineering-aware ranking

**Findings:** R03–R05, R10–R13, R20. **Depends on:** Tasks 1–3.

**Files:** Add `retrieval_passages.py`; modify `documents.py`, `semantic.py`, `retrieval_ranking.py` and source-result models. Add reader-to-retrieval tests for PDF, DOCX and spreadsheets rather than relying only on hand-authored metadata fixtures.

- [x] Preserve observed DOCX heading levels/order/table coordinates, spreadsheet row/header relationships and available PDF structure. Flag uncertain structure instead of inventing it.
- [x] Generate bounded embedding representations that include relevant source context and keep exact span maps back to immutable evidence. Keep literal source excerpts separate from normalized/search-only text.
- [x] Test long Arabic text and short orphan clauses, cross-page qualifications, table rows, formulas, hidden/merged cells and chunk boundary negations. Check prefixes are applied exactly once and token bounds include context.
- [x] Add documented identifier/phrase/numeric/unit ranking features and conservative Arabic normalization with negative examples. Do not perform numerical equivalence or commercial interpretation through embeddings.
- [x] Select useful distinct spans, expand only necessary surrounding context and expose precise read/open targets. Retain duplicate occurrence provenance across areas.
- [x] Evaluate the existing bilingual boost against no-boost and any proposed glossary on held-out cases. Keep only demonstrated improvements and version them.
- [x] Add explicit relevance/unsupported-answer handling informed by labeled cases. Keep no-match, unreadable-source and unavailable-index states separate. Do not hardcode a 0.7/0.8 “confidence” threshold.
- [x] Compare the resulting ranker with the baseline across language, identifiers, numbers and scope slices.

**Check:** Passage/reader/document/semantic/ranking/API suites **60 passed**. The three-group bilingual boost is not applied (meaning and no-boost now match; Task 1 meaning recall@10 0.958 with boost vs 0.972 without). After this increment, meaning recall@10 is **0.986**. Held-out combined recall@10 **0.9375**; held-out combined MRR **0.794** (0.005 below the Task 1 floor). Ranking version `rrf-2-features`. Bindings/typecheck passed.

**Deliverable:** Search passages preserve the engineering context needed to interpret them, with measurable relevance evidence. Completed 14 September 2026.

## Task 5: Manage model and background-index lifecycle

**Findings:** R01, R25–R29. **Depends on:** Tasks 2–4 generation/representation contracts.

**Files:** Add `embedding_runtime.py`, `retrieval_indexing.py`; modify `semantic.py`, `semantic_models.py`, `jobs.py`, `intake.py`, `api.py`, `storage.py`, `backup.py`, `factory_reset.py` and startup/shutdown hooks as required. Add `test_retrieval_lifecycle.py` and `test_embedding_runtime.py`.

- [x] Define a verified model manifest and atomic activation. Test missing/corrupt files, cancelled download, offline startup and altered runtime/preprocessing versions using synthetic/local fixtures.
- [x] Introduce one bounded managed model runtime per home/model fingerprint; test parallel loads, inference serialization/resource bounds, idle release and home separation.
- [x] Replace query-time whole-corpus fingerprinting with transactional source/extraction generation tracking. Audit every canonical evidence writer, including imports, reprocessing, visual/correspondence-derived evidence and restore; missed writers must not create false-ready state.
- [x] Add durable desired/published generations and one coalesced refresh request per Tender. Embed changed representations and reuse completed vectors; publish only after rechecking source generation.
- [x] Schedule maintenance outside the long-lived AI execution lane with bounded resources and foreground priority. Preserve explicit stop/Quit/reset and active-reader semantics.
- [x] Keep keyword search available with explicit status while semantic preparation is unavailable. Do not serve stale matches as current. Defer partial semantic operation unless its coverage contract is fully implemented and tested.
- [x] Add safe obsolete-vector/generation cleanup, disk-pressure failure, restart recovery and restore rebuild behavior. Never delete originals or historical authoritative records.
- [x] Benchmark vectorized exact scoring/caching before adopting any new index engine. Record cold/warm/resource results up to current supported limits.

**Check:** Focused backend suites **passed** (embedding runtime, retrieval lifecycle, semantic, package analysis, retrieval API/scope, extraction, API import, backup schema 4, measurements, visual sources, later-routes, factory reset). `npm run bindings` and `npm run check:ui` passed. DocumentSearch/Files UI **7 passed**. Exact scoring of 50,000 vectors is a NumPy matmul (no ANN). Browser MCP was not used for a live Documents walk in this increment.

**Deliverable:** Automatic preparation and updates that remain truthful and do not stall normal questions. Completed 14 September 2026.

## Task 6: Make Manager and staff retrieval economical and purposeful

**Findings:** R09, R15–R18, R20. **Depends on:** Tasks 2, 4 and 5.

**Files:** Modify `office.py`, `office_tools.py`, `office_business.py`, `staff_context.py`, `staff_runtime.py`, `ai_runtime_mcp.py` and exact tool capability definitions where necessary. Extend stall, usage-reduction, source-read and provider-adapter tests.

- [ ] Give the authorized engineering pass compact readiness/coverage and search guidance. Keep source passages out of the no-tools conversation-routing pass.
- [ ] Use common combined retrieval for ordinary engineering questions, then bounded exact source expansion. Tell the model when to use structured enumeration for exhaustive work.
- [ ] Share current-version span deduplication and fruitless-search tracking across keyword/semantic/combined calls, including overlaps and paraphrased queries.
- [ ] Allocate result text under a bounded prompt budget; expose useful continuation and deliberate reread after context loss. Never truncate an unreported exception or pretend omitted text was read.
- [ ] Preserve deferred source-read commits, cancellation and publication validation. A failed or oversized tool result cannot establish successful source inspection.
- [ ] Return actionable correctable argument errors for query length/filter/continuation mistakes; identity and authority violations remain refusals.
- [ ] Measure two-turn question/continuation scenarios through controlled adapters; schedule live model comparison only with selected approved account/model/spending scope.

**Deliverable:** Fewer redundant source calls and better evidence selection, verified through the actual tool path.

## Task 7: Add typed retrieval of saved work and approved knowledge

**Findings:** R21–R24, R29. **Depends on:** Tasks 2, 5 and 6.

**Files:** Add `retrieval_records.py` and domain adapter modules only as needed; modify `memory_service.py`, `knowledge.py`, `work_products.py`, `research_service.py`, `research_tools.py`, `engineering_tools.py`, `office_knowledge.py`, `work_brief.py` and the new retrieval schemas. Extend associated API/types/tests.

- [ ] Define collection adapters returning typed exact references and server-derived visibility for Tender notes/assumptions, decisions, work products, approved company knowledge and saved public passages.
- [ ] Write negative tests before indexing: foreign/raw Tender content, another staff actor/root, withdrawn knowledge, expired recheck date, changed source, public-only work-product references, wrong exact version and private data in preview metadata.
- [ ] Index titles and bounded content/rows with source/dependency metadata. Use each existing authority service to validate current read access, including public research references; do not infer permission merely from absence of local source IDs.
- [ ] Add exact-version text continuation to work-product reads and remove duplicated text fields from the agent projection. Find and inspect a passage after character 16,000.
- [ ] Add semantic lookup tools with explicit collection labels; existing grants receive no new tools automatically. Lists and exact reads remain useful for comprehensive inspection.
- [ ] Add compact relevant saved-work references to continuation, with the working brief still primary. Reading a saved result must not mark its original citations as newly inspected.
- [ ] Refresh projections on save/version/publication/withdrawal and check date-based revalidation at query/read time even without a write event.

**Deliverable:** The Manager can find relevant prior work without treating memory as source evidence or broadening authority.

## Task 8: Deliver one clear search experience

**Findings:** R06, R14, R18, R19, R21, R23, R24, R27. **Depends on:** Tasks 2, 5 and 7 contracts; document UI can ship earlier after Task 5.

**Files:** Modify `src/features/DocumentSearch.tsx`, `Files.tsx`, `EvidencePicker.tsx`, `WorkProductLibrary.tsx`, `WorkingMemoryCard.tsx`, `Knowledge.tsx`, `ResearchLibrary.tsx`, relevant source navigation and `src/api.ts`/generated bindings. Reuse existing components and queries rather than redesigning the workspace.

- [ ] Add UI tests for automatic/default search, strict mode errors, explicit Words-only operation, preparation/update/failure states, filters and stale-response cancellation after Tender changes.
- [ ] Show source location, relevant excerpt, current version and record-kind/freshness labels. Open the exact passage, row or saved-work version and preserve return navigation.
- [ ] Put method/model diagnostics and manual rebuild in More options. Show one actionable next step for download, unreadable pages, failed preparation or capacity limits.
- [ ] Update search cache keys/invalidation to use collection/filter/source generations, not just artifact ID/status. Abort obsolete searches rather than displaying an earlier Tender/query response.
- [ ] Make document/source-picker behavior consistent. Provide search within saved-work/research/knowledge views with typed results rather than one mixed list of apparent facts.
- [ ] Keep coverage states separate and avoid claiming “all documents read” when only indexing finished.
- [ ] Run affected UI suites and typecheck, then verify actual synthetic journeys in light/dark, narrow/desktop/scaled layouts and keyboard navigation.

**Check:** `npm run bindings`; `npm run check:ui`; `npm run test:ui -- src/features/Files.test.tsx src/features/DocumentSearch.test.tsx` plus changed feature tests. The current `scripts/test-ui.mjs` forwards these arguments to Vitest. Use the available browser/native workflow for actual rendered verification.

**Deliverable:** Search benefits are available through normal work without requiring the engineer to understand embeddings.

## Task 9: Verify adoption and document remaining limits

**Findings:** R01–R30 acceptance. **Depends on:** all delivered increments.

**Files:** Extend retrieval benchmarks and integration tests; update `docs/contracts.md`, `docs/progress.md`, `README.md` as appropriate and add a dated implementation acceptance report.

- [ ] Run the held-out relevance and operational comparisons without tuning to the answers. Inspect failed slices, not just aggregate scores.
- [ ] Verify original/source/extraction identity and permission checks across query, context expansion, saved-work lookup and publication.
- [ ] Run the complete authorized backend/UI regressions and frontend typecheck after integration. Record failures and their eventual resolutions honestly.
- [ ] Execute the ten synthetic application journeys in the analysis, including failure/recovery and exact source return. Keep native scaling/lifecycle acceptance distinct from browser viewport checks.
- [ ] Record live engine/model quality separately when explicitly configured/authorized. Never substitute deterministic adapters for a live-answer claim.
- [ ] Compare optional local reranker or model variants only if repaired retrieval still misses the agreed relevance target. Adopt only on documented quality/resource improvement and exact scope checks.
- [ ] Publish a concise acceptance record with implemented findings, deferred experiments, final checks and remaining live/platform limits. Do not describe this plan as delivered while any required acceptance remains open.

**Deliverable:** Evidence that the retrieval changes improve actual engineering work while preserving authority and source truth.

## Requirement coverage

| Analysis findings | Tasks |
| --- | --- |
| R01–R06 | 3, 4, 5, 8 |
| R07–R14 | 1, 2, 4, 8 |
| R15–R20 | 1, 2, 4, 6, 8 |
| R21–R24 | 3, 7, 8 |
| R25–R29 | 3, 5, 7, 9 |
| R30 | 1, 9 |

The first useful production increment is Task 2's common scoped retrieval with the reproduced scope/duplicate fixes. Establish its baseline first. Automatic preparation follows the extraction/generation contract work so it does not accelerate indexing of incomplete or disconnected source data.
