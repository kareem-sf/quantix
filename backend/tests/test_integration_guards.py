"""Regression coverage for approved scope, revisions and durable final publication."""

import asyncio
import json
from types import SimpleNamespace

import pytest
from openpyxl import load_workbook
from test_estimates import approval, rate, seed
from test_office import call_tool, result_for
from test_repository import source

from quantix import office
from quantix.estimates import EstimateService
from quantix.jobs import JobManager
from quantix.office_tools import OfficeContext
from quantix.office_types import OfficeOutput
from quantix.outputs import OutputService
from quantix.repository import Repository


@pytest.fixture
def workspace(tmp_path):
    repo = Repository(tmp_path)
    tid = repo.create_tender("Synthetic integration review")["id"]
    return repo, tid


def plan_for(repo, tid, source_ids):
    return repo.create_plan(
        tid,
        "Review concrete",
        [
            {
                "title": "Check concrete",
                "description": "Check specified concrete strength",
                "role": "Concrete reviewer",
                "source_ids": source_ids,
            }
        ],
    )


def jobs_for(repo):
    return JobManager(
        repo,
        SimpleNamespace(
            api_key=lambda: "test-only-no-network",
            public=lambda: SimpleNamespace(model="gpt-6-astra"),
        ),
    )


def test_revised_source_blocks_pending_plan_approval(workspace):
    repo, tid = workspace
    _, evidence, _ = source(repo, tid)
    plan = plan_for(repo, tid, [evidence["id"]])
    source(repo, tid, digest="b" * 64, body="Concrete strength 40 MPa")
    with pytest.raises(ValueError, match="source|Source|changed|revision"):
        repo.approve_plan(tid, plan["id"], "Proceed")


def test_new_package_material_invalidates_plan_even_without_seed_citations(workspace):
    repo, tid = workspace
    plan = plan_for(repo, tid, [])
    source(repo, tid)
    with pytest.raises(ValueError, match="source|Source|changed|revision"):
        repo.approve_plan(tid, plan["id"], "Proceed")


def test_revised_plan_basis_blocks_task_execution(workspace):
    repo, tid = workspace
    _, evidence, _ = source(repo, tid)
    plan = plan_for(repo, tid, [evidence["id"]])
    task = repo.approve_plan(tid, plan["id"], "Proceed")["tasks"][0]
    source(repo, tid, digest="b" * 64)
    with pytest.raises(ValueError):
        repo.update_task(tid, task["id"], status="running")


def test_unrelated_new_document_does_not_block_approved_task(workspace):
    repo, tid = workspace
    _, evidence, _ = source(repo, tid)
    plan = plan_for(repo, tid, [evidence["id"]])
    task = repo.approve_plan(tid, plan["id"], "Proceed")["tasks"][0]
    source(repo, tid, path="New quote.pdf", digest="b" * 64)
    repo.update_task(tid, task["id"], status="running")
    assert repo.get_task(tid, task["id"])["status"] == "running"


def test_historical_sources_remain_readable_but_cannot_create_current_facts(workspace):
    repo, tid = workspace
    _, evidence, _ = source(repo, tid)
    source(repo, tid, digest="b" * 64, body="Concrete strength 40 MPa")
    assert "30 MPa" in repo.get_evidence(tid, evidence["id"])["text"]
    with pytest.raises(ValueError, match="source|Source|superseded|current"):
        repo.add_finding(tid, "Strength", "30 MPa", "requirement", [evidence["id"]])


def test_completed_task_tracks_evidence_retrieved_beyond_its_seed_sources(workspace):
    repo, tid = workspace
    _, evidence, _ = source(repo, tid)
    plan = plan_for(repo, tid, [])
    task = repo.approve_plan(tid, plan["id"], "Proceed")["tasks"][0]
    repo.update_task(tid, task["id"], status="running")
    repo.update_task(
        tid,
        task["id"],
        status="completed",
        result={
            "summary": "Concrete is 30 MPa",
            "source_ids": [evidence["id"]],
        },
    )
    source(repo, tid, digest="b" * 64, body="Concrete strength 40 MPa")
    assert repo.get_task(tid, task["id"])["status"] == "needs_review"


def test_completed_task_tracks_sources_read_without_a_summary_citation(workspace):
    repo, tid = workspace
    _, evidence, _ = source(repo, tid)
    plan = plan_for(repo, tid, [])
    task = repo.approve_plan(tid, plan["id"], "Proceed")["tasks"][0]
    repo.update_task(tid, task["id"], status="running")
    repo.update_task(
        tid,
        task["id"],
        status="completed",
        result={
            "summary": "Source reviewed",
            "source_ids_read": [evidence["id"]],
        },
    )
    source(repo, tid, digest="b" * 64)
    assert repo.get_task(tid, task["id"])["status"] == "needs_review"


def test_historical_read_is_identified_and_does_not_authorize_a_current_citation(workspace):
    repo, tid = workspace
    _, evidence, _ = source(repo, tid)
    source(repo, tid, digest="b" * 64)
    run = repo.create_run(tid, "manager")
    context = OfficeContext(repo, tid, run["id"])
    historical = context.source(evidence["id"])
    assert historical["version"] == 1
    assert historical["is_current"] is False
    with pytest.raises(ValueError, match="superseded"):
        context.validate_sources([evidence["id"]])


@pytest.mark.asyncio
async def test_approval_conditions_reach_specialist_effective_instructions(workspace, monkeypatch):
    repo, tid = workspace
    _, evidence, _ = source(repo, tid)
    plan = plan_for(repo, tid, [evidence["id"]])
    condition = "Review only supplied evidence; do not perform web research."
    task = repo.approve_plan(tid, plan["id"], condition)["tasks"][0]
    run = repo.create_run(tid, "task")

    async def provider(agent, prompt, **kwargs):
        content = json.loads(prompt)
        assert condition in json.dumps(content)
        assert "binding" in agent.instructions.lower()
        return result_for(OfficeOutput(summary="Review prepared", source_ids=[evidence["id"]]))

    monkeypatch.setattr(office.Runner, "run", provider)
    await office.run_specialist(repo, tid, run["id"], task, "test-key")


@pytest.mark.asyncio
async def test_approved_limits_reach_manager_consultation(workspace, monkeypatch):
    repo, tid = workspace
    _, evidence, _ = source(repo, tid)
    plan = plan_for(repo, tid, [evidence["id"]])
    condition = "Use supplied sources only; no market research."
    repo.approve_plan(tid, plan["id"], condition)
    run = repo.create_run(tid, "manager")

    async def provider(starting_agent, input, **kwargs):
        assert json.loads(input)["engineer_approved_scope"]["rationale"] == condition
        if starting_agent.name == "Tender Manager":
            await call_tool(
                starting_agent,
                "consult_specialist",
                kwargs["context"],
                {
                    "role": "Concrete reviewer",
                    "brief": "Check concrete",
                    "source_ids": [evidence["id"]],
                },
            )
        return result_for(OfficeOutput(summary="Review prepared", source_ids=[evidence["id"]]))

    monkeypatch.setattr(office.Runner, "run", provider)
    result = await office.run_manager(repo, tid, run["id"], "Check approved scope", "test-key")
    assert result.output.source_ids == [evidence["id"]]


@pytest.mark.asyncio
async def test_resumed_task_uses_original_engineer_limits(workspace, monkeypatch):
    repo, tid = workspace
    plan = plan_for(repo, tid, [])
    condition = "Check only the concrete scope."
    task = repo.approve_plan(tid, plan["id"], condition)["tasks"][0]
    jobs = jobs_for(repo)
    run = jobs.start_task(tid, task["id"])
    pending = list(jobs.tasks.values())
    jobs.cancel(run["id"])
    await asyncio.gather(*pending, return_exceptions=True)

    async def provider(agent, prompt, **kwargs):
        assert json.loads(prompt)["engineer_approved_scope"]["rationale"] == condition
        return result_for(OfficeOutput(summary="Resumed within the approved scope"))

    monkeypatch.setattr(office.Runner, "run", provider)
    resumed = jobs.resume(run["id"])
    await asyncio.gather(*list(jobs.tasks.values()))
    assert repo.get_run(resumed["id"])["status"] == "completed"
    assert repo.get_task(tid, task["id"])["status"] == "completed"
    await jobs.close()


@pytest.mark.asyncio
async def test_sdk_worker_prepares_without_publishing_domain_results(workspace, monkeypatch):
    repo, tid = workspace
    run = repo.create_run(tid, "manager")

    async def provider(*args, **kwargs):
        return result_for(
            OfficeOutput(
                summary="Prepared analysis",
                plan={
                    "title": "Review",
                    "tasks": [
                        {"title": "Inspect", "description": "Check scope", "role": "Engineer"}
                    ],
                },
            )
        )

    monkeypatch.setattr(office.Runner, "run", provider)
    prepared = await office.run_manager(repo, tid, run["id"], "Review", "test-key")
    assert repo.messages(tid) == []
    assert repo.list_plans(tid) == []
    assert prepared.output.summary == "Prepared analysis"


@pytest.mark.asyncio
async def test_terminal_storage_failure_rolls_back_publication_before_resume(
    workspace, monkeypatch
):
    repo, tid = workspace
    jobs = jobs_for(repo)

    async def provider(*args, **kwargs):
        return result_for(OfficeOutput(summary="Completed source review"))

    monkeypatch.setattr(office.Runner, "run", provider)
    original = repo.update_run

    def fail_completion(run_id, **fields):
        if fields.get("status") == "completed":
            raise OSError("Synthetic completion storage failure")
        return original(run_id, **fields)

    monkeypatch.setattr(repo, "update_run", fail_completion)
    run = jobs.start_manager(tid, "Review")
    await asyncio.gather(*list(jobs.tasks.values()))
    assert repo.get_run(run["id"])["status"] == "failed"
    assert [m for m in repo.messages(tid) if m["role"] == "manager"] == []
    monkeypatch.setattr(repo, "update_run", original)
    resumed = jobs.resume(run["id"])
    await asyncio.gather(*list(jobs.tasks.values()))
    assert repo.get_run(resumed["id"])["status"] == "completed"
    assert len([m for m in repo.messages(tid) if m["role"] == "manager"]) == 1
    await jobs.close()


@pytest.mark.asyncio
async def test_task_finalization_failure_rolls_back_all_office_records(workspace, monkeypatch):
    repo, tid = workspace
    _, evidence, _ = source(repo, tid)
    plan = plan_for(repo, tid, [evidence["id"]])
    task = repo.approve_plan(tid, plan["id"], "Review concrete only")["tasks"][0]
    jobs = jobs_for(repo)

    async def provider(*args, **kwargs):
        return result_for(
            OfficeOutput(
                summary="Concrete reviewed",
                source_ids=[evidence["id"]],
                findings=[
                    {
                        "title": "Strength",
                        "detail": "30 MPa",
                        "kind": "requirement",
                        "source_ids": [evidence["id"]],
                    }
                ],
                plan={
                    "title": "Next review",
                    "tasks": [
                        {"title": "Check", "description": "Check next detail", "role": "Engineer"}
                    ],
                },
            )
        )

    monkeypatch.setattr(office.Runner, "run", provider)
    original = repo.update_task

    def fail_completion(tender_id, task_id, **fields):
        if fields["status"] == "completed":
            raise OSError("Synthetic task completion failure")
        return original(tender_id, task_id, **fields)

    monkeypatch.setattr(repo, "update_task", fail_completion)
    run = jobs.start_task(tid, task["id"])
    await asyncio.gather(*list(jobs.tasks.values()))
    assert repo.get_run(run["id"])["status"] == "failed"
    assert repo.get_run(run["id"])["result"] == {}
    assert repo.get_task(tid, task["id"])["status"] == "failed"
    assert repo.messages(tid) == repo.list_findings(tid) == []
    assert len(repo.list_plans(tid)) == 1
    assert not {"analysis_proposed", "completed"}.intersection(
        e["kind"] for e in repo.run_events(run["id"])
    )
    await jobs.close()


@pytest.mark.asyncio
async def test_close_before_approved_task_starts_preserves_interrupted_resume(workspace):
    repo, tid = workspace
    plan = plan_for(repo, tid, [])
    task = repo.approve_plan(tid, plan["id"], "Proceed")["tasks"][0]
    jobs = jobs_for(repo)
    run = jobs.start_task(tid, task["id"])
    await jobs.close()
    assert repo.get_run(run["id"])["status"] == "interrupted"
    assert repo.get_task(tid, task["id"])["status"] == "interrupted"
    assert jobs.tasks == {}


@pytest.mark.asyncio
async def test_worker_may_inspect_history_without_citing_it_as_current(workspace, monkeypatch):
    repo, tid = workspace
    _, old, _ = source(repo, tid)
    _, current, _ = source(repo, tid, digest="b" * 64)
    jobs = jobs_for(repo)

    async def provider(*args, **kwargs):
        context = kwargs["context"]
        context.source(old["id"])
        context.source(current["id"])
        return result_for(
            OfficeOutput(summary="Use the current specification", source_ids=[current["id"]])
        )

    monkeypatch.setattr(office.Runner, "run", provider)
    run = jobs.start_manager(tid, "Review the revision")
    await asyncio.gather(*list(jobs.tasks.values()))
    assert repo.get_run(run["id"])["status"] == "completed"
    assert repo.messages(tid)[-1]["source_ids"] == [current["id"]]
    await jobs.close()


@pytest.mark.asyncio
async def test_cancel_before_coroutine_starts_leaves_resumable_terminal_work(workspace):
    repo, tid = workspace
    jobs = jobs_for(repo)
    run = jobs.start_manager(tid, "Review")
    pending = list(jobs.tasks.values())
    jobs.cancel(run["id"])
    await asyncio.gather(*pending, return_exceptions=True)
    assert repo.get_run(run["id"])["status"] == "cancelled"
    assert not jobs.active(tid)
    assert not jobs.tasks
    await jobs.close()


@pytest.mark.asyncio
async def test_cancellation_after_preparation_does_not_publish(workspace, monkeypatch):
    repo, tid = workspace
    jobs = jobs_for(repo)

    async def provider(*args, **kwargs):
        return result_for(OfficeOutput(summary="Prepared only"))

    monkeypatch.setattr(office.Runner, "run", provider)
    worker = office.run_manager

    async def cancel_prepared(repo, tid, run_id, *args):
        prepared = await worker(repo, tid, run_id, *args)
        jobs.cancel(run_id)
        return prepared

    monkeypatch.setattr(office, "run_manager", cancel_prepared)
    run = jobs.start_manager(tid, "Review")
    await asyncio.gather(*list(jobs.tasks.values()), return_exceptions=True)
    assert repo.get_run(run["id"])["status"] == "cancelled"
    assert [m for m in repo.messages(tid) if m["role"] == "manager"] == []
    await jobs.close()


@pytest.mark.parametrize(
    "unit_rate,quantity,gross",
    [
        ("0.04", "1", "0.04"),
        ("11.40", "12.5", "142.50"),
        ("19.99", "3", "59.97"),
    ],
)
def test_inclusive_rates_preserve_entered_gross_amount(workspace, unit_rate, quantity, gross):
    repo, tid = workspace
    seed(repo, tid)
    estimates = EstimateService(repo)
    item = estimates.refresh(tid)["items"][0]
    proposal = estimates.propose_quantity(
        tid,
        item["id"],
        approval(
            quantity=quantity,
            calculation="Synthetic hand-checked quantity",
            source_ids=[item["source_id"]],
        ),
    )
    estimates.approve_quantity(tid, proposal["id"], approval())
    estimates.update_item(
        tid,
        item["id"],
        rate(vat_percent="14")
        | {
            "unit_rate": unit_rate,
            "tax_basis": "including_vat",
        },
    )
    view = estimates.view(tid)
    assert view["items"][0]["line_inc_vat"] == gross
    assert view["totals"][0]["total_inc_vat"] == gross


def test_inclusive_workbook_retains_original_rate_for_gross_formula(workspace):
    repo, tid = workspace
    seed(repo, tid)
    estimates = EstimateService(repo)
    item = estimates.refresh(tid)["items"][0]
    estimates.update_item(
        tid,
        item["id"],
        rate(vat_percent="14")
        | {
            "unit_rate": "0.04",
            "tax_basis": "including_vat",
        },
    )
    outputs = OutputService(repo)
    result = outputs.generate(tid, approval(kind="boq_xlsx"))
    book = load_workbook(outputs.path(tid, result["id"]), data_only=False)
    # Gross must use the supplied inclusive rate, never the rounded net amount in G7.
    assert "G7" not in book["BOQ"]["I7"].value
    assert book["Summary"]["D7"].value == 0.50
    book.close()
