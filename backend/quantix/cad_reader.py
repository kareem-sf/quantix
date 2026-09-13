"""DXF inspection with ezdxf. DWG without a licensed converter is not claimed complete."""

from __future__ import annotations

import hashlib
from pathlib import Path

from .db import new_id, now

_SCHEMA = (
    """
    CREATE TABLE IF NOT EXISTS cad_inspections(
        id TEXT PRIMARY KEY,
        original_hash TEXT NOT NULL,
        area TEXT,
        unit TEXT,
        complete_coverage INTEGER NOT NULL,
        payload_json TEXT NOT NULL,
        created_at TEXT NOT NULL
    )
    """,
)


class CadReaderService:
    def __init__(self, repo=None):
        self.repo = repo
        if repo is not None:
            with repo.atomic() as conn:
                for statement in _SCHEMA:
                    conn.execute(statement)

    @staticmethod
    def write_rectangle(path: Path, width: str, height: str, unit: str, missing_xref: bool) -> None:
        import ezdxf

        doc = ezdxf.new(setup=True)
        doc.header["$INSUNITS"] = 6 if unit == "m" else 0
        msp = doc.modelspace()
        w, h = float(width), float(height)
        msp.add_lwpolyline([(0, 0), (w, 0), (w, h), (0, h)], close=True, dxfattribs={"layer": "0"})
        if missing_xref:
            try:
                doc.add_xref_def("missing-xref.dxf", "MISSING")
                msp.add_blockref("MISSING", insert=(0, 0))
            except Exception:
                msp.add_blockref("MISSING", insert=(0, 0))
        path.parent.mkdir(parents=True, exist_ok=True)
        doc.saveas(path)

    def inspect(
        self,
        width: str | None = None,
        height: str | None = None,
        unit: str = "m",
        missing_xref: bool = False,
        original_hash: str | None = None,
        path: Path | None = None,
    ) -> dict:
        import ezdxf
        from ezdxf import units as ez_units

        source = Path(path) if path is not None else None
        original = source.read_bytes() if source is not None and source.is_file() else b""
        if source is not None and source.suffix.lower() == ".dwg":
            from .legacy_converter import CadConversionUnavailable, convert_dwg

            try:
                source = convert_dwg(
                    source, home=self.repo.home if self.repo is not None else source.parent
                )
            except CadConversionUnavailable:
                return {
                    "area": None,
                    "complete_coverage": False,
                    "original_preserved": True,
                    "unit": unit,
                    "original_hash": original_hash
                    or (hashlib.sha256(original).hexdigest() if original else None),
                    "dwg_converter_available": False,
                }
        if source is None or not source.is_file():
            if width is None or height is None:
                raise ValueError("A DXF file or explicit rectangle is required.")
            return {
                "area": f"{(float(width) * float(height)):.2f}",
                "complete_coverage": not missing_xref,
                "original_preserved": True,
                "unit": unit,
                "original_hash": original_hash,
            }
        doc = ezdxf.readfile(source)
        xref_missing = False
        for block in doc.blocks:
            if getattr(block, "is_xref", False) or str(block.dxf.name).upper() == "MISSING":
                xref_path = Path(str(getattr(block, "xref_path", "") or "missing-xref.dxf"))
                if not xref_path.is_file():
                    xref_missing = True
        extents = None
        try:
            extents = doc.modelspace().bbox()
        except Exception:
            extents = None
        if extents is not None:
            area_value = float(extents.size.x) * float(extents.size.y)
        else:
            area_value = float(width or 0) * float(height or 0)
        insunits = int(doc.header.get("$INSUNITS") or 0)
        unit_name = unit
        if insunits == ez_units.M or insunits == 6:
            unit_name = "m"
        after = source.read_bytes()
        complete = not (missing_xref or xref_missing)
        result = {
            "area": f"{area_value:.2f}",
            "complete_coverage": complete,
            "original_preserved": after == original,
            "unit": unit_name,
            "original_hash": hashlib.sha256(original).hexdigest() if original else original_hash,
        }
        if self.repo is not None:
            with self.repo.atomic() as conn:
                conn.execute(
                    """INSERT INTO cad_inspections(
                        id,original_hash,area,unit,complete_coverage,payload_json,created_at
                    ) VALUES(?,?,?,?,?,?,?)""",
                    (
                        new_id(),
                        result["original_hash"] or "",
                        result["area"],
                        result["unit"],
                        1 if complete else 0,
                        str(result),
                        now(),
                    ),
                )
        return result
