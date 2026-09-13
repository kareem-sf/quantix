"""Manager-facing dynamic staff generation tool contract tests."""

from __future__ import annotations

import hashlib
import json
from types import SimpleNamespace

import pytest

from quantix import diagnostics
from quantix.manager_profile import ManagerProfileService, OfficeConflict
from quantix.manager_runtime import ManagerRunProfiles
from quantix.office_events import OfficeEventService
from quantix.repository import Repository
from quantix.staff_store import StaffStore
from quantix.staff_store_models import StaffWorkOrderReceipt


@pytest.fixture(autouse=True)
def close_temporary_diagnostics():
    """Close only a diagnostics writer created by a Repository in this test."""

    before = diagnostics._writer
    yield
    after = diagnostics._writer
    if after is not None and after is not before:
        after.close()


def _personality():
    return {
        "description": "Evidence-led synthetic colleague.",
        "traits": ["careful"],
        "communication_style": "Plain language.",
        "problem_solving_style": "Use bounded checks.",
        "collaboration_style": "Share exact sources.",
        "uncertainty_handling": "Label gaps.",
        "initiative": "Suggest the next safe step.",
        "explanation_style": "Lead with the answer.",
        "language_preferences": ["English"],
        "working_habits": ["Keep citations"],
    }


def _profile(**overrides):
    value = {
        "display_name": "Unfamiliar Colleague",
        "role": "Concrete package synthesiser",
        "title": "Evidence Specialist",
        "specialisms": ["Package review"],
        "persona": "A bounded professional profile.",
        "personality": _personality(),
        "responsibilities": ["Compare source evidence"],
        "objectives": ["List material gaps"],
        "methods": ["Read current sources"],
        "deliverables": ["A source-backed gap list"],
        "success_criteria": ["Every gap has a source"],
        "context_needs": ["Current Tender sources"],
        "requested_tool_ids": [],
        "creation_reason": "Created for this Tender task.",
    }
    value.update(overrides)
    return value


def _work_order(**overrides):
    value = {
        "brief": "Review the current Tender source package.",
        "goal": "Give the Manager a source-backed gap list.",
        "source_ids": [],
        "expected_outputs": ["Gap list"],
        "completion_checks": ["Each gap has a source"],
    }
    value.update(overrides)
    return value


def _workspace(tmp_path):
    repo = Repository(tmp_path / "repo")
    tender = repo.create_tender("Synthetic generation Tender")
    run = repo.create_run(tender["id"], "conversation", "Create a colleague")
    ManagerRunProfiles(repo).capture(tender["id"], run["id"])
    context = SimpleNamespace(repo=repo, tender_id=tender["id"], run_id=run["id"])
    return repo, tender, run, context


def _source(repo, tender_id):
    body = b"synthetic reviewed source"
    artifact, _ = repo.register_artifact(
        tender_id,
        "Sources/spec.pdf",
        hashlib.sha256(body).hexdigest(),
        len(body),
        {
            "kind": "pdf",
            "status": "extracted",
            "segments": [{"locator": "page:1", "text": body.decode()}],
        },
    )
    return repo.artifact_evidence(tender_id, artifact["id"])[0]["id"]


def _tools(repo, tender_id, run_id, scope_id="plan-1"):
    from quantix.staff_generation import staff_generation_tools

    return {item.name: item for item in staff_generation_tools(repo, tender_id, run_id, scope_id)}


@pytest.mark.asyncio
async def test_manager_tools_create_full_profile_and_durable_event(tmp_path):
    repo, tender, run, context = _workspace(tmp_path)
    source_id = _source(repo, tender["id"])
    tools = _tools(repo, tender["id"], run["id"])
    result = await tools["create_staff"].invoke(
        context,
        {
            "profile": _profile(
                requested_tool_ids=["read_source", "tool-not-installed"],
            ),
            "work_order": _work_order(source_ids=[source_id]),
        },
        invocation_id="inv-create-1",
    )

    value = json.loads(result)
    assert value["staff"]["display_name"] == "Unfamiliar Colleague"
    assert value["staff"]["lifecycle"] == "available"
    assert value["work_order"]["staff_version"] == 1
    statuses = {item["id"]: item["status"] for item in value["requested_capabilities"]["tools"]}
    assert statuses == {
        "read_source": "Needs an approved work scope",
        "tool-not-installed": "Unavailable",
    }
    events = OfficeEventService(repo).page(tender["id"]).items
    assert [event.event_type for event in events] == ["staff_created"]
    assert events[0].actor_id == value["manager_id"]
    assert events[0].record_ref == {"kind": "staff", "id": value["staff"]["id"], "version": 1}
    assert events[0].payload == {"version": 1, "work_order_id": value["work_order"]["id"]}


@pytest.mark.asyncio
async def test_create_replay_is_exactly_one_saved_operation_and_event(tmp_path):
    repo, tender, run, context = _workspace(tmp_path)
    tools = _tools(repo, tender["id"], run["id"])
    arguments = {"profile": _profile(), "work_order": _work_order()}
    first = await tools["create_staff"].invoke(context, arguments, invocation_id="inv-replay")
    second = await tools["create_staff"].invoke(context, arguments, invocation_id="inv-replay")

    first_value, second_value = json.loads(first), json.loads(second)
    assert first_value["staff"]["id"] == second_value["staff"]["id"]
    assert first_value["work_order"]["id"] == second_value["work_order"]["id"]
    assert second_value["replayed"] is True
    assert len(StaffStore(repo).list_staff(tender["id"])) == 1
    assert len(OfficeEventService(repo).page(tender["id"]).items) == 1

    with pytest.raises(OfficeConflict):
        await tools["create_staff"].invoke(
            context,
            {"profile": _profile(display_name="Changed"), "work_order": _work_order()},
            invocation_id="inv-replay",
        )


@pytest.mark.asyncio
async def test_revision_read_and_reuse_keep_profile_versions_and_work_order_snapshots(tmp_path):
    repo, tender, run, context = _workspace(tmp_path)
    tools = _tools(repo, tender["id"], run["id"])
    created = json.loads(
        await tools["create_staff"].invoke(
            context, {"profile": _profile(), "work_order": _work_order()}, invocation_id="inv-create"
        )
    )
    revised = json.loads(
        await tools["revise_staff"].invoke(
            context,
            {"staff_id": created["staff"]["id"], "expected_version": 1, "profile": _profile(display_name="Revised")},
            invocation_id="inv-revise",
        )
    )
    assert revised["staff"]["version"] == 2
    listed = json.loads(await tools["list_office_staff"].invoke(context, {}, invocation_id=None))
    assert listed["items"][0]["staff"]["version"] == 2
    read = json.loads(
        await tools["read_staff"].invoke(
            context, {"staff_id": created["staff"]["id"]}, invocation_id=None
        )
    )
    assert read["staff"]["version"] == 2
    assert read["work_orders"][0]["id"] == created["work_order"]["id"]
    assert read["work_orders"][0]["staff_version"] == 1
    planned = json.loads(
        await tools["plan_staff_work"].invoke(
            context,
            {
                "staff_id": created["staff"]["id"],
                "expected_version": 2,
                "work_order": _work_order(brief="A second bounded brief."),
            },
            invocation_id="inv-plan",
        )
    )
    assert planned["work_order"]["staff_id"] == created["staff"]["id"]
    assert planned["work_order"]["staff_version"] == 2
    replayed = json.loads(
        await tools["plan_staff_work"].invoke(
            context,
            {
                "staff_id": created["staff"]["id"],
                "expected_version": 2,
                "work_order": _work_order(brief="A second bounded brief."),
            },
            invocation_id="inv-plan",
        )
    )
    assert replayed["replayed"] is True
    with pytest.raises(OfficeConflict):
        await tools["plan_staff_work"].invoke(
            context,
            {
                "staff_id": created["staff"]["id"],
                "expected_version": 1,
                "work_order": _work_order(brief="A stale profile brief."),
            },
            invocation_id="inv-stale-plan",
        )
    other = repo.create_tender("Other Tender")
    other_source = _source(repo, other["id"])
    with pytest.raises(ValueError, match="source"):
        await tools["plan_staff_work"].invoke(
            context,
            {
                "staff_id": created["staff"]["id"],
                "expected_version": 2,
                "work_order": _work_order(source_ids=[other_source]),
            },
            invocation_id="inv-foreign-source",
        )
    events = OfficeEventService(repo).page(tender["id"]).items
    assert [event.event_type for event in events] == ["staff_created", "profile_updated", "scope_changed"]
    assert events[-1].record_ref == {"kind": "work_order", "id": planned["work_order"]["id"], "version": 2}


@pytest.mark.asyncio
async def test_tools_reject_foreign_or_cancelled_runtime_context_without_state(tmp_path):
    repo, tender, run, context = _workspace(tmp_path)
    tools = _tools(repo, tender["id"], run["id"])
    foreign = SimpleNamespace(repo=repo, tender_id="foreign", run_id=run["id"])
    with pytest.raises(ValueError, match="context"):
        await tools["create_staff"].invoke(
            foreign, {"profile": _profile(), "work_order": _work_order()}, invocation_id="inv-foreign"
        )

    repo.update_run(run["id"], status="cancelled")
    with pytest.raises(OfficeConflict):
        await tools["create_staff"].invoke(
            context, {"profile": _profile(), "work_order": _work_order()}, invocation_id="inv-cancelled"
        )
    assert StaffStore(repo).list_staff(tender["id"]) == []
    assert OfficeEventService(repo).page(tender["id"]).items == []


@pytest.mark.asyncio
async def test_event_failure_rolls_back_profile_work_order_receipt_and_event(tmp_path, monkeypatch):
    repo, tender, run, context = _workspace(tmp_path)
    tools = _tools(repo, tender["id"], run["id"])

    def fail(*_args, **_kwargs):
        raise RuntimeError("synthetic event failure")

    monkeypatch.setattr(OfficeEventService, "append", fail)
    with pytest.raises(RuntimeError, match="synthetic event failure"):
        await tools["create_staff"].invoke(
            context,
            {"profile": _profile(), "work_order": _work_order()},
            invocation_id="inv-rollback",
        )

    assert StaffStore(repo).list_staff(tender["id"]) == []
    with repo.db.connect() as conn:
        assert conn.execute("SELECT COUNT(*) FROM office_work_orders").fetchone()[0] == 0
        assert conn.execute("SELECT COUNT(*) FROM office_generation_receipts").fetchone()[0] == 0
    assert OfficeEventService(repo).page(tender["id"]).items == []


def test_work_order_store_receipt_replay_and_version_conflict(tmp_path):
    repo, tender, run, context = _workspace(tmp_path)
    tools = _tools(repo, tender["id"], run["id"])
    import asyncio

    created = json.loads(
        asyncio.run(
            tools["create_staff"].invoke(
                context, {"profile": _profile(), "work_order": _work_order()}, invocation_id="inv-create"
            )
        )
    )
    store = StaffStore(repo)
    from quantix.staff_models import ManagerCreationContext

    manager_version = ManagerProfileService(repo).get().version
    lineage = ManagerCreationContext(tender["id"], run["id"], manager_version, "plan-1")
    first = store.create_work_order(lineage, created["staff"]["id"], 1, _work_order(brief="Reuse"), "reuse-1")
    replay = store.create_work_order(lineage, created["staff"]["id"], 1, _work_order(brief="Reuse"), "reuse-1")
    assert isinstance(first, StaffWorkOrderReceipt)
    assert replay.replayed is True
    assert replay.work_order.id == first.work_order.id
    with pytest.raises(OfficeConflict):
        store.create_work_order(lineage, created["staff"]["id"], 1, _work_order(brief="Changed"), "reuse-1")
