"""Saved calculations expose exact inputs without crossing Tender scope."""

import json

import pytest

from quantix.calculation_models import CalculationCheckRequest, CalculationRequest
from quantix.calculations import CalculationService
from quantix.db import dump, new_id, now
from quantix.execution_context import engineer_identity
from quantix.repository import Repository


def test_reads_exact_tender_calculation_and_preserves_assumptions(tmp_path):
    repo = Repository(tmp_path)
    tender = repo.create_tender("Synthetic calculation inspector")
    other = repo.create_tender("Other Tender")
    service = CalculationService(repo)
    saved = service.calculate(
        engineer_identity(tender["id"]),
        CalculationRequest(
            method_id="product",
            method_version="1",
            inputs={"quantity": "12.5", "factor": "8"},
            units={"quantity": "m", "factor": "m"},
            assumptions=["Synthetic wastage excluded."],
            precision="0.01",
            idempotency_key="calculation-read",
        ),
    )

    read = service.get(tender["id"], saved.id)
    assert read == saved
    assert read.assumptions == ["Synthetic wastage excluded."]
    with repo.db.connect() as conn:
        request = json.loads(
            conn.execute(
                "SELECT request_json FROM office_calculations WHERE id=?", (saved.id,)
            ).fetchone()[0]
        )
    assert request["assumptions"] == read.assumptions
    with pytest.raises(KeyError, match="selected Tender"):
        service.get(other["id"], saved.id)


def test_legacy_calculation_marks_unrecorded_assumptions_unknown(tmp_path):
    repo = Repository(tmp_path)
    tender = repo.create_tender("Synthetic legacy calculation")
    service = CalculationService(repo)
    identifier = new_id()
    request = {
        "engine_version": "pint-0.26.1-v2",
        "method_id": "sum",
        "method_version": "1",
        "inputs": {"values": ["1", "2"]},
        "units": {"quantity": "m"},
        "precision": "0.01",
        "rounding": "HALF_UP",
    }
    with repo.atomic() as conn:
        conn.execute(
            "INSERT INTO office_calculations VALUES(?,?,?,?,?,?,?,?,?,?)",
            (
                identifier,
                tender["id"],
                "sum",
                "1",
                "a" * 64,
                dump(request),
                dump({"sum": "3.00", "unit": "m"}),
                "b" * 64,
                "calculated",
                now(),
            ),
        )

    read = service.get(tender["id"], identifier)
    assert read.assumptions == []
    assert read.limitations == [
        "This legacy calculation did not record its assumptions. Treat them as unknown."
    ]
    checked = service.check(
        engineer_identity(tender["id"]),
        CalculationCheckRequest(
            calculation_id=identifier,
            method_id="sum",
            method_version="1",
        ),
    )
    assert checked["reproducible"] is False
    assert checked["record"].limitations == read.limitations
