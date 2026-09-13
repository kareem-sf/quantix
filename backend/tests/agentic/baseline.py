"""Working-source hash capture so integration cannot overwrite altered files."""

from __future__ import annotations

import hashlib
import shutil
from pathlib import Path

INTEGRATION_BLOCKED = (
    "An original working file changed after the baseline was captured. "
    "Integration is blocked so the engineer's copy is not overwritten."
)


def file_digest(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def capture_tree(root: Path) -> dict[str, str]:
    """Hash every regular file under ``root`` using stable POSIX relative paths."""

    captured: dict[str, str] = {}
    for path in sorted(root.rglob("*")):
        if not path.is_file() or path.is_symlink():
            continue
        relative = path.relative_to(root).as_posix()
        captured[relative] = file_digest(path)
    return captured


def copy_tree(source: Path, destination: Path) -> None:
    destination.mkdir(parents=True, exist_ok=True)
    for path in source.rglob("*"):
        relative = path.relative_to(source)
        target = destination / relative
        if path.is_dir():
            target.mkdir(parents=True, exist_ok=True)
            continue
        if path.is_file() and not path.is_symlink():
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(path, target)


def integrate_tree(source: Path, destination: Path, captured: dict[str, str]) -> None:
    """Copy ``source`` onto ``destination`` only while captured hashes still match.

    A destination file whose current digest differs from the captured baseline
    is treated as an independent engineer edit and blocks integration.
    """

    for relative, expected in captured.items():
        current = destination / Path(relative)
        if current.is_file() and file_digest(current) != expected:
            raise ValueError(f"{INTEGRATION_BLOCKED} ({relative})")
    copy_tree(source, destination)
