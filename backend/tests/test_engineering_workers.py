"""Deterministic work is attributable, repeatable and cannot overwrite other drafts."""

import json

import pytest

from quantix.execution_context import engineer_identity
from quantix.repository import Repository
from quantix.work_product_models import WorkProductDraft
from quantix.work_products import WorkProductService


def test_unrelated_drafts_are_independent_work_products(tmp_path):
    repo = Repository(tmp_path)
    tender = repo.create_tender("Synthetic work products")
    service = WorkProductService(repo)
    ctx = engineer_identity(tender["id"])
    first = service.save_draft(
        ctx,
        WorkProductDraft(
            kind="table",
            title="Quantity comparison",
            rows=[{"quantity": "12"}],
            idempotency_key="first",
        ),
    )
    second = service.save_draft(
        ctx,
        WorkProductDraft(
            kind="table",
            title="Supplier comparison",
            rows=[{"supplier": "Synthetic"}],
            idempotency_key="second",
        ),
    )
    assert first.product_id != second.product_id
    assert first.version == second.version == 1
    assert service.get(tender["id"], first.product_id, 1).title == "Quantity comparison"


def test_revision_requires_exact_product_and_version(tmp_path):
    repo = Repository(tmp_path)
    tender = repo.create_tender("Synthetic versions")
    service = WorkProductService(repo)
    ctx = engineer_identity(tender["id"])
    first = service.save_draft(
        ctx, WorkProductDraft(kind="note", title="Review", content="First", idempotency_key="first")
    )
    edit = WorkProductDraft(
        kind="note",
        title="Review",
        content="Second",
        idempotency_key="revision",
        product_id=first.product_id,
        expected_version=1,
    )
    second = service.save_draft(ctx, edit)
    assert second.product_id == first.product_id and second.version == 2
    assert service.save_draft(ctx, edit) == second
    with pytest.raises(ValueError, match="version"):
        service.save_draft(
            ctx, edit.model_copy(update={"idempotency_key": "stale", "content": "Third"})
        )
    other = repo.create_tender("Another Tender")
    with pytest.raises((KeyError, ValueError)):
        service.save_draft(
            engineer_identity(other["id"]),
            edit.model_copy(update={"idempotency_key": "wrong-tender"}),
        )


@pytest.mark.asyncio
async def test_calculation_tool_uses_real_decimal_arithmetic_and_records_work(tmp_path):
    from quantix.engineering_tools import engineering_tools
    from quantix.office_tools import OfficeContext

    repo = Repository(tmp_path)
    tender = repo.create_tender("Synthetic calculation")
    run = repo.create_run(tender["id"], "manager", "Calculate the volume")
    ctx = OfficeContext(repo, tender["id"], run["id"])
    tools = {tool.name: tool for tool in engineering_tools()}
    result = json.loads(
        await tools["calculate_engineering"].invoke(
            ctx,
            {
                "method": "product",
                "inputs": {"quantity": "12.5", "factor": "8"},
                "units": {"quantity": "m", "factor": "m"},
            },
            invocation_id="calculation-1",
        )
    )
    assert result["outputs"]["product"] == "100.00"
    repeated = json.loads(
        await tools["calculate_engineering"].invoke(
            ctx,
            {
                "method": "product",
                "inputs": {"quantity": "12.5", "factor": "8"},
                "units": {"quantity": "m", "factor": "m"},
            },
            invocation_id="calculation-1",
        )
    )
    assert repeated["id"] == result["id"]
    assert any(event["kind"] == "calculation_completed" for event in repo.run_events(run["id"]))


@pytest.mark.asyncio
async def test_saved_work_product_rejects_unread_sources(tmp_path):
    from quantix.engineering_tools import engineering_tools
    from quantix.office_tools import OfficeContext

    repo = Repository(tmp_path)
    tender = repo.create_tender("Synthetic source scope")
    run = repo.create_run(tender["id"], "manager", "Save a draft")
    ctx = OfficeContext(repo, tender["id"], run["id"])
    tool = next(item for item in engineering_tools() if item.name == "save_work_product")
    with pytest.raises((KeyError, ValueError)):
        await tool.invoke(
            ctx,
            {
                "kind": "note",
                "title": "Unsupported claim",
                "content": "A source says this.",
                "source_refs": ["unread"],
            },
            invocation_id="save-1",
        )


@pytest.mark.asyncio
async def test_citing_an_unread_tender_source_is_the_models_to_correct(tmp_path):
    """A real passage not read in this run is named so the AI can read it; the job goes on."""
    import hashlib

    from quantix.ai_tools import ToolArgumentError
    from quantix.engineering_tools import engineering_tools
    from quantix.office_tools import OfficeContext
    from quantix.tool_policy import ToolFenceError, dispatch

    repo = Repository(tmp_path)
    tender = repo.create_tender("Synthetic unread source")
    text = "Fire alarm control panel: Edwards EST4."
    artifact, _ = repo.register_artifact(
        tender["id"],
        "Systems.pdf",
        hashlib.sha256(text.encode()).hexdigest(),
        len(text),
        {"kind": "pdf", "status": "extracted", "segments": [{"locator": "page:1", "text": text}]},
    )
    source_id = repo.artifact_evidence(tender["id"], artifact["id"])[0]["id"]
    run = repo.create_run(tender["id"], "manager", "Save the evidence table")
    ctx = OfficeContext(repo, tender["id"], run["id"])
    tool = next(item for item in engineering_tools() if item.name == "save_work_product")
    draft = {
        "kind": "note",
        "title": "Fire alarm make",
        "content": "Edwards EST4.",
        "source_refs": [source_id],
    }
    with pytest.raises(ToolArgumentError, match=source_id):
        await tool.invoke(ctx, draft, invocation_id="save-unread")
    repo.update_run(run["id"], status="running")
    with pytest.raises(ToolFenceError) as refused:
        await dispatch("direct", tool, ctx, draft, invocation_id="save-unread-2")
    assert refused.value.recoverable

    ctx.seen_sources.add(source_id)
    saved = json.loads(await tool.invoke(ctx, draft, invocation_id="save-read"))
    assert saved["title"] == "Fire alarm make"


@pytest.mark.asyncio
async def test_stopping_before_write_admission_prevents_calculation(tmp_path, monkeypatch):
    from contextlib import contextmanager

    from quantix.ai_connections import AIConnectionService
    from quantix.engineering_tools import engineering_tools
    from quantix.office_tools import OfficeContext

    repo = Repository(tmp_path)
    tender = repo.create_tender("Synthetic stopped calculation")
    run = repo.create_run(tender["id"], "manager", "Calculate a draft")
    ctx = OfficeContext(repo, tender["id"], run["id"])

    @contextmanager
    def stopped_before_admission(self):
        repo.update_run(run["id"], status="cancelled")
        yield

    monkeypatch.setattr(AIConnectionService, "authority_guard", stopped_before_admission)
    tool = next(item for item in engineering_tools() if item.name == "calculate_engineering")
    with pytest.raises(InterruptedError, match="no longer active"):
        await tool.invoke(
            ctx,
            {"method": "product", "inputs": {"quantity": "10", "factor": "2"}},
            invocation_id="stopped",
        )
    assert not any(event["kind"] == "calculation_completed" for event in repo.run_events(run["id"]))


@pytest.mark.asyncio
async def test_a_calculation_missing_an_input_says_which_inputs_it_needs(tmp_path):
    from quantix.ai_tools import ToolArgumentError
    from quantix.engineering_tools import engineering_tools
    from quantix.office_tools import OfficeContext
    from quantix.repository import Repository

    repo = Repository(tmp_path)
    tender = repo.create_tender("Calculator inputs")
    run = repo.create_run(tender["id"], "manager", "Measure")
    context = OfficeContext(repo, tender["id"], run["id"])
    tool = next(item for item in engineering_tools() if item.name == "calculate_engineering")
    with pytest.raises(
        ToolArgumentError, match="needs inputs.quantity, inputs.factor; missing inputs.factor"
    ):
        await tool.invoke(
            context,
            {"method": "product", "inputs": {"quantity": "12"}},
            invocation_id="calc-missing",
        )


@pytest.mark.asyncio
async def test_precision_given_as_decimal_places_is_understood_and_a_bad_one_can_be_corrected(
    tmp_path,
):
    from quantix.ai_tools import ToolArgumentError
    from quantix.engineering_tools import engineering_tools
    from quantix.office_tools import OfficeContext

    repo = Repository(tmp_path)
    tender = repo.create_tender("Synthetic precision")
    run = repo.create_run(tender["id"], "manager", "Price the concrete")
    ctx = OfficeContext(repo, tender["id"], run["id"])
    tool = next(item for item in engineering_tools() if item.name == "calculate_engineering")
    result = json.loads(
        await tool.invoke(
            ctx,
            {
                "method": "product",
                "inputs": {"quantity": "410", "factor": "565.555"},
                "precision": "2",
            },
            invocation_id="places",
        )
    )
    assert result["outputs"]["product"] == "231877.55"
    # A wrong value goes back to the model to fix instead of ending the job.
    with pytest.raises(ToolArgumentError, match="rounding step"):
        await tool.invoke(
            ctx,
            {"method": "product", "inputs": {"quantity": "1", "factor": "2"}, "precision": "0.5"},
            invocation_id="bad",
        )
