"""Bounded recovery acceptance for the dynamic Tender Office.

All workspaces in this module are disposable synthetic repositories.  The
tests exercise the real repository, office stores, backup archive and app
lifespan; provider execution is either absent or an in-process cancellation
fixture.
"""

from __future__ import annotations

import asyncio
import hashlib
import json

import pytest
from fastapi.testclient import TestClient
from test_catalog_authority import configured_office
from test_dynamic_staff import _order, _profile
from test_repository import source
from test_staff_routing import _ready_binding_workspace

from quantix import diagnostics
from quantix.api import create_app
from quantix.backup import BackupService, apply_pending_restore
from quantix.db import new_id
from quantix.factory_reset import write_journal
from quantix.jobs import JobManager
from quantix.manager_profile import ManagerProfileService
from quantix.manager_runtime import ManagerRunProfiles
from quantix.office import prepare_result
from quantix.office_events import OfficeEventService
from quantix.office_message_models import OfficeRecipientTarget
from quantix.office_messages import OfficeMessageService
from quantix.office_research import ResearchRecord
from quantix.office_tools import OfficeContext
from quantix.office_types import OfficeOutput
from quantix.plan_review import PlanReviewService
from quantix.repository import Repository
from quantix.staff_assignment_models import PreparedStaffDraft
from quantix.staff_assignments import StaffAssignmentService
from quantix.staff_context import StaffContextService, build_staff_context
from quantix.staff_models import ManagerCreationContext
from quantix.staff_routing import StaffRoutingService
from quantix.staff_store import StaffStore


@pytest.fixture(autouse=True)
def close_temporary_diagnostics(tmp_path):
    """Close only diagnostic writers created below this test's temp root."""

    before = diagnostics._writer
    yield
    writer = diagnostics._writer
    if writer is None or writer is before or writer.directory is None:
        return
    root = tmp_path.resolve()
    directory = writer.directory.resolve()
    if directory != root and root not in directory.parents:
        return
    writer.close()
    with diagnostics._writer_lock:
        if diagnostics._writer is writer:
            diagnostics._writer = None


def _token() -> str:
    return "synthetic-office-recovery-token-0123456789"


def _profile_context(repo, tender_id, run_id, plan_id):
    return ManagerCreationContext(
        tender_id,
        run_id,
        ManagerProfileService(repo).get().version,
        plan_id,
    )


def _configure_extra_root(repo, tender_id, plan_id, *, instruction):
    run = repo.create_run(tender_id, "manager", instruction)
    ManagerRunProfiles(repo).capture(tender_id, run["id"])
    repo.add_message(tender_id, "engineer", instruction, run_id=run["id"])
    return run, _profile_context(repo, tender_id, run["id"], plan_id)


def test_fresh_normal_app_exposes_one_manager_and_no_generated_office_records(
    tmp_path, monkeypatch
):
    """Normal startup creates the Manager/read service while remaining idle."""

    monkeypatch.setattr("keyring.get_password", lambda *_: None)
    monkeypatch.setattr(
        "quantix.ai_execution.execute_api",
        lambda *_args, **_kwargs: pytest.fail("A fresh office read must not call a provider."),
    )
    monkeypatch.setattr(
        "quantix.staff_runtime.execute_api",
        lambda *_args, **_kwargs: pytest.fail("A fresh office read must not call a provider."),
    )
    home = tmp_path / "fresh-normal-home"
    app = create_app(home, _token())

    with TestClient(app) as client:
        client.headers["Authorization"] = f"Bearer {_token()}"
        health = client.get("/api/health")
        assert health.status_code == 200
        body = health.json()
        assert body["workspace_revision"] == 2
        assert body["office_revision"] == 2
        assert "dynamic_office" in body["capabilities"]

        tender = client.post("/api/tenders", json={"name": "Synthetic recovery Tender"}).json()
        manager = client.get("/api/manager-profile")
        assert manager.status_code == 200
        assert manager.json()["display_name"] == "Tender Manager"

        snapshot = client.get(f"/api/tenders/{tender['id']}/office")
        assert snapshot.status_code == 200
        office = snapshot.json()
        assert office["manager"]["display_name"] == "Tender Manager"
        assert office["staff"] == []
        assert office["assignments"] == []
        assert office["messages"]["items"] == []
        assert office["sequence"] == 0

        repo = app.state.repo
        with repo.db.connect() as conn:
            assert conn.execute("SELECT COUNT(*) FROM office_manager").fetchone()[0] == 1
            assert conn.execute("SELECT COUNT(*) FROM office_manager_versions").fetchone()[0] == 1
            for table in (
                "office_staff",
                "office_work_orders",
                "office_messages",
                "office_events",
                "office_assignments",
                "office_staff_results",
            ):
                assert conn.execute(f"SELECT COUNT(*) FROM {table}").fetchone()[0] == 0
            assert conn.execute("SELECT COUNT(*) FROM runs").fetchone()[0] == 0


def test_startup_marks_abandoned_staff_assignments_interrupted_without_provider_restart(
    tmp_path, monkeypatch
):
    """Queued, running and waiting staff work is recovered with events only."""

    monkeypatch.setattr("keyring.get_password", lambda *_: None)
    provider_calls = []
    monkeypatch.setattr(
        "quantix.ai_execution.execute_api",
        lambda *_args, **_kwargs: provider_calls.append(True),
    )
    monkeypatch.setattr(
        "quantix.staff_runtime.execute_api",
        lambda *_args, **_kwargs: provider_calls.append(True),
    )
    home = tmp_path / "interrupted-office-home"
    (
        repo,
        tender,
        _connections,
        _connection,
        _model,
        _policy,
        _route,
        first_staff,
        first_context,
        _artifact,
    ) = _ready_binding_workspace(
        home,
        monkeypatch,
        max_staff=3,
        max_assignments=3,
    )
    store = StaffStore(repo)
    assignments = StaffAssignmentService(repo)
    routing = StaffRoutingService(repo)

    def bind_and_queue(context, staff, suffix, *, status):
        binding = routing.bind(
            context,
            staff.staff.id,
            staff.work_order.id,
            route_option_id=routing.approved_grant(tender["id"], context.scope_id)
            .envelope.route_options[0]
            .id,
            idempotency_key=f"recovery-binding-{suffix}",
        )
        queued = assignments.queue(context, binding.id, f"recovery-queue-{suffix}")
        repo.update_run(context.run_id, status="running")
        if status in {"running", "waiting"}:
            current = assignments.start(tender["id"], queued.id, queued.revision)
            if status == "waiting":
                current = assignments.wait_for_reply(
                    tender["id"],
                    current.id,
                    current.revision,
                    "Waiting on a synthetic clarification.",
                )
            return current
        return queued

    first = bind_and_queue(first_context, first_staff, "queued", status="queued")

    def create_staff_and_root(number):
        planning = repo.create_run(
            tender["id"], "conversation", f"Create synthetic colleague {number}"
        )
        context = _profile_context(repo, tender["id"], planning["id"], first_context.scope_id)
        staff = store.create_generated(
            context,
            _profile(display_name=f"Synthetic colleague {number}"),
            _order(),
            f"recovery-staff-{number}",
        )
        root, root_context = _configure_extra_root(
            repo, tender["id"], first_context.scope_id, instruction=f"Run recovery root {number}"
        )
        return root_context, staff

    second_context, second_staff = create_staff_and_root(2)
    second = bind_and_queue(second_context, second_staff, "running", status="running")
    third_context, third_staff = create_staff_and_root(3)
    third = bind_and_queue(third_context, third_staff, "waiting", status="waiting")

    assert [first.status, second.status, third.status] == ["queued", "running", "waiting"]
    before_events = len(OfficeEventService(repo).page(tender["id"], limit=200).items)

    app = create_app(home, _token())
    with TestClient(app) as client:
        client.headers["Authorization"] = f"Bearer {_token()}"
        snapshot = client.get(f"/api/tenders/{tender['id']}/office")
        assert snapshot.status_code == 200
        assert {item["status"] for item in snapshot.json()["assignments"]} == {"interrupted"}
        assert app.state.jobs.tasks == {}

    assert provider_calls == []
    recovered = StaffAssignmentService(repo).list(tender["id"])
    assert [item.status for item in recovered] == ["interrupted"] * 3
    assert all("Review and resume" in item.detail for item in recovered)
    event_page = OfficeEventService(repo).page(tender["id"], limit=200)
    assert len(event_page.items) == before_events + 3
    assert [item.event_type for item in event_page.items[-3:]] == [
        "assignment_interrupted",
        "assignment_interrupted",
        "assignment_interrupted",
    ]


def test_backup_restore_retains_dynamic_profiles_versions_portraits_and_office_history(
    tmp_path, monkeypatch
):
    """A checked archive carries dynamic identities and their attributable history."""

    (
        repo,
        tender,
        _connections,
        _connection,
        _model,
        _policy,
        _route,
        staff,
        root_context,
        artifact,
    ) = _ready_binding_workspace(
        tmp_path / "backup-source-home",
        monkeypatch,
        requested_tool_ids=["read_source"],
        envelope_tools=("read_source",),
    )
    artifact_bytes = b"synthetic reviewed source"
    (repo.objects / artifact["content_hash"]).write_bytes(artifact_bytes)
    evidence = repo.artifact_evidence(tender["id"], artifact["id"])[0]
    source_bytes = repo.object_path(tender["id"], artifact["id"]).read_bytes()
    source_hash = hashlib.sha256(source_bytes).hexdigest()
    routing = StaffRoutingService(repo)
    route_option = routing.approved_grant(
        tender["id"], root_context.scope_id
    ).envelope.route_options[0]
    binding = routing.bind(
        root_context,
        staff.staff.id,
        staff.work_order.id,
        route_option.id,
        "backup-binding",
    )
    assignments = StaffAssignmentService(repo)
    queued = assignments.queue(root_context, binding.id, "backup-assignment")
    running = assignments.start(tender["id"], queued.id, queued.revision)
    context = build_staff_context(repo, binding.id, running.id)
    context.source(evidence["id"], offset=0, limit=100)
    messages = OfficeMessageService(repo)
    staff_message = messages.post_staff(
        context,
        "note",
        "The synthetic source was inspected and retained for review.",
        idempotency_key="backup-staff-note",
    )
    prepared = prepare_result(
        OfficeOutput(
            summary="The retained synthetic source is ready for Manager review.",
            source_ids=[evidence["id"]],
        ),
        context,
        {"requests": 0, "input_tokens": 0, "output_tokens": 0, "usage_complete": True},
        ResearchRecord(context),
    )
    result = assignments.save_result(
        tender["id"],
        running.id,
        PreparedStaffDraft(
            assignment_id=running.id,
            staff_id=staff.staff.id,
            staff_version=staff.staff.version,
            route_binding_id=binding.id,
            prepared=prepared,
            authored_notes=("Synthetic draft note.",),
        ),
    )
    result_message = messages.post_staff_result(
        tender["id"], result.id, idempotency_key="backup-staff-result"
    )
    manager_context = OfficeContext(
        repo,
        tender["id"],
        root_context.run_id,
        actor_id=ManagerProfileService(repo).get().id,
    )
    manager_message = messages.post_manager(
        manager_context,
        [OfficeRecipientTarget(staff_id=staff.staff.id, assignment_id=running.id)],
        "instruction",
        "Review the saved synthetic result.",
        idempotency_key="backup-manager-note",
    )

    revision_run = repo.create_run(tender["id"], "conversation", "Revise synthetic colleague")
    revision_context = _profile_context(
        repo, tender["id"], revision_run["id"], root_context.scope_id
    )
    revision = StaffStore(repo).revise_generated(
        revision_context,
        staff.staff.id,
        1,
        _profile(display_name="Synthetic colleague revised"),
        "backup-revision",
    )
    second_order = StaffStore(repo).create_work_order(
        revision_context,
        staff.staff.id,
        revision.staff.version,
        _order(
            brief="Review the retained synthetic result again.",
            source_ids=[evidence["id"]],
        ),
        "backup-second-order",
    )

    backup_service = BackupService(repo)
    backup = backup_service.create()
    archive_path = repo.home / "backups" / backup["filename"]
    before_original = repo.object_path(tender["id"], artifact["id"]).read_bytes()
    destination = Repository(tmp_path / "backup-restored-home")
    BackupService(destination).stage_restore(
        archive_path,
        True,
        "Restore the synthetic office archive.",
        expected_sha256=backup["sha256"],
    )
    apply_pending_restore(destination.home)
    restored = Repository(destination.home)

    restored_staff = StaffStore(restored)
    assert (
        restored_staff.get_staff(tender["id"], staff.staff.id, 1).display_name
        == staff.staff.display_name
    )
    current = restored_staff.get_staff(tender["id"], staff.staff.id)
    assert current.display_name == "Synthetic colleague revised"
    assert current.portrait == staff.staff.portrait
    assert restored_staff.get_work_order(tender["id"], staff.work_order.id).staff_version == 1
    assert (
        restored_staff.get_work_order(tender["id"], second_order.work_order.id).staff_version == 2
    )

    restored_assignment = StaffAssignmentService(restored).get(tender["id"], running.id)
    assert restored_assignment.status == "completed"
    assert restored_assignment.result_id == result.id
    restored_result = StaffAssignmentService(restored).get_result(tender["id"], result.id)
    assert restored_result.office_output.summary == result.office_output.summary
    assert restored_result.source_bases[0].source_id == evidence["id"]
    assert restored_result.source_bases[0].artifact_hash == artifact["content_hash"]
    receipts = StaffContextService(restored).list_receipts(tender["id"], running.id)
    assert [receipt.source_id for receipt in receipts] == [evidence["id"]]
    assert receipts[0].content_hash == artifact["content_hash"]

    restored_messages = OfficeMessageService(restored).page(tender["id"], limit=20).items
    assert {item.id for item in restored_messages} >= {
        staff_message.id,
        result_message.id,
        manager_message.id,
    }
    event_types = [
        item.event_type for item in OfficeEventService(restored).page(tender["id"], limit=200).items
    ]
    assert {
        "assignment_queued",
        "assignment_started",
        "source_inspected",
        "assignment_completed",
    } <= set(event_types)
    assert (restored.home / "objects" / source_hash).read_bytes() == source_bytes
    assert repo.object_path(tender["id"], artifact["id"]).read_bytes() == before_original
    assert hashlib.sha256(before_original).hexdigest() == source_hash


def test_confirmed_reset_recovery_does_not_construct_workspace_services_or_database(
    tmp_path, monkeypatch
):
    """A confirmed reset starts only the recovery surface and leaves no DB."""

    home = (tmp_path / "confirmed-reset-home").resolve()
    home.mkdir()
    write_journal(
        home,
        {
            "format": 1,
            "reset_id": new_id(),
            "home": str(home),
            "phase": "ready",
            "credentials_cleared": True,
            "fingerprint": "a" * 64,
            "confirmed_at": "2026-09-10T00:00:00+00:00",
            "detail": "Saved credentials are removed.",
            "credential_targets": [],
        },
    )
    calls = []

    def forbidden(*_args, **_kwargs):
        calls.append(True)
        raise AssertionError("Normal workspace services are forbidden during reset recovery.")

    monkeypatch.setattr("quantix.api.Repository", forbidden)
    monkeypatch.setattr("quantix.api.OfficeReadService", forbidden)
    monkeypatch.setattr("quantix.office_read.ManagerProfileService", forbidden)
    monkeypatch.setattr("quantix.office_read.StaffStore", forbidden)
    monkeypatch.setattr("quantix.office_read.OfficeEventService", forbidden)

    app = create_app(home, _token())
    assert app.state.repo is None
    assert not (home / "quantix.sqlite").exists()
    with TestClient(app) as client:
        client.headers["Authorization"] = f"Bearer {_token()}"
        health = client.get("/api/health")
        assert health.status_code == 200
        assert health.json()["reset_pending"] is True
        assert "dynamic_office" not in health.json()["capabilities"]
    assert calls == []
    assert not (home / "quantix.sqlite").exists()


@pytest.mark.asyncio
@pytest.mark.parametrize("delay_cleanup", [False, True])
async def test_stopping_dynamic_root_interrupts_child_and_keeps_reservation_uncertain(
    tmp_path, monkeypatch, delay_cleanup
):
    """Root Stop interrupts active staff work and preserves unresolved usage."""

    from quantix.staff_runtime_models import StaffCompletedDraft, StaffProviderOutput

    repo, tender, *_ = configured_office(tmp_path / "cancel-root-home", monkeypatch)
    _, evidence, _ = source(repo, tender["id"])
    plan = repo.create_plan(
        tender["id"],
        "Synthetic cancellation review",
        [
            {
                "title": "Review source",
                "role": "Generated reviewer",
                "description": "Review source",
                "source_ids": [evidence["id"]],
            }
        ],
    )
    jobs = JobManager(repo, object())
    child_started = asyncio.Event()
    cleanup_started = asyncio.Event()
    cleanup_release = asyncio.Event()
    child_callbacks = {}
    provider_calls = []

    async def provider(route, connection, credentials, context, prompt, output_type, **options):
        provider_calls.append("staff" if context.is_staff else "manager")
        reservation = await options["before_request"](200, 200)
        usage = {
            "requests": 1,
            "input_tokens": 200,
            "output_tokens": 100,
            "web_search_calls": 0,
            "usage_complete": True,
        }
        if context.is_staff:
            child_callbacks.update(options)
            child_started.set()
            try:
                await asyncio.Event().wait()
            except asyncio.CancelledError:
                cleanup_started.set()
                if delay_cleanup:
                    await cleanup_release.wait()
                raise
        definitions = {item.name: item for item in options["definitions"]}
        if not context.is_staff:
            payload = json.loads(prompt)
            profile = _profile(requested_tool_ids=["read_source"]).model_dump(mode="json")
            order = _order(source_ids=[evidence["id"]]).model_dump(mode="json")
            generated = json.loads(
                await definitions["create_staff"].invoke(
                    context,
                    {"profile": profile, "work_order": order},
                    invocation_id="cancel-create",
                )
            )
            await definitions["execute_staff"].invoke(
                context,
                {
                    "staff_id": generated["staff"]["id"],
                    "work_order_id": generated["work_order"]["id"],
                    "route_option_id": payload["reviewed_delegation"]["route_options"][0]["id"],
                },
                invocation_id="cancel-assign",
            )
            output = OfficeOutput(summary="The synthetic child is queued.")
            await options["on_response"](usage, reservation)
            return {"output": output, "usage": usage, "web_sources": []}
        output = StaffProviderOutput(
            result=StaffCompletedDraft(output=OfficeOutput(summary="Never reached."))
        )
        await options["on_response"](usage, reservation)
        return {"output": output, "usage": usage, "web_sources": []}

    monkeypatch.setattr("quantix.ai_execution.execute_api", provider)
    monkeypatch.setattr("quantix.staff_runtime.execute_api", provider)
    review_service = PlanReviewService(
        repo,
        save_runs_in_transaction=jobs.queue_approved_plan_runs,
        schedule_after_commit=jobs.schedule_approved_plan_runs,
    )
    review = review_service.review(tender["id"], plan["id"])
    approved = review_service.approve_and_start(
        tender["id"], plan["id"], {"fingerprint": review["fingerprint"], "engineer_confirmed": True}
    )
    root_id = approved["work_intents"][0]["run_id"]
    await asyncio.wait_for(child_started.wait(), timeout=5)
    jobs.stop_tender_work(tender["id"])
    assert repo.get_run(root_id)["status"] == "cancelled"
    assert StaffAssignmentService(repo).list(tender["id"])[0].status == "interrupted"
    with pytest.raises(ValueError, match="active|running"):
        await child_callbacks["before_request"](100, 100)
    if delay_cleanup:
        await asyncio.wait_for(cleanup_started.wait(), timeout=5)
        assert any(not task.done() for task in jobs.tasks.values())
        with pytest.raises(ValueError, match="active|running"):
            await child_callbacks["before_request"](100, 100)
        cleanup_release.set()
    await asyncio.gather(*list(jobs.tasks.values()), return_exceptions=True)

    root = repo.get_run(root_id)
    assert root["status"] == "cancelled"
    saved_assignments = StaffAssignmentService(repo).list(tender["id"])
    assert len(saved_assignments) == 1
    assert saved_assignments[0].status == "interrupted"
    assert provider_calls == ["manager", "staff"]
    with repo.db.connect() as conn:
        rows = [
            json.loads(row[0])
            for row in conn.execute("SELECT data_json FROM ai_usage WHERE run_id=?", (root_id,))
        ]
    assert rows
    assert any(row["status"] == "uncertain" and row.get("interrupted") is True for row in rows)
    await jobs.close()
