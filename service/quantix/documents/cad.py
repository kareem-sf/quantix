"""DWG and DXF drawings. Quantix reads them with its own reader, `qx-dwg` (the `cad/` crate, built on the MPL-2.0
libraries opencadcodec and opencadkernel), in a separate process, so a drawing the reader can't cope with only ever
stops that process. The reader writes every placed object once, with its layer, block, text, extent and exact length
and area in drawing units, to a folder beside the stored file; Quantix reads that folder whenever it needs the
drawing and never re-reads the file.

A drawing's pages are its spaces: model space first, then each paper layout in tab order. A page's text is the words
printed there, top to bottom, so search, quotes and citations work on drawings as on any document. Objects are named
by keys: the chain of block references they sit in and their own handle, e.g. "1F3/2A" for the line 2A inside the
block reference 1F3. Nothing here decides what an object is for: that is the office's to propose and the engineer's
to approve."""

import json
import math
import os
import subprocess
import sys
import threading
from dataclasses import dataclass
from io import BytesIO
from pathlib import Path

import numpy as np
from pydantic import BaseModel, Field

from quantix.documents.readers import PageText, Unreadable

FORMAT = 2  # the folder format this service reads; qx-dwg writes the same number
TIMEOUT = 600  # seconds for one drawing
CLOSED, ANNOTATION, IN_BLOCK, APPROXIMATE, DERIVED = 1, 2, 4, 8, 16
# Header units ($INSUNITS) Quantix names: the number, what it is called and metres in one unit
UNITS = {
    1: ("inches", 0.0254),
    2: ("feet", 0.3048),
    4: ("millimetres", 0.001),
    5: ("centimetres", 0.01),
    6: ("metres", 1.0),
    7: ("kilometres", 1000.0),
    10: ("yards", 0.9144),
    14: ("decimetres", 0.1),
}
UNIT_NAMES = {name: metres for name, metres in UNITS.values()}
_locks: dict[str, threading.Lock] = {}
_locks_lock = threading.Lock()


def reader() -> Path:
    """The qx-dwg program: QUANTIX_CAD when set, else the one built in this repository."""
    configured = os.environ.get("QUANTIX_CAD")
    if configured:
        return Path(configured)
    name = "qx-dwg.exe" if sys.platform == "win32" else "qx-dwg"
    target = Path(__file__).resolve().parents[3] / "cad" / "target"
    for build in ("release", "debug"):
        if (target / build / name).exists():
            return target / build / name
    raise Unreadable(
        "Quantix's drawing reader isn't built on this computer, so DWG and DXF drawings can't be read yet. "
        "Build it with: cargo build --release --manifest-path cad/Cargo.toml"
    )


def folder(stored: Path) -> Path:
    """Where the reader keeps a stored drawing's objects: named by the file's content, like the file itself."""
    return stored.parent.parent / "drawings" / stored.stem


def _lock(key: str) -> threading.Lock:
    with _locks_lock:
        return _locks.setdefault(key, threading.Lock())


def _current(out: Path) -> bool:
    try:
        return json.loads((out / "drawing.json").read_text(encoding="utf-8")).get("format") == FORMAT
    except (OSError, ValueError):
        return False


def prepare(stored: Path) -> Path:
    """Read the drawing into its folder, once; later calls find it there."""
    out = folder(stored)
    with _lock(str(out)):
        if _current(out):
            return out
        out.parent.mkdir(parents=True, exist_ok=True)
        try:
            done = subprocess.run(
                [str(reader()), "read", str(stored), str(out)],
                capture_output=True,
                text=True,
                encoding="utf-8",
                errors="replace",
                timeout=TIMEOUT,
            )
        except subprocess.TimeoutExpired as error:
            raise Unreadable("This drawing took too long to read; Quantix stopped reading it.") from error
        if done.returncode == 2:
            raise Unreadable(done.stderr.strip() or "This drawing can't be opened.")
        if done.returncode != 0 or not _current(out):
            raise Unreadable("Quantix's drawing reader stopped on this file, so it can't be read.")
    return out


def pages(stored: Path) -> list[PageText]:
    """The drawing's spaces as pages of the words printed in them."""
    drawing = load(prepare(stored))
    return [PageText(s.number, drawing.page_text(s.number), True, s.width, s.height) for s in drawing.spaces]


@dataclass
class Space:
    number: int
    name: str
    kind: str  # model | paper
    extents: tuple[float, float, float, float] | None
    objects: int

    @property
    def width(self) -> float | None:
        return self.extents[2] - self.extents[0] if self.extents else None

    @property
    def height(self) -> float | None:
        return self.extents[3] - self.extents[1] if self.extents else None

    @property
    def label(self) -> str:
        return "model space" if self.kind == "model" else f"layout “{self.name}”"


@dataclass
class Text:
    text: str
    raw: str | None
    x: float
    y: float
    height: float
    rotation: float  # degrees
    tag: str | None  # a block attribute's tag


@dataclass
class Placed:
    """A block reference as placed: the block it shows, its attributes and how many copies (a MINSERT has many)."""

    block: str
    attributes: dict[str, str]
    x: float
    y: float
    rotation: float
    scale: tuple[float, float]
    copies: int
    evaluated: str  # the block record it draws, e.g. the anonymous copy of a dynamic block


@dataclass
class DimensionText:
    measured: float  # from the dimension's own points, in drawing units
    shown: str  # what the drawing prints
    override: str | None  # the text the designer typed in, if any
    stored: float  # the measurement the file keeps
    x: float
    y: float
    kind: str


class Rule(BaseModel):
    """Which objects of a drawing space to take: every condition given must hold, and within a list any one will do.
    Annotation (the drawing of dimensions, leaders and tables) is never taken."""

    layers: list[str] = Field(default=[], description='Layer names; "A-WALL*" takes every layer starting A-WALL')
    blocks: list[str] = Field(default=[], description="Block names: their placed references, wherever they sit")
    types: list[str] = Field(
        default=[],
        description="Object types: Line, Arc, Circle, Ellipse, Polyline, Spline, Hatch, Solid, Point, Insert, Text, "
        "MText, Attribute, Dimension, MLine",
    )
    text: str | None = Field(default=None, description="Words in the object's text or block attributes")
    attributes: dict[str, str] = Field(default={}, description="Block attributes by tag, e.g. {'TYPE': 'D1'}")
    closed: bool | None = Field(default=None, description="Only closed outlines (true) or only open ones (false)")
    hatch_pattern: str | None = Field(default=None, description="Only hatches with this pattern")
    region: list[float] | None = Field(default=None, description="[left, bottom, right, top] in drawing units")
    rooms: list[str] = Field(
        default=[],
        description="Only objects inside these rooms, by name as query_drawing lists them; with types ['Room'], the "
        "rooms themselves (a room's length is its perimeter)",
    )
    keys: list[str] = Field(default=[], description="Exact objects by key, as query_drawing lists them")

    def empty(self) -> bool:
        return not any(
            [
                self.layers,
                self.blocks,
                self.types,
                self.text,
                self.attributes,
                self.region,
                self.keys,
                self.hatch_pattern,
                self.rooms,
            ]
        )


def _name_matches(names: list[str], wanted: list[str]) -> np.ndarray:
    """Which of `names` any of `wanted` names, ignoring case; a trailing * matches a prefix. Bound xref layers
    ("PLAN$0$A-WALL") match on their own last part too."""
    lowered = [n.lower() for n in names]
    local = [n.rsplit("$", 1)[-1] for n in lowered]
    hits = np.zeros(len(names), dtype=bool)
    for w in (w.strip().lower() for w in wanted if w.strip()):
        if w.endswith("*"):
            stem = w[:-1]
            hits |= np.array(
                [n.startswith(stem) or m.startswith(stem) for n, m in zip(lowered, local, strict=True)], dtype=bool
            )
        else:
            hits |= np.array([n == w or m == w for n, m in zip(lowered, local, strict=True)], dtype=bool)
    return hits


class Drawing:
    """A read drawing: its spaces, layers and blocks, and every placed object."""

    def __init__(self, out: Path):
        self.folder = out
        self.info = json.loads((out / "drawing.json").read_text(encoding="utf-8"))
        objects = json.loads((out / "objects.json").read_text(encoding="utf-8"))
        self.types: list[str] = objects["types"]
        self.layer_names: list[str] = objects["layers"]
        self.block_names: list[str] = objects["blocks"]
        self.keys: list[str] = objects["keys"]
        self.index = np.fromfile(out / "index.bin", dtype="<u4").reshape(-1, 8)
        self.num = np.fromfile(out / "num.bin", dtype="<f8").reshape(-1, 6)
        self.coords = np.fromfile(out / "coords.bin", dtype="<f8").reshape(-1, 2)
        self.texts = {row[0]: Text(*row[1:8]) for row in objects["texts"]}
        self.placed = {
            row[0]: Placed(row[1], row[2], row[3], row[4], row[5], (row[6], row[7]), row[8], row[9])
            for row in objects["inserts"]
        }
        self.dimensions = {row[0]: DimensionText(*row[1:8]) for row in objects["dims"]}
        self.tables = {row[0]: row[1] for row in objects["tables"]}
        self.hatches = {row[0]: (row[1], row[2]) for row in objects["hatches"]}
        self.viewports = objects["viewports"]
        self.layers = {layer["name"]: layer for layer in self.info["layers"]}
        self.spaces = [
            Space(s["number"], s["name"], s["kind"], tuple(s["extents"]) if s["extents"] else None, s["objects"])
            for s in self.info["spaces"]
        ]
        self._key_index: dict[str, int] | None = None

    # -- basic facts -------------------------------------------------------------------------------------------

    @property
    def units(self) -> tuple[str, float] | None:
        """The units the drawing's header states, or None when it states none Quantix knows."""
        return UNITS.get(self.info["units"]["insunits"])

    def space(self, number: int) -> Space:
        for s in self.spaces:
            if s.number == number:
                return s
        raise ValueError(f"The drawing has pages 1 to {len(self.spaces)}.")

    @property
    def key_index(self) -> dict[str, int]:
        if self._key_index is None:
            self._key_index = {k: i for i, k in enumerate(self.keys)}
        return self._key_index

    def type_of(self, i: int) -> str:
        return self.types[self.index[i, 1]]

    def layer_of(self, i: int) -> str:
        return self.layer_names[self.index[i, 2]]

    def block_of(self, i: int) -> str | None:
        b = int(self.index[i, 3])
        return self.block_names[b - 1] if b else None

    def flags(self, i: int) -> int:
        return int(self.index[i, 6])

    def parent(self, i: int) -> int | None:
        p = int(self.index[i, 7])
        return p - 1 if p else None

    def bbox(self, i: int) -> tuple[float, float, float, float] | None:
        b = self.num[i, :4]
        return None if np.isnan(b[0]) else (float(b[0]), float(b[1]), float(b[2]), float(b[3]))

    def length(self, i: int) -> float:
        return float(self.num[i, 4])

    def area(self, i: int) -> float:
        return float(self.num[i, 5])

    def copies(self, i: int) -> int:
        placed = self.placed.get(i)
        return placed.copies if placed else 1

    def parts(self, i: int) -> list[np.ndarray]:
        """An object's chains of points; a closed one's last point joins its first."""
        start, count = int(self.index[i, 4]), int(self.index[i, 5])
        if not count:
            return []
        points = self.coords[start : start + count]
        breaks = np.flatnonzero(np.isnan(points[:, 0]))
        chains = (chain[~np.isnan(chain[:, 0])] for chain in np.split(points, breaks))
        return [chain for chain in chains if len(chain)]

    def text_of(self, i: int) -> str:
        if i in self.texts:
            return self.texts[i].text
        if i in self.dimensions:
            return self.dimensions[i].shown
        if i in self.placed:
            return " ".join(v for v in self.placed[i].attributes.values() if v)
        if i in self.tables:
            return " ".join(" ".join(row) for row in self.tables[i])
        return ""

    # -- selection ---------------------------------------------------------------------------------------------

    def select(self, page: int, rule: Rule, include_annotation: bool = False) -> np.ndarray:
        """The objects of a space the rule takes, in drawing order."""
        index = self.index
        mask = index[:, 0] == page
        if not include_annotation:
            mask &= (index[:, 6] & (ANNOTATION | DERIVED)) == 0
        mask &= index[:, 1] != self.types.index("Viewport")
        if rule.keys:
            wanted = {self.key_index[k] for k in rule.keys if k in self.key_index}
            keyed = np.zeros(len(index), dtype=bool)
            keyed[list(wanted)] = True
            mask &= keyed
        if rule.layers:
            mask &= _name_matches(self.layer_names, rule.layers)[index[:, 2]]
        if rule.blocks:
            blocks = np.concatenate([[False], _name_matches(self.block_names, rule.blocks)])
            mask &= blocks[index[:, 3]]
        if rule.types:
            wanted_types = {t.strip().lower() for t in rule.types}
            types = np.array([t.lower() in wanted_types for t in self.types])
            mask &= types[index[:, 1]]
        if rule.closed is not None:
            mask &= ((index[:, 6] & CLOSED) != 0) == rule.closed
        if rule.region:
            left, bottom, right, top = rule.region
            b = self.num[:, :4]
            with np.errstate(invalid="ignore"):
                mask &= (b[:, 0] >= left) & (b[:, 1] >= bottom) & (b[:, 2] <= right) & (b[:, 3] <= top)
        found = np.flatnonzero(mask)
        if rule.hatch_pattern:
            pattern = rule.hatch_pattern.strip().lower()
            found = np.array([i for i in found if i in self.hatches and self.hatches[i][0].lower() == pattern], int)
        if rule.text:
            words = rule.text.lower().split()
            found = np.array([i for i in found if all(w in self.text_of(int(i)).lower() for w in words)], dtype=int)
        if rule.attributes:
            wanted = {k.upper(): v.strip().lower() for k, v in rule.attributes.items()}
            found = np.array(
                [
                    i
                    for i in found
                    if i in self.placed
                    and all(
                        str(self.placed[i].attributes.get(tag, "")).strip().lower() == value
                        for tag, value in wanted.items()
                    )
                ],
                dtype=int,
            )
        return found.astype(int)

    def totals(self, found: np.ndarray) -> dict[str, float]:
        """How many objects (a MINSERT counts each copy), their length and the area of the closed ones."""
        copies = sum(self.copies(int(i)) for i in found)
        return {
            "count": copies,
            "length": float(self.num[found, 4].sum()) if len(found) else 0.0,
            "area": float(self.num[found, 5].sum()) if len(found) else 0.0,
        }

    # -- words -------------------------------------------------------------------------------------------------

    def words(self, page: int) -> list[tuple[int, Text]]:
        """The texts printed in a space, in reading order: top to bottom, then left to right."""
        found = [
            (i, t)
            for i, t in self.texts.items()
            if self.index[i, 0] == page and not self.index[i, 6] & ANNOTATION and t.text.strip()
        ]
        if not found:
            return []
        band = max(float(np.median([t.height for _, t in found])) * 1.5, 1e-9)
        return sorted(found, key=lambda it: (-round(it[1].y / band), it[1].x, it[0]))

    def page_text(self, page: int) -> str:
        space = self.space(page)
        lines = ["Model space" if space.kind == "model" else f"Layout: {space.name}"]
        for _, text in self.words(page):
            lines += [line.strip() for line in text.text.splitlines() if line.strip()]
        for i, rows in self.tables.items():
            if self.index[i, 0] != page:
                continue
            lines.append("Table:")
            for r, row in enumerate(rows, start=1):
                cells = [f"{_column(c)}{r}={cell.strip()}" for c, cell in enumerate(row) if cell.strip()]
                if cells:
                    lines.append(" | ".join(cells))
        written = [d.shown for i, d in self.dimensions.items() if self.index[i, 0] == page and d.override]
        if written:
            lines.append("Dimension texts written by hand: " + " · ".join(written))
        return "\n".join(lines)


def _column(n: int) -> str:
    name = ""
    n += 1
    while n:
        n, r = divmod(n - 1, 26)
        name = chr(65 + r) + name
    return name


_loaded: dict[str, tuple[float, Drawing]] = {}
_loaded_lock = threading.Lock()
KEEP = 6  # drawings kept in memory at once


def load(out: Path) -> Drawing:
    """A drawing's folder, loaded once and kept while in use."""
    stamp = (out / "drawing.json").stat().st_mtime
    key = str(out)
    with _loaded_lock:
        found = _loaded.get(key)
        if found and found[0] == stamp:
            _loaded[key] = _loaded.pop(key)  # most recent last
            return found[1]
    drawing = Drawing(out)
    with _loaded_lock:
        _loaded[key] = (stamp, drawing)
        while len(_loaded) > KEEP:
            _loaded.pop(next(iter(_loaded)))
    return drawing


def drawing(stored: Path) -> Drawing:
    return load(prepare(stored))


def faces(segments: list[tuple[float, float, float, float]], tolerance: float) -> list[np.ndarray]:
    """The closed regions the segments enclose, each a ring of points (qx-dwg faces)."""
    if not segments:
        return []
    request = json.dumps({"segments": segments, "tolerance": tolerance})
    done = subprocess.run(
        [str(reader()), "faces"], input=request, capture_output=True, text=True, encoding="utf-8", timeout=TIMEOUT
    )
    if done.returncode != 0:
        raise ValueError("Quantix couldn't work out the regions of that drawing.")
    return [np.array(ring, dtype=float) for ring in json.loads(done.stdout)["faces"] if len(ring) >= 3]


def ring_area(ring: np.ndarray) -> float:
    if len(ring) < 3:
        return 0.0
    o = ring[0]
    x, y = ring[:, 0] - o[0], ring[:, 1] - o[1]
    return float(abs(np.dot(x, np.roll(y, -1)) - np.dot(np.roll(x, -1), y)) / 2)


def ring_length(ring: np.ndarray) -> float:
    return float(np.hypot(*(np.roll(ring, -1, axis=0) - ring).T).sum()) if len(ring) > 1 else 0.0


def inside(ring: np.ndarray, points: np.ndarray) -> np.ndarray:
    """Which points lie inside the ring (ray casting, vectorised)."""
    points = np.atleast_2d(points)
    x, y = points[:, 0][:, None], points[:, 1][:, None]
    a, b = ring, np.roll(ring, 1, axis=0)
    crosses = (a[:, 1] > y) != (b[:, 1] > y)
    with np.errstate(divide="ignore", invalid="ignore"):
        at = a[:, 0] + (y - a[:, 1]) * (b[:, 0] - a[:, 0]) / (b[:, 1] - a[:, 1])
    return (np.count_nonzero(crosses & (x < at), axis=1) % 2) == 1


# -- pictures ----------------------------------------------------------------------------------------------------

INK = (64, 64, 70)
MARK = (230, 108, 20)
PAPER = (255, 255, 255)


def render(
    d: Drawing,
    page: int,
    width: int = 1600,
    region: tuple[float, float, float, float] | None = None,
    marked: dict[int, str] | None = None,
    layers: set[str] | None = None,
) -> tuple[bytes, tuple[float, float, float, float]]:
    """A PNG of a space, or of a region of it (left, bottom, right, top in drawing units), about `width` pixels
    wide, with the marked objects drawn in orange and labelled. Returns the picture and the region it shows."""
    from PIL import Image, ImageDraw, ImageFont

    space = d.space(page)
    if region is None:
        if space.extents is None:
            raise ValueError(f"{space.label.capitalize()} has nothing drawn in it.")
        left, bottom, right, top = space.extents
        pad = max(right - left, top - bottom) * 0.02 or 1.0
        region = (left - pad, bottom - pad, right + pad, top + pad)
    left, bottom, right, top = region
    if right <= left or top <= bottom:
        raise ValueError("Give the region as [left, bottom, right, top] with left < right and bottom < top.")
    scale = width / (right - left)
    height = max(1, min(int(math.ceil((top - bottom) * scale)), 4 * width))
    image = Image.new("RGB", (width, height), PAPER)
    draw = ImageDraw.Draw(image)
    marked = marked or {}

    def pixel(points: np.ndarray) -> list[tuple[float, float]]:
        return [
            (float(x), float(y))
            for x, y in zip((points[:, 0] - left) * scale, (top - points[:, 1]) * scale, strict=True)
        ]

    b = d.num[:, :4]
    with np.errstate(invalid="ignore"):
        visible = (d.index[:, 0] == page) & (b[:, 2] >= left) & (b[:, 0] <= right) & (b[:, 3] >= bottom)
        visible &= b[:, 1] <= top
    if layers is not None:
        keep = np.array([n in layers for n in d.layer_names])
        visible &= keep[d.index[:, 2]]
    for i in np.flatnonzero(visible):
        if i in marked:
            continue
        closed = d.flags(i) & CLOSED
        for part in d.parts(i):
            if len(part) == 1:
                x, y = pixel(part)[0]
                draw.point((x, y), fill=INK)
            elif len(part) > 1:
                points = pixel(part)
                draw.line(points + ([points[0]] if closed else []), fill=INK, width=1)
    font_cache: dict[int, ImageFont.ImageFont] = {}
    for i, t in d.texts.items():
        if not visible[i] or i in marked:
            continue
        size = int(t.height * scale)
        if size < 7 or size > 200:
            continue
        font = font_cache.setdefault(size, ImageFont.load_default(size=size))
        x, y = pixel(np.array([[t.x, t.y]]))[0]
        draw.text((x, y - size), t.text.splitlines()[0][:80], fill=INK, font=font)
    label_font = ImageFont.load_default(size=14)
    for i, label in marked.items():
        closed = d.flags(i) & CLOSED
        parts = d.parts(i)
        for part in parts:
            points = pixel(part)
            if len(points) > 1:
                draw.line(points + ([points[0]] if closed else []), fill=MARK, width=3)
        box = d.bbox(i)
        if box is None:
            continue
        x, y = pixel(np.array([[box[0], box[3]]]))[0]
        if not parts:
            x2, y2 = pixel(np.array([[box[2], box[1]]]))[0]
            draw.rectangle((x - 4, y - 4, x2 + 4, y2 + 4), outline=MARK, width=3)
        if label:
            draw.text((x + 2, y - 16), label, fill=MARK, font=label_font)
    buffer = BytesIO()
    image.save(buffer, format="PNG")
    return buffer.getvalue(), region


# -- the screen's copy -------------------------------------------------------------------------------------------


def screen_copy(d: Drawing, page: int, stamp: str) -> bytes:
    """A space for the Takeoff screen to draw: every segment in 32-bit floats about the space's centre, with the
    object each belongs to, each object's layer, type, flags and extent, and the texts.

    Layout: b"QXD1", a u32 header length, the header JSON (padded to 4 bytes), then segments (f32 × 4 each), the
    object of each segment (u32), each object's layer, type and flags (u32 × 3) and extent (f32 × 4)."""
    space = d.space(page)
    ext = space.extents or (0.0, 0.0, 1.0, 1.0)
    ox, oy = (ext[0] + ext[2]) / 2, (ext[1] + ext[3]) / 2
    objects = np.flatnonzero(d.index[:, 0] == page)
    segments: list[np.ndarray] = []
    owners: list[np.ndarray] = []
    for n, i in enumerate(objects):
        closed = d.flags(i) & CLOSED
        for part in d.parts(int(i)):
            if len(part) == 1:
                part = np.vstack([part, part])
            if closed and len(part) > 2:
                part = np.vstack([part, part[:1]])
            pairs = np.hstack([part[:-1], part[1:]]) - np.array([ox, oy, ox, oy])
            segments.append(pairs)
            owners.append(np.full(len(pairs), n, dtype="<u4"))
    segs = np.vstack(segments).astype("<f4") if segments else np.zeros((0, 4), "<f4")
    owner = np.concatenate(owners) if owners else np.zeros(0, "<u4")
    meta = d.index[objects][:, [2, 1, 6]].astype("<u4")
    boxes = (d.num[objects, :4] - np.array([ox, oy, ox, oy])).astype("<f4")
    position = {int(i): n for n, i in enumerate(objects)}
    texts = [
        [position[i], t.text, round(t.x - ox, 3), round(t.y - oy, 3), round(t.height, 3), t.rotation]
        for i, t in d.texts.items()
        if i in position and t.text.strip()
    ]
    header = json.dumps(
        {
            "stamp": stamp,
            "page": page,
            "name": space.name,
            "kind": space.kind,
            "origin": [ox, oy],
            "extents": [ext[0] - ox, ext[1] - oy, ext[2] - ox, ext[3] - oy],
            "layers": d.layer_names,
            "hidden": [name for name, layer in d.layers.items() if layer.get("off") or layer.get("frozen")],
            "types": d.types,
            "objects": len(objects),
            "segments": len(segs),
            "texts": texts,
        },
        ensure_ascii=False,
    ).encode("utf-8")
    header += b" " * (-len(header) % 4)
    parts = [b"QXD1", len(header).to_bytes(4, "little"), header, segs.tobytes(), owner.tobytes()]
    parts += [meta.tobytes(), boxes.tobytes()]
    return b"".join(parts)


def objects_on_screen(d: Drawing, page: int) -> np.ndarray:
    """The objects of a space in the order screen_copy numbers them."""
    return np.flatnonzero(d.index[:, 0] == page)
