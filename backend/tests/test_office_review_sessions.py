"""Review discussion, resolution, and the review-room API surface."""

from __future__ import annotations

import pytest
from fastapi import FastAPI
from fastapi.testclient import TestClient

from quantix.calculation_models import CalculationRequest
from quantix.calculations import CalculationService
from quantix.execution_context import engineer_identity
from quantix.later_routes import create_router
from quantix.office_reviews import (
    OfficeReviewService,
    ReviewContribution,
    ReviewDraft,
    ReviewResolution,
)
from quantix.repository import Repository
from quantix.staff_models import OfficeConflict


def _session(repo, tender_id: str):
    identity = engineer_identity(tender_id)
    calculation = CalculationService(repo).calculate(
        identity,
        CalculationRequest(
            method_id="product",
            method_version="synthetic-area-v1",
            inputs={"quantity": "10", "factor": "10"},
            units={"quantity": "m", "factor": "m"},
            idempotency_key="session-inputs",
        ),
    )
    review = OfficeReviewService(repo).start(
        identity,
        ReviewDraft(
            subject_id="estimate-1",
            workings="Check the saved dimensions; the supplied author narrative is not evidence.",
            calculation_id=calculation.id,
            hide_author_conclusion=True,
            idempotency_key="session-review",
        ),
        author_total="110.00",
    )
    assert review.status == "checked"
    return review


def test_agreement_and_disagreement_stay_separate_findings(tmp_path):
    repo = Repository(tmp_path / "quantix")
    tender = repo.create_tender("Review session Tender")
    service = OfficeReviewService(repo)
    review = _session(repo, tender["id"])

    area = service.contribute(
        engineer_identity(tender["id"]),
        review.id,
        ReviewContribution(
            topic="measured area",
            agreed=True,
            detail="The 100 m2 area reproduces from the saved dimensions.",
            idempotency_key="contribute-area",
        ),
    )
    assert [finding.topic for finding in area.findings] == ["calculation", "measured area"]
    rule = service.contribute(
        engineer_identity(tender["id"]),
        review.id,
        ReviewContribution(
            topic="deduction rule",
            agreed=False,
            detail="The deduction rule in the workings is not an approved measurement rule.",
            idempotency_key="contribute-rule",
        ),
    )
    by_topic = {finding.topic: finding for finding in rule.findings}
    assert by_topic["measured area"].agreed is True
    assert by_topic["deduction rule"].agreed is False
    replay = service.contribute(
        engineer_identity(tender["id"]),
        review.id,
        ReviewContribution(
            topic="deduction rule",
            agreed=False,
            detail="The deduction rule in the workings is not an approved measurement rule.",
            idempotency_key="contribute-rule",
        ),
    )
    assert len(replay.findings) == 3


def test_resolution_closes_discussion_without_authorizing_commercial_change(tmp_path):
    repo = Repository(tmp_path / "quantix")
    tender = repo.create_tender("Review resolution Tender")
    service = OfficeReviewService(repo)
    review = _session(repo, tender["id"])
    before_decisions = repo.list_findings(tender["id"])

    resolved = service.resolve(
        engineer_identity(tender["id"]),
        review.id,
        ReviewResolution(
            resolution="Deduction rule needs an engineer decision before pricing.",
            idempotency_key="resolve-once",
        ),
    )
    assert resolved.resolution is not None
    assert resolved.resolved_at is not None
    assert resolved.status == "checked"
    assert repo.list_findings(tender["id"]) == before_decisions
    with repo.db.connect() as conn:
        assert (
            conn.execute(
                "SELECT COUNT(*) FROM decisions WHERE tender_id=?", (tender["id"],)
            ).fetchone()[0]
            == 0
        )

    replay = service.resolve(
        engineer_identity(tender["id"]),
        review.id,
        ReviewResolution(
            resolution="Deduction rule needs an engineer decision before pricing.",
            idempotency_key="resolve-once",
        ),
    )
    assert replay.resolution == resolved.resolution
    with pytest.raises(OfficeConflict):
        service.contribute(
            engineer_identity(tender["id"]),
            review.id,
            ReviewContribution(
                topic="late note",
                agreed=True,
                detail="Too late.",
                idempotency_key="contribute-late",
            ),
        )


def test_review_routes_are_tender_scoped(tmp_path):
    repo = Repository(tmp_path / "quantix")
    tender = repo.create_tender("Review route Tender")
    other = repo.create_tender("Other Tender")

    app = FastAPI()

    @app.middleware("http")
    async def auth(request, call_next):
        if request.headers.get("authorization") != "Bearer synthetic-token":
            from fastapi.responses import JSONResponse

            return JSONResponse({"detail": "Not authorised"}, status_code=401)
        return await call_next(request)

    app.include_router(create_router(repo))
    client = TestClient(app)
    headers = {"Authorization": "Bearer synthetic-token"}

    started = client.post(
        f"/api/tenders/{tender['id']}/reviews",
        headers=headers,
        json={
            "subject_id": "estimate-1",
            "workings": "Area deduction allowance 110.00.",
            "hide_author_conclusion": True,
            "idempotency_key": "route-review",
        },
    )
    assert started.status_code == 200, started.text
    assert started.json()["status"] == "incomplete"
    session_id = started.json()["id"]

    listed = client.get(f"/api/tenders/{tender['id']}/reviews?status=incomplete", headers=headers)
    assert listed.status_code == 200
    assert [item["id"] for item in listed.json()] == [session_id]
    assert (
        client.get(f"/api/tenders/{other['id']}/reviews/{session_id}", headers=headers).status_code
        == 404
    )

    resolved = client.post(
        f"/api/tenders/{tender['id']}/reviews/{session_id}/resolve",
        headers=headers,
        json={"resolution": "Needs an engineer decision.", "idempotency_key": "route-resolve"},
    )
    assert resolved.status_code == 200
    assert resolved.json()["resolution"] == "Needs an engineer decision."
