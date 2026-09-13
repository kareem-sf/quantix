# Quantix domain and local-data audit

9 September 2026. Source inspection of the current working tree, including uncommitted changes. This report covers backend domain workflows and local data; the separate AI and interface audits own provider internals and rendered UX.

## Assessment

The existing local application architecture is appropriate for an individual Tender engineer. Keep FastAPI, SQLite, the content-addressed original store, explicit Pydantic request models, and the separate domain services. A replacement framework, distributed queue, vector server, or generic workflow engine would add complexity without resolving the defects below.

The immediate priorities are to bind mail credentials to their actual destination, make failed automatic Manager handoffs visible, and correct which source records a fresh output depends on. Import coverage, extraction-cache publication, and damaged-database recovery also need bounded fixes. These are source-supported findings, not reproduced failures or a statement that the application has passed acceptance.

Severity: **P1** blocks safe acceptance of the affected capability; **P2** is a concrete correctness or recovery defect to address in the next working increment. Confidence describes the supporting source chain, not executed verification.

## Findings

### D01 — P1: Saved mail credentials are not bound to the destination account

**Trigger and consequence.** Save an SMTP or IMAP password, then change the host or username without supplying a replacement password. Quantix retains and uses the old secret with the new destination. Restoring an older workspace can produce the same mismatch: the database restores older mail settings while the OS credential store retains newer credentials. SMTP sending and mailbox synchronization can disclose a password to a destination/account it was not saved for. Do not accept real supplier-mail use until this is fixed.

**Evidence.** `backend/quantix/correspondence.py:137–139` derives one keyring namespace from the workspace. `_password` at `260–264` retrieves a secret using only `smtp` or `imap`. `update_settings` at `282–308` merges host/username changes but changes the keyring entry only when the corresponding password is supplied. SMTP uses the resulting endpoint and retained password at `528–565`; IMAP does so at `751–785`. `backend/quantix/backup.py:134–147` snapshots the settings database; neither restore nor the mail password key includes the endpoint identity. Exact-message send approval includes public account details (`correspondence.py:426–431`) but does not remedy secret reuse.

**Minimal change.** Bind each mail credential to a canonical account identity containing protocol, host, port/security and username. On a destination identity change, require a freshly supplied credential or an explicit reviewed transfer; do not silently reuse the previous secret. Keep old identity-bound credentials inaccessible to the new identity. Revalidate this binding after restore, and fail before network access when it differs. Apply one shared rule to both SMTP and IMAP; no new account-management framework is needed.

**Later acceptance example.** Save account A, switch to account B without a password, and confirm that no login request occurs. Restore account A's settings after saving account B's credential and confirm the same guard. No real mailbox or supplier action was performed during this audit.

**Confidence:** high. Credential retention and subsequent use are explicit in the call chain.

### D02 — P1: Old, undisplayed Manager messages can permanently block a fresh analysis export

**Trigger and consequence.** A Manager message cites a source, that source is revised, and the Manager writes a new analysis using current evidence. A newly generated analysis DOCX still collects the source IDs of *every* historical message. The final export checks then reject the old source even though the document shows only the latest Manager analysis. Generating the document again does not resolve the problem because message history is retained. This blocks an ordinary revision-to-submission workflow.

**Evidence.** `backend/quantix/outputs.py:271–273` puts all messages and the overview into the analysis capture. The recursive collector at `348–365` traverses every nested `source_id`, `source_ids`, and `source_ids_read`; `373–374` adds a blocker for superseded sources. `backend/quantix/output_word.py:91–98` renders only the latest Manager message. `outputs.py:427–446` rejects every noncurrent captured source, and `backend/quantix/submissions.py:75–85,111–116` prevents final export. The history-returning query is `backend/quantix/repository.py:297–305`.

**Minimal change.** Build an explicit output snapshot containing the exact selected Manager result and the records actually rendered. Collect source dependencies from that snapshot. Keep prior messages inspectable in history without making them current-document dependencies. Decide explicitly which historical findings the document includes; if it contains a historical appendix, label that appendix and its version rather than treating those sources as current factual support. Do not weaken current-source validation for the live analysis.

**Later acceptance example.** Create a cited analysis against revision 1, import revision 2, save a current analysis, and generate a fresh DOCX. Its source manifest must contain the selected result's sources; an unrelated old message must not block a valid export. A genuinely stale source in the selected result must still block it.

**Confidence:** high. The captured records and rendered records visibly differ.

### D03 — P2: A failed automatic Manager handoff disappears after successful work

**Trigger and consequence.** An import completes, or the last specialist task completes, but automatic Manager startup fails—for example because account readiness or approved AI authority needs attention. The previous work remains completed, no Manager run is created, and the startup error is discarded. To an engineer, the Manager simply does nothing after the package is imported or work finishes.

**Evidence.** `backend/quantix/jobs.py:194–221` commits the original run. Automatic analysis and consolidation start afterward at `223–239`. The exception handler at `242–258` records an error only if the original run is still `queued` or `running`. A failure from `start_manager` therefore has no visible failure record once the original run is completed. That method preflights policy before queue creation at `jobs.py:76–91`.

**Minimal change.** Handle follow-up startup separately from the completed parent work. Save a structured `manager_followup_needed` event and a short Manager/system notice with one next action, such as “The files are saved. Review AI access, then start the analysis.” Preserve the real error category and a diagnostic ID. Do not relabel a successful import as failed, and do not silently switch accounts or models.

**Later acceptance example.** Cause only Manager startup preflight to fail after a completed import or last task. The completed work must stay available and a durable, actionable handoff failure must remain after reopening.

**Confidence:** high. This is a direct terminal-state/control-flow mismatch.

### D04 — P2: Unreadable subfolders can be omitted without a coverage exception

**Trigger and consequence.** A selected Tender folder contains a subfolder whose directory listing is denied or unavailable. The traversal can skip that directory without creating a candidate or warning. The resulting import summary reports only the files it encountered, leaving the engineer unaware that part of the package was never registered.

**Evidence.** `backend/quantix/intake.py:38–43` calls `os.walk(root, followlinks=False)` without an `onerror` handler. Per-file exception handling at `65–86` applies only after filenames have been returned, so it cannot account for an unreadable directory listing. The final summary at `295–307` is based on the resulting candidate list and repository counts.

**Minimal change.** Supply an `onerror` handler that registers an explicit directory-level coverage exception with the relative location, a safe reason, and the next action. If the selected root cannot be enumerated, fail with the specific root-access error. Count unknown directory coverage separately from known registered files; do not invent the number of files inside the inaccessible directory.

**Later acceptance example.** A synthetic package with one readable file and one inaccessible subdirectory must preserve the readable file and report the unenumerated folder, rather than appearing fully enumerated.

**Confidence:** high. The directory error path is absent; no filesystem-permission experiment was run.

### D05 — P2: Concurrent imports share one extraction-cache temporary filename

**Trigger and consequence.** Two Tenders import the same uncached document at the same time. Both workers write the same `.partial` extraction file. One can move it away before the other's `os.replace`, creating a spurious failed artifact; overlapping writes can also expose an invalid cache. Separate Tenders are allowed to run concurrently, so this is not excluded by the job lane.

**Evidence.** `backend/quantix/intake.py:210–224` uses the common digest/suffix/version cache and derives a single temporary path with `cache.with_suffix('.partial')`. There is no cache-key lock. `backend/quantix/jobs.py:129` locks per Tender and dispatches imports to worker threads at `138–145`. Import catches the resulting `OSError` and records a failed extraction at `intake.py:270–278`.

**Minimal change.** Give each writer a unique temporary file in the extraction directory, flush it, and atomically replace the completed cache. Optionally use a small per-cache-key lock to avoid duplicate parsing. Readers should validate the cache structure/version and rebuild malformed derived content. Preserve the content-addressed original behavior.

**Later acceptance example.** Concurrent synthetic imports of one identical document into two Tenders must both register the correct evidence without sharing a temporary path or producing a failed artifact.

**Confidence:** high for the race; occurrence frequency is unmeasured.

### D06 — P2: The restore recovery path still requires a readable, valid main database

**Trigger and consequence.** A valid backup is already staged, but the current `quantix.sqlite` becomes corrupt before restart. Restore attempts to back up the damaged current database and aborts before installing the valid backup. The documented damaged-state capture handles missing/changed original or output files, but it does not recover from damage to the main SQLite database itself.

**Evidence.** `backend/quantix/backup.py:673–680` catches only `_ReferenceBackupError` when saving the previous workspace. That error covers unavailable/modified referenced files (`245–269`). `_snapshot_database` and `_database_references` at `134–159` require a readable database and successful integrity checks. Even `_capture_recovery_evidence` begins with those same operations at `299–303`. `backend/quantix/__main__.py:42–44` applies restore before app creation and lets errors abort startup, leaving no normal in-app recovery UI.

**Minimal change.** Add a narrowly defined offline path for SQLite corruption: retain the original database and available WAL/SHM bytes as explicitly unverified recovery evidence before replacing anything, then install the already inspected backup. Do not swallow permission, disk-full, unsafe-path, or inability-to-preserve-data errors. Expose a recovery entry point that can inspect/stage a backup without first constructing the damaged Repository.

**Later acceptance example.** Use an isolated synthetic workspace with a staged valid backup and a deliberately damaged current database. Restore must retain the damaged bytes and publish the valid snapshot, or give a precise preservation error. This destructive scenario was not executed.

**Confidence:** high for the failure path. No claim is made that a customer database is damaged.

### D07 — P2: Reimport cannot refresh evidence after a reader becomes available or improves

**Trigger and consequence.** A legacy DOC is imported while Word conversion is unavailable, or a document is partially extracted. The engineer later installs/fixes the reader and imports the unchanged source again. The cached unsupported/partial result is reused. Even if the extraction cache is manually rebuilt, the repository returns the old artifact when the original hash and old supported status match. The ordinary reimport action cannot repair that evidence without modifying the source file.

**Evidence.** `backend/quantix/intake.py:20,212–218` uses a fixed extraction version and reuses `unsupported` and `needs_attention` cache results. `backend/quantix/repository.py:88–93` skips registration whenever the hash is unchanged and the saved status is `extracted`, `needs_attention` or `unsupported`; it never compares the new extraction. The optional Word conversion reports reader unavailability at `backend/quantix/documents.py:215–236`. There is no reprocessing API in `backend/quantix/api.py`.

**Minimal change.** Add a clearly named “Read this file again” operation using preserved bytes. Retain original-file version/hash separately from extraction version and reader capability. Preserve old source evidence when replacing the active derived extraction, and mark dependent findings/approvals for review when their evidence basis changes. At minimum, allow explicit bypass of unsuccessful caches and avoid discarding a newly successful extraction.

**Later acceptance example.** Import a synthetic DOC with an unavailable converter, make a stubbed converter available, and reprocess the preserved original. New evidence must become available while the original bytes/hash remain unchanged and old evidence stays auditable.

**Confidence:** high. Both cache and repository short circuits are explicit.

## Persistent diagnostics requested for `~/.quantix`

This is an implementation requirement and maintainability gap, not evidence of an observed customer-data incident. The audited core starts Uvicorn with warning-level console logging and access logging disabled (`backend/quantix/__main__.py:50–53`). The Windows source launcher redirects output to repository-local `.quantix-dev/desktop.out.log` and `.quantix-dev/desktop.err.log` (`scripts/start.ps1:3–7`). `jobs.safe_error` returns a truncated exception string with limited key-pattern replacement (`backend/quantix/jobs.py:15–20`). Durable run events are useful business history, but they are not a consistent crash/startup/diagnostic log and should not become one.

Use standard Python logging with a rotating UTF-8 handler under `Path.home() / '.quantix' / 'logs'`, initialized before repository/startup work. Have native/source launch paths retain startup failures in the same user-owned diagnostic area. Keep the existing application data location explicit; the new log requirement does not require moving or merging Tender workspaces.

Useful fields are UTC time, application version, session/operation ID, component, safe error code, duration and outcome. Keep customer filenames, extracted text, prompts, email addresses/bodies, request bodies, account tokens, credential values and unrestricted provider stderr out of ordinary diagnostic records. Store detailed business content only in its existing local Tender record. Present a short action and diagnostic ID in the UI; expose “Open logs” or an explicit redacted diagnostic export under More options. Rotation and retention should be bounded. A diagnostic export must be user-reviewed; do not automatically upload it.

## Healthy choices to preserve

- **Originals and revisions:** content-addressed copies, SHA-256 checks, original-byte downloads, recorded relative paths, and separate derivative exports. Archive intake rejects traversal, linked entries, encryption and bounded expansion cases. Original client workbooks are patched as copies rather than round-tripped through a library that could discard unrelated members.
- **Commercial authority:** Decimal calculations, per-currency totals, unknown VAT states, explicit source confirmation, separate quantity/rate proposals, fingerprint-bound rate approval, and immutable decision ledgers. Candidate identification is labelled as incomplete BOQ coverage.
- **Document honesty:** bounded PDF/Office extraction; cell addresses, formulas and caches stay distinct; hidden rows/sheets and partial coverage are reported; DOC/DWG limitations are explicit. Word conversion uses a temporary copy and disables macros/automatic link updates.
- **Local architecture:** synchronous transactional repository methods, thread-local SQLite connections, foreign keys, WAL and full synchronization; jobs commit queue records before scheduling; Office publication and terminal state are grouped atomically. Source files are outside the repository and common private artifacts are ignored.
- **Search and knowledge:** FTS exact search and a separate rebuildable local semantic index; pinned embedding-model revision; current-source joins; no disguised keyword fallback; reusable notes remain separately approved and price/tax reuse always requires revalidation.
- **Submission and correspondence controls:** exact-content send approval, delivery-attempt reservation before network access, a separate append-only delivery journal that is not rolled back with the workspace, read-only IMAP selection, fingerprint-bound local submission exports, and current requirement/output review bases.
- **Backups:** SQLite snapshot API rather than copying a live main database alone, archive path/hash/size validation, backup identity bound to approval, database installed last, explicit restore history, and preservation of newer mail delivery history.

## Minimal improvement sequence

1. Bind SMTP/IMAP secrets to account identity and review restore behavior before real mail acceptance (D01).
2. Add persistent redacted diagnostics and the visible Manager follow-up error path (D03). This directly improves the investigation of apparently inactive AI without changing provider behavior.
3. Correct the analysis output snapshot and its source dependencies (D02).
4. Make directory coverage exceptions and extraction-cache publication reliable (D04–D05); then add explicit source reprocessing (D07).
5. Complete the offline corrupted-database recovery path (D06).

Keep the frontend surface small: one next action for the current state, with source history, commercial detail and diagnostics under More options. Avoid removing technical capability to obtain visual simplicity.

For dependency reproducibility, retain maintained libraries but move toward a real core lock/constraints set per supported platform. `backend/pyproject.toml` contains a mix of exact and ranged core dependencies; `README.md:32–39` uses the Windows snapshot only on Windows and unbounded platform resolution on macOS/Linux. This is a repeatability improvement, not a claim that the installed versions are obsolete or vulnerable. Do not replace current dependencies solely to standardize version syntax.

## Review coverage and limits

Read `AGENTS.md`, `docs/spec.md`, `docs/contracts.md`, and `docs/progress.md`. Inspected the repository/database, intake, PDF/Office extraction, legacy Word process ownership, job lifecycle, API boundary, estimating, output snapshot/currentness, Word and programme writers, client-workbook mapping validation, submission selection, requirement review/output links, mail settings/drafts/send/replies, backup/restore, local semantic retrieval, project-map review coverage, approved knowledge, measurement-to-BOQ validation, dependency declarations and source logging setup.

This was not a line-by-line audit of every geometry or worksheet-writing helper. AI runtimes/SDK internals, provider eligibility/billing, rendered frontend behavior, native desktop lifecycle internals and existing test correctness were outside this bounded subtask. No customer documents, extracted private text, runtime databases or credentials were read. No broad tests, lint, typechecks, browser QA, release build, model call, mail operation or source mutation was performed. Only this report was written. The later acceptance examples are proposed checks for an authorized session, not results.
