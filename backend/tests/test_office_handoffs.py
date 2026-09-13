"""Exact handoff transfer, staff reading, and desk listing regressions."""

from __future__ import annotations

import asyncio
import hashlib
import json

import pytest
from fastapi import FastAPI
from fastapi.testclient import TestClient
from test_staff_context import _build_staff_context
from test_staff_routing import _profile, _ready_binding_workspace, _work_order

from quantix.execution_context import engineer_identity
from quantix.manager_profile import ManagerProfileService
from quantix.office_handoff_models import TransferRequest
from quantix.office_handoffs import OfficeHandoffService
from quantix.office_tools import source_tools
from quantix.staff_assignments import StaffAssignmentService
from quantix.staff_models import ManagerCreationContext
from quantix.staff_routes import create_router
from quantix.staff_store import StaffStore
from quantix.tool_policy import ToolFenceError, dispatch


def _definition(name: str):
    return next(item for item in source_tools() if item.name == name)


def _pair(tmp_path, monkeypatch, *, tools=("read_handoff",)):
    repo, tender, _conn, _model, _route, envelope, routing, staff_a, root_context, _artifact = (
        _ready_binding_workspace(
            tmp_path,
            monkeypatch,
            requested_tool_ids=list(tools),
            envelope_tools=tools,
            max_staff=3,
            max_assignments=5,
        )
    )
    manager = ManagerProfileService(repo)
    planning = repo.create_run(tender["id"], "conversation", "Create second colleague")
    staff_b = StaffStore(repo).create_generated(
        ManagerCreationContext(
            tender["id"], planning["id"], manager.get().version, root_context.scope_id
        ),
        _profile(display_name="Second Colleague", requested_tool_ids=list(tools)),
        _work_order(),
        "create-second",
    )
    binding_b = routing.bind(
        root_context,
        staff_b.staff.id,
        staff_b.work_order.id,
        envelope.route_options[0].id,
        "bind-second",
    )
    assignments = StaffAssignmentService(repo)
    assignment_b = assignments.queue(root_context, binding_b.id, "assignment-second")
    assignment_b = assignments.start(tender["id"], assignment_b.id, assignment_b.revision)

    binding_a = routing.bind(
        root_context,
        staff_a.staff.id,
        staff_a.work_order.id,
        envelope.route_options[0].id,
        "bind-first",
    )
    assignment_a = assignments.queue(root_context, binding_a.id, "assignment-first")
    assignment_a = assignments.start(tender["id"], assignment_a.id, assignment_a.revision)
    return (
        repo,
        tender,
        (staff_a, binding_a, assignment_a.id),
        (staff_b, binding_b, assignment_b.id),
        (
            envelope,
            routing,
            root_context,
        ),
    )


def _transfer(repo, tender_id: str, sender_id: str, recipient_id: str, rows: int = 750):
    table = [
        {"row": index, "quantity": f"{index}.50", "unit": "m3", "formula": f"{index}*0.5"}
        for index in range(1, rows + 1)
    ]
    fingerprint = hashlib.sha256(json.dumps(table, separators=(",", ":")).encode()).hexdigest()
    service = OfficeHandoffService(repo)
    result_id = service.save_result_table(tender_id, sender_id, table, fingerprint=fingerprint)
    handoff = service.transfer_saved_result(
        engineer_identity(tender_id),
        TransferRequest(
            result_id=result_id,
            recipient_staff_id=recipient_id,
            purpose="Hand the measured table to the recipient.",
            expected_basis_fingerprint=fingerprint,
            idempotency_key="handoff-transfer",
        ),
    )
    return service, handoff, fingerprint


def test_recipient_staff_reads_exact_rows_without_union_into_source_reads(tmp_path, monkeypatch):
    repo, tender, sender, recipient, _extra = _pair(tmp_path, monkeypatch)
    staff_b, binding_b, assignment_b = recipient
    _service, handoff, _fingerprint = _transfer(
        repo, tender["id"], sender[0].staff.id, staff_b.staff.id
    )
    context = _build_staff_context(repo, binding_b.id, assignment_b)
    read = _definition("read_handoff")

    result = asyncio.run(
        dispatch(
            "direct",
            read,
            context,
            {"handoff_id": handoff.id, "offset": 700, "limit": 1},
            invocation_id="recipient-handoff-read",
        )
    )
    payload = json.loads(result)
    assert payload["items"] == [
        {"row": 701, "quantity": "701.50", "unit": "m3", "formula": "701*0.5"}
    ]
    assert payload["total_rows"] == 750
    assert payload["next_offset"] == 701
    assert payload["applicability"] == "current"
    # Receipt of handoff rows never marks a Tender source as inspected.
    assert context.seen_sources == set()
    with repo.db.connect() as conn:
        assert (
            conn.execute(
                "SELECT COUNT(*) FROM staff_source_receipts WHERE assignment_id=?", (assignment_b,)
            ).fetchone()[0]
            == 0
        )
        events = conn.execute(
            "SELECT kind,data_json FROM run_events WHERE run_id=? AND kind='handoff_inspected'",
            (context.run_id,),
        ).fetchall()
    assert len(events) == 1
    extent = json.loads(events[0]["data_json"])
    assert extent["handoff_id"] == handoff.id
    assert (extent["offset"], extent["limit"]) == (700, 1)


def test_unrelated_staff_and_ungranted_tool_cannot_read_a_handoff(tmp_path, monkeypatch):
    repo, tender, sender, recipient, (envelope, routing, root_context) = _pair(
        tmp_path, monkeypatch
    )
    staff_b, _binding_b, _assignment_b = recipient
    _service, handoff, _fingerprint = _transfer(
        repo, tender["id"], sender[0].staff.id, staff_b.staff.id
    )
    read = _definition("read_handoff")

    # Same-Tender outsider with the granted tool but no delivery relationship.
    manager = ManagerProfileService(repo)
    planning = repo.create_run(tender["id"], "conversation", "Create outsider colleague")
    outsider = StaffStore(repo).create_generated(
        ManagerCreationContext(
            tender["id"], planning["id"], manager.get().version, root_context.scope_id
        ),
        _profile(display_name="Outsider Colleague", requested_tool_ids=["read_handoff"]),
        _work_order(),
        "create-outsider",
    )
    outsider_binding = routing.bind(
        root_context,
        outsider.staff.id,
        outsider.work_order.id,
        envelope.route_options[0].id,
        "bind-outsider",
    )
    assignments = StaffAssignmentService(repo)
    outsider_assignment = assignments.queue(
        root_context, outsider_binding.id, "assignment-outsider"
    )
    outsider_assignment = assignments.start(
        tender["id"], outsider_assignment.id, outsider_assignment.revision
    )
    outsider_context = _build_staff_context(repo, outsider_binding.id, outsider_assignment.id)
    with pytest.raises(ToolFenceError):
        asyncio.run(
            dispatch(
                "direct",
                read,
                outsider_context,
                {"handoff_id": handoff.id, "offset": 0, "limit": 1},
                invocation_id="outsider-handoff-read",
            )
        )

    # The intended recipient without the granted capability is also blocked.
    ungranted = _build_staff_context(repo, recipient[1].id, recipient[2])
    ungranted.reviewed_tools = []
    with pytest.raises(ToolFenceError):
        asyncio.run(
            dispatch(
                "direct",
                read,
                ungranted,
                {"handoff_id": handoff.id, "offset": 0, "limit": 1},
                invocation_id="ungranted-handoff-read",
            )
        )


def _client(repo, *, token: str = "synthetic-handoff-token"):
    app = FastAPI()

    @app.middleware("http")
    async def auth(request, call_next):
        if (
            request.url.path.startswith("/api")
            and request.headers.get("authorization") != f"Bearer {token}"
        ):
            from fastapi.responses import JSONResponse

            return JSONResponse({"detail": "Not authorised"}, status_code=401)
        return await call_next(request)

    app.include_router(create_router(repo))
    return TestClient(app), token


def test_handoff_list_views_counterparts_and_rejects_bad_scope(tmp_path, monkeypatch):
    repo, tender, sender, recipient, _extra = _pair(tmp_path, monkeypatch)
    staff_a, _binding_a, _assignment_a = sender
    staff_b, _binding_b, _assignment_b = recipient
    _service, handoff, _fingerprint = _transfer(
        repo, tender["id"], staff_a.staff.id, staff_b.staff.id, rows=5
    )
    client, token = _client(repo)
    headers = {"Authorization": f"Bearer {token}"}

    received = client.get(
        f"/api/tenders/{tender['id']}/handoffs?staff_id={staff_b.staff.id}&direction=received",
        headers=headers,
    )
    assert received.status_code == 200
    body = received.json()
    assert body["total"] == 1
    assert body["items"][0]["handoff"]["id"] == handoff.id
    assert body["items"][0]["direction"] == "received"
    assert body["items"][0]["counterpart_staff_id"] == staff_a.staff.id
    assert staff_a.staff.display_name in body["items"][0]["counterpart_display_name"]

    sent = client.get(
        f"/api/tenders/{tender['id']}/handoffs?staff_id={staff_a.staff.id}&direction=sent",
        headers=headers,
    )
    assert sent.status_code == 200
    assert sent.json()["items"][0]["counterpart_staff_id"] == staff_b.staff.id

    other = repo.create_tender("Other Tender")
    assert (
        client.get(
            f"/api/tenders/{other['id']}/handoffs?staff_id={staff_b.staff.id}",
            headers=headers,
        ).status_code
        == 404
    )
    assert (
        client.get(
            f"/api/tenders/{tender['id']}/handoffs?staff_id={staff_b.staff.id}&direction=sideways",
            headers=headers,
        ).status_code
        == 422
    )
    assert (
        client.get(
            f"/api/tenders/{tender['id']}/handoffs?staff_id={staff_b.staff.id}&cursor=broken",
            headers=headers,
        ).status_code
        == 409
    )
    rows = client.get(
        f"/api/tenders/{tender['id']}/handoffs/{handoff.id}?offset=4&limit=1",
        headers=headers,
    )
    assert rows.status_code == 200
    assert rows.json()["items"] == [
        {"row": 5, "quantity": "5.50", "unit": "m3", "formula": "5*0.5"}
    ]
    assert rows.json()["total_rows"] == 5
