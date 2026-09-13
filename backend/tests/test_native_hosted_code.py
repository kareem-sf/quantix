"""Approved synthetic source uploads and provider-generated private artifacts."""
import hashlib
from contextlib import asynccontextmanager
from types import SimpleNamespace

import pytest
from pydantic_ai.messages import ModelResponse, NativeToolCallPart, TextPart
from pydantic_ai.models import ModelRequestParameters
from pydantic_ai.native_tools import CodeExecutionTool

from quantix.ai_generation_models import NativeToolGrant
from quantix.native_hosted_code import HostedCodeExecution, NativeArtifactService
from quantix.repository import Repository


class Client:
    def __init__(self, *, fail_delete=False):
        self.uploads, self.deleted, self.downloads = [], [], []
        self.fail_delete = fail_delete
        self.files = SimpleNamespace(create=self.create, delete=self.delete)
        content = SimpleNamespace(with_streaming_response=SimpleNamespace(retrieve=self.download))
        self.containers = SimpleNamespace(delete=self.delete, files=SimpleNamespace(content=content))

    async def create(self, *, file, purpose):
        self.uploads.append((file, purpose))
        return SimpleNamespace(id="file-owned")

    async def delete(self, identifier):
        if self.fail_delete:
            raise ConnectionError("fixture transport")
        self.deleted.append(identifier)

    @asynccontextmanager
    async def download(self, file_id, *, container_id):
        self.downloads.append((container_id, file_id))
        async def chunks():
            yield b"generated "
            yield b"draft"
        yield SimpleNamespace(iter_bytes=chunks)


def workspace(tmp_path, *, client=None):
    repo = Repository(tmp_path)
    tender = repo.create_tender("Synthetic hosted code")
    run = repo.create_run(tender["id"], "manager")
    content = b"approved original bytes"
    digest = hashlib.sha256(content).hexdigest()
    (repo.objects / digest).write_bytes(content)
    artifact, _ = repo.register_artifact(tender["id"], "source.txt", digest, len(content), {"kind": "text", "status": "extracted"})
    context = SimpleNamespace(repo=repo, tender_id=tender["id"], run_id=run["id"])
    grant = NativeToolGrant(approval_id="review", tender_id=tender["id"], run_id=run["id"], connection_id="connection", connection_revision=1,
        model_id="fixture", source_scope_fingerprint="a" * 64, native_tools=["code_execution"], max_calls_per_request=2,
        uploaded_artifacts=[{"artifact_id": artifact["id"], "version": 1, "content_hash": digest}], hosted_code_spend_usd=1,
        code_execution_price={"per_session_usd": .03, "source": "https://developers.openai.com/api/docs/pricing", "as_of": "2026-09-12"})
    connection = {"id": "connection", "revision": 1, "provider_id": "openai", "protocol": "openai_responses", "_native_tool_grant": grant.model_dump()}
    route = {"model_id": "fixture", "native_tools": ["code_execution"], "max_native_tool_calls": 2}
    client = client or Client()
    return repo, context, artifact, client, HostedCodeExecution(context, connection, route, client)


@pytest.mark.asyncio
async def test_upload_requires_reservation_and_preserves_exact_source_bytes(tmp_path):
    repo, context, artifact, client, hosted = workspace(tmp_path)
    params = ModelRequestParameters(native_tools=[CodeExecutionTool()])
    with pytest.raises(ValueError, match="Reserve"):
        await hosted.prepare(params, None)
    assert client.uploads == []
    prepared = await hosted.prepare(params, "reservation")
    assert client.uploads == [(("source.txt", b"approved original bytes"), "assistants")]
    assert prepared.native_tools[0].files[0].file_id == "file-owned"
    assert repo.object_path(context.tender_id, artifact["id"]).read_bytes() == b"approved original bytes"
    hosted.request_rejected("reservation")  # No inference is made by this upload-only test.
    await hosted.close()
    assert client.deleted == ["file-owned"]


@pytest.mark.asyncio
async def test_changed_source_is_rejected_before_any_upload(tmp_path):
    repo, context, artifact, client, hosted = workspace(tmp_path)
    repo.object_path(context.tender_id, artifact["id"]).write_bytes(b"changed bytes")
    with pytest.raises(ValueError, match="reviewed"):
        await hosted.prepare(ModelRequestParameters(native_tools=[CodeExecutionTool()]), "reservation")
    assert client.uploads == []


@pytest.mark.asyncio
async def test_arithmetic_with_zero_selected_originals_uploads_nothing(tmp_path):
    _, _, _, client, hosted = workspace(tmp_path)
    hosted.grant = hosted.grant.model_copy(update={"uploaded_artifacts": []})
    prepared = await hosted.prepare(ModelRequestParameters(native_tools=[CodeExecutionTool()]), "reservation")
    assert client.uploads == [] and prepared.native_tools[0].files == []


@pytest.mark.asyncio
async def test_generated_file_is_owned_private_and_not_an_accepted_source(tmp_path):
    repo, context, artifact, client, hosted = workspace(tmp_path)
    response = ModelResponse(parts=[NativeToolCallPart(tool_name="code_execution", args={"container_id": "cntr-owned", "code": "print(1)"}, tool_call_id="call", provider_name="openai"),
        TextPart("Created a draft.", provider_name="openai", provider_details={"annotations": [{"type": "container_file_citation", "container_id": "cntr-owned", "file_id": "cfile-owned", "filename": "../../draft.csv"}]})])
    await hosted.collect(response)
    saved = NativeArtifactService(repo).list(context.tender_id, context.run_id)
    assert saved[0].filename == "draft.csv" and saved[0].status == "unreviewed"
    path = NativeArtifactService(repo).path(context.tender_id, saved[0].id)
    assert path.is_relative_to(repo.home / "outputs" / "native-code") and path.read_bytes() == b"generated draft"
    assert len(repo.list_artifacts(context.tender_id)) == 1
    assert saved[0].input_artifacts[0].artifact_id == artifact["id"]
    await hosted.close()
    assert "cntr-owned" in client.deleted


@pytest.mark.asyncio
async def test_cleanup_failure_retains_usage_uncertainty_and_blocks_unsafe_retry(tmp_path):
    repo, context, _, client, hosted = workspace(tmp_path, client=Client(fail_delete=True))
    await hosted.prepare(ModelRequestParameters(native_tools=[CodeExecutionTool()]), "reservation")
    usage = {"usage_complete": True}
    hosted.observe(ModelResponse(parts=[]), "reservation")
    reported = []
    hosted.defer_usage(usage, "reservation", lambda value, identifier: reported.append((dict(value), identifier)))
    assert reported == []
    await hosted.close()
    assert reported[0][0]["usage_complete"] is False
    receipts = NativeArtifactService(repo).pending_cleanup(context.tender_id)
    assert len(receipts) == 1
    with pytest.raises(ValueError, match="previous uncertain"):
        HostedCodeExecution(context, hosted.connection, hosted.route, client)


@pytest.mark.asyncio
@pytest.mark.parametrize("search_calls", [None, 1, 7])
async def test_real_openai_sdk_native_wire_is_bounded_and_accounts_after_cleanup(tmp_path, monkeypatch, search_calls):
    import json

    import httpx2
    from openai import AsyncOpenAI
    from pydantic import BaseModel
    from pydantic_ai.models.openai import OpenAIResponsesModel
    from pydantic_ai.providers.openai import OpenAIProvider

    from quantix.ai_api_engine import run_model
    from quantix.ai_api_provider import APIModelBinding, build_model_settings
    from quantix.native_provider_transport import NativeCodeTransport
    repo, context, _, _, original = workspace(tmp_path)
    grant = original.grant.model_copy(update={"model_id": "gpt-6-astra"})
    connection = {**original.connection, "_model": {"model_id": "gpt-6-astra", "source": "provider",
        "capabilities": {"code_execution": True, "web_search": True, "streaming": False, "context_window": 1000000}}}
    route = {**original.route, "model_id": "gpt-6-astra", "max_output_tokens": 1024}
    if search_calls is not None:
        route = {**route, "web_search": True, "max_search_calls": search_calls,
                 "native_tools": ["web_search", "code_execution"]}
        grant = grant.model_copy(update={"native_tools": ["web_search", "code_execution"]})
    requests, admitted, reports, deleted = [], [], [], []
    def handler(request):
        path = request.url.path
        requests.append(path)
        if request.method == "DELETE":
            deleted.append(path)
            return httpx2.Response(200, json={"id": path.rsplit("/", 1)[-1], "object": "file", "deleted": True})
        if path.endswith("/files"):
            assert admitted and b"approved original bytes" in request.content
            return httpx2.Response(200, json={"id": "file-owned", "object": "file", "bytes": 23, "created_at": 1, "filename": "source.txt", "purpose": "assistants"})
        if path.endswith("/content"):
            return httpx2.Response(200, content=b"private generated output")
        assert path.endswith("/responses")
        body = json.loads(request.content)
        assert body["max_tool_calls"] == min(2, search_calls or 2) and body["store"] is False
        assert any(tool["type"] == "web_search" for tool in body["tools"]) == (search_calls is not None)
        code = next(tool for tool in body["tools"] if tool["type"] == "code_interpreter")
        assert code["container"] == {"type": "auto", "file_ids": ["file-owned"], "memory_limit": "1g", "network_policy": {"type": "disabled"}}
        return httpx2.Response(200, json={"id": "response-fixture", "object": "response", "created_at": 1, "status": "completed", "model": "gpt-6-astra",
            "output": [{"id": "ci-owned", "type": "code_interpreter_call", "status": "completed", "container_id": "cntr-owned", "code": "print(42)", "outputs": []},
                {"id": "msg-owned", "type": "message", "status": "completed", "role": "assistant", "content": [{"type": "output_text", "text": "Generated a file.", "annotations": [{"type": "container_file_citation", "container_id": "cntr-owned", "file_id": "cfile-owned", "filename": "draft.csv", "start_index": 0, "end_index": 1}]}]},
                {"id": "fc-owned", "type": "function_call", "status": "completed", "call_id": "call-output", "name": "final_result", "arguments": '{"summary":"Generated draft ready."}'}],
            "usage": {"input_tokens": 100, "output_tokens": 20, "total_tokens": 120, "input_tokens_details": {"cached_tokens": 0}, "output_tokens_details": {"reasoning_tokens": 0}}})
    @asynccontextmanager
    async def binding(selected, saved, credentials):
        async with httpx2.AsyncClient(transport=NativeCodeTransport(httpx2.MockTransport(handler))) as http:
            async with AsyncOpenAI(api_key="fixture-secret", http_client=http, max_retries=0) as client:
                model = OpenAIResponsesModel(selected["model_id"], provider=OpenAIProvider(openai_client=client))
                yield APIModelBinding(model, build_model_settings(selected, saved), client)
    monkeypatch.setattr("quantix.ai_api_engine.model_for_route", binding)
    monkeypatch.setattr("quantix.ai_native_tools.grant_for_execution", lambda *args: grant)
    class Output(BaseModel):
        summary: str
    def reported(usage, reservation):
        assert any("cntr-owned" in path for path in deleted)
        reports.append(usage)
    result = await run_model(route, connection, {}, context, "Use the approved source and create a draft.", Output, definitions=[],
        before_request=lambda *args: admitted.append(args) or "reservation", on_response=reported)
    assert result["output"].summary == "Generated draft ready."
    assert len(admitted) == len(reports) == 1 and reports[0]["code_execution_container_ids"] == ["cntr-owned"]
    assert reports[0]["usage_complete"] is True and len(result["native_artifacts"]) == 1
    assert "fixture-secret" not in str(result)
