# Workspace backup and restore

Implemented 2026-09-06. All execution and restore verification used synthetic temporary workspaces. No real workspace or user reference package was restored.

## Owned files

- `backend/quantix/backup.py`: SQLite snapshot backup, archive inspection, restore staging and offline publication.
- `backend/quantix/backup_models.py`: strict Pydantic API contracts, versioned manifest and restore journal.
- `backend/quantix/backup_routes.py`: router factory.
- `backend/tests/test_backup.py`: behavioral and filesystem/database integrity tests.
- This report. No configuration, API bootstrap, UI or other modules were changed.

## Root integration contract

`BackupService(repo)` exposes:

```python
create() -> dict  # BackupRecord
list() -> list[dict]  # BackupRecord list
inspect(path: str | Path) -> dict  # BackupInspection
stage_restore(path: str | Path, engineer_confirmed: bool, rationale: str) -> dict  # RestoreReady
path(backup_id: str) -> Path  # validates the archive before returning a download path
```

`create_router(repo)` returns an APIRouter with `/api` prefix, intended for the existing application bearer/origin protections and exception handlers:

- GET `/backups` -> list[BackupRecord]
- POST `/backups` with no body -> BackupRecord
- POST `/backups/inspect` with `{path}` -> BackupInspection
- POST `/backups/restore` with `{path,engineer_confirmed:true,rationale}` -> RestoreReady
- GET `/backups/{backup_id}/download` -> ZIP attachment

BackupRecord fields: id,filename,created_at,size,sha256,format_version,tender_count,original_count,unavailable_original_count,output_count,file_count,purpose (`manual|before_restore`). List uses stored creation metadata; explicit inspect/download performs full integrity validation. BackupInspection fields: valid:true,compatible:true,backup,total_uncompressed_bytes,detail. Invalid/incompatible archives raise ValueError rather than returning a misleading valid record.

RestoreReady fields: status:`restore_ready`,restore_id,backup_id,restart_required:true,detail. This HTTP operation changes only staging files and the pending journal; it never replaces the running database. Explicit true consent and a nonblank rationale are required. A second pending restore is rejected.

The launcher must call the following **inside its existing FileLock, before create_app/Repository/SemanticIndex or any application database connection**:

```python
from quantix.backup import apply_pending_restore

with FileLock(home / "workspace.lock", timeout=0):
    restored = apply_pending_restore(home)
    # Only after successful return create the repository/app/indexes.
```

Return is None when nothing is pending, otherwise `{status:'restored',restore_id,before_restore_backup_id}`. Any exception must abort startup; do not catch it and open the workspace anyway. The function itself may open the old database solely to make its recovery snapshot and checkpoint it, before application services start.

## Backup contents and consistency

The service uses Python sqlite3.Connection.backup on a separate connection, so committed WAL state is included. It switches the private snapshot to DELETE journal mode, keeps only supported public settings (`model`, `default_currency`, `preferences`), securely removes any other settings, and vacuums the snapshot. The live database and credential store are not modified.

All archive references come from the completed snapshot, never a later live query. Contents are the database, every content-addressed saved original referenced by any source revision, and registered generated XLSX/DOCX outputs plus their JSON manifests. Content hashes, sizes, database integrity, foreign keys and output manifest identity are checked. A registered file that was never readable has its exception record preserved and contributes to unavailable_original_count; a supposedly saved original that is missing/corrupt prevents successful backup.

ZIP archives and immutable, versioned metadata use new UUID filenames under `home/backups`. No recursive workspace crawl occurs. Credentials, environment files, keyring data, semantic.sqlite, embeddings, model weights, extraction caches, prior backups and staging files are excluded. This excludes credential stores and unsupported private settings; it does not classify or redact arbitrary user-authored source/document text.

Manifest format and database schema version are currently 1. The manifest lists each file's exact relative path, byte size and SHA-256. Archives allow at most 50,000 payload files, 50 GiB total uncompressed data and 8 GiB per file. Newer/unknown versions fail closed. Checksums establish integrity, not cryptographic proof of who created an externally supplied archive.

## Offline restore sequence

1. HTTP staging copies the chosen ZIP to a new home/restore-staging/UUID directory, validates every entry and hash, verifies the database and its exact referenced file set, and extracts only allowed regular files into that staging directory.
2. A flushed, atomically replaced pending-restore.json stores consent, rationale, archive hash and phase.
3. Offline startup rechecks the staged archive. In `staged`, it creates a versioned `before_restore` backup of the then-current workspace and records that ID before continuing.
4. In `prepared`, SQLite checkpoints/truncates the old WAL, changes to DELETE journal mode and closes the connection. Busy/locked connections stop the restore. Only checked, exact SQLite WAL/SHM/journal paths are removed.
5. The journal advances to `installing`. Payloads are installed through flushed temporary files and same-volume replacement. Existing unrelated originals/output files are retained; no recursive directory deletion or replacement occurs. The semantic database and its sidecars move aside into the restore staging directory so the index rebuilds against restored sources. Model files remain untouched.
6. The main database is replaced last. If interrupted before or after that swap, the next `installing` attempt **does not open the target database**: it first removes stale sidecars and idempotently reapplies the same staged snapshot. This prevents an unrelated WAL from being replayed against the restored database.
7. Completion moves the journal into home/restore-history. Only then may the application open its databases.

Every archive member and filesystem target is validated against the intended workspace. Absolute paths, traversal, drive/UNC aliases, backslashes, Windows trailing-dot/space aliases, duplicate/case-colliding paths, symlinks, junctions, directories, special files, encrypted entries, mismatched payload sets and hash mismatches are rejected. Filesystem destinations are checked independently of member names. Windows uses the installed pywin32 MoveFileEx wrapper with REPLACE_EXISTING and WRITE_THROUGH; file content is flushed before publication. Directory fsync is used on non-Windows hosts.

## Verification

```powershell
backend/.venv/Scripts/python.exe -m pytest backend/tests/test_backup.py -q
backend/.venv/Scripts/ruff.exe check backend/quantix/backup.py backend/quantix/backup_models.py backend/quantix/backup_routes.py backend/tests/test_backup.py
```

Result: **22 passed, 1 skipped; lint passed.** The skipped test attempted a real filesystem symlink and Windows denied creation because the current account lacks that privilege. ZIP symlink, traversal, duplicate path, hash corruption and unsupported-version tests passed. One existing upstream Starlette/AnyIO TestClient deprecation warning remains.

Tests verify snapshot-driven membership despite later live writes, committed WAL data, exclusion of private settings and credential/cache files, unchanged original bytes, all original revisions, registered output recovery into a different temporary home, explicit restore consent, live-state preservation during staging, before-restore backup preservation, locked-DB recovery, idempotent retries on either side of the database swap, stale WAL/SHM removal, changed-stage rejection, missing-original failures and API schemas.

Official API sources: [Python SQLite backup](https://docs.python.org/3.12/library/sqlite3.html#sqlite3.Connection.backup), [ZIP handling](https://docs.python.org/3.12/library/zipfile.html), [Windows MoveFileExW](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-movefileexw).

## Operational limits

The launcher owns exclusivity. Restore must never run against an active application database. If before-restore backup or checkpoint preservation fails, restore stops instead of overwriting current state. Staging and recovery copies remain locally for inspection/recovery and are not automatically pruned. A crash during archive publication may leave an unlisted complete ZIP or an unused staging directory; neither is treated as an applied restore. Sufficient free disk is required for the selected archive, extracted staging payload and before-restore backup. Hardware power-loss behavior ultimately depends on filesystem/storage guarantees; interruption recovery was tested with injected process-level failures, not power-cut hardware tests.
