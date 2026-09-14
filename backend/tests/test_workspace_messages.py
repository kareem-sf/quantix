import asyncio

import pytest
from fastapi.testclient import TestClient

from quantix.api import create_app
from quantix.jobs import JobManager
from quantix.office_types import OfficeOutput, PreparedOfficeResult
from quantix.pending import PendingInstructionService
from quantix.repository import Repository


def test_pending_instruction_has_one_editable_record_and_idempotency_conflict(tmp_path):
    repo = Repository(tmp_path)
    tender = repo.create_tender("Pending")
    service = PendingInstructionService(repo)

    first = service.upsert(
        tender["id"], "Review documents", "request-1", [], action="review_documents"
    )
    with pytest.raises(ValueError, match="idempotency"):
        service.upsert(tender["id"], "Changed body", "request-1", [])
    with pytest.raises(ValueError, match="Edit or cancel"):
        service.upsert(tender["id"], "Review the drawings", "request-2", [])
    edited = service.edit(
        tender["id"],
        "Review the drawings",
        pending_id=first["id"],
        expected_revision=first["revision"],
    )
    assert edited["id"] == first["id"]
    assert edited["content"] == "Review the drawings"
    assert edited["action"] == "review_documents"
    assert service.get(tender["id"])["id"] == first["id"]


def test_message_history_page_links_only_persisted_source_relationships(tmp_path):
    repo = Repository(tmp_path)
    tender = repo.create_tender("History")
    artifact, _ = repo.register_artifact(
        tender["id"],
        "spec.pdf",
        "a" * 64,
        10,
        {"kind": "pdf", "status": "extracted", "segments": [{"locator": "p1", "text": "C30"}]},
    )
    evidence = repo.artifact_evidence(tender["id"], artifact["id"])[0]
    run = repo.create_run(tender["id"], "manager")
    finding = repo.add_finding(
        tender["id"], "Concrete", "C30", "requirement", [evidence["id"]], run_id=run["id"]
    )
    repo.add_message(tender["id"], "manager", "C30", [evidence["id"]], run_id=run["id"])
    repo.add_message(tender["id"], "manager", "No source", [])
    page = repo.message_page(tender["id"], limit=1)
    assert len(page["items"]) == 1
    assert page["items"][0]["result_links"] == []
    assert page["next_cursor"]
    page2 = repo.message_page(tender["id"], limit=1, cursor=page["next_cursor"])
    assert page2["items"][0]["result_links"][0]["id"] == finding["id"]
    assert page2["items"][0]["result_links"][0]["kind"] == "finding"
    assert page2["items"][0]["result_links"][0]["target"] == (
        f"/tenders/{tender['id']}/work?view=decisions&record={finding['id']}"
    )


def test_history_initial_page_is_latest_dialogue_and_older_cursor_survives_append(tmp_path):
    repo = Repository(tmp_path)
    tender = repo.create_tender("Long dialogue")
    saved = [repo.add_message(tender["id"], "manager", f"Reply {index}") for index in range(120)]
    for index in range(60):
        repo.add_message(tender["id"], "system", f"Import activity {index}")
    page = repo.message_page(tender["id"], limit=50)
    assert [item["id"] for item in page["items"]] == [item["id"] for item in saved[-50:]]
    latest = repo.add_message(tender["id"], "manager", "Newest reply")
    refreshed = repo.message_page(tender["id"], limit=50)
    assert refreshed["items"][-1]["id"] == latest["id"]
    older = repo.message_page(tender["id"], limit=50, cursor=page["next_cursor"])
    assert [item["id"] for item in older["items"]] == [item["id"] for item in saved[20:70]]
    oldest = repo.message_page(tender["id"], limit=50, cursor=older["next_cursor"])
    assert [item["id"] for item in oldest["items"]] == [item["id"] for item in saved[:20]]
    assert oldest["next_cursor"] is None
    other = repo.create_tender("Other Tender")
    with pytest.raises(ValueError, match="cursor"):
        repo.message_page(other["id"], limit=50, cursor=page["next_cursor"])


def test_health_exposes_workspace_revision(tmp_path):
    app = create_app(tmp_path, "test-session-token")
    with TestClient(app) as client:
        client.headers["Authorization"] = "Bearer test-session-token"
        response = client.get("/api/health")
    assert response.status_code == 200
    assert response.json()["workspace_revision"] == 2


def test_http_busy_message_is_one_editable_pending_instruction(tmp_path):
    app = create_app(tmp_path, "test-session-token")
    with TestClient(app) as client:
        client.headers["Authorization"] = "Bearer test-session-token"
        tender = client.post("/api/tenders", json={"name": "Busy"}).json()
        run = app.state.repo.create_run(tender["id"], "import")
        endpoint = f"/api/tenders/{tender['id']}/messages"
        first = client.post(endpoint, json={"content": "hello", "idempotency_key": "req-1"})
        assert first.status_code == 200
        assert first.json()["outcome"] == "pending"
        assert first.json()["pending"]["wait_for_run_ids"] == [run["id"]]
        conflict = client.post(
            endpoint, json={"content": "another", "idempotency_key": "req-2"}
        )
        assert conflict.status_code == 409
        edited = client.patch(
            f"/api/tenders/{tender['id']}/pending-message",
            json={
                "content": "hello again",
                "pending_id": first.json()["pending"]["id"],
                "expected_revision": first.json()["pending"]["revision"],
            },
        )
        assert edited.status_code == 200
        assert edited.json()["content"] == "hello again"
        assert client.request(
            "DELETE",
            f"/api/tenders/{tender['id']}/pending-message",
            json={"pending_id": edited.json()["id"], "expected_revision": edited.json()["revision"]},
        ).json() == {"ok": True}
        assert client.get(f"/api/tenders/{tender['id']}/pending-message").json() is None


@pytest.mark.asyncio
async def test_a_message_runs_the_manager_directly_and_publishes_its_answer(tmp_path, monkeypatch):
    repo = Repository(tmp_path)
    tender = repo.create_tender("Direct")
    run = repo.create_run(tender["id"], "manager", "hello")
    jobs = JobManager(repo, object())

    async def manager(*_args):
        return PreparedOfficeResult(
            tender_id=tender["id"],
            run_id=run["id"],
            output=OfficeOutput(summary="Hello. Add the tender documents to start."),
            usage={"requests": 1},
            source_ids_read=(),
            web_sources=(),
        )

    monkeypatch.setattr("quantix.office.run_manager", manager)
    jobs._schedule(run)
    await asyncio.gather(*list(jobs.tasks.values()))
    saved = repo.get_run(run["id"])
    assert saved["status"] == "completed", (saved["error"], repo.run_events(run["id"]))
    assert [item["content"] for item in repo.messages(tender["id"])] == ["Hello. Add the tender documents to start."]
    assert repo.list_findings(tender["id"]) == [] and repo.list_plans(tender["id"]) == []
    await jobs.close()


@pytest.mark.asyncio
async def test_manager_failure_holds_waiting_draft_on_same_run(tmp_path, monkeypatch):
    repo = Repository(tmp_path)
    tender = repo.create_tender("Manager failure")
    run = repo.create_run(tender["id"], "manager", "check the concrete")
    jobs = JobManager(repo, object())
    pending = jobs.pending.upsert(tender["id"], "hello later", "req-later", [run["id"]])

    async def failed_manager(*_args):
        raise ValueError("The selected model failed before publication.")

    monkeypatch.setattr("quantix.office.run_manager", failed_manager)
    jobs._schedule(run)
    await asyncio.gather(*list(jobs.tasks.values()))
    assert repo.get_run(run["id"])["status"] == "failed"
    held = jobs.pending.get(tender["id"])
    assert held["id"] == pending["id"]
    assert held["status"] == "held"
    assert held["hold_reason"] == "failed"
    assert len(repo.list_runs(tender["id"])) == 1
    await jobs.close()


@pytest.mark.asyncio
async def test_stop_or_failed_wait_holds_pending_until_explicit_confirmation(tmp_path):
    repo = Repository(tmp_path)
    tender = repo.create_tender("Queue")
    run = repo.create_run(tender["id"], "manager")
    service = PendingInstructionService(repo)
    pending = service.upsert(tender["id"], "Review", "request-1", [run["id"]])
    service.on_run_finished(run["id"], "failed")
    held = service.get(tender["id"])
    assert held["id"] == pending["id"]
    assert held["status"] == "held"
    assert held["hold_reason"] == "failed"
    service.release(tender["id"])
    assert service.get(tender["id"])["status"] == "pending"


def test_earlier_failed_run_keeps_pending_held_after_later_run_succeeds(tmp_path):
    repo = Repository(tmp_path)
    tender = repo.create_tender("Batch")
    first = repo.create_run(tender["id"], "manager")
    second = repo.create_run(tender["id"], "manager")
    service = PendingInstructionService(repo)
    service.upsert(tender["id"], "Review", "request-1", [first["id"], second["id"]])
    repo.update_run(second["id"], status="running")
    repo.update_run(second["id"], status="completed", progress=100)
    service.on_run_finished(second["id"], "completed")
    assert service.get(tender["id"])["status"] == "pending"
    repo.update_run(first["id"], status="failed", detail="Work needs attention.")
    service.on_run_finished(first["id"], "failed")
    assert service.get(tender["id"])["hold_reason"] == "failed"


def test_startup_holds_every_pending_instruction_for_explicit_review(tmp_path):
    repo = Repository(tmp_path)
    tender = repo.create_tender("Restart")
    service = PendingInstructionService(repo)
    service.upsert(tender["id"], "Review", "request-1", [])
    repo.recover_interrupted_runs()
    pending = service.get(tender["id"])
    assert pending["status"] == "held"
    assert pending["hold_reason"] == "interrupted"


@pytest.mark.parametrize("status", ["failed", "cancelled", "interrupted"])
def test_intervening_work_failure_holds_pending_after_original_wait_succeeds(tmp_path, status):
    repo = Repository(tmp_path)
    tender = repo.create_tender("Intervening work")
    original = repo.create_run(tender["id"], "manager")
    service = PendingInstructionService(repo)
    service.upsert(tender["id"], "Review", "request-1", [original["id"]])
    later = repo.create_run(tender["id"], "manager")
    repo.update_run(later["id"], status=status)
    service.on_run_finished(later["id"], status)
    repo.update_run(original["id"], status="completed")
    service.on_run_finished(original["id"], "completed")
    assert service.get(tender["id"])["hold_reason"] == status
    assert service.get(tender["id"])["status"] == "held"


def test_pending_preflight_failure_keeps_draft_and_does_not_create_message(tmp_path, monkeypatch):
    repo = Repository(tmp_path)
    tender = repo.create_tender("Preflight")
    service = PendingInstructionService(repo)
    pending = service.upsert(tender["id"], "Review", "request-1", [])
    jobs = JobManager(repo, object())

    def blocked(*_args, **_kwargs):
        raise ValueError("budget needs review")

    monkeypatch.setattr(jobs, "_preflight_route", blocked)
    with pytest.raises(ValueError, match="budget"):
        jobs.consume_pending(
            tender["id"],
            pending_id=pending["id"],
            expected_revision=pending["revision"],
        )
    assert service.get(tender["id"])["id"] == pending["id"]
    assert repo.messages(tender["id"]) == []
    assert repo.list_runs(tender["id"]) == []


def test_stale_pending_revision_cannot_confirm_newer_edit(tmp_path):
    repo = Repository(tmp_path)
    tender = repo.create_tender("Stale")
    service = PendingInstructionService(repo)
    pending = service.upsert(tender["id"], "Review", "request-1", [])
    edited = service.edit(
        tender["id"],
        "Review latest",
        pending_id=pending["id"],
        expected_revision=pending["revision"],
    )
    with pytest.raises(ValueError, match="changed"):
        service.consume(
            tender["id"], pending_id=pending["id"], expected_revision=pending["revision"]
        )
    assert service.get(tender["id"])["revision"] == edited["revision"]


def test_old_pending_idempotency_receipt_cannot_return_newer_pending_draft(tmp_path):
    repo = Repository(tmp_path)
    tender = repo.create_tender("Receipt")
    jobs = JobManager(repo, object())
    repo.create_run(tender["id"], "import")
    first = jobs.submit_message(
        tender["id"], "first", idempotency_key="request-old"
    )["pending"]
    jobs.cancel_pending(
        tender["id"], pending_id=first["id"], expected_revision=first["revision"]
    )
    second = jobs.submit_message(
        tender["id"], "second", idempotency_key="request-new"
    )["pending"]
    assert second["id"] != first["id"]
    with pytest.raises(ValueError, match="cancelled or replaced"):
        jobs.submit_message(tender["id"], "first", idempotency_key="request-old")
