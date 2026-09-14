"""Deterministic geometry for calibrated PDF drawing measurements.

Coordinates are fractions from the page's top-left. PDF dimensions are supplied
by the document service, never inferred from a screenshot or title-block scale.
"""

import hashlib
import math
from contextlib import closing
from decimal import Decimal, InvalidOperation, localcontext

import pypdfium2 as pdfium

from .documents import MAX_FILE_BYTES, _pdf_lock
from .measurement_models import MeasurementInput


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
    """Read drawing page geometry and calculate a measurement; nothing is stored."""

    def __init__(self, repo):
        self.repo = repo

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
