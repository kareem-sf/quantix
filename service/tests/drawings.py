"""Synthetic CAD drawings for the tests, written by Quantix's own reader (qx-dwg sample): no customer drawing is ever
committed.

PLAN is a two-room flat in millimetres: walls 200 thick around 10 × 8 m, a fire-rated wall between the rooms 100
thick with a 900 mm door in it, three windows (two of type W1, one W2), a floor hatch over the bedroom with a 1 × 1 m
island, a hand-written dimension that says 9800 where the drawing measures 10000, a pipe through the fire-rated
wall, a line drawn twice, and a layout with the title block's words.

What the sample can't write (layers switched off or frozen, objects the reader skips) is added to a DXF it wrote,
as CAD would save it."""

import json
import subprocess
import tempfile
from pathlib import Path

from quantix.documents import cad


def make_drawing(spec: dict, suffix: str = ".dwg") -> bytes:
    with tempfile.TemporaryDirectory() as folder:
        spec_path, out = Path(folder) / "spec.json", Path(folder) / f"drawing{suffix}"
        spec_path.write_text(json.dumps(spec), encoding="utf-8")
        subprocess.run([str(cad.reader()), "sample", str(spec_path), str(out)], check=True, capture_output=True)
        return out.read_bytes()


def saved_by_cad(
    dxf: bytes, off: tuple[str, ...] = (), frozen: tuple[str, ...] = (), entities: tuple[str, ...] = ()
) -> bytes:
    """A DXF qx-dwg sample wrote, as CAD saves it with layers switched off (a negative colour) or frozen (flag 1),
    and with objects the sample can't write added to model space, given as their DXF group codes and values."""
    text = dxf.decode("utf-8")
    for name in (*off, *frozen):
        record = f"{name}\r\n 70\r\n     0\r\n 62\r\n     7\r\n"
        assert record in text
        flag, colour = "1" if name in frozen else "0", "-7" if name in off else "7"
        text = text.replace(record, f"{name}\r\n 70\r\n{flag:>6}\r\n 62\r\n{colour:>6}\r\n")
    end = "  0\r\nENDSEC\r\n  0\r\nSECTION\r\n  2\r\nOBJECTS"  # the end of the entities
    assert end in text
    return text.replace(end, "".join(f"{line}\r\n" for line in entities) + end).encode("utf-8")


def _line(layer: str, a: list[float], b: list[float]) -> dict:
    return {"type": "line", "layer": layer, "from": a, "to": b}


WALLS = [
    {"type": "polyline", "layer": "A-WALL", "points": [[0, 0], [10000, 0], [10000, 8000], [0, 8000]], "closed": True},
    _line("A-WALL", [200, 200], [4950, 200]),
    _line("A-WALL", [5050, 200], [9800, 200]),
    _line("A-WALL", [9800, 200], [9800, 7800]),
    _line("A-WALL", [9800, 7800], [5050, 7800]),
    _line("A-WALL", [4950, 7800], [200, 7800]),
    _line("A-WALL", [200, 7800], [200, 200]),
    _line("A-WALL-FIRE", [4950, 200], [4950, 3000]),
    _line("A-WALL-FIRE", [4950, 3900], [4950, 7800]),
    _line("A-WALL-FIRE", [5050, 200], [5050, 3000]),
    _line("A-WALL-FIRE", [5050, 3900], [5050, 7800]),
    _line("A-WALL-FIRE", [4950, 3000], [5050, 3000]),
    _line("A-WALL-FIRE", [4950, 3900], [5050, 3900]),
]
BLOCKS = [
    {
        "name": "DOOR-900",
        "base": [0, 0],
        "entities": [
            {"type": "line", "from": [0, 0], "to": [900, 0]},
            {"type": "arc", "centre": [0, 0], "radius": 900, "start": 0, "end": 90},
        ],
    },
    {
        "name": "WIN-1200",
        "base": [0, 0],
        "entities": [
            {"type": "line", "from": [0, 0], "to": [1200, 0]},
            {"type": "line", "from": [0, 200], "to": [1200, 200]},
            {"type": "line", "from": [0, 0], "to": [0, 200]},
            {"type": "line", "from": [1200, 0], "to": [1200, 200]},
        ],
    },
]
WINDOWS = [
    {"type": "insert", "block": "WIN-1200", "layer": "A-GLAZ", "at": [1000, 0], "attributes": {"TYPE": "W1"}},
    {"type": "insert", "block": "WIN-1200", "layer": "A-GLAZ", "at": [6000, 0], "attributes": {"TYPE": "W1"}},
    {"type": "insert", "block": "WIN-1200", "layer": "A-GLAZ", "at": [3000, 7800], "attributes": {"TYPE": "W2"}},
]
OTHERS = [
    {"type": "insert", "block": "DOOR-900", "layer": "A-DOOR", "at": [5050, 3000]},
    {"type": "text", "layer": "A-ROOM", "at": [2500, 4000], "value": "LIVING", "height": 250},
    {"type": "text", "layer": "A-ROOM", "at": [7500, 4000], "value": "BEDROOM", "height": 250},
    {
        "type": "hatch",
        "layer": "A-FLOR",
        "pattern": "ANSI31",
        "loops": [
            [[5050, 200], [9800, 200], [9800, 7800], [5050, 7800]],
            [[7000, 3000], [8000, 3000], [8000, 4000], [7000, 4000]],
        ],
    },
    {"type": "dimension", "layer": "A-DIMS", "from": [0, -1000], "to": [10000, -1000], "text": "9800"},
    {"type": "dimension", "layer": "A-DIMS", "from": [0, 8500], "to": [5000, 8500]},
    _line("M-PIPE", [4000, 6000], [6000, 6000]),
    _line("A-TEMP", [0, -2000], [1000, -2000]),
    _line("A-TEMP", [0, -2000], [1000, -2000]),
]
LAYOUT = {
    "name": "A-101",
    "entities": [
        {"type": "text", "at": [20, 20], "value": "DRAWING NO A-101", "height": 5},
        {"type": "text", "at": [20, 40], "value": "ALL DIMENSIONS ARE IN MILLIMETRES", "height": 5},
    ],
}
LAYERS = ["A-WALL", "A-WALL-FIRE", "A-DOOR", "A-GLAZ", "A-ROOM", "A-FLOR", "A-DIMS", "M-PIPE", "A-TEMP"]


def plan(extra: list[dict] | None = None, insunits: int = 4) -> dict:
    return {
        "insunits": insunits,
        "layers": LAYERS,
        "blocks": BLOCKS,
        "entities": WALLS + WINDOWS + OTHERS + (extra or []),
        "layouts": [LAYOUT],
    }


MAP = {
    "A-WALL": "walls",
    "A-WALL-FIRE": "fire_rated",
    "A-DOOR": "doors",
    "A-GLAZ": "windows",
    "A-ROOM": "room_label",
    "A-FLOR": "floor_finish",
    "A-DIMS": "dimensions",
    "M-PIPE": "services",
    "A-TEMP": "ignore",
}
