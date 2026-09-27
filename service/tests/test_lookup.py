"""The office looks up its own work, and answers the engineer from it: with the records and pages it opened, or with
the work still to do as its own tasks, never with a promise to look."""

import io
import re
import threading
from types import SimpleNamespace

import test_estimate
from PIL import Image
from pydantic_ai import ToolReturn
from pydantic_ai.messages import ModelResponse, RetryPromptPart, TextPart, ToolCallPart, ToolReturnPart
from test_documents import read_all, upload
from test_office import scripted, wait_for

from quantix import settings
from quantix.boq import records as boq
from quantix.documents import library
from quantix.estimate import records as estimate
from quantix.office import records as office
from quantix.office import tools
from quantix.office.models import Staff, Task
from quantix.review import lookup
from quantix.review import records as reviews

tender = test_estimate.tender  # the fixture: a priced school with an estimator
BUILD_UP = test_estimate.BUILD_UP
QUESTION = "How did you price the slab reinforcement?"
LINES = [estimate.LineIn(**line) for line in BUILD_UP]


def sent_back(messages) -> list[str]:
    return [str(p.content) for m in messages for p in m.parts if isinstance(p, RetryPromptPart)]


def returned(messages) -> list[str]:
    return [str(p.content) for m in messages for p in m.parts if isinstance(p, ToolReturnPart)]


def call(tool: str, **args) -> ModelResponse:
    return ModelResponse(parts=[ToolCallPart(tool, args)])


DONE = ModelResponse(parts=[TextPart("That's all.")])


def priced_after_a_send_back(client, tender_id, priya_id) -> tuple[str, str]:
    """Priya priced 4.3, Rania sent it back, Priya priced it again, Rania accepted and the engineer approved."""
    with client.app.state.sessions() as session:
        rania = office.hire(session, tender_id, "Rania Farouk", "Tender Manager", {}, is_manager=True)
        first = estimate.propose_rate(
            session,
            tender_id,
            priya_id,
            "4.3",
            "estimate",
            "Rebar only, at the supplier price per tonne.",
            unit_rate=2300,
        )
        [p] = reviews.pending(session, tender_id)
        verdict = reviews.Verdict(record=p.ref, accept=False, note="Add the fixing labour and the tie wire.")
        reviews.review(session, client.app.state.home, tender_id, rania, [verdict], autonomous=False)
        second = estimate.propose_rate(
            session,
            tender_id,
            priya_id,
            "4.3",
            "estimate",
            "Fixed by a gang of four, rebar with 5% wastage.",
            lines=LINES,
        )
        [p] = reviews.pending(session, tender_id)
        reason = "The build-up covers labour, rebar with wastage, wire and plant."
        verdict = reviews.Verdict(record=p.ref, accept=True, note="Checked each line.", warnings_reason=reason)
        reviews.review(session, client.app.state.home, tender_id, rania, [verdict], autonomous=False)
        estimate.decide(session, second, True)
        session.commit()
        return rania.id, second.id, first.id


def test_the_manager_answers_how_a_line_was_priced_from_its_record(client, tender, tmp_path):
    tender_id, priya_id, _ = tender
    rania_id, rate_id, first_id = priced_after_a_send_back(client, tender_id, priya_id)
    settings.save(tmp_path, office_ai={"connection_id": "scripted", "model": "brain"})
    seen: dict[str, list[str]] = {}

    def brain(messages, info):
        if "You are Rania Farouk" not in info.instructions:
            return DONE
        done, refused = returned(messages), sent_back(messages)
        seen["done"], seen["refused"] = done, refused
        steps = [
            call("message_engineer", text="I am checking the rebar rate now and will come back to you."),
            call("find_records", words="slab reinforcement"),
            lambda: call("open_record", references=[re.search(r"rate \w{8}", done[-1]).group(0)]),
            lambda: call(
                "message_engineer",
                text="Built up from a fixing gang, rebar with 5% wastage, tie wire and a bending machine.",
                sources=[f"rate {rate_id[:8]}"],
            ),
        ]
        step = len(done) + len(refused)
        if step >= len(steps):
            return DONE
        return steps[step]() if callable(steps[step]) else steps[step]

    client.app.state.office.model = lambda: scripted(brain)
    client.post(f"/tenders/{tender_id}/messages", json={"channel": rania_id, "text": QUESTION})
    chat = f"/tenders/{tender_id}/messages?channel={rania_id}"
    wait_for(lambda o: len(client.get(chat).json()) == 2, client, tender_id)

    # A reply with nothing behind it is sent back: answer now, or list the work as next steps
    assert seen["refused"][0].startswith("The engineer asked you something: answer it now.")
    found, opened = seen["done"][0], seen["done"][1]
    assert f"rate {rate_id[:8]}: 3488.00 per t, approved" in found
    # The whole story of the rate: its build-up and Quantix's figures, who made and decided it, and the earlier try
    assert "- labour: Steel fixer gang, 16 hr at 62 = 992.00" in opened
    assert "Quantix: 3488.00 per t; 28.1 t at this rate is 98,012.80." in opened
    assert "Made by Priya on" in opened and "Rania accepted it: Checked each line." in opened
    assert "The engineer approved it on" in opened
    assert f"- rate {first_id[:8]}" in opened and "sent back: Add the fixing labour and the tie wire." in opened

    answer = client.get(chat).json()[-1]
    assert answer["text"].startswith("Built up from a fixing gang")
    with client.app.state.sessions() as session:
        item_id = boq.find_item(session, tender_id, "4.3").id
    assert answer["sources"] == [
        {"label": "The rate for BOQ item 4.3", "document_id": None, "page": None, "boq_item_id": item_id, "url": None}
    ]


def test_only_what_someone_opened_can_be_given_as_a_source(client, tender, tmp_path):
    tender_id, priya_id, _ = tender
    rania_id, rate_id, _ = priced_after_a_send_back(client, tender_id, priya_id)
    settings.save(tmp_path, office_ai={"connection_id": "scripted", "model": "brain"})
    seen: dict[str, list[str]] = {}
    with client.app.state.sessions() as session:
        bill = next(d.id for d in library.documents(session, tender_id) if d.name == "Bill.xlsx")

    def brain(messages, info):
        if "You are Rania Farouk" not in info.instructions:
            return DONE
        done, refused = returned(messages), sent_back(messages)
        seen["refused"] = refused
        answer = dict(text="4.3 is 28.1 t in the client's bill.", sources=["Bill.xlsx, page 1"])
        steps = [
            call("message_engineer", **answer),
            call("message_engineer", text="The rate is approved.", sources=[f"rate {rate_id[:8]}"]),
            call("read_page", document_id=bill, page=1),
            call("message_engineer", **answer),
        ]
        step = len(done) + len(refused)
        return steps[step] if step < len(steps) else DONE

    client.app.state.office.model = lambda: scripted(brain)
    client.post(f"/tenders/{tender_id}/messages", json={"channel": rania_id, "text": "How much rebar is there?"})
    chat = f"/tenders/{tender_id}/messages?channel={rania_id}"
    wait_for(lambda o: len(client.get(chat).json()) == 2, client, tender_id)

    assert seen["refused"] == [
        "You haven't read Bill.xlsx, page 1. Read it with read_page first.",
        f"You haven't opened rate {rate_id[:8]}. Open it with open_record first.",
    ]
    answer = client.get(chat).json()[-1]
    assert answer["sources"] == [
        {"label": "Bill.xlsx, page 1", "document_id": bill, "page": 1, "boq_item_id": None, "url": None}
    ]


def test_work_promised_to_the_engineer_becomes_a_task_that_wakes_its_owner(client, tender, tmp_path):
    tender_id, priya_id, _ = tender
    rania_id, _, _ = priced_after_a_send_back(client, tender_id, priya_id)
    settings.save(tmp_path, office_ai={"connection_id": "scripted", "model": "brain"})
    turns: list[str] = []

    def brain(messages, info):
        if "You are Rania Farouk" not in info.instructions:
            return DONE
        prompt = next(p.content for m in messages for p in m.parts if p.part_kind == "user-prompt")
        done = returned(messages)
        if not done:
            turns.append(prompt)
        follow_up = re.search(r"- (\w+): Follow up: compare the rebar rate with the quote", prompt)
        if follow_up is None:
            steps = [
                call(
                    "message_engineer",
                    text="The rate is approved; I'll compare it with the steel quote.",
                    next_steps=["compare the rebar rate with the quote"],
                )
            ]
        else:
            steps = [call("complete_task", task_id=follow_up.group(1), result="The quote is 2,300 per t.")]
        return steps[len(done)] if len(done) < len(steps) else DONE

    client.app.state.office.model = lambda: scripted(brain)
    client.post(f"/tenders/{tender_id}/messages", json={"channel": rania_id, "text": "Is the rebar rate right?"})

    def finished(_):
        with client.app.state.sessions() as session:
            return [(t.title, t.status) for t in office.all_tasks(session, tender_id) if t.staff_id == rania_id]

    wait_for(lambda o: finished(o) == [("Follow up: compare the rebar rate with the quote", "done")], client, tender_id)
    assert len(turns) == 2  # woken once more, by the task alone, with nothing new from anyone
    assert "Follow up: compare the rebar rate with the quote" in turns[1]
    with client.app.state.sessions() as session:
        [task] = [t for t in office.all_tasks(session, tender_id) if t.staff_id == rania_id]
        assert task.result == "The quote is 2,300 per t."
        assert not office.new_task(session, session.get(Staff, rania_id))


def test_find_records_and_the_priced_boq(client, tender):
    tender_id, priya_id, _ = tender
    _, rate_id, _ = priced_after_a_send_back(client, tender_id, priya_id)
    with client.app.state.sessions() as session:
        [line] = lookup.search(session, tender_id, "reinforcement")
        assert line.startswith("boq ") and f"rate {rate_id[:8]}: 3488.00 per t, approved" in line
        assert lookup.search(session, tender_id, "", "fact")[0].startswith("fact ")
        assert lookup.search(session, tender_id, "nothing like this") == []

        text = lookup.priced(session, tender_id)
        head, *rows, foot = text.splitlines()
        assert head.startswith("3 lines, 1 priced, together 98,012.80 SAR (Quantix's figures).")
        assert rows[1] == (
            f"4.3 | Slab reinforcement | 28.1 t | 3488.00 | 98,012.80 | estimate | approved | rate {rate_id[:8]}"
        )
        assert rows[0] == "3.1 | Excavation | 1,240 m3 | not priced"
        assert foot == "Lines 1 to 3 of 3."
        assert lookup.priced(session, tender_id, start=3).splitlines()[1].startswith("6.3 | Waterproofing")


def fake_turn(client, tender_id, staff_id) -> SimpleNamespace:
    """What a tool sees of the turn it is called in."""
    state = client.app.state
    turn = tools.Turn(state.home, state.sessions, tender_id, staff_id, False, threading.Event())
    return SimpleNamespace(deps=turn)


def test_lines_past_forty_are_named_so_they_get_entered(client, tender):
    tender_id, priya_id, _ = tender
    with client.app.state.sessions() as session:
        bill = next(d.id for d in library.documents(session, tender_id) if d.name == "Bill.xlsx")
    ctx = fake_turn(client, tender_id, priya_id)
    tools.read_page(ctx, bill, 1)
    lines = [
        boq.ItemIn(item=f"9.{n}", description="Line", unit="m", quantity=1, document_id=bill, page=1, quote="q")
        for n in range(42)
    ]
    report = tools.propose_boq_items(ctx, lines)
    assert report.endswith("Not entered, because only 40 go at a time: 9.40, 9.41. Enter them in another call.")


def test_an_image_document_can_be_looked_at(client):
    tender_id = client.post("/tenders", json={"name": "Photos"}).json()["id"]
    picture = Image.new("RGB", (400, 200), "white")
    buffer = io.BytesIO()
    picture.save(buffer, format="PNG")
    upload(client, tender_id, {"Site photo.png": buffer.getvalue()})
    photo = read_all(client, tender_id)["Site photo.png"]["id"]
    with client.app.state.sessions() as session:
        omar = office.hire(session, tender_id, "Omar Haddad", "Quantity Surveyor", {})
        session.commit()
        omar_id = omar.id

    whole = tools.view_page(fake_turn(client, tender_id, omar_id), photo, 1)
    close = tools.view_page(fake_turn(client, tender_id, omar_id), photo, 1, region=[0, 0, 800, 400])
    assert isinstance(whole, ToolReturn) and whole.return_value == "The image Site photo.png follows."
    assert Image.open(io.BytesIO(whole.content[0].data)).size == (tools.VIEW_WIDTH, 800)
    assert close.return_value.startswith("A close-up of Site photo.png")
    with client.app.state.sessions() as session:
        assert office.has_opened(session, omar_id, "page", f"{photo}:1")
        assert session.query(Task).count() == 0
