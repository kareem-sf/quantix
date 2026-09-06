"""Snapshot-based backups and offline, restartable restore publication.

Only apply_pending_restore changes the workspace database. The launcher must hold
workspace.lock and call it before creating Repository or opening semantic indexes.
"""

import hashlib
import json
import os
import re
import shutil
import sqlite3
import stat
import threading
from contextlib import closing, contextmanager
from datetime import datetime
from pathlib import Path, PurePosixPath
from zipfile import ZIP_DEFLATED, BadZipFile, ZipFile

from .backup_models import BackupManifest, BackupRecord, RestoreBackupRequest, RestoreJournal
from .db import dump, new_id, now

_LOCK = threading.RLock()
MAX_FILES = 50000
MAX_BYTES = 50 * 1024**3
MAX_MEMBER_BYTES = 8 * 1024**3
MAX_MANIFEST_BYTES = 16 * 1024**2
CHUNK_BYTES = 1024**2
PUBLIC_SETTINGS = ("model", "default_currency", "preferences")
OUTPUT_NAME = (
    r"(?:(?:consolidated-boq|tender-analysis|technical-work|tender-registers|"
    r"supplier-comparison|construction-programme|client-boq)-[a-f0-9]{32}\.(?:xlsx|xlsm|docx|json)"
    r"|submission-[a-f0-9]{32}\.(?:zip|json))"
)


class _ReferenceBackupError(ValueError):
    """An intact database references unavailable or modified file bytes."""


def _relative(name: str) -> PurePosixPath:
    path = PurePosixPath(name)
    if (
        not name
        or "\\" in name
        or ":" in name
        or path.is_absolute()
        or path.as_posix() != name
        or any(part in {"", ".", ".."} for part in path.parts)
    ):
        raise ValueError("A backup path is unsafe.")
    if any(part.rstrip(". ") != part for part in path.parts):
        raise ValueError("A backup path has an unsafe Windows name.")
    return path


def _inside(home: Path, relative: str) -> Path:
    root = home.resolve()
    parts = _relative(relative).parts
    target = root.joinpath(*parts)
    if not target.resolve().is_relative_to(root):
        raise ValueError("A backup path leaves the workspace.")
    candidate = root
    for part in parts:
        candidate = candidate / part
        if candidate.is_symlink() or candidate.is_junction():
            raise ValueError("Backup and restore paths cannot use links or junctions.")
    return target


def _payload_name(name):
    _relative(name)
    if (
        name == "quantix.sqlite"
        or re.fullmatch(r"objects/[a-f0-9]{64}", name)
        or re.fullmatch(r"outputs/" + OUTPUT_NAME, name)
    ):
        return
    raise ValueError("The archive contains a file outside the backup format.")


def _sha(path):
    with path.open("rb") as handle:
        return hashlib.file_digest(handle, "sha256").hexdigest()


def _replace_file(source, target):
    if os.name == "nt":
        import win32api

        # Documented Win32 flags: REPLACE_EXISTING | WRITE_THROUGH. All callers
        # publish regular files within the same checked workspace volume.
        win32api.MoveFileEx(str(source), str(target), 0x1 | 0x8)
    else:
        os.replace(source, target)


def _sync_directory(path):
    if os.name != "nt":
        descriptor = os.open(path, os.O_RDONLY)
        try:
            os.fsync(descriptor)
        finally:
            os.close(descriptor)


def _atomic_json(path: Path, value):
    temporary = _inside(path.parent, path.name + ".partial")
    with temporary.open("w", encoding="utf-8", newline="\n") as handle:
        handle.write(dump(value))
        handle.flush()
        os.fsync(handle.fileno())
    _replace_file(temporary, path)
    _sync_directory(path.parent)


@contextmanager
def _scratch(home):
    work = _inside(home, "backup-work/" + new_id())
    work.mkdir(parents=True)
    try:
        yield work
    finally:
        for name in (
            "snapshot.sqlite",
            "snapshot.sqlite-journal",
            "snapshot.sqlite-wal",
            "snapshot.sqlite-shm",
        ):
            _inside(work, name).unlink(missing_ok=True)
        work.rmdir()


def _snapshot_database(home, target):
    source = _inside(home, "quantix.sqlite")
    for name in ("quantix.sqlite-wal", "quantix.sqlite-shm", "quantix.sqlite-journal"):
        _inside(home, name)
    if not source.is_file():
        raise ValueError("There is no workspace database to back up.")
    with closing(sqlite3.connect(source.as_uri() + "?mode=ro", uri=True, timeout=30)) as current:
        with closing(sqlite3.connect(target)) as snapshot:
            current.backup(snapshot, pages=1024)
            snapshot.execute("PRAGMA journal_mode=DELETE")
            snapshot.execute("PRAGMA secure_delete=ON")
            snapshot.execute("DELETE FROM settings WHERE key NOT IN (?,?,?)", PUBLIC_SETTINGS)
            snapshot.commit()
            snapshot.execute("VACUUM")


def _database_references(database):
    try:
        with closing(sqlite3.connect(database.as_uri() + "?mode=ro&immutable=1", uri=True)) as conn:
            conn.execute("PRAGMA trusted_schema=OFF")
            if conn.execute("PRAGMA user_version").fetchone()[0] != 1:
                raise ValueError("The backup uses an unsupported database format.")
            if (
                conn.execute("PRAGMA integrity_check").fetchone()[0] != "ok"
                or conn.execute("PRAGMA foreign_key_check").fetchone()
            ):
                raise ValueError("The backup database failed its integrity check.")
            if conn.execute(
                "SELECT 1 FROM settings WHERE key NOT IN (?,?,?)", PUBLIC_SETTINGS
            ).fetchone():
                raise ValueError("The backup database contains unsupported private settings.")
            references = {}
            unavailable = 0
            for digest, size in conn.execute("SELECT content_hash,size FROM artifacts"):
                if not digest:
                    unavailable += 1
                    continue
                if not re.fullmatch(r"[a-f0-9]{64}", digest):
                    raise ValueError("The source register contains an invalid content hash.")
                references["objects/" + digest] = (digest, int(size))
            originals = len(references)
            outputs = 0
            for table in ("generated_outputs", "submissions"):
                if not conn.execute(
                    "SELECT 1 FROM sqlite_master WHERE type='table' AND name=?", (table,)
                ).fetchone():
                    continue
                for row in conn.execute(f"SELECT record_json FROM {table}"):
                    record = json.loads(row[0])
                    filename = record["filename"]
                    _payload_name("outputs/" + filename)
                    references["outputs/" + filename] = (record["sha256"], record["size"])
                    manifest_name = PurePosixPath(filename).with_suffix(".json").as_posix()
                    manifest_bytes = dump(record).encode("utf-8")
                    references["outputs/" + manifest_name] = (
                        hashlib.sha256(manifest_bytes).hexdigest(),
                        len(manifest_bytes),
                    )
                    outputs += 1
            counts = {
                "tender_count": conn.execute("SELECT COUNT(*) FROM tenders").fetchone()[0],
                "original_count": originals,
                "unavailable_original_count": unavailable,
                "output_count": outputs,
            }
            return references, counts
    except (sqlite3.DatabaseError, KeyError, TypeError, json.JSONDecodeError) as exc:
        raise ValueError(
            "The archive does not contain a supported, intact Quantix database."
        ) from exc


def _record(manifest, path):
    return BackupRecord(
        id=manifest.id,
        filename=path.name,
        created_at=manifest.created_at,
        size=path.stat().st_size,
        sha256=_sha(path),
        format_version=1,
        tender_count=manifest.tender_count,
        original_count=manifest.original_count,
        unavailable_original_count=manifest.unavailable_original_count,
        output_count=manifest.output_count,
        file_count=len(manifest.files),
        purpose=manifest.purpose,
    ).model_dump()


def _create_archive(home, purpose="manual"):
    directory = _inside(home, "backups")
    directory.mkdir(exist_ok=True)
    identifier = new_id()
    target = _inside(home, f"backups/{identifier}.zip")
    partial = _inside(home, f"backups/{identifier}.partial")
    try:
        with _scratch(home) as work:
            database = work / "snapshot.sqlite"
            _snapshot_database(home, database)
            references, counts = _database_references(database)
            references["quantix.sqlite"] = (_sha(database), database.stat().st_size)
            if (
                len(references) > MAX_FILES
                or sum(value[1] for value in references.values()) > MAX_BYTES
            ):
                raise ValueError("The workspace exceeds the backup size or file-count limit.")
            files = []
            with ZipFile(
                partial, "x", compression=ZIP_DEFLATED, compresslevel=6, allowZip64=True
            ) as archive:
                for relative, (expected_hash, expected_size) in sorted(references.items()):
                    source = database if relative == "quantix.sqlite" else _inside(home, relative)
                    if (
                        not source.is_file()
                        or source.stat().st_size != expected_size
                        or expected_size > MAX_MEMBER_BYTES
                    ):
                        raise _ReferenceBackupError(
                            "A referenced source or output is missing, changed, or too large to back up."
                        )
                    digest, size = hashlib.sha256(), 0
                    with (
                        source.open("rb") as reader,
                        archive.open(relative, "w", force_zip64=True) as writer,
                    ):
                        while chunk := reader.read(CHUNK_BYTES):
                            size += len(chunk)
                            if size > expected_size:
                                raise _ReferenceBackupError(
                                    "A referenced file changed during backup."
                                )
                            digest.update(chunk)
                            writer.write(chunk)
                    if size != expected_size or digest.hexdigest() != expected_hash:
                        raise _ReferenceBackupError(
                            "A referenced file failed its backup integrity check."
                        )
                    files.append({"path": relative, "size": size, "sha256": expected_hash})
                manifest = BackupManifest(
                    format_version=1,
                    id=identifier,
                    created_at=now(),
                    database_schema=1,
                    purpose=purpose,
                    files=files,
                    **counts,
                )
                archive.writestr("manifest.json", manifest.model_dump_json())
            with partial.open("r+b") as completed:
                os.fsync(completed.fileno())
            _replace_file(partial, target)
            result = _record(manifest, target)
            _atomic_json(target.with_suffix(".json"), result)
            return result
    finally:
        partial.unlink(missing_ok=True)


def _capture_recovery_evidence(home):
    """Retain damaged pre-restore state without labelling it a usable backup."""
    identifier = new_id()
    relative_path = f"backups/{identifier}.recovery.zip"
    target = _inside(home, relative_path)
    partial = _inside(home, f"backups/{identifier}.recovery.partial")
    try:
        with _scratch(home) as work:
            database = work / "snapshot.sqlite"
            _snapshot_database(home, database)
            references, counts = _database_references(database)
            references["quantix.sqlite"] = (_sha(database), database.stat().st_size)
            if len(references) > MAX_FILES:
                raise ValueError("Recovery evidence exceeds the file-count limit.")
            files, missing, total = [], [], 0
            with ZipFile(
                partial, "x", compression=ZIP_DEFLATED, compresslevel=6, allowZip64=True
            ) as archive:
                for relative, (expected_hash, expected_size) in sorted(references.items()):
                    source = database if relative == "quantix.sqlite" else _inside(home, relative)
                    expected = {
                        "path": relative,
                        "expected_sha256": expected_hash,
                        "expected_size": expected_size,
                    }
                    try:
                        reader = source.open("rb")
                    except FileNotFoundError:
                        missing.append(expected | {"state": "missing"})
                        continue
                    # Do not conceal permission, unsafe-path, read or disk failures.
                    # The old database remains in place if its available bytes cannot be retained.
                    digest, size = hashlib.sha256(), 0
                    with reader, archive.open(relative, "w", force_zip64=True) as writer:
                        while chunk := reader.read(CHUNK_BYTES):
                            size += len(chunk)
                            total += len(chunk)
                            if size > MAX_MEMBER_BYTES or total > MAX_BYTES:
                                raise ValueError("Recovery evidence exceeds its size limit.")
                            digest.update(chunk)
                            writer.write(chunk)
                    actual_hash = digest.hexdigest()
                    state = (
                        "verified"
                        if actual_hash == expected_hash and size == expected_size
                        else "changed"
                    )
                    files.append(expected | {"size": size, "sha256": actual_hash, "state": state})
                evidence = {
                    "archive_type": "quantix-recovery-evidence",
                    "format_version": 1,
                    "complete_backup": False,
                    "id": identifier,
                    "created_at": now(),
                    "purpose": "before_restore",
                    "detail": "Pre-restore database and available files retained. Missing or changed files are listed; this archive is not a restorable backup.",
                    "files": files,
                    "missing_files": missing,
                    **counts,
                }
                manifest = dump(evidence).encode("utf-8")
                if len(manifest) > MAX_MANIFEST_BYTES or total + len(manifest) > MAX_BYTES:
                    raise ValueError("Recovery evidence exceeds its manifest or total size limit.")
                archive.writestr("manifest.json", manifest)
            with partial.open("r+b") as completed:
                os.fsync(completed.fileno())
            _replace_file(partial, target)
            _sync_directory(target.parent)
            return {"id": identifier, "path": relative_path}
    finally:
        partial.unlink(missing_ok=True)


def _validate_archive(home, path, destination=None):
    if destination is not None:
        try:
            relative = Path(destination).absolute().relative_to(home.resolve()).as_posix()
        except ValueError as exc:
            raise ValueError("Restore staging must remain inside the target workspace.") from exc
        destination = _inside(home, relative)
    source = Path(path).expanduser().resolve(strict=True)
    if not source.is_file() or source.stat().st_size > MAX_BYTES:
        raise ValueError("Choose a backup ZIP within the supported size limit.")
    try:
        with _scratch(home) as work, ZipFile(source) as archive:
            entries = archive.infolist()
            if len(entries) > MAX_FILES + 1:
                raise ValueError("The backup contains too many files.")
            names, total = set(), 0
            for entry in entries:
                _relative(entry.filename)
                if entry.filename.casefold() in names:
                    raise ValueError("The backup contains duplicate file paths.")
                names.add(entry.filename.casefold())
                mode = stat.S_IFMT(entry.external_attr >> 16)
                if entry.is_dir() or mode not in (0, stat.S_IFREG) or entry.flag_bits & 1:
                    raise ValueError(
                        "Backup archives cannot contain links, directories, special files or encryption."
                    )
                if entry.filename != "manifest.json":
                    _payload_name(entry.filename)
                if entry.file_size > MAX_MEMBER_BYTES:
                    raise ValueError("A backup member exceeds the size limit.")
                total += entry.file_size
            if total > MAX_BYTES or "manifest.json" not in names:
                raise ValueError("The backup is too large or has no manifest.")
            if archive.getinfo("manifest.json").file_size > MAX_MANIFEST_BYTES:
                raise ValueError("The backup manifest exceeds its size limit.")
            manifest_data = json.loads(archive.read("manifest.json"))
            if (
                isinstance(manifest_data, dict)
                and manifest_data.get("archive_type") == "quantix-recovery-evidence"
            ):
                raise ValueError("This file is recovery evidence, not a restorable backup.")
            manifest = BackupManifest.model_validate(manifest_data)
            expected = {}
            for member in manifest.files:
                _payload_name(member.path)
                if member.path in expected:
                    raise ValueError("The manifest contains duplicate file paths.")
                expected[member.path] = member
            if (
                set(expected) | {"manifest.json"} != {entry.filename for entry in entries}
                or "quantix.sqlite" not in expected
            ):
                raise ValueError("Backup contents do not match the manifest.")
            database = work / "snapshot.sqlite"
            for name, member in expected.items():
                if archive.getinfo(name).file_size != member.size:
                    raise ValueError("A backup file size does not match its manifest.")
                target = (
                    _inside(destination, name)
                    if destination is not None
                    else database
                    if name == "quantix.sqlite"
                    else None
                )
                if target:
                    target.parent.mkdir(parents=True, exist_ok=True)
                digest, size = hashlib.sha256(), 0
                writer = target.open("wb") if target else None
                try:
                    with archive.open(name) as reader:
                        while chunk := reader.read(CHUNK_BYTES):
                            size += len(chunk)
                            if size > member.size:
                                raise ValueError("A backup member exceeds its declared size.")
                            digest.update(chunk)
                            if writer:
                                writer.write(chunk)
                    if writer:
                        writer.flush()
                        os.fsync(writer.fileno())
                finally:
                    if writer:
                        writer.close()
                if size != member.size or digest.hexdigest() != member.sha256:
                    raise ValueError("A backup file failed its SHA-256 integrity check.")
            if destination is not None:
                database = _inside(destination, "quantix.sqlite")
            references, counts = _database_references(database)
            if set(references) | {"quantix.sqlite"} != set(expected):
                raise ValueError(
                    "The archive does not contain exactly the snapshot's referenced files."
                )
            for name, (digest, size) in references.items():
                if expected[name].sha256 != digest or expected[name].size != size:
                    raise ValueError("A source or output differs from the database snapshot.")
            if any(getattr(manifest, key) != value for key, value in counts.items()):
                raise ValueError("Backup manifest counts differ from the database snapshot.")
            return (
                manifest,
                _record(manifest, source),
                sum(member.size for member in manifest.files),
            )
    except (BadZipFile, KeyError, sqlite3.DatabaseError, json.JSONDecodeError) as exc:
        raise ValueError("The backup ZIP is damaged or incomplete.") from exc


class BackupService:
    def __init__(self, repo):
        self.repo = repo
        self.home = repo.home.resolve()
        _inside(self.home, "backups").mkdir(exist_ok=True)

    def create(self):
        with _LOCK:
            return _create_archive(self.home)

    def list(self):
        with _LOCK:
            results = []
            for path in sorted(_inside(self.home, "backups").glob("*.json"), reverse=True):
                if not re.fullmatch(r"[a-f0-9]{32}\.json", path.name):
                    continue
                path = _inside(self.home, "backups/" + path.name)
                result = BackupRecord.model_validate_json(
                    path.read_text(encoding="utf-8")
                ).model_dump()
                if (
                    result["id"] + ".json" != path.name
                    or result["filename"] != result["id"] + ".zip"
                ):
                    raise ValueError("A backup index contains an invalid filename.")
                if _inside(self.home, "backups/" + result["filename"]).is_file():
                    results.append(result)
            return sorted(results, key=lambda record: record["created_at"], reverse=True)

    def inspect(self, path):
        with _LOCK:
            manifest, backup, size = _validate_archive(self.home, path)
            return {
                "valid": True,
                "compatible": True,
                "backup": backup,
                "total_uncompressed_bytes": size,
                "detail": "Database and referenced files passed integrity checks. Credentials and rebuildable indexes are not included.",
            }

    def stage_restore(self, path, engineer_confirmed, rationale, *, expected_sha256):
        request = RestoreBackupRequest(
            path=str(path),
            engineer_confirmed=engineer_confirmed,
            rationale=rationale,
            expected_sha256=expected_sha256,
        )
        with _LOCK:
            pending = _inside(self.home, "pending-restore.json")
            if pending.exists():
                raise ValueError("A restore is already staged. Restart Quantix to finish it.")
            identifier = new_id()
            stage = _inside(self.home, "restore-staging/" + identifier)
            stage.mkdir(parents=True)
            source = Path(request.path).expanduser().resolve(strict=True)
            if not source.is_file() or _sha(source) != request.expected_sha256:
                raise ValueError(
                    "The backup changed after it was inspected. Check it again before restoring."
                )
            if source.stat().st_size > MAX_BYTES:
                raise ValueError("The backup exceeds the archive size limit.")
            archive = stage / "source.zip"
            with source.open("rb") as reader, archive.open("xb") as writer:
                shutil.copyfileobj(reader, writer, CHUNK_BYTES)
                writer.flush()
                os.fsync(writer.fileno())
            manifest, backup, size = _validate_archive(self.home, archive, stage / "payload")
            if backup["sha256"] != request.expected_sha256:
                raise ValueError(
                    "The backup changed while it was being prepared. Check it again before restoring."
                )
            journal = RestoreJournal(
                format_version=1,
                restore_id=identifier,
                backup_id=manifest.id,
                archive_sha256=backup["sha256"],
                accepted_at=now(),
                engineer_confirmed=True,
                rationale=request.rationale,
                phase="staged",
            )
            _atomic_json(pending, journal.model_dump())
            return {
                "status": "restore_ready",
                "restore_id": identifier,
                "backup_id": manifest.id,
                "restart_required": True,
                "detail": "Restore is ready. Restart Quantix to apply it. The current database and available files will be retained first; any damaged files will be recorded in a recovery capture.",
            }

    def pending_restore(self):
        with _LOCK:
            pending = _inside(self.home, "pending-restore.json")
            if not pending.exists():
                return None
            journal = RestoreJournal.model_validate_json(pending.read_text(encoding="utf-8"))
            return {
                "status": "restore_ready",
                "restore_id": journal.restore_id,
                "backup_id": journal.backup_id,
                "restart_required": True,
                "detail": "A checked backup is prepared for restoration. Close and reopen Quantix to apply it. The current database and available files will be retained first; any damaged files will be recorded in a recovery capture.",
            }

    def latest_restore(self):
        with _LOCK:
            completed = []
            for path in _inside(self.home, "restore-history").glob("*.json"):
                if not re.fullmatch(r"[a-f0-9]{32}\.json", path.name):
                    continue
                path = _inside(self.home, "restore-history/" + path.name)
                try:
                    journal = RestoreJournal.model_validate_json(path.read_text(encoding="utf-8"))
                    if journal.phase != "completed" or path.stem != journal.restore_id:
                        continue
                    result = _restore_outcome(journal)
                    completed.append(
                        (datetime.fromisoformat(result["completed_at"]).timestamp(), result)
                    )
                except (ValueError, OSError):
                    continue
            return max(completed, key=lambda item: item[0])[1] if completed else None

    def path(self, backup_id):
        if not re.fullmatch(r"[a-f0-9]{32}", backup_id):
            raise KeyError("This backup could not be found.")
        path = _inside(self.home, f"backups/{backup_id}.zip")
        if not path.is_file():
            raise KeyError("This backup could not be found.")
        self.inspect(path)
        return path


def _checkpoint_existing(home):
    path = _inside(home, "quantix.sqlite")
    for name in ("quantix.sqlite-wal", "quantix.sqlite-shm", "quantix.sqlite-journal"):
        _inside(home, name)
    if path.exists():
        with closing(sqlite3.connect(path, timeout=1)) as conn:
            result = conn.execute("PRAGMA wal_checkpoint(TRUNCATE)").fetchone()
            if result[0] != 0:
                raise ValueError("Close all workspace database connections before restoring.")
            if conn.execute("PRAGMA journal_mode=DELETE").fetchone()[0].lower() != "delete":
                raise ValueError("The workspace database could not be prepared for restore.")
    _remove_sidecars(home)


def _remove_sidecars(home):
    for name in ("quantix.sqlite-wal", "quantix.sqlite-shm", "quantix.sqlite-journal"):
        _inside(home, name).unlink(missing_ok=True)
    _sync_directory(home)


def _install(home, source, relative, restore_id, digest):
    destination = _inside(home, relative)
    destination.parent.mkdir(parents=True, exist_ok=True)
    if destination.exists() and _sha(destination) == digest:
        return
    temporary = _inside(destination.parent, destination.name + ".restore-" + restore_id)
    with source.open("rb") as reader, temporary.open("wb") as writer:
        shutil.copyfileobj(reader, writer, CHUNK_BYTES)
        writer.flush()
        os.fsync(writer.fileno())
    if _sha(temporary) != digest:
        raise ValueError("Staged restore data changed before publication.")
    _replace_file(temporary, destination)
    _sync_directory(destination.parent)


def _restore_outcome(journal):
    if not journal.completed_at:
        raise ValueError("The restore record has no recorded completion time.")
    return {
        "restore_id": journal.restore_id,
        "completed_at": journal.completed_at,
        "before_restore_backup_id": journal.before_restore_backup_id,
        "recovery_evidence_path": journal.recovery_evidence_path,
        "detail": (
            "Workspace restored. The damaged previous workspace was retained as recovery evidence with missing or changed files listed; it is not a complete backup."
            if journal.recovery_evidence_path
            else "Workspace restored. The previous workspace was retained as a checked backup."
            if journal.before_restore_backup_id
            else "Workspace restored. There was no previous workspace database to back up."
        ),
    }


def apply_pending_restore(home: Path):
    """Apply offline under the launcher's FileLock; exceptions must abort startup."""
    home = Path(home).resolve()
    pending = _inside(home, "pending-restore.json")
    if not pending.exists():
        return None
    with _LOCK:
        journal = RestoreJournal.model_validate_json(pending.read_text(encoding="utf-8"))
        stage = _inside(home, "restore-staging/" + journal.restore_id)
        archive = _inside(stage, "source.zip")
        if _sha(archive) != journal.archive_sha256:
            raise ValueError("The staged backup changed. Restore cannot continue.")
        manifest, backup, size = _validate_archive(home, archive, stage / "payload")
        if manifest.id != journal.backup_id:
            raise ValueError("The staged backup does not match the approved restore.")
        if journal.phase == "staged":
            if _inside(home, "quantix.sqlite").exists():
                try:
                    previous = _create_archive(home, "before_restore")
                except _ReferenceBackupError:
                    previous = _capture_recovery_evidence(home)
                    journal.recovery_evidence_path = previous["path"]
                journal.before_restore_backup_id = previous["id"]
            journal.phase = "prepared"
            _atomic_json(pending, journal.model_dump())
        if journal.phase == "prepared":
            _checkpoint_existing(home)
            journal.phase = "installing"
            _atomic_json(pending, journal.model_dump())
        if journal.phase == "installing":
            # Never open a possibly half-published DB here. Replay the same approved
            # snapshot after removing stale sidecars, with the application still closed.
            _remove_sidecars(home)
            for member in manifest.files:
                if member.path != "quantix.sqlite":
                    _install(
                        home,
                        _inside(stage / "payload", member.path),
                        member.path,
                        journal.restore_id,
                        member.sha256,
                    )
            for name in ("semantic.sqlite", "semantic.sqlite-wal", "semantic.sqlite-shm"):
                old = _inside(home, name)
                if old.exists():
                    retained = _inside(stage, "previous-" + name)
                    if retained.exists():
                        raise ValueError("An unexpected semantic index appeared during restore.")
                    _replace_file(old, retained)
            database = next(member for member in manifest.files if member.path == "quantix.sqlite")
            _install(
                home,
                _inside(stage / "payload", database.path),
                database.path,
                journal.restore_id,
                database.sha256,
            )
            journal.phase = "completed"
            journal.completed_at = now()
            _atomic_json(pending, journal.model_dump())
        history = _inside(home, "restore-history/" + journal.restore_id + ".json")
        history.parent.mkdir(exist_ok=True)
        _replace_file(pending, history)
        _sync_directory(home)
        return {"status": "restored", **_restore_outcome(journal)}
