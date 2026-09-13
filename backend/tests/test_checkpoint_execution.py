"""Real staff/job lifecycle checkpoints with synthetic provider responses."""

import asyncio
import json
from dataclasses import replace

import pytest
from test_catalog_authority import configured_office
from test_staff_routing import _profile, _source, _work_order
from test_staff_runtime import _workspace

from quantix.jobs import JobManager
from quantix.office_checkpoints import OfficeCheckpointService
from quantix.office_types import OfficeOutput
from quantix.staff_assignments import StaffAssignmentService
from quantix.staff_runtime import run_staff_assignment
from quantix.staff_runtime_models import StaffCompletedDraft, StaffProviderOutput


async def _completed(tmp_path, monkeypatch, during_provider=None, *, native_runtime=False):
    from quantix.manager_profile import ManagerProfileService
    from quantix.plan_review import PlanReviewService
    from quantix.staff_models import ManagerCreationContext
    from quantix.staff_routing import StaffRoutingService
    from quantix.staff_store import StaffStore
    repo, tender, _, connection, model, _, route = configured_office(tmp_path, monkeypatch)
    if native_runtime:
        from quantix.ai_connections import AIConnectionService
        from quantix.ai_policy import AIPolicyService
        from quantix.ai_setup_store import SetupStore
        connections = AIConnectionService(repo)
        connection = connections.create({"name": "Synthetic subscription", "provider_id": "codex",
            "protocol": "codex", "auth_type": "client_login", "billing": "subscription"})
        model = connections.save_model(connection["id"], {"model_id": model["model_id"],
            "display_name": "Synthetic native model", "capabilities": model["capabilities"]}, source="provider")
        monkeypatch.setattr("quantix.ai_components.AIComponentService.status", lambda *_: {
            "state": "ready", "version": "synthetic", "component_id": "codex"})
        evidence = {"check": {"status": "passed", "model_id": model["model_id"]},
                    "checked_revision": connection["revision"], "checked_component_version": "synthetic"}
        store = SetupStore(repo)
        store.update(connection["id"], selected_model_id=model["model_id"], **evidence)
        store.save_model_check(connection["id"], model["model_id"], evidence)
        route = {**route, "connection_id": connection["id"]}
        AIPolicyService(repo).update(tender["id"], {"allowed_connection_ids": [connection["id"]],
            "manager": route, "specialist": route, "max_requests": 12,
            "run_budget_usd": 10, "tender_budget_usd": 100, "engineer_confirmed": True,
            "rationale": "Synthetic original-client checkpoint approval"})
    artifact, source = _source(repo, tender["id"])
    source_id = source["id"]
    plan = repo.create_plan(tender["id"], "Checkpoint work", [{"title": "Inspect source", "role": "Reviewer",
        "description": "Inspect synthetic source", "source_ids": [source_id]}])
    manager = ManagerProfileService(repo).get()
    planning = repo.create_run(tender["id"], "conversation", "Create colleague")
    staff = StaffStore(repo).create_generated(
        ManagerCreationContext(tender["id"], planning["id"], manager.version, plan["id"]),
        _profile(requested_tool_ids=["read_source"]), _work_order(source_ids=[source_id]), "checkpoint-staff")
    repo.update_run(planning["id"], status="completed")
    jobs = JobManager(repo, object())
    review_service = PlanReviewService(repo, save_runs_in_transaction=jobs.queue_approved_plan_runs)
    review = review_service.review(tender["id"], plan["id"])
    approved = review_service.approve_and_start(tender["id"], plan["id"], {
        "fingerprint": review["fingerprint"], "engineer_confirmed": True})
    root_id = approved["work_intents"][0]["run_id"]
    routing = StaffRoutingService(repo)
    grant = routing.validate_root(tender["id"], root_id, plan["id"])
    owner = ManagerCreationContext(tender["id"], root_id, manager.version, plan["id"])
    binding = routing.bind(owner, staff.staff.id, staff.work_order.id, grant.envelope.route_options[0].id, "checkpoint-binding")
    assignment = StaffAssignmentService(repo).queue(owner, binding.id, "checkpoint-assignment")
    values = repo, tender, connection, model, route, staff, binding, assignment, source_id
    repo.update_run(assignment.root_run_id, status="running")

    async def provider(route, connection, credentials, context, *args, before_request, on_response, **kwargs):
        assert repo.db.held_connection() is None
        context.source(source_id)
        reservation = await before_request(100, 128)
        usage = {"requests": 1, "input_tokens": 100, "output_tokens": 20, "usage_complete": True}
        await on_response(usage, reservation)
        if during_provider:
            during_provider(repo)
        return {"output": StaffProviderOutput(result=StaffCompletedDraft(
            output=OfficeOutput(summary="The synthetic source was inspected.", source_ids=[source_id]))),
            "usage": usage, "web_sources": []}

    monkeypatch.setattr("quantix.staff_runtime.execute_api", provider)
    outcome = await run_staff_assignment(repo, tender["id"], assignment.id)
    return values, outcome


def _resume_without_schedule(repo, tender_id, root_id, monkeypatch):
    repo.update_run(root_id, status="failed")
    jobs = JobManager(repo, object())
    monkeypatch.setattr(jobs, "_schedule", lambda run, task_id=None: run)
    return jobs.resume(root_id)


@pytest.mark.asyncio
async def test_completed_step_checkpoints_atomically_and_actual_job_resume_avoids_provider_repeat(tmp_path, monkeypatch):
    values, completed = await _completed(tmp_path, monkeypatch)
    repo, tender, _, _, _, staff, binding, assignment, _ = values
    with repo.db.connect() as conn:
        checkpoint = conn.execute("SELECT * FROM office_step_checkpoints WHERE assignment_id=?", (assignment.id,)).fetchone()
        assert json.loads(checkpoint["outputs_json"])["result_id"] == completed.result_id
    # Reobserving completion does not duplicate the draft, checkpoint or spend.
    await run_staff_assignment(repo, tender["id"], assignment.id)
    repo.update_run(assignment.root_run_id, status="failed")
    calls = []

    async def no_repeat(*args, **kwargs):
        pytest.fail("A completed prior staff step called its provider again")

    async def manager(route, connection, credentials, context, instruction, *args,
                      definitions, before_request, on_response, **kwargs):
        calls.append(context.run_id)
        assert completed.result_id in instruction
        execute = next(item for item in definitions if item.name == "execute_staff")
        returned = json.loads(await execute.invoke(context, {
            "staff_id": staff.staff.id, "work_order_id": staff.work_order.id,
            "route_option_id": binding.route_option_id}, invocation_id="repeat-step"))
        assert returned["reused_prior_work"] is True and returned["result_id"] == completed.result_id
        replayed = json.loads(await execute.invoke(context, {
            "staff_id": staff.staff.id, "work_order_id": staff.work_order.id,
            "route_option_id": binding.route_option_id}, invocation_id="repeat-step"))
        assert replayed == returned
        from quantix.staff_models import OfficeConflict
        with pytest.raises(OfficeConflict, match="different completed work"):
            await execute.invoke(context, {"staff_id": "another-staff", "work_order_id": staff.work_order.id,
                "route_option_id": binding.route_option_id}, invocation_id="repeat-step")
        assert not context.seen_sources
        reservation = await before_request(100, 128)
        usage = {"requests": 1, "input_tokens": 100, "output_tokens": 20, "usage_complete": True}
        await on_response(usage, reservation)
        return {"output": OfficeOutput(summary="Prior saved draft is available for engineer review."),
                "usage": usage, "web_sources": []}

    monkeypatch.setattr("quantix.staff_runtime.execute_api", no_repeat)
    monkeypatch.setattr("quantix.ai_execution.execute_api", manager)
    jobs = JobManager(repo, object())
    resumed = jobs.resume(assignment.root_run_id)
    await asyncio.gather(*list(jobs.tasks.values()))
    assert repo.get_run(resumed["id"])["status"] == "completed", repo.get_run(resumed["id"])
    assert calls == [resumed["id"]]
    with repo.db.connect() as conn:
        assert conn.execute("SELECT count(*) FROM office_step_checkpoints").fetchone()[0] == 1
        assert conn.execute("SELECT count(*) FROM office_staff_results").fetchone()[0] == 1
        assert conn.execute("SELECT count(*) FROM office_checkpoint_reuses WHERE to_root_run_id=?", (resumed["id"],)).fetchone()[0] == 1
        assert conn.execute("SELECT count(*) FROM office_assignments WHERE root_run_id=?", (resumed["id"],)).fetchone()[0] == 0
        assert conn.execute("SELECT count(*) FROM ai_usage").fetchone()[0] == 2
    await jobs.close()


@pytest.mark.asyncio
async def test_interrupted_step_saves_no_completed_checkpoint(tmp_path, monkeypatch):
    repo, tender, _, _, _, _, _, assignment, _ = _workspace(tmp_path, monkeypatch)
    async def interrupted(*args, **kwargs):
        raise asyncio.CancelledError
    monkeypatch.setattr("quantix.staff_runtime.execute_api", interrupted)
    with pytest.raises(asyncio.CancelledError):
        await run_staff_assignment(repo, tender["id"], assignment.id)
    OfficeCheckpointService(repo)
    with repo.db.connect() as conn:
        assert conn.execute("SELECT count(*) FROM office_step_checkpoints").fetchone()[0] == 0
        assert conn.execute("SELECT count(*) FROM office_staff_results").fetchone()[0] == 0


@pytest.mark.asyncio
@pytest.mark.parametrize("change", ["tool_schema", "implementation", "runtime", "client_runtime", "preferences", "profile", "source", "instruction", "stopped"])
async def test_resume_compares_current_server_basis_before_marking_reuse(tmp_path, monkeypatch, change):
    values, _ = await _completed(tmp_path, monkeypatch, native_runtime=change == "client_runtime")
    repo, tender, _, _, _, staff, binding, assignment, source_id = values
    resumed = _resume_without_schedule(repo, tender["id"], assignment.root_run_id, monkeypatch)
    if change == "tool_schema":
        from quantix.office_tools import source_tools
        original = source_tools()
        monkeypatch.setattr("quantix.office_tools.source_tools", lambda: [
            replace(item, parameters={**item.parameters, "description": "Changed input contract"})
            if item.name == "read_source" else item for item in original])
    elif change == "implementation":
        monkeypatch.setattr("quantix.office_checkpoints.tool_implementation_fingerprint", lambda: "f" * 64)
    elif change in {"runtime", "client_runtime"}:
        from quantix.ai_setup_store import SetupStore
        # The account/model/schema stay unchanged; only installed software and
        # its successful check move to a newer version.
        if change == "client_runtime":
            monkeypatch.setattr("quantix.ai_components.AIComponentService.status", lambda *_: {
                "state": "ready", "version": "synthetic-upgraded", "component_id": "codex"})
        else:
            monkeypatch.setattr("quantix.ai_direct.direct_runtime_status", lambda _: {
                "state": "ready", "version": "synthetic-upgraded", "component_id": "direct-api"})
        SetupStore(repo).save_model_check(binding.route.connection_id, binding.route.model_id, {
            "check": {"status": "passed", "model_id": binding.route.model_id},
            "checked_revision": binding.connection_revision, "checked_component_version": "synthetic-upgraded"})
    elif change == "preferences":
        repo.set_setting("preferences", "A newly required calculation convention.")
    elif change == "profile":
        from quantix.manager_runtime import ManagerRunProfiles
        from quantix.staff_models import ManagerCreationContext, StaffProfileDraft
        from quantix.staff_store import StaffStore
        profile = {field: getattr(staff.staff, field) for field in StaffProfileDraft.model_fields}
        profile["persona"] = "Revised professional instruction for this colleague."
        manager = ManagerRunProfiles(repo).get(tender["id"], resumed["id"])
        StaffStore(repo).revise_generated(
            ManagerCreationContext(tender["id"], resumed["id"], manager.version, binding.plan_id),
            staff.staff.id, staff.staff.version, profile, "revised-profile")
    elif change == "source":
        with repo.atomic() as conn:
            conn.execute("UPDATE artifacts SET content_hash=? WHERE id=?", ("e" * 64, binding.artifacts[0].artifact_id))
    elif change == "instruction":
        with repo.atomic() as conn:
            conn.execute("UPDATE runs SET instruction=? WHERE id=?", ("Changed instruction", resumed["id"]))
    else:
        repo.update_run(resumed["id"], status="cancelled")
    service = OfficeCheckpointService(repo)
    if change in {"source", "instruction", "stopped"}:
        with pytest.raises(ValueError):
            service.verified_staff_for_root(tender["id"], resumed["id"], binding.plan_id)
    else:
        assert service.verified_staff_for_root(tender["id"], resumed["id"], binding.plan_id) == []
    with repo.db.connect() as conn:
        assert conn.execute("SELECT count(*) FROM office_checkpoint_reuses").fetchone()[0] == 0


@pytest.mark.asyncio
async def test_checkpoint_failure_rolls_back_completed_staff_draft(tmp_path, monkeypatch):
    captured = {}
    def fail(service, *args, **kwargs):
        captured["repo"] = service.repo
        raise ValueError("Synthetic checkpoint persistence failure")
    monkeypatch.setattr(OfficeCheckpointService, "checkpoint_staff_result", fail)
    with pytest.raises(ValueError, match="checkpoint persistence"):
        await _completed(tmp_path, monkeypatch)
    with captured["repo"].db.connect() as conn:
        assert conn.execute("SELECT count(*) FROM office_staff_results").fetchone()[0] == 0
        assert conn.execute("SELECT count(*) FROM office_assignments WHERE status='completed'").fetchone()[0] == 0


@pytest.mark.asyncio
async def test_changed_inputs_during_provider_keep_draft_but_do_not_claim_checkpoint(tmp_path, monkeypatch):
    values, result = await _completed(tmp_path, monkeypatch,
        during_provider=lambda repo: repo.set_setting("preferences", "A new engineering convention"))
    repo, tender, *_ = values
    assert StaffAssignmentService(repo).get_result(tender["id"], result.result_id).currentness == "current"
    with repo.db.connect() as conn:
        assert conn.execute("SELECT count(*) FROM office_step_checkpoints").fetchone()[0] == 0


@pytest.mark.asyncio
async def test_input_capture_and_packet_share_one_snapshot_before_provider(tmp_path, monkeypatch):
    import quantix.staff_runtime as runtime
    order = []
    capture = OfficeCheckpointService.capture_staff_inputs
    packet = runtime._provider_packet
    def capture_checked(service, *args):
        connection = service.repo.db.held_connection()
        assert connection is not None
        order.append(("capture", connection))
        return capture(service, *args)
    def packet_checked(context):
        assert order[-1][0] == "capture"
        assert context.repo.db.held_connection() is order[-1][1]
        order.append(("packet", context.repo.db.held_connection()))
        return packet(context)
    monkeypatch.setattr(OfficeCheckpointService, "capture_staff_inputs", capture_checked)
    monkeypatch.setattr(runtime, "_provider_packet", packet_checked)
    await _completed(tmp_path, monkeypatch)
    assert [item[0] for item in order] == ["capture", "packet"]


def test_bundled_helper_changes_invalidate_implementation_even_without_schema_changes(tmp_path, monkeypatch):
    import quantix.office_checkpoints as checkpoints
    (tmp_path / "office_tools.py").write_text("TOOL_VERSION = 1\n")
    helper = tmp_path / "calculation_helper.py"
    helper.write_text("PRECISION = 28\n")
    monkeypatch.setattr(checkpoints, "__file__", str(tmp_path / "office_checkpoints.py"))
    first = checkpoints.tool_implementation_fingerprint()
    helper.write_text("PRECISION = 1024\n")
    assert checkpoints.tool_implementation_fingerprint() != first
    (tmp_path / "office_tools.py").unlink()
    with pytest.raises(checkpoints.CheckpointImplementationUnavailable, match="source proof"):
        checkpoints.tool_implementation_fingerprint()
