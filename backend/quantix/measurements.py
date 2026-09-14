"""Deterministic geometry for explicitly calibrated, source-linked PDF takeoffs.

Coordinates are fractions from the page's top-left. PDF dimensions are supplied
by the document service, never inferred from a screenshot or title-block scale.
"""

import hashlib
import json
import math
from contextlib import closing
from decimal import Decimal, InvalidOperation, localcontext

import pypdfium2 as pdfium

from .db import dump, new_id, now, record
from .documents import MAX_FILE_BYTES, _pdf_lock
from .estimate_models import QuantityProposal
from .estimates import EstimateService
from .measurement_models import (
    AgentMeasurementProposal,
    MeasurementCreate,
    MeasurementInput,
    MeasurementLink,
)

SCHEMA = (
    "CREATE TABLE IF NOT EXISTS measurements(id TEXT PRIMARY KEY,tender_id TEXT NOT NULL REFERENCES tenders(id),artifact_id TEXT NOT NULL REFERENCES artifacts(id),source_id TEXT NOT NULL REFERENCES evidence(id),data_json TEXT NOT NULL,created_at TEXT NOT NULL)",
    "CREATE INDEX IF NOT EXISTS measurements_source ON measurements(tender_id,artifact_id,created_at)",
    "CREATE TRIGGER IF NOT EXISTS measurements_immutable BEFORE UPDATE ON measurements BEGIN SELECT RAISE(ABORT,'Measurement records are immutable'); END",
    "CREATE TRIGGER IF NOT EXISTS measurements_no_delete BEFORE DELETE ON measurements BEGIN SELECT RAISE(ABORT,'Measurement records are immutable'); END",
    "CREATE TABLE IF NOT EXISTS measurement_links(measurement_id TEXT NOT NULL REFERENCES measurements(id),item_id TEXT NOT NULL REFERENCES boq_items(id),proposal_id TEXT NOT NULL UNIQUE REFERENCES quantity_proposals(id),rationale TEXT NOT NULL,created_at TEXT NOT NULL,PRIMARY KEY(measurement_id,item_id))",
    "CREATE TRIGGER IF NOT EXISTS measurement_links_immutable BEFORE UPDATE ON measurement_links BEGIN SELECT RAISE(ABORT,'Measurement links are immutable'); END",
    "CREATE TRIGGER IF NOT EXISTS measurement_links_no_delete BEFORE DELETE ON measurement_links BEGIN SELECT RAISE(ABORT,'Measurement links are immutable'); END",
    "CREATE TABLE IF NOT EXISTS measurement_link_bases(proposal_id TEXT PRIMARY KEY REFERENCES quantity_proposals(id),basis_json TEXT NOT NULL)",
    "CREATE TRIGGER IF NOT EXISTS measurement_bases_no_update BEFORE UPDATE ON measurement_link_bases BEGIN SELECT RAISE(ABORT,'Measurement bases are immutable'); END",
    "CREATE TRIGGER IF NOT EXISTS measurement_bases_no_delete BEFORE DELETE ON measurement_link_bases BEGIN SELECT RAISE(ABORT,'Measurement bases are immutable'); END",
)

COMPATIBLE_UNITS = {
    "m": {"m", "lm", "rm", "م"},
    "m2": {"m2", "m²", "sqm", "م2", "م²"},
    "nr": {"nr", "no", "nos", "ea", "each", "عدد"},
}


def _fingerprint(value):
    return hashlib.sha256(
        json.dumps(value, sort_keys=True, ensure_ascii=False).encode()
    ).hexdigest()


def _evidence_basis(evidence):
    return {
        key: evidence.get(key)
        for key in ("artifact_id", "locator", "text", "page", "kind", "metadata")
    }


def _item_basis(item):
    return {
        key: item[key]
        for key in (
            "id",
            "artifact_id",
            "source_id",
            "description",
            "unit",
            "quantity_cell",
            "supplied_quantity",
        )
    }


def _proposal_basis(proposal):
    return {
        key: proposal[key] for key in ("id", "item_id", "quantity", "calculation", "source_ids")
    }


def _cancelled(cancelled):
    if cancelled and cancelled():
        raise InterruptedError("Measurement calculation cancelled.")


def _decimal(value):
    if isinstance(value, bool):
        raise ValueError("Use a finite numerical coordinate or dimension.")
    try:
        result = Decimal(str(value))
    except (InvalidOperation, ValueError) as exc:
        raise ValueError("Use a finite numerical coordinate or dimension.") from exc
    if not result.is_finite():
        raise ValueError("Use a finite numerical coordinate or dimension.")
    return result


def _points(values, minimum, maximum=500):
    if not isinstance(values, (list, tuple)) or not minimum <= len(values) <= maximum:
        raise ValueError(f"This measurement needs between {minimum} and {maximum} points.")
    result = []
    for point in values:
        if not isinstance(point, (list, tuple)) or len(point) != 2:
            raise ValueError("Each point needs horizontal and vertical page coordinates.")
        pair = tuple(_decimal(value) for value in point)
        if any(value < 0 or value > 1 for value in pair):
            raise ValueError("Measurement points must be inside the source page.")
        result.append(pair)
    if len(set(result)) != len(result):
        raise ValueError("Measurement points must be distinct.")
    return result


def _distance(a, b):
    return ((a[0] - b[0]) ** 2 + (a[1] - b[1]) ** 2).sqrt()


def _cross(a, b, c):
    return (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])


def _on_segment(a, b, c):
    return min(a[0], b[0]) <= c[0] <= max(a[0], b[0]) and min(a[1], b[1]) <= c[1] <= max(a[1], b[1])


def _intersects(a, b, c, d):
    ab_c, ab_d, cd_a, cd_b = _cross(a, b, c), _cross(a, b, d), _cross(c, d, a), _cross(c, d, b)
    if ab_c * ab_d < 0 and cd_a * cd_b < 0:
        return True
    return any(
        (cross == 0 and _on_segment(start, end, point))
        for cross, start, end, point in (
            (ab_c, a, b, c),
            (ab_d, a, b, d),
            (cd_a, c, d, a),
            (cd_b, c, d, b),
        )
    )


def _area(points):
    edges = list(zip(points, points[1:] + points[:1]))
    for index, (a, b) in enumerate(edges):
        for other, (c, d) in enumerate(edges[index + 1 :], index + 1):
            if other == index + 1 or (index == 0 and other == len(edges) - 1):
                continue
            if _intersects(a, b, c, d):
                raise ValueError(
                    "The area boundary crosses or touches itself. Draw one simple boundary."
                )
    return abs(sum(a[0] * b[1] - b[0] * a[1] for a, b in edges)) / 2


def _text(value):
    rendered = format(value.quantize(Decimal("0.000001")), "f")
    return rendered.rstrip("0").rstrip(".")


def calculate_measurement(
    mode, page_size, points, *, calibration_points=None, calibration_metres=None
):
    """Return a calculated quantity; this grants no BOQ or review authority."""
    if mode not in {"length", "area", "count"}:
        raise ValueError("Choose length, area or count.")
    if not isinstance(page_size, (list, tuple)) or len(page_size) != 2:
        raise ValueError("The source PDF page dimensions are unavailable.")
    width, height = (_decimal(value) for value in page_size)
    if not 0 < width <= 1_000_000 or not 0 < height <= 1_000_000:
        raise ValueError("The source PDF page dimensions are outside the supported range.")
    normal = _points(points, {"count": 1, "length": 2, "area": 3}[mode])
    physical = [(x * width, y * height) for x, y in normal]
    with localcontext() as context:
        context.prec = 40
        if mode == "count":
            if calibration_points is not None or calibration_metres is not None:
                raise ValueError("Counted marks do not use a length calibration.")
            quantity, unit = Decimal(len(physical)), "nr"
            basis = (
                f"{len(physical)} distinct marked positions. Object identification needs review."
            )
            calibrated = None
        else:
            calibration = _points(calibration_points, 2, 2)
            calibrated = _decimal(calibration_metres)
            if not 0 < calibrated <= 1_000_000:
                raise ValueError("Enter a positive printed calibration length in metres.")
            start, end = [(x * width, y * height) for x, y in calibration]
            scale = calibrated / _distance(start, end)
            if mode == "length":
                quantity = sum(_distance(a, b) for a, b in zip(physical, physical[1:])) * scale
                unit = "m"
                basis = f"Sum of {len(physical) - 1} segment lengths using the stated calibration."
            else:
                quantity = _area(physical) * scale**2
                unit = "m2"
                basis = "Area enclosed by the marked boundary using the squared calibration scale. No openings deducted."
            if quantity < Decimal("0.000001") or quantity > Decimal("999999999999.999999"):
                raise ValueError(
                    "The measured quantity is zero or outside the supported range. Check the boundary and calibration."
                )
        return {
            "mode": mode,
            "quantity": _text(quantity),
            "unit": unit,
            "calibration_metres": _text(calibrated) if calibrated is not None else None,
            "calculation": basis,
            "precision_note": "Calculated from marked positions; displayed decimals do not establish drawing accuracy. Review the calibration, boundaries and written dimensions before use.",
        }


class MeasurementService:
    """Immutable calibrated records, with separate engineer-controlled BOQ links."""

    def __init__(self, repo):
        self.repo = repo
        self.estimates = EstimateService(repo)
        with repo.db.connect(write=True) as conn:
            for statement in SCHEMA:
                conn.execute(statement)

    def page(self, tender_id, artifact_id, page=1, *, cancelled=None):
        if isinstance(page, bool) or not isinstance(page, int) or page < 1:
            raise ValueError("Choose a positive PDF page number.")
        artifact = self.repo.get_artifact(tender_id, artifact_id)
        if artifact["kind"] != "pdf":
            raise ValueError("Calibrated measurements require a PDF source.")
        path = self.repo.object_path(tender_id, artifact_id)
        _cancelled(cancelled)
        if path.stat().st_size > MAX_FILE_BYTES:
            raise ValueError("The source PDF exceeds the supported file size.")
        # Read once: the verified bytes are exactly what PDFium will measure.
        with path.open("rb") as stream:
            payload = stream.read(MAX_FILE_BYTES + 1)
        _cancelled(cancelled)
        if len(payload) > MAX_FILE_BYTES:
            raise ValueError("The source PDF exceeds the supported file size.")
        if hashlib.sha256(payload).hexdigest() != artifact["content_hash"]:
            raise ValueError("The saved PDF bytes no longer match the registered source hash.")
        try:
            with _pdf_lock(cancelled), pdfium.PdfDocument(payload) as document:
                page_count = len(document)
                if page > page_count:
                    raise ValueError("The requested page is outside the source PDF.")
                with closing(document[page - 1]) as pdf_page:
                    width, height = pdf_page.get_size()
        except pdfium.PdfiumError as exc:
            raise ValueError("The saved source PDF could not be opened for measurement.") from exc
        if not all(math.isfinite(value) and 0 < value <= 1_000_000 for value in (width, height)):
            raise ValueError("The source PDF page dimensions are outside the supported range.")
        _cancelled(cancelled)
        return {
            "artifact_id": artifact_id,
            "version": artifact["version"],
            "content_hash": artifact["content_hash"],
            "name": artifact["name"],
            "relative_path": artifact["relative_path"],
            "page": page,
            "page_count": page_count,
            "page_size": [width, height],
            "is_current": artifact["is_current"],
        }

    def calculate(self, tender_id, values, *, cancelled=None):
        request = MeasurementInput.model_validate(values)
        source = self.page(tender_id, request.artifact_id, request.page, cancelled=cancelled)
        if not source["is_current"]:
            raise ValueError("This drawing has a newer revision. Measure the current source.")
        calculated = calculate_measurement(
            request.mode,
            source["page_size"],
            request.points,
            calibration_points=request.calibration_points,
            calibration_metres=request.calibration_metres,
        )
        _cancelled(cancelled)
        return {
            **calculated,
            "source": source,
            "points": [list(point) for point in request.points],
            "calibration_points": [list(point) for point in request.calibration_points]
            if request.calibration_points
            else None,
            "coordinate_system": "normalized_top_left",
            "calculation_version": "calibrated-pdf-v1",
        }

    def create(self, tender_id, values):
        request = MeasurementCreate.model_validate(values)
        return self._store(tender_id, request, origin="engineer", review_rationale=request.rationale)

    def supporting_sources(self, tender_id, request):
        """Capture exact current evidence for the stated drawing and calibration."""
        sources = []
        checked_artifacts = set()
        page_supported = False
        for source_id in request.source_ids:
            evidence = self.repo.get_evidence(tender_id, source_id)
            artifact = self.repo.get_artifact(tender_id, evidence["artifact_id"])
            if not artifact["is_current"]:
                raise ValueError("Measurement support must use current source revisions.")
            if artifact["id"] not in checked_artifacts:
                path = self.repo.object_path(tender_id, artifact["id"])
                with path.open("rb") as stream:
                    if hashlib.file_digest(stream, "sha256").hexdigest() != artifact["content_hash"]:
                        raise ValueError("The saved supporting source bytes have changed.")
                checked_artifacts.add(artifact["id"])
            page_supported |= evidence["artifact_id"] == request.artifact_id and evidence.get("page") == request.page
            sources.append({
                "source_id": source_id, "artifact_id": artifact["id"],
                "artifact_name": artifact["name"], "relative_path": artifact["relative_path"],
                "locator": evidence["locator"], "page": evidence.get("page"),
                "version": artifact["version"], "content_hash": artifact["content_hash"],
                "evidence_hash": _fingerprint(_evidence_basis(evidence)), "kind": evidence["kind"],
            })
        if not page_supported:
            raise ValueError("Include a supporting source reference from the measured drawing page.")
        return sources

    def propose_agent(self, tender_id, values, run_id):
        request = AgentMeasurementProposal.model_validate(values)
        if not run_id or self.repo.get_run(run_id)["tender_id"] != tender_id:
            raise KeyError("An agent measurement must belong to its Tender run.")
        return self._store(tender_id, request, origin="agent", run_id=run_id)

    def _store(self, tender_id, request, *, origin, run_id=None, review_rationale=None):
        with self.repo.atomic() as conn:
            supporting = self.supporting_sources(tender_id, request) if origin == "agent" else []
            calculated = self.calculate(
                tender_id, request.model_dump(include=set(MeasurementInput.model_fields))
            )
            identifier, source_id, stamp = new_id(), new_id(), now()
            data = {
                **calculated,
                "scope_label": request.scope_label,
                "status": "proposed",
                "origin": origin,
                "reviewed_at": stamp if origin == "engineer" else None,
                "review_rationale": review_rationale,
                "run_id": run_id,
                "supporting_source_ids": [source["source_id"] for source in supporting],
                "supporting_sources": supporting,
            }
            source = calculated["source"]
            if self.page(tender_id, request.artifact_id, request.page) != source:
                raise ValueError(
                    "The source changed during measurement. Review the current drawing."
                )
            if origin == "agent" and self.supporting_sources(tender_id, request) != supporting:
                raise ValueError("The supporting evidence changed during measurement publication.")
            author_label = "Engineer-marked" if origin == "engineer" else "Agent-proposed, not engineer-reviewed"
            derived_text = (
                f"{author_label} measurement proposal: {request.scope_label}; "
                f"{data['quantity']} {data['unit']} on page {request.page}. "
                "This is a derived proposal, not text extracted from the drawing or a checked BOQ quantity. "
                + data["calculation"]
            )
            conn.execute(
                "INSERT INTO evidence(id,artifact_id,locator,text,page,kind,metadata_json) VALUES(?,?,?,?,?,?,?)",
                (
                    source_id,
                    request.artifact_id,
                    f"page:{request.page}/measurement:{identifier}",
                    derived_text,
                    request.page,
                    "measurement",
                    dump(
                        {
                            "origin": origin,
                            "measurement_id": identifier,
                            "status": "proposed",
                            "source_hash": source["content_hash"],
                            "source_version": source["version"],
                            "method": "engineer_marked_calibrated_proposal" if origin == "engineer" else "agent_proposed_calibrated_geometry",
                            "run_id": run_id,
                            "engineer_reviewed": origin == "engineer",
                            "supporting_source_ids": data["supporting_source_ids"],
                        }
                    ),
                ),
            )
            conn.execute(
                "INSERT INTO measurements VALUES(?,?,?,?,?,?)",
                (
                    identifier,
                    tender_id,
                    request.artifact_id,
                    source_id,
                    dump(
                        data
                        | {
                            "_evidence_fingerprint": _fingerprint(
                                _evidence_basis(self.repo.get_evidence(tender_id, source_id))
                            ),
                        }
                    ),
                    stamp,
                ),
            )
            if origin == "engineer":
                conn.execute(
                    "INSERT INTO decisions VALUES(?,?,?,?,?,?,?)",
                    (new_id(), tender_id, "measurement", identifier, "review_proposal", review_rationale, stamp),
                )
            self.repo.advance_retrieval_generation(tender_id, conn)
        self.repo.notify_retrieval_generation(tender_id)
        return self.get(tender_id, identifier)

    def _get(self, conn, tender_id, measurement_id):
        saved = record(
            conn.execute(
                "SELECT m.*,a.is_current,a.content_hash FROM measurements m JOIN artifacts a ON a.id=m.artifact_id WHERE m.id=? AND m.tender_id=?",
                (measurement_id, tender_id),
            ).fetchone()
        )
        reasons = []
        source = saved["data"]["source"]
        available = True
        if not saved["is_current"] or saved["content_hash"] != source["content_hash"]:
            reasons.append("source_revision_changed")
        try:
            path = self.repo.object_path(tender_id, saved["artifact_id"])
            with path.open("rb") as stream:
                if hashlib.file_digest(stream, "sha256").hexdigest() != source["content_hash"]:
                    reasons.append("source_hash_mismatch")
        except (OSError, ValueError):
            available = False
            reasons.append("source_unavailable")
        try:
            current_evidence = _fingerprint(
                _evidence_basis(self.repo.get_evidence(tender_id, saved["source_id"]))
            )
            if current_evidence != saved["data"].get("_evidence_fingerprint"):
                reasons.append("measurement_evidence_changed")
        except KeyError:
            reasons.append("measurement_evidence_unavailable")
        checked_support = set()
        for support in saved["data"].get("supporting_sources", []):
            try:
                evidence = self.repo.get_evidence(tender_id, support["source_id"])
                artifact = self.repo.get_artifact(tender_id, support["artifact_id"])
                if (
                    not artifact["is_current"] or artifact["version"] != support["version"]
                    or artifact["content_hash"] != support["content_hash"]
                    or _fingerprint(_evidence_basis(evidence)) != support["evidence_hash"]
                ):
                    reasons.append("supporting_evidence_changed")
                if support["artifact_id"] not in checked_support:
                    with self.repo.object_path(tender_id, support["artifact_id"]).open("rb") as stream:
                        if hashlib.file_digest(stream, "sha256").hexdigest() != support["content_hash"]:
                            reasons.append("supporting_source_hash_mismatch")
                    checked_support.add(support["artifact_id"])
            except (KeyError, ValueError, OSError):
                reasons.append("supporting_source_unavailable")
        reasons = list(dict.fromkeys(reasons))
        return {
            "id": saved["id"],
            "tender_id": tender_id,
            "source_id": saved["source_id"],
            "created_at": saved["created_at"],
            **{key: value for key, value in saved["data"].items() if not key.startswith("_")},
            "is_current": not reasons,
            "source_available": available,
            "stale_reasons": reasons,
            "links": [
                dict(row)
                for row in conn.execute(
                    "SELECT item_id,proposal_id,rationale,created_at FROM measurement_links WHERE measurement_id=? ORDER BY created_at,proposal_id",
                    (measurement_id,),
                )
            ],
        }

    def get(self, tender_id, measurement_id):
        with self.repo.db.connect() as conn:
            return self._get(conn, tender_id, measurement_id)

    def list(self, tender_id, *, artifact_id=None, offset=0, limit=50):
        self.repo.get_tender(tender_id)
        if artifact_id is not None:
            self.repo.get_artifact(tender_id, artifact_id)
        if (
            isinstance(offset, bool)
            or not isinstance(offset, int)
            or offset < 0
            or isinstance(limit, bool)
            or not isinstance(limit, int)
            or not 1 <= limit <= 100
        ):
            raise ValueError("Choose a nonnegative offset and a page size from 1 to 100.")
        with self.repo.db.connect() as conn:
            identifiers = [
                row[0]
                for row in conn.execute(
                    "SELECT id FROM measurements WHERE tender_id=? AND (? IS NULL OR artifact_id=?) ORDER BY created_at DESC,id DESC LIMIT ? OFFSET ?",
                    (tender_id, artifact_id, artifact_id, limit, offset),
                )
            ]
            return [self._get(conn, tender_id, identifier) for identifier in identifiers]

    def link(self, tender_id, measurement_id, values):
        request = MeasurementLink.model_validate(values)
        with self.repo.atomic() as conn:
            measurement = self._get(conn, tender_id, measurement_id)
            if not measurement["is_current"]:
                raise ValueError(
                    "This measurement's drawing or evidence changed or is unavailable. Measure the current source."
                )
            source = measurement["source"]
            current = self.page(tender_id, source["artifact_id"], source["page"])
            if (
                current["content_hash"] != source["content_hash"]
                or current["page_size"] != source["page_size"]
            ):
                raise ValueError("The source PDF has changed. Create a new measurement.")
            # Use the public view; EstimateService rechecks the active item during proposal creation.
            item = next(
                (
                    item
                    for item in self.estimates.view(tender_id)["items"]
                    if item["id"] == request.item_id
                ),
                None,
            )
            if item is None:
                raise KeyError("The BOQ item could not be found in the selected Tender.")
            if item["unit"].strip().casefold() not in COMPATIBLE_UNITS[measurement["unit"]]:
                raise ValueError(
                    f"The measured unit ({measurement['unit']}) is incompatible with the BOQ unit ({item['unit']})."
                )
            prior = conn.execute(
                "SELECT q.* FROM measurement_links l JOIN quantity_proposals q ON q.id=l.proposal_id WHERE l.measurement_id=? AND l.item_id=?",
                (measurement_id, request.item_id),
            ).fetchone()
            if prior:
                saved = record(prior)
                return QuantityProposal.model_validate(
                    {
                        **saved["data"],
                        **{key: saved[key] for key in ("id", "item_id", "status", "created_at")},
                    }
                ).model_dump()
            calibration = (
                f"Calibration: {measurement['calibration_metres']} m between {measurement['calibration_points']}. "
                if measurement["calibration_metres"] is not None
                else "Count of distinct engineer-marked positions; no scale applied. "
            )
            proposal = self.estimates.propose_quantity(
                tender_id,
                request.item_id,
                {
                    "engineer_confirmed": request.engineer_confirmed,
                    "rationale": request.rationale,
                    "quantity": measurement["quantity"],
                    "source_ids": list(dict.fromkeys([measurement["source_id"], item["source_id"], *measurement.get("supporting_source_ids", [])])),
                    "calculation": (
                        f"Measurement {measurement_id}; {measurement['scope_label']}; "
                        f"{source['relative_path']} v{source['version']}, page {source['page']}; SHA-256 {source['content_hash']}. "
                        + calibration
                        + measurement["calculation"]
                        + " "
                        + measurement["precision_note"]
                    ),
                },
            )
            conn.execute(
                "INSERT INTO measurement_links VALUES(?,?,?,?,?)",
                (measurement_id, request.item_id, proposal["id"], request.rationale, now()),
            )
            conn.execute(
                "INSERT INTO decisions VALUES(?,?,?,?,?,?,?)",
                (new_id(), tender_id, "measurement", measurement_id, "review_for_boq_proposal", request.rationale, now()),
            )
            conn.execute(
                "INSERT INTO measurement_link_bases VALUES(?,?)",
                (
                    proposal["id"],
                    dump(
                        {
                            "item": _fingerprint(_item_basis(item)),
                            "proposal": _fingerprint(_proposal_basis(proposal)),
                        }
                    ),
                ),
            )
            return proposal

    def validate_quantity_proposal(self, tender_id, proposal_id):
        with self.repo.db.connect() as conn:
            link = conn.execute(
                "SELECT l.measurement_id,b.basis_json,q.* FROM measurement_links l JOIN quantity_proposals q ON q.id=l.proposal_id LEFT JOIN measurement_link_bases b ON b.proposal_id=q.id WHERE q.id=? AND q.tender_id=?",
                (proposal_id, tender_id),
            ).fetchone()
            if link is None:
                return
            saved = record(link)
            measurement = self._get(conn, tender_id, saved["measurement_id"])
            if not measurement["is_current"]:
                raise ValueError(
                    "The measured drawing or evidence changed or is unavailable. Create a current measurement."
                )
            item = self.estimates._item(conn, tender_id, saved["item_id"])
            proposal = {**saved["data"], "id": saved["id"], "item_id": saved["item_id"]}
            basis = saved.get("basis") or {}
            if basis.get("item") != _fingerprint(_item_basis(item)) or basis.get(
                "proposal"
            ) != _fingerprint(_proposal_basis(proposal)):
                raise ValueError(
                    "The measurement quantity or BOQ basis changed. Review and link a new proposal."
                )
