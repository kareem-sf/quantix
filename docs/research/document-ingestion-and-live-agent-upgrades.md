# Document ingestion and live-agent workspace upgrades

**Evidence snapshot:** 2026-08-16  
**Decision target:** the next production architecture for Quantix Tender ingestion and the Engineer-facing live workspace  
**Source policy:** primary sources only—upstream documentation, tagged source code, release notes, and canonical repositories

## Executive decision

Quantix should keep Docling as its canonical document normalizer, but it should tighten the boundary around it before expanding extraction breadth. The highest-priority defect is that Docling 2.118.0 can return `PARTIAL_SUCCESS` after a document timeout and `DocumentConverter.convert(..., raises_on_error=True)` still accepts that status. Quantix must explicitly reject every status other than `SUCCESS`; partial output may be retained only as quarantined diagnostics and must never enter Tender evidence.

The next production slice should also:

1. pass explicit file-size and page-count limits into Docling;
2. preflight DOCX/XLSX ZIP containers before parsing;
3. traverse both `body` and `furniture`, including children under pictures, while retaining the picture/container provenance that explains where OCR text came from;
4. capture both XLSX formulas and cached values in a bounded companion pass;
5. replace the hard-coded RapidOCR `ch` profile with approved, hash-pinned language profiles;
6. emit honest phase events and heartbeats instead of a fabricated percentage; and
7. make the live workspace a projection of a durable, typed event ledger, with the Tendering Manager as the Engineer's single approval boundary.

These changes preserve Quantix's fail-closed behavior: the system either publishes one validated, fully attributable candidate, or publishes nothing and explains why.

## What the current implementation gets right—and where it is exposed

The current runtime already has strong foundations: the Python parser runs as a supervised child process, external Docling plugins are disabled, model artifacts are locally pinned, stdout/stderr and returned JSON are capped, and Rust validates reachability rather than silently harvesting every object in Docling's top-level arrays. Those choices should remain.

The following gaps are material:

| Current behavior | Risk | Production decision |
|---|---|---|
| [`convert_document.py`](../../src-tauri/runtime/docling/convert_document.py) calls `convert(..., raises_on_error=True)` but does not inspect `ConversionResult.status`. | A timeout can publish a partial parse because Docling accepts both `SUCCESS` and `PARTIAL_SUCCESS` in this mode. | Require exactly `SUCCESS`; quarantine status, structured errors, timings, and any partial artifact. |
| The wrapper does not pass `max_file_size` or `max_num_pages`. | Oversized inputs reach expensive decoding before Quantix's downstream JSON/location limits help. | Apply explicit, versioned product limits at the converter boundary and preflight before it. |
| [`document_parsing.rs`](../../src-tauri/src/document_parsing.rs) traverses picture children but only begins at `body`. | DOCX headers and footers now preserved as `furniture` can be omitted or make completeness validation fail. | Traverse `body` and `furniture` as distinct layers; preserve layer identity in evidence. |
| Picture children are retained, but the picture is treated as an ignored container. | OCR text can survive while losing the visual-region identity that explains it. | Record the picture/container as a provenance anchor, with page and bounding box where present. |
| The pinned RapidOCR profile is `ch`. | One fixed recognition profile is not a defensible policy for Arabic/English tender packages. | Select only from approved, locally available, hash-pinned OCR profiles; record the chosen profile per parse. |
| Docling's XLSX backend loads cached cell values. | Formula intent and stale/unavailable calculation state are invisible. | Pair each relevant cell's formula text with its cached value and workbook calculation metadata. |

## 1. Docling 2.118 document semantics

### Reading order is a tree, not a top-level array scan

A `DoclingDocument` stores content items in top-level collections such as `texts`, `tables`, and `pictures`, while reading order and hierarchy are expressed through the `body` tree's ordered references. `furniture` is a separate tree for headers, footers, and other non-body material. Parent/child references are therefore semantic, not incidental storage details. [Docling document model](https://github.com/docling-project/docling/blob/main/docs/concepts/docling_document.md)

Quantix should continue resolving only tree-reachable items and should fail closed when indexed content is unexpectedly orphaned. That protects against silently inventing an order from storage arrays. This matters in practice: an upstream Docling defect report shows OCR text created inside a table but not referenced from the document tree. The correct response for canonical Tender evidence is quarantine, not opportunistic scraping. [Docling issue #3473](https://github.com/docling-project/docling/issues/3473)

### Pictures can own real OCR text

In `docling-core` 2.91.0—the version locked with Quantix's Docling 2.118.0—`iterate_items()` has a `traverse_pictures` switch. When it is false, non-caption children of a `PictureItem` are deliberately skipped. The source explicitly documents the full-page OCR case: OCR output can be represented as text children under a top-level picture. [Tagged `DoclingDocument` source](https://raw.githubusercontent.com/docling-project/docling-core/v2.91.0/docling_core/types/doc/document.py) [Docling document API](https://docling-project.github.io/docling/reference/docling_document/)

Consequences for Quantix:

- Recursing through picture children is required; otherwise scanned-page text can disappear.
- The picture must remain visible in provenance even if it contributes no standalone prose. Its identity, label, page, bounding box, parent path, and child relationship explain that the text was OCR-derived from a visual region.
- Captions, annotations, and OCR text should not be flattened into one undifferentiated string. Preserve their tree role.
- A node is publishable only if every reference resolves, the tree validates, and each emitted evidence span maps back to a source hash, document layer, item reference, page, and geometry when available.

### `furniture` is now operationally important

Docling 2.118.0 fixes dropped DOCX headers and footers, among other DOCX reading-order issues. A parser that only traverses `body` does not fully consume the version it pins. Quantix should traverse both roots and tag evidence as `body` or `furniture`; it should not merge repeated headers into body prose without an explicit policy. [Docling 2.118.0 release](https://github.com/docling-project/docling/releases/tag/v2.118.0)

### Partial success must mean no publication

Docling's `document_timeout` stops work and returns partial results with `PARTIAL_SUCCESS`. In the tagged converter source, `raises_on_error=True` allows both `SUCCESS` and `PARTIAL_SUCCESS`, so exception-only handling is insufficient. [Pipeline options](https://docling-project.github.io/docling/reference/pipeline_options/) [Tagged converter source](https://raw.githubusercontent.com/docling-project/docling/v2.118.0/docling/document_converter.py)

The wrapper must inspect `ConversionResult.status` before serializing a publishable candidate. Its `errors`, page results, timings, and confidence may be saved in a diagnostic envelope, but no partial content should be normalized into canonical evidence.

## 2. Resource-bounded parsing and publication

### One boundary policy for every format

Limits should be explicit constants in a versioned ingestion policy, not implicit consequences of RAM or library defaults. Exact ceilings should be selected from a representative Tender corpus and target hardware, but the policy must cover:

- source bytes;
- page/sheet count;
- OOXML member count, per-member expanded bytes, total expanded bytes, and compression ratio;
- decoded image dimensions/pixels and aggregate image bytes;
- worksheet declared dimensions, non-empty cells, merged ranges, formulas, comments, drawings, and charts;
- parser wall time, CPU concurrency, process memory, temporary-disk use, stdout/stderr, result JSON, evidence-item count, and tree depth;
- agent/tool output size later in the pipeline.

`DocumentConverter.convert()` exposes `max_file_size` and `max_num_pages`; Quantix should pass them, while retaining its outer process supervisor because library limits are not a substitute for process isolation. [DocumentConverter API](https://docling-project.github.io/docling/reference/document_converter/) [Advanced pipeline options](https://docling-project.github.io/docling/usage/advanced_options/)

The production process boundary should use one disposable child per document, one CPU thread for the current profile, a wall deadline, process-tree termination, an OS-enforced memory cap where supported, a staging quota, bounded output capture, and atomic candidate-to-validation-to-publication. A failure at any stage deletes or quarantines the candidate and leaves canonical Tender state untouched.

Use stable rejection codes, for example: `file_limit`, `page_limit`, `archive_member_limit`, `archive_expansion_limit`, `image_limit`, `parser_timeout`, `memory_limit`, `partial_result`, `encrypted`, `malformed`, `unsupported`, `unresolved_reference`, and `publication_failure`.

### DOCX and XLSX require ZIP preflight

Python's documentation warns that XML parsers can be vulnerable to entity expansion and decompression attacks, and that ZIP files can expand far beyond their compressed size. `ZipInfo` exposes compressed and uncompressed sizes, but applying a safe resource policy is the caller's responsibility. [Python 3.12 XML security](https://docs.python.org/3.12/library/xml.html) [Python 3.12 `zipfile`](https://docs.python.org/3.12/library/zipfile.html)

Docling 2.118.0's DOCX backend applies per-member and total caps while normalizing Strict OOXML, but ordinary Transitional DOCX files pass to `python-docx` without that same whole-package budget. Its XLSX backend applies limits to selected drawings/images, not a complete archive-wide budget. [Tagged DOCX backend](https://raw.githubusercontent.com/docling-project/docling/v2.118.0/docling/backend/msword_backend.py) [Tagged XLSX backend](https://raw.githubusercontent.com/docling-project/docling/v2.118.0/docling/backend/msexcel_backend.py)

Before any OOXML parser runs, Quantix should inspect the central directory and reject:

- unsafe or ambiguous member paths;
- duplicate normalized names;
- encryption;
- excessive member counts, expanded totals, single-member size, or compression ratios; and
- packages missing the expected content types/relationships for the declared format.

Preflight is not sanitization. A package that fails remains unchanged and quarantined; Quantix should not rewrite it and proceed silently.

### PDF checking and recovery

The strongest complementary addition is a pinned `qpdf` preflight/diagnostic step. `qpdf --check` reports syntax, encryption, stream-encoding errors, and whether damaged structure was recoverable, with distinct exit statuses for correct, warning/recoverable, and erroneous files. [qpdf 12 CLI](https://qpdf.readthedocs.io/en/12.0/cli.html)

Production use should stop at diagnostics initially. If recovery is later enabled, it must be an explicit Engineer-approved operation that creates a derived document with its own hash, tool/version/arguments, source-parent hash, warnings, and approval receipt. The repaired PDF must never replace the original invisibly. `pikepdf` is a maintained Python interface to qpdf and offers structured jobs and warnings, but it only materially helps if in-process typed integration is worth another dependency; a supervised pinned `qpdf` executable is simpler. [pikepdf jobs](https://pikepdf.readthedocs.io/en/latest/topics/jobs.html) [pikepdf API](https://pikepdf.readthedocs.io/en/latest/api/main.html)

## 3. Fidelity improvements by format

### XLSX: formulas and cached values are different evidence

Docling 2.118.0 loads workbooks through `openpyxl` with `data_only=True`, so formula cells expose the value cached when an external spreadsheet application last calculated the workbook. `openpyxl` does not calculate formulas, and its documentation distinguishes formula text from cached values. [Tagged Docling XLSX backend](https://raw.githubusercontent.com/docling-project/docling/v2.118.0/docling/backend/msexcel_backend.py) [openpyxl reader source](https://openpyxl.readthedocs.io/en/stable/_modules/openpyxl/reader/excel.html) [openpyxl formula docs](https://openpyxl.readthedocs.io/en/3.1.2/simple_formulae.html)

Quantix should use the already-installed `openpyxl` for a bounded companion pass:

- open once with `data_only=False` for the formula/expression;
- open once with `data_only=True` for the cached display value;
- join strictly by workbook hash, exact sheet, row, and column;
- retain workbook calculation properties and whether the cache is absent;
- never claim Quantix recalculated the workbook; and
- preserve both representations in evidence and surface possible staleness to the Manager.

Read-only mode substantially reduces memory and uses lazy worksheets, but it relies on producers reporting correct worksheet dimensions. Inflated or incorrect dimensions therefore need independent cell/dimension budgets and adversarial-corpus testing. [openpyxl optimized modes](https://openpyxl.readthedocs.io/en/stable/optimized.html) [Docling issue #2307](https://github.com/docling-project/docling/issues/2307)

### OCR: profiles, not a magic language default

Docling 2.118.0's RapidOCR integration resolves model files from a selected language and OCR generation; it uses only the first configured language. The release specifically changed language resolution for PP-OCR models. [Tagged RapidOCR source](https://raw.githubusercontent.com/docling-project/docling/v2.118.0/docling/models/stages/ocr/rapid_ocr_model.py) [Docling PR #3863](https://github.com/docling-project/docling/pull/3863)

Quantix should define a small registry of approved offline OCR profiles—for example, profiles evaluated for English/Latin and Arabic Tender documents—with exact artifact hashes and a recorded selection reason. Mixed Arabic/English pages need a measured design because supplying multiple languages does not make RapidOCR multilingual. Automatic network downloads, model floating, or silently retrying with an unrecorded model would violate reproducibility.

Model choice should be accepted only after a corpus comparison that measures character/word accuracy and, more importantly, Tender-critical extraction: identifiers, units, decimal values, currency, dates, table coordinates, and citation geometry.

### DOCX/XLSX visual fallback

LibreOffice supports headless conversion and documented filter selection. It can be useful later for producing a derived PDF when layout fidelity, charts, or legacy office formats matter. It should not become the canonical text extractor: conversion can change pagination, fonts, calculation results, and layout. Run it with macros and network access disabled, an isolated user profile, strict resource limits, and complete derivation lineage. [LibreOffice conversion filters](https://help.libreoffice.org/latest/ast/text/shared/guide/convertfilters.html) [LibreOffice PDF parameters](https://help.libreoffice.org/latest/en-US/text/shared/guide/pdf_params.html?DbPAR=BASIC&System=WIN)

### Useful ingestion controls from agent projects

Agent Zero's document plugin fetches once, restricts schemes, rejects credential-bearing URLs, validates public destinations across redirects, applies redirect/timeout/retry controls, streams to a 50 MB cap, bounds global parse concurrency and per-document time, and isolates one parser in a subprocess. These are sound operational patterns if Quantix later accepts remote sources. Its retrieval path is not sufficient for Tender evidence, however: it reduces documents to text chunks and can return an empty list on search failure, making a retrieval error indistinguishable from no relevant evidence. Quantix must retain page/cell/block anchors and treat retrieval failure as an error. [Agent Zero document plugin at `baadd0d`](https://github.com/agent0ai/agent-zero/blob/baadd0dd0b09fa769a1027c183b964be85d5c8cc/plugins/_document_query/README.md) [Agent Zero fetcher at `baadd0d`](https://github.com/agent0ai/agent-zero/blob/baadd0dd0b09fa769a1027c183b964be85d5c8cc/plugins/_document_query/helpers/fetch.py) [Agent Zero document query at `baadd0d`](https://github.com/agent0ai/agent-zero/blob/baadd0dd0b09fa769a1027c183b964be85d5c8cc/plugins/_document_query/helpers/document_query.py)

Hermes performs a useful post-extraction coverage check for PDFs: it counts text by page, reports exact unreadable page ranges when the empty-page threshold is exceeded, and recommends selective rendering/vision or OCR rather than silently declaring extraction complete. Quantix should make coverage (`complete`, `partial`, or `unknown`) a first-class parse result and require `complete` for canonical publication unless an Engineer explicitly approves a derived recovery workflow. [Hermes document extraction at `92c998c`](https://github.com/NousResearch/hermes-agent/blob/92c998c86c8348b572b0409e3a53e380c8f60f10/website/docs/user-guide/features/document-extraction.md) [Hermes extraction implementation at `92c998c`](https://github.com/NousResearch/hermes-agent/blob/92c998c86c8348b572b0409e3a53e380c8f60f10/tools/read_extract.py)

## 4. Honest progress for ingestion

Docling's CLI can show per-file progress and verbose page-batch messages, but maintainers have stated that there is no stable general progress-callback API; suggested pipeline wrapping reaches into implementation details. The threaded pipeline exposes queue sizes, batches, and shutdown controls, but those are pipeline mechanics, not a durable application contract. [Docling CLI](https://github.com/docling-project/docling/blob/main/docs/reference/cli.md) [Docling progress discussion #582](https://github.com/docling-project/docling/discussions/582) [Docling issue #2224](https://github.com/docling-project/docling/issues/2224) [Pipeline options](https://docling-project.github.io/docling/reference/pipeline_options/)

Production progress should therefore be wrapper-owned structured JSON Lines on a dedicated channel:

| Event | Minimum payload | UI meaning |
|---|---|---|
| `source_discovered` | document ID, safe display name, byte size | Found the file |
| `source_hashed` | document ID, source SHA-256 | Identity locked |
| `preflight_started` / `preflight_passed` | format, policy version, relevant counts | Safety checks |
| `parser_started` | parser/runtime/model/profile versions | Reading document |
| `parser_heartbeat` | elapsed time, last confirmed phase; page/batch only if authoritative | Still working |
| `candidate_created` | candidate hash and bounded counts | Read completed; validating |
| `validation_passed` | schema/tree/evidence counts | Safe to publish |
| `published` | immutable publication/version ID | Available to Manager |
| `quarantined` | stable code, safe explanation, diagnostic receipt ID | Not used; action required |

Do not parse human log prose as state, and do not display invented percentages. Until an upstream stable callback exists, elapsed time plus current phase and a heartbeat is more truthful. A page-batch adapter can be explored behind an exact-version contract test, but it is a spike, not a production dependency.

## 5. Live-agent workspace architecture

### The durable event ledger is the authority

Hermes persists complete sessions—including messages, tool calls/results, model/configuration, usage/cost, workspace metadata, and lineage—in SQLite WAL, while separating stored session history from the compressed context actually sent to a model. Its transport exposes message, tool, approval, clarification, and session-lifecycle events, plus status/stop interfaces for long-running work. [Hermes session storage at `92c998c`](https://github.com/NousResearch/hermes-agent/blob/92c998c86c8348b572b0409e3a53e380c8f60f10/website/docs/developer-guide/session-storage.md) [Hermes context compression at `92c998c`](https://github.com/NousResearch/hermes-agent/blob/92c998c86c8348b572b0409e3a53e380c8f60f10/website/docs/developer-guide/context-compression-and-caching.md) [Hermes programmatic integration at `92c998c`](https://github.com/NousResearch/hermes-agent/blob/92c998c86c8348b572b0409e3a53e380c8f60f10/website/docs/developer-guide/programmatic-integration.md)

Agent Zero's canonical repository and usage guide emphasize projects that isolate files, memory, secrets, instructions, and model presets; subordinate agent contexts that report upward; and an interface where the user can watch, pause, steer, and inspect the hierarchy. Its state monitor uses per-session sequence numbers, runtime epochs, cursor-based log tails, validated snapshots, and coalesced pushes—useful projection mechanics, but not a substitute for an audit ledger. [Agent Zero repository at `baadd0d`](https://github.com/agent0ai/agent-zero/tree/baadd0dd0b09fa769a1027c183b964be85d5c8cc) [Agent Zero usage guide at `baadd0d`](https://github.com/agent0ai/agent-zero/blob/baadd0dd0b09fa769a1027c183b964be85d5c8cc/docs/guides/usage.md) [Agent Zero state monitor at `baadd0d`](https://github.com/agent0ai/agent-zero/blob/baadd0dd0b09fa769a1027c183b964be85d5c8cc/helpers/state_monitor.py) [Agent Zero state snapshots at `baadd0d`](https://github.com/agent0ai/agent-zero/blob/baadd0dd0b09fa769a1027c183b964be85d5c8cc/helpers/state_snapshot.py)

OpenHands provides the clearest persistence primitive: an append-only typed event log that rejects duplicate IDs and missing parents, uses locking, and supports branch traversal. Its agent server persists lifecycle events before publishing them, while explicitly treating token-stream deltas as ephemeral; startup recovery turns an interrupted running conversation into an error and synthesizes an interruption result for an unmatched tool action. [OpenHands event store at `23ee276`](https://github.com/OpenHands/software-agent-sdk/blob/23ee276f1c68f08123349d103754380f627d20c8/openhands-sdk/openhands/sdk/conversation/event_store.py) [OpenHands event service at `23ee276`](https://github.com/OpenHands/software-agent-sdk/blob/23ee276f1c68f08123349d103754380f627d20c8/openhands-agent-server/openhands/agent_server/event_service.py)

Quantix should adopt the common durable pattern without copying any project's unrestricted autonomy:

- Append every typed run event to durable storage before projecting it into the UI.
- Give each event an ID, monotonic sequence, schema version, timestamp, Tender/project ID, conversation/run ID, actor/agent ID, parent event/task/run, correlation/causation ID, type, durability, redaction status, and evidence/context references.
- Persist before publish. Treat Tauri events or a future WebSocket as live delivery only. On reconnect, request events `after_seq`; if a gap is detected, load a validated snapshot plus the durable tail. Never rely on an in-memory stream as the record.
- Persist visible messages, plan revisions, task assignments, tool requests/results, approvals, steering, stops, errors, provider/model/reasoning configuration, timings, evidence links, and final artifacts.
- Do not expose hidden chain-of-thought. Expose inspectable work products and operational trace: what was planned, assigned, attempted, observed, cited, changed, approved, or rejected.

### Manager-led approval flow

The Tendering Manager should be the Engineer's single default conversation partner:

1. The Engineer opens a Tender and supplies its source documents and objective.
2. The Manager inspects validated evidence, asks only blocking questions, and drafts a versioned plan with tasks, dependencies, proposed agents, provider/model choices, limits, and expected deliverables.
3. The Engineer approves that exact plan version or requests changes.
4. Only an approved plan can start delegated work.
5. Agents receive scoped context packages, not unrestricted global state; they return evidence-linked outputs to the Manager.
6. Material scope, provider, evidence-set, or plan changes create a new approval boundary.
7. The Manager presents an evidence-linked synthesis and explicit unresolved items for final Engineer review.

Approval, denial, timeout, and cancellation are durable events. Approval must gate the executor before mutation, not merely the UI or transport: Hermes constructs concrete edit proposals, asks for sensitive paths, and denies on approval-system errors; OpenHands models a waiting-for-confirmation state and pending actions. Quantix should persist the request, exact proposed operation, decision, actor, scope, and expiry under one correlation ID, and scope permission to Tender, plan version, agent, tool, and operation. [Hermes edit approval at `92c998c`](https://github.com/NousResearch/hermes-agent/blob/92c998c86c8348b572b0409e3a53e380c8f60f10/acp_adapter/edit_approval.py) [OpenHands confirmation policy at `23ee276`](https://github.com/OpenHands/software-agent-sdk/blob/23ee276f1c68f08123349d103754380f627d20c8/openhands-sdk/openhands/sdk/security/confirmation_policy.py)

Delegation should be a child-session lifecycle rather than a hidden function call. Persist parent/child/correlation IDs, scoped capabilities, explicit pending/running/cancel-requested/terminal states, bounded immutable results, result hashes, duration, and usage/cost roll-up. A cancel request is not a completed cancellation until the child records a terminal receipt. Hermes exposes a useful public lifecycle shape, but its documented first implementation retains metadata/results in process for only one hour; Quantix must make the lifecycle durable before acknowledging it. [Hermes subagent lifecycle at `92c998c`](https://github.com/NousResearch/hermes-agent/blob/92c998c86c8348b572b0409e3a53e380c8f60f10/website/docs/developer-guide/subagent-lifecycle-api.md) [Hermes delegation patterns at `92c998c`](https://github.com/NousResearch/hermes-agent/blob/92c998c86c8348b572b0409e3a53e380c8f60f10/website/docs/guides/delegation-patterns.md)

### A beginner-readable interface with full inspectability

The same ledger should support three disclosure levels:

1. **Now:** one sentence for what is happening, who is responsible, elapsed time, and whether the Engineer must act.
2. **Activity:** a chronological human-readable timeline of meaningful plan, task, message, tool, evidence, and approval events.
3. **Trace:** exact agent context package, inter-agent messages, tool inputs/results, runtime/provider/model settings, IDs, timestamps, and receipts.

Every agent has a stable identity, role, state, current assignment, parent, scoped inputs, linked evidence, model configuration, limits, messages, tool activity, and output. The Engineer can inspect any agent and every inter-agent exchange on demand. The shared group room is a projection of durable, addressed messages and context links—not an invisible shared prompt and not a firehose on the main screen.

Steering should append an instruction event; stop should append a cancellation request and eventual termination receipt. Provider disconnection, approval timeout, or missing capability must pause/fail closed, with the exact blocked task visible. Resumption binds to the same immutable evidence and plan version unless the Engineer approves a change. Streaming queues must be bounded; a slow client should be disconnected and later reconcile from the ledger rather than apply unbounded backpressure to the run.

Stored history, active model context, summaries, and memory are different objects. Keep the immutable event/evidence store; a derived context manifest that references exact message, tool, and artifact IDs; versioned summaries with covered event ranges and source IDs; and user/project memory. Snapshot the effective provider/model/API mode/options/capabilities and prompt/config hashes per run. Unsupported provider settings must fail explicitly rather than be silently dropped.

## 6. Ship, spike, and reject

### Production-worthy now

- Explicit `SUCCESS`-only publication.
- Converter file/page limits plus child-process, memory, output, staging, tree, and evidence limits.
- OOXML central-directory preflight.
- `body` + `furniture` + picture-child traversal with container/layer provenance.
- XLSX formula-and-cache companion extraction using the existing `openpyxl` dependency.
- Approved, offline, hash-pinned OCR profiles and per-run profile receipts.
- Structured ingestion phases and heartbeats.
- Durable typed agent event ledger, replayable projections, Manager-owned versioned plan approval, scoped agent contexts, and full on-demand trace.
- Explicit run limits: iterations, wall time, cost/token budget, delegation depth, queued work, context size, and tool output.

### Time-boxed spikes before adoption

- A Docling exact-version page/batch progress adapter.
- Arabic/English/mixed OCR profile comparison on Quantix's real Tender corpus.
- Pinned qpdf recovery as an Engineer-approved derived-artifact workflow.
- LibreOffice-derived visual PDF for complex Office files and legacy formats.
- Snapshot/rollback for proposed workspace artifacts only; canonical Tender state must keep explicit version-bound approvals and immutable receipts.

### Reject for now

- Automatic fallback to Marker, MinerU, Unstructured, PyMuPDF, or another broad extractor. A second lossy representation creates conflicts that Quantix cannot safely reconcile without a format-specific adjudication contract.
- Silent document repair, source replacement, model download, formula recalculation, or parser retry under different settings.
- Parsing Docling's human logs for application state or displaying a guessed percentage.
- Showing hidden model reasoning or flooding the default workspace with raw traces.
- Giving agents unrestricted global context, tools, delegation depth, or implicit approval.

## 7. Recommended implementation sequence

1. **Close the publication hole:** inspect `ConversionResult.status`, reject non-`SUCCESS`, persist a diagnostic receipt, and add stable rejection codes.
2. **Bound the input:** pass converter limits; add OOXML/PDF preflight; enforce OS process memory and staging quotas.
3. **Complete provenance:** traverse `body` and `furniture`, validate the full reachable tree, preserve picture/layer/container relationships and geometry.
4. **Improve Tender fidelity:** add formula/cache evidence and approved OCR profiles, recording exact runtime/model manifests.
5. **Make ingestion observable:** add structured phase/heartbeat/quarantine events to a durable ingestion-event table and project them into startup and Tender import UI.
6. **Generalize the ledger:** introduce typed run/agent/message/tool/approval events with monotonic replay and bounded payloads.
7. **Build the Manager workflow:** plan draft → questions → exact-version approval → bounded delegation → evidence-linked synthesis.
8. **Add progressive disclosure:** default status, activity timeline, then full agent/context/communication trace.

Each slice should work end to end before the next begins. No extraction or agent event should become canonical until schema validation, referential integrity, evidence binding, and atomic publication all succeed.

## 8. Acceptance corpus and gates

Before declaring these upgrades verified, use a private, hash-recorded corpus containing:

- born-digital, scanned, and mixed Arabic/English PDFs;
- a full-page image whose OCR text exists only beneath a `PictureItem`;
- encrypted, corrupt-but-recoverable, malformed, oversized, and timeout-inducing PDFs;
- DOCX body text, headers, footers, tables, images, tracked layout variation, malformed relationships, and archive-bomb fixtures;
- XLSX formulas with fresh, stale, and absent caches; hidden sheets; merged cells; inflated dimensions; drawings/charts; malformed relationships; and archive-bomb fixtures;
- a deliberately orphaned/unresolved Docling reference;
- runtime interruption at each candidate/validation/publication boundary; and
- live-agent disconnect/reconnect, duplicate delivery, approval timeout, stop, provider loss, resumed run, and plan-version mismatch.

For every case, assert both positive fidelity and negative guarantees: no partial publication, no source mutation, deterministic rejection code, bounded resource use, exact source/runtime/model hashes, replayable events, and citations that still resolve to the same source region.

## Source index

### Docling

- [Docling 2.118.0 release](https://github.com/docling-project/docling/releases/tag/v2.118.0)
- [Tagged `DocumentConverter` source](https://raw.githubusercontent.com/docling-project/docling/v2.118.0/docling/document_converter.py)
- [DocumentConverter reference](https://docling-project.github.io/docling/reference/document_converter/)
- [Pipeline options reference](https://docling-project.github.io/docling/reference/pipeline_options/)
- [DoclingDocument concept](https://github.com/docling-project/docling/blob/main/docs/concepts/docling_document.md)
- [Tagged docling-core 2.91.0 document source](https://raw.githubusercontent.com/docling-project/docling-core/v2.91.0/docling_core/types/doc/document.py)
- [Tagged DOCX backend](https://raw.githubusercontent.com/docling-project/docling/v2.118.0/docling/backend/msword_backend.py)
- [Tagged XLSX backend](https://raw.githubusercontent.com/docling-project/docling/v2.118.0/docling/backend/msexcel_backend.py)
- [Tagged RapidOCR backend](https://raw.githubusercontent.com/docling-project/docling/v2.118.0/docling/models/stages/ocr/rapid_ocr_model.py)

### File safety and fidelity

- [Python 3.12 XML security](https://docs.python.org/3.12/library/xml.html)
- [Python 3.12 `zipfile`](https://docs.python.org/3.12/library/zipfile.html)
- [openpyxl optimized modes](https://openpyxl.readthedocs.io/en/stable/optimized.html)
- [openpyxl formula handling](https://openpyxl.readthedocs.io/en/3.1.2/simple_formulae.html)
- [qpdf 12 CLI](https://qpdf.readthedocs.io/en/12.0/cli.html)
- [pikepdf job API](https://pikepdf.readthedocs.io/en/latest/topics/jobs.html)
- [LibreOffice conversion filters](https://help.libreoffice.org/latest/ast/text/shared/guide/convertfilters.html)

### Agent workspaces

- [Hermes sessions](https://hermes-agent.nousresearch.com/docs/user-guide/sessions/)
- [Hermes context compression](https://hermes-agent.nousresearch.com/docs/developer-guide/context-compression-and-caching/)
- [Hermes approvals](https://hermes-agent.nousresearch.com/docs/user-guide/features/acp/)
- [Agent Zero canonical repository](https://github.com/agent0ai/agent-zero)
- [Agent Zero usage guide](https://github.com/agent0ai/agent-zero/blob/main/docs/guides/usage.md)
- [OpenHands architecture](https://github.com/All-Hands-AI/OpenHands/blob/main/openhands/README.md)
- [OpenHands conversation API](https://docs.openhands.dev/sdk/api-reference/openhands.sdk.conversation)
- [OpenHands agent server](https://docs.openhands.dev/sdk/arch/agent-server)
