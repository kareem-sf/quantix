# Juhayna First Tender — Test-Run Record

Status: in progress; this record is intentionally not a completion certificate.

This record consolidates the shared notes from the first real Juhayna tender run. It is an engineering test record for the local Quantix build, not a tender approval, a release qualification, or evidence that a submission was made.

## Scope and goals

The run exercised the first end-to-end path for a real tender package:

- start a Tender from `ManagerWorkspace`;
- import and register the selected package into the canonical Tender store;
- prepare the governed document-tool runtime;
- parse package documents into evidence;
- expose the Manager projection and the focused Bid Decision / Work Plan experience;
- verify that long-running work remains truthful, bounded, cancellable where supported, and recoverable after a failure.

The run also served as the first production-shaped observation of intake, runtime readiness, document parsing, and the current Manager/focused-action renderer slices. It did not authorize external communication or submission.

## Run identity and configuration

| Field | Value |
| --- | --- |
| Application Home | `C:\Users\kareem\.quantix-juhayna-first-tender` |
| Selected source | `C:\Users\kareem\Desktop\Juhayna` |
| Tender | Juhayna first tender |
| Tender ID | `2417df05752e1dcad6dc8fdbe51b6c11` |
| Host/build | Local Quantix debug development build |
| Provider/model | `OpenAI account via Codex` / `GPT-5.3-Codex-Spark` |
| Effort profile | Reasoning `low` (Spark-low) |
| Run isolation | A dedicated Application Home was used; the original `C:\Users\kareem\.quantix` home and source package were preserved |

The Spark-low choice was a run configuration, not a waiver of the Host's policy gates. Provider identity, execution approval, tender scope, and evidence-bound actions remained governed by the native Host.

## Package inventory and registration

The selected source contained 314 paths. Intake recorded 123 supported documents, skipped 184 other files, and marked 7 files as needing attention. That is 191 registration exceptions in the source inventory and 122 unique content blobs after deduplication. The persisted registered-media summary was:

| Registered media | Count | Total size |
| --- | ---: | ---: |
| PDF | 114 | 105.4 MB |
| XLSX | 7 | 2.61 MB |
| DOCX | 2 | 876 KB |

The warm-parse progress record first referred to approximately `77/123` documents, then reached `85/123` and `100/123`, and ultimately completed all `123/123` supported documents.

Registration exceptions were surfaced as first-class intake records rather than silently treating failed files as registered. The source inventory had 191 such exceptions: 184 files were categorized as other/non-supported files and 7 files needed attention. The exact per-file exception-code breakdown was not preserved in the shared run notes; the authoritative per-file values are the Tender Store's `DocumentRegisterEntry` records in the isolated Application Home.

## Chronology and observations

### 1. Initial Start a Tender attempt

From a fresh debug Application Home, the Engineer opened `ManagerWorkspace`, chose **Start a Tender**, and selected `C:\Users\kareem\Desktop\Juhayna`.

The UI created an active `PackageIntakeProgress` at `checking_source`, but for more than 50 seconds it remained at zero discovered, processed, and registered files. Cancel was disabled, CPU was high, the staging/tender directory was not created, and there was no native log output. A later observation showed that the staging timestamp changed while the staging directory remained empty. This localized the stall to the startup path before `fs::create_dir(stage_root)`.

### 2. Intake stall diagnosis and fix

`start_manager_tender_from_package_with_control` performed `TenderStore::create` before creating the staging directory. Store creation entered `register_sqlite_vec()`. Concurrent/re-entrant OnceLock auto-extension registration could contend during startup, leaving the intake worker spinning before any package copy or staging work became observable.

The bounded native fix moved SQLite-vector registration into controlled Host startup and made the registration callable from the store without allowing concurrent worker registration. The fix retained the existing store behavior and added focused regression coverage for the Manager tender-start path.

The result is an intake operation that can reach package progress and staging creation instead of blocking in first-store initialization.

### 3. Runtime-readiness stall diagnosis and fix

The real application then exposed a separate readiness problem. `inspect_document_tool_readiness` was called concurrently by the Manager StrictMode probe, About & Diagnostics, and the Doctor's nested probe. Each caller synchronously hashed roughly 347 MB of bundled Codex and uv provenance in an unoptimized debug binary before its first `await`. CPU was effectively one core saturated; no uv/python child remained, and `runtime_preparation` stayed `not_started`. About & Diagnostics could remain on `Inspecting…`.

The native readiness fix made inspection single-flight: concurrent callers share one bounded inspection instead of repeating the full provenance work. The blocking provenance/setup probe runs on a blocking worker, and each public inspection has a 30-second deadline that returns a typed `RuntimeProbeFailed` result rather than hanging indefinitely. Completion uses the retaining watch API so a result is not lost when the worker completes before a subscriber attaches. A completed flight remains available until its first waiter consumes the result; only then is it cleared. A later non-concurrent inspection starts a fresh flight and revalidates the filesystem, preserving drift detection rather than creating a permanent cache.

The cold-start readiness path also installed the governed document runtime and reconciled the persisted preparation state. The run observed the normal preparation/check sequence and a cold-start reconciliation after installation; readiness was not treated as healthy merely because a prior process had run.

The installed isolated runtime reached `ready` with observed component versions Python `0.147.0`, OCR `0.12.2`, and `3.9.2` for the third recorded runtime component. The installation included a roughly 500 MB local model download, OCR self-check, manifest publication, and final readiness publication.

The renderer-side correction is also important. A slow result with `repair_required`, `runtime_probe_failed`, and `repair_available: false` is treated as transient `checking`, not as proof that the runtime is damaged. Manager retries one non-overlapping inspection every 500 ms, cleans up safely on unmount/StrictMode replay, and calls `resumeManagerIntakes` once readiness is consumed. In the real cold start, the UI remained on **Checking local document tools** for roughly four to five minutes, never offered a false Prepare/Cancel path, consumed the retained Ready result, and resumed parsing automatically.

### 4. Document parsing chronology

The following timings are the run observations, not benchmark guarantees:

| Observation | Result |
| --- | --- |
| Large DOCX | A roughly 113k-word DOCX took 8m03s |
| MEP specification, attempt 8 | 261 pages, 4.16 MB; elapsed 23m37s |
| Warm document pass | Progress/count observations reached 77, 85, 100, and ultimately all 123 supported documents |

The MEP attempt number indicates repeated governed parsing attempts during the live run, not eight successful submissions or eight completed tender runs. The terminal parsing observation establishes that all 123 supported documents produced committed parsed-document records. It does not establish that the 191 registration exceptions are resolved or that the resulting evidence is approved for release.

The governed attempt history observed during the run was:

- attempt 2 was left running by an earlier controlled process stop and was reconciled to `interrupted` at the next cold start;
- attempt 3 parsed the large approximately 822,592-byte, 113k-word DOCX with about 9,090 evidence locations in 8m03s after the thread-cap fix;
- attempts 4 through 6 parsed successfully;
- attempt 7 began the 4.16 MB MEP technical specification and was deliberately interrupted by the controlled restart used to install the next fixes;
- the next cold start reconciled attempt 7 to `interrupted` without losing the five already parsed documents;
- attempt 8 retried that exact 261-page MEP specification and committed successfully after 23m37s;
- attempts 9 onward proceeded automatically through vendor/LOD PDFs, seven BOQ XLSX workbooks, architectural sheets, structural/civil sheets, and MEP drawings;
- warm files commonly completed in approximately 2–30 seconds, with denser drawings observed around 42–80 seconds;
- attempts 123, 124, and 125 completed in 13.6s, 14.9s, and 17.7s respectively;
- parsing completed at 123 of 123 supported documents. The final governed history contained 123 `parsed` attempts and two intentional `interrupted` receipts from controlled restarts, with no genuine parse failure or parse exception.

All parsed-document count changes were checked against the isolated Tender SQLite store in read-only mode. The UI and store advanced together, and every transition to the next source artifact happened only after the previous document committed atomically.

## Resource observations

- During the pre-fix readiness issue, the debug process saturated approximately one CPU core while synchronously hashing the bundled runtime provenance.
- During the first intake stall, CPU was high while no staging directory or native progress output appeared.
- Removing the hard-coded two-thread inference cap was live-observed on the MEP specification. During the parallel embedding phase, CPU rose by roughly 200–300 CPU-seconds per 30-second wall-clock sample, showing broad multi-core use instead of the former approximately one-core/two-thread behavior.
- The warmed ONNX embedding session reached an observed high-water mark of roughly 4.4 GB working/private memory on a system with about 15.8 GB visible RAM, leaving roughly 2.9–3.2 GB free during the largest batch. The value plateaued across repeated samples and Quantix remained responsive; it did not behave like an unbounded per-batch leak. The session retained this allocation to make subsequent documents fast.
- Evidence inference was already bounded by `EMBEDDING_BATCH_SIZE = 32` and `EMBEDDING_MAX_TOKENS = 512`; the 4.4 GB observation was therefore associated with the warmed multi-threaded ONNX session rather than one unbounded all-document inference call.
- Parsing remained long-running for the 113k-word DOCX and 261-page MEP specification even after the throughput correction. This remains a real deferred performance concern.
- The single-flight readiness fix addresses duplicated readiness work and indefinite readiness waiting. It does not claim to improve the underlying document-parser throughput.
- At 2026-08-21 14:50 (Africa/Cairo), attempt 125 committed and the live Manager intake record reached 123 of 123 parsed documents.

## Provider handoff after parsing

Immediately after the final document committed, Manager attempted the first Codex Agent Run and failed before a Provider Thread or Turn was established. The persisted run evidence was:

- run sequence `1`, run ID `fa201334279ed0af205a2d84f1867100`;
- selected provider/model remained `codex` / `gpt-5.3-codex-spark` with low reasoning;
- status `failed`, no provider thread reference, no provider turn reference, no token usage, and approximately 545 ms between the persisted start/completion timestamps;
- failure category `process_failed`, retry-safe, with the redacted detail `The supervised Codex process did not complete the operation.`;
- the Manager intake stage moved to `waiting_for_provider` while all 123 parsed documents remained committed and accessible.

Read-only process-tree inspection showed that the Quantix process no longer had a Codex app-server child after the long parsing interval, although the retained provider snapshot still said Ready. The first run therefore selected a closed provider actor, failed on the pre-thread request, and only then cleared that actor. The 70,260-byte permission grant and the absence of a Provider Thread show that this was not an accepted model turn or a tender-result-quality failure.

A bounded stale-provider replacement was implemented and its focused regression passed. Live retries then exposed a second, independent blocking defect. Runs 2, 3, and 4 also failed before a Provider Thread was established. An exact direct protocol reproduction against the bundled `codex-cli 0.147.0` proved that initialization, account inspection, and the seven-model catalogue all succeeded, including `gpt-5.3-codex-spark`. The first `thread/start` request was rejected with JSON-RPC `-32600` because Quantix sent the sandbox mode as `workspaceWrite`; this API field requires `workspace-write`. The app-server exited cleanly only after diagnostic stdin was closed, so the stored generic `process_failed` result also obscured a concrete provider-protocol error. Quantix was corrected to send the exact `workspace-write` enum while retaining `workspaceWrite` for the separate Turn sandbox-policy type. Live run 5 then established a real Provider Thread and Turn, proving this correction.

After correcting the thread sandbox enum, run 5 established Provider Thread `01a02459-2a01-7e81-b50b-e0de2ac5fe60` and Provider Turn `01a02459-3156-7062-9e6f-1da5c726d1be`. The Turn started and observed available subscription capacity, then failed after 13.82s without token usage. The governed Agent Run again stored a generic retry-safe `process_failed` failure. The local Codex rollout contained the exact upstream cause: OpenAI rejected the strict response schema with `invalid_json_schema` because `generation_instruction` used `oneOf`, which is not permitted in that structured-output context. Quantix was corrected to express the same strict nullable object as `type: ["object", "null"]`, retaining the exact properties, required list, and `additionalProperties: false` validation.

The next GUI retry refreshed the persisted Manager provider binding and re-entered the pipeline, but it failed in approximately one second without creating Agent Run 6. Database inspection localized this third retry blocker before the provider boundary: the immutable Tender Analyst profile version `3` persisted by runs 1–5 still contained the old output contract, while the in-code profile with the same version contained the corrected contract. Quantix correctly rejected this silent mutation with `IntegrityFailed`, but the background Manager path collapsed that exact Host error into the same generic intake failure. Advancing the Tender Analyst profile version to `4` retained immutable v3 and bound the corrected contract to a new identity. The next live GUI retry created Agent Run 6 with profile v4 and entered `extracting_tender_facts`, proving the retry and profile correction.

Run 6 established Provider Thread `01a0246c-c3d4-74a3-aa9e-e0d18e437472` and Provider Turn `01a0246c-c6de-7310-8cb5-634de29aa7ee`. Codex completed the structured task in 14.24s and emitted 114,966 input tokens, 5,967 output tokens, 3,860 reasoning-output tokens, and 120,933 total tokens against a 121,600-token context window. Quantix persisted usage and rate-limit observations but then left the Agent Run in `running` while the Manager moved to `failed`. The run workspace had been partially deleted and retained only an empty `working` directory. Process inspection showed that the persistent Codex app-server still owned a `node_repl` child after the Turn; on Windows, that child retained a handle/current directory inside `working`. The 20 x 25ms cleanup retry therefore ended with a Windows removal error, and `dispose_workspace` returned before the transaction could terminalize the Agent Run.

The first cleanup correction accepted a verified directory-only residual tree only for `PermissionDenied`. After restart recovery safely reconciled run 6 to `indeterminate`, the Engineer closed that uncertain run through the real GUI and Quantix created run 7. Codex again completed the structured Turn, this time reporting 115,031 input tokens, 10,155 output tokens, 5,819 reasoning-output tokens, and 125,186 total tokens. Run 7 reproduced the same false `running` state and again left only an empty `working` directory. This proves the final Windows removal error was surfaced under a different `std::io::ErrorKind`; the bounded follow-up is to classify the residual tree after any final Windows removal error while retaining fail-closed behavior for every file, data-bearing entry, link, unsafe entry, or inspection failure.

The live renderer also made the retry CTA difficult to reach: the fixed Manager composer covered the lower part of the current-action card until the conversation was scrolled. Accessibility clicks while the CTA was occluded returned an unknown action/refresh outcome. Scrolling exposed the same button and produced visible `Reconnecting Tender intake…` feedback. This is recorded as a later layout/input-feedback fix because the scroll workaround allowed the test to continue.

## Manager and focused Bid Decision / Work Plan slice

The renderer slice exercised in this run was deliberately focused:

- Manager `current_action: review_bid_decision` opens the real focused Bid Decision experience, using `BidDecisionPanel` rather than scrolling to an old message or reviving the obsolete permanent module catalogue.
- After accepting the bid decision, the focused flow supports composing and reviewing the Work Plan in the existing `TenderOfficePanel` path.
- `prepare_work_plan` and `review_work_plan` actions are wired to focused experiences.
- Work Plan approval is exact and explicit; activation is a separate explicit action.
- If activation fails after approval, the approved plan remains visible as an approved-plan retry state. The UI does not fake a rollback or claim activation succeeded.
- Successful mutations notify the Manager so its projection is refreshed after each mutation.

Focused verification covered action routing, bid acceptance into plan work, exact approval into activation, activation failure with retry, and stale-refresh behavior. The current post-activation Manager Work surface is still a projection summary; the actionable production Work controls remain a pending lifecycle slice (see below).

## Pending lifecycle slices

The first run did not establish the complete tender lifecycle. The architecture review found that the native Host already contains most governed lifecycle commands and that the principal gap is truthful Manager/Work orchestration. The ordered remaining slices are:

- **Production Work:** mount the existing `TenderOfficePanel` production controls in the Manager Work surface so running tasks can be interrupted, query-blocked work can request specialist evidence/treatment, artifact reviews can be inspected, and permitted Major findings can receive exact exceptions. The native scheduler and commands already exist; renderer coverage is the missing slice.
- **Commercial chain:** expose controlled calculations, Basis of Estimate, priced baseline, adjustments, strategy/scenario decisions, and the exact Tender Price using the existing governed panels.
- **Coordinated baseline:** assemble and approve the exact coordinated bid baseline before package production.
- **Package production:** generate governed submission sections, assemble the complete submission package, and preserve coverage/uncovered-requirement truth.
- **Final review and release:** run package validation, manual verification, section reviews, and approved finding exceptions before approving release and exporting a verified Release Copy.
- **Obsolete surface removal:** remove the old `TenderWorkspace` permanent module catalogue only after the replacement path covers its current responsibilities. It must not be revived as a compatibility layer.
- **Package reconciliation:** reconcile all parse/register outcomes and any unresolved intake exceptions before release.

These are product work items, not evidence that the current Juhayna run failed or succeeded at those later gates.

The native completion condition is `SubmissionReleaseState::ReadyForSubmission` plus a verified `ReleaseCopyExport`. Even that state means Quantix produced a verified release copy; it does not mean the tender was externally submitted. A critical missing native regression is an end-to-end Rust test that actually exercises `approve_submission_release` and `export_release_copy`.

## Bugs, fixes, and regression evidence

| Area | Root cause | Bounded fix | Regression evidence |
| --- | --- | --- | --- |
| First Tender intake | Re-entrant/concurrent OnceLock SQLite-vector registration occurred inside `TenderStore::create` before staging creation | Register the extension during controlled Host startup; keep store registration non-racing | Native Manager-start/intake regression and focused store/workspace unit tests passed |
| Runtime readiness | Repeated callers synchronously hashed ~347 MB of bundled provenance before the first await | Single-flight shared inspection, blocking worker, 30s typed deadline, retaining completion, consume-then-clear semantics | Focused host readiness concurrency/late-consumer test passed; subsequent calls still revalidate |
| False runtime repair UI | The renderer could interpret a slow transient probe as a damaged/missing runtime and offer reinstall/cancel controls | Treat the exact non-repairable probe timeout as `checking`, retry one request at a time, and consume the retained Ready result | Three focused transient-readiness renderer tests passed; real cold start stayed truthful and resumed intake |
| Lost readiness publication | `watch::Sender::send` could discard a completed result when no receiver was subscribed at publication time | Use retained replacement semantics and keep a completed flight until the first late consumer receives it | Focused late-consumer native test passed |
| Blank Tender AI controls | Provider/model controls depended on catalogue/readiness completion and could render empty during startup | Load runtime and application settings independently and project the persisted exact selection before catalogue hydration | Manager renderer tests and TypeScript checks passed; Spark/low remained visible throughout the real cold start |
| File exception projection | Registered documents and registration failures were mixed or omitted, hiding raw Host details | Add `registration_state` and `exception` to the native DTO, regenerate committed bindings, and split Registered / Registration exceptions with raw codes and provenance | Rust projection coverage plus the 43-test renderer set passed; real Files view showed 123 Registered and 191 Registration exceptions |
| Embedding throughput | `TextEmbedding` was initialized with a hard-coded `.with_intra_threads(2)` cap | Remove the cap and allow the maintained library/runtime to select available cores | Focused Rust checks passed; the real MEP run visibly used broad multi-core inference |
| Repeated identical evidence | Exact duplicate evidence strings were embedded repeatedly within one call | Deduplicate after applying the existing prefix/trim behavior, infer unique strings once, validate output, then reconstruct one stable vector per original location | Three pure helper tests passed; order/count/error semantics preserved |
| Stale Codex provider after long intake | A terminated provider actor could remain in the Host slot and still appear live while idle | Refresh readiness before each Codex turn; on retry-safe pre-turn process failure, discard the stale actor and create one fresh provider without retrying after turn acceptance | `a_dead_idle_provider_is_replaced_before_the_next_turn` and the existing pre-turn process-failure regression passed; the next live retry reached the separate sandbox-enum blocker |
| Codex `thread/start` sandbox spelling | Quantix sent `sandbox: "workspaceWrite"`; Codex 0.147.0 requires the `SandboxMode` value `workspace-write` (the separate turn `SandboxPolicy.type` remains `workspaceWrite`) | Send the exact thread-start enum and assert the wire contract in the runtime fixture | Direct live protocol reproduction isolated JSON-RPC `-32600`; live run 5 subsequently established Thread and Turn references |
| Tender Record output schema | The extraction contract represented the nullable `generation_instruction` with `oneOf`; the strict Responses schema rejected it before inference | Express the same nullable object as `type: ["object", "null"]` while retaining exact required fields and Host validation | Live run 5 reached a real Thread/Turn and the local rollout recorded `invalid_json_schema`; focused contract coverage added, with post-fix live retry blocked earlier by the profile-version invariant |
| Immutable profile not versioned with contract | The Tender Analyst output contract changed but its immutable profile version remained `3`; an existing DB therefore compared old and new contracts under one identity and rejected retry with `IntegrityFailed` before creating a run | Advance the Tender Analyst profile version to `4` with the contract and update focused expectations | The next real GUI retry created Agent Run 6 with profile v4 and reached a successful Codex structured Turn |
| Windows workspace cleanup blocks terminalization | The persistent Codex process retained an empty `working` directory handle after successful Turns; `remove_dir_all` deleted the data but returned Windows removal errors under more than one error kind, and cleanup failure occurred before the Agent Run terminal UPDATE | In progress: after any final Windows removal error, accept only a verified directory-only residual tree while continuing to fail closed for files, data, links, unsafe entries, or inspection errors | Live runs 6 and 7 both completed at Codex, persisted usage, then remained falsely `running`; each residual workspace contained only the locked empty `working` directory |
| Provider failure diagnostics | Concrete pre-turn RPC errors and failed-turn structured-output errors are collapsed into generic `process_failed` summaries, requiring local rollout inspection | Deferred: preserve a bounded, redacted error code/category such as invalid sandbox enum or `invalid_json_schema` without exposing Tender content | Direct protocol and rollout evidence recovered the hidden causes for runs 1–5 |
| Manager retry CTA visibility | The fixed composer can cover the current-action CTA at the bottom of the Manager timeline; an occluded accessibility click can have ambiguous feedback | Deferred: keep the active CTA visible above the composer and make in-flight/accepted input feedback unambiguous | Real GUI workaround verified by scrolling; dedicated renderer regression pending |
| Focused activation UX | An approved plan could be lost or falsely shown as rolled back after activation failure | Preserve the approved plan and expose an explicit retry state | Focused renderer activation-failure/retry test passed |
| Manager action routing | Bid/Work Plan actions could route to obsolete or non-focused surfaces | Route current actions to the real focused panels and refresh projection after mutations | Focused Manager routing and focused-action tests passed |

## Verification evidence

The following checks were recorded for the bounded changes:

```text
npx vitest run src/TenderFocusedAction.test.tsx src/ManagerWorkspace.test.tsx -t "focused|routes review bid decision"
  2 test files passed; 2 selected tests passed; 39 tests skipped

npx tsc --noEmit
  passed

npx prettier --check src/ManagerWorkspace.tsx src/ManagerWorkspace.test.tsx src/TenderFocusedAction.tsx src/TenderFocusedAction.test.tsx src/TenderFocusedAction.css src/TenderOfficePanel.tsx
  passed

git diff --check
  passed
```

The native focused readiness test passed after the late-consumer retaining-watch fix. The SQLite-vector startup/intake regression and the focused Tender Store/workspace unit tests also passed. These are code-level checks; they do not substitute for a terminal live-tender result.

Additional recorded checks included:

- the full renderer set around the file projection completed with 43 passing tests;
- focused readiness renderer coverage completed with 3 passing tests and 35 skipped by its filter;
- `cargo check --manifest-path src-tauri/Cargo.toml --lib --no-default-features` passed after both embedding changes;
- focused embedding helper coverage completed with 3 passing tests;
- Rust formatting and targeted `git diff --check` passed for the touched native files;
- the real cold restart reconciled the running parse receipt, preserved five committed parsed documents, consumed retained readiness, and automatically created the next parse attempt.

The principal files changed during the run were `src-tauri/src/host.rs`, `src-tauri/src/tender_store.rs`, `src-tauri/src/tender_store/workspace.rs`, `src-tauri/src/embedding.rs`, `src/ManagerWorkspace.tsx`, `src/ManagerWorkspace.test.tsx`, `src/TenderOfficePanel.tsx`, `src/TenderFocusedAction.tsx`, `src/TenderFocusedAction.test.tsx`, `src/TenderFocusedAction.css`, and generated declarations under `src/bindings`. Generated declarations were regenerated rather than edited manually.

## Deferred performance work

After this run, sustained document-parse throughput remains explicitly deferred for a later, separately measured performance task. In particular, the 23m37s MEP-spec observation and the long warm pass are not considered fully solved by removing the thread cap or deduplicating exact inputs. The later task should measure extraction time separately from embedding time, record per-document evidence-location and unique-input counts, evaluate an appropriate CPU/memory balance for the ONNX session, and add durable progress telemetry within a document. Any optimization must preserve governed execution, provenance validation, cancellation, bounded progress, evidence integrity, stable location-to-vector ordering, and truthful per-document outcomes.

Native development rebuilds also repeatedly left the renderer visible but not operational for roughly four to seven minutes while runtime readiness completed; competing Rust compilation made the delay longer. The UI ultimately recovered without data loss, so this is deferred rather than treated as the current tender blocker. A later task should separate compile/restart time, runtime provenance hashing, provider startup, and renderer hydration in telemetry so the user sees one truthful progress state instead of an apparently hung application.

The parsed package produced 17,312 evidence locations. With the current fixed 256-location extraction batches, Manager planned approximately 68 sequential Tender Analyst turns before independent record review. The first successful batch consumed 120,933 of a 121,600-token context window. This run continues unless it becomes a correctness blocker, but batching by raw location count is a high-risk performance and context-headroom issue: a later optimization should batch by measured serialized/token size, retain stable exact-evidence coverage, and expose total-batch/current-batch progress without weakening provenance.

## Truthful current status

The Juhayna run is a partial, still-in-progress engineering observation. It demonstrated and fixed bounded native startup/readiness failure modes, completed governed parsing for all 123 supported documents, and exercised the focused Bid Decision / Work Plan renderer slice. Provider run 5 proved that Quantix can establish a real Codex Thread and Turn with Spark/low; that Turn exposed and led to correction of the strict Tender Record output schema. Profile v4 then allowed live runs 6 and 7 to complete real structured extraction Turns successfully. Run 6 was recovered to `indeterminate` and explicitly closed before retry; the Manager is currently at `failed` and run 7 is falsely `running` because the first Windows empty-residual correction covered only one OS error kind. Quantix remains responsive, all parsed evidence and exact provider observations remain committed; the generalized cleanup/terminalization correction and another live retry are in progress. The available evidence does **not** establish that all registration exceptions were resolved, that the Work Plan was activated successfully, that a tender was complete, or that anything was externally submitted. No such claim is made here.

## Continuation after operational diagnostics implementation

The secure operational diagnostics implementation was completed before the next live desktop retry. The repository now writes a redacted application journal and isolated Tender journals, exposes a merged diagnostics timeline, supports temporary Tender-scoped deep diagnostics and redacted support bundles, and integrates diagnostics health with Quantix Doctor. The Juhayna-critical path now emits bounded events for startup, package intake, per-artifact parsing, embedding, Manager intake, Agent Runs, Provider Turns, retry/repair boundaries, and final outcomes. Raw Tender content, filenames, absolute paths, prompts, responses, tool payloads, provider traffic, credentials, and hidden reasoning are excluded by the typed intake schema and scrubber.

Repository verification after the diagnostics changes passed `npm run verify`: 111 renderer tests, 699 Rust library tests with two explicit live tests ignored, and every integration suite passed. The deterministic Juhayna-like diagnostics regression also passed and proved correlated parsing, embedding, and Provider failure events without the injected content sentinel leaking into normal logs or support exports.

The first native diagnostics journal was then created under the isolated Juhayna Application Home:

```text
C:\Users\kareem\.quantix-juhayna-first-tender\logs\application\2026-08-21\000001.jsonl
```

The recorded startup events used schema version 1, application scope, safe summaries, monotonic session sequences, and no source paths or Tender content. At this point no new Tender operation had yet run, so the absence of a Tender journal was correct stream isolation rather than missing attribution.

### Installation schema identity defect found by acceptance

Native acceptance initially rejected the default Quantix home as `SetupRequired`. Read-only SQLite inspection proved the database passed `quick_check`, reported installation schema 22, and contained no active update. Comparing its exact `sqlite_schema` to a clean home localized the mismatch to the intentionally removed `provider_cleanup_jobs.deletion_id -> deletion_receipts.deletion_id` foreign key. The table had changed without advancing the installation schema identity.

The product schema was advanced to 23 with no compatibility layer or migration. A byte-identical SHA-256-verified backup of the Juhayna installation catalogue was created before touching the test environment. The Juhayna catalogue already matched every schema-23 table, index, and trigger except the `installation.schema_version = 22` check. A one-time, fail-closed local test-environment correction rebuilt only that metadata table inside one transaction. Post-correction `quick_check`, `foreign_key_check`, the Juhayna catalogue identity, and a complete schema-object comparison against a clean schema-23 home all passed. No Tender database, evidence object, runtime, model, or source package was modified.

### First deterministic acceptance attempt after schema correction

The first deterministic native acceptance attempt reached the challenged candidate and verified candidate identity, schema 23, runtime provenance, accessibility, bounded input/memory/output/time, estimating, EITL fixture facts, update idleness, and the public acceptance corpus. It failed later lifecycle gates. Timing evidence separated two independent costs:

- approximately 306 seconds inside candidate Host lifecycle, with most of that elapsed before the acceptance fixture Tender was created because the existing Juhayna home was cold-inspected;
- approximately 610 seconds across the complete deterministic driver invocation.

The persisted fixture Tender contained the exact acceptance content but no Agent Run. Code and database tracing then proved deterministic acceptance had drifted behind the current per-Tender AI binding contract: its internal fake Provider path still required a live, Engineer-approved Tender selection. The acceptance permissions assertion had also drifted behind the current isolated-workspace contract by expecting `workspace_write_allowed = false`; current bootstrap profiles permit writes only inside their staged Agent Run workspace while remaining network-denied and prohibited-action bounded. These are acceptance-harness defects, not evidence of a successful or failed Juhayna Tender outcome. A focused test-first correction and a clean, prepared acceptance home are in progress. The five-minute cold inspection remains a recorded performance issue even if the corrected clean-home acceptance passes.

### Authoritative Juhayna state before the next desktop retry

Read-only inspection after the earlier live continuation found ten durable Agent Runs, not seven. Runs 1-5 are terminal failures, runs 6-8 are `indeterminate` with explicit Engineer close dispositions recorded for 6-8, run 9 is a retry-safe `output_invalid` failure after a real Provider Thread and Turn with 121,475 total tokens, and run 10 is an unresolved `indeterminate` run. The Manager intake is `failed`, all 123 supported documents remain parsed, `manager_intake_extraction_batches` is empty, and no Tender Record has been published.

This supersedes the earlier run-7-only status paragraph above. The next production-shaped desktop action is to reconcile and explicitly close run 10, retry Manager intake under deep diagnostics, and use the new correlated journal to diagnose any next blocker. The Tender is still incomplete; no Bid Decision, Work Plan activation, production package, formal approval, or external submission is claimed.

### Provider JSON canonicalization blocker

The local Codex rollout for run 9 contained a valid Tender Record extraction object with five schema-shaped records. The returned text was 10,715 bytes; RFC 8785 canonicalization produced 10,714 bytes. A byte-level comparison found exactly one insignificant ASCII space outside every JSON string and no semantic, ordering, or schema difference. Quantix rejected the otherwise valid structured response as `output_invalid` because the Tender Record validator required the Provider's raw response bytes to equal the canonical serialization before it would perform domain validation or persist the candidate.

This is a Host completion-boundary defect. A structured Provider is allowed to return semantically valid JSON containing insignificant whitespace. Quantix should parse and canonicalize a valid completed candidate once at the Agent Run boundary, run the existing fail-closed domain validators against those canonical bytes, and persist only those canonical bytes. Malformed JSON must continue to fail as `output_invalid`; domain validation and persistence invariants must remain unchanged. A regression that injects the same outside-string whitespace is being added test-first before the bounded completion-boundary correction. This defect, rather than the content of the five proposed records, is the immediate run-9 retry blocker.
