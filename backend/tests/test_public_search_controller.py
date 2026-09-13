"""Manager drains deferred public research only after provider leases close."""

from __future__ import annotations

import asyncio
import json
from contextlib import contextmanager

import pytest
from test_catalog_authority import configured_office
from test_dynamic_staff import _order, _profile

from quantix.ai_connections import AIConnectionService
from quantix.execution_context import engineer_identity
from quantix.jobs import JobManager
from quantix.office_types import OfficeOutput
from quantix.plan_review import PlanReviewService
from quantix.research_models import ResearchSearchOutput
from quantix.research_search import PublicSearchService
from quantix.research_tools import research_tools
from quantix.staff_lifecycle import StaffLifecycleService
from quantix.staff_lifecycle_models import StaffLifecycleRequest
from quantix.staff_runtime_models import StaffCompletedDraft, StaffProviderOutput
from quantix.staff_store import StaffStore


@pytest.mark.asyncio
@pytest.mark.parametrize("search_fails", [False, True])
async def test_manager_continues_with_completed_or_failed_search_receipt_after_lease(
    tmp_path, monkeypatch, search_fails
):
    repo, tender, *_ = configured_office(tmp_path, monkeypatch, search=True)
    plan = repo.create_plan(
        tender["id"],
        "Synthetic public research",
        [
            {
                "title": "Research pump capacity",
                "role": "Tender Manager",
                "description": "Find a current public source.",
                "source_ids": [],
            }
        ],
    )
    jobs = JobManager(repo, object())
    leases = 0
    actual_lease = AIConnectionService.lease

    @contextmanager
    def observed_lease(service, connection_id):
        nonlocal leases
        assert leases == 0, "Deferred research must wait for the active lease to close."
        with actual_lease(service, connection_id) as connection:
            leases += 1
            try:
                yield connection
            finally:
                leases -= 1

    monkeypatch.setattr(AIConnectionService, "lease", observed_lease)
    phases: list[str] = []
    request_ids: list[str] = []

    async def manager_provider(
        _route,
        _connection,
        _credentials,
        context,
        prompt,
        _output_type,
        **options,
    ):
        phases.append("manager")
        reservation = await options["before_request"](100, 128)
        usage = {
            "requests": 1,
            "input_tokens": 100,
            "output_tokens": 30,
            "web_search_calls": 0,
            "usage_complete": True,
        }
        if len(phases) == 1:
            tool = {item.name: item for item in research_tools()}["search_public_sources"]
            queued = json.loads(
                await tool.invoke(
                    context,
                    {
                        "request": {
                            "query": "current concrete pump output",
                            "limit": 3,
                        }
                    },
                    invocation_id="manager-public-search",
                )
            )
            request_ids.append(queued["id"])
            output = OfficeOutput(summary="Public research is queued.")
        else:
            payload = json.loads(prompt)
            receipts = payload["actual_public_search_receipts_not_citations"]
            assert len(receipts) == 1
            assert receipts[0]["id"] == request_ids[0]
            assert receipts[0]["status"] == ("failed" if search_fails else "completed")
            if not search_fails:
                assert receipts[0]["results"][0]["citable"] is False
            output = OfficeOutput(summary="The public research receipt was reviewed.")
        await options["on_response"](usage, reservation)
        return {"output": output, "usage": usage, "web_sources": []}

    async def research_provider(
        route,
        _connection,
        _credentials,
        _context,
        _instruction,
        _output_type,
        **options,
    ):
        phases.append("search")
        assert leases == 1
        if search_fails:
            raise ValueError("Synthetic public search failure.")
        reservation = await options["before_request"](100, 128)
        usage = {
            "requests": 1,
            "input_tokens": 100,
            "output_tokens": 30,
            "web_search_calls": 1,
            "usage_complete": True,
        }
        await options["on_response"](usage, reservation)
        return {
            "output": ResearchSearchOutput(
                results=[
                    {
                        "url": "https://provider.example/pump",
                        "title": "Pump source",
                        "summary": "Search summary requiring exact page fetch.",
                    }
                ]
            ),
            "usage": usage,
            "web_sources": [
                {
                    "url": "https://provider.example/pump",
                    "title": "Pump source",
                    "retrieved_at": "2026-09-13T00:00:00Z",
                    "cited": True,
                }
            ],
        }

    monkeypatch.setattr("quantix.ai_execution.execute_api", manager_provider)
    monkeypatch.setattr("quantix.research_search.execute_api", research_provider)
    review_service = PlanReviewService(
        repo,
        save_runs_in_transaction=jobs.queue_approved_plan_runs,
        schedule_after_commit=jobs.schedule_approved_plan_runs,
    )
    review = review_service.review(tender["id"], plan["id"])
    result = review_service.approve_and_start(
        tender["id"],
        plan["id"],
        {"fingerprint": review["fingerprint"], "engineer_confirmed": True},
    )
    await asyncio.gather(*list(jobs.tasks.values()))
    root = repo.get_run(result["work_intents"][0]["run_id"])
    assert root["status"] == "completed", root.get("error")
    assert phases == ["manager", "search", "manager"]
    receipt = PublicSearchService(repo).get(tender["id"], request_ids[0])
    assert receipt.status == ("failed" if search_fails else "completed")
    await jobs.close()


@pytest.mark.asyncio
@pytest.mark.parametrize("retire_origin", [False, True])
async def test_staff_search_request_is_executed_by_manager_after_staff_lease(
    tmp_path, monkeypatch, retire_origin
):
    repo, tender, *_ = configured_office(tmp_path, monkeypatch, search=True)
    plan = repo.create_plan(
        tender["id"],
        "Synthetic staff research",
        [
            {
                "title": "Research pump capacity",
                "role": "Plant specialist",
                "description": "Request a current public source.",
                "source_ids": [],
            }
        ],
    )
    jobs = JobManager(repo, object())
    leases = 0
    actual_lease = AIConnectionService.lease

    @contextmanager
    def observed_lease(service, connection_id):
        nonlocal leases
        assert leases == 0
        with actual_lease(service, connection_id) as connection:
            leases += 1
            try:
                yield connection
            finally:
                leases -= 1

    monkeypatch.setattr(AIConnectionService, "lease", observed_lease)
    phases: list[str] = []
    request_id = None

    async def provider(
        _route,
        _connection,
        _credentials,
        context,
        prompt,
        _output_type,
        **options,
    ):
        nonlocal request_id
        phase = "staff" if context.is_staff else "manager"
        phases.append(phase)
        definitions = {item.name: item for item in options["definitions"]}
        reservation = await options["before_request"](100, 128)
        usage = {
            "requests": 1,
            "input_tokens": 100,
            "output_tokens": 30,
            "web_search_calls": 0,
            "usage_complete": True,
        }
        if phases == ["manager"]:
            payload = json.loads(prompt)
            profile = _profile(role="Plant research specialist").model_dump(mode="json")
            profile["requested_tool_ids"] = ["search_public_sources"]
            generated = json.loads(
                await definitions["create_staff"].invoke(
                    context,
                    {"profile": profile, "work_order": _order().model_dump(mode="json")},
                    invocation_id="create-research-staff",
                )
            )
            await definitions["execute_staff"].invoke(
                context,
                {
                    "staff_id": generated["staff"]["id"],
                    "work_order_id": generated["work_order"]["id"],
                    "route_option_id": payload["reviewed_delegation"]["route_options"][0]["id"],
                },
                invocation_id="assign-research-staff",
            )
            output = OfficeOutput(summary="The research colleague is queued.")
        elif context.is_staff:
            queued = json.loads(
                await definitions["search_public_sources"].invoke(
                    context,
                    {
                        "request": {
                            "query": "current concrete pump output",
                            "limit": 3,
                        }
                    },
                    invocation_id="staff-public-search",
                )
            )
            request_id = queued["id"]
            output = StaffProviderOutput(
                result=StaffCompletedDraft(
                    output=OfficeOutput(summary="Public research was requested.")
                )
            )
        else:
            payload = json.loads(prompt)
            assert len(payload["actual_staff_outcomes_not_source_inspection"]) == 1
            receipt = payload["actual_public_search_receipts_not_citations"][0]
            assert receipt["id"] == request_id
            assert receipt["requested_by_assignment_id"] is not None
            assert receipt["status"] == ("failed" if retire_origin else "completed")
            output = OfficeOutput(summary="The staff research handoff was reviewed.")
        await options["on_response"](usage, reservation)
        return {"output": output, "usage": usage, "web_sources": []}

    async def research_provider(
        _route,
        _connection,
        _credentials,
        _context,
        _instruction,
        _output_type,
        **options,
    ):
        phases.append("search")
        reservation = await options["before_request"](100, 128)
        usage = {
            "requests": 1,
            "input_tokens": 100,
            "output_tokens": 20,
            "web_search_calls": 1,
            "usage_complete": True,
        }
        await options["on_response"](usage, reservation)
        return {
            "output": ResearchSearchOutput(results=[]),
            "usage": usage,
            "web_sources": [
                {
                    "url": "https://provider.example/pump",
                    "title": "Pump source",
                    "retrieved_at": "2026-09-13T00:00:00Z",
                    "cited": True,
                }
            ],
        }

    monkeypatch.setattr("quantix.ai_execution.execute_api", provider)
    monkeypatch.setattr("quantix.staff_runtime.execute_api", provider)
    monkeypatch.setattr("quantix.research_search.execute_api", research_provider)
    if retire_origin:
        execute_search = PublicSearchService.execute

        async def retire_then_execute(service, manager_context, queued_id):
            queued = service.get(tender["id"], queued_id)
            staff = StaffStore(repo).get_staff(tender["id"], queued.requested_by_actor_id)
            StaffLifecycleService(repo).transition(
                engineer_identity(tender["id"]),
                StaffLifecycleRequest(
                    staff_id=staff.id,
                    expected_version=staff.version,
                    target="retired",
                    reason="Synthetic retirement before deferred research.",
                    idempotency_key="retire-search-origin",
                ),
            )
            return await execute_search(service, manager_context, queued_id)

        monkeypatch.setattr(PublicSearchService, "execute", retire_then_execute)
    review_service = PlanReviewService(
        repo,
        save_runs_in_transaction=jobs.queue_approved_plan_runs,
        schedule_after_commit=jobs.schedule_approved_plan_runs,
    )
    review = review_service.review(tender["id"], plan["id"])
    result = review_service.approve_and_start(
        tender["id"],
        plan["id"],
        {"fingerprint": review["fingerprint"], "engineer_confirmed": True},
    )
    await asyncio.gather(*list(jobs.tasks.values()))
    root = repo.get_run(result["work_intents"][0]["run_id"])
    assert root["status"] == "completed", root.get("error")
    assert phases == (
        ["manager", "staff", "manager"]
        if retire_origin
        else ["manager", "staff", "search", "manager"]
    )
    receipt = PublicSearchService(repo).get(tender["id"], request_id)
    assert receipt.actor_id != receipt.requested_by_actor_id
    assert receipt.requested_by_assignment_id is not None
    assert receipt.status == ("failed" if retire_origin else "completed")
    await jobs.close()
