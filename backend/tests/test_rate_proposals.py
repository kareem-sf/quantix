import sqlite3

import pytest
from fastapi import FastAPI
from fastapi.testclient import TestClient
from test_estimates import approval, rate, seed

from quantix.estimate_routes import create_router
from quantix.estimates import EstimateService
from quantix.repository import Repository


@pytest.fixture
def setup(tmp_path):
    repo = Repository(tmp_path)
    tid = repo.create_tender("Concrete tender")["id"]
    seed(repo, tid)
    service = EstimateService(repo)
    item = service.refresh(tid)["items"][0]
    return repo, tid, service, item


def proposal_payload():
    return {
        "unit_rate": "10.10",
        "currency": "EGP",
        "tax_basis": "excluding_vat",
        "vat_percent": "14",
        "provenance": {
            "basis": "estimated",
            "observed_on": "2026-09-06",
            "source_ids": [],
            "urls": [],
            "geography": "Cairo",
            "conditions": "Proposed allowance pending quotation.",
        },
    }


def test_rate_proposal_cannot_install_price_or_confirm_source_until_engineer_approval(setup):
    repo, tid, service, item = setup
    proposed = service.propose_rate(tid, item["id"], proposal_payload())
    assert proposed["status"] == "proposed"
    assert service.view(tid)["items"][0]["unit_rate"] is None
    assert service.view(tid)["items"][0]["effective_quantity"] == "12.5"
    with pytest.raises(ValueError):
        service.approve_rate(
            tid, proposed["id"], {"engineer_confirmed": False, "rationale": "AI approval"}
        )
    accepted = service.approve_rate(tid, proposed["id"], approval(confirm_source=True))
    installed = service.view(tid)["items"][0]
    assert accepted["status"] == "approved"
    assert accepted["is_current"] is True
    assert installed["line_inc_vat"] == "143.93"
    assert installed["supplied_quantity"] == "12.5"
    with pytest.raises(ValueError):
        service.approve_rate(tid, proposed["id"], approval(confirm_source=True))


def test_changed_item_cannot_be_silently_overwritten_by_pending_rate(setup):
    repo, tid, service, item = setup
    proposed = service.propose_rate(tid, item["id"], proposal_payload())
    service.update_item(tid, item["id"], rate(vat_percent="0") | {"unit_rate": "25"})
    assert service.list_rate_proposals(tid)[0]["is_current"] is False
    with pytest.raises(ValueError, match="changed|current"):
        service.approve_rate(tid, proposed["id"], approval())
    assert service.view(tid)["items"][0]["unit_rate"] == "25"


def test_rate_proposal_payload_is_immutable_and_rejects_approval_or_quantity_fields(setup):
    repo, tid, service, item = setup
    for extra in [{"engineer_confirmed": True}, {"confirm_source": True}, {"quantity": "99"}]:
        with pytest.raises(ValueError):
            service.propose_rate(tid, item["id"], proposal_payload() | extra)
    proposed = service.propose_rate(tid, item["id"], proposal_payload())
    with repo.db.connect(write=True) as conn:
        with pytest.raises(sqlite3.IntegrityError):
            conn.execute(
                "UPDATE rate_proposals SET payload_json='{}' WHERE id=?", (proposed["id"],)
            )


def test_rate_proposal_scope_and_source_revision_are_checked(setup):
    repo, tid, service, item = setup
    other = repo.create_tender("Other tender")["id"]
    with pytest.raises((ValueError, KeyError)):
        service.propose_rate(other, item["id"], proposal_payload())
    proposed = service.propose_rate(tid, item["id"], proposal_payload())
    seed(repo, tid, digest="b" * 64)
    service.refresh(tid)
    with pytest.raises((ValueError, KeyError)):
        service.approve_rate(tid, proposed["id"], approval())


def test_rate_proposal_routes_return_typed_engineer_decisions(setup):
    repo, tid, service, item = setup
    proposed = service.propose_rate(tid, item["id"], proposal_payload())
    app = FastAPI()
    app.include_router(create_router(repo))
    client = TestClient(app)
    assert (
        client.get(f"/api/tenders/{tid}/estimate/rate-proposals").json()[0]["id"] == proposed["id"]
    )
    response = client.post(
        f"/api/tenders/{tid}/estimate/rate-proposals/{proposed['id']}/approve",
        json=approval(confirm_source=True),
    )
    assert response.status_code == 200
    assert response.json()["status"] == "approved"


def test_measurement_source_revision_invalidates_the_rate_item_basis(setup):
    repo, tid, service, item = setup
    drawing = {
        "kind": "pdf",
        "status": "extracted",
        "segments": [{"locator": "page:1", "text": "Footing geometry"}],
    }
    artifact, _ = repo.register_artifact(tid, "Drawing.pdf", "c" * 64, 100, drawing)
    source_id = repo.artifact_evidence(tid, artifact["id"])[0]["id"]
    measured = service.propose_quantity(
        tid, item["id"], approval(quantity="15", calculation="5 x 3 x 1", source_ids=[source_id])
    )
    service.approve_quantity(tid, measured["id"], approval())
    proposed = service.propose_rate(tid, item["id"], proposal_payload())
    repo.register_artifact(tid, "Drawing.pdf", "d" * 64, 100, drawing)
    with pytest.raises(ValueError, match="changed|current"):
        service.approve_rate(tid, proposed["id"], approval(confirm_source=True))
