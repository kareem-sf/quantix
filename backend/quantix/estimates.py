"""Conservative BOQ interpretation and engineer-controlled Decimal estimating."""

import hashlib
import json
import re
from datetime import UTC, datetime
from decimal import ROUND_HALF_UP, Decimal, InvalidOperation, localcontext
from urllib.parse import urlsplit

from openpyxl.utils.cell import coordinate_from_string

from .db import dump, new_id, now, record
from .estimate_models import (
    EngineerDecision,
    ItemUpdate,
    QuantityRequest,
    RateApproval,
    UnitRateInput,
)
from .source_boq import BOQ_TABLE, EXCLUSION_SCHEMA

SCHEMA = (
    BOQ_TABLE,
    *EXCLUSION_SCHEMA,
    "CREATE TABLE IF NOT EXISTS estimate_state(tender_id TEXT PRIMARY KEY REFERENCES tenders(id),revision INTEGER NOT NULL)",
    "CREATE TABLE IF NOT EXISTS quantity_proposals(id TEXT PRIMARY KEY,tender_id TEXT NOT NULL REFERENCES tenders(id),item_id TEXT NOT NULL REFERENCES boq_items(id),status TEXT NOT NULL,data_json TEXT NOT NULL,created_at TEXT NOT NULL,updated_at TEXT NOT NULL)",
    "CREATE TABLE IF NOT EXISTS rate_proposals(id TEXT PRIMARY KEY,tender_id TEXT NOT NULL REFERENCES tenders(id),item_id TEXT NOT NULL REFERENCES boq_items(id),payload_json TEXT NOT NULL,basis_json TEXT NOT NULL,basis_fingerprint TEXT NOT NULL,approved_basis_fingerprint TEXT,source_ids_json TEXT NOT NULL,run_id TEXT REFERENCES runs(id),status TEXT NOT NULL DEFAULT 'proposed',created_at TEXT NOT NULL)",
    "CREATE TRIGGER IF NOT EXISTS rate_proposals_payload_immutable BEFORE UPDATE OF id,tender_id,item_id,payload_json,basis_json,basis_fingerprint,source_ids_json,run_id,created_at ON rate_proposals BEGIN SELECT RAISE(ABORT,'Rate proposal content and basis are immutable'); END",
    "CREATE TRIGGER IF NOT EXISTS rate_proposals_no_delete BEFORE DELETE ON rate_proposals BEGIN SELECT RAISE(ABORT,'Rate proposal history is immutable'); END",
)
UNITS = {
    "m",
    "m2",
    "m3",
    "lm",
    "rm",
    "sqm",
    "cum",
    "kg",
    "g",
    "t",
    "ton",
    "tonne",
    "tonnes",
    "no",
    "nos",
    "nr",
    "ea",
    "each",
    "item",
    "ls",
    "sum",
    "set",
    "lot",
    "hr",
    "hour",
    "day",
    "م",
    "م2",
    "م3",
    "كجم",
    "طن",
    "عدد",
    "مقطوعية",
}
HEADERS = {
    "quantity": "quantity",
    "qty": "quantity",
    "q'ty": "quantity",
    "الكمية": "quantity",
    "unit": "unit",
    "uom": "unit",
    "الوحدة": "unit",
    "rate": "rate",
    "unit rate": "rate",
    "amount": "amount",
    "total": "amount",
    "item": "item",
    "item no": "item",
    "no": "item",
    "description": "description",
    "الوصف": "description",
    "بيان الأعمال": "description",
}


def _number(value):
    if value is None or isinstance(value, bool):
        return None
    try:
        number = Decimal(str(value).strip().replace(",", ""))
        return number if number.is_finite() and 0 <= number <= Decimal("999999999999") else None
    except InvalidOperation:
        return None


def _cell_number(cell):
    if cell.get("data_type") == "e" or cell.get("cached_data_type") == "e":
        return None
    if cell.get("formula"):
        if "#REF!" in str(cell["formula"]):
            return None
        return _number(cell.get("cached_value"))
    return _number(cell.get("value"))


def _unit(value):
    return re.sub(
        r"[\s.²³]", lambda match: {"²": "2", "³": "3"}.get(match[0], ""), str(value).lower()
    )


def _column(cell):
    return coordinate_from_string(cell["coordinate"])[0]


def _candidate(evidence, artifact, headers):
    cells = evidence.get("metadata", {}).get("cells", [])
    header_cells = {
        HEADERS[str(c.get("value", "")).strip().lower().rstrip(".")]: _column(c)
        for c in cells
        if str(c.get("value", "")).strip().lower().rstrip(".") in HEADERS
    }
    if "description" in header_cells or {"unit", "quantity"} <= header_cells.keys():
        headers.update(header_cells)
        return None
    units = [c for c in cells if _unit(c.get("value")) in UNITS and not c.get("formula")]
    descriptions = [
        c
        for c in cells
        if isinstance(c.get("value"), str)
        and _unit(c["value"]) not in UNITS
        and not c.get("formula")
        and len(c["value"].strip()) > 5
        and _number(c["value"]) is None
    ]
    if not units or not descriptions:
        return None
    description = " ".join(c["value"].strip() for c in descriptions)
    if re.match(
        r"^(sub\s*total|grand total|total carried|carried forward|brought forward)\b",
        description,
        re.I,
    ):
        return None
    unit = units[0]
    excluded = {headers.get(key) for key in ("rate", "amount", "item")}
    candidates = {
        c["coordinate"]: format(_cell_number(c), "f")
        for c in cells
        if _column(c) not in excluded and _cell_number(c) is not None
    }
    preferred = [
        address
        for address in candidates
        if coordinate_from_string(address)[0] == headers.get("quantity")
    ]
    quantity_cell = (
        preferred[0]
        if len(preferred) == 1
        else next(iter(candidates))
        if len(candidates) == 1
        else None
    )
    issues = ["Confirm this inferred BOQ row against the source before including it in totals."]
    if headers.get("unit") and headers["unit"] != _column(unit):
        issues.append("Header labels conflict with the row's unit and quantity values.")
    if len(units) > 1:
        issues.append("Multiple unit cells were found; confirm the row interpretation.")
    if quantity_cell is None:
        issues.append("The quantity column is ambiguous or has no usable numeric value.")
    if any(c.get("formula") for c in cells):
        issues.append(
            "Source formulas and cached values need review; formulas were not recalculated."
        )
    if (
        evidence.get("metadata", {}).get("row_hidden")
        or evidence.get("metadata", {}).get("sheet_state", "visible") != "visible"
    ):
        issues.append("The source row or sheet is hidden.")
    return {
        "document": artifact["relative_path"],
        "sheet": evidence.get("sheet") or "",
        "locator": evidence["locator"],
        "description": description[:6000],
        "unit": _unit(unit["value"]),
        "unit_cell": unit["coordinate"],
        "quantity_cell": quantity_cell,
        "quantity_candidates": candidates,
        "supplied_quantity": candidates.get(quantity_cell),
        "confirmed": False,
        "issues": issues,
        "unit_rate": None,
        "components": [],
        "currency": None,
        "tax_basis": "unknown",
        "vat_percent": None,
        "provenance": None,
    }


def _money(value):
    return format(value.quantize(Decimal("0.01"), rounding=ROUND_HALF_UP), "f")


def _current_sources(conn, source_ids):
    return all(
        conn.execute(
            "SELECT 1 FROM evidence e JOIN artifacts a ON a.id=e.artifact_id WHERE e.id=? AND a.is_current=1",
            (source_id,),
        ).fetchone()
        for source_id in source_ids
    )


class EstimateService:
    def __init__(self, repo):
        self.repo = repo
        with repo.db.connect(write=True) as conn:
            for statement in SCHEMA:
                conn.execute(statement)

    def propose_source_row(self, tender_id, values, *, run_id=None):
        from .source_boq import propose_source_row

        return propose_source_row(self, tender_id, values, run_id=run_id)

    def exclude_source_row(self, tender_id, item_id, values):
        from .source_boq import exclude_source_row

        return exclude_source_row(self, tender_id, item_id, values)

    def refresh(self, tender_id):
        with self.repo.atomic():
            tender = self.repo.get_tender(tender_id)
            with self.repo.db.connect(write=True) as conn:
                conn.execute("UPDATE boq_items SET active=0 WHERE tender_id=?", (tender_id,))
                conn.execute("""UPDATE boq_items SET active=1 WHERE tender_id=? AND row_key<>''
                    AND EXISTS (SELECT 1 FROM artifacts a WHERE a.id=boq_items.artifact_id AND a.is_current=1)""", (tender_id,))
                for artifact in self.repo.list_artifacts(tender_id):
                    if artifact["kind"] != "spreadsheet":
                        continue
                    headers_by_sheet = {}
                    offset = 0
                    while rows := self.repo.artifact_evidence(
                        tender_id, artifact["id"], offset, 200
                    ):
                        for evidence in rows:
                            headers = headers_by_sheet.setdefault(evidence.get("sheet"), {})
                            data = _candidate(evidence, artifact, headers)
                            if data is None:
                                continue
                            conn.execute(
                                "INSERT INTO boq_items(id,tender_id,source_id,artifact_id,active,data_json) VALUES(?,?,?,?,1,?) ON CONFLICT(tender_id,source_id,row_key) DO UPDATE SET active=1",
                                (new_id(), tender_id, evidence["id"], artifact["id"], dump(data)),
                            )
                        offset += len(rows)
                conn.execute(
                    "INSERT INTO estimate_state VALUES(?,?) ON CONFLICT(tender_id) DO UPDATE SET revision=excluded.revision",
                    (tender_id, tender["revision"]),
                )
        return self.view(tender_id)

    def _item(self, conn, tender_id, item_id):
        row = record(
            conn.execute(
                "SELECT b.* FROM boq_items b JOIN artifacts a ON a.id=b.artifact_id WHERE b.id=? AND b.tender_id=? AND b.active=1 AND a.is_current=1",
                (item_id, tender_id),
            ).fetchone()
        )
        return {key: row[key] for key in ("id", "tender_id", "artifact_id", "source_id")} | row[
            "data"
        ]

    def _write(self, conn, item):
        data = {
            key: value
            for key, value in item.items()
            if key not in {"id", "tender_id", "artifact_id", "source_id"}
        }
        conn.execute("UPDATE boq_items SET data_json=? WHERE id=?", (dump(data), item["id"]))

    def _decision(self, conn, tender_id, target, identifier, decision, rationale):
        decision_id = new_id()
        conn.execute(
            "INSERT INTO decisions VALUES(?,?,?,?,?,?,?)",
            (decision_id, tender_id, target, identifier, decision, rationale, now()),
        )
        return decision_id

    def rate_basis(self, tender_id, item_id):
        """Capture current source, quantity selection and installed commercial values."""
        with self.repo.db.connect() as conn:
            item = self._item(conn, tender_id, item_id)
            artifact = self.repo.get_artifact(tender_id, item["artifact_id"])
            measurements = [
                record(row)
                for row in conn.execute(
                    "SELECT id,data_json FROM quantity_proposals WHERE item_id=? AND status='approved' ORDER BY id",
                    (item_id,),
                )
            ]
            basis = {
                "item": item,
                "source_hash": artifact["content_hash"],
                "approved_measurements": measurements,
            }
            linked_sources = set((item["provenance"] or {}).get("source_ids", []))
            for measurement in measurements:
                linked_sources.update(measurement["data"]["source_ids"])
            basis["linked_source_versions"] = []
            for source_id in sorted(linked_sources):
                evidence = self.repo.get_evidence(tender_id, source_id)
                linked = self.repo.get_artifact(tender_id, evidence["artifact_id"])
                basis["linked_source_versions"].append(
                    {
                        "source_id": source_id,
                        "artifact_id": linked["id"],
                        "hash": linked["content_hash"],
                        "is_current": linked["is_current"],
                    }
                )
        fingerprint = hashlib.sha256(
            json.dumps(basis, sort_keys=True, ensure_ascii=False).encode()
        ).hexdigest()
        return {"basis": basis, "fingerprint": fingerprint, "source_id": item["source_id"]}

    def validate_rate(self, tender_id, item_id, values, expected_basis=None):
        request = UnitRateInput.model_validate(values)
        basis = self.rate_basis(tender_id, item_id)
        if expected_basis is not None and basis["fingerprint"] != expected_basis:
            raise ValueError(
                "The BOQ item changed since it was read or proposed. Review its current basis."
            )
        source = request.provenance
        if source.observed_on > datetime.now(UTC).date():
            raise ValueError("A rate source date cannot be in the future.")
        if source.basis == "observed" and not (source.source_ids or source.urls):
            raise ValueError("An observed rate requires source evidence or a URL.")
        for source_id in [basis["source_id"], *source.source_ids]:
            evidence = self.repo.get_evidence(tender_id, source_id)
            if not self.repo.get_artifact(tender_id, evidence["artifact_id"])["is_current"]:
                raise ValueError("A rate proposal source is no longer current.")
        for url in source.urls:
            parsed = urlsplit(url)
            if parsed.scheme not in {"http", "https"} or not parsed.hostname or parsed.username:
                raise ValueError("Rate sources require HTTP or HTTPS URLs.")
        return request, basis

    def propose_rate(self, tender_id, item_id, values, *, expected_basis=None, run_id=None):
        with self.repo.atomic():
            request, basis = self.validate_rate(tender_id, item_id, values, expected_basis)
            if run_id and self.repo.get_run(run_id)["tender_id"] != tender_id:
                raise ValueError("The proposing run does not belong to this Tender.")
            identifier = new_id()
            sources = sorted(set([basis["source_id"], *request.provenance.source_ids]))
            with self.repo.db.connect(write=True) as conn:
                conn.execute(
                    "INSERT INTO rate_proposals(id,tender_id,item_id,payload_json,basis_json,basis_fingerprint,source_ids_json,run_id,created_at) VALUES(?,?,?,?,?,?,?,?,?)",
                    (
                        identifier,
                        tender_id,
                        item_id,
                        request.model_dump_json(),
                        dump(basis["basis"]),
                        basis["fingerprint"],
                        dump(sources),
                        run_id,
                        now(),
                    ),
                )
        return self.get_rate_proposal(tender_id, identifier)

    def get_rate_proposal(self, tender_id, proposal_id):
        with self.repo.db.connect() as conn:
            proposed = record(
                conn.execute(
                    "SELECT * FROM rate_proposals WHERE id=? AND tender_id=?",
                    (proposal_id, tender_id),
                ).fetchone()
            )
        try:
            expected = (
                proposed["approved_basis_fingerprint"]
                if proposed["status"] == "approved"
                else proposed["basis_fingerprint"]
            )
            self.validate_rate(tender_id, proposed["item_id"], proposed["payload"], expected)
            proposed["is_current"] = True
        except (ValueError, KeyError):
            proposed["is_current"] = False
        return proposed

    def list_rate_proposals(self, tender_id):
        self.repo.get_tender(tender_id)
        with self.repo.db.connect() as conn:
            ids = [
                row[0]
                for row in conn.execute(
                    "SELECT id FROM rate_proposals WHERE tender_id=? ORDER BY created_at DESC,rowid DESC",
                    (tender_id,),
                )
            ]
        return [self.get_rate_proposal(tender_id, identifier) for identifier in ids]

    def approve_rate(self, tender_id, proposal_id, values):
        decision = RateApproval.model_validate(values)
        with self.repo.atomic():
            proposed = self.get_rate_proposal(tender_id, proposal_id)
            if proposed["status"] != "proposed" or not proposed["is_current"]:
                raise ValueError(
                    "The proposal is already decided or its item/source basis changed."
                )
            payload = {
                key: value
                for key, value in proposed["payload"].items()
                if key not in {"unit_rate", "components"} or value is not None
            }
            self.update_item(tender_id, proposed["item_id"], payload | decision.model_dump())
            installed = self.rate_basis(tender_id, proposed["item_id"])["fingerprint"]
            with self.repo.db.connect(write=True) as conn:
                conn.execute(
                    "UPDATE rate_proposals SET status='approved',approved_basis_fingerprint=? WHERE id=?",
                    (installed, proposal_id),
                )
                self._decision(
                    conn, tender_id, "rate_proposal", proposal_id, "approve", decision.rationale
                )
        return self.get_rate_proposal(tender_id, proposal_id)

    def update_item(self, tender_id, item_id, values):
        with localcontext() as arithmetic:
            arithmetic.prec = 80
            return self._update_item(tender_id, item_id, values)

    def _update_item(self, tender_id, item_id, values):
        request = ItemUpdate.model_validate(values)
        if request.vat_percent is not None and Decimal(request.vat_percent) > 100:
            raise ValueError("VAT percentage must be between zero and 100.")
        with self.repo.db.connect(write=True) as conn:
            item = self._item(conn, tender_id, item_id)
            if item.get("source_proposal"):
                from .source_boq import validate_source_row

                validate_source_row(self.repo, tender_id, item["source_proposal"])
            if request.provenance:
                source = request.provenance
                for source_id in source.source_ids:
                    self.repo.get_evidence(tender_id, source_id)
                if source.observed_on > datetime.now(UTC).date():
                    raise ValueError("A rate source date cannot be in the future.")
                if source.basis == "observed" and not (source.source_ids or source.urls):
                    raise ValueError("An observed rate needs a source reference or URL.")
                for url in source.urls:
                    parsed = urlsplit(url)
                    if (
                        parsed.scheme not in {"http", "https"}
                        or not parsed.hostname
                        or parsed.username
                    ):
                        raise ValueError("Rate source URLs must be HTTP or HTTPS addresses.")
            if request.quantity_cell:
                if request.quantity_cell not in item["quantity_candidates"]:
                    raise ValueError("Choose a numeric quantity cell from the supplied source row.")
                item["quantity_cell"] = request.quantity_cell
                item["supplied_quantity"] = item["quantity_candidates"][request.quantity_cell]
            if request.confirm_source:
                approved = [
                    record(row)
                    for row in conn.execute(
                        "SELECT data_json FROM quantity_proposals WHERE item_id=? AND status='approved'",
                        (item_id,),
                    )
                ]
                has_current_measurement = any(
                    _current_sources(conn, proposal["data"]["source_ids"]) for proposal in approved
                )
                if item["supplied_quantity"] is None and not has_current_measurement:
                    raise ValueError(
                        "Resolve the supplied quantity cell or approve a current measured quantity before confirming this row."
                    )
                item["confirmed"] = True
            for name in ("unit_rate", "currency", "tax_basis", "vat_percent", "provenance"):
                if name in request.model_fields_set:
                    value = getattr(request, name)
                    item[name] = (
                        value.model_dump(mode="json") if name == "provenance" and value else value
                    )
            if item["tax_basis"] is None:
                item["tax_basis"] = "unknown"
            if "components" in request.model_fields_set:
                item["components"] = [
                    component.model_dump() for component in request.components or []
                ]
                item["unit_rate"] = (
                    format(
                        sum(
                            (
                                Decimal(c["quantity"]) * Decimal(c["unit_rate"])
                                for c in item["components"]
                            ),
                            Decimal(0),
                        ),
                        "f",
                    )
                    if item["components"]
                    else None
                )
            elif "unit_rate" in request.model_fields_set:
                item["components"] = []
            if item["unit_rate"] is not None and not (
                item["currency"] and item["tax_basis"] and item["provenance"]
            ):
                raise ValueError(
                    "A priced item requires dated rate provenance, currency and tax basis."
                )
            self._write(conn, item)
            self._decision(conn, tender_id, "estimate_item", item_id, "update", request.rationale)
        return next(item for item in self.view(tender_id)["items"] if item["id"] == item_id)

    def propose_quantity(self, tender_id, item_id, values):
        request = QuantityRequest.model_validate(values)
        with self.repo.atomic():
            return self._store_quantity_proposal(
                tender_id,
                item_id,
                request.model_dump(exclude={"engineer_confirmed", "rationale"}),
                origin="engineer",
                rationale=request.rationale,
            )

    def validate_agent_quantity(self, tender_id, values, expected_basis):
        from .quantity_models import AgentQuantityProposal

        request = AgentQuantityProposal.model_validate(values)
        if not isinstance(expected_basis, str) or not re.fullmatch(r"[a-f0-9]{64}", expected_basis):
            raise ValueError("An agent quantity must retain the BOQ item basis actually read in this run.")
        basis = self.rate_basis(tender_id, request.item_id)
        if basis["fingerprint"] != expected_basis:
            raise ValueError("The BOQ item changed since it was read. Review the current item before proposing a quantity.")
        for source_id in [basis["source_id"], *request.source_ids]:
            evidence = self.repo.get_evidence(tender_id, source_id)
            if not self.repo.get_artifact(tender_id, evidence["artifact_id"])["is_current"]:
                raise ValueError("A proposed quantity must use current supporting sources.")
        return request, basis

    def propose_agent_quantity(self, tender_id, values, run_id, expected_basis):
        with self.repo.atomic():
            if not run_id or self.repo.get_run(run_id)["tender_id"] != tender_id:
                raise ValueError("The proposing run does not belong to this Tender.")
            request, basis = self.validate_agent_quantity(tender_id, values, expected_basis)
            data = request.model_dump(exclude={"item_id"})
            data["source_ids"] = list(dict.fromkeys([basis["source_id"], *request.source_ids]))
            return self._store_quantity_proposal(
                tender_id, request.item_id, data, origin="agent", run_id=run_id,
                basis_fingerprint=self.quantity_item_fingerprint(tender_id, request.item_id),
            )

    def quantity_item_fingerprint(self, tender_id, item_id):
        """Bind quantity review to the source row, unit and supplied quantity interpretation."""
        with self.repo.db.connect() as conn:
            item = self._item(conn, tender_id, item_id)
            artifact = self.repo.get_artifact(tender_id, item["artifact_id"])
            basis = {
                key: item[key] for key in (
                    "source_id", "artifact_id", "description", "unit", "quantity_cell", "supplied_quantity",
                )
            }
            basis["content_hash"] = artifact["content_hash"]
            return hashlib.sha256(json.dumps(basis, sort_keys=True, ensure_ascii=False).encode()).hexdigest()

    def _store_quantity_proposal(self, tender_id, item_id, values, *, origin, rationale=None, run_id=None, basis_fingerprint=None):
        with self.repo.db.connect(write=True) as conn:
            self._item(conn, tender_id, item_id)
            for source_id in values["source_ids"]:
                evidence = self.repo.get_evidence(tender_id, source_id)
                if not self.repo.get_artifact(tender_id, evidence["artifact_id"])["is_current"]:
                    raise ValueError("A quantity proposal source is no longer current.")
            identifier, stamp = new_id(), now()
            data = values | {"origin": origin, "run_id": run_id, "basis_fingerprint": basis_fingerprint}
            conn.execute(
                "INSERT INTO quantity_proposals VALUES(?,?,?,'proposed',?,?,?)",
                (identifier, tender_id, item_id, dump(data), stamp, stamp),
            )
            if origin == "engineer":
                self._decision(conn, tender_id, "quantity_proposal", identifier, "propose", rationale)
        return {
            "id": identifier,
            "item_id": item_id,
            "status": "proposed",
            "created_at": stamp,
            **data,
        }

    def approve_quantity(self, tender_id, proposal_id, values):
        request = EngineerDecision.model_validate(values)
        with self.repo.db.connect(write=True) as conn:
            proposal = record(
                conn.execute(
                    "SELECT * FROM quantity_proposals WHERE id=? AND tender_id=?",
                    (proposal_id, tender_id),
                ).fetchone()
            )
            self._item(conn, tender_id, proposal["item_id"])
            if proposal["status"] != "proposed":
                raise ValueError("Only a proposed quantity can be approved.")
            if proposal["data"].get("origin") == "agent" and proposal["data"].get("basis_fingerprint") != self.quantity_item_fingerprint(tender_id, proposal["item_id"]):
                raise ValueError("The BOQ source or quantity interpretation changed after this proposal. Ask for a new quantity proposal from the current item.")
            if not _current_sources(conn, proposal["data"]["source_ids"]):
                raise ValueError(
                    "Measurement evidence has changed. Create a new proposal from current sources."
                )
            conn.execute(
                "UPDATE quantity_proposals SET status='superseded',updated_at=? WHERE item_id=? AND status='approved'",
                (now(), proposal["item_id"]),
            )
            conn.execute(
                "UPDATE quantity_proposals SET status='approved',updated_at=? WHERE id=?",
                (now(), proposal_id),
            )
            self._decision(
                conn, tender_id, "quantity_proposal", proposal_id, "approve", request.rationale
            )
        return self.view(tender_id)

    def view(self, tender_id):
        with localcontext() as arithmetic:
            arithmetic.prec = 80
            return self._view(tender_id)

    def _quantity_current(self, conn, tender_id, proposal):
        if not _current_sources(conn, proposal["data"]["source_ids"]):
            return False
        if proposal["data"].get("origin") == "agent" and proposal["data"].get("basis_fingerprint") != self.quantity_item_fingerprint(tender_id, proposal["item_id"]):
            return False
        return True

    def _view(self, tender_id):
        tender = self.repo.get_tender(tender_id)
        with self.repo.db.connect() as conn:
            state = conn.execute(
                "SELECT revision FROM estimate_state WHERE tender_id=?", (tender_id,)
            ).fetchone()
            refresh_required = state is None or state[0] != tender["revision"]
            identifiers = [
                row[0]
                for row in conn.execute(
                    "SELECT b.id FROM boq_items b JOIN artifacts a ON a.id=b.artifact_id WHERE b.tender_id=? AND b.active=1 AND a.is_current=1 ORDER BY a.relative_path,b.rowid",
                    (tender_id,),
                )
            ]
            items = []
            for identifier in identifiers:
                item = self._item(conn, tender_id, identifier)
                if item.get("source_proposal"):
                    evidence = self.repo.get_evidence(tender_id, item["source_id"])
                    if item["source_excerpt"] not in evidence["text"]:
                        item["confirmed"] = False
                        item["issues"].append("The extracted source passage changed. Review and replace this BOQ proposal before pricing.")
                proposals = [
                    record(row)
                    for row in conn.execute(
                        "SELECT * FROM quantity_proposals WHERE item_id=? ORDER BY created_at",
                        (identifier,),
                    )
                ]
                for proposal in proposals:
                    if proposal["status"] == "approved" and not self._quantity_current(
                        conn, tender_id, proposal
                    ):
                        proposal["status"] = "needs_review"
                        item["confirmed"] = False
                        item["issues"].append(
                            "Approved measurement evidence has changed; review the quantity again."
                        )
                if item["provenance"] and not _current_sources(
                    conn, item["provenance"].get("source_ids", [])
                ):
                    item["confirmed"] = False
                    item["issues"].append(
                        "Rate source evidence has changed; replace or revalidate the rate source."
                    )
                item["quantity_proposals"] = [
                    {
                        "id": p["id"],
                        "item_id": p["item_id"],
                        "status": p["status"],
                        "created_at": p["created_at"],
                        **p["data"],
                    }
                    for p in proposals
                ]
                approved = next((p for p in proposals if p["status"] == "approved"), None)
                item["effective_quantity"] = (
                    approved["data"]["quantity"] if approved else item["supplied_quantity"]
                )
                item["quantity_basis"] = "approved_measurement" if approved else "supplied_boq"
                item["rate_ex_vat"] = item["line_ex_vat"] = item["line_inc_vat"] = None
                if (
                    item["unit_rate"] is not None
                    and item["effective_quantity"] is not None
                    and item["confirmed"]
                ):
                    value, vat = (
                        Decimal(item["unit_rate"]),
                        Decimal(item["vat_percent"]) if item["vat_percent"] is not None else None,
                    )
                    if item["tax_basis"] == "including_vat" and vat is not None:
                        value = value / (1 + vat / 100)
                    if item["tax_basis"] == "excluding_vat" or (
                        item["tax_basis"] == "including_vat" and vat is not None
                    ):
                        item["rate_ex_vat"] = format(value, "f")
                        net = Decimal(item["effective_quantity"]) * value
                        item["line_ex_vat"] = _money(net)
                        if vat is not None:
                            item["line_inc_vat"] = _money(
                                Decimal(item["effective_quantity"]) * Decimal(item["unit_rate"])
                                if item["tax_basis"] == "including_vat"
                                else net * (1 + vat / 100)
                            )
                items.append(item)
        counts = {
            "unpriced_count": sum(item["unit_rate"] is None for item in items),
            "unconfirmed_count": sum(not item["confirmed"] for item in items),
            "unknown_vat_count": sum(
                item["vat_percent"] is None or item["tax_basis"] == "unknown" for item in items
            ),
            "unresolved_quantity_count": sum(item["effective_quantity"] is None for item in items),
        }
        from .source_boq import retired_source_rows

        retired = retired_source_rows(self.repo, tender_id)
        complete = bool(items) and not refresh_required and not any(counts.values()) and not retired
        totals = []
        for currency in sorted({item["currency"] for item in items if item["currency"]}):
            group = [item for item in items if item["currency"] == currency]
            subtotal = _money(
                sum(
                    (
                        Decimal(item["line_ex_vat"])
                        for item in group
                        if item["line_ex_vat"] is not None
                    ),
                    Decimal(0),
                )
            )
            net_complete = (
                not refresh_required
                and not retired
                and all(item["line_ex_vat"] is not None for item in group)
                and not counts["unpriced_count"]
            )
            gross_complete = net_complete and all(
                item["line_inc_vat"] is not None for item in group
            )
            totals.append(
                {
                    "currency": currency,
                    "priced_subtotal_ex_vat": subtotal,
                    "total_ex_vat": subtotal if net_complete else None,
                    "total_inc_vat": _money(
                        sum((Decimal(item["line_inc_vat"]) for item in group), Decimal(0))
                    )
                    if gross_complete
                    else None,
                    "complete": gross_complete,
                }
            )
        reasons = [
            label
            for key, label in (
                ("unpriced_count", "Some BOQ items have no rate."),
                ("unconfirmed_count", "Some source rows need engineer confirmation."),
                ("unknown_vat_count", "VAT treatment is not established for all items."),
                ("unresolved_quantity_count", "Some quantities need source clarification."),
            )
            if counts[key]
        ]
        if refresh_required:
            reasons.append("Refresh BOQ candidates after source changes.")
        if retired:
            reasons.append(f"{len(retired)} BOQ rows from earlier source revisions need a checked replacement or an explicit scope decision.")
        if not items:
            reasons.append("No current BOQ rows are saved. Review revised rows above, refresh Excel source rows or add a row from an exact source passage.")
        return {
            "tender_id": tender_id,
            "items": items,
            "totals": totals,
            "complete": complete,
            "refresh_required": refresh_required,
            **counts,
            "blocking_reasons": reasons,
            "coverage_note": "Excel rows and source-based BOQ proposals. Check each row against its source; this does not establish complete scope coverage or a complete takeoff.",
            "retired_source_rows": retired,
        }
