"""The Tender Manager reviews everything his staff propose before it reaches the engineer."""

import re
import time
from datetime import UTC, datetime
from decimal import Decimal
from types import SimpleNamespace

import pytest
import test_estimate
import test_subcontract
from pydantic_ai.messages import (
    ModelResponse,
    RetryPromptPart,
    TextPart,
    ToolCallPart,
    ToolReturnPart,
    UserPromptPart,
)
from test_documents import PDF, make_xlsx, read_all, upload
from test_lookup import fake_turn
from test_office import scripted, wait_for
from test_revisions import PLAN, YARD

from quantix import settings
from quantix.boq import records as boq
from quantix.company import CompanyRule
from quantix.documents.models import WebPage
from quantix.estimate import records as estimate
from quantix.office import agents, packs, runtime, tools
from quantix.office import records as office
from quantix.office.models import Staff, Task, TurnRecord
from quantix.review import records as reviews
from quantix.subcontract import records as subcontract
from quantix.subcontract.models import Package
from quantix.submission import records as submission
from quantix.takeoff import records as takeoff

subcontracted = test_subcontract.tender  # the fixture: a school's groundworks, with Rania and Omar

QUOTES = (
    "A2=3.1 | B2=Excavation to reduce levels | C2=m3 | D2=1240",
    "A3=4.2 | B3=خرسانة مسلحة للبلاطة | C3=م3 | D3=312.4",
)


@pytest.fixture
def office_with_work(client):
    """A tender where Omar entered two BOQ lines and a fact, waiting for Rania, the Tender Manager."""
    tender_id = client.post("/tenders", json={"name": "Synthetic school"}).json()["id"]
    upload(client, tender_id, {"Bill.xlsx": make_xlsx(), "Conditions.pdf": PDF})
    documents = read_all(client, tender_id)
    bill, conditions = documents["Bill.xlsx"]["id"], documents["Conditions.pdf"]["id"]
    with client.app.state.sessions() as session:
        rania = office.hire(session, tender_id, "Rania Farouk", "Tender Manager", {}, is_manager=True)
        omar = office.hire(session, tender_id, "Omar Haddad", "Quantity Surveyor", {})
        lines = [
            boq.ItemIn(item="3.1", description="Excavation", unit="m3", quantity=Decimal("1240"), document_id=bill,
                       page=1, quote=QUOTES[0]),
            boq.ItemIn(item="4.2", description="Slab", unit="م3", quantity=Decimal("312.4"), document_id=bill,
                       page=1, quote=QUOTES[1]),
        ]  # fmt: skip
        boq.propose_items(session, tender_id, omar, lines)
        boq.propose_fact(session, tender_id, omar, "method_of_measurement", "Re-measured", conditions, 1, "Tender")
        session.commit()
        return tender_id, rania.id, omar.id


def refs(client, tender_id) -> dict[str, str]:
    """The Manager's queue, as {line: ref}."""
    with client.app.state.sessions() as session:
        return {p.record.id: p.ref for p in reviews.pending(session, tender_id)}


def reach(is_manager: bool) -> set[str]:
    """Every tool someone can use: their core and every pack, loaded or not."""
    core, work = packs.loadout(SimpleNamespace(is_manager=is_manager, profile={}), 0, True)
    return {t.__name__ for t in core} | {name for pack in work for name in pack.toolset.tools}


def test_the_manager_leads_and_reviews_but_never_produces_records():
    names = reach(is_manager=True)
    assert {"review_queue", "open_record", "review", "hire", "assign_task", "ask_engineer"} <= names
    produce = {t.__name__ for w in packs.WORK.values() for t in w.produces}
    assert not names & produce  # every record has a producer and a different reviewer
    assert "propose_rate" in produce and "review" not in reach(is_manager=False)
    assert tuple(packs.WORK) == tools.WORK_KINDS  # hire offers exactly the packs there are


def test_the_manager_accepts_or_sends_back_each_record(client, office_with_work):
    tender_id, rania_id, omar_id = office_with_work
    boq_page = client.get(f"/tenders/{tender_id}/boq").json()
    excavation, slab = boq_page["items"]
    [fact] = boq_page["facts"]
    queue = refs(client, tender_id)
    gates = client.get(f"/tenders/{tender_id}/gates").json()
    assert (gates["manager"], gates["boq"], gates["facts"]) == (3, 0, 0)  # nothing reaches the engineer unreviewed

    with client.app.state.sessions() as session:
        rania = session.get(Staff, rania_id)
        verdict = reviews.Verdict
        report = reviews.review(
            session,
            client.app.state.home,
            tender_id,
            rania,
            [
                verdict(record=queue[excavation["id"]], accept=True, note="Quantity matches Bill.xlsx row 2."),
                verdict(record=queue[slab["id"]], accept=False, note="Use the unit as printed, m3, not م3."),
                verdict(record=queue[fact["id"]], accept=True, note="ok"),  # a note that says nothing
                verdict(record="rate 12345678", accept=True, note="Checked against the quote."),
            ],
            autonomous=False,
        )
        session.commit()
    assert report.startswith(
        "Accepted 1 (waiting for the engineer). Sent back 1. Once your review is done, tell the engineer with "
        "message_engineer what now waits for their approval and where: 1 BOQ line on the Estimate screen.\nNot done:\n"
    )
    assert f"{queue[fact['id']]}: say in the note what you checked, or what to correct" in report
    assert "rate 12345678: nothing with that reference is waiting for your review" in report

    items = {i["item"]: i for i in client.get(f"/tenders/{tender_id}/boq").json()["items"]}
    assert (items["3.1"]["status"], items["3.1"]["reviewed_by"]) == ("reviewed", rania_id)
    assert "4.2" not in items  # sent back: Omar enters it again
    team = client.get(f"/tenders/{tender_id}/messages", params={"channel": "team"}).json()
    assert (team[-1]["sender"], team[-1]["text"]) == (
        rania_id,
        "Omar, I sent back BOQ item 4.2: Use the unit as printed, m3, not م3.",
    )
    tasks = client.get(f"/tenders/{tender_id}/tasks").json()  # the redo is Omar's to do, as an open task
    assert [(t["staff_id"], t["title"], t["brief"], t["status"]) for t in tasks] == [
        (omar_id, "Redo BOQ item 4.2", "Use the unit as printed, m3, not م3.", "open")
    ]
    gates = client.get(f"/tenders/{tender_id}/gates").json()
    assert (gates["manager"], gates["boq"]) == (1, 1)  # the fact still waits for him; the line waits for the engineer


def test_the_queue_says_who_did_what_and_shows_the_detail(client, office_with_work):
    tender_id, _, _ = office_with_work
    queue = client.get(f"/tenders/{tender_id}/review").json()
    assert [(w["kind"], w["line"].split(" · ", 2)[1]) for w in queue] == [
        ("boq", "by Omar"),
        ("boq", "by Omar"),
        ("fact", "by Omar"),
    ]
    assert queue[0]["line"].endswith("3.1: Excavation · 1,240 m3")
    with client.app.state.sessions() as session:
        assert reviews.counts(session, tender_id) == "Omar: 2 BOQ lines, 1 fact"
        found = reviews.find(session, tender_id, queue[0]["ref"])
        assert reviews.details(session, found) == f"From Bill.xlsx, page 1: “{QUOTES[0]}”"


def test_in_an_autonomous_office_his_acceptance_approves_and_the_engineer_can_reopen(client, office_with_work):
    tender_id, rania_id, _ = office_with_work
    [fact] = client.get(f"/tenders/{tender_id}/boq").json()["facts"]
    with client.app.state.sessions() as session:
        verdicts = [reviews.Verdict(record=refs(client, tender_id)[fact["id"]], accept=True, note="Read it on page 1.")]
        reviews.review(session, client.app.state.home, tender_id, session.get(Staff, rania_id), verdicts, True)
        session.commit()
    [fact] = client.get(f"/tenders/{tender_id}/boq").json()["facts"]
    assert (fact["status"], fact["review_note"]) == ("office_approved", "Read it on page 1.")

    excavation = client.get(f"/tenders/{tender_id}/boq").json()["items"][0]
    refused = client.post(f"/records/boq/{excavation['id']}/reopen", json={"reason": "No."})
    assert refused.json()["detail"] == "Only approved work can be reopened; send back what is still waiting instead."
    assert client.post(f"/records/fact/{fact['id']}/reopen", json={"reason": "Check page 2."}).status_code == 204
    assert client.get(f"/tenders/{tender_id}/boq").json()["facts"] == []
    team = client.get(f"/tenders/{tender_id}/messages", params={"channel": "team"}).json()
    assert (team[-1]["sender"], team[-1]["text"]) == (
        "engineer",
        "Omar, I sent back the method of measurement: Check page 2.",
    )


def test_the_manager_wakes_for_new_work_in_his_queue(client, office_with_work, tmp_path):
    tender_id, _, _ = office_with_work
    settings.save(tmp_path, office_ai={"connection_id": "scripted", "model": "brain"})
    manager_prompts: list[str] = []

    def brain(messages, info):
        if "You are Rania Farouk" in info.instructions:
            prompt = next(str(p.content) for m in messages for p in m.parts if isinstance(p, UserPromptPart))
            if not manager_prompts or manager_prompts[-1] != prompt:
                manager_prompts.append(prompt)
        return ModelResponse(parts=[TextPart("Done.")])

    client.app.state.office.model = lambda: scripted(brain)
    client.app.state.office.engineer_spoke(tender_id)  # nothing was said to him: his queue alone wakes him
    wait_for(lambda o: len(manager_prompts) >= 2, client, tender_id)
    assert (
        "Waiting for your review: Omar: 2 BOQ lines, 1 fact. Go through them with review_queue" in (manager_prompts[0])
    )
    assert manager_prompts[1].startswith(runtime.QUEUE_LEFT)  # left undecided: woken once more to settle it
    client.app.state.office.engineer_spoke(tender_id)
    time.sleep(0.3)
    wait_for(lambda o: True, client, tender_id)
    assert len(manager_prompts) == 2  # then the same queue doesn't wake him again


def test_the_manager_tells_the_engineer_what_waits_for_them_and_when_nothing_does(client, office_with_work, tmp_path):
    """The real tender: Salem accepted the redone markups the engineer had asked to see, and never said so. The
    engineer found them by chance on the Estimate screen, and no one told them when nothing waited any more."""
    tender_id, rania_id, _ = office_with_work
    excavation, slab = client.get(f"/tenders/{tender_id}/boq").json()["items"]
    for item in (excavation, slab):
        decide(client, tender_id, rania_id, item["id"], True, "Quantity matches Bill.xlsx row 2.")
    with client.app.state.sessions() as session:
        assert reviews.for_engineer(session, tender_id) == "2 BOQ lines on the Estimate screen"
        assert agents.standing(session, tender_id).endswith(
            "- Waiting for the engineer's approval: 2 BOQ lines on the Estimate screen."
        )
    settings.save(tmp_path, office_ai={"connection_id": "scripted", "model": "brain"})
    prompts: list[str] = []

    def brain(messages, info):
        replied = any(isinstance(p, ToolReturnPart) for m in messages for p in m.parts)
        if "You are Rania Farouk" not in info.instructions or replied:
            return ModelResponse(parts=[TextPart("Done.")])
        prompt = next(str(p.content) for m in messages for p in m.parts if isinstance(p, UserPromptPart))
        if prompt.startswith(runtime.APPROVALS_WAITING):
            text = "BOQ lines 3.1 and 4.2 wait for your approval on the Estimate screen."
        elif prompt.startswith(runtime.ALL_APPROVED):
            text = "You have approved everything; nothing waits for you."
        else:  # the fact he left in his queue, woken once more for it
            return ModelResponse(parts=[TextPart("Done.")])
        prompts.append(prompt)
        return ModelResponse(parts=[ToolCallPart("message_engineer", {"text": text})])

    client.app.state.office.model = lambda: scripted(brain)
    client.app.state.office.engineer_spoke(tender_id)  # nothing was said to him: what waits for the engineer wakes him
    wait_for(lambda o: prompts, client, tender_id)
    assert prompts[0].startswith(runtime.APPROVALS_WAITING)
    assert "· by Omar · 3.1: Excavation · 1,240 m3 · on the Estimate screen" in prompts[0]
    client.app.state.office.engineer_spoke(tender_id)
    time.sleep(0.3)
    wait_for(lambda o: True, client, tender_id)
    assert len(prompts) == 1  # he told them: not woken again for the same work

    for item in (excavation, slab):  # approved on the Estimate screen, with nothing said in the chat
        assert client.post(f"/boq/{item['id']}/decision", json={"approve": True}).status_code == 200
    wait_for(lambda o: len(prompts) == 2, client, tender_id)
    assert prompts[1].startswith(runtime.ALL_APPROVED)
    assert "- Nothing is waiting for the engineer's approval." in prompts[1]
    client.app.state.office.engineer_spoke(tender_id)
    time.sleep(0.3)
    wait_for(lambda o: True, client, tender_id)
    assert len(prompts) == 2
    chat = client.get(f"/tenders/{tender_id}/messages", params={"channel": rania_id}).json()
    assert [m["text"] for m in chat] == ["You have approved everything; nothing waits for you."]  # one current update


def decide(client, tender_id, manager_id, record_id, accept, note, lesson=None) -> str:
    with client.app.state.sessions() as session:
        [ref] = [p.ref for p in reviews.pending(session, tender_id) if p.record.id == record_id]
        verdict = reviews.Verdict(record=ref, accept=accept, note=note, lesson=lesson)
        manager = session.get(Staff, manager_id)
        report = reviews.review(session, client.app.state.home, tender_id, manager, [verdict], autonomous=False)
        session.commit()
        return report


def test_what_the_office_cant_settle_goes_to_the_engineer_with_where_it_shows(client, office_with_work):
    tender_id, rania_id, _ = office_with_work
    conditions = read_all(client, tender_id)["Conditions.pdf"]["id"]
    [fact] = client.get(f"/tenders/{tender_id}/boq").json()["facts"]
    problem = "The conditions say the works are re-measured, but the bill's preamble cites POMI."
    source = reviews.Source(document_id=conditions, page=1, what="Re-measured works")
    with client.app.state.sessions() as session:
        rania, ref = session.get(Staff, rania_id), refs(client, tender_id)[fact["id"]]
        with pytest.raises(ValueError, match="Show the engineer where the problem is"):
            reviews.escalate(session, tender_id, rania, ref, problem, [], ["Re-measured"])
        with pytest.raises(ValueError, match="has no page 9"):
            wrong = reviews.Source(document_id=conditions, page=9, what="Re-measured works")
            reviews.escalate(session, tender_id, rania, ref, problem, [wrong], ["Re-measured"])
        reviews.escalate(session, tender_id, rania, ref, problem, [source], ["Re-measured", "POMI, per the bill"])
        with pytest.raises(ValueError, match="You already escalated it"):
            reviews.escalate(session, tender_id, rania, ref, problem, [source], ["Re-measured"])
        assert reviews.counts(session, tender_id) == "Omar: 2 BOQ lines"  # it waits for the engineer, not for him
        session.commit()

    [decision] = client.get(f"/tenders/{tender_id}/decisions").json()
    assert (decision["title"], decision["text"], decision["options"]) == (
        "The method of measurement",
        problem,
        ["Re-measured", "POMI, per the bill"],
    )
    assert (decision["subject_kind"], decision["subject_id"]) == ("fact", fact["id"])
    assert [(s["label"], s["document_id"], s["page"]) for s in decision["sources"]] == [
        ("Conditions.pdf, page 1", conditions, 1),  # where the fact was read
        ("Conditions.pdf, page 1: Re-measured works", conditions, 1),
    ]
    held = decide(client, tender_id, rania_id, fact["id"], True, "The engineer will decide this.")
    assert held.endswith("you escalated it to the engineer; apply their answer when it comes")

    client.post(f"/decisions/{decision['id']}/answer", json={"answer": "Re-measured"})
    chat = client.get(f"/tenders/{tender_id}/messages", params={"channel": rania_id}).json()
    assert chat[-1]["text"] == "About “The method of measurement”: Re-measured"
    applied = decide(client, tender_id, rania_id, fact["id"], True, "The engineer confirmed re-measurement.")
    assert applied.startswith("Accepted 1 (waiting for the engineer). Sent back 0.")


def test_after_two_send_backs_the_manager_escalates(client, office_with_work):
    tender_id, rania_id, omar_id = office_with_work
    note = "Excavation in rock needs a breaker: price it."

    def propose() -> str:
        with client.app.state.sessions() as session:
            why = "Own rate from excavator outputs and current prices."
            rate = estimate.propose_rate(session, tender_id, omar_id, "3.1", "estimate", why, unit_rate=Decimal(18))
            session.commit()
            return rate.id

    for _ in range(2):
        assert decide(client, tender_id, rania_id, propose(), False, note).endswith("Sent back 1.")
    third = propose()
    refused = decide(client, tender_id, rania_id, third, False, note)
    assert refused.endswith(
        f"it has been sent back 2 times already and still isn't right (last: {note}). Escalate it to the engineer "
        "with escalate: the problem, where it shows and your suggested corrections"
    )
    with client.app.state.sessions() as session:
        ref = refs(client, tender_id)[third]
        source = reviews.Source(boq_item="3.1", what="The line to price")
        suggestions = ["Price excavation in rock with a breaker", "Ask the client for the soil report"]
        problem = "Omar keeps pricing 3.1 as soft digging, but the site is rock."
        reviews.escalate(session, tender_id, session.get(Staff, rania_id), ref, problem, [source], suggestions)
        session.commit()
    [decision] = client.get(f"/tenders/{tender_id}/decisions").json()
    assert decision["title"] == "The rate for BOQ item 3.1"
    assert [s["label"] for s in decision["sources"]] == ["BOQ line 3.1", "BOQ line 3.1: The line to price"]
    client.post(f"/decisions/{decision['id']}/answer", json={"answer": suggestions[0]})
    assert decide(client, tender_id, rania_id, third, False, suggestions[0]).endswith("Sent back 1.")
    for _ in range(2):  # the count starts again from the engineer's answer
        assert decide(client, tender_id, rania_id, propose(), False, note).endswith("Sent back 1.")
    assert "Escalate it to the engineer" in decide(client, tender_id, rania_id, propose(), False, note)


def test_the_manager_escalates_through_his_tool(client, office_with_work, tmp_path):
    tender_id, _, _ = office_with_work
    conditions = read_all(client, tender_id)["Conditions.pdf"]["id"]
    [fact] = client.get(f"/tenders/{tender_id}/boq").json()["facts"]
    ref = refs(client, tender_id)[fact["id"]]
    settings.save(tmp_path, office_ai={"connection_id": "scripted", "model": "brain"})
    returned: list[str] = []

    def brain(messages, info):
        prompt = next(str(p.content) for m in messages for p in m.parts if isinstance(p, UserPromptPart))
        if "You are Rania Farouk" not in info.instructions or runtime.QUEUE_LEFT in prompt:  # the BOQ lines he left
            return ModelResponse(parts=[TextPart("Done.")])
        returned[:] = [str(p.content) for m in messages for p in m.parts if isinstance(p, ToolReturnPart)]
        if returned:
            return ModelResponse(parts=[TextPart("Done.")])
        escalation = {
            "record": ref,
            "problem": "The conditions and the bill give different methods of measurement.",
            "sources": [{"document_id": conditions, "page": 1, "what": "Re-measured works"}],
            "suggestions": ["Re-measured", "POMI, per the bill"],
        }
        return ModelResponse(parts=[ToolCallPart("escalate", escalation)])

    client.app.state.office.model = lambda: scripted(brain)
    client.app.state.office.engineer_spoke(tender_id)
    wait_for(lambda o: returned, client, tender_id)
    assert returned == [
        "Escalated to the engineer as “The method of measurement”. Carry on with other work until they answer."
    ]
    [decision] = client.get(f"/tenders/{tender_id}/decisions").json()
    assert (decision["subject_kind"], decision["subject_id"]) == ("fact", fact["id"])


def test_a_task_is_done_only_once_its_work_is_filed(client, office_with_work, tmp_path):
    tender_id, rania_id, omar_id = office_with_work
    [slab] = [i for i in client.get(f"/tenders/{tender_id}/boq").json()["items"] if i["item"] == "4.2"]
    decide(client, tender_id, rania_id, slab["id"], False, "Use the unit as printed, m3, not م3.")  # Omar's redo task
    settings.save(tmp_path, office_ai={"connection_id": "scripted", "model": "brain"})
    refused: list[str] = []

    def brain(messages, info):
        if "You are Omar Haddad" not in info.instructions:
            return ModelResponse(parts=[TextPart("Done.")])
        refused[:] = [str(p.content) for m in messages for p in m.parts if isinstance(p, RetryPromptPart)]
        if refused:
            return ModelResponse(parts=[TextPart("Done.")])
        prompt = next(str(p.content) for m in messages for p in m.parts if isinstance(p, UserPromptPart))
        task_id = re.search(r"- (\w+): Redo BOQ item 4.2", prompt).group(1)
        claim = {"task_id": task_id, "result": "Entered it again as m3.", "only_reported": True}
        return ModelResponse(parts=[ToolCallPart("complete_task", claim)])  # claims the redo without doing it

    client.app.state.office.model = lambda: scripted(brain)
    client.app.state.office.engineer_spoke(tender_id)
    wait_for(lambda o: refused, client, tender_id)
    assert refused[0].startswith("You haven't filed the corrected work yet. Redo it with its tool")
    assert [t["status"] for t in client.get(f"/tenders/{tender_id}/tasks").json()] == ["open"]

    with client.app.state.sessions() as session:
        since = office.open_tasks(session, session.get(Staff, omar_id))[0].created_at
        assert reviews.filed_since(session, omar_id, since) == ""
        omar = session.get(Staff, omar_id)
        line = boq.ItemIn(item="4.2", description="Slab", unit="m3", quantity=Decimal("312.4"),
                          document_id=slab["source"]["document_id"], page=1, quote=QUOTES[1])  # fmt: skip
        boq.propose_items(session, tender_id, omar, [line])
        assert reviews.filed_since(session, omar_id, since) == "1 BOQ line"
        session.commit()
    [again] = [i for i in client.get(f"/tenders/{tender_id}/boq").json()["items"] if i["item"] == "4.2"]
    decide(client, tender_id, rania_id, again["id"], False, "Quote the quantity as printed.")  # sent back again
    with client.app.state.sessions() as session:
        [task] = office.open_tasks(session, session.get(Staff, omar_id))
        assert task.brief == "Quote the quantity as printed."
        assert reviews.filed_since(session, omar_id, task.created_at) == ""  # the redo counts from the new send-back


UNIT = "Enter each unit exactly as the client's bill prints it, never translated."
METHOD = "Take the method of measurement from the conditions, not from the bill's preamble."


def test_a_lesson_from_work_sent_back_is_followed_by_the_whole_office(client, office_with_work):
    tender_id, rania_id, omar_id = office_with_work
    bill = read_all(client, tender_id)["Bill.xlsx"]["id"]
    excavation, slab = client.get(f"/tenders/{tender_id}/boq").json()["items"]
    [fact] = client.get(f"/tenders/{tender_id}/boq").json()["facts"]

    report = decide(client, tender_id, rania_id, excavation["id"], True, "Quantity matches row 2.", lesson=UNIT)
    assert report.endswith("a lesson comes from work that needed correcting. Give it when you send something back, "
                           "or when you accept its corrected version.")  # fmt: skip
    report = decide(client, tender_id, rania_id, slab["id"], False, "Use the unit as printed.", lesson=UNIT)
    assert report.endswith(f"Lesson kept: the whole office follows “{UNIT}” from now on.")
    similar = "Enter every unit exactly as the client's bill prints it."
    report = decide(client, tender_id, rania_id, fact["id"], False, "Read the conditions again.", lesson=similar)
    assert report.endswith(f"Lesson not kept: the office already follows: “{UNIT}”")

    with client.app.state.sessions() as session:  # Omar enters the line again, corrected
        line = boq.ItemIn(item="4.2", description="Slab", unit="م3", quantity=Decimal("312.4"), document_id=bill,
                          page=1, quote=QUOTES[1])  # fmt: skip
        boq.propose_items(session, tender_id, session.get(Staff, omar_id), [line])
        session.commit()
    redone = next(i for i in client.get(f"/tenders/{tender_id}/boq").json()["items"] if i["item"] == "4.2")
    evidence = "Quote the whole row of the bill, with its item number and quantity, as the evidence."
    report = decide(client, tender_id, rania_id, redone["id"], True, "The unit is as printed now.", lesson=evidence)
    assert report.endswith(f"Lesson kept: the whole office follows “{evidence}” from now on.")
    with client.app.state.sessions() as session:
        conditions = read_all(client, tender_id)["Conditions.pdf"]["id"]
        boq.propose_fact(session, tender_id, session.get(Staff, omar_id), "method_of_measurement", "Re-measured",
                         conditions, 1, "Tender")  # fmt: skip
        session.commit()
    [fact] = client.get(f"/tenders/{tender_id}/boq").json()["facts"]
    report = decide(client, tender_id, rania_id, fact["id"], False, "Quote the clause itself.", lesson="Check units.")
    assert report.endswith("is too short to follow. Say it as a rule the whole office can apply.")

    with client.app.state.sessions() as session:  # someone who joins later doesn't make the same mistake
        salma = office.hire(session, tender_id, "Salma Nasser", "Estimator", {})
        session.commit()
        briefing = agents.situation(session, salma, [])
    assert f"What the office learned on this tender, which everyone follows:\n- {UNIT}\n- {evidence}" in briefing


def test_the_engineer_keeps_a_lesson_as_a_company_rule_or_drops_it(client, office_with_work):
    tender_id, rania_id, omar_id = office_with_work
    excavation, slab = client.get(f"/tenders/{tender_id}/boq").json()["items"]
    [fact] = client.get(f"/tenders/{tender_id}/boq").json()["facts"]
    decide(client, tender_id, rania_id, slab["id"], False, "Use the unit as printed.", lesson=UNIT)
    decide(client, tender_id, rania_id, fact["id"], False, "Read the conditions, page 1.", lesson=METHOD)

    unit, method = client.get(f"/tenders/{tender_id}/lessons").json()
    assert (unit["text"], unit["topic"], unit["source"], unit["status"]) == (UNIT, "BOQ", "BOQ item 4.2", "tender")
    assert (method["topic"], method["source"]) == ("Tender facts", "the method of measurement")
    suggested = client.get("/lessons").json()  # every tender's, for Company rules, newest first
    assert [(lesson["text"], lesson["tender_name"]) for lesson in suggested] == [
        (METHOD, "Synthetic school"),
        (UNIT, "Synthetic school"),
    ]
    assert client.patch(f"/lessons/{unit['id']}", json={"status": "kept"}).json()["status"] == "kept"
    assert [lesson["text"] for lesson in client.get("/lessons").json()] == [METHOD]  # kept: a rule now
    assert [(r["topic"], r["text"]) for r in client.get("/rules").json()] == [("BOQ", UNIT)]
    assert client.patch(f"/lessons/{method['id']}", json={"status": "dropped"}).status_code == 200
    assert [lesson["status"] for lesson in client.get(f"/tenders/{tender_id}/lessons").json()] == ["kept"]
    refused = client.patch(f"/lessons/{unit['id']}", json={"status": "dropped"})
    assert (refused.status_code, refused.json()["detail"]) == (400, "This lesson has already been kept or dropped.")

    with client.app.state.sessions() as session:
        briefing = agents.situation(session, session.get(Staff, omar_id), [])
    assert f"- BOQ: {UNIT}" in briefing  # a company rule now, which every team follows
    assert "What the office learned" not in briefing and METHOD not in briefing
    report = decide(client, tender_id, rania_id, excavation["id"], False, "Check the quantity.", lesson=METHOD)
    assert report.endswith(f"Lesson not kept: the engineer dropped a lesson like it: “{METHOD}”")


def test_settings_show_how_each_ai_has_done_from_the_turns_and_the_reviews(client, office_with_work):
    tender_id, rania_id, omar_id = office_with_work
    excavation, slab = client.get(f"/tenders/{tender_id}/boq").json()["items"]
    calls = [{"tool": "read_page", "sent_back": "No document has that id."}, {"tool": "list_boq", "sent_back": None}]
    with client.app.state.sessions() as session:  # Omar's earlier turn on another AI, then the one he filed in
        early, later = datetime(2026, 1, 1, tzinfo=UTC), datetime(2026, 6, 1, tzinfo=UTC)
        session.add(
            TurnRecord(
                tender_id=tender_id,
                staff_id=omar_id,
                model="model-old",
                started_at=early,
                ended="out_of_steps",
                calls=[],
                input_tokens=400,
                output_tokens=100,
            )
        )
        session.add(
            TurnRecord(
                tender_id=tender_id,
                staff_id=omar_id,
                model="model-a",
                started_at=later,
                ended="done",
                calls=calls,
                input_tokens=1000,
                output_tokens=200,
            )
        )
        session.commit()
    decide(client, tender_id, rania_id, excavation["id"], True, "Quantity matches row 2.")
    decide(client, tender_id, rania_id, slab["id"], False, "Use the unit as printed.")

    usage = client.get("/ai/usage").json()
    score = {"calls": 0, "calls_sent_back": 0, "accepted": 0, "sent_back": 0}
    assert usage["models"] == [  # the fact still with the Manager counts neither way
        {"model": "model-a", "turns": 1, "finished": 1, **score, "calls": 2, "calls_sent_back": 1, "accepted": 1,
         "sent_back": 1, "tokens": 1200},
        {"model": "model-old", "turns": 1, "finished": 0, **score, "tokens": 500},
    ]  # fmt: skip
    assert usage["tenders"] == [{"tender_id": tender_id, "name": "Synthetic school", "tokens": 1700}]


def test_work_filed_before_any_of_its_makers_turns_counts_for_no_ai(client, office_with_work):
    tender_id, rania_id, omar_id = office_with_work
    excavation, _ = client.get(f"/tenders/{tender_id}/boq").json()["items"]
    with client.app.state.sessions() as session:  # Omar's first recorded turn began after he filed his work
        later = datetime(2099, 1, 1, tzinfo=UTC)
        session.add(TurnRecord(tender_id=tender_id, staff_id=omar_id, model="model-a", started_at=later, ended="done",
                               calls=[], input_tokens=10, output_tokens=5))  # fmt: skip
        session.commit()
    decide(client, tender_id, rania_id, excavation["id"], True, "Quantity matches row 2.")
    [score] = client.get("/ai/usage").json()["models"]
    assert (score["model"], score["accepted"], score["sent_back"]) == ("model-a", 0, 0)


def every_kind(client, tender) -> dict[str, str]:
    """Omar files one of each kind of work the Manager reviews, besides BOQ lines and facts: a scale and a
    measurement on the drawing, a quoted rate and a web price, markups, a checklist item and its draft, an enquiry
    and a quote recommendation. Their references, by kind."""
    tender_id, _, omar, gulf, _ = tender
    package_id = test_subcontract.two_quotes(client, tender)
    upload(client, tender_id, {"A-102.pdf": PLAN, "Conditions.pdf": test_estimate.CONDITIONS})
    docs = read_all(client, tender_id)
    plan, conditions = docs["A-102.pdf"]["id"], docs["Conditions.pdf"]["id"]
    with client.app.state.sessions() as session:
        web = WebPage(url="https://prices.example/rebar", title="Rebar prices", text="Rebar B500B 2,350 SAR per t")
        session.add(web)
        session.flush()
        scale = takeoff.set_scale(session, tender_id, omar, plan, 1, [[85.04, 141.73], [481.89, 141.73]], 40, "40.00")
        wall = [YARD[0], YARD[1]]
        measured = takeoff.measure(
            session, tender_id, omar, plan, 1, "length", "Boundary wall", wall, "m2", Decimal("1.2"), None
        )
        quoted = estimate.propose_rate(
            session, tender_id, omar, "3.1", "quote", "Gulf's price.", Decimal(17), document_id=gulf, page=1,
            quote="3.1 Excavation 17.00"
        )  # fmt: skip
        priced = "Delivered to site; the supplier's price includes cutting and bending."
        web_rate = estimate.propose_rate(
            session, tender_id, omar, "4.3", "web", priced, Decimal(2350), quote=web.text, web_page_id=web.id
        )
        site = [estimate.PreliminaryIn(item="Site office", quantity=3, unit="month", rate=5000)]
        markups = estimate.propose_markups(
            session, tender_id, omar, site, Decimal("0.06"), Decimal("0.05"), Decimal(0), "Three months on site."
        )
        clause = "VAT at 15% shall be shown separately"
        wanted = submission.RequirementIn(
            section="Commercial", title="VAT shown separately", document_id=conditions, page=1, quote=clause
        )
        submission.add_requirements(session, tender_id, omar, [wanted])
        requirement = submission.find_requirement(session, tender_id, "VAT shown separately")
        draft = submission.draft(session, requirement, omar, "VAT statement", "VAT at 15% from [date].")
        package = session.get(Package, package_id)
        enquiry = subcontract.draft_enquiry(
            session, package, subcontract.find_company(session, "Najd Contracting"), omar, "Groundworks enquiry",
            "Please price 3.1 and 6.3."
        )  # fmt: skip
        subcontract.recommend(
            session, package, omar, subcontract.find_company(session, "Najd Contracting"), "Cheapest."
        )
        session.commit()
        return {
            "scale": f"scale {scale.id[:8]}",
            "measurement": f"measurement {measured.id[:8]}",
            "quoted": f"rate {quoted.id[:8]}",
            "web": f"rate {web_rate.id[:8]}",
            "markups": f"markups {markups.id[:8]}",
            "checklist": f"checklist {requirement.id[:8]}",
            "draft": f"draft {draft.id[:8]}",
            "enquiry": f"enquiry {enquiry.id[:8]}",
            "recommendation": f"recommendation {package_id[:8]}",
        }


def test_every_kind_of_work_comes_to_the_manager_with_what_to_check_it_against(client, subcontracted):
    tender_id, rania_id = subcontracted[:2]
    refs = every_kind(client, subcontracted)
    queue = {w["ref"]: w["line"].split(" · ", 2)[2] for w in client.get(f"/tenders/{tender_id}/review").json()}
    assert {kind: queue[ref] for kind, ref in refs.items()} == {
        "scale": "scale of A-102.pdf, page 1: about 1:286",
        "measurement": "“Boundary wall”: 48 m2 (not linked to a BOQ line)",
        "quoted": "3.1: 17.00 per m3 (quote, unit rate)",
        "web": "4.3: 2350.00 per t (web, unit rate)",
        "markups": "preliminaries 15,000.00 in 1 items, overheads 6.0%, profit 5.0%",
        "checklist": "“VAT shown separately” (Commercial) from Conditions.pdf, page 1",
        "draft": "“VAT statement” for “VAT shown separately”",
        "enquiry": "to Najd Contracting for Groundworks: “Groundworks enquiry”",
        "recommendation": "Najd Contracting for Groundworks: Cheapest.",
    }
    with client.app.state.sessions() as session:
        shown = {kind: reviews.details(session, reviews.find(session, tender_id, ref)) for kind, ref in refs.items()}
    assert shown["scale"] == (
        "Set from “40.00” as 40.0 m between [[85.04, 141.73], [481.89, 141.73]] (page points) on A-102.pdf, page 1. "
        "Check it against the scale printed in the title block."
    )
    assert shown["measurement"].startswith(
        "A length of 2 points on A-102.pdf, page 1: [[85.04, 170.08], [481.89, 170.08]]. Multiplied by 1.2"
    )
    assert shown["quoted"] == (
        "3.1: Excavation · 1,240 m3. Basis: quote.\nQuoted on Gulf.pdf, page 1: “3.1 Excavation 17.00”\n"
        "Note: Gulf's price."
    )
    assert shown["web"].splitlines()[1] == (
        f"On the web page Rebar prices (https://prices.example/rebar), read {datetime.now(UTC):%d %b %Y}: “Rebar "
        "B500B 2,350 SAR per t”"
    )
    # net: 1,240 × 17.00 + 28.1 × 2,350.00 + 980 × 38.00 = 21,080.00 + 66,035.00 + 37,240.00
    assert shown["markups"] == (
        "On a net cost of 124,355.00:\n- Site office: 3 month × 5000 = 15000.00\nPreliminaries 15,000.00 (12.1% of "
        "net), overheads 8,361.30, profit 7,385.82, adjustment 0.00; price 155,102.12.\nNote: Three months on site."
    )
    assert shown["checklist"] == "Required by Conditions.pdf, page 1: “VAT at 15% shall be shown separately”"
    assert shown["draft"] == "VAT at 15% from [date]."
    assert shown["enquiry"] == "Subject: Groundworks enquiry\nPlease price 3.1 and 6.3."
    assert shown["recommendation"] == (
        "1. Najd Contracting: quoted 20460.00, exclusions 0, levelled 57700.00\n"
        "2. Gulf Groundworks: quoted 55380.00, exclusions 5000, levelled 60380.00\nRecommendation: Cheapest."
    )

    listed = tools.review_queue(fake_turn(client, tender_id, rania_id))
    lines = {line.split(" · ")[0]: line for line in listed.splitlines()[1:]}
    assert listed.startswith(f"{len(lines)} waiting for your review:\n")
    assert lines[refs["draft"]].endswith(" · Quantix found 1 blocker")  # the date still to fill in
    assert lines[refs["recommendation"]].endswith(" · Quantix found 1 warning")  # Najd left out 6.3
    assert " · Quantix found" not in lines[refs["enquiry"]]


@pytest.mark.xfail(strict=True, reason="Bug: a measurement's multiplier is shown as stored, 1.2000 m")
def test_the_manager_reads_a_measurements_multiplier_as_it_was_given(client, subcontracted):
    tender_id = subcontracted[0]
    refs = every_kind(client, subcontracted)
    with client.app.state.sessions() as session:
        shown = reviews.details(session, reviews.find(session, tender_id, refs["measurement"]))
    assert shown.endswith(". Multiplied by 1.2 m.")


def test_the_manager_sends_back_a_checklist_item_an_enquiry_and_a_recommendation_to_their_maker(client, subcontracted):
    tender_id, rania_id, omar_id = subcontracted[:3]
    refs = every_kind(client, subcontracted)
    unit = "Take each checklist item from the clause that requires it, quoted whole."
    with client.app.state.sessions() as session:
        session.add(CompanyRule(topic="Subcontract", text="Name the drawings and the BOQ items every enquiry prices."))
        verdict = reviews.Verdict
        report = reviews.review(
            session,
            client.app.state.home,
            tender_id,
            session.get(Staff, rania_id),
            [
                verdict(record=refs["checklist"], accept=False, note="Quote the whole clause 4.2.", lesson=unit),
                verdict(
                    record=refs["enquiry"],
                    accept=False,
                    note="Name the drawings it prices.",
                    lesson="Name the drawings and the BOQ items each enquiry prices.",
                ),  # fmt: skip
                verdict(record=refs["recommendation"], accept=False, note="Level the dewatering first."),
                verdict(record=refs["markups"], accept=True, note="Checked the site office rate."),
                verdict(record=refs["scale"], accept=True, note="Checked the 40.00 m dimension.", lesson=unit),
            ],
            autonomous=False,
        )
        session.commit()
    assert report.splitlines() == [
        "Accepted 2 (waiting for the engineer). Sent back 3. Once your review is done, tell the engineer with "
        "message_engineer what now waits for their approval and where: 1 set of markups on the Estimate screen, "
        "under Markups and summary; 1 scale on the Takeoff screen.",
        f"Lesson kept: the whole office follows “{unit}” from now on.",
        "Lesson not kept: it is already a company rule: “Name the drawings and the BOQ items every enquiry prices.”",
        f"Lesson not kept for {refs['scale']}: a lesson comes from work that needed correcting. Give it when you send "
        "something back, or when you accept its corrected version.",
    ]
    team = client.get(f"/tenders/{tender_id}/messages", params={"channel": "team"}).json()
    assert [m["text"] for m in team if m["sender"] == rania_id] == [
        "Omar, I sent back the checklist item “VAT shown separately”; I removed it: Quote the whole clause 4.2.",
        "Omar, I sent back the enquiry “Groundworks enquiry”: Name the drawings it prices.",
        "Omar, I sent back your recommendation for Groundworks: Level the dewatering first.",
    ]
    [lesson] = client.get(f"/tenders/{tender_id}/lessons").json()
    assert (lesson["topic"], lesson["source"]) == ("Submission", "the checklist item “VAT shown separately”")
    with client.app.state.sessions() as session:
        assert submission.requirements(session, tender_id) == []
        [package] = subcontract.packages(session, tender_id)
        assert (package.recommended_quote_id, package.recommendation, package.recommended_by) == (None, None, None)
        [enquiry] = subcontract.enquiries(session, package.id)
        assert (enquiry.status, enquiry.reviewed_by, enquiry.review_note) == (
            "rejected",
            rania_id,
            "Name the drawings it prices.",
        )
        waiting = [p.kind for p in reviews.pending(session, tender_id)]
        assert not {"checklist", "enquiry", "recommendation"} & set(waiting)


@pytest.mark.xfail(
    strict=True,
    reason="Bug: a checklist item, enquiry or recommendation sent back gives its maker no task to redo it",
)
def test_work_of_every_kind_sent_back_is_its_makers_task_to_redo(client, subcontracted):
    """The review tool tells the Manager that what he sends back becomes its maker's task, so not to assign it too:
    for BOQ lines, rates and the other reviewed records it does."""
    tender_id, rania_id, omar_id = subcontracted[:3]
    refs = every_kind(client, subcontracted)
    with client.app.state.sessions() as session:
        verdicts = [
            reviews.Verdict(record=refs[kind], accept=False, note="Correct it as the team room says.")
            for kind in ("checklist", "enquiry", "recommendation")
        ]
        reviews.review(session, client.app.state.home, tender_id, session.get(Staff, rania_id), verdicts, False)
        session.commit()
        tasks = [t.title for t in session.query(Task).filter_by(staff_id=omar_id, status="open")]
    assert len(tasks) == 3


def test_what_cannot_be_reopened_escalated_or_found_is_refused_plainly(client, office_with_work):
    tender_id, rania_id, omar_id = office_with_work
    excavation, slab = client.get(f"/tenders/{tender_id}/boq").json()["items"]
    missing = "0" * 32
    refused = client.post(f"/records/checklist/{excavation['id']}/reopen", json={"reason": "Check it."})
    assert (refused.status_code, refused.json()["detail"]) == (
        400,
        "Only BOQ lines, facts, scales, measurements, layer maps, queries, rates, markups and drafts can be reopened.",
    )
    assert client.post(f"/records/boq/{missing}/reopen", json={"reason": "Check it."}).status_code == 404
    assert client.get(f"/records/boq/{missing}/findings").status_code == 404
    assert client.get(f"/records/tender/{excavation['id']}/findings").status_code == 404
    for path in (f"/tenders/{missing}/review", f"/tenders/{missing}/audit", f"/tenders/{missing}/lessons"):
        assert client.get(path).json() == {"detail": "Tender not found."}
    assert client.patch(f"/lessons/{missing}", json={"status": "kept"}).status_code == 404

    conditions = read_all(client, tender_id)["Conditions.pdf"]["id"]
    problem = "The bill measures the excavation net, but the conditions ask for bulked volumes."
    where = [reviews.Source(document_id=conditions, page=1, what="The measurement clause")]
    with client.app.state.sessions() as session:
        rania = session.get(Staff, rania_id)
        excavation_ref = f"boq {excavation['id'][:8]}"
        for ref, words, sources, suggestions, message in (
            ("rate 12345678", problem, where, ["Bulk it"], "nothing with that reference is waiting for your review"),
            ("checklist 1234", problem, where, ["Bulk it"], "nothing with that reference is waiting for your review"),
            (
                excavation_ref,
                "Wrong volume.",
                where,
                ["Bulk it"],
                "Say what is wrong and why the office can't settle it.",
            ),
            (excavation_ref, problem, where, [" "], "Suggest 1 to 4 corrections for the engineer to choose from"),
            (excavation_ref, problem, where, ["a", "b", "c", "d", "e"], "Suggest 1 to 4 corrections"),
            (
                excavation_ref,
                problem,
                [reviews.Source(document_id=missing, page=1, what="The clause")],
                ["Bulk it"],
                "The clause: no document of this tender has that id; use list_documents",
            ),
        ):
            with pytest.raises(ValueError, match=re.escape(message)):
                reviews.escalate(session, tender_id, rania, ref, words, sources, suggestions)

        approved = session.get(boq.BoqItem, slab["id"])
        boq.approve(session, approved)  # the engineer approved it
        with pytest.raises(ValueError, match=re.escape(
            "The clause: no document of this tender has that id; use list_documents. For approved work you can "
            "leave sources out: Quantix's finding says where."
        )):  # fmt: skip
            bad = [reviews.Source(document_id=missing, page=1, what="The clause")]
            reviews.escalate(session, tender_id, rania, f"boq {slab['id'][:8]}", problem, bad, ["Re-measure it"])
        decision = reviews.escalate(session, tender_id, rania, f"boq {slab['id'][:8]}", problem, [], ["Re-measure it"])
        assert decision.options == ["Re-measure it", reviews.KEEP_APPROVED]
        session.commit()
        decision_id = decision.id

    # the engineer reopened the line before answering: their answer doesn't send it back a second time
    assert (
        client.post(f"/records/boq/{slab['id']}/reopen", json={"reason": "Use the unit as printed."}).status_code == 204
    )
    client.post(f"/decisions/{decision_id}/answer", json={"answer": "Re-measure it"})
    with client.app.state.sessions() as session:
        redo = [t for t in office.all_tasks(session, tender_id) if t.staff_id == omar_id]
        assert [(t.title, t.brief) for t in redo] == [("Redo BOQ item 4.2", "Use the unit as printed.")]
        assert session.get(boq.BoqItem, slab["id"]).reason == "Use the unit as printed."
        assert reviews.escalation(session, slab["id"]) is None  # answered
