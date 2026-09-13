"""Synthetic local HTTP and bundled execution acceptance; no external AI calls."""

import asyncio
import json
import sqlite3
from contextlib import asynccontextmanager

import pytest
from fastapi.testclient import TestClient
from pydantic_ai.models.function import DeltaToolCall, FunctionModel
from test_catalog_authority import configured_office

from quantix.ai_api_provider import APIModelBinding
from quantix.api import create_app
from quantix.jobs import JobManager
from quantix.office_types import OfficeOutput


def _plan(repo, tender_id):
    return repo.create_plan(tender_id, "Synthetic plan", [
        {"title": f"Review {name}", "role": name, "description": f"Review {name}", "source_ids": []}
        for name in ("Concrete", "Rebar")])


async def _finish(jobs):
    await asyncio.gather(*list(jobs.tasks.values()))


def test_actual_http_combined_approval_commits_before_scheduling_and_retries_once(tmp_path, monkeypatch):
    app = create_app(tmp_path, "synthetic-session")
    repo, tender, _, _, _, _, _ = configured_office(tmp_path, monkeypatch, repo=app.state.repo)
    plan = _plan(repo, tender["id"])
    jobs = app.state.jobs
    scheduled = []
    schedule = jobs._schedule

    def committed_schedule(run, task_id=None):
        # An independent reader sees each durable run before its coroutine is scheduled.
        with sqlite3.connect(repo.db.path) as connection:
            assert connection.execute("SELECT status FROM runs WHERE id=?", (run["id"],)).fetchone() == ("queued",)
            assert connection.execute("SELECT status FROM plans WHERE id=?", (plan["id"],)).fetchone() == ("approved",)
        assert repo.db.held_connection() is None
        scheduled.append(run["id"])
        return schedule(run, task_id)

    async def synthetic_execute(route, connection, credentials, context, instruction, output_type, **hooks):
        reservation = await hooks["before_request"](100, 100)
        usage = {"requests": 1, "input_tokens": 100, "output_tokens": 50, "usage_complete": True}
        await hooks["on_response"](usage, reservation)
        return {"output": OfficeOutput(summary="Synthetic review complete; no source evidence was inspected."),
                "usage": usage, "web_sources": []}

    monkeypatch.setattr(jobs, "_schedule", committed_schedule)
    monkeypatch.setattr("quantix.ai_execution.execute_api", synthetic_execute)
    with TestClient(app) as client:
        client.headers["Authorization"] = "Bearer synthetic-session"
        url = f"/api/tenders/{tender['id']}/plans/{plan['id']}/review"
        review = client.get(url)
        assert review.status_code == 200
        assert review.json()["can_approve"] is True
        request = {"fingerprint": review.json()["fingerprint"], "engineer_confirmed": True, "rationale": "   "}
        response = client.post(url + "/approve-and-start", json=request)
        assert response.status_code == 200, response.text
        receipt = response.json()
        client.portal.call(_finish, jobs)
        assert len(scheduled) == 1
        assert receipt["work_intents"][0]["kind"] == "manager"
        assert receipt["work_intents"][0]["task_id"] is None
        assert {intent["run_id"] for intent in receipt["work_intents"]} == set(scheduled)
        assert all(repo.get_run(identifier)["status"] == "completed" for identifier in scheduled)
        retry = client.post(url + "/approve-and-start", json=request)
        assert retry.status_code == 200
        assert retry.json()["work_intents"] == receipt["work_intents"]
        assert len(scheduled) == len(repo.list_runs(tender["id"])) == 1


def test_actual_http_combined_approval_queue_failure_rolls_back_every_grant(tmp_path, monkeypatch):
    app = create_app(tmp_path, "synthetic-session")
    repo, tender, connections, account, _, policy, _ = configured_office(tmp_path, monkeypatch, repo=app.state.repo)
    plan = _plan(repo, tender["id"])
    # A displayed, stale account grant is eligible for explicit renewal, but
    # must remain stale if recording the single Manager root fails.
    with repo.db.connect(write=True) as conn:
        row = conn.execute("SELECT data_json FROM tender_ai_policy WHERE tender_id=?", (tender["id"],)).fetchone()
        saved = json.loads(row[0])
        saved["_connection_versions"][account["id"]] = 0
        conn.execute("UPDATE tender_ai_policy SET data_json=? WHERE tender_id=?", (json.dumps(saved), tender["id"]))
        policy_before = conn.execute("SELECT revision,data_json FROM tender_ai_policy WHERE tender_id=?", (tender["id"],)).fetchone()
        policy_before = tuple(policy_before)
    original = app.state.jobs._queue
    calls = []

    def fail_after_root(*args, **kwargs):
        result = original(*args, **kwargs)
        calls.append(result["id"])
        raise RuntimeError("Synthetic failure after saving the Manager root")

    monkeypatch.setattr(app.state.jobs, "_queue", fail_after_root)
    with TestClient(app, raise_server_exceptions=False) as client:
        client.headers["Authorization"] = "Bearer synthetic-session"
        url = f"/api/tenders/{tender['id']}/plans/{plan['id']}/review"
        review = client.get(url).json()
        assert review["can_approve"] is True, review["blockers"]
        response = client.post(url + "/approve-and-start", json={
            "fingerprint": review["fingerprint"], "engineer_confirmed": True})
        assert response.status_code == 500
        assert len(calls) == 1
        assert repo.list_runs(tender["id"]) == []
        assert app.state.jobs.tasks == {}
        assert repo.get_plan(tender["id"], plan["id"])["status"] == "proposed"
        with repo.db.connect() as conn:
            assert tuple(conn.execute("SELECT revision,data_json FROM tender_ai_policy WHERE tender_id=?", (tender["id"],)).fetchone()) == policy_before
            for table in ("plan_ai_team", "ai_team_history", "plan_review_approvals"):
                assert conn.execute(f"SELECT count(*) FROM {table}").fetchone()[0] == 0


@pytest.mark.asyncio
@pytest.mark.parametrize("max_requests", [1, 2])
async def test_routing_and_engineering_share_per_run_request_budget(tmp_path, monkeypatch, max_requests):
    repo, tender, _, _, _, policy, _ = configured_office(tmp_path, monkeypatch, max_requests=max_requests)
    calls = []

    async def scripted(messages, info):
        conversation = not info.function_tools
        calls.append(conversation)
        payload = {"kind": "engineering", "reply": "", "next_action": "Review sources"} if conversation else {
            "summary": "No project evidence has been inspected."}
        encoded = json.dumps(payload)
        split = len(encoded) // 2
        yield {0: DeltaToolCall(name=info.output_tools[0].name, json_args=encoded[:split])}
        yield {0: DeltaToolCall(json_args=encoded[split:])}

    @asynccontextmanager
    async def binding(*_):
        yield APIModelBinding(model=FunctionModel(stream_function=scripted, model_name="synthetic-model"), settings={}, client=None)

    monkeypatch.setattr("quantix.ai_api_engine.model_for_route", binding)
    jobs = JobManager(repo, object())
    for index in range(2):
        response = jobs.submit_message(tender["id"], "Review concrete", idempotency_key=f"synthetic-{index}")
        run_id = response["run"]["id"]
        await _finish(jobs)
        run = repo.get_run(run_id)
        assert run["status"] == ("completed" if max_requests == 2 else "failed"), run
        with repo.db.connect() as conn:
            assert policy._totals(conn, tender["id"], run_id)[2] == max_requests
    assert calls == ([True, False, True, False] if max_requests == 2 else [True, True])
    assert len(repo.list_runs(tender["id"])) == 2
    await jobs.close()


def test_consumed_and_edited_idempotent_receipts_cannot_target_later_pending(tmp_path, monkeypatch):
    repo, tender, *_ = configured_office(tmp_path, monkeypatch)
    jobs = JobManager(repo, object())
    monkeypatch.setattr(jobs, "_schedule", lambda run, task_id=None: run)
    prior = repo.create_run(tender["id"], "task")
    initial = jobs.submit_message(tender["id"], "Original", idempotency_key="original")["pending"]
    edited = jobs.edit_pending(tender["id"], "Edited", pending_id=initial["id"], expected_revision=initial["revision"])
    with pytest.raises(ValueError, match="no longer identifies"):
        jobs.submit_message(tender["id"], "Original", idempotency_key="original")
    repo.update_run(prior["id"], status="completed")
    consumed = jobs.consume_pending(tender["id"], pending_id=edited["id"], expected_revision=edited["revision"])
    next_pending = jobs.submit_message(tender["id"], "Later", idempotency_key="later")["pending"]
    receipt = jobs.submit_message(tender["id"], "Edited", idempotency_key=edited["idempotency_key"])
    assert receipt["run"]["id"] == consumed["run"]["id"]
    assert receipt["outcome"] == "immediate"
    assert jobs.pending.get(tender["id"])["id"] == next_pending["id"]


@pytest.mark.asyncio
async def test_interrupted_conversation_marks_unreported_usage_uncertain(tmp_path, monkeypatch):
    from quantix.conversation import run_conversation

    repo, tender, *_ = configured_office(tmp_path, monkeypatch)
    run = repo.create_run(tender["id"], "conversation", "Hello")

    async def interrupted(*args, **hooks):
        await hooks["before_request"](100, 100)
        raise asyncio.CancelledError

    monkeypatch.setattr("quantix.ai_execution.execute_api", interrupted)
    with pytest.raises(asyncio.CancelledError):
        await run_conversation(repo, tender["id"], run["id"], "Hello")
    with repo.db.connect() as conn:
        usage = json.loads(conn.execute("SELECT data_json FROM ai_usage WHERE run_id=?", (run["id"],)).fetchone()[0])
    assert usage["status"] == "uncertain"
    assert usage["reserved_usd"] > 0


def test_pending_edit_and_new_idempotency_receipt_commit_together(tmp_path, monkeypatch):
    from quantix.pending import PendingInstructionService

    repo, tender, *_ = configured_office(tmp_path, monkeypatch)
    pending = PendingInstructionService(repo)
    original = pending.upsert(tender["id"], "Original", "original", [])
    remember = pending.remember_idempotency

    def atomic_receipt(*args, **kwargs):
        # Another connection must still see the original draft until its new
        # receipt is saved. Otherwise a concurrent consume can lose its run ID
        # when this edit subsequently overwrites that receipt.
        with sqlite3.connect(repo.db.path) as reader:
            assert reader.execute("SELECT revision FROM pending_instructions WHERE id=?", (original["id"],)).fetchone() == (original["revision"],)
        return remember(*args, **kwargs)

    monkeypatch.setattr(pending, "remember_idempotency", atomic_receipt)
    edited = pending.edit(tender["id"], "Edited", pending_id=original["id"], expected_revision=original["revision"])
    assert pending.lookup_idempotency(tender["id"], edited["idempotency_key"], "Edited")["pending_id"] == original["id"]


@pytest.mark.parametrize("status", ["failed", "cancelled", "interrupted"])
def test_http_resume_conversation_reclassifies_without_duplicate_message_or_receipt(tmp_path, monkeypatch, status):
    from quantix.conversation import ConversationOutput

    app = create_app(tmp_path, "synthetic-session")
    repo, tender, *_ = configured_office(tmp_path, monkeypatch, repo=app.state.repo)
    original = repo.create_run(tender["id"], "conversation", "Hello")
    repo.add_message(tender["id"], "engineer", "Hello")
    repo.update_run(original["id"], status=status)
    jobs = app.state.jobs
    jobs.pending.remember_idempotency(tender["id"], "original", "Hello", outcome_kind="immediate", run_id=original["id"])
    pending = jobs.pending.upsert(tender["id"], "Later instruction", "later", [original["id"]])
    jobs.pending.on_run_finished(original["id"], status)
    operations = []

    async def conversation(route, account, credentials, context, instruction, output_type, **hooks):
        operations.append(hooks["operation"])
        assert hooks["definitions"] == []
        assert context.run_id != original["id"]
        reservation = await hooks["before_request"](100, 100)
        usage = {"requests": 1, "input_tokens": 100, "output_tokens": 50, "usage_complete": True}
        await hooks["on_response"](usage, reservation)
        return {"output": ConversationOutput(kind="conversation", reply="Hello."), "usage": usage, "web_sources": []}

    monkeypatch.setattr("quantix.ai_execution.execute_api", conversation)
    with TestClient(app) as client:
        client.headers["Authorization"] = "Bearer synthetic-session"
        response = client.post(f"/api/runs/{original['id']}/resume")
        assert response.status_code == 200, response.text
        resumed = response.json()
        client.portal.call(_finish, jobs)
        assert resumed["kind"] == "conversation"
        assert resumed["instruction"] == original["instruction"]
        assert repo.get_run(resumed["id"])["status"] == "completed"
        assert repo.get_run(original["id"])["status"] == status
        assert operations == ["conversation"]
        assert [message["role"] for message in repo.messages(tender["id"])] == ["engineer", "manager"]
        assert jobs.submit_message(tender["id"], "Hello", idempotency_key="original")["run"]["id"] == original["id"]
        assert jobs.pending.get(tender["id"])["id"] == pending["id"]
        assert jobs.pending.get(tender["id"])["status"] == "held"
        assert len(repo.list_runs(tender["id"])) == 2


def test_http_resume_conversation_rechecks_changed_account_authority(tmp_path, monkeypatch):
    from quantix.ai_models import ConnectionInput

    app = create_app(tmp_path, "synthetic-session")
    repo, tender, connections, account, *_ = configured_office(tmp_path, monkeypatch, repo=app.state.repo)
    original = repo.create_run(tender["id"], "conversation", "Hello")
    repo.update_run(original["id"], status="failed")
    values = {key: value for key, value in account.items() if key in ConnectionInput.model_fields}
    values.update(credentials={"api_key": "changed-synthetic-key"}, session_only=True)
    connections.update(account["id"], values)
    with TestClient(app) as client:
        client.headers["Authorization"] = "Bearer synthetic-session"
        response = client.post(f"/api/runs/{original['id']}/resume")
        assert response.status_code == 409
        assert "connection changed" in response.json()["detail"].lower()
        assert len(repo.list_runs(tender["id"])) == 1
        assert repo.messages(tender["id"]) == []


@pytest.mark.asyncio
@pytest.mark.parametrize(("reply", "action", "expected"), [
    ("Hello.", "Add files.", "Hello.\n\nAdd files."),
    ("مرحباً.", "أضف ملفات المناقصة.", "مرحباً.\n\nأضف ملفات المناقصة."),
    ("Hello. Add files.", "Add files.", "Hello. Add files."),
])
async def test_conversation_publishes_its_next_action_once(tmp_path, monkeypatch, reply, action, expected):
    from quantix.conversation import ConversationOutput, PreparedConversationResult

    repo, tender, *_ = configured_office(tmp_path, monkeypatch)
    jobs = JobManager(repo, object())
    run = repo.create_run(tender["id"], "conversation", "Hello")

    async def completed(*_):
        return PreparedConversationResult(tender["id"], run["id"],
            ConversationOutput(kind="conversation", reply=reply, next_action=action), {})

    monkeypatch.setattr("quantix.conversation.run_conversation", completed)
    jobs._schedule(run)
    await _finish(jobs)
    assert repo.messages(tender["id"])[-1]["content"] == expected
    await jobs.close()
