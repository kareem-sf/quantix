"""Bundled local engines: Tesseract OCR (Arabic + English) and the meaning-search model.

Quantix ships these with the application so a new engineer never installs or
downloads anything. `scripts/fetch_engines.py` assembles the folder and its
`engines.json` manifest; the desktop app passes the installed folder in
`QUANTIX_ENGINES_DIR`. In development the folder is `backend/engines`.
"""

from __future__ import annotations

import hashlib
import json
import os
import threading
from pathlib import Path

MANIFEST = "engines.json"
_VERIFIED: dict[str, dict] = {}
_LOCK = threading.Lock()


def engines_dir() -> Path | None:
    named = os.environ.get("QUANTIX_ENGINES_DIR")
    candidates = [Path(named)] if named else []
    candidates.append(Path(__file__).resolve().parents[1] / "engines")
    for candidate in candidates:
        if (candidate / MANIFEST).is_file():
            return candidate
    return None


def manifest(root: Path | None = None) -> dict:
    root = root or engines_dir()
    if root is None:
        return {}
    try:
        return json.loads((root / MANIFEST).read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return {}


def _digest(path: Path) -> str:
    hasher = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            hasher.update(block)
    return hasher.hexdigest()


def verify(root: Path | None = None, *, full: bool = False) -> dict:
    """Check the bundled files against the manifest.

    The quick check compares presence and size; the full check hashes every file
    once per process. Returns readiness per engine and the first problem found.
    """

    root = root or engines_dir()
    if root is None:
        return {"ocr": False, "meaning": False, "detail": "The bundled engines folder is missing."}
    key = f"{root}|{full}"
    with _LOCK:
        if key in _VERIFIED:
            return _VERIFIED[key]
        data = manifest(root)
        ready = {"ocr": bool(data.get("tesseract")), "meaning": bool(data.get("model"))}
        detail = None
        for relative, expected in (data.get("files") or {}).items():
            path = root / relative
            engine = "ocr" if relative.startswith("tesseract/") else "meaning"
            try:
                if not path.is_file() or path.stat().st_size != expected["size"] or (
                    full and _digest(path) != expected["sha256"]
                ):
                    ready[engine] = False
                    detail = detail or f"A bundled engine file is missing or damaged: {relative}"
            except OSError:
                ready[engine] = False
                detail = detail or f"A bundled engine file cannot be read: {relative}"
        result = ready | {"detail": detail}
        _VERIFIED[key] = result
        return result


def tesseract_path() -> Path | None:
    root = engines_dir()
    data = manifest(root)
    if root is None or not data.get("tesseract"):
        return None
    path = root / data["tesseract"]["executable"]
    return path if path.is_file() else None


def model_path(name: str, revision: str) -> Path | None:
    """The bundled model folder, only when it is exactly the pinned model revision."""

    root = engines_dir()
    model = manifest(root).get("model") or {}
    if root is None or model.get("name") != name or model.get("revision") != revision:
        return None
    path = root / model["path"]
    return path if path.is_dir() else None
