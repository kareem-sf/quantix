"""The office looks up its own work, and answers the engineer from it: with the records and pages it opened, or with
the work still to do as its own tasks, never with a promise to look."""

import io
import re
import threading
from datetime import UTC, datetime, timedelta
from decimal import Decimal
from pathlib import Path
from types import SimpleNamespace

import pytest
import test_estimate
import test_subcontract
from PIL import Image
from pydantic_ai import ModelRetry, ToolReturn
from pydantic_ai.messages import ModelResponse, RetryPromptPart, TextPart, ToolCallPart, ToolReturnPart
from test_documents import read_all, upload
from test_office import scripted, wait_for
from test_revisions import PLAN, YARD

from quantix import settings
from quantix.boq import records as boq
from quantix.boq.models import BoqItem
from quantix.documents import library
from quantix.documents.models import WebPage
from quantix.estimate import records as estimate
from quantix.office import agents, tools
from quantix.office import records as office
from quantix.office.models import Staff, Task
from quantix.review import activity, lookup
from quantix.review import records as reviews
from quantix.subcontract import records as subcontract
from quantix.subcontract.models import Package
from quantix.submission import records as submission
from quantix.takeoff import records as takeoff

tender = test_estimate.tender  # the fixture: a priced school with an estimator
subcontracted = test_subcontract.tender  # the fixture: the school's groundworks, with two quotes to record
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
        assert markups.startswith(
            f"markups {filed.id[:8]} · preliminaries priced item by item (1 items: Site support allowance)"
        )
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
        # and the set the engineer sent back last, not a newer duplicate turned down the day before
        filed.decided_at = datetime.now(UTC)
        duplicate = estimate.propose_markups(session, tender_id, priya_id, [site], six, zero, zero, "Again.")
        duplicate.status, duplicate.decided_at = "rejected", filed.decided_at - timedelta(days=1)
        assert lookup.find(session, tender_id, "markups") == ("markups", filed)
        # a redo the Manager sent back after that is dated by his review: it once sent Rashid to the older set
        redo = estimate.propose_markups(session, tender_id, priya_id, [site], six, zero, zero, "72 days.")
        redo.status, redo.reviewed_at = "rejected", filed.decided_at + timedelta(minutes=30)
        assert lookup.find(session, tender_id, "markups") == ("markups", redo)

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


def test_sources_written_as_the_last_line_are_taken_as_the_sources(client, tender):
    """On the real tender Salem ended his answer with "Sources: markups, estimate_summary" instead of giving them,
    was refused, and stopped: the engineer never got the answer."""
    written = "It is both.\n\n- Overheads 6%.\n\n**Sources:** markups, Bill.xlsx, page 2; estimate_summary"
    assert tools._written_sources(written) == (
        "It is both.\n\n- Overheads 6%.",
        ["markups", "Bill.xlsx, page 2", "estimate_summary"],
    )
    assert tools._written_sources("No sources line here.") == ("No sources line here.", [])

    tender_id, priya_id, _ = tender
    rania_id, rate_id, _ = priced_after_a_send_back(client, tender_id, priya_id)
    client.post(f"/tenders/{tender_id}/messages", json={"channel": rania_id, "text": QUESTION})
    ctx = fake_turn(client, tender_id, rania_id)
    tools.open_record(ctx, [f"rate {rate_id[:8]}"])
    assert tools.message_engineer(ctx, f"Built up from a gang.\nSources: rate {rate_id[:8]}").startswith("Sent.")
    answer = client.get(f"/tenders/{tender_id}/messages", params={"channel": rania_id}).json()[-1]
    assert answer["text"] == "Built up from a gang."
    assert [s["label"] for s in answer["sources"]] == ["The rate for BOQ item 4.3"]
    # given and written both, as Salem did next: the line still doesn't show
    tools.message_engineer(ctx, f"Wire is 1%.\n\nSources: rate {rate_id[:8]}", sources=[f"rate {rate_id[:8]}"])
    assert (
        client.get(f"/tenders/{tender_id}/messages", params={"channel": rania_id}).json()[-1]["text"] == "Wire is 1%."
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


def document_id(client, tender_id, name) -> str:
    with client.app.state.sessions() as session:
        return next(d.id for d in library.documents(session, tender_id) if d.name == name)


def test_a_boq_line_opens_with_its_rate_its_measurements_and_its_package(client, tender):
    tender_id, priya_id, quote_id = tender
    home = client.app.state.home
    upload(client, tender_id, {"A-102.pdf": PLAN})
    plan = read_all(client, tender_id)["A-102.pdf"]["id"]
    body = {"document_id": plan, "page": 1, "line": [[85.04, 141.73], [481.89, 141.73]], "length_m": 40}
    assert client.post(f"/tenders/{tender_id}/scales", json={**body, "dimension": "40.00"}).status_code == 201
    patch = [[100, 100], [200, 100], [200, 200]]
    with client.app.state.sessions() as session:
        why = "Membrane and labour at current prices."
        rate = estimate.propose_rate(session, tender_id, priya_id, "6.3", "estimate", why, unit_rate=Decimal(38))
        roof = takeoff.measure(session, tender_id, priya_id, plan, 1, "area", "Roof", YARD, "m2", None, "6.3")
        loose = takeoff.measure(
            session, tender_id, priya_id, quote_id, 1, "area", "Loose patch", patch, "m2", None, None
        )
        package = subcontract.create_package(session, tender_id, priya_id, "Waterproofing", "supply", ["6.3"])
        session.commit()
        r, m, p = rate.id[:8], roof.id[:8], package.id[:8]
        loose_ref = f"measurement {loose.id[:8]}"

    with client.app.state.sessions() as session:
        kind, item = lookup.find(session, tender_id, "6.3")
        assert lookup.related(session, kind, item) == [("rate", rate.id)]
        opened = lookup.explain(session, home, tender_id, kind, item).splitlines()
        assert opened[:6] == [
            f"boq {item.id[:8]} · approved by the engineer",
            "6.3: Waterproofing · 980 m2",
            "From Bill.xlsx, page 1: “A3=6.3 | B3=Waterproofing | C3=m2 | D3=980”",
            f"Rate: 38.00 per m2 (estimate); 980 m2 is 37,240.00 · rate {r}, with the Manager",
            f"Measured: “Roof” 800.02 m2 · measurement {m}, with the Manager",
            "In the Waterproofing package.",
        ]
        assert opened[6].startswith("Made by Priya on ") and opened[7].startswith("The engineer approved it on ")
        assert lookup.explain(session, home, tender_id, "boq", boq.find_item(session, tender_id, "3.1")).splitlines()[
            3
        ] == ("Not priced yet.")

        kind, measured = lookup.find(session, tender_id, f"measurement {m}")
        assert lookup.related(session, kind, measured) == [("boq", item.id)]
        shown = lookup.explain(session, home, tender_id, kind, measured)  # undecided, so with Quantix's checks
        assert shown.startswith(f"measurement {m} · with the Tender Manager for review\n")
        assert " of 4 points on A-102.pdf, page 1: [[85.04, 170.08], " in shown
        assert shown.endswith(
            "The BOQ has 980 m2 for 6.3.\n"
            f"Made by Priya on {measured.created_at:%d %b %Y}.\n"
            "Quantix's checks:\n- WARNING: The drawings measure 800.02 m2 for 6.3, -18.4% from the BOQ's 980. "
            "(A-102.pdf, page 1; Bill.xlsx, page 1)"
        )

        one = lookup.takeoff_view(session, tender_id, "6.3").splitlines()
        assert one[0] == "6.3 Waterproofing: BOQ 980 m2."
        assert one[1].startswith(f"- measurement {m} · “Roof” on A-102.pdf, page 1 at about 1:286: ")
        assert one[1].endswith(" of 4 points = 800.02 m2 · with the Manager")
        assert one[2] == "Takeoff 800.02 m2, -18.4% against the BOQ (differs)."
        assert lookup.takeoff_view(session, tender_id, "3.1") == (
            "3.1 Excavation: BOQ 1,240 m3. Nothing is measured for it yet."
        )
        assert lookup.takeoff_view(session, tender_id).splitlines() == [
            "(no BOQ line) Loose patch: takeoff - m2, BOQ -  → no scale",  # Quote.pdf has no scale
            "6.3 Waterproofing: takeoff 800.02 m2, BOQ 980 m2 (-18.4%) → differs",
            "Not measured yet, 2 lines with a quantity: 3.1, 4.3",
        ]
        assert lookup.search(session, tender_id, "roof", "measurement") == [
            f"measurement {m} · “Roof” 800.02 m2 for 6.3 · with the Manager"
        ]
        assert lookup.search(session, tender_id, "patch") == [f"{loose_ref} · “Loose patch” - m2 · with the Manager"]
        assert lookup.search(session, tender_id, "", "package") == [
            f"package {p} · Waterproofing (supply), 1 items · no quote recommended yet"
        ]
        assert lookup.packages_view(session, tender_id) == (
            f"package {p} · Waterproofing (supply), 1 lines · 0 enquiries, 0 sent · quotes: none yet · no quote "
            "recommended yet"
        )

    empty = client.post("/tenders", json={"name": "Nothing yet"}).json()["id"]
    with client.app.state.sessions() as session:
        assert lookup.takeoff_view(session, empty) == "Nothing is measured yet, and the BOQ has no quantities."
        assert lookup.packages_view(session, empty) == (
            "No packages yet. Group lines to price from outside with create_package."
        )
        assert lookup.priced(session, empty) == "The BOQ is empty."
        with pytest.raises(ValueError, match=f"There is no measurement {m} on this tender"):
            lookup.find(session, empty, f"measurement {m}")


def test_a_record_opens_with_who_decided_it_and_every_version_before_it(client, tender):
    tender_id, priya_id, _ = tender
    home = client.app.state.home
    note = "Excavator output 45 m3 an hour at current plant and labour prices."
    with client.app.state.sessions() as session:
        rania = office.hire(session, tender_id, "Rania Farouk", "Tender Manager", {}, is_manager=True)
        for n in range(10):  # each new proposal replaces the one still waiting, a minute after the last
            latest = estimate.propose_rate(session, tender_id, priya_id, "3.1", "estimate", note, Decimal(10 + n))
            latest.created_at += timedelta(minutes=n)
        waterproofing = boq.find_item(session, tender_id, "6.3")
        reviews.reopen(session, "boq", waterproofing.id, "Check the quantity against the roof plan.")
        line = boq.ItemIn(item="6.3", description="Waterproofing", unit="m2", quantity=Decimal(980),
                          document_id=waterproofing.document_id, page=1, quote=waterproofing.quote)  # fmt: skip
        boq.propose_items(session, tender_id, session.get(Staff, priya_id), [line])
        vat = next(f for f in boq.facts(session, tender_id) if f.kind == "vat")
        reviews.reopen(session, "fact", vat.id, "Quote the whole clause.")
        clause = "VAT at 15% shall be shown separately"
        again = boq.propose_fact(session, tender_id, session.get(Staff, priya_id), "vat", "15%", vat.document_id, 1,
                                 clause)  # fmt: skip
        verdict = reviews.Verdict(record=f"fact {again.id[:8]}", accept=True, note="Read the whole clause on page 1.")
        reviews.review(session, home, tender_id, rania, [verdict], autonomous=True)
        site = [estimate.PreliminaryIn(item="Site office", quantity=3, unit="month", rate=5000)]
        six, five = Decimal("0.06"), Decimal("0.05")
        first = estimate.propose_markups(session, tender_id, priya_id, site, six, five, Decimal(0), "Three months.")
        estimate.propose_markups(session, tender_id, priya_id, site, six, six, Decimal(0), "Three months again.")
        session.commit()

        rate = lookup.explain(session, home, tender_id, "rate", latest)
        history = rate.split("\nOther versions of the same work, newest first:\n")[1].splitlines()
        assert len(history) == 9 and history[-1] == "… and 1 older."  # eight shown
        assert re.fullmatch(
            r"- rate \w{8} · \d\d \w{3} \d\d:\d\d · 18\.00 per unit \(estimate\) · replaced by a newer version",
            history[0],
        )
        _, redone = lookup.find(session, tender_id, "6.3")
        assert re.search(
            r"\n- boq \w{8} · \d\d \w{3} \d\d:\d\d · 980 m2 · sent back: Check the quantity against the roof plan\.$",
            lookup.explain(session, home, tender_id, "boq", redone),
        )
        sent_back = lookup.explain(session, home, tender_id, "fact", vat)
        assert f"The engineer sent it back on {vat.decided_at:%d %b %Y}: Quote the whole clause." in sent_back
        kind, current = lookup.find(session, tender_id, f"fact {again.id[:8]}")
        shown = lookup.explain(session, home, tender_id, kind, current).splitlines()
        assert shown[:5] == [
            f"fact {again.id[:8]} · approved by the office, not reviewed by the engineer",
            f"From Conditions.pdf, page 1: “{clause}”",
            f"Made by Priya on {again.created_at:%d %b %Y}.",
            "Rania accepted it: Read the whole clause on page 1.",
            f"Approved by the office on {again.decided_at:%d %b %Y}, without the engineer's review.",
        ]
        assert re.fullmatch(rf"- fact {vat.id[:8]} · .+ · 15% · sent back: Quote the whole clause\.", shown[-1])
        kind, markups = lookup.find(session, tender_id, "Markups")
        assert (
            lookup.explain(session, home, tender_id, kind, markups).endswith(
                " · overheads 6.0%, profit 5.0% · replaced by a newer version"
            )
            and first.status == "replaced"
        )


def test_a_checklist_item_points_to_its_sent_back_draft_to_correct(client, tender):
    tender_id, priya_id, _ = tender
    home = client.app.state.home
    conditions = document_id(client, tender_id, "Conditions.pdf")
    clause = "VAT at 15% shall be shown separately"
    wanted = submission.RequirementIn(
        section="Commercial", title="VAT shown separately", document_id=conditions, page=1, quote=clause
    )
    with client.app.state.sessions() as session:
        rania = office.hire(session, tender_id, "Rania Farouk", "Tender Manager", {}, is_manager=True)
        submission.add_requirements(session, tender_id, priya_id, [wanted])
        requirement = submission.find_requirement(session, tender_id, "VAT shown separately")
        c = requirement.id[:8]
        assert lookup.state(session, "checklist", requirement) == "nothing drafted or attached yet"
        first = submission.draft(session, requirement, priya_id, "VAT statement", "Our price excludes VAT at 15%.")
        assert lookup.state(session, "checklist", requirement) == "its draft is with the Tender Manager"
        d = first.id[:8]
        verdicts = [
            reviews.Verdict(record=f"checklist {c}", accept=True, note="Checked the clause on page 1."),
            reviews.Verdict(record=f"draft {d}", accept=False, note="Say the VAT amount as well."),
        ]
        reviews.review(session, home, tender_id, rania, verdicts, autonomous=False)
        session.commit()

        assert lookup.state(session, "checklist", requirement) == (
            f"its draft was sent back: open draft {d} for the text to correct"
        )
        kind, found = lookup.find(session, tender_id, f"requirement {c}")
        assert (kind, found) == ("checklist", requirement)
        assert lookup.explain(session, home, tender_id, kind, found) == (
            f"checklist {c} · its draft was sent back: open draft {d} for the text to correct\n"
            f"Commercial · VAT shown separately\nRequired by Conditions.pdf, page 1: “{clause}”\n"
            f"Sent back: draft {d}, “VAT statement”: Say the VAT amount as well.\n"
            "Open it for its text, correct it and draft it again for this item.\n"
            f"Made by Priya on {requirement.created_at:%d %b %Y}.\nRania accepted it: Checked the clause on page 1."
        )
        assert lookup.search(session, tender_id, "vat statement", "draft") == [
            f"draft {d} · “VAT statement” for checklist {c} (Commercial · VAT shown separately) · sent back"
        ]
        assert lookup.search(session, tender_id, "", "checklist") == [
            f"checklist {c} · Commercial · VAT shown separately · its draft was sent back: open draft {d} for the "
            "text to correct"
        ]

        second = submission.draft(session, requirement, priya_id, "VAT statement", "VAT at 15% is 1,500.00.")
        session.commit()
        assert lookup.explain(session, home, tender_id, "checklist", requirement).splitlines()[3] == (
            f"Current draft: draft {second.id[:8]}, “VAT statement”"
        )
        assert re.search(
            rf"\n- draft {d} · .+ · “VAT statement” · sent back: Say the VAT amount as well\.$",
            lookup.explain(session, home, tender_id, "draft", second),
        )
        for kind, record in (("draft", second), ("checklist", requirement)):
            office.note_opened(session, tender_id, priya_id, kind, record.id)
        where = {"document_id": conditions, "page": 1}  # a draft opens the page its checklist item rests on
        assert lookup.cited(session, tender_id, priya_id, f"draft {second.id[:8]}") == {
            "label": "The draft “VAT statement”",
            **where,
        }
        assert lookup.cited(session, tender_id, priya_id, f"checklist {c}") == {
            "label": "The checklist item “VAT shown separately”",
            **where,
        }
        verdict = reviews.Verdict(record=f"draft {second.id[:8]}", accept=True, note="The amount is right now.")
        reviews.review(session, home, tender_id, rania, [verdict], autonomous=False)
        assert lookup.state(session, "checklist", requirement) == "its draft is waiting for the engineer"
        submission.decide(session, second, True)
        assert lookup.state(session, "checklist", requirement) == "ready"


def test_packages_their_enquiries_and_quotes_are_looked_up_like_any_record(client, subcontracted):
    tender_id, rania_id, omar_id, _, _ = subcontracted
    home = client.app.state.home
    package_id = test_subcontract.two_quotes(client, subcontracted)
    with client.app.state.sessions() as session:
        package = session.get(Package, package_id)
        gulf = subcontract.find_company(session, "Gulf Groundworks")
        enquiry = subcontract.draft_enquiry(session, package, gulf, omar_id, "Groundworks enquiry", "Please price.")
        najd = subcontract.recommend(
            session, package, omar_id, subcontract.find_company(session, "Najd Contracting"), "Cheapest."
        )
        session.commit()
        gulf_quote = next(q for q in subcontract.quotes(session, package_id) if q.company_id == gulf.id)
        p, e, g, n = package_id[:8], enquiry.id[:8], gulf_quote.id[:8], najd.id[:8]

        kind, found = lookup.find(session, tender_id, f"recommendation {p}")
        assert (kind, found) == ("package", package)
        assert lookup.explain(session, home, tender_id, kind, found) == (
            f"package {p} · recommendation with the Manager\nGroundworks (subcontract): 3.1, 6.3\n"
            f"Enquiry to Gulf Groundworks: a draft · enquiry {e}\n"
            f"Quote from Gulf Groundworks · quote {g}\nQuote from Najd Contracting · quote {n}\n"
            "Levelled by Quantix:\n"
            "1. Najd Contracting: quoted 20460.00, exclusions 0, levelled 57700.00\n"
            "2. Gulf Groundworks: quoted 55380.00, exclusions 5000, levelled 60380.00\n"
            "Recommended: Najd Contracting. Cheapest.\n"
            f"Made by Omar on {package.created_at:%d %b %Y}.\n"
            "Quantix's checks:\n- WARNING: Najd Contracting left out 6.3; our own rate stands in for them."
        )
        assert lookup.explain(session, home, tender_id, *lookup.find(session, tender_id, f"quote {g}")) == (
            f"quote {g} · recorded\nGulf Groundworks's quote for the Groundworks package, from Gulf.pdf:\n"
            "- 3.1: 17.00 per m3 (page 1: “3.1 Excavation 17.00”)\n"
            "- 6.3: 35.00 per m2 (page 1: “6.3 Waterproofing 35.00”)\n"
            "- Excludes Dewatering, which we put at 5000 (page 1)\n"
            f"Made by Omar on {gulf_quote.created_at:%d %b %Y}."
        )
        assert lookup.search(session, tender_id, "gulf") == [f"quote {g} · Gulf Groundworks for Groundworks, 2 rates"]
        assert lookup.packages_view(session, tender_id) == (
            f"package {p} · Groundworks (subcontract), 2 lines · 1 enquiries, 0 sent · quotes: Gulf Groundworks, Najd "
            "Contracting · recommendation with the Manager"
        )
        for kind, record_id in (("quote", gulf_quote.id), ("package", package_id), ("enquiry", enquiry.id)):
            office.note_opened(session, tender_id, omar_id, kind, record_id)
        assert [
            lookup.cited(session, tender_id, omar_id, ref) for ref in (f"quote {g}", f"package {p}", f"enquiry {e}")
        ] == [
            {"label": "Gulf Groundworks's quote for Groundworks"},
            {"label": "The Groundworks package"},
            {"label": "The enquiry “Groundworks enquiry”"},
        ]

        rania = session.get(Staff, rania_id)
        verdicts = [
            reviews.Verdict(record=f"enquiry {e}", accept=False, note="Name the drawings it prices."),
            reviews.Verdict(
                record=f"recommendation {p}",
                accept=True,
                note="Checked the levelling.",
                warnings_reason="Our 6.3 rate is sound.",
            ),
        ]
        assert reviews.review(session, home, tender_id, rania, verdicts, autonomous=False).startswith(
            "Accepted 1 (waiting for the engineer). Sent back 1."
        )
        session.commit()
        assert lookup.state(session, "package", package) == "recommendation waiting for the engineer"
        assert lookup.explain(session, home, tender_id, "enquiry", enquiry) == (
            f"enquiry {e} · sent back\nTo Gulf Groundworks for the Groundworks package.\nSubject: Groundworks "
            f"enquiry\nPlease price.\nMade by Omar on {enquiry.created_at:%d %b %Y}.\nRania sent it back: Name the "
            "drawings it prices."
        )
    assert client.post(f"/packages/{package_id}/choice", json={"quote_id": najd.id}).status_code == 200
    with client.app.state.sessions() as session:
        package = session.get(Package, package_id)
        assert lookup.state(session, "package", package) == "the engineer chose a quote"
        assert "\nChosen: Najd Contracting.\n" in lookup.explain(session, home, tender_id, "package", package)
    elsewhere = client.post("/tenders", json={"name": "Another school"}).json()["id"]
    with client.app.state.sessions() as session:
        with pytest.raises(ValueError, match=f"There is no quote {g} on this tender"):
            lookup.find(session, elsewhere, f"quote {g}")


def test_an_answer_rests_on_what_its_sender_opened_since_the_engineer_wrote(client, tender):
    tender_id, priya_id, _ = tender
    conditions = document_id(client, tender_id, "Conditions.pdf")
    url = "https://prices.example/rebar"
    with client.app.state.sessions() as session:
        web_page = WebPage(url=url, title="Rebar prices", text="Rebar B500B 2,350 SAR per t")
        session.add(web_page)
        session.commit()
        web_page_id, read_at = web_page.id, web_page.read_at
        refused = {
            "priced_boq": "You haven't looked at priced_boq yet: call it first, then give it as a source.",
            url: f"You haven't read {url}. Read it with read_web_page first.",
            "Nowhere.pdf, page 1": "No document is called “Nowhere.pdf”. Name it as list_documents shows it.",
            "Conditions, page 1": "You haven't read Conditions.pdf, page 1. Read it with read_page first.",
            "rate 0000ffff": "There is no rate 0000ffff on this tender. Use find_records to look it up by words.",
            "markups": "No markups have been proposed yet.",
            "the site visit": "“the site visit” is neither a record like “rate 42a9fb15” nor a BOQ line like “C.1.2”",
        }
        for source, message in refused.items():
            with pytest.raises(ValueError, match=re.escape(message)):
                lookup.cited(session, tender_id, priya_id, source)
        assert not lookup.promised(session, tender_id, priya_id)  # the engineer hasn't asked anything

    client.post(f"/tenders/{tender_id}/messages", json={"channel": priya_id, "text": "Where does the VAT come from?"})
    with client.app.state.sessions() as session:
        vat = next(f for f in boq.facts(session, tender_id) if f.kind == "vat")
        for kind, ref in (
            ("summary", "priced_boq"),
            ("summary", "a summary no longer kept"),
            ("page", f"{conditions}:1"),
            ("page", f"{'0' * 32}:1"),  # a document since deleted
            ("web", web_page_id),
            ("web", "0" * 32),
            ("fact", vat.id),
            ("task", "1234abcd"),  # not a record an answer rests on
        ):
            office.note_opened(session, tender_id, priya_id, kind, ref)
        session.commit()
        assert set(lookup.citable(session, tender_id, priya_id)) == {
            "priced_boq",
            "Conditions.pdf, page 1",
            url,
            f"fact {vat.id[:8]}",
        }
        assert len(lookup.citable(session, tender_id, priya_id, limit=2)) == 2
        assert lookup.cited(session, tender_id, priya_id, " Priced_BOQ ") == {"label": "The priced BOQ"}
        assert lookup.cited(session, tender_id, priya_id, url) == {
            "label": f"Rebar prices, read {read_at:%d %b %Y}",
            "url": url,
        }
        assert lookup.cited(session, tender_id, priya_id, "“Conditions” page 1") == {
            "label": "Conditions.pdf, page 1",
            "document_id": conditions,
            "page": 1,
        }
        assert lookup.cited(session, tender_id, priya_id, f"fact {vat.id[:8]} · VAT: 15%") == {
            "label": "The vat",
            "document_id": conditions,
            "page": 1,
        }


def test_the_priced_boq_by_bill_and_a_page_at_a_time(client, tender, monkeypatch):
    tender_id, priya_id, _ = tender
    with client.app.state.sessions() as session:
        excavation = boq.find_item(session, tender_id, "3.1")
        session.add(
            BoqItem(tender_id=tender_id, section="Roads", item="R.1", description="Road markings", unit="item",
                    quantity=None, document_id=excavation.document_id, page=1, quote=excavation.quote, position=9,
                    proposed_by=priya_id, status="approved")
        )  # fmt: skip
        session.flush()
        why = "Lump sum for the markings from a specialist's price."
        markings = estimate.propose_rate(session, tender_id, priya_id, "R.1", "estimate", why, Decimal(5000))
        session.commit()

        assert lookup.priced(session, tender_id, section="roads").splitlines() == [
            "1 lines in “roads”, 0 priced, together 0 SAR (Quantix's figures). item | description | quantity | rate | "
            "amount | basis | state | reference",
            "Roads / R.1 | Road markings | - item | 5000.00 | - | estimate | with the Manager | "
            f"rate {markings.id[:8]}",
            "Lines 1 to 1 of 1.",
        ]
        with pytest.raises(
            ValueError, match="No BOQ section matches “Drainage”. The sections are: \\(no section\\), Roads."
        ):
            lookup.priced(session, tender_id, section="Drainage")
        with pytest.raises(ValueError, match="There are 4 lines; start from 1 to 4."):
            lookup.priced(session, tender_id, start=5)
        monkeypatch.setattr(lookup, "PAGE", 2)
        first, second = lookup.priced(session, tender_id), lookup.priced(session, tender_id, start=3)
        assert first.splitlines()[-1] == "Lines 1 to 2 of 4. Call again with start=3 for the rest."
        assert second.splitlines()[1:] == [
            "6.3 | Waterproofing | 980 m2 | not priced",
            "Roads / R.1 | Road markings | - item | 5000.00 | - | estimate | with the Manager | "
            f"rate {markings.id[:8]}",
            "Lines 3 to 4 of 4.",
        ]
        with pytest.raises(ValueError, match="kind is one of boq, fact, checklist, draft, measurement, query"):
            lookup.search(session, tender_id, "markings", "rate")


def test_the_conversation_and_what_changed_show_decisions_tasks_and_answers(client, tender):
    tender_id, priya_id, _ = tender
    home = client.app.state.home
    start = datetime.now(UTC)
    note = "Excavator output 45 m3 an hour at current plant and labour prices."
    with client.app.state.sessions() as session:
        rania = office.hire(session, tender_id, "Rania Farouk", "Tender Manager", {}, is_manager=True)
        priya = session.get(Staff, priya_id)
        excavation = estimate.propose_rate(session, tender_id, priya_id, "3.1", "estimate", note, Decimal(18))
        rebar = estimate.propose_rate(session, tender_id, priya_id, "4.3", "estimate", note, Decimal(3400))
        verdicts = [
            reviews.Verdict(record=f"rate {excavation.id[:8]}", accept=True, note="Checked the output and prices."),
            reviews.Verdict(record=f"rate {rebar.id[:8]}", accept=False, note="Build it up from the steel quote."),
        ]
        reviews.review(session, home, tender_id, rania, verdicts, autonomous=False)
        estimate.decide(session, excavation, True)
        task = office.assign(
            session, tender_id, rania, priya, "Check the dewatering allowance", "Read the soil report."
        )
        office.complete(session, priya, task.id, "No dewatering: the water table is 6 m down.")
        question = office.ask(session, tender_id, rania, "Dewatering allowance", "Allow for dewatering?", ["Yes", "No"])
        office.answer(session, question, "Leave it out")
        office.ask(session, tender_id, rania, "Rock excavation", "Is there rock below 2 m?", ["Yes", "No"])
        session.commit()

        assert activity.changed(session, tender_id, start).splitlines() == [
            f"Since {start:%d %b %H:%M}:",
            "Filed: Priya 2 rates",
            "The Tender Manager accepted 1 and sent back 1.",
            "The engineer approved 1 and sent back 0.",
            "Tasks finished: Priya: Check the dewatering allowance",
            "Questions the engineer answered: Dewatering allowance",
        ]
        later = datetime.now(UTC) + timedelta(hours=1)
        assert activity.changed(session, tender_id, later) == f"Nothing has changed since {later:%d %b %H:%M}."
        found = [line.split(" · ", 1)[1] for line in activity.conversation(session, tender_id, "dewatering")]
        assert sorted(found) == [
            "Decision “Dewatering allowance”: Allow for dewatering? The engineer answered: Leave it out",
            "Priya in the team room: Finished: Check the dewatering allowance. No dewatering: the water table is 6 m "
            "down.",
            "Rania in the team room: Rania asked Priya to Check the dewatering allowance",
            "Task for Priya: Check the dewatering allowance (done). "
            "Result: No dewatering: the water table is 6 m down.",
            "The engineer in the chat between Rania and the engineer: About “Dewatering allowance”: Leave it out",
        ]
        assert [line.split(" · ", 1)[1] for line in activity.conversation(session, tender_id, "rock")] == [
            "Decision “Rock excavation”: Is there rock below 2 m? Waiting for the engineer."
        ]
        answered = office.messages(session, tender_id, rania.id)[-1]
        assert activity.since(session, tender_id, None) == answered.created_at  # when the engineer last wrote
        assert abs(activity.since(session, tender_id, 2) - (datetime.now(UTC) - timedelta(hours=2))) < timedelta(
            minutes=1
        )
