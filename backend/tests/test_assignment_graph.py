"""Work-graph persistence, revision retention, and node-status derivation."""

from __future__ import annotations

import pytest
from fastapi import FastAPI
from fastapi.testclient import TestClient

from quantix.assignment_graph import AssignmentGraphService, node_statuses
from quantix.assignment_graph_models import (
    GraphDraft,
    GraphRevisionRequest,
    WorkGraphVersion,
    WorkNode,
)
from quantix.execution_context import OfficeExecutionIdentity
from quantix.repository import Repository
from quantix.staff_models import OfficeConflict
from quantix.staff_routes import create_router


def _ctx(tender_id: str, root_run_id: str) -> OfficeExecutionIdentity:
    return OfficeExecutionIdentity(
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


def _nodes():
    return [
        {
            "key": "A",
            "brief": "Buy reinforcement now.",
            "expected_output": "Purchase list",
            "completion_criteria": "Every line has a supplier.",
            "owner_staff_id": "staff-a",
            "prerequisite_keys": [],
        },
        {
            "key": "B",
            "brief": "Schedule staged delivery.",
            "expected_output": "Delivery plan",
            "completion_criteria": "Dates cover every purchase line.",
            "owner_staff_id": "staff-b",
            "prerequisite_keys": ["A"],
        },
    ]


def _workspace(tmp_path):
    repo = Repository(tmp_path / "quantix")
    tender = repo.create_tender("Graph Tender")
    run = repo.create_run(tender["id"], "manager", "Graph work")
    return repo, tender["id"], run["id"]


def test_revision_preserves_completed_nodes_and_keeps_their_brief(tmp_path):
    repo, tender_id, root_id = _workspace(tmp_path)
    graphs = AssignmentGraphService(repo)
    first = graphs.propose_graph(
        _ctx(tender_id, root_id),
        GraphDraft(
            outcome="Deliver reinforcement.",
            root_run_id=root_id,
            nodes=_nodes(),
            idempotency_key="graph-propose",
        ),
    )
    assert first.revision == 1
    graphs.mark_completed(tender_id, root_id, "A")

    second = graphs.revise_graph(
        _ctx(tender_id, root_id),
        GraphRevisionRequest(
            expected_revision=1,
            root_run_id=root_id,
            reason="Split the delivery branch.",
            nodes=[
                {
                    "key": "A",
                    "brief": "Changed brief that must not win.",
                    "expected_output": "Purchase list",
                    "completion_criteria": "Every line has a supplier.",
                    "prerequisite_keys": [],
                },
                {
                    "key": "B",
                    "brief": "Schedule staged delivery in two drops.",
                    "expected_output": "Delivery plan",
                    "completion_criteria": "Dates cover every purchase line.",
                    "owner_staff_id": "staff-b",
                    "prerequisite_keys": ["A"],
                },
            ],
            idempotency_key="graph-revise",
        ),
    )
    assert second.revision == 2
    kept = next(item for item in second.nodes if item.key == "A")
    changed = next(item for item in second.nodes if item.key == "B")
    assert kept.state == "completed"
    assert kept.brief == "Buy reinforcement now."
    assert changed.brief == "Schedule staged delivery in two drops."
    assert changed.state in {"ready", "blocked"}


def test_node_statuses_distinguish_waiting_owner_and_failed(tmp_path):
    repo, tender_id, root_id = _workspace(tmp_path)
    graphs = AssignmentGraphService(repo)
    graphs.propose_graph(
        _ctx(tender_id, root_id),
        GraphDraft(
            outcome="Deliver reinforcement.",
            root_run_id=root_id,
            nodes=[
                {
                    "key": "A",
                    "brief": "Buy reinforcement now.",
                    "expected_output": "Purchase list",
                    "completion_criteria": "Every line has a supplier.",
                    "prerequisite_keys": [],
                },
                {
                    "key": "B",
                    "brief": "Schedule staged delivery.",
                    "expected_output": "Delivery plan",
                    "completion_criteria": "Dates cover every purchase line.",
                    "owner_staff_id": "staff-b",
                    "prerequisite_keys": ["A"],
                },
            ],
            idempotency_key="graph-status",
        ),
    )
    status = graphs.status(tender_id, root_id)
    assert status is not None
    derived = {item.key: item for item in status.nodes}
    assert derived["A"].derived == "needs_owner"
    assert "owner" in derived["A"].remedy
    assert derived["B"].derived == "waiting"
    assert derived["B"].waiting_on == ["A"]

    graphs.mark_completed(tender_id, root_id, "A")
    status = graphs.status(tender_id, root_id)
    assert status is not None
    derived = {item.key: item for item in status.nodes}
    assert derived["A"].derived == "completed"
    assert derived["B"].derived == "ready"

    failed_graph = WorkGraphVersion(
        id="graph-failed",
        root_id=root_id,
        revision=3,
        instruction_revision_id="instruction-1",
        nodes=[
            WorkNode(
                id="node-f",
                key="F",
                owner_staff_id="staff-f",
                expected_output="Attempted output",
                completion_criteria="Done once.",
                prerequisite_ids=[],
                state="failed",
                brief="A failed branch.",
            )
        ],
        edges=[],
        basis_fingerprint="fp",
        created_at="2026-09-10T00:00:00Z",
    )
    assert node_statuses(failed_graph)[0].derived == "needs_attempt"


def test_cycle_rejected_atomically_without_partial_graph(tmp_path):
    repo, tender_id, root_id = _workspace(tmp_path)
    graphs = AssignmentGraphService(repo)
    with pytest.raises(ValueError, match="cycle"):
        graphs.propose_graph(
            _ctx(tender_id, root_id),
            GraphDraft(
                outcome="Cyclic work.",
                root_run_id=root_id,
                nodes=[
                    {
                        "key": "A",
                        "brief": "First.",
                        "expected_output": "A done",
                        "completion_criteria": "A complete.",
                        "prerequisite_keys": ["B"],
                    },
                    {
                        "key": "B",
                        "brief": "Second.",
                        "expected_output": "B done",
                        "completion_criteria": "B complete.",
                        "prerequisite_keys": ["A"],
                    },
                ],
                idempotency_key="graph-cycle",
            ),
        )
    assert graphs.count(tender_id, root_id) == 0
    assert graphs.status(tender_id, root_id) is None


def test_latest_graph_route_is_tender_scoped(tmp_path):
    repo, tender_id, root_id = _workspace(tmp_path)
    graphs = AssignmentGraphService(repo)
    graphs.propose_graph(
        _ctx(tender_id, root_id),
        GraphDraft(
            outcome="Deliver reinforcement.",
            root_run_id=root_id,
            nodes=_nodes(),
            idempotency_key="graph-route",
        ),
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
        f"/api/tenders/{tender_id}/office/graphs/latest?root_run_id={root_id}",
        headers=headers,
    )
    assert response.status_code == 200
    body = response.json()
    assert body["graph"]["revision"] == 1
    assert {item["key"]: item["derived"] for item in body["nodes"]} == {
        "A": "ready",
        "B": "waiting",
    }

    assert (
        client.get(
            f"/api/tenders/{tender_id}/office/graphs/latest?root_run_id={'0' * 32}",
            headers=headers,
        ).status_code
        == 404
    )
    other = repo.create_tender("Other Tender")
    assert (
        client.get(
            f"/api/tenders/{other['id']}/office/graphs/latest?root_run_id={root_id}",
            headers=headers,
        ).status_code
        == 404
    )
    with pytest.raises(OfficeConflict):
        graphs.revise_graph(
            _ctx(tender_id, root_id),
            GraphRevisionRequest(
                expected_revision=99,
                root_run_id=root_id,
                reason="Stale revision.",
                nodes=_nodes(),
                idempotency_key="graph-stale",
            ),
        )
