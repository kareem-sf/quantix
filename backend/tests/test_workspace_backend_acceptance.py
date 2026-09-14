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


def _plan(repo, tender_id):
    return repo.create_plan(
        tender_id,
        "Synthetic plan",
        [
            {
                "title": f"Review {name}",
                "role": name,
                "description": f"Review {name}",
                "source_ids": [],
            }
            for name in ("Concrete", "Rebar")
        ],
    )


async def _finish(jobs):
    await asyncio.gather(*list(jobs.tasks.values()))


@pytest.mark.asyncio
async def test_each_message_is_one_metered_manager_request(tmp_path, monkeypatch):
    repo, tender, _, _, _, policy, _ = configured_office(tmp_path, monkeypatch, max_requests=1)
    offered_tools = []

    async def scripted(messages, info):
        offered_tools.append(bool(info.function_tools))
        encoded = json.dumps({"summary": "No project evidence has been inspected."})
        split = len(encoded) // 2
        yield {0: DeltaToolCall(name=info.output_tools[0].name, json_args=encoded[:split])}
        yield {0: DeltaToolCall(json_args=encoded[split:])}

    @asynccontextmanager
    async def binding(*_):
        yield APIModelBinding(
            model=FunctionModel(stream_function=scripted, model_name="synthetic-model"),
            settings={},
            client=None,
        )

    monkeypatch.setattr("quantix.ai_api_engine.model_for_route", binding)
    jobs = JobManager(repo, object())
    for index in range(2):
        response = jobs.submit_message(
            tender["id"], "Review concrete", idempotency_key=f"synthetic-{index}"
        )
        run_id = response["run"]["id"]
        await _finish(jobs)
        run = repo.get_run(run_id)
        assert run["status"] == "completed", run
        assert run["kind"] == "manager"
        with repo.db.connect() as conn:
            assert policy._totals(conn, tender["id"], run_id)[2] == 1
    assert offered_tools == [True, True]
    assert len(repo.list_runs(tender["id"])) == 2
    await jobs.close()


def test_consumed_and_edited_idempotent_receipts_cannot_target_later_pending(tmp_path, monkeypatch):
    repo, tender, *_ = configured_office(tmp_path, monkeypatch)
    jobs = JobManager(repo, object())
    monkeypatch.setattr(jobs, "_schedule", lambda run, task_id=None: run)
    prior = repo.create_run(tender["id"], "manager")
    initial = jobs.submit_message(tender["id"], "Original", idempotency_key="original")["pending"]
    edited = jobs.edit_pending(
        tender["id"], "Edited", pending_id=initial["id"], expected_revision=initial["revision"]
    )
    with pytest.raises(ValueError, match="no longer identifies"):
        jobs.submit_message(tender["id"], "Original", idempotency_key="original")
    repo.update_run(prior["id"], status="completed")
    consumed = jobs.consume_pending(
        tender["id"], pending_id=edited["id"], expected_revision=edited["revision"]
    )
    next_pending = jobs.submit_message(tender["id"], "Later", idempotency_key="later")["pending"]
    receipt = jobs.submit_message(tender["id"], "Edited", idempotency_key=edited["idempotency_key"])
    assert receipt["run"]["id"] == consumed["run"]["id"]
    assert receipt["outcome"] == "immediate"
    assert jobs.pending.get(tender["id"])["id"] == next_pending["id"]


@pytest.mark.asyncio
async def test_interrupted_manager_request_marks_unreported_usage_uncertain(tmp_path, monkeypatch):
    from quantix.office import run_manager

    repo, tender, *_ = configured_office(tmp_path, monkeypatch)
    run = repo.create_run(tender["id"], "manager", "Hello")

    async def interrupted(*args, **hooks):
        await hooks["before_request"](100, 100)
        raise asyncio.CancelledError

    monkeypatch.setattr("quantix.ai_execution.execute_api", interrupted)
    with pytest.raises(asyncio.CancelledError):
        await run_manager(repo, tender["id"], run["id"], "Hello")
    with repo.db.connect() as conn:
        usage = json.loads(
            conn.execute("SELECT data_json FROM ai_usage WHERE run_id=?", (run["id"],)).fetchone()[
                0
            ]
        )
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
            assert reader.execute(
                "SELECT revision FROM pending_instructions WHERE id=?", (original["id"],)
            ).fetchone() == (original["revision"],)
        return remember(*args, **kwargs)

    monkeypatch.setattr(pending, "remember_idempotency", atomic_receipt)
    edited = pending.edit(
        tender["id"], "Edited", pending_id=original["id"], expected_revision=original["revision"]
    )
    assert (
        pending.lookup_idempotency(tender["id"], edited["idempotency_key"], "Edited")["pending_id"]
        == original["id"]
    )


@pytest.mark.parametrize("status", ["failed", "cancelled", "interrupted"])
def test_http_resume_reruns_the_manager_without_duplicate_message_or_receipt(
    tmp_path, monkeypatch, status
):

    app = create_app(tmp_path, "synthetic-session")
    repo, tender, *_ = configured_office(tmp_path, monkeypatch, repo=app.state.repo)
    original = repo.create_run(tender["id"], "manager", "Hello")
    repo.add_message(tender["id"], "engineer", "Hello")
    repo.update_run(original["id"], status=status)
    jobs = app.state.jobs
    jobs.pending.remember_idempotency(
        tender["id"], "original", "Hello", outcome_kind="immediate", run_id=original["id"]
    )
    pending = jobs.pending.upsert(tender["id"], "Later instruction", "later", [original["id"]])
    jobs.pending.on_run_finished(original["id"], status)
    turns = []

    async def manager(route, account, credentials, context, instruction, output_type, **hooks):
        turns.append(context.run_id)
        assert context.run_id != original["id"]
        reservation = await hooks["before_request"](100, 100)
        usage = {"requests": 1, "input_tokens": 100, "output_tokens": 50, "usage_complete": True}
        await hooks["on_response"](usage, reservation)
        return {"output": {"summary": "Hello."}, "usage": usage, "web_sources": []}

    monkeypatch.setattr("quantix.ai_execution.execute_api", manager)
    with TestClient(app) as client:
        client.headers["Authorization"] = "Bearer synthetic-session"
        response = client.post(f"/api/runs/{original['id']}/resume")
        assert response.status_code == 200, response.text
        resumed = response.json()
        client.portal.call(_finish, jobs)
        assert resumed["kind"] == "manager"
        assert resumed["instruction"] == original["instruction"]
        assert repo.get_run(resumed["id"])["status"] == "completed"
        assert repo.get_run(original["id"])["status"] == status
        assert turns == [resumed["id"]]
        assert [message["role"] for message in repo.messages(tender["id"])] == [
            "engineer",
            "manager",
        ]
        assert (
            jobs.submit_message(tender["id"], "Hello", idempotency_key="original")["run"]["id"]
            == original["id"]
        )
        assert jobs.pending.get(tender["id"])["id"] == pending["id"]
        assert jobs.pending.get(tender["id"])["status"] == "held"
        assert len(repo.list_runs(tender["id"])) == 2


def test_http_resume_rechecks_changed_account_authority(tmp_path, monkeypatch):
    from quantix.ai_models import ConnectionInput

    app = create_app(tmp_path, "synthetic-session")
    repo, tender, connections, account, *_ = configured_office(
        tmp_path, monkeypatch, repo=app.state.repo
    )
    original = repo.create_run(tender["id"], "manager", "Hello")
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
