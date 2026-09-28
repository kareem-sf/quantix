"""A PDF drawing's own lines, as a drawing Quantix measures by its objects. Most drawings in a tender are PDFs
printed from CAD, and they keep the drawing's lines as lines: Quantix reads them from the page and keeps them in the
same folder format as a DWG it read (see `cad`), so choosing, measuring and checking work the same on both.

A PDF holds strokes, not CAD objects, so Quantix puts objects back together:
- strokes of one pen whose ends meet, where no third stroke meets them, are one line; a line that comes back to
  where it started is a closed outline, with an area;
- a pen (colour and line weight) is a layer, unless the PDF keeps the drawing's own layers, whose names it then uses;
- CAD prints its text as strokes, with the words written invisibly over them: the strokes inside a word's box are
  that word's object, so they are drawn but never taken for linework;
- filled shapes are hatches, and curves drawn as a circle are a circle.

Coordinates are page points from the bottom left, as the PDF gives them: the sheet's points from the top left are
(x, height - y). A page's scale turns them into metres."""

import ctypes
import json
import math
import shutil
import threading
from dataclasses import dataclass, field
from functools import lru_cache
from pathlib import Path

import numpy as np
import pypdfium2 as pdfium
import pypdfium2.raw as raw

from quantix.documents import cad
from quantix.documents.readers import PDFIUM, Unreadable

FORMAT = 1  # what this reading writes; bumped whenever it changes, so older folders are read again
MIN_LINES = 200  # a page with fewer drawn strokes than this is a scan or a text page, not a drawing to measure
JOIN = 0.01  # points: stroke ends closer than this are one point
CURVE = 0.01  # points: the most a curve's straight pieces stray from it
LETTER_PAD = 0.2  # a word's box grows by this share of its height to take in the strokes of its letters
MANY_RINGS = 50  # a fill of more pieces than this is summed by the way round each runs, not by nesting
TYPES = ["Line", "Polyline", "Circle", "Hatch", "Point", "Text", "Viewport"]  # Viewport: never used; cad asks
LINE, POLYLINE, CIRCLE, HATCH, POINT, TEXT = range(6)
LETTERING = 32  # a flag beyond cad's: strokes that draw a word's letters
MM_PER_POINT = 25.4 / 72
_NAMES = {
    "black": (0, 0, 0),
    "white": (255, 255, 255),
    "grey": (128, 128, 128),
    "red": (255, 0, 0),
    "orange": (255, 127, 0),
    "yellow": (255, 255, 0),
    "green": (0, 200, 0),
    "cyan": (0, 255, 255),
    "blue": (0, 0, 255),
    "magenta": (255, 0, 255),
    "purple": (128, 0, 160),
    "brown": (150, 92, 0),
    "pink": (255, 150, 200),
}
_locks: dict[str, threading.Lock] = {}
_locks_lock = threading.Lock()

Point = tuple[float, float]


def folder(stored: Path, number: int) -> Path:
    return stored.parent.parent / "drawings" / f"{stored.stem}-p{number}"


@lru_cache(maxsize=4096)
def pen_name(rgb: tuple[int, int, int], width_pt: float | None) -> str:
    """A layer's name for strokes of one colour and weight ("red FF0000 · 0.35 mm"), or for fills (width None)."""
    name = min(_NAMES, key=lambda n: sum((a - b) ** 2 for a, b in zip(_NAMES[n], rgb, strict=True)))
    colour = f"{name} {rgb[0]:02X}{rgb[1]:02X}{rgb[2]:02X}"
    if width_pt is None:
        return f"{colour} · fill"
    return f"{colour} · {width_pt * MM_PER_POINT:.2f} mm" if width_pt > 0 else f"{colour} · hairline"


# -- reading the page ----------------------------------------------------------------------------------------------


@dataclass
class _Stroke:
    """One path object of the page, or one word: its parts as chains of points in page space."""

    kind: int
    layer: str
    colour: int  # 0xRRGGBB
    parts: list[list[Point]]
    extent: tuple[float, float, float, float]
    closed: bool = False  # every part closed
    curved: bool = False
    text: str = ""
    height: float = 0.0
    rotation: float = 0.0
    anchor: Point = (0.0, 0.0)
    flags: int = 0
    letters: list[int] = field(default_factory=list)  # a word's strokes, by their place on the page


def _compose(inner: tuple, outer: tuple) -> tuple:
    """The matrix that applies `inner`, then `outer` (a, b, c, d, e, f as PDF writes them)."""
    a, b, c, d, e, f = inner
    A, B, C, D, E, F = outer
    return (a * A + b * C, a * B + b * D, c * A + d * C, c * B + d * D, e * A + f * C + E, e * B + f * D + F)


def _matrix(obj) -> tuple:
    m = raw.FS_MATRIX()
    raw.FPDFPageObj_GetMatrix(obj, ctypes.byref(m))
    return (m.a, m.b, m.c, m.d, m.e, m.f)


def _rgb(getter, obj) -> tuple[int, int, int] | None:
    r, g, b, a = ctypes.c_uint(), ctypes.c_uint(), ctypes.c_uint(), ctypes.c_uint()
    if not getter(obj, ctypes.byref(r), ctypes.byref(g), ctypes.byref(b), ctypes.byref(a)):
        return None
    return (r.value, g.value, b.value)


def _utf16(buffer, size: int) -> str:
    return bytes(buffer)[:size].decode("utf-16-le", errors="replace").rstrip("\x00")


def _layer(obj) -> str | None:
    """The PDF layer (optional content group) the object is drawn on, when the PDF keeps its layers."""
    size = ctypes.c_ulong()
    for m in range(raw.FPDFPageObj_CountMarks(obj)):
        mark = raw.FPDFPageObj_GetMark(obj, m)
        raw.FPDFPageObjMark_GetName(mark, None, 0, ctypes.byref(size))
        buffer = (ctypes.c_ushort * (size.value // 2 + 1))()
        raw.FPDFPageObjMark_GetName(mark, buffer, size.value, ctypes.byref(size))
        if _utf16(buffer, size.value) != "OC":
            continue
        if raw.FPDFPageObjMark_GetParamStringValue(mark, b"Name", None, 0, ctypes.byref(size)) and size.value > 2:
            buffer = (ctypes.c_ushort * (size.value // 2 + 1))()
            raw.FPDFPageObjMark_GetParamStringValue(mark, b"Name", buffer, size.value, ctypes.byref(size))
            name = _utf16(buffer, size.value).strip()
            if name:
                return name
    return None


@lru_cache(maxsize=128)
def _weights(n: int) -> tuple[tuple[float, float, float, float], ...]:
    """Bernstein weights for n points along a cubic curve, after its start."""
    out = []
    for k in range(1, n + 1):
        t = k / n
        u = 1 - t
        out.append((u * u * u, 3 * u * u * t, 3 * u * t * t, t * t * t))
    return tuple(out)


def _flatten(p0: Point, p1: Point, p2: Point, p3: Point) -> list[Point]:
    """A cubic curve as straight pieces, none straying more than CURVE from it."""
    bend = max(
        math.hypot(p0[0] - 2 * p1[0] + p2[0], p0[1] - 2 * p1[1] + p2[1]),
        math.hypot(p1[0] - 2 * p2[0] + p3[0], p1[1] - 2 * p2[1] + p3[1]),
    )
    n = min(max(math.ceil(math.sqrt(0.75 * bend / CURVE)), 1), 128)
    return [
        (a * p0[0] + b * p1[0] + c * p2[0] + d * p3[0], a * p0[1] + b * p1[1] + c * p2[1] + d * p3[1])
        for a, b, c, d in _weights(n)
    ]


_MOVE, _BEZIER = raw.FPDF_SEGMENT_MOVETO, raw.FPDF_SEGMENT_BEZIERTO


def _path(obj, matrix: tuple) -> tuple[list[list[Point]], list[bool], bool]:
    """A path's parts in page space, whether each is closed, and whether it has curves."""
    a, b, c, d, e, f = matrix
    x, y = ctypes.c_float(), ctypes.c_float()
    px, py = ctypes.byref(x), ctypes.byref(y)
    segment_at, point_of = raw.FPDFPath_GetPathSegment, raw.FPDFPathSegment_GetPoint
    type_of, closes = raw.FPDFPathSegment_GetType, raw.FPDFPathSegment_GetClose
    parts: list[list[Point]] = []
    closed: list[bool] = []
    curved = False
    pending: list[Point] = []
    for index in range(raw.FPDFPath_CountSegments(obj)):
        segment = segment_at(obj, index)
        point_of(segment, px, py)
        X, Y = x.value, y.value
        point = (a * X + c * Y + e, b * X + d * Y + f)
        kind = type_of(segment)
        if kind == _MOVE or not parts:
            parts.append([point])
            closed.append(False)
        elif kind == _BEZIER:
            curved = True
            pending.append(point)
            if len(pending) == 3:
                parts[-1].extend(_flatten(parts[-1][-1], *pending))
                pending = []
        else:
            parts[-1].append(point)
        if closes(segment):
            closed[-1] = True
    for i, part in enumerate(parts):
        if len(part) > 2 and math.dist(part[0], part[-1]) <= JOIN:
            part.pop()
            closed[i] = True
    return parts, closed, curved


def _clip(obj, matrix: tuple) -> list[np.ndarray]:
    """The outlines an object is clipped to, in page space; none when it isn't clipped."""
    clip = raw.FPDFPageObj_GetClipPath(obj)
    if not clip:
        return []
    a, b, c, d, e, f = matrix
    x, y = ctypes.c_float(), ctypes.c_float()
    outlines = []
    for p in range(raw.FPDFClipPath_CountPaths(clip)):
        ring: list[Point] = []
        pending: list[Point] = []
        for s in range(raw.FPDFClipPath_CountPathSegments(clip, p)):
            segment = raw.FPDFClipPath_GetPathSegment(clip, p, s)
            raw.FPDFPathSegment_GetPoint(segment, ctypes.byref(x), ctypes.byref(y))
            point = (a * x.value + c * y.value + e, b * x.value + d * y.value + f)
            if raw.FPDFPathSegment_GetType(segment) == _BEZIER and ring:
                pending.append(point)
                if len(pending) == 3:
                    ring.extend(_flatten(ring[-1], *pending))
                    pending = []
            else:
                ring.append(point)
        if len(ring) >= 3:
            outlines.append(np.array(ring))
    return outlines


def _clipped(parts: list[list[Point]], closed: list[bool], outlines: list[np.ndarray]) -> list[list[Point]]:
    """What of the parts shows inside every clip outline, as open chains."""
    kept: list[list[Point]] = []
    for part, shut in zip(parts, closed, strict=True):
        points = np.array(part + [part[0]] if shut else part, dtype=float)
        if len(points) < 2:
            continue
        a, b = points[:-1], points[1:]
        cuts = [np.zeros(len(a)), np.ones(len(a))]
        for ring in outlines:
            for p, q in zip(ring, np.roll(ring, -1, axis=0), strict=True):
                r, s = b - a, q - p
                denominator = r[:, 0] * s[1] - r[:, 1] * s[0]
                with np.errstate(divide="ignore", invalid="ignore"):
                    t = ((p[0] - a[:, 0]) * s[1] - (p[1] - a[:, 1]) * s[0]) / denominator
                    u = ((p[0] - a[:, 0]) * r[:, 1] - (p[1] - a[:, 1]) * r[:, 0]) / denominator
                cuts.append(np.where((t > 0) & (t < 1) & (u >= 0) & (u <= 1), t, np.nan))
        ts = np.sort(np.column_stack(cuts), axis=1)
        chain: list[Point] = []
        for i in range(len(a)):
            row = ts[i][~np.isnan(ts[i])]
            for t0, t1 in zip(row, row[1:], strict=False):
                if t1 - t0 <= 1e-9:
                    continue
                middle = a[i] + (b[i] - a[i]) * ((t0 + t1) / 2)
                if not all(cad.inside(ring, middle)[0] for ring in outlines):
                    if len(chain) > 1:
                        kept.append(chain)
                    chain = []
                    continue
                start, end = tuple(a[i] + (b[i] - a[i]) * t0), tuple(a[i] + (b[i] - a[i]) * t1)
                if not chain or math.dist(chain[-1], start) > JOIN:
                    if len(chain) > 1:
                        kept.append(chain)
                    chain = [start]
                chain.append(end)
        if len(chain) > 1:
            kept.append(chain)
    return kept


def _walk(container, count, get, matrix: tuple, found: list, stats: dict, textpage) -> None:
    for index in range(count(container)):
        obj = get(container, index)
        kind = raw.FPDFPageObj_GetType(obj)
        own = _compose(_matrix(obj), matrix)
        if kind == raw.FPDF_PAGEOBJ_FORM:
            _walk(obj, raw.FPDFFormObj_CountObjects, raw.FPDFFormObj_GetObject, own, found, stats, textpage)
        elif kind == raw.FPDF_PAGEOBJ_PATH:
            _read_path(obj, own, _clip(obj, matrix), found, stats)
        elif kind == raw.FPDF_PAGEOBJ_TEXT:
            _read_text(obj, own, matrix, found, textpage)
        elif kind in (raw.FPDF_PAGEOBJ_IMAGE, raw.FPDF_PAGEOBJ_SHADING):
            stats["pictures"] += 1


def _extent(parts: list[list[Point]]) -> tuple[float, float, float, float]:
    xs = [p[0] for part in parts for p in part]
    ys = [p[1] for part in parts for p in part]
    return (min(xs), min(ys), max(xs), max(ys))


def _read_path(obj, matrix: tuple, outlines: list[np.ndarray], found: list, stats: dict) -> None:
    fill, stroke = ctypes.c_int(), ctypes.c_int()
    raw.FPDFPath_GetDrawMode(obj, ctypes.byref(fill), ctypes.byref(stroke))
    if not fill.value and not stroke.value:
        return  # a clipping path: it draws nothing
    stats["strokes"] += 1
    stroked = bool(stroke.value)
    rgb = _rgb(raw.FPDFPageObj_GetStrokeColor if stroked else raw.FPDFPageObj_GetFillColor, obj) or (0, 0, 0)
    if rgb == (255, 255, 255):
        return  # white doesn't print on the sheet: it masks what is under it
    parts, closed, curved = _path(obj, matrix)
    if outlines and parts and stroked and not fill.value:  # hatch lines cut to their boundary, and the like
        parts = _clipped(parts, closed, outlines)
        closed = [False] * len(parts)
    if not parts:
        return
    width = None
    if stroked:
        w = ctypes.c_float()
        raw.FPDFPageObj_GetStrokeWidth(obj, ctypes.byref(w))
        a, b, c, d = matrix[:4]
        width = round(w.value * math.sqrt(abs(a * d - b * c)), 3)  # in the path's own units: scaled to the page
    layer = _layer(obj) or pen_name(rgb, width)
    colour = (rgb[0] << 16) | (rgb[1] << 8) | rgb[2]
    extent = _extent(parts)
    if extent[2] - extent[0] <= JOIN and extent[3] - extent[1] <= JOIN:
        found.append(_Stroke(POINT, layer, colour, [parts[0][:1]], extent))
        return
    if fill.value and not stroked:
        found.append(_Stroke(HATCH, layer, colour, parts, extent, closed=True, curved=curved))
        return
    whole = all(closed)
    if whole and curved and len(parts) == 1 and _circle(parts[0], extent):
        kind = CIRCLE
    elif len(parts) == 1 and len(parts[0]) == 2 and not curved:
        kind = LINE
    else:
        kind = POLYLINE
    found.append(_Stroke(kind, layer, colour, parts, extent, closed=whole or bool(fill.value), curved=curved))


def _circle(ring: list[Point], extent: tuple) -> bool:
    cx, cy = (extent[0] + extent[2]) / 2, (extent[1] + extent[3]) / 2
    radii = [math.hypot(x - cx, y - cy) for x, y in ring]
    mean = sum(radii) / len(radii)
    return len(ring) >= 8 and max(radii) - min(radii) <= max(mean * 0.01, JOIN)


def _read_text(obj, matrix: tuple, outer: tuple, found: list, textpage) -> None:
    size = raw.FPDFTextObj_GetText(obj, textpage.raw, None, 0)
    buffer = (ctypes.c_ushort * (size // 2 + 1))()
    raw.FPDFTextObj_GetText(obj, textpage.raw, buffer, size)
    text = _utf16(buffer, size).strip()
    if not text:
        return
    font = ctypes.c_float()
    raw.FPDFTextObj_GetFontSize(obj, ctypes.byref(font))
    a, b, c, d, e, f = matrix
    left, bottom, right, top = ctypes.c_float(), ctypes.c_float(), ctypes.c_float(), ctypes.c_float()
    raw.FPDFPageObj_GetBounds(obj, ctypes.byref(left), ctypes.byref(bottom), ctypes.byref(right), ctypes.byref(top))
    A, B, C, D, E, F = outer  # the bounds are in the space the text sits in: a form's, or the page's
    xs, ys = (left.value, right.value), (bottom.value, top.value)
    placed = [(A * x + C * y + E, B * x + D * y + F) for x in xs for y in ys]
    rgb = _rgb(raw.FPDFPageObj_GetFillColor, obj) or (0, 0, 0)
    invisible = raw.FPDFTextObj_GetTextRenderMode(obj) == raw.FPDF_TEXTRENDERMODE_INVISIBLE
    found.append(
        _Stroke(
            TEXT,
            _layer(obj) or pen_name(rgb, None).replace("fill", "text"),
            (rgb[0] << 16) | (rgb[1] << 8) | rgb[2],
            [],
            _extent([placed]),
            text=text,
            height=font.value * math.hypot(c, d),
            rotation=math.degrees(math.atan2(b, a)),
            anchor=(e, f),
            flags=cad.INVISIBLE if invisible else 0,
        )
    )


def line_count(stored: Path, number: int) -> int:
    """How many strokes a PDF page draws, quickly: a drawing has thousands, a page of text a few."""
    return _line_count(str(stored), stored.stat().st_mtime, number)


@lru_cache(maxsize=256)
def _line_count(path: str, _stamp: float, number: int) -> int:
    def count(container, n, get) -> int:
        total = 0
        for index in range(n(container)):
            obj = get(container, index)
            kind = raw.FPDFPageObj_GetType(obj)
            if kind == raw.FPDF_PAGEOBJ_PATH:
                total += 1
            elif kind == raw.FPDF_PAGEOBJ_FORM:
                total += count(obj, raw.FPDFFormObj_CountObjects, raw.FPDFFormObj_GetObject)
        return total

    with PDFIUM:
        document = pdfium.PdfDocument(path)
        try:
            if not 1 <= number <= len(document):
                return 0
            page = document[number - 1]
            try:
                return count(page.raw, raw.FPDFPage_CountObjects, raw.FPDFPage_GetObject)
            finally:
                page.close()
        finally:
            document.close()


def _read(stored: Path, number: int) -> tuple[list[_Stroke], dict, tuple[float, float]]:
    with PDFIUM:
        try:
            document = pdfium.PdfDocument(stored)
        except pdfium.PdfiumError as error:
            raise Unreadable("This PDF can't be opened.") from error
        try:
            if not 1 <= number <= len(document):
                raise ValueError(f"The PDF has pages 1 to {len(document)}.")
            page = document[number - 1]
            textpage = page.get_textpage()
            try:
                size = page.get_size()
                found: list[_Stroke] = []
                stats = {"strokes": 0, "pictures": 0}
                identity = (1, 0, 0, 1, 0, 0)
                _walk(page.raw, raw.FPDFPage_CountObjects, raw.FPDFPage_GetObject, identity, found, stats, textpage)
            finally:
                textpage.close()
                page.close()
        finally:
            document.close()
    return found, stats, size


# -- putting objects back together -------------------------------------------------------------------------------


def _letters(found: list[_Stroke]) -> None:
    """Strokes that lie inside an invisible word's box draw its letters: they become the word's."""
    words = [i for i, s in enumerate(found) if s.kind == TEXT and s.flags & cad.INVISIBLE]
    strokes = [i for i, s in enumerate(found) if s.kind != TEXT]
    if not words or not strokes:
        return
    boxes = np.array([found[i].extent for i in words])
    pad = (boxes[:, 3] - boxes[:, 1]) * LETTER_PAD
    boxes += np.column_stack([-pad, -pad, pad, pad])
    extents = np.array([found[i].extent for i in strokes])
    order = np.argsort((boxes[:, 2] - boxes[:, 0]) * (boxes[:, 3] - boxes[:, 1]))  # the smallest word first
    taken = np.zeros(len(strokes), dtype=bool)
    for w in order:
        left, bottom, right, top = boxes[w]
        inside = ~taken & (extents[:, 0] >= left) & (extents[:, 1] >= bottom)
        inside &= (extents[:, 2] <= right) & (extents[:, 3] <= top)
        for s in np.flatnonzero(inside):
            found[strokes[s]].flags |= LETTERING
            found[words[w]].letters.append(strokes[s])
        taken |= inside


def _join(found: list[_Stroke]) -> list[list[int]]:
    """The page's objects as groups of strokes: open, single-part lines of one pen that meet end to end, where no
    third line meets them, are one group; every other stroke is a group of its own. A stroke taken backwards is
    written ~i. Groups keep the page's drawing order."""
    ends: dict[tuple, list[tuple[int, int]]] = {}
    keys: dict[int, tuple[tuple, tuple]] = {}
    for i, s in enumerate(found):
        if s.kind in (LINE, POLYLINE) and not s.flags & LETTERING and len(s.parts) == 1 and not s.closed:
            part = s.parts[0]
            first = (s.layer, round(part[0][0] / JOIN), round(part[0][1] / JOIN))
            last = (s.layer, round(part[-1][0] / JOIN), round(part[-1][1] / JOIN))
            keys[i] = (first, last)
            ends.setdefault(first, []).append((i, 0))
            ends.setdefault(last, []).append((i, 1))

    def partner(i: int, end: int) -> tuple[int, int] | None:
        at = ends[keys[i][end]]
        if len(at) != 2:
            return None  # a free end, or where three or more lines meet
        other = at[0] if at[1] == (i, end) else at[1]
        return None if other[0] == i else other

    used: set[int] = set()
    groups: dict[int, list[int]] = {}
    for i in keys:
        if i in used:
            continue
        start, start_end, seen = i, 0, {i}  # back to the chain's start, then along it
        while (p := partner(start, start_end)) is not None and p[0] not in seen:
            seen.add(p[0])
            start, start_end = p[0], 1 - p[1]
        chain, current, entry = [], start, start_end
        while True:
            chain.append(current if entry == 0 else ~current)
            used.add(current)
            p = partner(current, 1 - entry)
            if p is None or p[0] in used:
                break
            current, entry = p
        groups[min(c if c >= 0 else ~c for c in chain)] = chain
    return [groups[i] if i in keys else [i] for i in range(len(found)) if i not in keys or i in groups]


def _chain(found: list[_Stroke], group: list[int]) -> tuple[list[Point], bool]:
    points: list[Point] = []
    for c in group:
        part = found[c].parts[0] if c >= 0 else found[~c].parts[0][::-1]
        points.extend(part if not points else part[1:])
    if len(points) > 2 and math.dist(points[0], points[-1]) <= JOIN:
        return points[:-1], True
    return points, False


def _signed(ring: list[Point]) -> float:
    ox, oy = ring[0]
    total = 0.0
    for (x0, y0), (x1, y1) in zip(ring, ring[1:] + ring[:1], strict=True):
        total += (x0 - ox) * (y1 - oy) - (x1 - ox) * (y0 - oy)
    return total / 2


def _area(parts: list[list[Point]]) -> float:
    """What closed parts enclose, a part inside another cutting a hole in it (and one inside that, an island)."""
    rings = [p for p in parts if len(p) >= 3]
    if len(rings) <= 1 or len(rings) > MANY_RINGS:  # many pieces: holes run the other way round
        return abs(sum(_signed(r) for r in rings))
    arrays = [np.array(r) for r in rings]
    areas = [abs(_signed(r)) for r in rings]
    total = 0.0
    for i, ring in enumerate(arrays):
        depth = sum(1 for j, other in enumerate(arrays) if areas[j] > areas[i] and cad.inside(other, ring[:1])[0])
        total += areas[i] if depth % 2 == 0 else -areas[i]
    return abs(total)


def _length(parts: list[list[Point]], closed: bool) -> float:
    total = 0.0
    for part in parts:
        total += sum(map(math.dist, part, part[1:]))
        if closed and len(part) > 2:
            total += math.dist(part[-1], part[0])
    return total


# -- the folder --------------------------------------------------------------------------------------------------


def _current(out: Path) -> bool:
    try:
        info = json.loads((out / "drawing.json").read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return False
    return info.get("format") == cad.FORMAT and info.get("pdf") == FORMAT


def _lock(key: str) -> threading.Lock:
    with _locks_lock:
        return _locks.setdefault(key, threading.Lock())


def prepare(stored: Path, number: int) -> Path:
    """Read a PDF page's drawing into its folder, once; later calls find it there."""
    out = folder(stored, number)
    with _lock(str(out)):
        if _current(out):
            return out
        found, stats, (width, height) = _read(stored, number)
        _write(out, number, found, stats, width, height)
    return out


def page_drawing(stored: Path, number: int) -> cad.Drawing:
    return cad.load(prepare(stored, number))


def _write(out: Path, number: int, found: list[_Stroke], stats: dict, width: float, height: float) -> None:
    _letters(found)
    layers: dict[str, tuple[int, int]] = {}  # name: number and colour, in the order they come
    index, num, coords, colours, keys, texts, hatches = [], [], [], [], [], [], []
    nan = (math.nan, math.nan)

    def add(kind: int, stroke: _Stroke, parts: list[list[Point]], closed: bool, flags: int) -> int:
        i, start = len(index), len(coords)
        for n, part in enumerate(parts):
            if n:
                coords.append(nan)
            coords.extend(part)
        extent = _extent(parts) if parts else stroke.extent
        if kind == CIRCLE:
            cx, cy = (extent[0] + extent[2]) / 2, (extent[1] + extent[3]) / 2
            radius = sum(math.hypot(x - cx, y - cy) for x, y in parts[0]) / len(parts[0])
            length, area = 2 * math.pi * radius, math.pi * radius * radius
        elif kind in (TEXT, POINT):
            length, area = 0.0, 0.0
        else:
            length, area = _length(parts, closed), _area(parts) if closed else 0.0
        flags |= cad.CLOSED if closed else 0
        flags |= cad.APPROXIMATE if stroke.curved and kind != CIRCLE else 0
        if stroke.layer not in layers:
            layers[stroke.layer] = (len(layers), stroke.colour)
        index.append((number, kind, layers[stroke.layer][0], 0, start, len(coords) - start, flags, 0))
        num.append((*extent, length, area, 0.0))
        colours.append(stroke.colour)
        keys.append(f"p{number}.{i + 1}")
        return i

    for group in _join(found):
        first = found[group[0] if group[0] >= 0 else ~group[0]]
        if first.kind == TEXT:
            letters = [found[s] for s in first.letters]
            word = first
            if letters:  # the word takes its letters' pen, so hiding the pen hides its words too
                word = _Stroke(TEXT, letters[0].layer, letters[0].colour, [], first.extent)
            i = add(TEXT, word, [p for s in letters for p in s.parts], False, first.flags)
            texts.append([i, first.text, None, first.anchor[0], first.anchor[1], first.height, first.rotation, None])
        elif first.flags & LETTERING:
            continue  # drawn with its word
        elif len(group) > 1 or first.kind in (LINE, POLYLINE) and len(first.parts) == 1 and not first.closed:
            chain, closed = _chain(found, group)
            curved = any(found[c if c >= 0 else ~c].curved for c in group)
            kind = LINE if len(chain) == 2 and not closed and not curved else POLYLINE
            add(kind, first, [chain], closed, cad.APPROXIMATE if curved else 0)
        else:
            i = add(first.kind, first, first.parts, first.closed, 0)
            if first.kind == HATCH:
                hatches.append([i, "SOLID", True])
    names = list(layers)
    drawing = {
        "format": cad.FORMAT,
        "pdf": FORMAT,
        "reader": f"pdfium {pdfium.PDFIUM_INFO.build}",
        "version": "PDF",
        "units": {"insunits": 0, "measurement": 1},
        "dimlfac": 1.0,
        "dimscale": 1.0,
        "spaces": [
            {
                "number": number,
                "name": f"Page {number}",
                "block": None,
                "kind": "sheet",
                "extents": [0.0, 0.0, width, height],
                "objects": len(index),
            }
        ],
        "layers": [
            {"name": name, "off": False, "frozen": False, "locked": False, "linetype": "", "colour": f"#{colour:06X}"}
            for name, (_, colour) in layers.items()
        ],
        "blocks": [],
        "xrefs": [],
        "read": {
            "records": stats["strokes"],
            "decoded": stats["strokes"],
            "skipped": 0,
            "recovered": 0,
            "diagnostics": [],
            "not_read": {"pictures": stats["pictures"]} if stats["pictures"] else {},
            "clipped": 0,
        },
    }
    objects = {
        "types": TYPES,
        "layers": names,
        "blocks": [],
        "keys": keys,
        "texts": texts,
        "inserts": [],
        "dims": [],
        "tables": [],
        "hatches": hatches,
        "viewports": [],
    }
    staging = out.with_name(out.name + ".reading")
    shutil.rmtree(staging, ignore_errors=True)
    staging.mkdir(parents=True)
    (staging / "drawing.json").write_text(json.dumps(drawing), encoding="utf-8")
    (staging / "objects.json").write_text(json.dumps(objects, ensure_ascii=False), encoding="utf-8")
    np.array(index, dtype="<u4").reshape(-1, 8).tofile(staging / "index.bin")
    np.array(num, dtype="<f8").reshape(-1, 7).tofile(staging / "num.bin")
    np.array(coords, dtype="<f8").reshape(-1, 2).tofile(staging / "coords.bin")
    np.array(colours, dtype="<u4").tofile(staging / "colours.bin")
    shutil.rmtree(out, ignore_errors=True)
    staging.rename(out)
