import asyncio
import json
from types import SimpleNamespace

import pytest
from fastapi import FastAPI
from fastapi.testclient import TestClient

from quantix.ai_tools import tool
from quantix.code_composition import CodeCompositionService
from quantix.code_composition_models import CompositionRequest
from quantix.local_code_inspection import LocalCodeInspection
from quantix.local_code_records import write_local_record
from quantix.repository import Repository
from quantix.sandbox_protocol import sha256
from quantix.sandbox_routes import create_router


def workspace(tmp_path):
    repo = Repository(tmp_path)
    tender = repo.create_tender("Synthetic local-code inspection")
    run = repo.create_run(tender["id"], "manager")
    repo.update_run(run["id"], status="running")
    context = SimpleNamespace(
        repo=repo, tender_id=tender["id"], run_id=run["id"], actor_id="manager"
    )
    return repo, context, LocalCodeInspection(repo)


@pytest.mark.asyncio
async def test_real_monty_receipt_readback_and_scope(tmp_path):
    repo, context, service = workspace(tmp_path)
    result = await CodeCompositionService(repo.home).execute(
        CompositionRequest(code="quantity * factor", inputs={"quantity": 6, "factor": 7}),
        context=context,
        tools=[],
    )
    page = service.page(context.tender_id, context.run_id)
    assert page.items[0].id == result.id and page.items[0].actor_id == "manager"
    detail = service.detail(context.tender_id, context.run_id, "monty", result.id)
    assert detail.output == 42 and detail.code == "quantity * factor"
    assert (
        detail.input_values == {"quantity": 6, "factor": 7}
        and detail.record_integrity == "verified"
    )
    assert list(detail.input_values) == ["quantity", "factor"]
    assert detail.limits["seconds"] == 10
    other = repo.create_tender("Other synthetic Tender")
    with pytest.raises(KeyError):
        service.detail(other["id"], context.run_id, "monty", result.id)
    with pytest.raises(KeyError):
        service.detail(
            context.tender_id, context.run_id, "monty", result.id, actor_id="other-actor"
        )
    path = repo.home / "code" / "receipts" / f"{result.id}.json"
    changed = json.loads(path.read_text())
    changed["code"] = "wrong code"
    path.write_text(json.dumps(changed))
    with pytest.raises(ValueError, match="receipt changed"):
        service.detail(context.tender_id, context.run_id, "monty", result.id)


@pytest.mark.asyncio
async def test_real_monty_start_and_cancel_are_discoverable(tmp_path):
    repo, context, service = workspace(tmp_path)
    entered = asyncio.Event()

    @tool
    async def wait_here(ctx):
        entered.set()
        await asyncio.Event().wait()

    task = asyncio.create_task(
        CodeCompositionService(repo.home).execute(
            CompositionRequest(code="await wait_here()"), context=context, tools=[wait_here]
        )
    )
    await asyncio.wait_for(entered.wait(), timeout=10)
    page = service.page(context.tender_id, context.run_id)
    assert page.items[0].status == "running" and page.items[0].phase == "calling_reviewed_tool"
    task.cancel()
    with pytest.raises(asyncio.CancelledError):
        await task
    detail = service.detail(context.tender_id, context.run_id, "monty", page.items[0].id)
    assert detail.run.status == "cancelled" and detail.calls[0]["status"] == "attempted"


def python_fixture(repo, context):
    identifier = "a" * 32
    folder = repo.home / "code" / "runs" / identifier
    (folder / "inputs").mkdir(parents=True)
    (folder / "outputs").mkdir()
    (folder / "code.py").write_text("print('synthetic')")
    original, output = b"synthetic original", b"<html>synthetic output</html>"
    (folder / "inputs" / "source.txt").write_bytes(original)
    (folder / "outputs" / "result.html").write_bytes(output)
    write_local_record(
        folder / "receipt.json",
        {
            "id": identifier,
            "status": "completed",
            "created_at": "2026-09-13T00:00:00Z",
            "code_sha256": sha256(b"print('synthetic')"),
            "limits": {"seconds": 120},
            "image_id": "sha256:" + "c" * 64,
            "inputs": [
                {
                    "id": "synthetic-source",
                    "name": "source.txt",
                    "sha256": sha256(original),
                    "size_bytes": len(original),
                    "source_version": "1",
                }
            ],
            "outputs": [
                {"name": "result.html", "sha256": sha256(output), "size_bytes": len(output)}
            ],
        },
        context=context,
        engine="python",
        phase="completed",
    )
    return identifier, folder, output


def test_python_saved_code_input_output_hashes_and_attachment_only(tmp_path):
    repo, context, service = workspace(tmp_path)
    identifier, folder, output = python_fixture(repo, context)
    detail = service.detail(context.tender_id, context.run_id, "python", identifier)
    assert detail.inputs[0].source_version == "1" and detail.outputs[0].name == "result.html"
    app = FastAPI()
    app.include_router(create_router(repo))
    client = TestClient(app)
    url = f"/api/tenders/{context.tender_id}/local-code-runs/python/{identifier}/files/output/result.html?root_run_id={context.run_id}"
    response = client.get(url)
    assert response.content == output and response.headers["content-disposition"].startswith(
        "attachment"
    )
    assert response.headers["content-type"] == "application/octet-stream"
    assert str(repo.home) not in json.dumps(detail.model_dump())
    for path in (
        folder / "code.py",
        folder / "inputs" / "source.txt",
        folder / "outputs" / "result.html",
    ):
        old = path.read_bytes()
        path.write_bytes(b"changed")
        with pytest.raises(ValueError, match="changed"):
            service.detail(context.tender_id, context.run_id, "python", identifier)
        path.write_bytes(old)
    (folder / "outputs" / "result.html").write_bytes(b"changed")
    assert client.get(url).status_code == 409
    with pytest.raises(KeyError):
        service.file(
            context.tender_id, context.run_id, "python", identifier, "output", "../code.py"
        )


def test_stopped_root_and_legacy_attribution_are_truthful(tmp_path):
    repo, context, service = workspace(tmp_path)
    identifier, folder, _ = python_fixture(repo, context)
    value = json.loads((folder / "receipt.json").read_text())
    for field in ("execution_identity", "record_version", "record_sha256"):
        value.pop(field)
    value["status"] = "running"
    (folder / "receipt.json").write_text(json.dumps(value))
    repo.update_run(context.run_id, status="cancelled")
    page = service.page(context.tender_id, context.run_id)
    assert page.items[0].legacy_attribution and page.items[0].status == "interrupted"
    assert service.page(context.tender_id, context.run_id, actor_id="manager").items == []
