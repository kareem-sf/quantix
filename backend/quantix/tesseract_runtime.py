"""Locate Tesseract: the bundled engine first, then managed home, PATH and Program Files."""

from __future__ import annotations

import os
import shutil
import subprocess
from pathlib import Path

from .storage import ai_components_dir, normal_home

_LANGUAGES: dict[str, frozenset[str]] = {}


def tesseract_executable(home: Path | str | None = None) -> Path | None:
    from .engines import tesseract_path

    named = os.environ.get("QUANTIX_TESSERACT")
    candidates: list[Path] = []
    if named:
        candidates.append(Path(named))
    bundled = tesseract_path()
    if bundled is not None:
        candidates.append(bundled)
    if home is not None:
        root = Path(home)
        candidates.append(ai_components_dir(root) / "tesseract" / "tesseract.exe")
        candidates.append(ai_components_dir(root) / "tesseract" / "tesseract")
    candidates.append(normal_home() / "ai-components" / "tesseract" / "tesseract.exe")
    which = shutil.which("tesseract")
    if which:
        candidates.append(Path(which))
    program_files = [os.environ.get("ProgramFiles"), os.environ.get("ProgramFiles(x86)")]
    for root in program_files:
        if root:
            candidates.append(Path(root) / "Tesseract-OCR" / "tesseract.exe")
    seen: set[str] = set()
    for path in candidates:
        resolved = path.expanduser()
        key = str(resolved).lower()
        if key in seen:
            continue
        seen.add(key)
        if resolved.is_file():
            return resolved
    return None


def tesseract_languages(home: Path | str | None = None) -> frozenset[str]:
    """Language codes this machine can recognise; empty when Tesseract is missing."""

    executable = tesseract_executable(home)
    if executable is None:
        return frozenset()
    key = str(executable).lower()
    if key in _LANGUAGES:
        return _LANGUAGES[key]
    try:
        listed = subprocess.run(
            [str(executable), "--list-langs"],
            capture_output=True,
            text=True,
            timeout=30,
            check=False,
            stdin=subprocess.DEVNULL,
        )
    except (OSError, subprocess.SubprocessError):
        return frozenset()
    # The first line is a heading; codes never contain spaces.
    codes = frozenset(
        line.strip()
        for line in listed.stdout.splitlines()[1:]
        if line.strip() and " " not in line.strip()
    )
    _LANGUAGES[key] = codes
    return codes
