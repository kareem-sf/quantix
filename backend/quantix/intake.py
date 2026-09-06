"""Register a complete source folder or archive without changing the supplied package."""

import hashlib
import json
import os
import stat
import tempfile
import threading
import zipfile
from dataclasses import asdict, dataclass
from pathlib import Path, PurePosixPath, PureWindowsPath

from .db import dump
from .documents import extract_document
from .repository import Repository

MAX_FILES = 10000
MAX_PACKAGE_BYTES = 2 * 1024**3
MAX_FILE_BYTES = 256 * 1024**2
EXTRACTION_VERSION = "1"


@dataclass
class Candidate:
    name: str
    path: Path | None
    size: int
    problem: str | None = None


def _check_cancel(event: threading.Event):
    if event.is_set():
        raise InterruptedError(
            "Import stopped. Registered documents are saved; resume to continue."
        )


def _folder_files(root: Path, cancelled: threading.Event):
    files = []
    total = 0
    for directory, dirs, names in os.walk(root, followlinks=False):
        _check_cancel(cancelled)
        base = Path(directory)
        kept = []
        for name in sorted(dirs):
            child = base / name
            if child.is_symlink() or child.is_junction():
                files.append(
                    Candidate(
                        child.relative_to(root).as_posix(),
                        None,
                        0,
                        "Linked folders are not followed. Import the original source folder directly.",
                    )
                )
            else:
                kept.append(name)
        dirs[:] = kept
        for name in sorted(names):
            child = base / name
            relative = child.relative_to(root).as_posix()
            if child.is_symlink():
                files.append(Candidate(relative, None, 0, "Linked files are not followed."))
                continue
            try:
                size = child.stat().st_size
                total += size
                files.append(
                    Candidate(
                        relative,
                        child,
                        size,
                        "This file exceeds the 256 MB import limit."
                        if size > MAX_FILE_BYTES
                        else None,
                    )
                )
            except OSError:
                files.append(
                    Candidate(
                        relative,
                        None,
                        0,
                        "This file could not be read. Check its permissions and availability.",
                    )
                )
            if len(files) > MAX_FILES or total > MAX_PACKAGE_BYTES:
                raise ValueError(
                    "The package exceeds 10,000 files or 2 GB. Import smaller packages into this Tender."
                )
    return files


def _archive_files(source: Path, temporary: Path, cancelled: threading.Event):
    files = []
    try:
        with zipfile.ZipFile(source) as archive:
            members = [m for m in archive.infolist() if not m.is_dir()]
            if len(members) > MAX_FILES or sum(m.file_size for m in members) > MAX_PACKAGE_BYTES:
                raise ValueError("The archive exceeds 10,000 files or 2 GB when expanded.")
            seen = set()
            for member in members:
                name = member.filename.replace("\\", "/")
                parts = PurePosixPath(name)
                if (
                    parts.is_absolute()
                    or PureWindowsPath(name).drive
                    or ".." in parts.parts
                    or ":" in name
                    or "\x00" in name
                    or not parts.name
                ):
                    raise ValueError(
                        "The archive contains an unsafe file path. Obtain a clean package."
                    )
                if name.casefold() in seen:
                    raise ValueError(
                        "The archive contains duplicate file paths. Obtain a clean package."
                    )
                seen.add(name.casefold())
                if (
                    member.file_size > MAX_FILE_BYTES
                    or member.file_size > max(member.compress_size, 1) * 500
                ):
                    raise ValueError(
                        "The archive contains an oversized or unusually compressed file."
                    )
            for index, member in enumerate(members):
                _check_cancel(cancelled)
                name = member.filename.replace("\\", "/")
                if stat.S_ISLNK(member.external_attr >> 16):
                    files.append(
                        Candidate(name, None, 0, "Linked archive entries are not followed.")
                    )
                    continue
                if member.flag_bits & 1:
                    files.append(
                        Candidate(
                            name,
                            None,
                            member.file_size,
                            "This archive entry is encrypted. Supply an unencrypted source.",
                        )
                    )
                    continue
                target = temporary / str(index)
                written = 0
                with archive.open(member) as input_file, target.open("wb") as output:
                    while chunk := input_file.read(1024 * 1024):
                        _check_cancel(cancelled)
                        written += len(chunk)
                        if written > min(member.file_size, MAX_FILE_BYTES):
                            raise ValueError("The archive expanded beyond its declared size.")
                        output.write(chunk)
                files.append(Candidate(name, target, written))
    except zipfile.BadZipFile as error:
        raise ValueError("This ZIP archive is damaged or is not a supported ZIP file.") from error
    return files


def _copy_object(repo: Repository, candidate: Candidate, cancelled: threading.Event):
    assert candidate.path is not None
    before = candidate.path.stat()
    digest = hashlib.sha256()
    temporary = None
    size = 0
    try:
        with tempfile.NamedTemporaryFile(
            dir=repo.objects, suffix=".partial", delete=False
        ) as output:
            temporary = Path(output.name)
            with candidate.path.open("rb") as source:
                while chunk := source.read(1024 * 1024):
                    _check_cancel(cancelled)
                    size += len(chunk)
                    if size > MAX_FILE_BYTES:
                        raise ValueError(
                            "The source file exceeded the import size limit while reading."
                        )
                    digest.update(chunk)
                    output.write(chunk)
            output.flush()
            os.fsync(output.fileno())
        after = candidate.path.stat()
        if (
            before.st_size != after.st_size
            or before.st_mtime_ns != after.st_mtime_ns
            or size != before.st_size
        ):
            raise ValueError(
                "This source changed while it was being copied. Save it and import again."
            )
        hash_value = digest.hexdigest()
        destination = repo.objects / hash_value
        if destination.exists():
            with destination.open("rb") as saved:
                if hashlib.file_digest(saved, "sha256").hexdigest() != hash_value:
                    raise ValueError(
                        "A saved source failed its integrity check. Restore the workspace from backup."
                    )
        else:
            os.replace(temporary, destination)
            temporary = None
        return hash_value, size, destination
    finally:
        if temporary is not None:
            temporary.unlink(missing_ok=True)


def _extract(repo, path, name, digest, cancelled):
    suffix = Path(name).suffix.lower().replace(".", "") or "unknown"
    cache = repo.home / "extractions" / f"{digest}-{suffix}-{EXTRACTION_VERSION}.json"
    if cache.exists():
        try:
            result = json.loads(cache.read_text(encoding="utf-8"))
            if result.get("status") in {"extracted", "needs_attention", "unsupported"}:
                return result
        except (OSError, ValueError):
            pass  # This is a reproducible derived index; rebuild a corrupt cache from the original.
    result = asdict(extract_document(path, name, cancelled.is_set))
    _check_cancel(cancelled)
    partial = cache.with_suffix(".partial")
    partial.write_text(dump(result), encoding="utf-8")
    os.replace(partial, cache)
    return result


def import_package(
    repo: Repository, tender_id: str, source_path: Path, run_id: str, cancelled: threading.Event
) -> dict:
    repo.get_tender(tender_id)
    _check_cancel(cancelled)
    source = Path(source_path).expanduser().resolve(strict=True)
    if source == repo.home or source.is_relative_to(repo.home) or repo.home.is_relative_to(source):
        raise ValueError("Choose a source package outside the Quantix workspace.")
    if not source.is_dir() and source.suffix.lower() != ".zip":
        raise ValueError("Choose a folder or a ZIP archive containing the Tender package.")
    repo.event(run_id, "import_started", "Reading the package file list.")
    with tempfile.TemporaryDirectory(prefix="quantix-intake-") as temp_dir:
        candidates = (
            _folder_files(source, cancelled)
            if source.is_dir()
            else _archive_files(source, Path(temp_dir), cancelled)
        )
        if not candidates:
            raise ValueError("This package contains no files.")
        changed, hashes = 0, []
        for index, candidate in enumerate(candidates):
            _check_cancel(cancelled)
            repo.update_run(
                run_id,
                progress=int(index / len(candidates) * 90),
                detail=f"Reading {index + 1} of {len(candidates)}: {candidate.name}",
            )
            digest, size = "", candidate.size
            if candidate.problem:
                extraction = {
                    "kind": "other",
                    "status": "unsupported",
                    "warnings": [{"code": "source_unavailable", "message": candidate.problem}],
                    "segments": [],
                    "metadata": {},
                }
            else:
                try:
                    digest, size, object_path = _copy_object(repo, candidate, cancelled)
                    extraction = _extract(repo, object_path, candidate.name, digest, cancelled)
                    hashes.append(digest)
                except InterruptedError:
                    raise
                except (OSError, ValueError) as error:
                    extraction = {
                        "kind": "other",
                        "status": "failed",
                        "warnings": [{"code": "read_failed", "message": str(error)}],
                        "segments": [],
                        "metadata": {},
                    }
            _check_cancel(cancelled)
            artifact, is_new = repo.register_artifact(
                tender_id, candidate.name, digest, size, extraction
            )
            changed += int(is_new)
            repo.event(
                run_id,
                "file_registered",
                f"{candidate.name}: {artifact['status'].replace('_', ' ')}",
                {"artifact_id": artifact["id"], "status": artifact["status"]},
            )
    _check_cancel(cancelled)
    overview = repo.overview(tender_id)
    coverage = overview["coverage"]
    duplicate_count = len(hashes) - len(set(hashes))
    summary = (
        f"Registered {len(candidates)} source files; {changed} new or changed versions. "
        f"The Tender now contains {overview['artifact_count']} current files across {len(overview['areas'])} document areas. "
        f"{overview['evidence_count']} source passages and spreadsheet rows are available to search.\n\n"
        f"{coverage['needs_attention']} files need attention, {coverage['unsupported']} need an additional reader, "
        f"and {coverage['failed']} could not be read. {duplicate_count} identical copies shared their document processing.\n\n"
        "This is the document register, not a completed engineering review. Open Files to inspect coverage and source details. "
        "The Tender Manager can now analyse the available evidence and propose a work plan."
    )
    repo.add_message(tender_id, "system", summary, run_id=run_id)
    repo.update_run(run_id, progress=95, detail="The document register is saved.")
    return {
        "registered": len(candidates),
        "changed": changed,
        "duplicate_count": duplicate_count,
        "coverage": coverage,
        "evidence_count": overview["evidence_count"],
    }
