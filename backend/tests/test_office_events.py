"""Durable, Tender-scoped live-office event storage."""

import math
from concurrent.futures import ThreadPoolExecutor

import pytest

from quantix import diagnostics
from quantix.office_event_models import OfficeEventPage
from quantix.office_events import OFFICE_EVENT_TYPES, OfficeEventService
from quantix.repository import Repository


def _service(tmp_path):
    repo = Repository(tmp_path)
    tender = repo.create_tender("Synthetic office")
    return repo, tender["id"], OfficeEventService(repo)


@pytest.fixture(autouse=True)
def close_temporary_diagnostics_writer(tmp_path):
    """Release each Repository's temp-home lock without creating a default writer."""

    yield
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


def test_fresh_service_has_no_fake_events_and_latest_cursor_is_empty(tmp_path):
    repo, tender_id, service = _service(tmp_path)

    page = service.page(tender_id)

    assert isinstance(page, OfficeEventPage)
    assert page.items == []
    assert page.cursor is None
    assert not page.has_more
    assert not page.reset_required
    assert service.latest_cursor(tender_id) is None


def test_append_assigns_tender_sequence_and_preserves_event_data(tmp_path):
    _, tender_id, service = _service(tmp_path)

    first = service.append(
        tender_id,
        "staff_created",
        actor_id="generated-manager",
        assignment_id="assignment-1",
        record_ref={"staff_id": "staff-1", "role": "Package reviewer"},
        payload={"reason": "The tender needs a package review."},
    )
    waiting = service.append(tender_id, "assignment_waiting", assignment_id="assignment-1")
    second = service.append(tender_id, "assignment_started", assignment_id="assignment-1")

    assert first.event_id and len(first.event_id) == 32
    assert first.sequence == 1
    assert first.tender_id == tender_id
    assert first.event_type == "staff_created"
    assert first.actor_id == "generated-manager"
    assert first.assignment_id == "assignment-1"
    assert first.record_ref == {"staff_id": "staff-1", "role": "Package reviewer"}
    assert first.payload == {"reason": "The tender needs a package review."}
    assert first.occurred_at.endswith("+00:00")
    assert waiting.sequence == 2
    assert waiting.event_type == "assignment_waiting"
    assert second.sequence == 3
    assert second.event_type in OFFICE_EVENT_TYPES
    assert service.latest_cursor(tender_id) == second.event_id


def test_events_survive_repository_reopen(tmp_path):
    repo, tender_id, service = _service(tmp_path)
    event = service.append(tender_id, "assignment_completed", payload={"result": "draft"})

    reopened = OfficeEventService(Repository(tmp_path))

    assert reopened.page(tender_id).items == [event]
    assert reopened.latest_cursor(tender_id) == event.event_id


def test_idempotent_replay_returns_same_event_and_changed_data_conflicts(tmp_path):
    _, tender_id, service = _service(tmp_path)
    event = service.append(
        tender_id,
        "message_posted",
        actor_id="manager",
        payload={"text": "Review the drawing package."},
        idempotency_key="message-1",
    )

    replay = service.append(
        tender_id,
        "message_posted",
        actor_id="manager",
        payload={"text": "Review the drawing package."},
        idempotency_key="message-1",
    )
    assert replay == event

    with pytest.raises(ValueError, match="idempotency"):
        service.append(
            tender_id,
            "message_posted",
            actor_id="manager",
            payload={"text": "Send the package."},
            idempotency_key="message-1",
        )

    assert service.page(tender_id).items == [event]


def test_page_cursor_is_stable_and_is_scoped_to_tender(tmp_path):
    repo = Repository(tmp_path)
    tender_a = repo.create_tender("Synthetic A")["id"]
    tender_b = repo.create_tender("Synthetic B")["id"]
    service = OfficeEventService(repo)
    events = [service.append(tender_a, "assignment_queued") for _ in range(3)]

    first = service.page(tender_a, limit=2)
    second = service.page(tender_a, after=first.cursor, limit=2)
    repeated = service.page(tender_a, after=first.cursor, limit=2)

    assert [event.event_id for event in first.items] == [events[0].event_id, events[1].event_id]
    assert first.cursor == events[1].event_id
    assert first.has_more
    assert not first.reset_required
    assert [event.event_id for event in second.items] == [events[2].event_id]
    assert second.cursor == events[2].event_id
    assert not second.has_more
    assert repeated == second

    foreign = service.append(tender_b, "assignment_queued")
    with pytest.raises(ValueError, match="Tender"):
        service.page(tender_a, after=foreign.event_id)


def test_replay_preserves_json_types_but_ignores_object_key_order(tmp_path):
    _, tender_id, service = _service(tmp_path)
    event = service.append(tender_id, "message_posted", payload={"value": True, "note": "same"},
                           idempotency_key="typed-message")
    assert service.append(tender_id, "message_posted", payload={"note": "same", "value": True},
                          idempotency_key="typed-message") == event
    with pytest.raises(ValueError, match="idempotency"):
        service.append(tender_id, "message_posted", payload={"value": 1, "note": "same"},
                       idempotency_key="typed-message")
    assert service.page(tender_id).items == [event]


def test_unknown_cursor_requests_snapshot_reset_at_current_cursor(tmp_path):
    _, tender_id, service = _service(tmp_path)
    event = service.append(tender_id, "scope_changed")

    page = service.page(tender_id, after="expired-event-id")

    assert page.items == []
    assert page.cursor == event.event_id
    assert not page.has_more
    assert page.reset_required


def test_outer_transaction_rolls_back_event_and_schema_writes(tmp_path):
    repo = Repository(tmp_path)
    tender_id = repo.create_tender("Synthetic rollback")["id"]

    with pytest.raises(RuntimeError):
        with repo.atomic():
            OfficeEventService(repo).append(tender_id, "assignment_queued")
            raise RuntimeError("rollback")

    # A fresh service recreates any rolled-back schema and confirms no event leaked.
    service = OfficeEventService(repo)
    assert service.page(tender_id).items == []


def test_concurrent_appends_have_unique_contiguous_sequences(tmp_path):
    repo = Repository(tmp_path)
    tender_id = repo.create_tender("Synthetic concurrent")["id"]

    def append(index):
        return OfficeEventService(repo).append(
            tender_id,
            "assignment_completed",
            assignment_id=f"assignment-{index}",
        )

    with ThreadPoolExecutor(max_workers=8) as workers:
        events = list(workers.map(append, range(24)))

    assert sorted(event.sequence for event in events) == list(range(1, 25))
    page = OfficeEventService(repo).page(tender_id, limit=200)
    assert [event.sequence for event in page.items] == list(range(1, 25))


def test_append_validates_kind_strings_payload_and_tender_scope(tmp_path):
    repo, tender_id, service = _service(tmp_path)

    with pytest.raises(ValueError, match="event type"):
        service.append(tender_id, "invented_event")
    with pytest.raises(ValueError, match="event type"):
        service.append(tender_id, "   ")
    with pytest.raises(ValueError, match="payload"):
        service.append(tender_id, "message_posted", payload={"bad": object()})
    with pytest.raises(KeyError, match="Tender"):
        service.append("missing-tender", "assignment_queued")
    with pytest.raises(ValueError, match="limit"):
        service.page(tender_id, limit=0)
    with pytest.raises(ValueError, match="limit"):
        service.page(tender_id, limit=201)


@pytest.mark.parametrize("value", [math.nan, math.inf, -math.inf])
def test_append_rejects_nonfinite_json_values(tmp_path, value):
    _, tender_id, service = _service(tmp_path)

    with pytest.raises(ValueError, match="serialized"):
        service.append(tender_id, "message_posted", payload={"value": value})
    with pytest.raises(ValueError, match="serialized"):
        service.append(tender_id, "message_posted", record_ref={"value": value})
