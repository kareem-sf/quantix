import hashlib
import json

import pytest
from fastapi import FastAPI
from fastapi.testclient import TestClient
from test_catalog_authority import configured_office
from test_estimates import seed

from quantix.ai_tools import ToolArgumentError
from quantix.estimates import EstimateService
from quantix.office_tools import OfficeContext, source_tools
from quantix.proposal_tools import proposal_tools
from quantix.repository import Repository
from quantix.takeoff import TakeoffService, compare, normal_unit
from quantix.takeoff_models import TakeoffLineProposal
from quantix.team import TeamService
from quantix.team_models import StaffDraft
from quantix.team_runtime import run_queued


def _line(**values):
    return TakeoffLineProposal.model_validate(
        {
            "description": "Foundation concrete",
            "unit": "m3",
            "quantity": "12.5",
            "method": "dimensions",
            "working": "6 nr x 2.0 x 2.0 x 0.5 = 12.0; plus ground beam 0.5",
            "source_ids": ["drawing"],
            **values,
        }
    )


@pytest.mark.parametrize(
    ("quantity", "unit", "expected"),
    [
        ("12.5", "m3", "matches"),
        ("12.7", "m³", "matches"),
        ("14.0", "cum", "differs"),
        ("12.5", "m2", "unit_differs"),
    ],
)
def test_quantix_compares_takeoff_quantities_with_the_boq(quantity, unit, expected):
    item = {"description": "Cast concrete foundations", "unit": "m3", "supplied_quantity": "12.5"}
    result = compare(_line(quantity=quantity, unit=unit, boq_item_id="item"), item)
    assert result["comparison"] == expected
    if expected == "differs":
        assert result["difference"] == "1.5" and result["difference_percent"] == "12.0"


def test_missing_and_unshown_items_and_unknown_boq_quantities():
    assert compare(_line(), None)["comparison"] == "not_in_boq"
    item = {"description": "Blockwork", "unit": "m2", "supplied_quantity": "40"}
    assert (
        compare(_line(quantity=None, method=None, unit="m2", boq_item_id="item"), item)[
            "comparison"
        ]
        == "not_on_drawings"
    )
    assert (
        compare(_line(boq_item_id="item"), item | {"unit": "m3", "supplied_quantity": None})[
            "comparison"
        ]
        == "no_boq_quantity"
    )
    assert normal_unit(" No. ") == "nr" and normal_unit("م²") == "m2"
    with pytest.raises(ValueError):
        TakeoffLineProposal.model_validate(
            {"description": "Nothing", "unit": "m", "working": "x", "source_ids": ["s"]}
        )


@pytest.fixture
def office(tmp_path, monkeypatch):
    repo = Repository(tmp_path)
    tender = repo.create_tender("Synthetic takeoff")
    route = configured_office(tmp_path, monkeypatch, repo=repo, tender=tender)[-1]
    source = b"Synthetic BOQ workbook"
    digest = hashlib.sha256(source).hexdigest()
    (repo.objects / digest).write_bytes(source)
    seed(repo, tender["id"], digest=digest)
    item = EstimateService(repo).refresh(tender["id"])["items"][0]
    text = "Foundation schedule: F1 pad footings 2.0 x 2.0 x 0.5 m, 6 nr. Ground beam 0.5 m3. Precast manholes 3 nr."
    drawing, _ = repo.register_artifact(
        tender["id"],
        "Drawings/S-101 Foundations.pdf",
        hashlib.sha256(text.encode()).hexdigest(),
        len(text),
        {"kind": "pdf", "status": "extracted", "segments": [{"locator": "page:1", "text": text}]},
    )
    drawing_source = repo.artifact_evidence(tender["id"], drawing["id"])[0]["id"]
    run = repo.create_run(tender["id"], "manager", "Take off the foundations")
    return repo, tender, run, item, drawing_source, route


def _tool(name):
    return next(item for item in [*source_tools(), *proposal_tools()] if item.name == name)


@pytest.mark.asyncio
async def test_a_boq_item_must_be_read_before_a_line_is_matched_to_it(office):
    repo, tender, run, item, drawing_source, _ = office
    context = OfficeContext(repo, tender["id"], run["id"])
    context.seen_sources.add(drawing_source)
    line = {
        "description": "Foundation concrete",
        "unit": "m3",
        "quantity": "12.5",
        "method": "dimensions",
        "working": "6 x 2.0 x 2.0 x 0.5 + 0.5",
        "source_ids": [drawing_source],
        "boq_item_id": item["id"],
    }
    with pytest.raises(ToolArgumentError, match="inspect_estimate"):
        await _tool("propose").invoke(context, {"kind": "takeoff", "items": [line]})


@pytest.mark.asyncio
async def test_staff_takeoff_is_saved_on_completion_and_reviewed_by_the_engineer(
    office, monkeypatch
):
    from quantix.takeoff_routes import create_router
    from quantix.tool_policy import dispatch

    repo, tender, run, item, drawing_source, route = office
    team = TeamService(repo)
    surveyor = team.hire(
        tender["id"],
        run["id"],
        StaffDraft(
            name="Samir Haddad",
            role="Quantity Surveyor",
            specialisms=["Substructure takeoff"],
            background="Twenty years measuring civil works.",
            working_style="Measures from dimensions first.",
        ),
    )
    assignment = team.assign(
        tender["id"],
        run["id"],
        surveyor.id,
        title="Foundations takeoff",
        brief="Take off drawing S-101 and check it against the BOQ.",
        expected_result="Takeoff lines",
        source_ids=[drawing_source],
        route=route,
    )

    async def provider(route, connection, credentials, context, prompt, output_type, **kwargs):
        tools = {definition.name: definition for definition in kwargs["definitions"]}
        await dispatch(
            "direct",
            tools["inspect_estimate"],
            context,
            {"offset": 0, "limit": 10},
            invocation_id="s-boq",
        )
        await dispatch(
            "direct",
            tools["read_source"],
            context,
            {"source_id": drawing_source},
            invocation_id="s-read",
        )
        await dispatch(
            "direct",
            tools["propose"],
            context,
            {
                "kind": "takeoff",
                "items": [
                    {
                        "description": "Pad footings and ground beam concrete",
                        "location": "Grid A-C/1-3",
                        "unit": "m3",
                        "quantity": "12.5",
                        "method": "schedule",
                        "working": "6 x 2.0 x 2.0 x 0.5 = 12.0; ground beam 0.5",
                        "source_ids": [drawing_source],
                        "boq_item_id": item["id"],
                    },
                    {
                        "description": "Precast manholes",
                        "unit": "nr",
                        "quantity": "3",
                        "method": "schedule",
                        "working": "3 nr from the foundation schedule",
                        "source_ids": [drawing_source],
                    },
                ],
            },
            invocation_id="s-propose",
        )
        return {
            "output": {
                "kind": "completed",
                "summary": "Two takeoff lines saved; manholes are not in the BOQ.",
                "source_ids": [drawing_source],
            },
            "usage": {"requests": 3},
            "web_sources": [],
        }

    monkeypatch.setattr("quantix.ai_execution.execute_api", provider)
    [done] = await run_queued(repo, tender["id"], run["id"])
    assert done.status == "completed", done.detail
    assert done.result.saved_records == {"takeoff": 2}
    assert assignment.id == done.id

    app = FastAPI()
    app.include_router(create_router(repo))
    with TestClient(app) as client:
        lines = client.get(f"/api/tenders/{tender['id']}/takeoff").json()
        by_comparison = {line["comparison"]: line for line in lines}
        assert set(by_comparison) == {"matches", "not_in_boq"}
        assert by_comparison["matches"]["boq"]["quantity"] == "12.5"
        assert by_comparison["not_in_boq"]["author"] == "Samir Haddad"
        assert all(line["status"] == "proposed" and line["is_current"] for line in lines)
        missing = by_comparison["not_in_boq"]["id"]
        reviewed = client.post(
            f"/api/tenders/{tender['id']}/takeoff/{missing}/review",
            json={"decision": "accepted", "note": "Raise as a clarification."},
        )
        assert reviewed.status_code == 200, reviewed.text
        assert reviewed.json()["status"] == "accepted"
    with repo.db.connect() as conn:
        decision = conn.execute(
            "SELECT decision,rationale FROM decisions WHERE target_type='takeoff_line' AND target_id=?",
            (missing,),
        ).fetchone()
    assert tuple(decision) == ("accepted", "Raise as a clarification.")
    listed = json.loads(
        await _tool("inspect_tender_records").invoke(
            OfficeContext(repo, tender["id"], run["id"]),
            {"record_type": "takeoff", "offset": 0, "limit": 10},
        )
    )
    assert listed["total"] == 2
    assert {record["status"] for record in listed["records"]} == {"accepted", "proposed"}
    assert TakeoffService(repo).list(tender["id"])[0].tender_id == tender["id"]
