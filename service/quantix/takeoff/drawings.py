"""What Quantix works out from CAD drawings (DWG and DXF): the evidence for a drawing's units, what each layer
holds, the rooms, and the checks that find what is missing and what disagrees. Everything here is computed from the
drawing's own objects; what a layer or block *is* comes only from the layer map the office proposes and the engineer
approves, never from a naming convention Quantix assumes."""

import re
from collections import Counter, defaultdict
from dataclasses import dataclass
from decimal import Decimal
from pathlib import Path
from typing import Any

import numpy as np
from sqlalchemy import select
from sqlalchemy.orm import Session

from quantix.core.review import APPROVED, LIVE
from quantix.documents import cad, library
from quantix.documents.models import Document
from quantix.takeoff.models import LayerMap, Measurement, Scale

# What a layer or block can be, for the layer map
MEANINGS = {
    "walls": "Walls",
    "columns": "Columns",
    "structure": "Structure (beams, slabs, footings)",
    "doors": "Doors",
    "windows": "Windows and glazing",
    "room_boundary": "Room outlines",
    "room_label": "Room names",
    "floor_finish": "Floor finishes",
    "wall_finish": "Wall finishes and cladding",
    "ceiling": "Ceilings",
    "skirting": "Skirting",
    "sanitary": "Sanitary fittings",
    "fixtures": "Fixtures, equipment and lighting",
    "furniture": "Furniture",
    "services": "Services: pipes, ducts, cables",
    "fire_rated": "Fire-rated walls and floors",
    "stairs": "Stairs and ramps",
    "external_works": "External works and paving",
    "landscape": "Landscape",
    "grid": "Grid lines and grid labels",
    "levels": "Levels",
    "dimensions": "Dimensions",
    "annotation": "Notes and annotation",
    "title_block": "Title block",
    "hatching": "Hatching that shows a material",
    "ignore": "Not needed for takeoff",
}
# What is work to price: drawn work nothing measures is a gap to check against the BOQ
PRICED = {
    "walls",
    "columns",
    "structure",
    "doors",
    "windows",
    "floor_finish",
    "wall_finish",
    "ceiling",
    "skirting",
    "sanitary",
    "fixtures",
    "services",
    "stairs",
    "external_works",
    "landscape",
}
WORDS = {"Text", "MText", "Attribute", "Dimension", "Table", "MultiLeader", "Leader"}
MIN_ROOM_M2 = 1.0  # a closed region smaller than this is not a room
MIN_ROOM_WIDTH_M = 0.5  # nor one thinner than this: the space inside a wall between its faces
WALL_SPACING_M = (0.05, 0.6)  # plausible distances between the two faces of a wall
DIMENSION_SLACK = Decimal("0.005")  # a written dimension more than 0.5% from the drawn length disagrees with it
MAX_SEGMENTS = 200_000  # linework sent to find rooms


def drawing_document(session: Session, tender_id: str, document_id: str) -> Document:
    document = session.get(Document, document_id)
    if document is None or document.tender_id != tender_id:
        raise ValueError("No document has that id. Use list_documents to find its id.")
    if document.kind != "cad":
        raise ValueError(f"{document.name} isn't a CAD drawing (DWG or DXF): read it with read_page.")
    if document.status in ("waiting", "reading"):
        raise ValueError(f"{document.name} is still being read.")
    if document.status not in ("read", "replaced"):
        raise ValueError(f"{document.name} couldn't be read: {document.note}")
    return document


def open_drawing(home: Path, document: Document) -> cad.Drawing:
    return cad.drawing(library.stored_file(home, document))


def unit_name(metres: float) -> str:
    for name, value in cad.UNIT_NAMES.items():
        if abs(value - metres) <= value * 1e-9:
            return name
    return f"{metres:g} metres to the unit"


def units_record(session: Session, document_id: str) -> Scale | None:
    """A drawing's units: the newest approved record, or else the newest still being decided."""
    query = select(Scale).where(Scale.document_id == document_id, Scale.page == 1, Scale.status.in_(LIVE))
    found = list(session.scalars(query.order_by(Scale.created_at.desc())))
    return next((s for s in found if s.status in APPROVED), found[0] if found else None)


def metres_per_unit(session: Session, document_id: str) -> float | None:
    record = units_record(session, document_id)
    return record.metres_per_point if record else None


def units_evidence(d: cad.Drawing) -> list[str]:
    """What in the drawing says what its units are, for whoever proposes or checks them."""
    lines = []
    header = d.units
    if header:
        lines.append(f"The drawing's header says its units are {header[0]}.")
    else:
        lines.append("The drawing's header doesn't say its units.")
    model = d.spaces[0] if d.spaces else None
    if model and model.extents:
        for name, metres in [header] if header else [("millimetres", 0.001), ("metres", 1.0)]:
            lines.append(
                f"In {name}, model space spans {model.width * metres:,.1f} m × {model.height * metres:,.1f} m."
            )
    notes = [
        line
        for s in d.spaces
        for line in d.page_text(s.number).splitlines()
        if re.search(r"\b(millimet|centimet|metres?|meters?|mm)\b", line, re.IGNORECASE) and len(line) < 200
    ]
    for line in notes[:3]:
        lines.append(f"A note on the drawing: “{line}”.")
    return lines


# -- the layer map ---------------------------------------------------------------------------------------------


@dataclass
class Meanings:
    layers: dict[str, str]
    blocks: dict[str, str]
    approved: bool  # every entry comes from an approved map

    def layers_for(self, *meanings: str) -> list[str]:
        return [name for name, m in self.layers.items() if m in meanings]

    def blocks_for(self, *meanings: str) -> list[str]:
        return [name for name, m in self.blocks.items() if m in meanings]


def layer_maps(session: Session, tender_id: str) -> list[LayerMap]:
    query = select(LayerMap).where(LayerMap.tender_id == tender_id, LayerMap.status.in_(LIVE))
    return list(session.scalars(query.order_by(LayerMap.created_at)))


def meanings(session: Session, tender_id: str) -> Meanings:
    """What the tender's layers and blocks are: name by name, from the newest map that names them, an approved map
    before one still being decided."""
    maps = sorted(layer_maps(session, tender_id), key=lambda m: (m.status in APPROVED, m.created_at))
    layers: dict[str, str] = {}
    blocks: dict[str, str] = {}
    for m in maps:
        layers |= {k.lower(): v for k, v in m.layers.items()}
        blocks |= {k.lower(): v for k, v in m.blocks.items()}
    return Meanings(layers, blocks, bool(maps) and all(m.status in APPROVED for m in maps))


def meaning_of(names: dict[str, str], name: str | None) -> str | None:
    if not name:
        return None
    lowered = name.lower()
    return names.get(lowered) or names.get(lowered.rsplit("$", 1)[-1])


def _straight_segments(d: cad.Drawing, found: np.ndarray) -> np.ndarray:
    """The straight pieces of the objects' chains, as rows of x1, y1, x2, y2."""
    rows = []
    for i in found:
        closed = d.flags(int(i)) & cad.CLOSED
        for part in d.parts(int(i)):
            if len(part) < 2:
                continue
            ring = np.vstack([part, part[:1]]) if closed and len(part) > 2 else part
            rows.append(np.hstack([ring[:-1], ring[1:]]))
    return np.vstack(rows) if rows else np.zeros((0, 4))


def wall_spacing(d: cad.Drawing, found: np.ndarray, sample: int = 1500) -> tuple[float, float] | None:
    """The commonest distance between parallel, overlapping lines, and the share of pairs at it: the thickness of
    walls drawn as two faces."""
    segments = _straight_segments(d, found)
    if len(segments) < 2:
        return None
    lengths = np.hypot(segments[:, 2] - segments[:, 0], segments[:, 3] - segments[:, 1])
    segments = segments[lengths > 0]
    lengths = lengths[lengths > 0]
    if len(segments) > sample:
        segments = segments[np.argsort(-lengths)[:sample]]
        lengths = np.sort(lengths)[::-1][:sample]
    direction = (segments[:, 2:] - segments[:, :2]) / lengths[:, None]
    direction[direction[:, 0] < 0] *= -1
    parallel = (
        np.abs(
            direction[:, 0][:, None] * direction[:, 1][None, :] - direction[:, 1][:, None] * direction[:, 0][None, :]
        )
        < 0.02
    )
    np.fill_diagonal(parallel, False)
    middle = (segments[:, :2] + segments[:, 2:]) / 2
    offset = middle[None, :, :] - segments[:, None, :2]
    distance = np.abs(offset[:, :, 0] * direction[:, None, 1] - offset[:, :, 1] * direction[:, None, 0])
    along = offset[:, :, 0] * direction[:, None, 0] + offset[:, :, 1] * direction[:, None, 1]
    overlapping = (along > 0) & (along < lengths[:, None])
    pairs = distance[parallel & overlapping & (distance > 0)]
    if len(pairs) == 0:
        return None
    step = max(float(np.median(lengths)) / 1000, 1e-9)
    rounded = np.round(pairs / step) * step
    value, count = Counter(rounded.round(9).tolist()).most_common(1)[0]
    return float(value), count / len(pairs)


@dataclass
class LayerFacts:
    name: str
    objects: int
    types: Counter
    length: float
    closed: int
    closed_area_median: float
    blocks: Counter
    texts: list[str]
    hatch_patterns: Counter
    off: bool
    frozen: bool


def layer_facts(d: cad.Drawing, page: int) -> list[LayerFacts]:
    """One row of facts per layer used in a space, largest first."""
    on_page = d.select(page, cad.Rule())
    by_layer: dict[int, list[int]] = defaultdict(list)
    for i in on_page:
        by_layer[int(d.index[i, 2])].append(int(i))
    rows = []
    for layer_id, found in by_layer.items():
        name = d.layer_names[layer_id]
        found_array = np.array(found)
        closed = [i for i in found if d.flags(i) & cad.CLOSED and d.area(i) > 0]
        info = d.layers.get(name, {})
        rows.append(
            LayerFacts(
                name=name,
                objects=len(found),
                types=Counter(d.type_of(i) for i in found),
                length=float(d.num[found_array, 4].sum()),
                closed=len(closed),
                closed_area_median=float(np.median([d.area(i) for i in closed])) if closed else 0.0,
                blocks=Counter(d.placed[i].block for i in found if i in d.placed),
                texts=[d.texts[i].text.replace("\n", " ")[:40] for i in found if i in d.texts][:4],
                hatch_patterns=Counter(d.hatches[i][0] for i in found if i in d.hatches),
                off=bool(info.get("off")),
                frozen=bool(info.get("frozen")),
            )
        )
    return sorted(rows, key=lambda r: -r.objects)


def block_counts(d: cad.Drawing, page: int) -> Counter:
    """Placed copies of each block in a space, wherever they sit (a MINSERT counts each copy)."""
    counts: Counter = Counter()
    for i, placed in d.placed.items():
        if d.index[i, 0] == page and not d.flags(i) & (cad.ANNOTATION | cad.DERIVED):
            counts[placed.block] += placed.copies
    return counts


# -- rooms -----------------------------------------------------------------------------------------------------


@dataclass
class Room:
    number: int
    name: str
    ring: np.ndarray
    area: float  # drawing units squared
    perimeter: float  # drawing units
    source: str  # outline | walls


def _circle_centre(a: np.ndarray, b: np.ndarray, c: np.ndarray) -> np.ndarray | None:
    d = 2 * (a[0] * (b[1] - c[1]) + b[0] * (c[1] - a[1]) + c[0] * (a[1] - b[1]))
    if abs(d) < 1e-12:
        return None
    sa, sb, sc = a @ a, b @ b, c @ c
    return np.array(
        [
            (sa * (b[1] - c[1]) + sb * (c[1] - a[1]) + sc * (a[1] - b[1])) / d,
            (sa * (c[0] - b[0]) + sb * (a[0] - c[0]) + sc * (b[0] - a[0])) / d,
        ]
    )


_rooms_found: dict[tuple, list["Room"]] = {}


def rooms(session: Session, home: Path, document: Document, page: int = 1) -> list[Room]:
    """The rooms of a space: closed outlines on the room-outline layers, or else the regions the walls, columns,
    doors and windows close off. Each takes the room names printed inside it. Needs the layer map."""
    known = meanings(session, document.tender_id)
    d = open_drawing(home, document)
    metres = metres_per_unit(session, document.id) or (d.units[1] if d.units else None)
    key = (str(d.folder), page, tuple(sorted(known.layers.items())), tuple(sorted(known.blocks.items())), metres)
    if key not in _rooms_found:
        if len(_rooms_found) > 32:
            _rooms_found.clear()
        _rooms_found[key] = _find_rooms(d, page, known, metres)
    return _rooms_found[key]


def _find_rooms(d: cad.Drawing, page: int, known: Meanings, metres: float | None) -> list[Room]:
    min_area = MIN_ROOM_M2 / metres**2 if metres else 0.0
    min_width = MIN_ROOM_WIDTH_M / metres if metres else 0.0
    rings: list[tuple[np.ndarray, str]] = []
    outline_layers = known.layers_for("room_boundary")
    if outline_layers:
        for i in d.select(page, cad.Rule(layers=outline_layers, closed=True)):
            parts = d.parts(int(i))
            if parts and cad.ring_area(parts[0]) >= min_area:
                rings.append((parts[0], "outline"))
    enclosing = known.layers_for("walls", "fire_rated", "columns", "doors", "windows")
    enclosing_blocks = known.blocks_for("doors", "windows", "columns")
    if not rings and (enclosing or enclosing_blocks):
        segments = _enclosing_segments(d, page, known)
        tolerance = 0.001 / metres if metres else 1e-6
        for ring in cad.faces(segments, tolerance):
            area, perimeter = cad.ring_area(ring), cad.ring_length(ring)
            if area >= min_area and perimeter and 2 * area / perimeter >= min_width:
                rings.append((ring, "walls"))
    label_layers = known.layers_for("room_label")
    if label_layers:
        texts = [(i, t) for i, t in d.words(page) if meaning_of(known.layers, d.layer_of(i)) == "room_label"]
    else:  # no layer is mapped as room names: take words, not levels, numbers or tags
        texts = [(i, t) for i, t in d.words(page) if len(re.findall(r"[^\W\d_]", t.text)) >= 3]
    anchors = np.array([[t.x, t.y] for _, t in texts]) if texts else np.zeros((0, 2))
    order = sorted(range(len(rings)), key=lambda k: cad.ring_area(rings[k][0]))
    taken: set[int] = set()
    names: dict[int, list[str]] = defaultdict(list)
    for k in order:
        if not len(anchors):
            break
        hits = np.flatnonzero(cad.inside(rings[k][0], anchors))
        for h in hits:
            if int(h) not in taken:
                taken.add(int(h))
                names[k].append(" ".join(texts[int(h)][1].text.split()))
    found = []
    for k, (ring, source) in enumerate(rings):
        name = " / ".join(names[k]) if names[k] else f"Unnamed space {k + 1}"
        found.append(Room(k + 1, name, ring, cad.ring_area(ring), cad.ring_length(ring), source))
    return found


def _enclosing_segments(d: cad.Drawing, page: int, known: Meanings) -> list[tuple[float, float, float, float]]:
    """The linework that closes rooms off: walls, columns and windows. A door's own leaf and swing are left out, or
    they would cut the swing out of the room it opens into; instead each swing closes its opening with the radius
    that isn't the leaf: the line from the hinge to the far jamb."""
    chosen: set[int] = set()
    layers = known.layers_for("walls", "fire_rated", "columns", "windows")
    if layers:
        chosen |= {int(i) for i in d.select(page, cad.Rule(layers=layers))}
    blocks = known.blocks_for("windows", "columns")
    if blocks:
        for insert in d.select(page, cad.Rule(blocks=blocks)):
            chosen |= _inside_insert(d, int(insert))
    derived = np.flatnonzero((d.index[:, 0] == page) & ((d.index[:, 6] & cad.DERIVED) != 0))
    chosen |= {
        int(i) for i in derived if meaning_of(known.layers, d.layer_of(int(i))) in ("walls", "fire_rated", "columns")
    }
    doors: set[int] = set()
    door_layers = known.layers_for("doors")
    if door_layers:
        doors |= {int(i) for i in d.select(page, cad.Rule(layers=door_layers))}
    door_blocks = known.blocks_for("doors")
    if door_blocks:
        for insert in d.select(page, cad.Rule(blocks=door_blocks)):
            doors |= _inside_insert(d, int(insert))
    chosen -= doors
    rows = _straight_segments(d, np.array(sorted(chosen), dtype=int))
    segments = [tuple(map(float, r)) for r in rows[:MAX_SEGMENTS]]
    by_parent: dict[int | None, list[int]] = defaultdict(list)
    for i in doors:
        by_parent[d.parent(i)].append(i)
    for siblings in by_parent.values():
        straight = _straight_segments(d, np.array([i for i in siblings if d.type_of(i) != "Arc"], dtype=int))
        for i in siblings:
            parts = d.parts(i)
            if d.type_of(i) != "Arc" or not parts or len(parts[0]) < 3:
                continue
            points = parts[0]
            centre = _circle_centre(points[0], points[len(points) // 2], points[-1])
            if centre is None:
                continue
            radius = float(np.hypot(*(points[0] - centre)))
            ends = [points[0], points[-1]]
            leaves = [end for end in ends if _drawn(straight, centre, end, radius * 0.02)]
            for end in ends:
                if len(leaves) != 1 or not np.array_equal(end, leaves[0]):
                    segments.append((float(centre[0]), float(centre[1]), float(end[0]), float(end[1])))
    return segments


def _drawn(segments: np.ndarray, a: np.ndarray, b: np.ndarray, within: float) -> bool:
    """Whether a straight line from a to b is drawn among the segments."""
    if not len(segments):
        return False
    forward = np.hypot(*(segments[:, :2] - a).T) + np.hypot(*(segments[:, 2:] - b).T)
    backward = np.hypot(*(segments[:, :2] - b).T) + np.hypot(*(segments[:, 2:] - a).T)
    return bool(np.min(np.minimum(forward, backward)) <= 2 * within)


def _inside_insert(d: cad.Drawing, insert: int) -> set[int]:
    """The objects placed inside a block reference: they follow it in the drawing's order."""
    found = set()
    i = insert + 1
    while i < len(d.index):
        parent = d.parent(i)
        chain = parent
        while chain is not None and chain != insert:
            chain = d.parent(chain)
        if chain != insert:
            break
        if not d.flags(i) & cad.ANNOTATION:
            found.add(i)
        i += 1
    return found


# -- choosing objects ------------------------------------------------------------------------------------------


def choose(
    session: Session, home: Path, document: Document, page: int, rule: cad.Rule
) -> tuple[np.ndarray, list[Room]]:
    """The objects a rule takes, and the rooms when it takes rooms."""
    d = open_drawing(home, document)
    d.space(page)
    wanted_rooms = [r.strip().lower() for r in rule.rooms if r.strip()]
    takes_rooms = [t.lower() for t in rule.types] == ["room"]
    if not wanted_rooms and not takes_rooms:
        return d.select(page, rule), []
    found_rooms = rooms(session, home, document, page)
    if wanted_rooms:
        found_rooms = [r for r in found_rooms if any(w in r.name.lower() for w in wanted_rooms)]
        if not found_rooms:
            raise ValueError(
                "No room has that name. Rooms come from the layer map's room outlines, or from its walls, doors and "
                "windows: list them with query_drawing and types ['Room']."
            )
    if takes_rooms:
        return np.zeros(0, dtype=int), found_rooms
    base = d.select(page, rule.model_copy(update={"rooms": []}))
    if not len(base):
        return base, found_rooms
    centres = np.column_stack([(d.num[base, 0] + d.num[base, 2]) / 2, (d.num[base, 1] + d.num[base, 3]) / 2])
    keep = np.zeros(len(base), dtype=bool)
    for room in found_rooms:
        keep |= cad.inside(room.ring, centres)
    return base[keep], found_rooms


def room_key(room: Room) -> str:
    return f"room:{room.name}"


# -- the drawing's own checks ----------------------------------------------------------------------------------


@dataclass
class Problem:
    key: str
    severity: str  # blocker | warning
    message: str
    page: int
    objects: list[str]  # keys of the objects it is about, to show them
    document_id: str | None = None  # where it shows


def drawing_problems(session: Session, home: Path, document: Document) -> list[Problem]:
    """What Quantix finds in a drawing by itself: what couldn't be read, what can't be trusted for takeoff, written
    dimensions that disagree with the drawing, and drawn work nothing measures yet."""
    d = open_drawing(home, document)
    found: list[Problem] = []
    tag = document.id[:8]
    read = d.info["read"]
    if not d.units and units_record(session, document.id) is None:
        found.append(
            Problem(
                f"drawing-units:{tag}",
                "warning",
                f"{document.name} doesn't say its units: nothing measured on it has a quantity until its units are "
                "set.",
                1,
                [],
            )
        )
    for xref in d.info["xrefs"]:
        if not xref["loaded"]:
            found.append(
                Problem(
                    f"drawing-xref:{tag}:{xref['name']}",
                    "warning",
                    f"{document.name} refers to another drawing, {xref['path'] or xref['name']}, that isn't in it: "
                    "what that drawing shows isn't read. Add that drawing to the package.",
                    1,
                    [],
                )
            )
    for what, n in read["not_read"].items():
        if what == "construction lines":
            continue
        found.append(
            Problem(f"drawing-not-read:{tag}:{what}", "warning", f"{n} {what} in {document.name} aren't read.", 1, [])
        )
    if read["records"] and read["decoded"] < read["records"]:
        missing = read["records"] - read["decoded"]
        found.append(
            Problem(
                f"drawing-records:{tag}",
                "warning",
                f"{missing} of the {read['records']:,} records in {document.name} couldn't be decoded.",
                1,
                [],
            )
        )
    for space in d.spaces:
        found += _hidden_layers(d, space, document, tag)
        found += _drawn_twice(d, space, document, tag)
        found += _written_dimensions(d, space, document, tag)
    found += _unmeasured(session, home, document, d, tag)
    found += _room_names(session, home, document, d, tag)
    found += _crossings(session, document, d, tag)
    for problem in found:
        problem.document_id = document.id
    return found


def _hidden_layers(d: cad.Drawing, space: cad.Space, document: Document, tag: str) -> list[Problem]:
    hidden = {name for name, layer in d.layers.items() if layer.get("off") or layer.get("frozen")}
    if not hidden:
        return []
    counts = Counter(d.layer_of(int(i)) for i in d.select(space.number, cad.Rule()) if d.layer_of(int(i)) in hidden)
    if not counts:
        return []
    names = ", ".join(f"{name} ({n})" for name, n in counts.most_common(8))
    return [
        Problem(
            f"drawing-hidden:{tag}:{space.number}",
            "warning",
            f"Layers that are off or frozen in {document.name}, {space.label}, still hold objects: {names}. They don't "
            "print, so check whether their work belongs to the tender.",
            space.number,
            [],
        )
    ]


def _drawn_twice(d: cad.Drawing, space: cad.Space, document: Document, tag: str) -> list[Problem]:
    lines = d.select(space.number, cad.Rule(types=["Line"]))
    seen: dict[tuple, int] = {}
    twice: dict[str, list[str]] = defaultdict(list)
    for i in lines:
        part = d.parts(int(i))
        if not part or len(part[0]) != 2:
            continue
        a, b = sorted([tuple(np.round(part[0][0], 3)), tuple(np.round(part[0][1], 3))])
        key = (int(d.index[i, 2]), a, b)
        if key in seen:
            twice[d.layer_of(int(i))].append(d.keys[int(i)])
        else:
            seen[key] = int(i)
    found = []
    for layer, keys in sorted(twice.items(), key=lambda kv: -len(kv[1]))[:5]:
        found.append(
            Problem(
                f"drawing-twice:{tag}:{space.number}:{layer}",
                "warning",
                f"{len(keys)} lines on layer {layer} in {document.name}, {space.label}, are drawn twice over others: "
                "a length taken by layer counts them twice.",
                space.number,
                keys[:50],
            )
        )
    return found


def _written_dimensions(d: cad.Drawing, space: cad.Space, document: Document, tag: str) -> list[Problem]:
    dimlfac = Decimal(str(d.info.get("dimlfac") or 1.0))
    wrong = []
    for i, dim in d.dimensions.items():
        if d.index[i, 0] != space.number or not dim.override or dim.kind.lower().startswith("angular"):
            continue
        written = [n for n in re.findall(r"\d+(?:[.,]\d+)?", dim.shown.replace(",", ""))]
        if len(written) != 1:
            continue
        value, drawn = Decimal(written[0]), Decimal(str(dim.measured)) * dimlfac
        if drawn == 0:
            continue
        if abs(value - drawn) > max(abs(drawn) * DIMENSION_SLACK, Decimal(1)):
            wrong.append((i, dim.shown, drawn))
    if not wrong:
        return []
    listed = "; ".join(f"“{shown}” where the drawing measures {drawn:,.0f}" for _, shown, drawn in wrong[:8])
    return [
        Problem(
            f"drawing-dimensions:{tag}:{space.number}",
            "warning",
            f"{len(wrong)} dimension{'s' if len(wrong) > 1 else ''} in {document.name}, {space.label}, "
            f"{'are' if len(wrong) > 1 else 'is'} written by hand and disagree with the drawn length: {listed}. "
            "Written dimensions usually govern: take off from the written size or raise a query.",
            space.number,
            [d.keys[i] for i, _, _ in wrong[:50]],
        )
    ]


def _measured_keys(session: Session, document: Document) -> set[str]:
    query = select(Measurement.entities).where(
        Measurement.document_id == document.id, Measurement.status.in_(LIVE), Measurement.entities.is_not(None)
    )
    return {k for keys in session.scalars(query) for k in keys or []}


def _unmeasured(session: Session, home: Path, document: Document, d: cad.Drawing, tag: str) -> list[Problem]:
    """Drawn work the layer map says is work, which no measurement takes: each is a gap to check against the BOQ."""
    known = meanings(session, document.tender_id)
    if not known.layers and not known.blocks:
        return []
    measured = _measured_keys(session, document)
    found = []
    for space in d.spaces:
        if space.kind != "model":
            continue
        on_page = d.select(space.number, cad.Rule())
        by_meaning: dict[tuple[str, str], list[int]] = defaultdict(list)
        for i in on_page:
            i = int(i)
            if d.type_of(i) in WORDS:
                continue  # words say what the work is; they aren't work themselves
            placed = d.placed.get(i)
            meaning = meaning_of(known.blocks, placed.block) if placed else None
            name = placed.block if meaning else None
            if not meaning and not _in_mapped_block(d, i, known):
                meaning, name = meaning_of(known.layers, d.layer_of(i)), d.layer_of(i)
            if meaning in PRICED:
                by_meaning[(meaning, name)].append(i)
        for (meaning, name), objects in sorted(by_meaning.items()):
            if any(d.keys[i] in measured for i in objects):
                continue
            count = sum(d.copies(i) for i in objects)
            found.append(
                Problem(
                    f"drawing-unmeasured:{tag}:{name}",
                    "warning",
                    f"{MEANINGS[meaning]} ({name}: {count} object{'s' if count > 1 else ''}) are drawn in "
                    f"{document.name}, but nothing measures them yet. Measure them against their BOQ line, or raise "
                    "a query if no BOQ line covers them.",
                    space.number,
                    [d.keys[i] for i in objects[:50]],
                )
            )
    return found


def _in_mapped_block(d: cad.Drawing, i: int, known: Meanings) -> bool:
    """Whether an object sits inside a block the layer map names: that block stands for it."""
    parent = d.parent(i)
    while parent is not None:
        placed = d.placed.get(parent)
        if placed is not None and meaning_of(known.blocks, placed.block):
            return True
        parent = d.parent(parent)
    return False


def _room_names(session: Session, home: Path, document: Document, d: cad.Drawing, tag: str) -> list[Problem]:
    known = meanings(session, document.tender_id)
    if not known.layers_for("room_label"):
        return []
    found_rooms = rooms(session, home, document, 1)
    named = {part for room in found_rooms for part in room.name.split(" / ")}
    loose = [
        t.text
        for i, t in d.words(1)
        if meaning_of(known.layers, d.layer_of(i)) == "room_label" and " ".join(t.text.split()) not in named
    ]
    if not loose:
        return []
    return [
        Problem(
            f"drawing-room-names:{tag}",
            "warning",
            f"{len(loose)} room name{'s are' if len(loose) > 1 else ' is'} in no closed room in {document.name}: "
            + ", ".join(f"“{n}”" for n in loose[:10])
            + ". Their outlines are open or the layer map misses a wall layer, so their areas can't be measured.",
            1,
            [],
        )
    ]


def _crossings(session: Session, document: Document, d: cad.Drawing, tag: str) -> list[Problem]:
    """Where pipes, ducts and cables cross fire-rated walls and floors: each needs a sleeve and fire stopping."""
    known = meanings(session, document.tender_id)
    services, fire = known.layers_for("services"), known.layers_for("fire_rated")
    if not services or not fire:
        return []
    a = _straight_segments(d, d.select(1, cad.Rule(layers=services)))
    b = _straight_segments(d, d.select(1, cad.Rule(layers=fire)))
    if not len(a) or not len(b):
        return []
    points = _intersections(a, b)
    if not len(points):
        return []
    metres = metres_per_unit(session, document.id) or (d.units[1] if d.units else None)
    # the two faces of a wall are one crossing: points closer than a thick wall are one place
    merge = WALL_SPACING_M[1] / metres if metres else 0.0
    places: list[np.ndarray] = []
    for p in points:
        if not any(np.hypot(*(p - q)) <= merge for q in places):
            places.append(p)
    return [
        Problem(
            f"drawing-crossings:{tag}:{len(places)}",
            "warning",
            f"Services cross fire-rated walls or floors at {len(places)} place{'s' if len(places) > 1 else ''} in "
            f"{document.name}: each needs a sleeve and fire stopping. Check the BOQ has them, or raise a query.",
            1,
            [],
        )
    ]


def _intersections(a: np.ndarray, b: np.ndarray, chunk: int = 2000) -> np.ndarray:
    """Where segments of `a` cross segments of `b`."""
    found = []
    for start in range(0, len(a), chunk):
        p = a[start : start + chunk]
        r = p[:, 2:] - p[:, :2]
        s = b[:, 2:] - b[:, :2]
        denominator = r[:, None, 0] * s[None, :, 1] - r[:, None, 1] * s[None, :, 0]
        qp = b[None, :, :2] - p[:, None, :2]
        with np.errstate(divide="ignore", invalid="ignore"):
            t = (qp[:, :, 0] * s[None, :, 1] - qp[:, :, 1] * s[None, :, 0]) / denominator
            u = (qp[:, :, 0] * r[:, None, 1] - qp[:, :, 1] * r[:, None, 0]) / denominator
        hit = (denominator != 0) & (t >= 0) & (t <= 1) & (u >= 0) & (u <= 1)
        rows, _ = np.nonzero(hit)
        found.append(p[rows, :2] + t[hit][:, None] * r[rows])
    return np.vstack(found) if found else np.zeros((0, 2))


# -- across drawings: grids ------------------------------------------------------------------------------------


def grid(session: Session, home: Path, document: Document) -> dict[str, tuple[str, float]]:
    """A drawing's grid: each label with its direction (x for a vertical line, y for a horizontal one) and where
    its line runs, from the layer map's grid layers."""
    known = meanings(session, document.tender_id)
    layers = known.layers_for("grid")
    if not layers:
        return {}
    d = open_drawing(home, document)
    model = d.spaces[0]
    if model.extents is None:
        return {}
    size = max(model.width or 0, model.height or 0)
    lines = []
    for i in d.select(1, cad.Rule(layers=layers, types=["Line", "Polyline"])):
        for part in d.parts(int(i)):
            if len(part) == 2 and np.hypot(*(part[1] - part[0])) >= size * 0.1:
                lines.append(part)
    labels = [
        (t.text.strip(), np.array([t.x, t.y]))
        for i, t in d.texts.items()
        if d.index[i, 0] == 1 and 1 <= len(t.text.strip()) <= 3 and meaning_of(known.layers, d.layer_of(i)) == "grid"
    ]
    found = {}
    for label, at in labels:
        best, where = None, None
        for part in lines:
            for end in part:
                gap = float(np.hypot(*(end - at)))
                if gap <= size * 0.03 and (best is None or gap < best):
                    dx, dy = abs(part[1][0] - part[0][0]), abs(part[1][1] - part[0][1])
                    best, where = gap, ("x", float(part[0][0])) if dx < dy else ("y", float(part[0][1]))
        if where:
            found[label] = where
    return found


def grid_problems(session: Session, home: Path, tender_id: str) -> list[Problem]:
    """Grids that disagree between drawings: a label one has and another hasn't, or different spacing."""
    drawings = [d for d in library.documents(session, tender_id) if d.kind == "cad" and d.status == "read"]
    grids = [(document, g) for document in drawings if (g := grid(session, home, document))]
    found = []
    for (a, ga), (b, gb) in zip(grids, grids[1:], strict=False):
        only_a, only_b = sorted(set(ga) - set(gb)), sorted(set(gb) - set(ga))
        if only_a or only_b:
            found.append(
                Problem(
                    f"grid-labels:{a.id[:8]}:{b.id[:8]}",
                    "warning",
                    f"The grids differ: {a.name} has {', '.join(only_a) or 'no other labels'}, {b.name} has "
                    f"{', '.join(only_b) or 'no other labels'}.",
                    1,
                    [],
                    a.id,
                )
            )
        shared = sorted(set(ga) & set(gb))
        for axis in ("x", "y"):
            labels = sorted((ga[k][1], k) for k in shared if ga[k][0] == axis and gb[k][0] == axis)
            for (pa, la), (pb, lb) in zip(labels, labels[1:], strict=False):
                span_a, span_b = pb - pa, gb[lb][1] - gb[la][1]
                if abs(span_a - span_b) > max(abs(span_a) * 0.001, 1.0):
                    found.append(
                        Problem(
                            f"grid-spacing:{a.id[:8]}:{b.id[:8]}:{la}:{lb}",
                            "warning",
                            f"Grid {la} to {lb} is {span_a:,.0f} in {a.name} but {span_b:,.0f} in {b.name}.",
                            1,
                            [],
                            a.id,
                        )
                    )
    return found


# -- the layer map's own checks --------------------------------------------------------------------------------


def map_problems(session: Session, home: Path, layer_map: LayerMap) -> list[tuple[str, str, str]]:
    """What the drawing says against the layer map: (key, severity, message)."""
    found = []
    drawings = [d for d in library.documents(session, layer_map.tender_id) if d.kind == "cad" and d.status == "read"]
    loaded = [(document, open_drawing(home, document)) for document in drawings]
    names = {n.lower() for _, d in loaded for n in d.layer_names} | {
        n.lower().rsplit("$", 1)[-1] for _, d in loaded for n in d.layer_names
    }
    blocks = {n.lower() for _, d in loaded for n in d.block_names}
    for layer in layer_map.layers:
        if layer.lower() not in names:
            found.append(
                (f"map-layer-missing:{layer_map.id}:{layer}", "blocker", f"No drawing has a layer called “{layer}”.")
            )
    for block in layer_map.blocks:
        if block.lower() not in blocks:
            found.append(
                (f"map-block-missing:{layer_map.id}:{block}", "blocker", f"No drawing has a block called “{block}”.")
            )
    for document, d in loaded:
        if document.id != layer_map.document_id:
            continue
        facts = {f.name.lower(): f for f in layer_facts(d, 1)}
        metres = metres_per_unit(session, document.id) or (d.units[1] if d.units else None)
        for layer, meaning in layer_map.layers.items():
            fact = facts.get(layer.lower())
            if fact is None:
                continue
            problem = None
            if meaning == "walls":
                objects = d.select(1, cad.Rule(layers=[layer]))
                spacing = wall_spacing(d, objects)
                low, high = WALL_SPACING_M
                if spacing is None or (metres and not low <= spacing[0] * metres <= high):
                    problem = "holds no pairs of parallel lines a wall's thickness apart"
            elif meaning == "room_boundary" and not fact.closed:
                problem = "has no closed outlines to take rooms from"
            elif meaning == "room_label" and not fact.texts:
                problem = "has no text"
            elif meaning == "doors" and not fact.types.get("Arc") and not fact.blocks:
                problem = "has no door swings or door blocks"
            if problem:
                found.append(
                    (
                        f"map-meaning:{layer_map.id}:{layer}",
                        "warning",
                        f"Layer “{layer}”, mapped as {MEANINGS[meaning].lower()}, {problem} in {document.name}.",
                    )
                )
    return found


# -- the BOQ's own checks --------------------------------------------------------------------------------------

_SUMS = re.compile(r"\bprovisional\s+sums?\b|\bprime\s+cost\b|\bp\.?\s?c\.?\s+sums?\b", re.IGNORECASE)
_KNOWN_UNITS = {"m", "m2", "m3", "nr", "kg", "t", "ton", "tonne", "item", "sum", "ls", "l.s", "lump sum", "no", "set"}


def boq_problems(session: Session, tender_id: str) -> list[Problem]:
    """What the BOQ itself shows: lines entered twice, sums to price as the tender says, lines with no quantity and
    units Quantix can't compare."""
    from quantix.boq import records as boq
    from quantix.takeoff import records as takeoff

    items = [i for i in boq.items(session, tender_id) if i.status in LIVE]
    found = []
    seen: dict[tuple, Any] = {}
    for item in items:
        words = " ".join(re.sub(r"[^\w\s]", " ", item.description.lower()).split())
        key = (item.section or "", words, takeoff.plain_unit(item.unit))
        if key in seen and words:
            other = seen[key]
            found.append(
                Problem(
                    f"boq-twice:{other.id[:8]}:{item.id[:8]}",
                    "warning",
                    f"{boq.reference(other)} and {boq.reference(item)} have the same description and unit: check the "
                    "work isn't billed twice.",
                    item.page,
                    [],
                    item.document_id,
                )
            )
        else:
            seen[key] = item
    sums = [i for i in items if _SUMS.search(i.description)]
    if sums:
        found.append(
            Problem(
                f"boq-sums:{tender_id[:8]}",
                "warning",
                f"{len(sums)} BOQ lines are provisional or prime cost sums ("
                + ", ".join(boq.reference(i) for i in sums[:8])
                + "): price them exactly as the tender says, with the stated markup only.",
                1,
                [],
            )
        )
    blank = [
        i
        for i in items
        if (i.quantity is None or i.quantity == 0) and takeoff.plain_unit(i.unit) in ("m", "m2", "m3", "nr")
    ]
    if blank:
        found.append(
            Problem(
                f"boq-no-quantity:{tender_id[:8]}",
                "warning",
                f"{len(blank)} measured BOQ lines have no quantity ("
                + ", ".join(boq.reference(i) for i in blank[:8])
                + "): the rate alone is priced, or the quantity is missing from the bill.",
                1,
                [],
            )
        )
    odd = [i for i in items if takeoff.plain_unit(i.unit) not in _KNOWN_UNITS and i.unit.strip()]
    if odd:
        found.append(
            Problem(
                f"boq-units:{tender_id[:8]}",
                "warning",
                "Units Quantix doesn't recognise: "
                + ", ".join(f"{boq.reference(i)} “{i.unit}”" for i in odd[:8])
                + ". Check them against the method of measurement.",
                1,
                [],
            )
        )
    for row in takeoff.compare(session, tender_id):
        if row.result == "not_on_drawings":
            found.append(
                Problem(
                    f"boq-not-drawn:{row.boq_item_id}",
                    "warning",
                    f"BOQ line {row.item} ({row.description[:60]}) is billed but the office found nothing of it on the "
                    "drawings: raise a query.",
                    1,
                    [],
                )
            )
    return found
