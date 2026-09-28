"""The layer map: what the layers and blocks of the tender's drawings are. The office works it out on a drawing from
what each layer holds, proposes it, the Tender Manager reviews it against Quantix's checks and the engineer approves
it. Rooms, the checks for drawn work nothing measures, and the crossings of services and fire-rated walls rest on it.
Quantix never guesses a layer's meaning from its name."""

from datetime import UTC, datetime
from pathlib import Path

from sqlalchemy import select
from sqlalchemy.orm import Session

from quantix.core.review import REVIEWED
from quantix.documents.models import Document
from quantix.office import records as office
from quantix.office.models import ENGINEER
from quantix.takeoff import drawings
from quantix.takeoff.models import LayerMap


def propose(
    session: Session,
    home: Path,
    tender_id: str,
    by: str,
    document_id: str,
    layers: dict[str, str],
    blocks: dict[str, str],
    note: str,
    status: str = "proposed",
) -> LayerMap:
    document = drawings.drawing_document(session, tender_id, document_id)
    if not layers and not blocks:
        raise ValueError("Map at least one layer or block.")
    wrong = sorted({m for m in [*layers.values(), *blocks.values()] if m not in drawings.MEANINGS})
    if wrong:
        raise ValueError(
            f"Not a meaning Quantix knows: {', '.join(wrong)}. Use one of: {', '.join(drawings.MEANINGS)}."
        )
    d = drawings.open_drawing(home, document)
    names = {n.lower() for n in d.layer_names} | {n.lower().rsplit("$", 1)[-1] for n in d.layer_names}
    unknown = [name for name in layers if name.lower() not in names]
    block_names = {n.lower() for n in d.block_names}
    unknown += [name for name in blocks if name.lower() not in block_names]
    if unknown:
        raise ValueError(
            f"{document.name} has no layer or block called {', '.join(f'“{n}”' for n in unknown)}: map the names "
            "drawing_overview lists."
        )
    if len(note.split()) < 3:
        raise ValueError("Say in the note how you worked the map out: what you looked at on each layer.")
    layer_map = LayerMap(
        tender_id=tender_id,
        document_id=document_id,
        layers={k.strip(): v for k, v in layers.items()},
        blocks={k.strip(): v for k, v in blocks.items()},
        note=note.strip(),
        proposed_by=by,
        status=status,
    )
    session.add(layer_map)
    session.flush()
    return layer_map


def label(session: Session, record: LayerMap) -> str:
    return f"the layer map worked out on {session.get(Document, record.document_id).name}"


def approve(session: Session, record: LayerMap, status: str = "approved") -> None:
    record.status, record.decided_at = status, datetime.now(UTC)


def decide(session: Session, record: LayerMap, approve_it: bool, reason: str | None = None) -> None:
    if approve_it:
        approve(session, record)
    else:
        office.send_back(session, record.tender_id, record, label(session, record), reason, ENGINEER)


def waiting(session: Session, tender_id: str) -> int:
    query = select(LayerMap.id).where(LayerMap.tender_id == tender_id, LayerMap.status == REVIEWED)
    return len(session.scalars(query).all())


def describe(record: LayerMap) -> str:
    """The map in a line: how many layers and blocks, by meaning."""
    counts: dict[str, int] = {}
    for meaning in [*record.layers.values(), *record.blocks.values()]:
        counts[meaning] = counts.get(meaning, 0) + 1
    listed = ", ".join(f"{drawings.MEANINGS[m].lower()} {n}" for m, n in sorted(counts.items(), key=lambda kv: -kv[1]))
    return f"{len(record.layers)} layers and {len(record.blocks)} blocks: {listed}"
