"""Exercise the bundled SDK loop with a synthetic model and real local services."""

import asyncio
import json
import sys
from contextlib import asynccontextmanager
from pathlib import Path
from types import SimpleNamespace

import mcp.types as mcp_types
import pytest
from pydantic_ai import Agent
from pydantic_ai.messages import ModelRequest, ModelResponse, ToolCallPart, UserPromptPart
from pydantic_ai.models import CompletedStreamedResponse
from pydantic_ai.models.function import FunctionModel
from pydantic_ai.usage import RequestUsage
from test_catalog_authority import configured_office
from test_dynamic_staff import _order, _profile
from test_repository import source

from quantix import office
from quantix.ai_api_provider import APIModelBinding
from quantix.staff_assignments import StaffAssignmentService
from quantix.staff_runtime_models import StaffCompletedDraft, StaffProviderOutput
from quantix.staff_store import StaffStore


@pytest.mark.asyncio
async def test_real_sdk_specialist_tool_loop_preserves_usage_and_proposed_records(tmp_path, monkeypatch):
    repo, tender, _connections, _connection, _model, policy, _route = configured_office(
        tmp_path, monkeypatch, model_id="gpt-6-astra", reasoning="xhigh"
    )
    tender_id = tender["id"]
    _, evidence, _ = source(repo, tender_id)
    plan = repo.create_plan(
        tender_id,
        "Synthetic concrete review",
        [{
            "title": "Review concrete",
            "description": "Check the supplied concrete clause.",
            "role": "Concrete specification reviewer",
            "source_ids": [evidence["id"]],
        }],
    )

    from quantix.jobs import JobManager
    from quantix.plan_review import PlanReviewService

    jobs = JobManager(repo, object())
    review_service = PlanReviewService(
        repo,
        save_runs_in_transaction=jobs.queue_approved_plan_runs,
        schedule_after_commit=jobs.schedule_approved_plan_runs,
    )
    condition = "Use supplied sources only; no market research."
    review = review_service.review(tender_id, plan["id"])
    approval = review_service.approve_and_start(
        tender_id,
        plan["id"],
        {"fingerprint": review["fingerprint"], "engineer_confirmed": True, "rationale": condition},
    )
    root_id = approval["work_intents"][0]["run_id"]

    phases = []
    manager_requests = 0
    staff_requests = 0

    async def scripted(messages, info):
        nonlocal manager_requests, staff_requests

        assert info.model_settings["openai_reasoning_effort"] == "xhigh"
        assert "synthetic-key" not in str(messages) + str(info.instructions)
        for definition in info.function_tools:
            assert definition.parameters_json_schema["additionalProperties"] is False
            assert "tender_id" not in definition.parameters_json_schema["properties"]

        names = {definition.name for definition in info.function_tools}
        if "create_staff" in names:
            manager_requests += 1
            phases.append("manager")
            assert info.instructions.strip() == office.INSTRUCTIONS.strip()
            turn = next(part.content for message in messages if isinstance(message, ModelRequest)
                        for part in message.parts if isinstance(part, UserPromptPart))
            prompt_data, _ = json.JSONDecoder().raw_decode(turn)
            assert prompt_data["engineer_approved_scope"]["rationale"] == condition
            assert "consult_specialist" not in names
            if manager_requests == 1:
                output = ToolCallPart("read_source", {"source_id": evidence["id"]})
            elif manager_requests == 2:
                profile = _profile(role="Concrete specification reviewer").model_dump(mode="json")
                profile["requested_tool_ids"] = ["read_source"]
                order = _order(source_ids=[evidence["id"]]).model_dump(mode="json")
                output = ToolCallPart(
                    "create_staff", {"profile": profile, "work_order": order}
                )
            elif manager_requests == 3:
                staff = StaffStore(repo).list_staff(tender_id)[0]
                order = StaffStore(repo).list_work_orders(tender_id, staff.id)[0]
                output = ToolCallPart(
                    "execute_staff",
                    {
                        "staff_id": staff.id,
                        "work_order_id": order.id,
                        "route_option_id": review["delegation"]["route_options"][0]["id"],
                    },
                )
            elif manager_requests == 4:
                output = ToolCallPart(
                    info.output_tools[0].name,
                    office.OfficeOutput(summary="The scoped colleague is queued.").model_dump(mode="json"),
                )
            else:
                assignment = StaffAssignmentService(repo).list(tender_id)[0]
                assert assignment.result_id
                if manager_requests == 5:
                    output = ToolCallPart(
                        "read_staff_result", {"result_id": assignment.result_id}
                    )
                elif manager_requests == 6:
                    output = ToolCallPart(
                        "read_source", {"source_id": evidence["id"]}
                    )
                else:
                    assert manager_requests == 7
                    payload = office.OfficeOutput(
                        summary="The Manager consolidated the colleague's review.",
                        source_ids=[evidence["id"]],
                        findings=[{
                            "title": "Manager consolidation",
                            "detail": "The source-backed colleague result is ready for engineer review.",
                            "kind": "observation",
                            "source_ids": [evidence["id"]],
                        }],
                    )
                    output = ToolCallPart(
                        info.output_tools[0].name, payload.model_dump(mode="json")
                    )
            return ModelResponse(
                [output],
                usage=RequestUsage(input_tokens=100, output_tokens=50),
                model_name="gpt-6-astra",
            )

        staff_requests += 1
        phases.append("staff")
        assert names == {"read_source"}
        packet_text = str(messages) + str(info.instructions)
        assert condition in packet_text
        assert repo.get_run(root_id)["instruction"] in packet_text
        if staff_requests == 1:
            output = ToolCallPart("read_source", {"source_id": evidence["id"]})
        else:
            assert staff_requests == 2
            payload = StaffProviderOutput(
                result=StaffCompletedDraft(
                    output=office.OfficeOutput(
                        summary="The colleague inspected the concrete clause.",
                        source_ids=[evidence["id"]],
                        findings=[{
                            "title": "Child proposed source observation",
                            "detail": "The supplied source states 30 MPa.",
                            "kind": "observation",
                            "source_ids": [evidence["id"]],
                        }],
                    )
                )
            )
            output = ToolCallPart(
                info.output_tools[0].name, payload.model_dump(mode="json")
            )
        return ModelResponse(
            [output],
            usage=RequestUsage(input_tokens=100, output_tokens=50),
            model_name="gpt-6-astra",
        )

    class ScriptedToolModel(FunctionModel):
        @asynccontextmanager
        async def request_stream(self, messages, model_settings, parameters, run_context=None):
            # This fixture verifies deterministic tool dispatch and exact token
            # accounting. Incremental chunks are tested in test_ai_api_stream.
            response = await self.request(messages, model_settings, parameters)
            yield CompletedStreamedResponse(response, model_request_parameters=parameters)

    scripted_errors = []

    async def capture_scripted_error(*args, **kwargs):
        try:
            return await scripted(*args, **kwargs)
        except Exception as error:
            import traceback
            frame = traceback.extract_tb(error.__traceback__)[-1]
            scripted_errors.append((type(error).__name__, str(error)[:300], frame.line))
            raise

    model = ScriptedToolModel(capture_scripted_error, model_name="gpt-6-astra")

    @asynccontextmanager
    async def binding(route, connection, credentials):
        assert route["model_id"] == "gpt-6-astra"
        assert credentials == {"api_key": "synthetic-key"}
        yield APIModelBinding(
            model=model,
            settings={"openai_reasoning_effort": route["reasoning"]},
            client=None,
        )

    original_run = Agent.run

    async def checked_run(self, *args, **kwargs):
        assert self.instrument is False
        assert kwargs["usage_limits"].request_limit <= 12
        return await original_run(self, *args, **kwargs)

    monkeypatch.setattr("quantix.ai_api_engine.model_for_route", binding)
    monkeypatch.setattr(Agent, "run", checked_run)
    await asyncio.gather(*list(jobs.tasks.values()))

    root = repo.get_run(root_id)
    assert root["status"] == "completed", (root.get("error"), phases, scripted_errors)
    assert phases == ["manager", "manager", "manager", "manager", "staff", "staff", "manager", "manager", "manager"]
    assert root["usage"]["requests"] == 9
    assert root["usage"]["input_tokens"] == 900
    assert root["usage"]["output_tokens"] == 450
    with repo.db.connect() as conn:
        assert policy._totals(conn, tender_id, root_id)[2] == 9
        usage_rows = conn.execute(
            "SELECT data_json FROM ai_usage WHERE tender_id=? AND run_id=? ORDER BY created_at",
            (tender_id, root_id),
        ).fetchall()
    assert len(usage_rows) == 9
    assert sum(json.loads(row[0])["requests"] for row in usage_rows) == 9

    saved_assignment = StaffAssignmentService(repo).list(tender_id)[0]
    saved_result = StaffAssignmentService(repo).get_result(tender_id, saved_assignment.result_id)
    assert saved_assignment.status == "completed"
    assert saved_result.source_ids_read == (evidence["id"],)
    assert saved_result.office_output.findings[0].title == "Child proposed source observation"
    assert [finding["title"] for finding in repo.list_findings(tender_id)] == ["Manager consolidation"]
    assert len(StaffStore(repo).list_staff(tender_id)) == 1
    await jobs.close()


@pytest.mark.asyncio
async def test_worker_dispatches_supported_execute_operation_without_provider(tmp_path, monkeypatch):
    """The child runtime uses the worker's advertised execute operation."""

    sys.path.insert(0, str(Path(__file__).parents[1] / "ai_worker"))
    from quantix_ai_worker import execution as worker_execution
    from quantix_ai_worker import remote as worker_remote
    from quantix_ai_worker.server import Worker

    from quantix.ai_worker_client import AIWorkerClient

    repo = SimpleNamespace(
        home=tmp_path,
        get_run=lambda run_id: {"id": run_id, "status": "running"},
    )
    host = AIWorkerClient(repo)
    account = {
        "id": "synthetic-worker",
        "provider_id": "codex",
        "protocol": "custom",
        "billing": "metered",
        "auth_type": "api_key",
    }
    account_home = tmp_path / "ai-runtimes" / account["id"]
    worker = Worker.__new__(Worker)
    worker.connection = account
    worker.account_home = account_home
    worker.accounts = None
    captured = {}

    async def fake_execute(route, connection, credentials, context, instruction, output_schema):
        captured.update(
            route=route,
            connection=connection,
            credentials=credentials,
            instruction=instruction,
            output_schema=output_schema,
        )
        return {
            "output": {"summary": "Synthetic worker response."},
            "usage": {"requests": 1, "input_tokens": 2, "output_tokens": 3, "usage_complete": True},
            "web_sources": [],
        }

    @asynccontextmanager
    async def execution_context(_payload):
        yield SimpleNamespace()

    monkeypatch.setattr(worker_execution, "execute_api", fake_execute)
    monkeypatch.setattr(worker_remote, "execution_context", execution_context)

    class InProcessWorkerSession:
        async def call_tool(self, operation, arguments):
            result = await worker.operation(operation, arguments)
            return mcp_types.CallToolResult(
                content=[], structuredContent={"ok": True, "result": result}
            )

    @asynccontextmanager
    async def session(_connection):
        yield InProcessWorkerSession()

    monkeypatch.setattr(host, "_session", session)
    context = SimpleNamespace(repo=repo, run_id="run-worker")
    route = {
        "connection_id": account["id"],
        "model_id": "synthetic-model",
        "max_output_tokens": 128,
        "web_search": False,
        "max_search_calls": 0,
    }
    result = await host.execute(
        route,
        {**account, "_execution_limits": {"max_requests": 2}, "_model": {}},
        {"api_key": "synthetic-key"},
        context,
        "Synthetic execute instruction",
        office.OfficeOutput,
    )

    assert result["output"].summary == "Synthetic worker response."
    assert captured["connection"]["_operation"] == "execute"
    assert captured["route"] == route
    assert captured["credentials"] == {"api_key": "synthetic-key"}
    assert captured["instruction"] == "Synthetic execute instruction"
    assert captured["output_schema"]["additionalProperties"] is False
