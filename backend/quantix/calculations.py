"""Deterministic Decimal calculations with recorded method, units and rounding."""

from __future__ import annotations

import hashlib
import json
from decimal import ROUND_HALF_UP, Decimal, localcontext

from .calculation_models import CalculationCheckRequest, CalculationRecord, CalculationRequest
from .db import dump, new_id, now
from .execution_context import OfficeExecutionIdentity
from .units import ARITHMETIC_PRECISION, DimensionError, add, convert, multiply, quantity, subtract

_SCHEMA = (
    """
    CREATE TABLE IF NOT EXISTS office_calculations (
        id TEXT PRIMARY KEY,
        tender_id TEXT,
        method_id TEXT NOT NULL,
        method_version TEXT NOT NULL,
        formula_hash TEXT NOT NULL,
        request_json TEXT NOT NULL,
        outputs_json TEXT NOT NULL,
        basis_fingerprint TEXT NOT NULL,
        status TEXT NOT NULL,
        created_at TEXT NOT NULL
    )
    """,
)


def _fingerprint(payload: dict) -> str:
    canonical = json.dumps(payload, ensure_ascii=False, separators=(",", ":"), sort_keys=True)
    return hashlib.sha256(canonical.encode("utf-8")).hexdigest()


def _quantize(value: Decimal, precision: str) -> Decimal:
    with localcontext() as ctx:
        ctx.prec = ARITHMETIC_PRECISION
        ctx.rounding = ROUND_HALF_UP
        return value.quantize(Decimal(precision))


class CalculationService:
    def __init__(self, repo):
        self.repo = repo
        with repo.atomic() as conn:
            for statement in _SCHEMA:
                conn.execute(statement)

    def calculate(
        self, ctx: OfficeExecutionIdentity, request: CalculationRequest
    ) -> CalculationRecord:
        formula = {
            "engine_version": "pint-0.26.1-v3",
            "method_id": request.method_id,
            "method_version": request.method_version,
            "inputs": request.inputs,
            "units": request.units,
            "precision": request.precision,
            "rounding": request.rounding,
        }
        formula_hash = _fingerprint(formula)
        outputs = {}
        checks = []
        dimension_error = False
        status = "calculated"
        try:
            if request.method_id in {"product", "emission_factor", "unit_calculation"}:
                left = quantity(request.inputs["quantity"], request.units.get("quantity", "1"))
                right = quantity(request.inputs["factor"], request.units.get("factor", "1"))
                result = multiply(left, right)
                outputs = {
                    "product": str(_quantize(result.value, request.precision)),
                    "unit": result.unit,
                }
            elif request.method_id == "sum":
                total = quantity("0", request.units.get("quantity", "1"))
                for item in request.inputs["values"]:
                    total = add(total, quantity(item, request.units.get("quantity", "1")))
                outputs = {
                    "sum": str(_quantize(total.value, request.precision)),
                    "unit": total.unit,
                }
            elif request.method_id == "difference":
                left = quantity(request.inputs["left"], request.units.get("left", "1"))
                right = quantity(request.inputs["right"], request.units.get("right", "1"))
                result = subtract(left, right)
                outputs = {
                    "difference": str(_quantize(result.value, request.precision)),
                    "unit": result.unit,
                }
            elif request.method_id == "add_units":
                left = quantity(request.inputs["left"], request.units["left"])
                right = quantity(request.inputs["right"], request.units["right"])
                result = add(left, right)
                outputs = {
                    "sum": str(_quantize(result.value, request.precision)),
                    "unit": result.unit,
                }
            elif request.method_id == "convert_unit":
                result = convert(
                    quantity(request.inputs["value"], request.units["from"]), request.units["to"]
                )
                outputs = {
                    "value": str(_quantize(result.value, request.precision)),
                    "unit": result.unit,
                }
            else:
                raise ValueError("That calculation method is not installed.")
        except DimensionError:
            dimension_error = True
            status = "invalid"
            outputs = {}
        basis = _fingerprint({**formula, "outputs": outputs, "assumptions": request.assumptions})
        identifier, stamp = new_id(), now()
        with self.repo.atomic() as conn:
            conn.execute(
                """
                INSERT INTO office_calculations(
                    id,tender_id,method_id,method_version,formula_hash,request_json,outputs_json,
                    basis_fingerprint,status,created_at
                ) VALUES(?,?,?,?,?,?,?,?,?,?)
                """,
                (
                    identifier,
                    ctx.tender_id,
                    request.method_id,
                    request.method_version,
                    formula_hash,
                    dump({**formula, "assumptions": request.assumptions}),
                    dump(outputs),
                    basis,
                    status,
                    stamp,
                ),
            )
        return CalculationRecord(
            id=identifier,
            tender_id=ctx.tender_id,
            method_id=request.method_id,
            method_version=request.method_version,
            formula_hash=formula_hash,
            typed_inputs=request.inputs,
            units=request.units,
            assumptions=request.assumptions,
            precision=request.precision,
            rounding=request.rounding,
            outputs=outputs,
            checks=checks,
            basis_fingerprint=basis,
            status=status,
            dimension_error=dimension_error,
            created_at=stamp,
        )

    def get(self, tender_id: str, calculation_id: str) -> CalculationRecord:
        self.repo.get_tender(tender_id)
        with self.repo.db.connect() as conn:
            row = conn.execute(
                """
                SELECT * FROM office_calculations
                WHERE tender_id=? AND id=?
                """,
                (tender_id, calculation_id),
            ).fetchone()
        if row is None:
            raise KeyError("This calculation is not in the selected Tender.")
        request = json.loads(row["request_json"])
        limitations = []
        if "assumptions" not in request:
            limitations.append(
                "This legacy calculation did not record its assumptions. Treat them as unknown."
            )
        return CalculationRecord(
            id=row["id"],
            tender_id=row["tender_id"],
            method_id=row["method_id"],
            method_version=row["method_version"],
            formula_hash=row["formula_hash"],
            typed_inputs=request["inputs"],
            units=request["units"],
            assumptions=request.get("assumptions", []),
            precision=request.get("precision", "0.01"),
            rounding=request.get("rounding", "HALF_UP"),
            outputs=json.loads(row["outputs_json"]),
            checks=[],
            basis_fingerprint=row["basis_fingerprint"],
            status=row["status"],
            dimension_error=row["status"] == "invalid",
            limitations=limitations,
            created_at=row["created_at"],
        )

    def check(self, ctx: OfficeExecutionIdentity, request: CalculationCheckRequest) -> dict:
        with self.repo.atomic() as conn:
            row = conn.execute(
                "SELECT * FROM office_calculations WHERE id=? AND (tender_id=? OR tender_id IS NULL)",
                (request.calculation_id, ctx.tender_id),
            ).fetchone()
        if row is None:
            raise KeyError("This calculation is not in the selected Tender.")
        original = json.loads(row["request_json"])
        assumptions_recorded = "assumptions" in original
        replay = self.calculate(
            ctx,
            CalculationRequest(
                method_id=original["method_id"],
                method_version=original["method_version"],
                inputs=original["inputs"],
                units=original["units"],
                assumptions=original.get("assumptions", []),
                precision=original["precision"],
                rounding=original["rounding"],
                idempotency_key=f"check-{request.calculation_id}",
            ),
        )
        if not assumptions_recorded:
            replay = replay.model_copy(
                update={
                    "limitations": [
                        "This legacy calculation did not record its assumptions. Treat them as unknown."
                    ]
                }
            )
        return {
            "reproducible": assumptions_recorded
            and row["status"] == "calculated"
            and replay.status == "calculated"
            and not replay.dimension_error
            and replay.outputs == json.loads(row["outputs_json"])
            and replay.formula_hash == row["formula_hash"],
            "record": replay,
        }
