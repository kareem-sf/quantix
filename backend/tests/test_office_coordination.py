"""Required-response persistence and durable delivery-state reads."""

from __future__ import annotations

import pytest
from agentic.fixtures import prepare_busy_staff
from fastapi import FastAPI
from fastapi.testclient import TestClient

from quantix.execution_context import OfficeExecutionIdentity
from quantix.office_coordination import OfficeCoordinationService
from quantix.office_delivery_models import CoordinationRequest, ReceiptRequest
from quantix.staff_routes import create_router


def _deliver(service, tender_id, root_run_id, staff_id, assignment_id, key, response=None):
    identity = OfficeExecutionIdentity(
        tender_id=tender_id,
        actor_kind="engineer",
        actor_id="engineer",
        root_run_id=root_run_id,
        budget_scope_id=None,
        assignment_id=None,
        profile_version=None,
        route_binding_id=None,
        instruction_revision_id=None,
        grant_fingerprint=None,
        ownership_epoch=None,
        trusted_invocation_id=None,
    )
    return service.deliver(
        identity,
        CoordinationRequest(
            kind="question",
            text="Is the concrete specified as C30/37?",
            root_run_id=root_run_id,
            recipient_staff_id=staff_id,
            recipient_assignment_id=assignment_id,
            source_ids=[],
            source_versions=[1],
            required_response=response,
            idempotency_key=key,
        ),
    )


def test_required_response_survives_delivery_acknowledge_and_replay(tmp_path, monkeypatch):
    from quantix.repository import Repository

    monkeypatch.setattr(
        "quantix.ai_direct.direct_runtime_status",
        lambda _: {
            "state": "ready",
            "version": "synthetic",
            "component_id": "direct-api",
            "detail": "Ready",
            "progress": 100,
        },
    )
    repo = Repository(tmp_path / "quantix")
    tender = repo.create_tender("Coordination Tender")
    staff, assignment, context = prepare_busy_staff(repo, tender["id"])
    service = OfficeCoordinationService(repo)

    receipt = _deliver(
        service,
        tender["id"],
        context.run_id,
        staff.staff.id,
        assignment.id,
        "coordination-deliver",
        response="Confirm the curing time.",
    )
    assert receipt.required_response == "Confirm the curing time."

    replay = _deliver(
        service,
        tender["id"],
        context.run_id,
        staff.staff.id,
        assignment.id,
        "coordination-deliver",
        response="Confirm the curing time.",
    )
    assert replay.replayed is True
    assert replay.required_response == "Confirm the curing time."

    ack = service.acknowledge(
        OfficeExecutionIdentity(
            tender_id=tender["id"],
            actor_kind="engineer",
            actor_id="engineer",
            root_run_id=None,
            budget_scope_id=None,
            assignment_id=None,
            profile_version=None,
            route_binding_id=None,
            instruction_revision_id=None,
            grant_fingerprint=None,
            ownership_epoch=None,
            trusted_invocation_id=None,
        ),
        ReceiptRequest(
            message_id=receipt.message_id,
            recipient_assignment_id=assignment.id,
            extent="received",
            idempotency_key="coordination-ack",
        ),
    )
    assert ack.state == "acknowledged"
    assert ack.required_response == "Confirm the curing time."

    latest = service.delivery_for(tender["id"], receipt.message_id)
    assert latest.state == "acknowledged"
    assert latest.required_response == "Confirm the curing time."

    with pytest.raises(KeyError):
        service.delivery_for(tender["id"], "0" * 32)

    page = service.messages.page(tender["id"], limit=50)
    delivered = next(item for item in page.items if item.id == receipt.message_id)
    assert delivered.delivery_state == "acknowledged"
    assert delivered.required_response == "Confirm the curing time."


def test_delivery_state_route_is_tender_scoped(tmp_path, monkeypatch):
    from quantix.repository import Repository

    monkeypatch.setattr(
        "quantix.ai_direct.direct_runtime_status",
        lambda _: {
            "state": "ready",
            "version": "synthetic",
            "component_id": "direct-api",
            "detail": "Ready",
            "progress": 100,
        },
    )
    repo = Repository(tmp_path / "quantix")
    tender = repo.create_tender("Coordination Route Tender")
    staff, assignment, context = prepare_busy_staff(repo, tender["id"])
    service = OfficeCoordinationService(repo)
    receipt = _deliver(
        service, tender["id"], context.run_id, staff.staff.id, assignment.id, "coordination-route"
    )

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

    response = client.get(
        f"/api/tenders/{tender['id']}/office/messages/{receipt.message_id}/delivery",
        headers=headers,
    )
    assert response.status_code == 200
    assert response.json()["state"] == "delivered"

    other = repo.create_tender("Other Tender")
    assert (
        client.get(
            f"/api/tenders/{other['id']}/office/messages/{receipt.message_id}/delivery",
            headers=headers,
        ).status_code
        == 404
    )
