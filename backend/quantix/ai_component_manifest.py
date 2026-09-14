"""Reviewed software assets and bounded, hash-checked private extraction.

Only installer metadata lives here. This module never imports a provider SDK.
"""

import base64
import binascii
import hashlib
import json
import os
import platform
import re
import shutil
import stat
import sys
import tarfile
import urllib.request
import zipfile
from pathlib import Path, PurePosixPath


class ComponentUnavailable(ValueError):
    """A supported AI software installation is not available."""


def asset_directory() -> Path:
    base = (
        Path(sys._MEIPASS)
        if getattr(sys, "frozen", False)
        else Path(__file__).resolve().parent.parent
    )
    return base / "ai-components"


def worker_directory() -> Path:
    base = (
        Path(sys._MEIPASS)
        if getattr(sys, "frozen", False)
        else Path(__file__).resolve().parent.parent
    )
    return base / "ai_worker"


def native_host() -> Path:
    suffix = ".exe" if os.name == "nt" else ""
    explicit = os.environ.get("QUANTIX_AI_HOST")
    if explicit:
        candidate = Path(explicit)
        if not candidate.is_absolute():
            raise ComponentUnavailable("Quantix's AI launcher needs an absolute application path.")
    elif getattr(sys, "frozen", False):
        candidate = Path(sys._MEIPASS) / "native" / f"quantix-ai-host{suffix}"
    else:
        candidate = (
            Path(__file__).resolve().parents[2]
            / "src-tauri"
            / "target"
            / "debug"
            / f"quantix-ai-host{suffix}"
        )
    if not candidate.is_file() or candidate.stat().st_size == 0:
        raise ComponentUnavailable(
            "Quantix's AI launcher is missing. Repair the Quantix application before preparing AI software."
        )
    if os.name != "nt" and not os.access(candidate, os.X_OK):
        raise ComponentUnavailable(
            "Quantix's AI launcher cannot run. Repair the Quantix application."
        )
    return candidate.resolve()


def platform_key() -> str:
    arch = {"amd64": "x86_64", "x86_64": "x86_64", "arm64": "aarch64", "aarch64": "aarch64"}.get(
        platform.machine().lower()
    )
    operating_system = {"win32": "windows", "darwin": "macos", "linux": "linux"}.get(sys.platform)
    if (
        not arch
        or not operating_system
        or (sys.platform == "linux" and any(Path("/lib").glob("ld-musl-*.so.1")))
    ):
        raise ComponentUnavailable(
            "The pinned AI software does not support this operating system and processor."
        )
    return f"{operating_system}-{arch}"


def read_manifest() -> dict:
    try:
        value = json.loads((asset_directory() / "manifest.json").read_text(encoding="utf-8"))
        if value["format"] != 1 or not value["components"] or not value["platforms"]:
            raise ValueError()
        return value
    except (OSError, ValueError, KeyError, TypeError):
        raise ComponentUnavailable(
            "Quantix's AI software manifest is missing or damaged. Repair the Quantix application."
        ) from None


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        while chunk := source.read(1024 * 1024):
            digest.update(chunk)
    return digest.hexdigest()


def inside(root: Path, relative: str | Path) -> Path:
    path = (root / relative).resolve()
    if path == root.resolve() or not path.is_relative_to(root.resolve()):
        raise ComponentUnavailable(
            "An AI software path is outside its private installation directory."
        )
    return path


def manifest_file(definition: dict, key="path", hash_key="sha256") -> Path:
    path = inside(asset_directory(), definition[key])
    if not path.is_file() or sha256(path) != definition[hash_key]:
        raise ComponentUnavailable(
            "An AI software lock is missing or damaged. Repair the Quantix application."
        )
    return path


def worker_files() -> list[Path]:
    root = worker_directory()
    files = sorted(path for path in root.rglob("*.py") if "__pycache__" not in path.parts)
    if not (root / "worker_entry.py").is_file() or not files:
        raise ComponentUnavailable(
            "Quantix's AI worker files are missing. Repair the Quantix application."
        )
    return files


def diagnostics_writer() -> Path:
    """Return the canonical dependency-free writer copied into workers."""
    path = Path(__file__).resolve().with_name("diagnostics.py")
    if not path.is_file() or path.stat().st_size == 0:
        raise ComponentUnavailable(
            "Quantix's diagnostic writer is missing. Repair the Quantix application."
        )
    return path


def worker_fingerprint() -> str:
    digest = hashlib.sha256()
    for path in worker_files():
        digest.update(path.relative_to(worker_directory()).as_posix().encode())
        digest.update(bytes.fromhex(sha256(path)))
    writer = diagnostics_writer()
    digest.update(b"quantix_ai_worker/diagnostics.py")
    digest.update(bytes.fromhex(sha256(writer)))
    return digest.hexdigest()


def checked_download(asset: dict, target: Path, cancelled) -> None:
    # Python/uv/Node publishers provide SHA-256. Native Grok npm packages
    # publish SHA-512 SRI; retain the publisher's algorithm without substituting
    # a locally calculated, unreviewed checksum.
    expected = {}
    if "sha256" in asset:
        if not re.fullmatch(r"[a-f0-9]{64}", asset.get("sha256", "")):
            raise ComponentUnavailable(
                "The selected AI software download has no reviewed publisher hash."
            )
        expected["sha256"] = bytes.fromhex(asset["sha256"])
    if "integrity" in asset:
        try:
            algorithm, encoded = asset["integrity"].split("-", 1)
            digest = base64.b64decode(encoded, validate=True)
            if algorithm != "sha512" or len(digest) != 64:
                raise ValueError()
            expected["sha512"] = digest
        except (AttributeError, binascii.Error, TypeError, ValueError):
            raise ComponentUnavailable(
                "The selected AI software download has no reviewed publisher SHA-512 integrity."
            ) from None
    if not expected or not asset.get("url", "").startswith("https://"):
        raise ComponentUnavailable(
            "The selected AI software download has no reviewed publisher hash."
        )
    if cancelled.is_set():
        raise InterruptedError("AI software preparation was cancelled.")
    request = urllib.request.Request(asset["url"], headers={"User-Agent": "Quantix-AI-Setup"})
    digests, size = {algorithm: hashlib.new(algorithm) for algorithm in expected}, 0
    with urllib.request.urlopen(request, timeout=30) as response, target.open("wb") as destination:
        while chunk := response.read(1024 * 1024):
            if cancelled.is_set():
                raise InterruptedError("AI software preparation was cancelled.")
            size += len(chunk)
            if size > 600 * 1024 * 1024:
                raise ComponentUnavailable(
                    "The AI software archive exceeds the supported download size."
                )
            for digest in digests.values():
                digest.update(chunk)
            destination.write(chunk)
    if any(digests[algorithm].digest() != value for algorithm, value in expected.items()):
        raise ComponentUnavailable(
            "The AI software download did not match its reviewed publisher hash. Retry preparation."
        )


def extract_archive(archive: Path, destination: Path, cancelled, allow_links=True) -> None:
    destination.mkdir(parents=True, exist_ok=True)
    maximum_size, maximum_files = 3 * 1024**3, 100000

    def member_path(name):
        parts = PurePosixPath(name).parts
        if (
            not parts
            or PurePosixPath(name).is_absolute()
            or ".." in parts
            or "\\" in name
            or ":" in name
        ):
            raise ComponentUnavailable("The AI software archive contains an unsafe path.")
        return inside(destination, name)

    if zipfile.is_zipfile(archive):
        with zipfile.ZipFile(archive) as bundle:
            members = bundle.infolist()
            if (
                len(members) > maximum_files
                or sum(item.file_size for item in members) > maximum_size
            ):
                raise ComponentUnavailable("The AI software archive exceeds its extraction limits.")
            for member in members:
                if cancelled.is_set():
                    raise InterruptedError("AI software preparation was cancelled.")
                target = member_path(member.filename)
                if stat.S_ISLNK(member.external_attr >> 16):
                    raise ComponentUnavailable(
                        "The Windows AI software archive contains an unexpected link."
                    )
                if member.is_dir():
                    target.mkdir(parents=True, exist_ok=True)
                else:
                    target.parent.mkdir(parents=True, exist_ok=True)
                    with bundle.open(member) as source, target.open("wb") as output:
                        shutil.copyfileobj(source, output)
                    if os.name != "nt":
                        target.chmod((member.external_attr >> 16) & 0o755 or 0o644)
    else:
        with tarfile.open(archive, "r:*") as bundle:
            members = bundle.getmembers()
            if len(members) > maximum_files or sum(item.size for item in members) > maximum_size:
                raise ComponentUnavailable("The AI software archive exceeds its extraction limits.")
            for member in members:
                if cancelled.is_set():
                    raise InterruptedError("AI software preparation was cancelled.")
                member_path(member.name)
                if not (member.isfile() or member.isdir() or member.issym() or member.islnk()):
                    raise ComponentUnavailable(
                        "The AI software archive contains an unsupported device entry."
                    )
                if not allow_links and (member.issym() or member.islnk()):
                    raise ComponentUnavailable(
                        "The native AI software archive contains an unexpected link."
                    )
                # Python's data filter bounds symbolic/hard links and metadata.
                bundle.extract(member, destination, filter="data")


def inventory(directory: Path, cancelled=None) -> dict:
    result = {}
    for path in sorted(directory.rglob("*")):
        if cancelled is not None and cancelled.is_set():
            raise InterruptedError("AI software preparation was cancelled.")
        if path.name == "receipt.json" or "__pycache__" in path.parts:
            continue
        if path.is_file():
            result[path.relative_to(directory).as_posix()] = {
                "size": path.stat().st_size,
                "sha256": sha256(path),
            }
    return result


def receipt_valid(directory: Path, *, deep=False, allowed_roots=()) -> bool:
    try:
        receipt = json.loads((directory / "receipt.json").read_text(encoding="utf-8"))
        if receipt.get("format") != 1 or not receipt.get("files"):
            return False
        names = receipt["files"] if deep else receipt.get("required", [])
        if not names:
            return False
        for relative in names:
            expected = receipt["files"][relative]
            # uv uses interpreter links on POSIX. A component may link only into
            # its separately recorded, hash-checked private Python base.
            path = Path(os.path.abspath(directory / relative))
            if not path.is_relative_to(directory.absolute()) or path == directory.absolute():
                return False
            resolved = path.resolve()
            if not resolved.is_relative_to(directory.resolve()) and not any(
                resolved.is_relative_to(root.resolve()) for root in allowed_roots
            ):
                return False
            if not path.is_file() or path.stat().st_size != expected["size"]:
                return False
            if deep and sha256(path) != expected["sha256"]:
                return False
        return True
    except (OSError, ValueError, KeyError, TypeError):
        return False
