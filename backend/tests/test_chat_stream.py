"""Observing a durable run streams public state without executing it again."""

import json

import pytest

from quantix.repository import Repository


def workspace(tmp_path):
    repo = Repository(tmp_path)
    tender = repo.create_tender("Synthetic streaming Tender")
    run = repo.create_run(tender["id"], "conversation", "Read the synthetic BOQ")
    return repo, tender, run


@pytest.mark.asyncio
async def test_completed_run_streams_saved_message_without_replaying_work(tmp_path):
    from quantix.chat_stream import RunChatStream

    repo, tender, run = workspace(tmp_path)
    repo.event(run["id"], "tool_called", "Reading the BOQ.", {"api_key": "private-canary"})
    repo.add_message(tender["id"], "manager", "The reviewed total is 120 m³.", run_id=run["id"])
    repo.update_run(
        run["id"],
        status="completed",
        progress=100,
        result={"summary": "The reviewed total is 120 m³."},
    )
    before = repo.run_events(run["id"])
    stream = RunChatStream(repo)
    first = [chunk async for chunk in stream.chunks(tender["id"], run["id"])]
    second = [chunk async for chunk in stream.chunks(tender["id"], run["id"])]
    assert first == second
    assert first[0]["type"] == "start"
    assert "120 m³" in "".join(item.get("delta", "") for item in first)
    assert first[-1]["type"] == "finish"
    assert "private-canary" not in json.dumps(first)
    assert repo.run_events(run["id"]) == before
    assert len(repo.list_runs(tender["id"])) == 1


@pytest.mark.asyncio
async def test_stream_rejects_another_tenders_run(tmp_path):
    from quantix.chat_stream import RunChatStream

    repo, _tender, run = workspace(tmp_path)
    other = repo.create_tender("Other synthetic Tender")
    with pytest.raises(KeyError):
        _ = [chunk async for chunk in RunChatStream(repo).chunks(other["id"], run["id"])]


@pytest.mark.asyncio
async def test_observer_disconnect_does_not_cancel_authorized_job(tmp_path):
    from quantix.chat_stream import RunChatStream

    repo, tender, run = workspace(tmp_path)
    chunks = RunChatStream(repo).chunks(tender["id"], run["id"])
    assert (await anext(chunks))["type"] == "start"
    await chunks.aclose()
    assert repo.get_run(run["id"])["status"] == "queued"


@pytest.mark.asyncio
async def test_provider_deltas_remain_drafts_until_saved_message(tmp_path):
    from quantix.chat_stream import RunChatStream

    repo, tender, run = workspace(tmp_path)
    repo.event(
        run["id"],
        "assistant_text_delta",
        "Draft response received.",
        {"text": "An unreviewed number is 900."},
    )
    repo.update_run(run["id"], status="failed", detail="The source check failed.")
    chunks = [chunk async for chunk in RunChatStream(repo).chunks(tender["id"], run["id"])]
    draft = next(item for item in chunks if item["type"] == "data-draft")
    assert draft["data"]["text"] == "An unreviewed number is 900."
    assert not any(item["type"] == "text-delta" for item in chunks)
    assert chunks[-1]["finishReason"] == "error"


@pytest.mark.asyncio
async def test_retry_replaces_abandoned_draft(tmp_path):
    from quantix.chat_stream import RunChatStream

    repo, tender, run = workspace(tmp_path)
    repo.event(
        run["id"],
        "assistant_text_delta",
        "Draft response received.",
        {"text": "Incorrect earlier draft."},
    )
    repo.event(
        run["id"],
        "assistant_text_delta",
        "Draft response received.",
        {"text": "Corrected draft.", "reset": True},
    )
    repo.update_run(run["id"], status="completed")
    chunks = [chunk async for chunk in RunChatStream(repo).chunks(tender["id"], run["id"])]
    drafts = [item for item in chunks if item["type"] == "data-draft"]
    assert drafts[-1]["data"]["text"] == "Corrected draft."


@pytest.mark.asyncio
async def test_staff_drafts_keep_their_own_assignment_identity(tmp_path):
    from quantix.chat_stream import RunChatStream

    repo, tender, run = workspace(tmp_path)
    repo.event(run["id"], "assistant_text_delta", "Manager draft.", {"text": "Manager message."})
    repo.event(
        run["id"],
        "assistant_text_delta",
        "Staff draft.",
        {
            "text": "Staff finding.",
            "assignment_id": "assignment-1",
            "actor_id": "staff-1",
            "reset": True,
        },
    )
    repo.update_run(run["id"], status="completed")
    chunks = [chunk async for chunk in RunChatStream(repo).chunks(tender["id"], run["id"])]
    manager = [item for item in chunks if item["type"] == "data-draft"]
    staff = [item for item in chunks if item["type"] == "data-staff-draft"]
    assert manager[-1]["data"]["text"] == "Manager message."
    assert staff[-1]["data"]["assignment_id"] == "assignment-1"


@pytest.mark.asyncio
async def test_reset_closes_existing_observer_before_database_access(tmp_path, monkeypatch):
    from quantix.chat_stream import RunChatStream

    repo, tender, run = workspace(tmp_path)
    reset_pending = False
    stream = RunChatStream(repo, should_stop=lambda: reset_pending)
    chunks = stream.chunks(tender["id"], run["id"])
    assert (await anext(chunks))["type"] == "start"
    reset_pending = True

    def deleted_database(*args):
        pytest.fail("An observer accessed Tender records after reset began.")

    monkeypatch.setattr(stream, "_snapshot", deleted_database)
    assert [chunk async for chunk in chunks] == []
    monkeypatch.setattr(repo, "get_run", deleted_database)
    assert [chunk async for chunk in stream.chunks(tender["id"], run["id"])] == []


@pytest.mark.asyncio
async def test_activity_stream_keeps_every_event_with_its_own_identity(tmp_path):
    from quantix.chat_stream import RunChatStream
    repo, tender, run = workspace(tmp_path)
    repo.event(run["id"], "tool_started", "First source")
    repo.event(run["id"], "tool_started", "Second source")
    repo.update_run(run["id"], status="completed")
    chunks = [chunk async for chunk in RunChatStream(repo).chunks(tender["id"], run["id"])]
    activity = [chunk for chunk in chunks if chunk["type"] == "data-activity"]
    assert len({item["id"] for item in activity}) == 2
    assert all("created_at" in item["data"] for item in activity)
