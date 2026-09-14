"""The Manager's working brief carries progress between turns without granting authority."""

from __future__ import annotations

import hashlib
import json

import pytest

from quantix.ai_tools import ToolArgumentError
from quantix.repository import Repository
from quantix.staff_models import OfficeConflict
from quantix.work_brief import WorkBriefService
from quantix.work_brief_models import BriefPoint, BriefQuestion, BriefStep, WorkBriefDraft


def _source(repo, tender_id: str, text: str, path: str = "Sources/spec.pdf"):
    artifact, _ = repo.register_artifact(
        tender_id,
        path,
        hashlib.sha256(text.encode()).hexdigest(),
        len(text.encode()),
        {"kind": "pdf", "status": "extracted", "segments": [{"locator": "page:1", "text": text}]},
    )
    return artifact, repo.artifact_evidence(tender_id, artifact["id"])[0]


def _draft(**changes) -> WorkBriefDraft:
    values = {
        "outcome": "Compare the two concrete suppliers for the ground slab.",
        "status": "in_progress",
        "next_step": "Read supplier B's delivery terms.",
        "done_when": ["Both quotes are compared on the same delivered basis."],
        "steps": [
            BriefStep(title="Read supplier A quote", state="done"),
            BriefStep(title="Read supplier B quote", state="in_progress"),
        ],
        "open_questions": [
            BriefQuestion(text="Is pumping included?", owner="engineer", affects="Placement cost")
        ],
    }
    return WorkBriefDraft(**(values | changes))


def _save(service, tender_id, draft, *, expected_version, inspected=(), key="save-1"):
    return service.save(
        tender_id,
        "run-synthetic",
        "manager-synthetic",
        draft,
        expected_version=expected_version,
        inspected_source_ids=set(inspected),
        idempotency_key=key,
    )


def test_brief_versions_carry_read_sources_and_reject_unread_ones(tmp_path):
    repo = Repository(tmp_path / "quantix")
    tender = repo.create_tender("Synthetic brief Tender")
    _, grade = _source(repo, tender["id"], "Specified concrete is C30/37.")
    _, cover = _source(repo, tender["id"], "Minimum cover is 40 mm.", "Sources/cover.pdf")
    service = WorkBriefService(repo)
    assert service.current(tender["id"]) is None

    settled = [BriefPoint(text="Concrete grade is C30/37.", source_ids=[grade["id"]])]
    first = _save(
        service, tender["id"], _draft(settled=settled), expected_version=0, inspected={grade["id"]}
    )
    assert first.version == 1
    assert first.dependency_state == "current"
    replay = _save(
        service, tender["id"], _draft(settled=settled), expected_version=0, inspected={grade["id"]}
    )
    assert replay.id == first.id

    # A later turn keeps a point it did not re-read, but cannot add an unread one.
    second = _save(
        service,
        tender["id"],
        _draft(settled=settled, status="waiting_for_engineer"),
        expected_version=1,
        key="save-2",
    )
    assert second.version == 2
    with pytest.raises(ValueError, match="not read in this run"):
        _save(
            service,
            tender["id"],
            _draft(
                settled=[*settled, BriefPoint(text="Cover is 40 mm.", source_ids=[cover["id"]])]
            ),
            expected_version=2,
            key="save-3",
        )
    with pytest.raises(OfficeConflict, match="version 2"):
        _save(service, tender["id"], _draft(), expected_version=1, key="save-4")
    with pytest.raises(ValueError, match="not saved in this Tender"):
        _save(
            service,
            tender["id"],
            _draft(work_product_ids=["missing-product"]),
            expected_version=2,
            key="save-5",
        )
    with pytest.raises(ValueError, match="keep it under"):
        _save(
            service,
            tender["id"],
            _draft(
                steps=[
                    BriefStep(title=f"Step {index}", state="to_do", note="x" * 500)
                    for index in range(20)
                ],
                done_when=["y" * 300] * 8,
            ),
            expected_version=2,
            key="save-6",
        )
    assert service.current(tender["id"]).version == 2


def test_revised_source_marks_the_brief_for_review_and_cannot_be_carried(tmp_path):
    repo = Repository(tmp_path / "quantix")
    tender = repo.create_tender("Synthetic revision Tender")
    artifact, grade = _source(repo, tender["id"], "Specified concrete is C30/37.")
    service = WorkBriefService(repo)
    settled = [BriefPoint(text="Concrete grade is C30/37.", source_ids=[grade["id"]])]
    _save(
        service, tender["id"], _draft(settled=settled), expected_version=0, inspected={grade["id"]}
    )

    _source(repo, tender["id"], "Specified concrete is C35/45.", artifact["relative_path"])
    current = service.current(tender["id"])
    assert current.dependency_state == "needs_review"
    assert "sources_changed" in service.prompt_view(tender["id"])
    with pytest.raises(ValueError, match="superseded"):
        _save(service, tender["id"], _draft(settled=settled), expected_version=1, key="carry-stale")


@pytest.fixture
def manager_run(tmp_path, monkeypatch):
    from test_catalog_authority import configured_office

    from quantix.manager_runtime import ManagerRunProfiles

    repo = Repository(tmp_path)
    tender = repo.create_tender("Synthetic continuation Tender")
    configured_office(tmp_path, monkeypatch, repo=repo, tender=tender)
    _, evidence = _source(repo, tender["id"], "Supplier A delivers within 5 working days.")

    def new_run(instruction):
        run = repo.create_run(tender["id"], "manager", instruction)
        ManagerRunProfiles(repo).capture(tender["id"], run["id"])
        return run

    return repo, tender, evidence, new_run


async def _call(definitions, name, context, arguments, invocation_id):
    from quantix.tool_policy import dispatch

    definition = next(item for item in definitions if item.name == name)
    return await dispatch("direct", definition, context, arguments, invocation_id=invocation_id)


@pytest.mark.asyncio
async def test_manager_saves_a_brief_and_the_next_turn_continues_from_it(manager_run, monkeypatch):
    from office_test_support import api_result_for

    from quantix import office
    from quantix.office_types import OfficeOutput
    from quantix.tool_policy import ToolFenceError

    repo, tender, evidence, new_run = manager_run
    prompts, offered = [], []

    async def first_turn(route, connection, credentials, context, prompt, output_type, **kwargs):
        definitions = kwargs["definitions"]
        prompts.append(json.loads(prompt))
        offered.append({item.name for item in definitions})
        await _call(definitions, "search_sources", context, {"query": "delivers"}, None)
        # A malformed call comes back as a correction, not a failed run.
        with pytest.raises(ToolFenceError) as malformed:
            await _call(
                definitions,
                "save_work_brief",
                context,
                {"outcome": "Compare suppliers", "status": "almost"},
                "bad-call",
            )
        assert malformed.value.recoverable is True
        await _call(
            definitions,
            "save_work_brief",
            context,
            {
                "outcome": "Compare supplier delivery for the ground slab.",
                "status": "in_progress",
                "expected_version": 0,
                "steps": [
                    {"title": "Read supplier A terms", "state": "done"},
                    {"title": "Read supplier B terms", "state": "to_do"},
                ],
                "settled": [
                    {
                        "text": "Supplier A delivers in 5 working days.",
                        "source_ids": [evidence["id"]],
                    }
                ],
                "next_step": "Read supplier B terms.",
            },
            "save-brief",
        )
        return api_result_for(OfficeOutput(summary="Supplier A terms are read."))

    monkeypatch.setattr("quantix.ai_execution.execute_api", first_turn)
    run = new_run("Compare supplier delivery")
    await office.run_manager(repo, tender["id"], run["id"], "Compare supplier delivery")

    assert prompts[0]["manager_work_brief"] == {
        "version": 0,
        "note": "No working brief is saved yet.",
    }
    assert "save_work_brief" in offered[0] and "list_work_products" in offered[0]
    assert "request_child_assignment" not in offered[0]
    assert any(event["kind"] == "work_brief_saved" for event in repo.run_events(run["id"]))

    async def second_turn(route, connection, credentials, context, prompt, output_type, **kwargs):
        prompts.append(json.loads(prompt))
        return api_result_for(OfficeOutput(summary="Continuing with supplier B."))

    monkeypatch.setattr("quantix.ai_execution.execute_api", second_turn)
    follow_up = new_run("Carry on")
    await office.run_manager(repo, tender["id"], follow_up["id"], "Carry on")

    brief = prompts[1]["manager_work_brief"]
    assert brief["version"] == 1
    assert brief["next_step"] == "Read supplier B terms."
    assert [step["state"] for step in brief["steps"]] == ["done", "to_do"]
    assert brief["settled"][0]["source_ids"] == [evidence["id"]]
    assert "not source evidence" in brief["note"]


@pytest.mark.asyncio
async def test_staff_cannot_use_the_manager_brief_tool(manager_run):
    from quantix.office_tools import OfficeContext
    from quantix.work_brief_tools import work_brief_tools

    repo, tender, _, new_run = manager_run
    run = new_run("Synthetic")
    tools = {item.name: item for item in work_brief_tools(repo, tender["id"], run["id"], "manager")}
    staff_like = OfficeContext(
        repo, tender["id"], run["id"], actor_id="staff-1", assignment_id="assignment-1"
    )
    with pytest.raises(ValueError, match="Only the Tender Manager"):
        await tools["save_work_brief"].invoke(
            staff_like,
            {"outcome": "x", "status": "in_progress", "expected_version": 0},
            invocation_id="staff-call",
        )
    assert WorkBriefService(repo).current(tender["id"]) is None


def test_brief_route_returns_the_current_brief(tmp_path):
    from fastapi import FastAPI
    from fastapi.testclient import TestClient

    from quantix.later_routes import create_router

    repo = Repository(tmp_path / "quantix")
    app = FastAPI()
    app.include_router(create_router(repo))
    with TestClient(app) as client:
        tender = repo.create_tender("Synthetic route Tender")
        empty = client.get(f"/api/tenders/{tender['id']}/work-brief")
        assert empty.status_code == 200 and empty.json() == {"brief": None}
        WorkBriefService(repo).save(
            tender["id"],
            "run-synthetic",
            "manager-synthetic",
            _draft(),
            expected_version=0,
            inspected_source_ids=set(),
            idempotency_key="route",
        )
        saved = client.get(f"/api/tenders/{tender['id']}/work-brief").json()["brief"]
        assert saved["version"] == 1
        assert saved["steps"][1]["state"] == "in_progress"
        assert client.get("/api/tenders/missing/work-brief").status_code == 404


def test_argument_errors_name_fields_without_echoing_values():
    from quantix.ai_tools import tool

    @tool
    async def synthetic(ctx, limit: int):
        return "ok"

    import asyncio

    with pytest.raises(ToolArgumentError) as error:
        asyncio.run(synthetic.invoke(None, {"limit": "private-looking-value"}))
    assert "limit" in str(error.value)
    assert "private-looking-value" not in str(error.value)

    # Naming another Tender stays a refusal rather than a correction prompt.
    with pytest.raises(ValueError, match="cannot name a Tender") as refused:
        asyncio.run(synthetic.invoke(None, {"limit": 1, "tender_id": "other"}))
    assert not isinstance(refused.value, ToolArgumentError)
