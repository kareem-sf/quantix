"""The office looks up its own work, and answers the engineer from it: with the records and pages it opened, or with
the work still to do as its own tasks, never with a promise to look."""

import io
import re
import threading
from decimal import Decimal
from pathlib import Path
from types import SimpleNamespace

import pytest
import test_estimate
from PIL import Image
from pydantic_ai import ModelRetry, ToolReturn
from pydantic_ai.messages import ModelResponse, RetryPromptPart, TextPart, ToolCallPart, ToolReturnPart
from test_documents import read_all, upload
from test_office import scripted, wait_for

from quantix import settings
from quantix.boq import records as boq
from quantix.documents import library
from quantix.estimate import records as estimate
from quantix.office import agents, tools
from quantix.office import records as office
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
        priya = session.get(Staff, priya_id)
        [redo] = office.open_tasks(session, priya)
        office.complete(session, priya, redo.id, "Priced again with the fixing labour and the tie wire.")
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

        # the real tender: Salem searched for "markup" and "site support allowance" four times and found nothing
        assert lookup.search(session, tender_id, "markup") == []
        site = estimate.PreliminaryIn(
            item="Site support allowance", quantity=Decimal(4), unit="month", rate=Decimal(9000)
        )
        zero, six = Decimal(0), Decimal("0.06")
        filed = estimate.propose_markups(session, tender_id, priya_id, [site], six, zero, zero, "Four months of staff.")
        [markups] = lookup.search(session, tender_id, "site support allowance")
        assert markups.startswith("markups · preliminaries priced item by item (1 items: Site support allowance)")
        assert "overheads 6.0% and profit 0.0% of cost, adjustment 0.00 as a lump sum" in markups
        assert lookup.search(session, tender_id, "markup") == [markups]
        assert lookup.search(session, tender_id, "", "markups") == [markups]
        # the real tender again: every set was sent back, so the office saw "none proposed yet" and nothing to open
        filed.status = "rejected"
        [returned] = lookup.search(session, tender_id, "markup")
        assert returned.endswith("· sent back") and lookup.find(session, tender_id, "markups") == ("markups", filed)
        assert lookup.find(session, tender_id, returned) == ("markups", filed)  # the whole line, as Salem cited it
        assert agents.standing(session, tender_id).count(
            "Markups: the last set was sent back, and none proposed since."
        )

        text = lookup.priced(session, tender_id)
        head, *rows, foot = text.splitlines()
        assert head.startswith("3 lines, 1 priced, together 98,012.80 SAR (Quantix's figures).")
        assert rows[1] == (
            f"4.3 | Slab reinforcement | 28.1 t | 3488.00 | 98,012.80 | estimate | approved | rate {rate_id[:8]}"
        )
        assert rows[0] == "3.1 | Excavation | 1,240 m3 | not priced"
        assert foot == "Lines 1 to 3 of 3."
        assert lookup.priced(session, tender_id, start=3).splitlines()[1].startswith("6.3 | Waterproofing")


def test_every_summary_the_office_looks_at_can_be_given_as_a_source(client, tender):
    """A summary tool records that the person looked at it, so they can rest what they tell the engineer on it: the
    Manager's update resting on what_changed was refused until every one of them was listed."""
    source = Path(tools.__file__).read_text(encoding="utf-8")
    recorded = set(re.findall(r'_opened\(ctx, session, "summary", "(\w+)"\)', source))
    assert {"what_changed", "price_breakdown", "estimate_summary"} <= recorded
    tender_id, priya_id, _ = tender
    with client.app.state.sessions() as session:
        for key in sorted(recorded):
            office.note_opened(session, tender_id, priya_id, "summary", key)
            assert lookup.cited(session, tender_id, priya_id, key) == {"label": lookup.SUMMARIES[key]}


def fake_turn(client, tender_id, staff_id) -> SimpleNamespace:
    """What a tool sees of the turn it is called in."""
    state = client.app.state
    turn = tools.Turn(state.home, state.sessions, tender_id, staff_id, False, threading.Event())
    return SimpleNamespace(deps=turn, tool_call_id="call")


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


def test_an_answer_stays_and_promises_nothing_more(client, tender):
    tender_id, priya_id, _ = tender
    rania_id, rate_id, _ = priced_after_a_send_back(client, tender_id, priya_id)
    client.post(f"/tenders/{tender_id}/messages", json={"channel": rania_id, "text": QUESTION})
    ctx = fake_turn(client, tender_id, rania_id)
    tools.open_record(ctx, [f"rate {rate_id[:8]}"])
    with pytest.raises(ModelRetry, match="Send this without next_steps"):
        tools.message_engineer(ctx, "Built up.", sources=[f"rate {rate_id[:8]}"], next_steps=["run the audit"])
    assert (
        tools.message_engineer(ctx, "Built up from a gang, rebar and wire.", sources=[f"rate {rate_id[:8]}"]) == "Sent."
    )
    assert tools.message_engineer(ctx, "The audit is clear.") == "Sent."  # a later update doesn't replace the answer
    chat = client.get(f"/tenders/{tender_id}/messages", params={"channel": rania_id}).json()
    assert [m["text"] for m in chat] == [QUESTION, "Built up from a gang, rebar and wire.", "The audit is clear."]


def test_a_question_gets_one_set_of_next_steps_then_an_answer(client, tender):
    tender_id, priya_id, _ = tender
    rania_id, rate_id, _ = priced_after_a_send_back(client, tender_id, priya_id)
    client.post(f"/tenders/{tender_id}/messages", json={"channel": rania_id, "text": QUESTION})
    ctx = fake_turn(client, tender_id, rania_id)
    promise = tools.message_engineer(ctx, "Checking the build-up.", next_steps=["open the rate record"])
    assert promise == "Sent. 1 next step is now your task."
    for again in ({"next_steps": ["check again"]}, {}):
        with pytest.raises(ModelRetry, match="You already set next steps for the engineer's question: answer it now"):
            tools.message_engineer(ctx, "Still checking.", **again)
    tools.open_record(ctx, [f"rate {rate_id[:8]}"])
    assert tools.message_engineer(ctx, "Built up from a gang, rebar and wire.", sources=[f"rate {rate_id[:8]}"]) == (
        "Sent. It replaces your last message, which the engineer hadn't answered."
    )


def test_a_refused_answer_names_what_it_may_rest_on(client, tender, tmp_path):
    """On the real tender the Manager opened the rate, answered without sources three times and gave up: the refusal
    now names what he opened since the question, as sources are written."""
    tender_id, priya_id, _ = tender
    rania_id, rate_id, _ = priced_after_a_send_back(client, tender_id, priya_id)
    settings.save(tmp_path, office_ai={"connection_id": "scripted", "model": "brain"})
    seen: dict[str, list[str]] = {}

    def brain(messages, info):
        if "You are Rania Farouk" not in info.instructions:
            return DONE
        done, refused = returned(messages), sent_back(messages)
        seen["refused"] = refused
        steps = [
            call("open_record", references=[f"rate {rate_id[:8]}"]),
            call("message_engineer", text="Built up from a fixing gang and rebar with wastage."),
            call("message_engineer", text="Built up from a fixing gang.", sources=[f"rate {rate_id[:8]}"]),
        ]
        step = len(done) + len(refused)
        return steps[step] if step < len(steps) else DONE

    client.app.state.office.model = lambda: scripted(brain)
    client.post(f"/tenders/{tender_id}/messages", json={"channel": rania_id, "text": QUESTION})
    chat = f"/tenders/{tender_id}/messages?channel={rania_id}"
    wait_for(lambda o: len(client.get(chat).json()) == 2, client, tender_id)

    [refusal] = seen["refused"]
    assert refusal.startswith("The engineer asked you something: answer it now.")
    opened = refusal.split("Since the engineer wrote, you opened: ")[1].rstrip(".").split("; ")
    assert f"rate {rate_id[:8]}" in opened and any(o.startswith("boq ") for o in opened)  # the rate and its line
    assert client.get(chat).json()[-1]["text"] == "Built up from a fixing gang."
