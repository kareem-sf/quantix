# Final backend review

Reviewed the current integrated implementation on 2026-09-06, including untracked backend files and current job/publication, revision, estimate, correspondence, backup and startup logic. Review and regression work use synthetic temporary workspaces only. No real provider/mail calls, credential reads or reference-package mutations were performed. The assigned Astra/xhigh requirement was retained; no model substitution was requested.

## Findings

### P1 Recovery from a missing saved original causes repeated startup failure

Location: `backend/quantix/backup.py:217`, `_create_archive`, and `backend/quantix/backup.py:655`, `apply_pending_restore` staged phase. **Resolved.**

Reproduction: create a valid backup containing a saved original; delete that original only in a temporary workspace; stage the valid backup with its inspected SHA-256; call apply_pending_restore. Target archive validation succeeds, but creating the mandatory before-restore backup raises `A referenced source or output is missing, changed, or too large to back up.` The pending restore remains and the original is not recovered. Since startup applies the pending restore before opening the app, subsequent starts repeat the failure.

This is a concrete recovery failure in the situation a backup is intended to repair. The previous implementation deliberately failed closed, but it had no supported path that preserved the damaged pre-restore evidence while restoring a healthy target. Root authorized a narrow fix: capture clearly marked, non-restorable pre-restore recovery evidence when referenced originals/outputs are missing or changed; retain full validation of the selected target backup.

### P2 Explicit unapproved browser origins can execute authenticated API mutations

Location: `backend/quantix/api.py:266`, `enforce_origin`, alongside the existing authentication middleware and CORS configuration. **Resolved.**

Reproduction: an in-process API request with a valid synthetic bearer token and `Origin: https://untrusted.invalid` can POST a Tender successfully (HTTP 200, one record created). CORS omits Access-Control-Allow-Origin but does not prevent execution. This violates the documented server-side origin boundary. A valid bearer token is still required, so this is defense-in-depth rather than an unauthenticated compromise.

Root authorized rejection of explicit unapproved origins with 403 before routing. Authenticated non-browser clients without an Origin header must continue to work.

## Ruled-out concern and reviewed guards

A suspected completed-task revision gap was not present in the current integrated code. A temporary task assigned source A, then completed with additional cited/read source B, correctly becomes needs_review when B is revised. Current repository logic includes both result.source_ids and result.source_ids_read in revision dependencies.

Reviewed the actual prepared-result/job transaction boundary, task start/completion source validation, current-source rate proposal approval, Decimal VAT handling and exported inclusive-rate formula, scoped attachment/hash approval, TLS-only SMTP/IMAP, durable pre-network delivery receipts and same-home rollback protection, archive member/hash/snapshot-reference validation, and offline restore ordering. No additional concrete critical defect was established in those paths during this review. This does not claim live mail/provider or full real-package acceptance; root owns those checks.

## Authorized remediation

The fixes are implemented and verified. Writes were limited to `backup.py`, `backup_models.py`, `api.py`, their two test files, and this report.

- `backup.py:286` captures a separate `backups/<id>.recovery.zip` only when the intact current database references missing or changed files. Its manifest explicitly marks `archive_type: quantix-recovery-evidence` and `complete_backup: false`. It retains the consistent pre-restore database, all available referenced bytes with actual and expected hashes, and explicit missing-file entries. Missing original rows and provenance remain in the retained database. Credentials and derived indexes remain excluded.
- Normal backup creation still rejects missing or changed files. Selected restore archives retain full path, member, hash, database integrity and snapshot-reference validation. `backup.py:399` explicitly rejects recovery evidence as a restorable backup; recovery archives are excluded from normal backup listings.
- The recovery archive is durably published before the restore journal advances. Interrupted installation reuses that same archive and replays the approved snapshot, removing stale WAL/SHM before the database is installed last. No active HTTP restore, recursive removal, mail-ledger replacement, or source-package write was introduced.
- The narrow recovery path handles missing or modified referenced file bytes. It deliberately stops before replacing the database if the current database itself cannot be snapshotted/validated, a path is unsafe, available bytes cannot be read or retained, or storage limits are exceeded. It does not silently omit unreadable data or claim to repair a corrupt SQLite database.
- `backup.py:569` adds `BackupService.latest_restore() -> dict | None`. It selects the latest valid completed history with a matching restore ID/filename. The result contains `restore_id`, `completed_at`, `before_restore_backup_id`, `recovery_evidence_path`, and `detail`. `apply_pending_restore()` returns the same fields plus `status: restored`. New journals persist completion time; older completed journals use the history file modification time. Malformed/uncompleted history records are ignored. The optional journal fields are defined in `backup_models.py:85`.
- For a healthy previous workspace, `before_restore_backup_id` identifies its normal checked backup and `recovery_evidence_path` is null. For a damaged previous workspace, that ID identifies the retained recovery capture and the relative recovery path is populated. Consumers must use this path and must not describe the recovery capture as a complete backup. Root owns the typed restored-notice endpoint and desktop presentation.
- `api.py:266` rejects any explicitly unapproved Origin, including empty/null origins and preflight requests, with HTTP 403 before routing. Approved desktop origins and authenticated no-Origin CLI requests retain access. Bearer authentication remains required.
- As requested, `api.py:21`, `api.py:95`, and `api.py:283` register the existing knowledge router and advertise the knowledge capability. No knowledge service implementation was changed.

## Verification

The pre-fix focused regressions reproduced eight failures: three unapproved-origin mutations, one preflight boundary, three damaged-workspace restore cases, and the missing durable-history reader. After implementation:

`backend/.venv/Scripts/python.exe -m pytest tests/test_api.py tests/test_backup.py tests/test_correspondence.py -q` (working directory `backend`): **61 passed, 1 skipped**. The skipped synthetic symlink test requires Windows symlink permission; there is one upstream Starlette/AnyIO deprecation warning.

Coverage includes missing originals, changed originals, missing generated outputs, original byte/hash integrity, preserved pre-restore database rows, exclusion of synthetic credentials, explicit non-restorable archive rejection, resumed degraded restoration without mixed WAL, unchanged recovery evidence across interruption, legacy/malformed history, approved/no-Origin access, unapproved preflights, knowledge route authentication, and existing correspondence same-home restore/send-ledger protection.

Ruff check and formatting passed for all five owned Python files. No live provider, SMTP or IMAP operations were executed. Parent retains ownership of the full integration gate and real-package acceptance.
