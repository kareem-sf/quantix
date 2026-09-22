import hashlib
import json
import typing

import pytest

from quantix.ai_tools import ToolArgumentError
from quantix.office import compose
from quantix.office_tools import OfficeContext
from quantix.office_types import ManagerAnswer
from quantix.proposal_tools import RULES, ProposalKind, proposal_tools
from quantix.repository import Repository


@pytest.fixture
def workspace(tmp_path):
    repo = Repository(tmp_path)
    tender = repo.create_tender("Synthetic proposals")
    text = "Ground slab concrete shall be C30/37."
    artifact, _ = repo.register_artifact(
        tender["id"],
        "Specs/concrete.pdf",
        hashlib.sha256(text.encode()).hexdigest(),
        len(text),
        {"kind": "pdf", "status": "extracted", "segments": [{"locator": "page:1", "text": text}]},
    )
    source_id = repo.artifact_evidence(tender["id"], artifact["id"])[0]["id"]
    run = repo.create_run(tender["id"], "manager", "Plan the concrete review")
    return OfficeContext(repo, tender["id"], run["id"]), source_id


def _tool(name):
    return next(item for item in proposal_tools() if item.name == name)


def _plan(source_id, title="Concrete review"):
    return {
        "title": title,
        "tasks": [
            {
                "title": "Check slab",
                "description": "Check the ground slab.",
                "role": "Quantity Surveyor",
                "source_ids": [source_id],
            }
        ],
    }


@pytest.mark.asyncio
@pytest.mark.parametrize("kind", typing.get_args(ProposalKind))
async def test_every_kind_has_rules_and_an_item_schema(workspace, kind):
    context, _ = workspace
    result = json.loads(await _tool("proposal_format").invoke(context, {"kind": kind}))
    assert result["rules"] == RULES[kind]
    assert result["item_schema"]["type"] == "object"


@pytest.mark.asyncio
async def test_propose_checks_fields_and_evidence_when_called(workspace):
    context, source_id = workspace
    propose = _tool("propose")
    with pytest.raises(ToolArgumentError):
        await propose.invoke(context, {"kind": "plan", "items": [{"title": "No tasks"}]})
    with pytest.raises(ToolArgumentError, match="read|evidence|source"):
        await propose.invoke(context, {"kind": "plan", "items": [_plan(source_id)]})
    assert context.proposals == {}


@pytest.mark.asyncio
async def test_a_plan_is_replaced_and_list_kinds_append_until_replaced(workspace):
    context, source_id = workspace
    context.seen_sources.add(source_id)
    propose = _tool("propose")
    await propose.invoke(context, {"kind": "plan", "items": [_plan(source_id, "First")]})
    await propose.invoke(context, {"kind": "plan", "items": [_plan(source_id, "Second")]})
    output = compose(ManagerAnswer(summary="Plan proposed.", source_ids=[source_id]), context)
    assert output.plan.title == "Second"
    with pytest.raises(ToolArgumentError, match="exactly one"):
        await propose.invoke(
            context, {"kind": "plan", "items": [_plan(source_id), _plan(source_id)]}
        )
    await propose.invoke(context, {"kind": "plan", "items": []})
    assert compose(ManagerAnswer(summary="Withdrawn."), context).plan is None

    def line(description):
        return {
            "description": description,
            "unit": "m3",
            "quantity": "12.5",
            "method": "dimensions",
            "working": "10 x 5 x 0.25",
            "source_ids": [source_id],
        }

    await propose.invoke(context, {"kind": "takeoff", "items": [line("Slab")]})
    result = json.loads(
        await propose.invoke(context, {"kind": "takeoff", "items": [line("Footings")]})
    )
    assert result["staged_total"] == 2
    await propose.invoke(context, {"kind": "takeoff", "items": [line("Walls")], "replace": True})
    assert [item.description for item in compose(ManagerAnswer(summary="x"), context).takeoff] == [
        "Walls"
    ]


@pytest.mark.asyncio
async def test_staff_stage_takeoff_but_not_plans(workspace):
    context, source_id = workspace
    staff = OfficeContext(
        context.repo, context.tender_id, context.run_id, actor_id="staff-1", assignment_id="a-1"
    )
    staff.seen_sources.add(source_id)
    with pytest.raises(ToolArgumentError, match="Staff stage"):
        await _tool("propose").invoke(staff, {"kind": "plan", "items": [_plan(source_id)]})


def test_near_miss_boq_row_fields_are_accepted_and_a_bad_call_lists_every_field():
    from quantix.proposal_tools import field_guide, normalize_items

    fixed = normalize_items(
        "boq_item_proposals",
        [
            {
                "item_number": "6.3.1",
                "source_ids": ["abc"],
                "excerpt": "6.3.1 Fire alarm control panel 1 no",
                "description": "Fire alarm control panel",
                "unit": "no",
                "quantity": 1,
            }
        ],
    )
    assert fixed == [
        {
            "row_reference": "6.3.1",
            "source_id": "abc",
            "source_excerpt": "6.3.1 Fire alarm control panel 1 no",
            "description": "Fire alarm control panel",
            "unit": "no",
            "quantity": "1",
        }
    ]
    guide = field_guide("boq_item_proposals")
    assert "row_reference (string, required)" in guide
    assert 'quantity (number written as text, e.g. "12.5", required)' in guide


@pytest.mark.asyncio
async def test_a_rejected_record_call_names_the_fields_to_use(workspace):
    context, _ = workspace
    with pytest.raises(ToolArgumentError, match="has exactly these fields"):
        await _tool("propose").invoke(
            context, {"kind": "boq_item_proposals", "items": [{"page_reference": "p.3"}]}
        )


@pytest.mark.asyncio
async def test_a_job_that_stops_part_way_keeps_its_staged_records_and_asks_to_carry_on(workspace):
    from quantix.office import salvage_staged
    from quantix.office_research import ResearchRecord

    context, source_id = workspace
    context.seen_sources.add(source_id)
    context.repo.update_run(context.run_id, status="running")
    await _tool("propose").invoke(context, {"kind": "plan", "items": [_plan(source_id)]})

    output = salvage_staged(
        context,
        ResearchRecord(context),
        "The Tender Manager used all 32 AI steps allowed for one job before it finished.",
    )

    assert output is not None
    assert output.plan is not None
    assert "used all the AI steps allowed for one job" in output.summary
    assert "AI steps" in salvage_staged.__globals__["_stop_in_words"](
        "The tender's AI request allowance for this work is used up."
    )
    assert "a work plan" in output.summary
    assert output.question.choices[0].recommended is True

    context.repo.update_run(context.run_id, status="cancelled")
    assert salvage_staged(context, ResearchRecord(context), "stopped") is None


def test_nothing_is_salvaged_when_nothing_was_staged(workspace):
    from quantix.office import salvage_staged
    from quantix.office_research import ResearchRecord

    context, _ = workspace
    assert salvage_staged(context, ResearchRecord(context), "timed out") is None


@pytest.mark.asyncio
async def test_two_different_rows_with_one_item_number_are_refused_while_staging(tmp_path):
    repo = Repository(tmp_path)
    tender = repo.create_tender("Duplicate item numbers")
    text = "6.1.1 Cable tray 20 m\n6.1.1 Cable ladder 15 m"
    digest = hashlib.sha256(text.encode()).hexdigest()
    (repo.objects / digest).write_bytes(text.encode())
    artifact, _ = repo.register_artifact(
        tender["id"],
        "Bill.pdf",
        digest,
        len(text),
        {"kind": "pdf", "status": "extracted", "segments": [{"locator": "page:1", "text": text}]},
    )
    source_id = repo.artifact_evidence(tender["id"], artifact["id"])[0]["id"]
    run = repo.create_run(tender["id"], "manager", "Load the BOQ")
    context = OfficeContext(repo, tender["id"], run["id"])
    context.seen_sources.add(source_id)
    row = {"source_id": source_id, "row_reference": "6.1.1", "unit": "m"}
    await _tool("propose").invoke(
        context,
        {
            "kind": "boq_item_proposals",
            "items": [{**row, "source_excerpt": "6.1.1 Cable tray 20 m", "quantity": "20"}],
        },
    )
    # The final save would refuse this, so staging refuses it first, naming the row.
    with pytest.raises(ToolArgumentError, match="BOQ row 6.1.1: Another staged row"):
        await _tool("propose").invoke(
            context,
            {
                "kind": "boq_item_proposals",
                "items": [{**row, "source_excerpt": "6.1.1 Cable ladder 15 m", "quantity": "15"}],
            },
        )
    assert len(context.proposals["boq_item_proposals"]) == 1


def test_the_manager_is_told_which_earlier_jobs_saved_nothing(workspace):
    import json as json_module

    from quantix.office import _prompt

    context, _ = workspace
    repo = context.repo
    failed = repo.create_run(context.tender_id, "manager", "Load BOQ pages 10-17")
    repo.update_run(failed["id"], status="failed", error="A clash stopped the save.")
    later = repo.create_run(context.tender_id, "manager", "Carry on")
    from quantix.office_tools import OfficeContext

    prompt = json_module.loads(
        _prompt(OfficeContext(repo, context.tender_id, later["id"]), "Carry on")
    )
    unfinished = prompt["earlier_jobs_that_did_not_finish"]
    assert [job["request"] for job in unfinished] == ["Load BOQ pages 10-17"]
    assert all(job["staged_records_saved"] is False for job in unfinished)
    now = prompt["tender_records_now"]
    assert now["boq_rows"] == {"accepted": 0, "waiting_for_engineer": 0, "priced": 0}
    assert (now["approved_local_exports"], now["submission_requirements"]) == (0, [])


@pytest.mark.asyncio
async def test_staged_records_are_kept_even_when_an_earlier_brief_is_unfinished(workspace):
    from quantix.office import salvage_staged
    from quantix.office_research import ResearchRecord
    from quantix.work_brief import WorkBriefService
    from quantix.work_brief_models import BriefStep, WorkBriefDraft
    from quantix.work_progress import require_current_brief

    context, source_id = workspace
    repo = context.repo
    earlier = repo.create_run(context.tender_id, "manager", "Price the bill")
    WorkBriefService(repo).save(
        context.tender_id,
        earlier["id"],
        "manager",
        WorkBriefDraft(
            outcome="Price the bill",
            status="in_progress",
            steps=[BriefStep(title="Price fire alarm", state="done", owner="Tender Manager")],
        ),
        expected_version=0,
        inspected_source_ids=set(),
        idempotency_key="brief-earlier",
    )
    context.seen_sources.add(source_id)
    repo.update_run(context.run_id, status="running")
    await _tool("propose").invoke(context, {"kind": "plan", "items": [_plan(source_id)]})

    output = salvage_staged(context, ResearchRecord(context), "This task reached its work limit.")

    assert output is not None and output.plan is not None
    assert "AI steps" in output.summary
    # Saving the kept records later is not blocked by the unfinished brief either.
    require_current_brief(output, context)


def test_near_miss_requirement_fields_are_accepted_before_validation():
    from quantix.proposal_tools import normalize_items

    fixed = normalize_items(
        "submission_requirements",
        [
            {
                "title": "Sealed technical specification",
                "detail": "Stamp every page.",
                "source_ids": ["abc"],
                "deliverable_kind": "technical_docx",
                "exceptions": "Except small enterprises",
                "due_date": "with the offer",
                "language": "Arabic",
            }
        ],
    )
    assert fixed[0]["exceptions"] == ["Except small enterprises"]
    assert "due_date" not in fixed[0] and "language" not in fixed[0]
