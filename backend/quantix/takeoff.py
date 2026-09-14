"""Save takeoff lines with a comparison Quantix computes itself, and record the engineer's review."""

from __future__ import annotations

import re
from decimal import Decimal

from .db import dump, new_id, now, record
from .takeoff_models import TakeoffLine, TakeoffLineProposal, TakeoffReview

SCHEMA = (
    "CREATE TABLE IF NOT EXISTS takeoff_lines(id TEXT PRIMARY KEY,tender_id TEXT NOT NULL REFERENCES tenders(id),"
    "run_id TEXT NOT NULL,status TEXT NOT NULL,data_json TEXT NOT NULL,created_at TEXT NOT NULL,updated_at TEXT NOT NULL)",
    "CREATE INDEX IF NOT EXISTS takeoff_lines_tender ON takeoff_lines(tender_id,created_at)",
)

# Quantities within this share of the BOQ quantity count as matching.
MATCH_TOLERANCE = Decimal("0.02")

_UNITS = {
    "m": {"m", "lm", "rm", "m1", "lin.m", "l.m", "م", "م.ط"},
    "m2": {"m2", "m²", "sqm", "sq.m", "sq m", "م2", "م²"},
    "m3": {"m3", "m³", "cum", "cu.m", "cu m", "م3", "م³"},
    "nr": {"nr", "no", "no.", "nos", "ea", "each", "pcs", "pc", "number", "عدد"},
    "kg": {"kg", "kgs", "كجم", "كغ"},
    "t": {"t", "ton", "tons", "tonne", "tonnes", "طن"},
    "item": {"item", "sum", "ls", "l.s.", "lump sum", "مقطوعية"},
}


def normal_unit(value: str) -> str:
    text = re.sub(r"\s+", " ", str(value or "").strip().casefold())
    return next((unit for unit, spellings in _UNITS.items() if text in spellings), text)


def compare(proposal: TakeoffLineProposal, item: dict | None) -> dict:
    """Compare a takeoff line with its BOQ item; the model never labels the result."""
    if item is None:
        return {"boq": None, "comparison": "not_in_boq"}
    supplied = item.get("supplied_quantity")
    boq = {"description": item["description"], "unit": item["unit"], "quantity": supplied}
    if proposal.quantity is None:
        return {"boq": boq, "comparison": "not_on_drawings"}
    if normal_unit(proposal.unit) != normal_unit(item["unit"]):
        return {"boq": boq, "comparison": "unit_differs"}
    if supplied is None:
        return {"boq": boq, "comparison": "no_boq_quantity"}
    taken, listed = Decimal(proposal.quantity), Decimal(str(supplied))
    difference = taken - listed
    if listed == 0:
        matches, percent = taken == 0, None
    else:
        matches = abs(difference) <= abs(listed) * MATCH_TOLERANCE
        percent = format((difference / listed * 100).quantize(Decimal("0.1")), "f")
    return {
        "boq": boq,
        "comparison": "matches" if matches else "differs",
        "difference": format(difference.normalize(), "f"),
        "difference_percent": percent,
    }


class TakeoffService:
    def __init__(self, repo):
        self.repo = repo
        with repo.db.connect(write=True) as conn:
            for statement in SCHEMA:
                conn.execute(statement)

    def validate(self, context, proposal: TakeoffLineProposal) -> None:
        context.validate_sources(proposal.source_ids)
        if proposal.boq_item_id is not None and proposal.boq_item_id not in context.item_bases:
            raise ValueError(
                "Read the BOQ item with inspect_estimate in this run before matching a takeoff line to it."
            )

    def publish(self, context, proposals: list[TakeoffLineProposal], *, author: str) -> list[dict]:
        """Save lines inside the caller's publication transaction."""
        from .estimates import EstimateService

        if not proposals:
            return []
        estimates = EstimateService(self.repo)
        items = {item["id"]: item for item in estimates.view(context.tender_id)["items"]}
        saved = []
        stamp = now()
        with self.repo.db.connect(write=True) as conn:
            for proposal in proposals:
                self.validate(context, proposal)
                item = items.get(proposal.boq_item_id) if proposal.boq_item_id else None
                if proposal.boq_item_id and item is None:
                    raise ValueError("A takeoff line's BOQ item is no longer in the estimate.")
                data = {
                    **proposal.model_dump(),
                    **compare(proposal, item),
                    "assignment_id": context.assignment_id,
                    "author": author,
                    "item_fingerprint": estimates.quantity_item_fingerprint(context.tender_id, item["id"]) if item else None,
                    "review_note": "",
                    "reviewed_at": None,
                }
                identifier = new_id()
                conn.execute(
                    "INSERT INTO takeoff_lines VALUES(?,?,?,?,?,?,?)",
                    (identifier, context.tender_id, context.run_id, "proposed", dump(data), stamp, stamp),
                )
                saved.append({"id": identifier, "comparison": data["comparison"]})
        return saved

    def list(self, tender_id: str) -> list[TakeoffLine]:
        from .estimates import EstimateService

        self.repo.get_tender(tender_id)
        with self.repo.db.connect() as conn:
            rows = [record(row) for row in conn.execute(
                "SELECT * FROM takeoff_lines WHERE tender_id=? ORDER BY created_at DESC, rowid DESC", (tender_id,))]
        estimates = EstimateService(self.repo)
        fingerprints: dict[str, str | None] = {}
        artifacts: dict[str, bool] = {}
        lines = []
        for row in rows:
            data = row["data"]
            current = True
            for source_id in data["source_ids"]:
                try:
                    artifact_id = self.repo.get_evidence(tender_id, source_id)["artifact_id"]
                    if artifact_id not in artifacts:
                        artifacts[artifact_id] = self.repo.get_artifact(tender_id, artifact_id)["is_current"]
                    current = current and artifacts[artifact_id]
                except KeyError:
                    current = False
            item_id = data.get("boq_item_id")
            if item_id:
                if item_id not in fingerprints:
                    try:
                        fingerprints[item_id] = estimates.quantity_item_fingerprint(tender_id, item_id)
                    except KeyError:
                        fingerprints[item_id] = None
                current = current and fingerprints[item_id] == data.get("item_fingerprint")
            lines.append(TakeoffLine.model_validate({
                **{key: value for key, value in data.items() if key != "item_fingerprint"},
                "id": row["id"], "tender_id": tender_id, "run_id": row["run_id"], "status": row["status"],
                "is_current": current, "created_at": row["created_at"], "updated_at": row["updated_at"],
            }))
        return lines

    def review(self, tender_id: str, line_id: str, request: TakeoffReview) -> TakeoffLine:
        with self.repo.atomic() as conn:
            row = conn.execute("SELECT * FROM takeoff_lines WHERE id=? AND tender_id=?", (line_id, tender_id)).fetchone()
            if row is None:
                raise KeyError("This takeoff line is not in the tender.")
            saved = record(row)
            stamp = now()
            data = saved["data"] | {"review_note": request.note, "reviewed_at": stamp}
            conn.execute("UPDATE takeoff_lines SET status=?,data_json=?,updated_at=? WHERE id=?",
                         (request.decision, dump(data), stamp, line_id))
            conn.execute("INSERT INTO decisions VALUES(?,?,?,?,?,?,?)",
                         (new_id(), tender_id, "takeoff_line", line_id, request.decision,
                          request.note or f"Takeoff line {request.decision} by the engineer.", stamp))
        return next(line for line in self.list(tender_id) if line.id == line_id)
