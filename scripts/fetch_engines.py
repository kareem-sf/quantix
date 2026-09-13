"""Assemble Quantix's bundled local engines into backend/engines.

Quantix ships its text-recognition engine (Tesseract with Arabic and English)
and its meaning-search model with the application, so engineers never install
or download anything. This script downloads pinned, SHA-256 verified sources,
unpacks them without installing anything on the build machine, and writes the
`engines.json` manifest that `quantix.engines` verifies at runtime.

Run with the backend environment:  node scripts/python.mjs scripts/fetch_engines.py
Use --print-hashes to compute digests when updating a pinned source.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import shutil
import subprocess
import sys
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
ENGINES = ROOT / "backend" / "engines"
CACHE = ROOT / "backend" / ".engine-cache"

MODEL_NAME = "intfloat/multilingual-e5-small"
MODEL_REVISION = "614241f622f53c4eeff9890bdc4f31cfecc418b3"
MODEL_FOLDER = f"models/multilingual-e5-small-{MODEL_REVISION}"
TESSDATA_COMMIT = "e12c65a915945e4c28e237a9b52bc4a8f39a0cec"

# name -> (url, sha256)
_TESSDATA = f"https://raw.githubusercontent.com/tesseract-ocr/tessdata_best/{TESSDATA_COMMIT}"
_MODEL = f"https://huggingface.co/{MODEL_NAME}/resolve/{MODEL_REVISION}"
SOURCES: dict[str, tuple[str, str]] = {
    "7zip.msi": ("https://github.com/ip7z/7zip/releases/download/26.03/7z2603-x64.msi", "c0680064d698a62dd4a5a47f403db356a6531a5473e4c4b1d090ea2590513926"),
    "tesseract-setup.exe": (
        "https://github.com/UB-Mannheim/tesseract/releases/download/v5.4.0.20240606/"
        "tesseract-ocr-w64-setup-5.4.0.20240606.exe",
        "c885fff6998e0608ba4bb8ab51436e1c6775c2bafc2559a19b423e18678b60c9",
    ),
    "tessdata/eng.traineddata": (f"{_TESSDATA}/eng.traineddata", "8280aed0782fe27257a68ea10fe7ef324ca0f8d85bd2fd145d1c2b560bcb66ba"),
    "tessdata/ara.traineddata": (f"{_TESSDATA}/ara.traineddata", "ab9d157d8e38ca00e7e39c7d5363a5239e053f5b0dbdb3167dde9d8124335896"),
    "tessdata/osd.traineddata": (f"{_TESSDATA}/osd.traineddata", "9cf5d576fcc47564f11265841e5ca839001e7e6f38ff7f7aacf46d15a96b00ff"),
    "model/config.json": (f"{_MODEL}/config.json", "69137736cab8b8903a07fe8afaafdda25aac55415a12a55d1bffa9f581abf959"),
    "model/tokenizer.json": (f"{_MODEL}/tokenizer.json", "0b44a9d7b51c3c62626640cda0e2c2f70fdacdc25bbbd68038369d14ebdf4c39"),
    "model/tokenizer_config.json": (f"{_MODEL}/tokenizer_config.json", "a1d6bc8734a6f635dc158508bef000f8e2e5a759c7d92f984b2c86e5ff53425b"),
    "model/special_tokens_map.json": (f"{_MODEL}/special_tokens_map.json", "d05497f1da52c5e09554c0cd874037a083e1dc1b9cfd48034d1c717f1afc07a7"),
    "model/onnx/model.onnx": (f"{_MODEL}/onnx/model.onnx", "ca456c06b3a9505ddfd9131408916dd79290368331e7d76bb621f1cba6bc8665"),
}


def sha256(path: Path) -> str:
    hasher = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            hasher.update(block)
    return hasher.hexdigest()


def fetch(name: str) -> Path:
    url, expected = SOURCES[name]
    target = CACHE / name
    if not target.is_file() or (expected and sha256(target) != expected):
        target.parent.mkdir(parents=True, exist_ok=True)
        partial = target.with_suffix(target.suffix + ".part")
        print(f"Downloading {name}")
        with urllib.request.urlopen(url, timeout=120) as response, partial.open("wb") as out:
            shutil.copyfileobj(response, out, 1024 * 1024)
        partial.replace(target)
    if expected and sha256(target) != expected:
        raise SystemExit(f"{name} does not match its pinned SHA-256; refusing to bundle it.")
    return target


def seven_zip() -> Path:
    folder = CACHE / "7zip"
    executable = next(folder.rglob("7z.exe"), None) if folder.exists() else None
    if executable is None:
        # An administrative MSI extraction unpacks files without installing 7-Zip.
        # msiexec parses its own command line: the property value must be quoted.
        subprocess.run(
            f'msiexec /a "{fetch("7zip.msi")}" /qn TARGETDIR="{folder}"',
            check=True,
        )
        executable = next(folder.rglob("7z.exe"))
    return executable


def pe_imports(path: Path) -> set[str]:
    """DLL names imported by a Windows executable or library (read from its PE import table)."""

    import struct

    data = path.read_bytes()
    pe = struct.unpack_from("<I", data, 0x3C)[0]
    sections, optional_size = struct.unpack_from("<H", data, pe + 6)[0], struct.unpack_from("<H", data, pe + 20)[0]
    optional = pe + 24
    magic = struct.unpack_from("<H", data, optional)[0]
    directories = optional + (112 if magic == 0x20B else 96)
    import_rva = struct.unpack_from("<I", data, directories + 8)[0]
    table = optional + optional_size
    spans = [struct.unpack_from("<IIII", data, table + 40 * index + 8) for index in range(sections)]

    def offset(rva):
        for virtual_size, virtual_address, raw_size, raw_pointer in spans:
            if virtual_address <= rva < virtual_address + max(virtual_size, raw_size):
                return raw_pointer + rva - virtual_address
        raise ValueError("RVA outside sections")

    names, cursor = set(), offset(import_rva) if import_rva else None
    while cursor is not None:
        name_rva = struct.unpack_from("<I", data, cursor + 12)[0]
        if not name_rva:
            break
        start = offset(name_rva)
        names.add(data[start:data.index(b"\0", start)].decode("ascii").lower())
        cursor += 20
    return names


def keep_runtime_only(folder: Path) -> None:
    """Keep tesseract.exe and the bundled DLLs it needs; drop training tools."""

    bundled = {path.name.lower(): path for path in folder.glob("*.dll")}
    needed, queue = set(), [folder / "tesseract.exe"]
    while queue:
        for name in pe_imports(queue.pop()):
            if name in bundled and name not in needed:
                needed.add(name)
                queue.append(bundled[name])
    for path in folder.iterdir():
        if path.is_file() and path.name.lower() != "tesseract.exe" and path.name.lower() not in needed:
            path.unlink()


def assemble() -> None:
    if sys.platform != "win32":
        raise SystemExit("Bundled engines are currently assembled for Windows packages.")
    if ENGINES.exists():
        shutil.rmtree(ENGINES)
    tesseract = ENGINES / "tesseract"
    unpacked = CACHE / "tesseract-unpacked"
    if unpacked.exists():
        shutil.rmtree(unpacked)
    subprocess.run(
        [str(seven_zip()), "x", str(fetch("tesseract-setup.exe")), f"-o{unpacked}", "-y"],
        check=True, stdout=subprocess.DEVNULL,
    )
    shutil.copytree(
        unpacked, tesseract,
        ignore=shutil.ignore_patterns("$PLUGINSDIR", "$TEMP", "uninstall*", "*.html", "doc", "tessdata"),
    )
    keep_runtime_only(tesseract)
    (tesseract / "tessdata").mkdir()
    for language in ("eng", "ara", "osd"):
        shutil.copy2(fetch(f"tessdata/{language}.traineddata"), tesseract / "tessdata")
    model = ENGINES / MODEL_FOLDER
    for name in SOURCES:
        if name.startswith("model/"):
            relative = name.removeprefix("model/")
            (model / relative).parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(fetch(name), model / relative)
    files = {
        path.relative_to(ENGINES).as_posix(): {"size": path.stat().st_size, "sha256": sha256(path)}
        for path in sorted(ENGINES.rglob("*"))
        if path.is_file()
    }
    manifest = {
        "format": 1,
        "tesseract": {"executable": "tesseract/tesseract.exe", "version": "5.4.0.20240606",
                      "languages": ["eng", "ara", "osd"], "tessdata": f"tessdata_best@{TESSDATA_COMMIT}"},
        "model": {"name": MODEL_NAME, "revision": MODEL_REVISION, "path": MODEL_FOLDER},
        "files": files,
    }
    (ENGINES / "engines.json").write_text(json.dumps(manifest, indent=1), encoding="utf-8")
    size = sum(item["size"] for item in files.values())
    print(f"Bundled engines ready: {len(files)} files, {size / 1024 / 1024:.0f} MB in {ENGINES}")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--print-hashes", action="store_true", help="Download sources and print their SHA-256.")
    arguments = parser.parse_args()
    if arguments.print_hashes:
        for name in SOURCES:
            print(f"{name}: {sha256(fetch(name))}")
        return
    assemble()


if __name__ == "__main__":
    main()
