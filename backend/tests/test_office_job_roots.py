"""The approved office owns one durable root and retains real engineer origins."""

import pytest
from test_catalog_authority import configured_office

from quantix.jobs import JobManager
from quantix.manager_runtime import ManagerRunProfiles
from quantix.plan_review import PlanReviewService
from quantix.staff_routing import StaffRoutingService


def approved_office(tmp_path, monkeypatch, *, schedule=None):
    repo, tender, *_ = configured_office(tmp_path, monkeypatch)
    plan = repo.create_plan(tender["id"], "Review the tender package", [
        {"title": title, "role": "Engineer-defined specialty", "description": title, "source_ids": []}
        for title in ("Check concrete requirements", "Compare envelope requirements")
    ])
    jobs = JobManager(repo, object())
    if schedule is not None:
        monkeypatch.setattr(jobs, "_schedule", schedule(jobs))
    service = PlanReviewService(
        repo, save_runs_in_transaction=jobs.queue_approved_plan_runs,
        schedule_after_commit=jobs.schedule_approved_plan_runs if schedule is not None else None,
    )
    review = service.review(tender["id"], plan["id"])
    result = service.approve_and_start(tender["id"], plan["id"], {
        "fingerprint": review["fingerprint"], "engineer_confirmed": True,
    })
    return repo, tender, plan, jobs, service, result


def test_approval_queues_one_pinned_manager_for_all_work_and_schedules_after_commit(tmp_path, monkeypatch):
    observed = []

    def schedule(jobs):
        def record(run, task_id=None):
            # A separate SQLite connection must see the committed receipt.
            import sqlite3
            with sqlite3.connect(jobs.repo.db.path) as conn:
                assert conn.execute("SELECT COUNT(*) FROM plan_review_approvals").fetchone()[0] == 1
            assert task_id is None
            observed.append(run["id"])
            jobs.tasks[run["id"]] = object()
            return run
        return record

    repo, tender, plan, jobs, service, result = approved_office(tmp_path, monkeypatch, schedule=schedule)
    intents = result["work_intents"]
    assert len(intents) == 1
    assert intents[0]["kind"] == "manager"
    assert intents[0]["task_id"] is None
    run = repo.get_run(intents[0]["run_id"])
    assert ManagerRunProfiles(repo).get(tender["id"], run["id"]).version >= 1
    assert len(repo.list_runs(tender["id"])) == 1
    assert all(task["status"] == "ready" for task in repo.get_plan(tender["id"], plan["id"])["tasks"])
    assert observed == [run["id"]]
    jobs.schedule_approved_plan_runs(tender["id"], plan["id"], intents)
    service.approve_and_start(tender["id"], plan["id"], {
        "fingerprint": result["review"]["fingerprint"], "engineer_confirmed": True,
    })
    assert observed == [run["id"]]


def test_scheduler_rejects_an_unrecorded_root_or_changed_plan_identity(tmp_path, monkeypatch):
    repo, tender, plan, jobs, _, result = approved_office(tmp_path, monkeypatch)
    monkeypatch.setattr(jobs, "_schedule", lambda *_: pytest.fail("Unrecorded work must not schedule"))
    raw = repo.create_run(tender["id"], "manager", "Unrecorded root")
    forged = result["work_intents"][0] | {"run_id": raw["id"]}
    with pytest.raises(ValueError, match="receipt|recorded|approved"):
        jobs.schedule_approved_plan_runs(tender["id"], plan["id"], [forged])
    with pytest.raises((ValueError, KeyError), match="plan|Tender"):
        jobs.schedule_approved_plan_runs(tender["id"], "different-plan", result["work_intents"])


def test_later_engineer_submission_is_linked_to_its_actual_root(tmp_path, monkeypatch):
    repo, tender, plan, jobs, _, result = approved_office(tmp_path, monkeypatch)
    repo.update_run(result["work_intents"][0]["run_id"], status="completed")
    monkeypatch.setattr(jobs, "_schedule", lambda run, *_: run)
    response = jobs.submit_message(tender["id"], "Check an unfamiliar concrete interface", idempotency_key="later-real-request")
    run = response["run"]
    messages = [message for message in repo.messages(tender["id"]) if message["role"] == "engineer"]
    assert len(messages) == 1
    assert messages[0]["run_id"] == run["id"]
    routing = StaffRoutingService(repo)
    grant = routing.approved_grant(tender["id"], plan["id"])
    with repo.atomic() as conn:
        routing._validate_root_origin(conn, tender["id"], run["id"], grant)
    assert jobs.submit_message(tender["id"], run["instruction"], idempotency_key="later-real-request")["run"]["id"] == run["id"]
    assert len([message for message in repo.messages(tender["id"]) if message["role"] == "engineer"]) == 1


def test_explicit_resume_retains_dialogue_and_records_a_valid_new_attempt(tmp_path, monkeypatch):
    repo, tender, plan, jobs, _, result = approved_office(tmp_path, monkeypatch)
    monkeypatch.setattr(jobs, "_schedule", lambda run, *_: run)
    original_id = result["work_intents"][0]["run_id"]
    repo.update_run(original_id, status="failed")
    before_messages = repo.messages(tender["id"])
    resumed = jobs.resume(original_id)
    StaffRoutingService(repo).validate_root(tender["id"], resumed["id"], plan["id"])
    assert repo.messages(tender["id"]) == before_messages
    assert resumed["instruction"] == repo.get_run(original_id)["instruction"]
    repo.update_run(resumed["id"], status="cancelled")
    again = jobs.resume(resumed["id"])
    StaffRoutingService(repo).validate_root(tender["id"], again["id"], plan["id"])
    assert repo.messages(tender["id"]) == before_messages
    with repo.db.connect() as conn:
        assert conn.execute("SELECT COUNT(*) FROM office_resume_origins").fetchone()[0] == 2
        assert conn.execute("SELECT COUNT(*) FROM decisions WHERE target_type='office_run' AND decision='resume'").fetchone()[0] == 2


def test_raw_stopped_run_cannot_acquire_delegation_through_resume(tmp_path, monkeypatch):
    repo, tender, _, jobs, _, result = approved_office(tmp_path, monkeypatch)
    repo.update_run(result["work_intents"][0]["run_id"], status="completed")
    raw = repo.create_run(tender["id"], "manager", "Unrecorded work")
    ManagerRunProfiles(repo).capture(tender["id"], raw["id"])
    repo.update_run(raw["id"], status="failed")
    before = repo.list_runs(tender["id"])
    monkeypatch.setattr(jobs, "_schedule", lambda *_: pytest.fail("An unrecorded root must not be resumed into delegation"))
    with pytest.raises(ValueError, match="no instruction"):
        jobs.resume(raw["id"])
    assert repo.list_runs(tender["id"]) == before
