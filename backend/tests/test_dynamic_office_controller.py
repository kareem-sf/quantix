"""Synthetic provider journey through real approval, staff work and publication."""

import asyncio
import json
from contextlib import contextmanager

import pytest
from test_catalog_authority import configured_office
from test_dynamic_staff import _order, _profile
from test_repository import source

from quantix.ai_connections import AIConnectionService
from quantix.jobs import JobManager
from quantix.office_types import OfficeOutput
from quantix.plan_review import PlanReviewService
from quantix.staff_assignments import StaffAssignmentService
from quantix.staff_runtime_models import StaffCompletedDraft, StaffProviderOutput
from quantix.staff_store import StaffStore


@pytest.mark.asyncio
@pytest.mark.parametrize("manager_inspects", [True, False])
async def test_manager_staff_manager_share_root_and_release_each_provider_lease(tmp_path, monkeypatch, manager_inspects):
    repo, tender, *_ = configured_office(tmp_path, monkeypatch)
    _, evidence, _ = source(repo, tender["id"])
    plan = repo.create_plan(tender["id"], "Synthetic source review", [{
        "title": "Review concrete", "role": "Unregistered review specialty",
        "description": "Review the actual source", "source_ids": [evidence["id"]],
    }])
    jobs = JobManager(repo, object())
    leases = 0
    actual_lease = AIConnectionService.lease

    @contextmanager
    def observed_lease(service, connection_id):
        nonlocal leases
        assert leases == 0, "A child must wait until its Manager account lease closes"
        with actual_lease(service, connection_id) as account:
            leases += 1
            try:
                yield account
            finally:
                leases -= 1

    monkeypatch.setattr(AIConnectionService, "lease", observed_lease)
    phases = []
    roots = []

    async def provider(route, connection, credentials, context, prompt, output_type, **options):
        assert leases == 1
        phases.append("staff" if context.is_staff else "manager")
        roots.append(context.run_id)
        definitions = {item.name: item for item in options["definitions"]}
        reservation = await options["before_request"](200, 200)
        usage = {"requests": 1, "input_tokens": 200, "output_tokens": 100,
                 "web_search_calls": 0, "usage_complete": True}
        if context.is_staff:
            assert "execute_staff" not in definitions
            assert context.seen_sources == set()
            await definitions["read_source"].invoke(context, {"source_id": evidence["id"]})
            output = StaffProviderOutput(result=StaffCompletedDraft(output=OfficeOutput(
                summary="A saved staff observation.", source_ids=[evidence["id"]],
            )))
        elif len(phases) == 1:
            assert options.get("consult") is None
            payload = json.loads(prompt)
            profile = _profile(role="واجهة خرسانية غير مسجلة").model_dump(mode="json")
            profile["requested_tool_ids"] = ["read_source"]
            order = _order().model_dump(mode="json")
            order["source_ids"] = [evidence["id"]]
            generated = json.loads(await definitions["create_staff"].invoke(
                context, {"profile": profile, "work_order": order}, invocation_id="create-live-staff"))
            await definitions["execute_staff"].invoke(context, {
                "staff_id": generated["staff"]["id"], "work_order_id": generated["work_order"]["id"],
                "route_option_id": payload["reviewed_delegation"]["route_options"][0]["id"],
            }, invocation_id="assign-live-staff")
            assert len(phases) == 1
            output = OfficeOutput(summary="The staff assignment is queued.")
        else:
            result_id = json.loads(prompt)["actual_staff_outcomes_not_source_inspection"][0]["result_id"]
            await definitions["read_staff_result"].invoke(context, {"result_id": result_id})
            assert context.seen_sources == set(), "Reading a draft must not import another actor's source reads"
            if manager_inspects:
                await definitions["read_source"].invoke(context, {"source_id": evidence["id"]})
            output = OfficeOutput(summary="The source-backed review is saved.", source_ids=[evidence["id"]], findings=[{
                "title": "Concrete source reviewed", "detail": "The synthetic source was inspected.",
                "kind": "observation", "source_ids": [evidence["id"]],
            }])
        await options["on_response"](usage, reservation)
        return {"output": output, "usage": usage, "web_sources": []}

    monkeypatch.setattr("quantix.ai_execution.execute_api", provider)
    monkeypatch.setattr("quantix.staff_runtime.execute_api", provider)
    review_service = PlanReviewService(repo, save_runs_in_transaction=jobs.queue_approved_plan_runs,
                                       schedule_after_commit=jobs.schedule_approved_plan_runs)
    review = review_service.review(tender["id"], plan["id"])
    result = review_service.approve_and_start(tender["id"], plan["id"], {
        "fingerprint": review["fingerprint"], "engineer_confirmed": True,
    })
    await asyncio.gather(*list(jobs.tasks.values()))
    root = repo.get_run(result["work_intents"][0]["run_id"])
    assert phases == ["manager", "staff", "manager"], root.get("error")
    assert len(set(roots)) == len(repo.list_runs(tender["id"])) == 1
    assert root["status"] == ("completed" if manager_inspects else "failed"), root.get("error")
    assert len(repo.list_findings(tender["id"])) == (1 if manager_inspects else 0)
    assert len(StaffStore(repo).list_staff(tender["id"])) == 1
    saved_assignment = StaffAssignmentService(repo).list(tender["id"])[0]
    assert saved_assignment.status == "completed"
    assert StaffAssignmentService(repo).get_result(tender["id"], saved_assignment.result_id).office_output.summary == "A saved staff observation."
    with repo.db.connect() as conn:
        usage = [dict(row) for row in conn.execute("SELECT run_id,data_json FROM ai_usage WHERE tender_id=?", (tender["id"],))]
    assert len(usage) == 3
    assert {row["run_id"] for row in usage} == {root["id"]}
    assert sum(json.loads(row["data_json"])["requests"] for row in usage) == 3
    await jobs.close()
