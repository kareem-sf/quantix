import asyncio
import importlib
import json
from types import SimpleNamespace

import pytest

from quantix import diagnostics
from quantix.office_tools import source_tools


class MemoryRepository:
    def setting(self, key, default=None):
        return default

    def atomic(self):
        from contextlib import nullcontext

        return nullcontext()

    """Contract fake until the independently owned repository is integrated."""

    def __init__(self):
        self.replies = []
        self.findings = []
        self.plans = []
        self.events = []
        self.evidence = {
            "source-1": {
                "id": "source-1",
                "artifact_id": "artifact-1",
                "artifact_name": "Spec.pdf",
                "relative_path": "Civil/Spec.pdf",
                "locator": "Page 2",
                "page": 2,
                "text": "The slab concrete shall achieve 35 MPa.",
                "kind": "text",
            },
            "source-2": {
                "id": "source-2",
                "artifact_id": "artifact-1",
                "artifact_name": "Spec.pdf",
                "relative_path": "Civil/Spec.pdf",
                "locator": "Page 3",
                "page": 3,
                "text": "The engineer must approve substitutions.",
                "kind": "text",
            },
        }

    def get_tender(self, tender_id):
        if tender_id != "tender-1":
            raise KeyError(tender_id)
        return {"id": tender_id, "name": "School extension"}

    def get_run(self, run_id):
        return {"id": run_id, "tender_id": "tender-1", "status": "running", "kind": "manager"}

    def overview(self, tender_id):
        self.get_tender(tender_id)
        return {
            "tender": self.get_tender(tender_id),
            "artifacts": 1,
            "evidence": 2,
            "coverage": {
                "registered": 1,
                "extracted": 1,
                "needs_attention": 0,
                "unsupported": 0,
                "failed": 0,
            },
            "areas": ["Civil"],
            "findings": [],
            "plan": None,
            "active_runs": [],
            "boq_count": 0,
            "private_home": r"C:\Users\private\Quantix",
        }

    def list_artifacts(self, tender_id):
        self.get_tender(tender_id)
        return [
            {
                "id": "artifact-1",
                "name": "Spec.pdf",
                "relative_path": "Civil/Spec.pdf",
                "status": "extracted",
                "kind": "pdf",
                "area": "Civil",
                "metadata": {"private_path": r"C:\private\original.pdf"},
            }
        ]

    def messages(self, tender_id):
        return []

    def list_tasks(self, tender_id):
        return []

    def list_plans(self, tender_id):
        self.get_tender(tender_id)
        return self.plans

    def approved_scope(self, tender_id, plan_id):
        self.get_tender(tender_id)
        if plan_id != "plan-1":
            raise KeyError(plan_id)
        return {
            "plan_id": plan_id,
            "title": "Concrete review",
            "tender_revision": 1,
            "rationale": "Review the specified strength only.",
        }

    def get_artifact(self, tender_id, artifact_id):
        self.get_tender(tender_id)
        if artifact_id != "artifact-1":
            raise KeyError(artifact_id)
        return {
            "id": artifact_id,
            "version": 1,
            "is_current": True,
            "name": "Spec.pdf",
            "relative_path": "Civil/Spec.pdf",
            "content_hash": "hash-1",
            "kind": "pdf",
        }

    def search_keyword(self, tender_id, query, limit=20, **_scope):
        self.get_tender(tender_id)
        rows = [self.evidence["source-1"]]
        return rows, {"scanned": len(rows), "truncated": False, "more": False, "ceiling": limit}

    def get_evidence(self, tender_id, evidence_id):
        self.get_tender(tender_id)
        return self.evidence[evidence_id]

    def artifact_evidence(self, tender_id, artifact_id, offset=0, limit=50):
        self.get_tender(tender_id)
        if artifact_id != "artifact-1":
            raise KeyError(artifact_id)
        return list(self.evidence.values())[offset : offset + limit]

    def add_message(self, tender_id, role, content, source_ids=None, run_id=None):
        row = dict(role=role, content=content, source_ids=source_ids, run_id=run_id)
        self.replies.append(row)
        return row

    def add_finding(self, tender_id, title, detail, kind, source_ids, origin="agent", run_id=None):
        row = dict(
            title=title,
            detail=detail,
            kind=kind,
            source_ids=source_ids,
            origin=origin,
            run_id=run_id,
            state="proposed",
        )
        self.findings.append(row)
        return row

    def create_plan(self, tender_id, title, tasks, run_id=None):
        row = dict(title=title, tasks=tasks, run_id=run_id, status="proposed")
        self.plans.append(row)
        return row

    def event(self, run_id, kind, message, data=None):
        self.events.append(dict(run_id=run_id, kind=kind, message=message, data=data))


def api_result_for(output, web_sources=None):
    return {
        "output": output,
        "web_sources": web_sources or [],
        "usage": {
            "requests": 1,
            "input_tokens": 100,
            "output_tokens": 50,
            "total_tokens": 150,
            "cached_input_tokens": 10,
            "reasoning_tokens": 25,
            "usage_complete": True,
            "request_details": [{"input_tokens": 100, "output_tokens": 50}],
        },
    }


@pytest.fixture(autouse=True)
def configured_memory_repository(tmp_path, monkeypatch, request):
    """Keep unit evidence fixtures while using real account and budget services."""
    from test_catalog_authority import configured_office

    from quantix.repository import Repository

    original = MemoryRepository.__init__

    def initialize(self):
        original(self)
        backing = Repository(tmp_path)
        tender = backing.create_tender("School extension")
        with backing.db.connect(write=True) as conn:
            conn.execute("UPDATE tenders SET id='tender-1' WHERE id=?", (tender["id"],))
        configured_office(
            tmp_path,
            monkeypatch,
            repo=backing,
            tender={"id": "tender-1"},
            model_id="gpt-6-astra",
            reasoning="xhigh",
            search=True,
        )
        run = backing.create_run("tender-1", "manager", "Synthetic work")
        with backing.db.connect(write=True) as conn:
            conn.execute("UPDATE runs SET id='run-1' WHERE id=?", (run["id"],))
        self.db, self.home = backing.db, backing.home
        self.atomic = backing.atomic
        self.list_runs = backing.list_runs
        self.get_task = backing.get_task

    monkeypatch.setattr(MemoryRepository, "__init__", initialize)

    def close_temp_writer():
        writer = diagnostics._writer
        if writer is None or writer.directory is None:
            return
        root = tmp_path.resolve()
        directory = writer.directory.resolve()
        if directory != root and root not in directory.parents:
            return
        writer.close()
        with diagnostics._writer_lock:
            if diagnostics._writer is writer:
                diagnostics._writer = None

    request.addfinalizer(close_temp_writer)


def office_module():
    return importlib.import_module("quantix.office")


def test_derived_measurement_source_keeps_its_proposal_origin_and_status():
    from quantix.office_tools import OfficeContext

    repo = MemoryRepository()
    repo.evidence["source-1"].update(
        kind="measurement",
        metadata={
            "origin": "engineer",
            "measurement_id": "measurement-1",
            "status": "proposed",
            "method": "engineer_marked_calibrated_proposal",
            "source_version": 1,
            "source_hash": "a" * 64,
        },
    )
    result = OfficeContext(repo, "tender-1", "run-1").source("source-1")
    assert result["kind"] == "measurement"
    assert result["metadata"]["origin"] == "engineer"
    assert result["metadata"]["status"] == "proposed"
    assert result["metadata"]["measurement_id"] == "measurement-1"
    assert result["metadata"]["source_version"] == "1"


async def call_api_tool(options, name, context, arguments):
    from quantix.office_tools import source_tools

    definitions = source_tools() + ([options["consult"]] if options.get("consult") else [])
    definition = next(item for item in definitions if item.name == name)
    return await definition.invoke(context, arguments)


@pytest.mark.asyncio
async def test_manager_prepares_source_bound_proposals_without_publishing(monkeypatch):
    office = office_module()
    repo = MemoryRepository()

    async def provider(route, connection, credentials, context, prompt, output_type, **kwargs):
        await call_api_tool(kwargs, "search_sources", context, {"query": "concrete", "limit": 5})
        return api_result_for(
            office.OfficeOutput.model_validate(
                {
                    "summary": "The concrete specification requires review.",
                    "source_ids": ["source-1"],
                    "findings": [
                        {
                            "title": "Concrete grade",
                            "detail": "Specified strength is 35 MPa.",
                            "kind": "requirement",
                            "source_ids": ["source-1"],
                        }
                    ],
                    "plan": {
                        "title": "Concrete package review",
                        "tasks": [
                            {
                                "title": "Check concrete scope",
                                "description": "Review concrete clauses and BOQ.",
                                "role": "Concrete specification reviewer",
                                "source_ids": ["source-1"],
                            }
                        ],
                    },
                }
            )
        )

    monkeypatch.setattr("quantix.ai_execution.execute_api", provider)
    result = await office.run_manager(repo, "tender-1", "run-1", "Review the package")
    assert result.output.source_ids == ["source-1"]
    assert result.output.findings[0].kind == "requirement"
    assert result.output.plan.tasks[0].role == "Concrete specification reviewer"
    assert result.usage["input_tokens"] == 100
    assert result.output.summary == "The concrete specification requires review."
    assert repo.replies == repo.findings == repo.plans == []


@pytest.mark.asyncio
@pytest.mark.parametrize("source_id", ["fabricated", "source-2"])
async def test_unread_or_fabricated_source_rejects_entire_output_before_writes(
    monkeypatch, source_id
):
    office = office_module()
    repo = MemoryRepository()

    async def provider(route, connection, credentials, context, prompt, output_type, **kwargs):
        return api_result_for(
            office.OfficeOutput(summary="Unsupported fact", source_ids=[source_id])
        )

    monkeypatch.setattr("quantix.ai_execution.execute_api", provider)
    with pytest.raises(ValueError, match="source|evidence"):
        await office.run_manager(repo, "tender-1", "run-1", "Review")
    assert repo.replies == []
    assert repo.findings == []
    assert repo.plans == []


@pytest.mark.asyncio
async def test_production_agent_has_bounded_turns_strict_scoped_tools_and_private_client(
    monkeypatch,
):
    office = office_module()
    repo = MemoryRepository()

    async def provider(route, connection, credentials, context, prompt, output_type, **kwargs):
        assert route["model_id"] == "gpt-6-astra"
        assert route["reasoning"] == "xhigh"
        assert 1 <= connection["_execution_limits"]["max_requests"] <= 16
        assert credentials == {"api_key": "synthetic-key"}
        assert "synthetic-key" not in prompt
        assert "C:\\" not in prompt
        for definition in source_tools():
            assert definition.parameters["additionalProperties"] is False
            assert "tender_id" not in definition.parameters["properties"]
        assert route["web_search"] is True
        return api_result_for(office.OfficeOutput(summary="No project evidence has been analysed."))

    monkeypatch.setattr("quantix.ai_execution.execute_api", provider)
    await office.run_manager(repo, "tender-1", "run-1", "Review")


@pytest.mark.asyncio
async def test_specialist_uses_saved_task_sources_and_returns_proposals(monkeypatch):
    office = office_module()
    repo = MemoryRepository()
    from quantix.ai_policy import AIPolicyService

    actual_routes = AIPolicyService.routes_for
    # This unit fixture supplies its saved scope directly. Full persisted
    # plan/team authority is exercised by the combined approval tests.
    monkeypatch.setattr(
        AIPolicyService, "routes_for", lambda self, tender_id, **_: actual_routes(self, tender_id)
    )

    async def provider(route, connection, credentials, context, prompt, output_type, **kwargs):
        assert "35 MPa" in prompt
        assert json.loads(prompt)["assigned_task"]["role"] == "Concrete reviewer"
        return api_result_for(
            office.OfficeOutput(
                summary="Concrete grade checked against the clause.", source_ids=["source-1"]
            )
        )

    monkeypatch.setattr("quantix.ai_execution.execute_api", provider)
    task = {
        "id": "task-1",
        "plan_id": "plan-1",
        "title": "Review concrete",
        "description": "Check specified strength",
        "role": "Concrete reviewer",
        "source_ids": ["source-1"],
    }
    result = await office.run_specialist(repo, "tender-1", "run-1", task)
    assert result.output.source_ids == ["source-1"]
    assert result.run_id == "run-1"
    assert repo.replies == []


@pytest.mark.asyncio
async def test_cancellation_propagates_without_a_success_reply(monkeypatch):
    office = office_module()
    repo = MemoryRepository()

    async def provider(*args, **kwargs):
        raise asyncio.CancelledError

    monkeypatch.setattr("quantix.ai_execution.execute_api", provider)
    with pytest.raises(asyncio.CancelledError):
        await office.run_manager(repo, "tender-1", "run-1", "Review")
    assert repo.replies == []


@pytest.mark.asyncio
async def test_run_cannot_be_used_for_another_tender(monkeypatch):
    office = office_module()
    repo = MemoryRepository()
    repo.get_run = lambda run_id: {"id": run_id, "tender_id": "other-tender"}
    with pytest.raises(ValueError, match="Tender|tender"):
        await office.run_manager(repo, "tender-1", "run-1", "Review")
    assert repo.replies == []


def web_source(url="https://supplier.example/products/rebar"):
    return {
        "url": url,
        "title": "Supplier price list",
        "retrieved_at": "2026-09-09T00:00:00Z",
        "cited": True,
    }


def price_proposal(url="https://supplier.example/products/rebar", basis="observed"):
    return {
        "item": "Rebar",
        "amount": "38000.00",
        "currency": "EGP",
        "unit": "tonne",
        "location": "Cairo",
        "tax_basis": "Not stated",
        "basis": basis,
        "observed_on": "2026-09-06",
        "valid_until": None,
        "conditions": "Published listing; delivery and availability require confirmation.",
        "urls": [url],
    }


@pytest.mark.asyncio
async def test_research_retains_provider_citations_and_unapproved_price_basis(monkeypatch):
    office = office_module()
    repo = MemoryRepository()
    url = "https://supplier.example/products/rebar"

    async def provider(route, connection, credentials, context, prompt, output_type, **kwargs):
        return api_result_for(
            office.OfficeOutput.model_validate(
                {
                    "summary": "A published rebar price is available; commercial terms need confirmation.",
                    "web_findings": [
                        {
                            "title": "Rebar market reference",
                            "detail": "Published supplier listing.",
                            "urls": [url],
                        }
                    ],
                    "price_proposals": [price_proposal()],
                }
            ),
            [web_source()],
        )

    monkeypatch.setattr("quantix.ai_execution.execute_api", provider)
    result = await office.run_manager(repo, "tender-1", "run-1", "Research current prices")
    assert result.web_sources[0]["url"] == url
    assert result.web_sources[0]["retrieved_at"]
    assert result.output.price_proposals[0].basis == "observed"
    assert result.output.price_proposals[0].tax_basis == "Not stated"
    assert repo.replies == []
    assert not any(event["kind"] == "research_proposed" for event in repo.events)


@pytest.mark.asyncio
async def test_fabricated_web_price_source_rejects_output_before_writes(monkeypatch):
    office = office_module()
    repo = MemoryRepository()

    async def provider(route, connection, credentials, context, prompt, output_type, **kwargs):
        return api_result_for(
            office.OfficeOutput.model_validate(
                {
                    "summary": "A price was found.",
                    "price_proposals": [price_proposal("https://invented.example/price")],
                }
            ),
            [web_source()],
        )

    monkeypatch.setattr("quantix.ai_execution.execute_api", provider)
    with pytest.raises(ValueError, match="URL|source|search"):
        await office.run_manager(repo, "tender-1", "run-1", "Research")
    assert repo.replies == []
    assert repo.findings == []


@pytest.mark.asyncio
async def test_model_output_cannot_include_approval_fields(monkeypatch):
    office = office_module()
    repo = MemoryRepository()

    async def provider(route, connection, credentials, context, prompt, output_type, **kwargs):
        return api_result_for({"summary": "Approved", "approved": True})

    monkeypatch.setattr("quantix.ai_execution.execute_api", provider)
    with pytest.raises(ValueError):
        await office.run_manager(repo, "tender-1", "run-1", "Review")
    assert repo.replies == []


@pytest.mark.asyncio
async def test_manager_can_consult_scoped_specialist_without_recursive_delegation(tmp_path, monkeypatch):
    """Keep the historical test target while exercising queued staff work."""
    from test_catalog_authority import configured_office
    from test_dynamic_staff import _order, _profile
    from test_repository import source

    from quantix import ai_execution as ai_execution_module
    from quantix import staff_runtime as staff_runtime_module
    from quantix.jobs import JobManager
    from quantix.plan_review import PlanReviewService
    from quantix.staff_assignments import StaffAssignmentService
    from quantix.staff_runtime_models import StaffCompletedDraft, StaffProviderOutput
    from quantix.staff_store import StaffStore

    office = office_module()
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

    async def provider(route, connection, context, prompt, output_type, **kwargs):
        # This adapter-shaped test provider is intentionally shared by Manager
        # and staff so the controller's persisted handoff remains observable.
        reservation = await kwargs["before_request"](200, 200)
        usage = {
            "requests": 1,
            "input_tokens": 200,
            "output_tokens": 100,
            "web_search_calls": 0,
            "usage_complete": True,
        }
        await kwargs["on_response"](usage, reservation)
        definitions = {item.name: item for item in kwargs["definitions"]}
        payload = json.loads(prompt.rsplit("\n\n", 1)[-1])
        assert kwargs.get("consult") is None
        if context.is_staff:
            phases.append("staff")
            assert set(definitions) == {"read_source"}
            assert "execute_staff" not in definitions
            assert context.seen_sources == set()
            packet = payload
            assert packet["engineer_approved_scope"]["rationale"] == condition
            assert packet["engineer_request"] == repo.get_run(root_id)["instruction"]
            await definitions["read_source"].invoke(context, {"source_id": evidence["id"]})
            child_output = office.OfficeOutput(
                summary="The colleague inspected the concrete clause.",
                source_ids=[evidence["id"]],
                findings=[{
                    "title": "Child proposed source observation",
                    "detail": "The supplied source states 30 MPa.",
                    "kind": "observation",
                    "source_ids": [evidence["id"]],
                }],
            )
            return {
                "output": StaffProviderOutput(
                    result=StaffCompletedDraft(output=child_output)
                ),
                "usage": usage,
                "web_sources": [],
            }

        phases.append("manager")
        assert route["reasoning"] == "xhigh"
        assert connection["_execution_limits"]["max_requests"] <= 12
        assert payload["engineer_approved_scope"]["rationale"] == condition
        assert "consult_specialist" not in definitions
        if phases.count("manager") == 1:
            await definitions["read_source"].invoke(context, {"source_id": evidence["id"]})
            profile = _profile(role="Concrete specification reviewer").model_dump(mode="json")
            profile["requested_tool_ids"] = ["read_source"]
            order = _order(source_ids=[evidence["id"]]).model_dump(mode="json")
            created = json.loads(
                await definitions["create_staff"].invoke(
                    context, {"profile": profile, "work_order": order}, invocation_id="create-staff"
                )
            )
            staff = StaffStore(repo).get_staff(tender_id, created["staff"]["id"])
            work_order = StaffStore(repo).get_work_order(tender_id, created["work_order"]["id"])
            queued = json.loads(
                await definitions["execute_staff"].invoke(
                    context,
                    {
                        "staff_id": staff.id,
                        "work_order_id": work_order.id,
                        "route_option_id": payload["reviewed_delegation"]["route_options"][0]["id"],
                    },
                    invocation_id="queue-staff",
                )
            )
            assert queued["assignment"]["status"] == "queued"
            return {"output": office.OfficeOutput(summary="The scoped colleague is queued."), "usage": usage, "web_sources": []}

        assignments = StaffAssignmentService(repo)
        assignment = assignments.list(tender_id)[0]
        assert assignment.result_id
        manager_sources_before_draft = set(context.seen_sources)
        await definitions["read_staff_result"].invoke(context, {"result_id": assignment.result_id})
        assert context.seen_sources == manager_sources_before_draft
        await definitions["read_source"].invoke(context, {"source_id": evidence["id"]})
        return {
            "output": office.OfficeOutput(
                summary="The Manager consolidated the colleague's review.",
                source_ids=[evidence["id"]],
                findings=[{
                    "title": "Manager consolidation",
                    "detail": "The source-backed colleague result is ready for engineer review.",
                    "kind": "observation",
                    "source_ids": [evidence["id"]],
                }],
            ),
            "usage": usage,
            "web_sources": [],
        }

    async def adapter(route, connection, credentials, context, prompt, output_type, **kwargs):
        return await provider(route, connection, context, prompt, output_type, **kwargs)

    try:
        with monkeypatch.context() as local:
            local.setattr("quantix.ai_execution.execute_api", adapter)
            local.setattr("quantix.staff_runtime.execute_api", adapter)
            await asyncio.gather(*list(jobs.tasks.values()))
    finally:
        # ``staff_runtime`` imports the executor by value.  Restore that
        # module-level alias before the next test module starts.
        staff_runtime_module.execute_api = ai_execution_module.execute_api

    root = repo.get_run(root_id)
    assert phases == ["manager", "staff", "manager"], root.get("error")
    assert root["status"] == "completed", root.get("error")
    with repo.db.connect() as conn:
        assert policy._totals(conn, tender_id, root_id)[2] == 3
    saved_assignment = StaffAssignmentService(repo).list(tender_id)[0]
    saved_result = StaffAssignmentService(repo).get_result(tender_id, saved_assignment.result_id)
    assert saved_assignment.status == "completed"
    assert saved_result.office_output.findings[0].title == "Child proposed source observation"
    assert [finding["title"] for finding in repo.list_findings(tender_id)] == ["Manager consolidation"]
    assert len(StaffStore(repo).list_staff(tender_id)) == 1
    await jobs.close()


@pytest.mark.asyncio
async def test_source_read_keeps_spreadsheet_formula_context_and_marks_truncation(monkeypatch):
    import json

    office = office_module()
    repo = MemoryRepository()
    repo.evidence["source-1"]["text"] = "x" * 15000
    repo.evidence["source-1"]["metadata"] = {
        "cells": [
            {
                "coordinate": "F12",
                "value": "=D12*E12",
                "cached_value": None,
                "formula": "=D12*E12",
                "data_type": "f",
                "number_format": "0.00",
            }
        ],
        "private_path": r"C:\private\pricing.xlsx",
    }

    async def provider(route, connection, credentials, context, prompt, output_type, **kwargs):
        source = json.loads(
            await call_api_tool(kwargs, "read_source", context, {"source_id": "source-1"})
        )
        assert source["text_is_partial"] is True
        assert source["metadata"]["cells"][0]["formula"] == "=D12*E12"
        assert "private_path" not in source["metadata"]
        return api_result_for(office.OfficeOutput(summary="This source needs further inspection."))

    monkeypatch.setattr("quantix.ai_execution.execute_api", provider)
    await office.run_manager(repo, "tender-1", "run-1", "Inspect")


@pytest.mark.asyncio
async def test_existing_engineer_decisions_and_plan_survive_manager_context(monkeypatch):
    import json

    office = office_module()
    repo = MemoryRepository()
    original = repo.overview

    def overview(tender_id):
        return original(tender_id) | {
            "findings": [
                {
                    "title": "Finish selection",
                    "detail": "Use supplier A pending quote.",
                    "state": "accepted",
                    "is_stale": False,
                    "source_ids": ["source-1"],
                }
            ],
            "plan": {
                "title": "Approved finish review",
                "status": "approved",
                "tasks": [
                    {
                        "title": "Check delivery",
                        "description": "Research availability",
                        "role": "Buyer",
                        "status": "ready",
                        "source_ids": [],
                    }
                ],
            },
        }

    repo.overview = overview

    async def provider(route, connection, credentials, context, prompt, output_type, **kwargs):
        context = json.loads(prompt)
        assert context["existing_findings"][0]["state"] == "accepted"
        assert context["plan"]["status"] == "approved"
        assert context["plan"]["tasks"][0]["title"] == "Check delivery"
        return api_result_for(office.OfficeOutput(summary="The approved work remains visible."))

    monkeypatch.setattr("quantix.ai_execution.execute_api", provider)
    await office.run_manager(repo, "tender-1", "run-1", "Where are we?")


@pytest.mark.asyncio
async def test_summary_cannot_bypass_web_source_validation(monkeypatch):
    office = office_module()
    repo = MemoryRepository()

    async def provider(route, connection, credentials, context, prompt, output_type, **kwargs):
        return api_result_for(
            office.OfficeOutput(summary="Price source: https://invented.example/price")
        )

    monkeypatch.setattr("quantix.ai_execution.execute_api", provider)
    with pytest.raises(ValueError, match="URL|source|search"):
        await office.run_manager(repo, "tender-1", "run-1", "Research")
    assert repo.replies == []


def test_empty_proposals_are_rejected_before_repository_writes():
    office = office_module()
    with pytest.raises(ValueError):
        office.OfficeOutput(summary="   ")


def result_for(output, raw_responses=None):
    return SimpleNamespace(
        final_output=output,
        interruptions=[],
        raw_responses=raw_responses or [],
        context_wrapper=SimpleNamespace(
            usage=SimpleNamespace(
                requests=1,
                input_tokens=100,
                output_tokens=50,
                total_tokens=150,
                input_tokens_details=SimpleNamespace(cached_tokens=10),
                output_tokens_details=SimpleNamespace(reasoning_tokens=25),
            )
        ),
    )


async def call_tool(agent, name, context, arguments):
    import json

    from agents.tool_context import ToolContext

    tool = next(tool for tool in agent.tools if tool.name == name)
    encoded = json.dumps(arguments)
    return await tool.on_invoke_tool(
        ToolContext(context, tool_name=name, tool_call_id="call-1", tool_arguments=encoded), encoded
    )


def web_response(url="https://supplier.example/products/rebar"):
    from agents.items import ModelResponse
    from agents.usage import Usage
    from openai.types.responses import ResponseOutputMessage

    message = ResponseOutputMessage(
        id="msg-web",
        role="assistant",
        status="completed",
        type="message",
        content=[
            {
                "type": "output_text",
                "text": "Published rebar price.",
                "annotations": [
                    {
                        "type": "url_citation",
                        "url": url,
                        "title": "Supplier price list",
                        "start_index": 0,
                        "end_index": 21,
                    },
                ],
            }
        ],
    )
    return ModelResponse(
        output=[message],
        usage=Usage(requests=1, input_tokens=100, output_tokens=50, total_tokens=150),
        response_id="resp-web",
    )
