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
