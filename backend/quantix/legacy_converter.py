"""Local DWG→DXF conversion only through a detected ODA File Converter."""

from __future__ import annotations

import os
import shutil
import subprocess
from pathlib import Path

from .storage import runtime_tmp_dir


class CadConversionUnavailable(Exception):
    """DWG remains registered; DXF or PDF companions are the working drawing path."""


def oda_converter() -> Path | None:
    named = os.environ.get("QUANTIX_ODA_CONVERTER")
    candidates = []
    if named:
        candidates.append(Path(named))
    found = shutil.which("ODAFileConverter")
    if found:
        candidates.append(Path(found))
    program_files = [os.environ.get("ProgramFiles"), os.environ.get("ProgramFiles(x86)")]
    for root in program_files:
        if not root:
            continue
        candidates.extend(Path(root).glob("ODA/ODAFileConverter*/ODAFileConverter.exe"))
        candidates.extend(Path(root).glob("ODA File Converter*/ODAFileConverter.exe"))
    for path in candidates:
        if path.is_file():
            return path
    return None


def convert_dwg(path: Path, *, home: Path | None = None) -> Path:
    converter = oda_converter()
    if converter is None:
        raise CadConversionUnavailable(
            "A licensed local DWG converter is not installed. Use a DXF or PDF companion."
        )
    source = Path(path)
    if source.suffix.lower() != ".dwg":
        raise CadConversionUnavailable("Only DWG files can use the local DWG converter.")
    output_dir = runtime_tmp_dir(home) / "cad-convert"
    output_dir.mkdir(parents=True, exist_ok=True)
    completed = subprocess.run(
        [str(converter), str(source.parent), str(output_dir), "ACAD2018", "DXF", "0", "1", source.name],
        capture_output=True,
        timeout=60,
        check=False,
    )
    dxf = output_dir / (source.stem + ".dxf")
    if completed.returncode != 0 or not dxf.is_file():
        raise CadConversionUnavailable("The local DWG converter could not produce a DXF file.")
    return dxf
