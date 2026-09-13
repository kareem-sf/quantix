"""Focused API and read-projection checks for the live Tender Office."""

from __future__ import annotations

import pytest
from fastapi import FastAPI
from fastapi.testclient import TestClient

from quantix.execution_context import engineer_identity
from quantix.manager_profile import ManagerProfileService
from quantix.manager_runtime import ManagerRunProfiles
from quantix.office_events import OfficeEventService
from quantix.office_messages import OfficeMessageService
from quantix.office_read import OfficeReadService
from quantix.office_tools import OfficeContext
from quantix.repository import Repository
from quantix.staff_models import (
    ManagerCreationContext,
    Personality,
    StaffProfileDraft,
    StaffWorkOrder,
)
from quantix.staff_notebook_models import NotebookEntryDraft, NotebookQuery
from quantix.staff_notebooks import StaffNotebookService
from quantix.staff_routes import create_router
from quantix.staff_store import StaffStore


@pytest.fixture
def workspace(tmp_path):
    repo = Repository(tmp_path / "quantix")
    manager = ManagerProfileService(repo)
    tender = repo.create_tender("Synthetic office Tender")
    other = repo.create_tender("Other office Tender")
    return repo, manager, tender["id"], other["id"]


def _profile(name: str) -> StaffProfileDraft:
    personality = Personality(
        description="A careful construction colleague.",
        traits=["careful", "direct"],
        communication_style="State the evidence and next action.",
        problem_solving_style="Break work into bounded checks.",
        collaboration_style="Keep ownership visible.",
        uncertainty_handling="Label missing evidence.",
        initiative="Propose a safe next step.",
        explanation_style="Lead with the answer and source references.",
        language_preferences=["English"],
        working_habits=["Keep source references with findings."],
    )
    return StaffProfileDraft(
        display_name=name,
        role="Package evidence analyst",
        title="Package Evidence Analyst",
        specialisms=["Package reconciliation"],
        persona="A practical analyst for tender evidence.",
        personality=personality,
        responsibilities=["Check received package records."],
        objectives=["Give the Manager a traceable gap list."],
        methods=["Compare records and cite exact source rows."],
        deliverables=["A source-backed gap list."],
        success_criteria=["Each gap has an evidence reference."],
        context_needs=["Current package sources."],
        requested_tool_ids=["read_source"],
        creation_reason="Created to reconcile the package for this task.",
    )


def _order() -> StaffWorkOrder:
    return StaffWorkOrder(
        brief="Review the package and identify material evidence gaps.",
        goal="Give the Tender Manager a source-backed gap list.",
        source_ids=[],
        expected_outputs=["Source-backed gap list"],
        completion_checks=["Each gap has a source or explicit missing-source note"],
    )


def _staff(repo, manager, tender_id: str, name: str, key: str):
    run = repo.create_run(tender_id, "conversation", "Create a synthetic colleague")
    receipt = StaffStore(repo).create_generated(
        ManagerCreationContext(tender_id, run["id"], manager.get().version, f"scope-{key}"),
        _profile(name),
        _order(),
        key,
    )
    return receipt


def _client(repo, *, token: str = "synthetic-token"):
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


def test_fresh_snapshot_has_only_manager_and_reads_do_not_create_records(workspace):
    repo, _manager, tender_id, _other_id = workspace
    reader = OfficeReadService(repo)
    with repo.db.connect() as conn:
        before = {
            table: conn.execute(f"SELECT COUNT(*) FROM {table}").fetchone()[0]
            for table in ("office_staff", "office_assignments", "office_events", "office_messages")
        }
    snapshot = reader.snapshot(tender_id)
    assert snapshot.manager.display_name == "Tender Manager"
    assert snapshot.staff == []
    assert snapshot.assignments == []
    assert snapshot.messages.items == []
    assert snapshot.sequence == 0
    with repo.db.connect() as conn:
        after = {
            table: conn.execute(f"SELECT COUNT(*) FROM {table}").fetchone()[0] for table in before
        }
    assert after == before


def test_two_tenders_are_isolated_and_desk_matches_the_store(workspace):
    repo, manager, tender_id, other_id = workspace
    first = _staff(repo, manager, tender_id, "Amina Hassan", "a")
    second = _staff(repo, manager, other_id, "Omar Khalil", "b")
    reader = OfficeReadService(repo)

    assert [item.id for item in reader.snapshot(tender_id).staff] == [first.staff.id]
    assert [item.id for item in reader.snapshot(other_id).staff] == [second.staff.id]
    desk = reader.staff_desk(tender_id, first.staff.id)
    assert desk.profile == first.staff
    assert desk.version_numbers == [1]
    assert desk.work_orders == [first.work_order]
    with pytest.raises(KeyError):
        reader.staff_desk(tender_id, second.staff.id)


def test_staff_cursor_is_tender_scoped_and_survives_append(workspace):
    repo, manager, tender_id, other_id = workspace
    first = _staff(repo, manager, tender_id, "First colleague", "first")
    reader = OfficeReadService(repo)
    first_page = reader.staff_page(tender_id, limit=1)
    assert first_page.items[0].id == first.staff.id
    assert first_page.next_cursor is None

    second = _staff(repo, manager, tender_id, "Second colleague", "second")
    first_page = reader.staff_page(tender_id, limit=1)
    assert first_page.next_cursor
    next_page = reader.staff_page(tender_id, cursor=first_page.next_cursor, limit=1)
    assert [item.id for item in next_page.items] == [second.staff.id]
    with pytest.raises(ValueError, match="Tender"):
        reader.staff_page(other_id, cursor=first_page.next_cursor, limit=1)


def test_snapshot_cursor_and_duplicate_event_pages_are_consistent(workspace):
    repo, _manager, tender_id, _other_id = workspace
    events = OfficeEventService(repo)
    first = events.append(tender_id, "scope_changed", payload={"revision": 1})
    reader = OfficeReadService(repo)
    snapshot = reader.snapshot(tender_id)
    assert snapshot.cursor == first.event_id
    assert snapshot.sequence == 1
    second = events.append(tender_id, "assignment_waiting", payload={"detail": "synthetic"})
    page = reader.events_page(tender_id, after=snapshot.cursor, limit=1)
    repeat = reader.events_page(tender_id, after=snapshot.cursor, limit=1)
    assert [item.event_id for item in page.items] == [second.event_id]
    assert repeat == page
    assert page.instance_id == snapshot.instance_id


def test_events_unknown_cursor_requires_snapshot_and_foreign_cursor_is_rejected(workspace):
    repo, _manager, tender_id, other_id = workspace
    events = OfficeEventService(repo)
    foreign = events.append(other_id, "scope_changed")
    reader = OfficeReadService(repo)
    with pytest.raises(ValueError, match="Tender"):
        reader.events_page(tender_id, after=foreign.event_id)
    reset = reader.events_page(tender_id, after="missing-event")
    assert reset.reset_required is True
    assert reset.items == []


def test_router_auth_foreign_ids_and_bounds(workspace):
    repo, manager, tender_id, other_id = workspace
    staff = _staff(repo, manager, tender_id, "Route colleague", "route")
    client, token = _client(repo)
    assert client.get(f"/api/tenders/{tender_id}/office").status_code == 401
    headers = {"Authorization": f"Bearer {token}"}
    response = client.get(f"/api/tenders/{other_id}/staff/{staff.staff.id}", headers=headers)
    assert response.status_code == 404
    response = client.get(f"/api/tenders/{tender_id}/staff?limit=0", headers=headers)
    assert response.status_code == 422
    response = client.get(f"/api/tenders/{tender_id}/office/events?limit=101", headers=headers)
    assert response.status_code == 422


def test_notebook_views_filter_kind_current_and_page_without_gaps(workspace):
    repo, manager, tender_id, _other_id = workspace
    staff = _staff(repo, manager, tender_id, "Notebook colleague", "notebook")
    notebooks = StaffNotebookService(repo)
    engineer = engineer_identity(tender_id)
    finding = notebooks.append(
        engineer,
        staff.staff.id,
        NotebookEntryDraft(kind="finding", text="Concrete grade C30/37 note."),
    )
    notebooks.append(
        engineer,
        staff.staff.id,
        NotebookEntryDraft(kind="open_question", text="Confirm curing time with the supplier."),
    )
    correction = notebooks.append(
        engineer,
        staff.staff.id,
        NotebookEntryDraft(
            kind="finding",
            text="Corrected concrete grade note.",
            supersedes_id=finding.id,
        ),
    )

    open_page = notebooks.retrieve(
        engineer, NotebookQuery(staff_id=staff.staff.id, kind="open_question", limit=50)
    )
    assert [item.kind for item in open_page.items] == ["open_question"]

    history = notebooks.retrieve(
        engineer, NotebookQuery(staff_id=staff.staff.id, current=False, limit=50)
    )
    assert [item.id for item in history.items] == [finding.id]
    assert history.items[0].current is False
    assert history.items[0].stale is True

    current_findings = notebooks.retrieve(
        engineer,
        NotebookQuery(staff_id=staff.staff.id, kind="finding", current=True, limit=50),
    )
    assert [item.id for item in current_findings.items] == [correction.id]

    seen: list[str] = []
    cursor: str | None = None
    while True:
        page = notebooks.retrieve(
            engineer,
            NotebookQuery(staff_id=staff.staff.id, current=True, limit=1, cursor=cursor),
        )
        seen.extend(item.id for item in page.items)
        if not page.next_cursor:
            break
        cursor = page.next_cursor
    assert sorted(seen) == sorted(
        [item.id for item in current_findings.items] + [item.id for item in open_page.items]
    )
    assert len(set(seen)) == len(seen)

    client, token = _client(repo)
    headers = {"Authorization": f"Bearer {token}"}
    base = f"/api/tenders/{tender_id}/staff/{staff.staff.id}/notebook"
    response = client.get(f"{base}?kind=open_question", headers=headers)
    assert response.status_code == 200
    assert [item["kind"] for item in response.json()["items"]] == ["open_question"]
    response = client.get(f"{base}?current=false", headers=headers)
    assert response.status_code == 200
    assert [item["id"] for item in response.json()["items"]] == [finding.id]
    assert client.get(f"{base}?kind=mystery", headers=headers).status_code == 422


def test_messages_and_event_reads_use_actual_records(workspace):
    repo, manager, tender_id, _other_id = workspace
    staff = _staff(repo, manager, tender_id, "Message colleague", "message")
    run = repo.get_run(staff.work_order.creator_run_id)
    ManagerRunProfiles(repo).capture(tender_id, run["id"])
    # Manager messages resolve the persisted profile and are retained as the
    # exact speaker snapshot.  No generated text is supplied by this test.
    saved = OfficeMessageService(repo).post_manager(
        OfficeContext(repo, tender_id, run["id"], actor_id=manager.get().id),
        [{"staff_id": staff.staff.id}],
        "instruction",
        "Review the synthetic package.",
        idempotency_key="api-message",
    )
    reader = OfficeReadService(repo)
    page = reader.messages_page(tender_id, staff_id=staff.staff.id, limit=1)
    assert page.items[0].id == saved.id
    assert page.items[0].sender.id == manager.get().id
    assert page.items[0].recipients[0].id == staff.staff.id
