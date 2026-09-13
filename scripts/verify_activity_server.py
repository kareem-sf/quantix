"""Synthetic-only browser acceptance server; never imported by the application.

Runs the real API, JobManager, controllers, budget ledger and tool policy. Only
the provider binding is substituted with deterministic Pydantic AI streams.
All state is isolated beneath ~/.quantix/runtime/verification/activity-2026-09-13.
No network model calls, OS credential writes, or customer files are used.
"""
from __future__ import annotations

import asyncio
import hashlib
import json
import socket
import sys
from contextlib import asynccontextmanager
from datetime import UTC, datetime
from pathlib import Path
from types import SimpleNamespace

PROJECT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(PROJECT / "backend"))

ROOT = Path.home() / ".quantix" / "runtime" / "verification" / "activity-2026-09-13"
TOKEN = "synthetic-activity-local-only-20260913"
PORT = 18765
MODEL = "gpt-5"


def seed(app, home):
    from quantix.ai_connections import AIConnectionService
    from quantix.ai_direct import direct_runtime_status
    from quantix.ai_policy import AIPolicyService
    from quantix.ai_setup_store import SetupStore
    from quantix.run_activity import ActivityRecorder

    repo = app.state.repo
    tender = repo.create_tender("Synthetic activity acceptance — training files only")
    body = ("SYNTHETIC TRAINING SPECIFICATION. Not a customer Tender.\n"
            "Section 3: The ground slab uses C35 concrete. Synthetic BOQ quantity: 120 m3.\n"
            "The Tender Manager must keep quantities unconfirmed until engineer review.\n")
    artifact, _ = repo.register_artifact(tender["id"], "Synthetic concrete specification.txt",
        hashlib.sha256(body.encode()).hexdigest(), len(body.encode()),
        {"kind": "text", "status": "extracted", "segments": [{"locator": "Section 3", "text": body}]})
    (repo.objects / artifact["content_hash"]).write_text(body, encoding="utf-8")
    evidence = repo.artifact_evidence(tender["id"], artifact["id"])[0]
    connections = AIConnectionService(repo)
    account = connections.create({"name": "SYNTHETIC provider — no external calls", "provider_id": "openai",
        "protocol": "openai_responses", "base_url": "https://synthetic-provider.invalid/v1",
        "auth_type": "api_key", "billing": "metered", "credentials": {"api_key": "synthetic-not-a-real-key"},
        "session_only": True})
    connections.save_model(account["id"], {"model_id": MODEL, "display_name": "Synthetic gpt-5 stream fixture",
        "capabilities": {"tools": True, "structured_output": True, "reasoning": ["high"], "streaming": True,
            "max_output_tokens": 20000, "context_window": 200000},
        "pricing": {"input_per_million": 1, "output_per_million": 2, "source": "Synthetic test prices only", "as_of": "2026-09-13"}}, source="provider")
    account = connections.get(account["id"])
    runtime = direct_runtime_status(account)
    if runtime["state"] != "ready":
        raise RuntimeError(runtime["detail"])
    store = SetupStore(repo)
    check = {"check": {"status": "passed", "model_id": MODEL}, "checked_revision": account["revision"],
        "checked_component_version": runtime["version"]}
    store.update(account["id"], selected_model_id=MODEL, **check)
    store.save_model_check(account["id"], MODEL, check)
    route = {"connection_id": account["id"], "model_id": MODEL, "reasoning": "high", "max_output_tokens": 8192,
        "web_search": False, "max_search_calls": 3}
    AIPolicyService(repo).update(tender["id"], {"allowed_connection_ids": [account["id"]], "manager": route,
        "run_budget_usd": 100, "tender_budget_usd": 1000, "max_requests": 20,
        "engineer_confirmed": True, "rationale": "Synthetic browser acceptance authority; no external billing."})
    historical = repo.create_run(tender["id"], "manager", "Synthetic historical pagination fixture")
    context = SimpleNamespace(repo=repo, tender_id=tender["id"], run_id=historical["id"])
    recorder = ActivityRecorder(context)
    operation = recorder.start("model_request", "Synthetic historical request fixture", {"instructions": "Synthetic historical source review."}, provider="openai", model=MODEL, phase="prepared")
    with repo.atomic():
        for index in range(325):
            tool = recorder.start("tool", f"Synthetic historical source check {index + 1}",
                {"arguments": {"source_id": evidence["id"], "check": index + 1}}, tool="read_source", parent_operation_id=operation)
            recorder.record(tool, "tool", "completed", f"Synthetic historical source check {index + 1} completed",
                {"result": {"source_ids": [evidence["id"]], "text": f"Synthetic check {index + 1}: C35 concrete, 120 m3."}})
    recorder.start("reasoning_summary", "Synthetic historical public summary", {"text": "SYNTHETIC PUBLIC SUMMARY. " + "The source was compared with the synthetic BOQ. " * 100},
        phase="completed", parent_operation_id=operation)
    recorder.record(operation, "model_request", "completed", "Synthetic historical fixture completed", {"usage": {"requests": 1, "input_tokens": 120, "output_tokens": 80}})
    repo.add_message(tender["id"], "manager", "Synthetic historical review: the training specification states C35 concrete and 120 m³. These quantities remain unconfirmed.", [evidence["id"]], run_id=historical["id"])
    repo.update_run(historical["id"], status="completed", progress=100, result={"summary": "Synthetic historical review completed."})
    return {"tender_id": tender["id"], "source_id": evidence["id"], "artifact_id": artifact["id"],
        "connection_id": account["id"], "historical_run_id": historical["id"], "home": str(home)}


def install_synthetic_provider(fixture):
    from pydantic_ai.messages import ModelRequest, ToolReturnPart
    from pydantic_ai.models.function import (
        DeltaThinkingPart,
        DeltaToolCall,
        FunctionModel,
    )
    from quantix import ai_api_engine
    from quantix.ai_api_provider import APIModelBinding

    @asynccontextmanager
    async def binding(route, connection, credentials):
        if connection.get("id") != fixture["connection_id"] or credentials.get("api_key") != "synthetic-not-a-real-key":
            raise ValueError("This isolated server permits only its synthetic provider fixture.")
        async def stream(messages, info):
            await asyncio.sleep(1)
            tools = {tool.name: tool for tool in info.function_tools}
            if not tools:
                output = {"kind": "engineering"}
            else:
                returned = [part.tool_name for message in messages if isinstance(message, ModelRequest)
                    for part in message.parts if isinstance(part, ToolReturnPart)]
                yield {0: DeltaThinkingPart(content="SYNTHETIC PROVIDER SUMMARY: I will read the training specification, compare its stated quantity, and keep engineer approval separate. ")}
                await asyncio.sleep(1)
                if "search_sources" not in returned:
                    yield {1: DeltaToolCall(name="search_sources", json_args=json.dumps({"query": "concrete", "limit": 5}), tool_call_id="synthetic-search")}
                    return
                if "read_source" not in returned:
                    yield {1: DeltaToolCall(name="read_source", json_args=json.dumps({"source_id": fixture["source_id"], "reread": True}), tool_call_id="synthetic-read")}
                    return
                output = {"summary": "Synthetic review complete: Section 3 specifies C35 concrete and the training BOQ states 120 m³. The quantity remains unconfirmed. Next, review the cited source before accepting any quantity.", "source_ids": [fixture["source_id"]]}
            raw = json.dumps(output)
            yield {2: DeltaToolCall(name=info.output_tools[0].name, json_args=raw[:len(raw)//2], tool_call_id="synthetic-output")}
            await asyncio.sleep(2)
            yield {2: DeltaToolCall(json_args=raw[len(raw)//2:])}
        yield APIModelBinding(FunctionModel(stream_function=stream, model_name=MODEL), {"max_tokens": route["max_output_tokens"]}, None)
    ai_api_engine.model_for_route = binding


def main():
    with socket.socket() as probe:
        probe.bind(("127.0.0.1", PORT))
    home = ROOT / ("server-" + datetime.now(UTC).strftime("%Y%m%d-%H%M%S"))
    home.mkdir(parents=True, exist_ok=False)
    import uvicorn
    from quantix import api

    api.ALLOWED_ORIGINS = (*api.ALLOWED_ORIGINS, "http://127.0.0.1:1421")
    app = api.create_app(home, TOKEN)
    fixture = seed(app, home)
    install_synthetic_provider(fixture)
    manifest = {**fixture, "base_url": f"http://127.0.0.1:{PORT}", "api_base_url": f"http://127.0.0.1:{PORT}/api", "token": TOKEN,
        "synthetic_only": True, "provider_substitution": "Pydantic AI FunctionModel; no inference/network calls",
        "try_message": "Review the synthetic concrete specification.", "vite_port": 1421}
    (ROOT / "manifest.json").write_text(json.dumps(manifest, indent=2), encoding="utf-8")
    print(json.dumps(manifest), flush=True)
    uvicorn.run(app, host="127.0.0.1", port=PORT, log_level="warning", access_log=False)


if __name__ == "__main__":
    main()
