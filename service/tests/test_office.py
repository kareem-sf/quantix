"""The office at work, end to end: real runtime, tools and records, with only the model scripted."""

import re
import threading
import time
from contextlib import contextmanager
from datetime import UTC, datetime, timedelta
from types import SimpleNamespace

import pytest
from conftest import TOKEN
from fastapi.testclient import TestClient
from pydantic_ai.exceptions import ModelAPIError
from pydantic_ai.messages import (
    ModelResponse,
    RetryPromptPart,
    TextPart,
    ThinkingPart,
    ToolCallPart,
    ToolReturnPart,
    UserPromptPart,
)
from pydantic_ai.models.function import DeltaThinkingPart, DeltaToolCall, FunctionModel
from test_documents import PDF, make_pdf, read_all, upload

from quantix import settings
from quantix.api.app import create_app
from quantix.office import agents, runtime
from quantix.office.models import ENGINEER, TEAM, Staff, TurnRecord


def prompt_of(messages) -> str:
    return next(p.content for m in messages for p in m.parts if isinstance(p, UserPromptPart))


def returns(messages) -> list[str]:
    return [str(p.content) for m in messages for p in m.parts if isinstance(p, ToolReturnPart)]


def call(tool: str, **args) -> ModelResponse:
    return ModelResponse(parts=[ToolCallPart(tool, args)])


DONE = ModelResponse(parts=[TextPart("That's everything for now.")])


def scripted(brain) -> FunctionModel:
    """The brain as a model the office can stream from, as it does from a real service."""

    async def stream(messages, info):
        response = brain(messages, info)
        if not response.parts:
            yield ""  # an empty answer, as models sometimes give when they have nothing to add
        for index, part in enumerate(response.parts):
            if isinstance(part, TextPart):
                yield part.content
            elif isinstance(part, ThinkingPart):
                yield {index: DeltaThinkingPart(content=part.content)}
            elif isinstance(part, ToolCallPart):
                yield {index: DeltaToolCall(part.tool_name, part.args_as_json_str(), tool_call_id=part.tool_call_id)}

    return FunctionModel(brain, stream_function=stream)


def office_brain(messages, info) -> ModelResponse:
    """Plays every person in the office, deciding from who it is and what it has already done this turn."""
    if info.output_tools:  # creating the Tender Manager's persona
        return call(
            info.output_tools[0].name,
            name="Rania Farouk",
            discipline="Civil engineering",
            experience_years=19,
            background="Led tenders for schools and clinics across the Gulf.",
            working_style="Reads the conditions first, then plans the team.",
            opinions="Distrusts quantities nobody has measured.",
            voice="Direct and brief.",
        )
    who, prompt, done = info.instructions, prompt_of(messages), returns(messages)
    if "You are Rania Farouk" in who:
        if "About “Tender security wording”" in prompt:
            return [call("post_to_team", text="The engineer confirmed the 1% bond."), DONE][len(done)]
        if "Finished:" in prompt:
            question = "The conditions ask for a 1% tender security. Shall we price the bond at 1%?"
            options = ["Yes, 1%", "Ask the client first"]
            steps = [call("ask_engineer", title="Tender security wording", question=question, options=options), DONE]
            return steps[len(done)]
        steps = [
            call(
                "hire",
                name="Omar Haddad",
                role="Quantity Surveyor",
                discipline="Quantity surveying",
                experience_years=11,
                background="Measured schools and warehouses for Gulf contractors.",
                working_style="Measures twice.",
                opinions="Never trusts a BOQ quantity without a drawing.",
                voice="Plain and exact.",
                work=["documents", "takeoff"],
            ),
            call(
                "assign_task", staff_name="Omar", title="find the tender security clause", brief="Read the conditions."
            ),
            call("message_engineer", text="I've asked Omar to find the tender security clause."),
            DONE,
        ]
        return steps[len(done)]
    if "You are Omar Haddad" in who:
        if not done:
            return call("search_documents", query="tender security")
        if len(done) == 1:
            return call("read_page", document_id=done[0].split(" · ")[0], page=1)
        if len(done) == 2:
            task_id = re.search(r"- (\w+): find the tender security clause", prompt).group(1)
            return call(
                "complete_task",
                task_id=task_id,
                result="Tender security is one percent (Conditions.pdf, page 1).",
                only_reported=True,  # finding a clause files nothing
            )
        return DONE
    return DONE


def wait_for(condition, client, tender_id, seconds=15.0):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        office = client.get(f"/tenders/{tender_id}/office").json()
        if office["state"] != "working" and condition(office):
            return office
        time.sleep(0.05)
    raise AssertionError(f"The office did not get there in time: {client.get(f'/tenders/{tender_id}/office').json()}")


def manager_accepts(client, tender_id, autonomous=False) -> str:
    """The Tender Manager accepts everything waiting for his review, as a Manager who checked it would."""
    from quantix.office import records as office_records
    from quantix.review import records as reviews

    with client.app.state.sessions() as session:
        manager = office_records.manager(session, tender_id) or office_records.hire(
            session, tender_id, "Rania Farouk", "Tender Manager", {}, is_manager=True
        )
        reason = "Checked each warning against the documents."
        verdicts = [
            reviews.Verdict(record=p.ref, accept=True, note="Checked it against its source.", warnings_reason=reason)
            for p in reviews.pending(session, tender_id)
        ]
        report = reviews.review(session, client.app.state.home, tender_id, manager, verdicts, autonomous)
        session.commit()
        return report


@pytest.fixture
def office(client, tmp_path):
    """A tender with its package read and the office's AI set to the scripted brain."""
    tender_id = client.post("/tenders", json={"name": "Synthetic school"}).json()["id"]
    upload(client, tender_id, {"Conditions.pdf": PDF})
    read_all(client, tender_id)
    settings.save(tmp_path, office_ai={"connection_id": "scripted", "model": "office-brain"})

    def use(brain):
        client.app.state.office.model = lambda: scripted(brain)

    use(office_brain)
    return tender_id, use


def team_room(client, tender_id):
    return client.get(f"/tenders/{tender_id}/messages", params={"channel": TEAM}).json()


def test_the_office_works_a_request_through_to_a_decision(client, office):
    tender_id, _ = office
    assert client.get(f"/tenders/{tender_id}/office").json()["staff"] == []  # nobody works until asked

    client.post(f"/tenders/{tender_id}/messages", json={"channel": TEAM, "text": "Check the tender security, please."})
    state = wait_for(lambda o: o["waiting"] == 1, client, tender_id)

    rania, omar = state["staff"]
    assert (rania["name"], rania["role"], rania["is_manager"]) == ("Rania Farouk", "Tender Manager", True)
    assert rania["profile"]["opinions"] == "Distrusts quantities nobody has measured."
    assert (omar["name"], omar["role"]) == ("Omar Haddad", "Quantity Surveyor")

    said = [(m["sender"], m["kind"], m["text"]) for m in team_room(client, tender_id)]
    assert ("engineer", "message", "Check the tender security, please.") in said
    assert (rania["id"], "task", "Rania asked Omar to find the tender security clause") in said
    assert any(s == omar["id"] and t.startswith("Finished: find the tender security clause") for s, _, t in said)
    assert not any("stopped with" in t for _, _, t in said)  # Omar finished his task, so there is nothing to note

    [task] = client.get(f"/tenders/{tender_id}/tasks").json()
    assert (task["status"], task["result"]) == ("done", "Tender security is one percent (Conditions.pdf, page 1).")
    chat = client.get(f"/tenders/{tender_id}/messages", params={"channel": rania["id"]}).json()
    assert [m["text"] for m in chat] == ["I've asked Omar to find the tender security clause."]

    [decision] = client.get(f"/tenders/{tender_id}/decisions").json()
    assert decision["options"] == ["Yes, 1%", "Ask the client first"] and decision["raised_by"] == rania["id"]

    client.post(f"/decisions/{decision['id']}/answer", json={"answer": "Yes, 1%"})
    wait_for(lambda o: o["waiting"] == 0, client, tender_id)
    wait_for(lambda o: "The engineer confirmed the 1% bond." in str(team_room(client, tender_id)), client, tender_id)


def test_stop_halts_the_office_until_the_engineer_writes(client, office):
    tender_id, use = office
    calls = []

    def busy(messages, info):
        calls.append(1)
        if info.output_tools:
            return office_brain(messages, info)
        return call("list_documents")

    use(busy)
    client.post(f"/tenders/{tender_id}/messages", json={"channel": TEAM, "text": "Go."})
    while len(calls) < 3:
        time.sleep(0.01)
    assert client.post(f"/tenders/{tender_id}/office/stop").status_code == 204
    wait_for(lambda o: o["state"] == "paused", client, tender_id)
    settled = len(calls)
    time.sleep(0.3)
    assert len(calls) == settled


def test_an_autonomous_office_decides_for_itself(client, office, tmp_path):
    tender_id, _ = office
    settings.save(tmp_path, office_mode="autonomous")
    client.post(f"/tenders/{tender_id}/messages", json={"channel": TEAM, "text": "Check the tender security, please."})
    wait_for(lambda o: len(o["staff"]) == 2 and o["staff"][1]["now"] is None, client, tender_id)
    time.sleep(0.3)
    assert client.get(f"/tenders/{tender_id}/decisions").json() == []


def test_an_ai_failure_pauses_the_office_with_a_plain_reason(client, office):
    tender_id, use = office

    class Refused(Exception):
        status_code = 401

    def refuses(messages, info):
        raise Refused()

    use(refuses)
    client.post(f"/tenders/{tender_id}/messages", json={"channel": TEAM, "text": "Hello"})
    wait_for(lambda o: o["state"] == "paused", client, tender_id)
    notice = team_room(client, tender_id)[-1]
    assert (notice["sender"], notice["kind"]) == ("office", "note")
    assert (
        notice["text"]
        == "The office stopped: The key was refused. Check it and try again. Send a message to try again."
    )


def test_the_engineer_can_only_message_people_on_the_team(client, office):
    tender_id, _ = office
    assert client.post(f"/tenders/{tender_id}/messages", json={"channel": "nobody", "text": "Hi"}).status_code == 400


def test_running_out_of_steps_carries_on_without_losing_what_was_read(client, office):
    tender_id, use = office
    document_id = client.get(f"/tenders/{tender_id}/documents").json()[0]["id"]

    def reads_too_long(messages, info):
        if info.output_tools:
            return office_brain(messages, info)
        latest = [p.content for m in messages for p in m.parts if isinstance(p, UserPromptPart)][-1]
        if "cut short" not in latest:
            return call("read_page", document_id=document_id, page=1)  # never finishes on its own
        if any(p.tool_name == "message_engineer" for m in messages for p in m.parts if isinstance(p, ToolReturnPart)):
            return DONE
        pages = sum(1 for text in returns(messages) if text.startswith("Conditions.pdf, page 1"))
        return call("message_engineer", text=f"I read {pages} pages before I ran out of steps.")

    use(reads_too_long)
    client.post(f"/tenders/{tender_id}/messages", json={"channel": TEAM, "text": "Review the package."})

    def chat_of(office):  # the office looks idle for a moment between a turn cut short and the one carrying it on
        people = office["staff"]
        return people and client.get(f"/tenders/{tender_id}/messages", params={"channel": people[0]["id"]}).json()

    state = wait_for(chat_of, client, tender_id)
    [rania] = state["staff"]
    chat = chat_of(state)
    assert [m["text"] for m in chat] == ["I read 12 pages before I ran out of steps."]
    assert rania["now"] is None


def timed_out() -> ModelAPIError:
    try:
        raise type("APITimeoutError", (Exception,), {})()
    except Exception as cause:
        try:
            raise ModelAPIError("scripted", "Request timed out.") from cause
        except ModelAPIError as error:
            return error


def test_a_passing_ai_failure_is_retried_with_the_turn_so_far(client, office, monkeypatch):
    tender_id, use = office
    monkeypatch.setattr(runtime, "RETRY_WAIT", 0)
    failed = []

    def flaky(messages, info):
        if info.output_tools:
            return office_brain(messages, info)
        if not returns(messages):
            return call("list_documents")
        if not failed:
            failed.append(1)
            raise timed_out()
        if any(p.tool_name == "message_engineer" for m in messages for p in m.parts if isinstance(p, ToolReturnPart)):
            return DONE
        return call("message_engineer", text=f"Carried on with {len(returns(messages))} earlier result.")

    use(flaky)
    client.post(f"/tenders/{tender_id}/messages", json={"channel": TEAM, "text": "Review the package."})

    def chat_of(office):  # the office looks idle for a moment between the failed turn and the one retrying it
        people = office["staff"]
        return people and client.get(f"/tenders/{tender_id}/messages", params={"channel": people[0]["id"]}).json()

    chat = chat_of(wait_for(chat_of, client, tender_id))
    assert [m["text"] for m in chat] == ["Carried on with 1 earlier result."]


def test_the_office_pauses_when_the_ai_keeps_failing(client, office, monkeypatch):
    tender_id, use = office
    monkeypatch.setattr(runtime, "RETRY_WAIT", 0)

    def down(messages, info):
        if info.output_tools:
            return office_brain(messages, info)
        raise timed_out()

    use(down)
    client.post(f"/tenders/{tender_id}/messages", json={"channel": TEAM, "text": "Review the package."})
    wait_for(lambda o: o["state"] == "paused", client, tender_id)
    assert team_room(client, tender_id)[-1]["text"].startswith(
        "The office stopped: The AI service took too long to answer."
    )


def test_a_question_the_office_gave_up_on_is_answered_when_it_carries_on(client, office, monkeypatch):
    """The real tender: the AI service went down mid-answer, the office paused, and the question was left as read."""
    tender_id, use = office
    monkeypatch.setattr(runtime, "RETRY_WAIT", 0)

    def down(messages, info):
        if info.output_tools:
            return office_brain(messages, info)
        if not returns(messages):
            return call("list_documents")  # partway into the answer
        raise timed_out()

    use(down)
    client.post(f"/tenders/{tender_id}/messages", json={"channel": TEAM, "text": "Is the markup a percentage?"})
    paused = wait_for(lambda o: o["state"] == "paused", client, tender_id)
    assert paused["notice"].startswith("The office stopped: The AI service took too long to answer.")
    assert [m["now"] for m in paused["staff"]] == [None]  # nobody is doing anything while it is stopped
    failed = [t for t in turn_records(client, tender_id) if t.ended == "ai_failed"]
    assert len(failed) == runtime.RETRIES + 1

    told = []

    def back(messages, info):
        told.append(prompt_of(messages))
        return DONE

    use(back)
    client.post(f"/tenders/{tender_id}/messages", json={"channel": TEAM, "text": "Are you there?"})
    wait_for(lambda o: o["state"] == "idle" and told, client, tender_id)
    new = told[0].split("New for you:")[1]
    assert "Is the markup a percentage?" in new and "Are you there?" in new
    assert client.get(f"/tenders/{tender_id}/office").json()["notice"] is None


def test_each_turn_keeps_its_thinking_notes_and_tool_calls_for_the_chat(client, office):
    tender_id, use = office

    def thinks(messages, info):
        if info.output_tools:
            return office_brain(messages, info)
        done = [p for m in messages for p in m.parts if isinstance(p, ToolReturnPart | RetryPromptPart)]
        if not done:
            return ModelResponse(
                parts=[
                    ThinkingPart("The engineer wants the package reviewed."),
                    TextPart("First I'll see which documents we have."),
                    ToolCallPart("list_documents", {}),
                ]
            )
        return [call("read_page", document_id="nope", page=1), DONE][len(done) - 1]

    use(thinks)
    client.post(f"/tenders/{tender_id}/messages", json={"channel": TEAM, "text": "Review the package."})
    state = wait_for(lambda o: o["staff"] and turn_records(client, tender_id), client, tender_id)
    [turn] = client.get(f"/tenders/{tender_id}/turns", params={"staff_id": state["staff"][0]["id"]}).json()
    assert (turn["running"], turn["ended"], turn["steps"]) == (False, "done", 6)
    assert "model" not in turn and "input_tokens" not in turn  # no provider details in the chat

    brief, thinking, note, listed, read, done = client.get(f"/turns/{turn['id']}").json()["log"]
    assert brief["kind"] == "brief" and "Review the package." in brief["text"]
    assert (thinking["kind"], thinking["text"]) == ("thinking", "The engineer wants the package reviewed.")
    assert (note["kind"], note["text"]) == ("note", "First I'll see which documents we have.")
    assert (listed["tool"], listed["args"], listed["sent_back"]) == ("list_documents", "{}", None)
    assert "Conditions.pdf" in listed["result"]
    assert (read["tool"], read["result"]) == ("read_page", None)
    assert read["sent_back"] == "No document has that id. Use list_documents or search_documents to find its id."
    assert (done["kind"], done["text"]) == ("note", "That's everything for now.")
    assert client.get("/turns/999999").status_code == 404


def test_the_chat_follows_a_turn_while_it_runs(client, office, monkeypatch):
    tender_id, _ = office
    monkeypatch.setattr(agents, "SAVE_EVERY", 0)
    carry_on = threading.Event()

    async def stream(messages, info):
        if not returns(messages):
            yield {0: DeltaToolCall("list_documents", "{}", tool_call_id="list")}
        else:
            yield "Reading the conditions next."
            carry_on.wait(10)  # the AI is still writing
            yield " Done."

    client.app.state.office.model = lambda: FunctionModel(office_brain, stream_function=stream)  # the persona
    client.post(f"/tenders/{tender_id}/messages", json={"channel": TEAM, "text": "Review the package."})
    deadline = time.monotonic() + 10
    while time.monotonic() < deadline:
        turns = client.get(f"/tenders/{tender_id}/turns").json()
        if turns and turns[-1]["steps"] == 3:
            break
        time.sleep(0.05)
    [turn] = turns
    assert turn["running"] and turn["ended_at"] is None
    log = client.get(f"/turns/{turn['id']}").json()["log"]
    assert [s["kind"] for s in log] == ["brief", "tool", "note"]
    assert log[-1]["text"] == "Reading the conditions next." and "Conditions.pdf" in log[-2]["result"]

    carry_on.set()
    wait_for(lambda o: o["state"] == "idle", client, tender_id)
    [turn] = client.get(f"/tenders/{tender_id}/turns").json()
    assert not turn["running"] and turn["ended"] == "done"
    assert client.get(f"/turns/{turn['id']}").json()["log"][-1]["text"] == "Reading the conditions next. Done."


def test_a_placeholder_profile_is_sent_back():
    from pydantic import ValidationError

    from quantix.office.tools import Persona

    with pytest.raises(ValidationError) as caught:  # what a weak model wrote for the real package's Manager
        Persona(
            name="Salem Al Suwaidi",
            discipline="Tender management",
            experience_years=50,
            background="Character_Overview",
            working_style="Runs his desk like a site command post.",
            opinions="Do not touch the \u8fdb\u53d6 rates.",
            voice="Blunt and dry.",
        )
    assert sorted(e["loc"][0] for e in caught.value.errors()) == ["background", "experience_years", "opinions"]


def test_an_ai_that_cannot_read_images_is_not_shown_drawings(client, office):
    tender_id, use = office
    document_id = client.get(f"/tenders/{tender_id}/documents").json()[0]["id"]
    seen: list[str] = []

    def looks(messages, info):
        if info.output_tools:
            return office_brain(messages, info)
        seen[:] = [t.name for t in info.function_tools] + [info.instructions]
        return DONE

    use(looks)
    client.app.state.office.sees_images = lambda: False
    client.post(f"/tenders/{tender_id}/messages", json={"channel": TEAM, "text": "Look at the conditions."})
    wait_for(lambda o: o["staff"] and seen, client, tender_id)
    assert "read_page" in seen and not {"view_page", "find_on_page", "set_scale", "measure"} & set(seen)
    assert "The office's AI can't read images, so you can't look at PDF drawings or measure on them." in seen[-1]
    assert "CAD drawings (DWG and DXF) need no looking" in seen[-1] and "view_drawing" not in seen
    assert document_id  # the conditions are there to read as text


def test_a_person_asks_one_question_at_a_time(client, office):
    tender_id, use = office
    replies: list[str] = []

    def asks_twice(messages, info):
        if info.output_tools:
            return office_brain(messages, info)
        done = returns(messages)
        replies[:] = done
        steps = [
            call("ask_engineer", title="Zero-quantity lines", question="Price them?", options=["Yes", "No"]),
            call("ask_engineer", title="Zero-quantity lines, again", question="Price them?", options=["Yes", "No"]),
            DONE,
        ]
        return steps[len(done)]

    use(asks_twice)
    client.post(f"/tenders/{tender_id}/messages", json={"channel": TEAM, "text": "Review the BOQ."})
    wait_for(lambda o: o["waiting"] == 1 and len(replies) == 2, client, tender_id)
    assert replies[1] == (
        "Your question “Zero-quantity lines” is still waiting for the engineer. Ask the next one after."
    )
    assert len(client.get(f"/tenders/{tender_id}/decisions").json()) == 1


def test_an_empty_or_failing_reply_ends_the_turn_not_the_office(client, office):
    tender_id, use = office
    calls = []

    def stumbles(messages, info):
        if info.output_tools:
            return office_brain(messages, info)
        calls.append(1)
        if len(calls) <= 3:
            return call("read_page", document_id="no-such-document", page=1)  # fails every time
        return ModelResponse(parts=[])  # then answers with nothing at all

    use(stumbles)
    client.post(f"/tenders/{tender_id}/messages", json={"channel": TEAM, "text": "Review the package."})
    state = wait_for(lambda o: o["staff"] and len(calls) >= 4, client, tender_id)
    time.sleep(0.3)
    assert state["state"] == "idle"
    assert len(calls) == 4  # three failing calls, then the empty answer ends the turn: nobody prompts them again
    assert not [m for m in team_room(client, tender_id) if m["text"].startswith("The office stopped")]


def test_everyone_sees_what_the_engineer_decided(client, office):
    from quantix.office import agents
    from quantix.office import records as office_records

    tender_id, _ = office
    with client.app.state.sessions() as session:
        salem = office_records.hire(session, tender_id, "Salem Al Suwaidi", "Tender Manager", {}, is_manager=True)
        nora = office_records.hire(session, tender_id, "Nora Al-Otaibi", "Commercial QS", {})
        question = office_records.ask(session, tender_id, salem, "Zero-quantity lines", "Price them?", ["Yes", "No"])
        office_records.answer(session, question, "Price them rate-only.")
        subjects = "retention bond insurance currency validity programme plant labour haulage asphalt fencing lighting"
        for subject in subjects.split():  # a long tender: the first decision still counts
            later = office_records.ask(session, tender_id, salem, subject.title(), "Which?", ["A", "B"])
            office_records.answer(session, later, "A.")
        session.commit()
        brief = agents.situation(session, nora, [])
    assert "The engineer has decided:\n- Zero-quantity lines: Price them rate-only.\n- Retention: A." in brief
    assert "- Lighting: A." in brief


def test_each_turn_shows_where_things_stand_and_the_chat_so_far(client, office):
    from quantix.office import agents
    from quantix.office import records as office_records
    from quantix.office.models import ENGINEER

    tender_id, _ = office
    with client.app.state.sessions() as session:
        salem = office_records.hire(session, tender_id, "Salem Al Suwaidi", "Tender Manager", {}, is_manager=True)
        rashid = office_records.hire(session, tender_id, "Rashid Al-Ghamdi", "Estimator", {})
        office_records.assign(session, tender_id, salem, rashid, "reset the drawing scales", "From a dimension.")
        office_records.post(session, tender_id, salem.id, salem.id, "The zero lines are C.21.1 and C.7.1.2.")
        office_records.post(session, tender_id, ENGINEER, salem.id, "One combined query.")
        session.commit()
        brief = agents.situation(session, salem, [])
    assert (
        "Where the tender stands:\n- BOQ: 0 lines, 0 approved, 0 waiting for the engineer, 0 with the Tender " in brief
    )
    assert "- Markups: none proposed yet." in brief  # so no one reports them as accepted
    assert "Open tasks in the team:\n- Rashid: reset the drawing scales" in brief  # so no one is briefed twice
    assert (
        "Earlier in your chat with the engineer:\n- You: The zero lines are C.21.1 and C.7.1.2.\n"
        "- Engineer: One combined query." in brief
    )


def test_a_full_team_gives_new_work_to_the_people_it_has(client, office):
    from quantix.office import records as office_records

    tender_id, use = office
    with client.app.state.sessions() as session:
        office_records.hire(session, tender_id, "Rania Farouk", "Tender Manager", {}, is_manager=True)
        for name in ("Omar Haddad", "Layla Nasser", "Sami Khoury", "Dana Aziz", "Tariq Saleh"):
            office_records.hire(session, tender_id, name, "Estimator", {})
        session.commit()
    seen: list[str] = []

    def manager(messages, info):
        seen[:] = [str(p.content) for m in messages for p in m.parts if isinstance(p, RetryPromptPart)]
        if seen or "You are Rania Farouk" not in info.instructions:
            return DONE
        profile = {"discipline": "Pricing", "experience_years": 9, "background": "Priced roads in Riyadh."}
        profile |= {"working_style": "Builds rates up.", "opinions": "Quotes lie.", "voice": "Short and plain."}
        done = [p for m in messages for p in m.parts if isinstance(p, ToolReturnPart)]
        if not done:  # with a full team, the hiring tools are loaded on request
            return call("load_capability", id="team")
        return call("hire", name="Hadi Nassar", role="Estimator", work=["pricing"], **profile)

    use(manager)
    client.post(f"/tenders/{tender_id}/messages", json={"channel": TEAM, "text": "Price the small items."})
    wait_for(lambda o: seen, client, tender_id)
    assert seen[0].startswith("The team already has 5 people: Omar Haddad (Estimator)")
    assert "Hadi Nassar" not in [m["name"] for m in client.get(f"/tenders/{tender_id}/office").json()["staff"]]


def test_an_unanswered_update_to_the_engineer_is_replaced_by_the_next(client, office):
    tender_id, use = office

    def manager(messages, info):
        if info.output_tools:
            return office_brain(messages, info)
        answered = "Noted, carry on." in prompt_of(messages)
        # a reply to the engineer rests on something the Manager looked at
        steps = [call("estimate_summary"), call("message_engineer", text="Third.", sources=["estimate_summary"])]
        steps = steps if answered else []
        steps = steps or [call("message_engineer", text="First."), call("message_engineer", text="Second.")]
        return [*steps, DONE][len(returns(messages))]

    use(manager)
    client.post(f"/tenders/{tender_id}/messages", json={"channel": TEAM, "text": "Review the package."})
    [rania] = wait_for(lambda o: o["staff"] and o["state"] == "idle", client, tender_id)["staff"]

    def chat():
        return [m["text"] for m in client.get(f"/tenders/{tender_id}/messages", params={"channel": rania["id"]}).json()]

    assert chat() == ["Second."]  # one current update, not a stack of them
    client.post(f"/tenders/{tender_id}/messages", json={"channel": rania["id"], "text": "Noted, carry on."})
    wait_for(lambda o: len(chat()) == 3, client, tender_id)
    assert chat() == ["Second.", "Noted, carry on.", "Third."]  # what the engineer answered stays


def test_someone_released_during_a_pass_does_not_take_their_turn(client, office):
    import asyncio

    from quantix.office import records as office_records

    tender_id, _ = office
    with client.app.state.sessions() as session:
        salem = office_records.hire(session, tender_id, "Salem Al Suwaidi", "Tender Manager", {}, is_manager=True)
        yousef = office_records.hire(session, tender_id, "Yousef Al-Mansouri", "Buyer", {})
        office_records.assign(session, tender_id, salem, yousef, "check the market", "Small items.")  # in his inbox
        yousef.status = "released"
        session.commit()
        yousef_id = yousef.id
    office = client.app.state.office
    assert asyncio.run(office._turn(scripted(office_brain), tender_id, yousef_id)) is False


def test_finishing_a_task_twice_says_what_is_still_open(client, office):
    from quantix.office import records as office_records

    tender_id, _ = office
    with client.app.state.sessions() as session:
        salem = office_records.hire(session, tender_id, "Salem Al Suwaidi", "Tender Manager", {}, is_manager=True)
        rashid = office_records.hire(session, tender_id, "Rashid Al-Ghamdi", "Estimator", {})
        first = office_records.assign(session, tender_id, salem, rashid, "set the scales", "From a dimension.")
        office_records.complete(session, rashid, first.id, "Done.")
        with pytest.raises(ValueError, match="You have none open: report other work in the team room."):
            office_records.complete(session, rashid, first.id, "Done again.")
        second = office_records.assign(session, tender_id, salem, rashid, "measure the areas", "Both sheets.")
        with pytest.raises(ValueError, match=f"Yours: {second.id} \\(measure the areas\\)."):
            office_records.complete(session, rashid, first.id, "Done again.")


def test_someone_who_stops_silently_with_open_tasks_is_followed_up(client, office):
    """Nothing else would wake them again, so their tasks would wait unseen: Quantix says so in the team room, which
    brings the Manager in. The notice doesn't wake the person it names."""
    from quantix.office import records as office_records

    tender_id, use = office
    with client.app.state.sessions() as session:
        salem = office_records.hire(session, tender_id, "Salem Al Suwaidi", "Tender Manager", {}, is_manager=True)
        rashid = office_records.hire(session, tender_id, "Rashid Al-Ghamdi", "Estimator", {})
        office_records.assign(session, tender_id, salem, rashid, "redraft the insurance statement", "Keep the covers.")
        session.commit()
        salem_id, rashid_id = salem.id, rashid.id
    briefings: list[str] = []

    def brain(messages, info):
        if "You are Salem" in info.instructions:
            briefings.append(prompt_of(messages))
            return DONE
        return [call("list_documents"), DONE][len(returns(messages))]  # Rashid looks, then stops

    use(brain)
    client.post(f"/tenders/{tender_id}/messages", json={"channel": TEAM, "text": "Carry on, please."})
    wait_for(lambda o: len(turn_records(client, tender_id)) >= 3, client, tender_id)

    notice = ("office", "note", "Rashid stopped with 1 open task, without filing or saying anything.")
    assert notice in [(m["sender"], m["kind"], m["text"]) for m in team_room(client, tender_id)]
    assert f"- Quantix, team room: {notice[2]}" in briefings[-1]
    assert [t.staff_id for t in turn_records(client, tender_id)] == [salem_id, rashid_id, salem_id]


def test_released_staff_leave_their_open_tasks_to_be_given_out_again(client, office):
    from quantix.office import agents, tools
    from quantix.office import records as office_records

    tender_id, _ = office
    with client.app.state.sessions() as session:
        salem = office_records.hire(session, tender_id, "Salem Al Suwaidi", "Tender Manager", {}, is_manager=True)
        nora = office_records.hire(session, tender_id, "Nora Al-Otaibi", "Commercial QS", {})
        office_records.assign(session, tender_id, salem, nora, "audit the insurance annexure", "Every limit.")
        session.commit()
        salem_id = salem.id
    state = client.app.state
    turn = tools.Turn(state.home, state.sessions, tender_id, salem_id, False, threading.Event())
    report = tools.release(SimpleNamespace(deps=turn, tool_call_id="call"), "Nora", "Her review is finished.")
    assert report == (
        "Nora has been released, leaving these tasks undone: audit the insurance annexure. Give any that still need "
        "doing to someone else with assign_task."
    )
    with client.app.state.sessions() as session:  # the Manager's list shows only what someone on the team will do
        briefing = agents.situation(session, session.get(Staff, salem_id), [])
    assert "Open tasks in the team" not in briefing and "- Nora: audit the insurance annexure" not in briefing


def test_the_manager_sets_the_due_date_the_engineer_gives_or_a_page_he_read_states(client, office):
    """He said he had set it when nothing could: now he can, the team room shows it, and the date always shows
    where it comes from. A date from the documents quotes its page; the engineer's own date stands over it."""
    from datetime import date

    from pydantic_ai import ModelRetry

    from quantix.office import records as office_records
    from quantix.office import tools

    tender_id, _ = office
    upload(
        client, tender_id, {"ITT.pdf": make_pdf([["Instructions to Tenderers", "Tenders are due by 14 October 2026."]])}
    )
    read_all(client, tender_id)
    with client.app.state.sessions() as session:
        salem = office_records.hire(session, tender_id, "Salem Al Suwaidi", "Tender Manager", {}, is_manager=True)
        session.commit()
        salem_id = salem.id
    state = client.app.state
    ctx = SimpleNamespace(
        deps=tools.Turn(state.home, state.sessions, tender_id, salem_id, False, threading.Event()), tool_call_id="call"
    )
    october = date(2026, 10, 14)
    with pytest.raises(ModelRetry, match="You haven't read ITT.pdf, page 1"):
        tools.set_due_date(ctx, october, "ITT.pdf, page 1", "Tenders are due by 14 October 2026.")
    with client.app.state.sessions() as session:
        office_records.note_opened(session, tender_id, salem_id, "page", f"{itt_id(client, tender_id)}:1")
        session.commit()
    with pytest.raises(ModelRetry, match="Quote the words on that page"):
        tools.set_due_date(ctx, october, "ITT.pdf, page 1")
    with pytest.raises(ModelRetry, match="is not on ITT.pdf, page 1"):
        tools.set_due_date(ctx, october, "ITT.pdf, page 1", "Tenders close on 14 October 2026.")
    assert (
        tools.set_due_date(ctx, october, "ITT.pdf, page 1", "due by 14 October 2026")
        == "The tender is due 14 October 2026."
    )
    tender = client.get(f"/tenders/{tender_id}").json()
    assert tender["due_date"] == "2026-10-14"
    source = {k: v for k, v in tender["due_date_source"].items() if k != "set_at"}
    assert source == {
        "basis": "document",
        "set_by": "Salem",
        "document_id": itt_id(client, tender_id),
        "document_name": "ITT.pdf",
        "page": 1,
        "quote": "due by 14 October 2026",
    }

    assert tools.set_due_date(ctx, date(2026, 9, 30), "engineer") == "The tender is due 30 September 2026."
    source = client.get(f"/tenders/{tender_id}").json()["due_date_source"]
    assert (source["basis"], source["set_by"], source["document_id"], source["quote"]) == (
        "engineer",
        "Salem",
        None,
        None,
    )
    said = team_room(client, tender_id)[-1]
    assert (said["sender"], said["text"]) == (
        salem_id,
        "Salem set the due date to 30 September 2026, as the engineer asked.",
    )
    with pytest.raises(ModelRetry, match="The engineer set the due date to 30 September 2026, and it stands"):
        tools.set_due_date(ctx, october, "ITT.pdf, page 1", "due by 14 October 2026")


def itt_id(client, tender_id) -> str:
    return next(d["id"] for d in client.get(f"/tenders/{tender_id}/documents").json() if d["name"] == "ITT.pdf")


def test_only_the_manager_brings_decisions_to_the_engineer():
    from test_review import reach

    assert "ask_engineer" in reach(is_manager=True)
    assert "ask_engineer" not in reach(is_manager=False)  # staff raise it with the Manager instead


def test_a_question_already_decided_is_not_asked_again(client, office):
    from quantix.office import records as office_records

    tender_id, _ = office
    with client.app.state.sessions() as session:
        salem = office_records.hire(session, tender_id, "Salem Al Suwaidi", "Tender Manager", {}, is_manager=True)
        first = office_records.ask(session, tender_id, salem, "Client query format", "One or two?", ["One", "Two"])
        office_records.answer(session, first, "One combined query.")
        with pytest.raises(ValueError, match="The engineer already decided “Client query format”: One combined query."):
            office_records.ask(session, tender_id, salem, "Confirm combined client query wording", "OK?", ["Yes", "No"])
        office_records.ask(session, tender_id, salem, "Retention percentage", "5% or 10%?", ["5%", "10%"])
        session.commit()


def turn_records(client, tender_id) -> list[TurnRecord]:
    with client.app.state.sessions() as session:
        query = TurnRecord.__table__.select().where(TurnRecord.tender_id == tender_id).order_by(TurnRecord.id)
        return list(session.execute(query))


@contextmanager
def restarted(home):
    """Quantix opened again on the same data folder, as after closing the app."""
    with TestClient(create_app(home, TOKEN), headers={"Authorization": f"Bearer {TOKEN}"}) as again:
        yield again


def test_each_turn_is_recorded_with_what_quantix_sent_back(client, office):
    tender_id, use = office

    def stumbles_once(messages, info):
        if info.output_tools:
            return office_brain(messages, info)
        done = [p for m in messages for p in m.parts if isinstance(p, ToolReturnPart | RetryPromptPart)]
        return [call("read_page", document_id="nope", page=1), call("list_documents"), DONE][len(done)]

    use(stumbles_once)
    client.post(f"/tenders/{tender_id}/messages", json={"channel": TEAM, "text": "Review the package."})
    wait_for(lambda o: o["staff"] and turn_records(client, tender_id), client, tender_id)
    [turn] = turn_records(client, tender_id)
    assert (turn.ended, turn.requests, turn.model) == ("done", 3, "function:stumbles_once:stream")
    missing = "No document has that id. Use list_documents or search_documents to find its id."
    assert turn.calls == [{"tool": "read_page", "sent_back": missing}, {"tool": "list_documents", "sent_back": None}]
    assert turn.input_tokens > 0 and turn.output_tokens > 0 and turn.ended_at >= turn.started_at


def test_a_stopped_office_stays_stopped_after_a_restart(client, office, tmp_path, monkeypatch):
    from quantix.office import records as office_records

    tender_id, _ = office
    asked = []
    monkeypatch.setattr(runtime, "office_model", lambda home: scripted(lambda m, i: asked.append(1) or DONE))
    with client.app.state.sessions() as session:
        office_records.hire(session, tender_id, "Rania Farouk", "Tender Manager", {}, is_manager=True)
        session.commit()
    client.post(f"/tenders/{tender_id}/office/stop")
    with client.app.state.sessions() as session:  # waiting in the Manager's inbox, as it was when Quantix closed
        office_records.post(session, tender_id, ENGINEER, TEAM, "Price the asphalt.")
        session.commit()
    client.app.state.office.close()
    with restarted(tmp_path) as again:
        time.sleep(0.3)
        assert again.get(f"/tenders/{tender_id}/office").json()["state"] == "paused"
    assert asked == []


def test_work_cut_off_by_closing_quantix_carries_on_after_a_restart(client, office, tmp_path, monkeypatch):
    tender_id, use = office
    first = client.app.state.office

    def reads_on(messages, info):
        if info.output_tools:
            return office_brain(messages, info)
        if len(returns(messages)) == 2:  # midway through a long piece of work, Quantix is closed
            threading.Thread(target=first.close).start()
            while not first._closing.is_set():
                time.sleep(0.01)
        return call("list_documents")

    use(reads_on)
    client.post(f"/tenders/{tender_id}/messages", json={"channel": TEAM, "text": "Review the package."})
    first._thread.join(timeout=15)
    [cut_off] = turn_records(client, tender_id)
    assert cut_off.ended == "interrupted"
    assert [c["tool"] for c in cut_off.calls][:2] == ["list_documents", "list_documents"]

    woken = []

    def finishes(messages, info):
        woken.append(info.instructions.split(",")[0])
        return DONE

    monkeypatch.setattr(runtime, "office_model", lambda home: scripted(finishes))
    with restarted(tmp_path) as again:  # nothing new has been said: the unfinished turn alone wakes her
        wait_for(lambda o: len(turn_records(again, tender_id)) == 2, again, tender_id)
    assert woken == ["You are Rania Farouk"]
    assert turn_records(client, tender_id)[-1].ended == "done"


def test_the_turn_budget_is_kept_across_a_restart(client, office, tmp_path, monkeypatch):
    from quantix.office import records as office_records

    tender_id, _ = office
    asked = []
    monkeypatch.setattr(runtime, "office_model", lambda home: scripted(lambda m, i: asked.append(1) or DONE))
    client.app.state.office.close()
    with client.app.state.sessions() as session:
        rania = office_records.hire(session, tender_id, "Rania Farouk", "Tender Manager", {}, is_manager=True)
        office_records.post(session, tender_id, ENGINEER, TEAM, "Price the asphalt.")
        session.flush()
        for _ in range(runtime.TURN_BUDGET):  # a long stretch of work since the engineer last wrote
            session.add(TurnRecord(tender_id=tender_id, staff_id=rania.id, model="scripted", ended="done"))
        session.commit()
    with restarted(tmp_path) as again:
        wait_for(lambda o: o["state"] == "paused", again, tender_id)
    assert team_room(client, tender_id)[-1]["text"].startswith("The office paused after a long stretch of work.")
    assert asked == []


def test_turns_in_the_same_clock_tick_as_the_engineers_message_count_against_the_budget(client):
    from quantix.office import records as office_records

    tender_id = client.post("/tenders", json={"name": "Synthetic school"}).json()["id"]
    tick = datetime.now(UTC)  # Windows' clock moves in 15.6 ms steps: a message and the turns after it can share one
    with client.app.state.sessions() as session:
        rania = office_records.hire(session, tender_id, "Rania Farouk", "Tender Manager", {}, is_manager=True)

        def turn(started_at):
            session.add(
                TurnRecord(
                    tender_id=tender_id, staff_id=rania.id, model="scripted", ended="done", started_at=started_at
                )
            )

        turn(tick - timedelta(minutes=1))  # before the engineer last wrote: not counted
        office_records.post(session, tender_id, ENGINEER, TEAM, "Price the asphalt.").created_at = tick
        for _ in range(3):
            turn(tick)
        session.commit()
        assert office_records.turns_since_engineer(session, tender_id) == 3


def test_the_office_pauses_once_the_tender_has_used_its_ai_allowance(client, office):
    from quantix.office import records as office_records

    tender_id, use = office
    asked = []
    use(lambda messages, info: asked.append(1) or DONE)
    with client.app.state.sessions() as session:
        rania = office_records.hire(session, tender_id, "Rania Farouk", "Tender Manager", {}, is_manager=True)
        session.flush()
        session.add(
            TurnRecord(
                tender_id=tender_id,
                staff_id=rania.id,
                model="scripted",
                ended="done",
                input_tokens=900,
                output_tokens=100,
            )
        )
        session.commit()
    assert client.patch("/settings", json={"tender_allowance": 1000}).json()["tender_allowance"] == 1000
    client.post(f"/tenders/{tender_id}/messages", json={"channel": TEAM, "text": "Price the asphalt."})
    wait_for(lambda o: o["state"] == "paused", client, tender_id)
    assert team_room(client, tender_id)[-1]["text"] == runtime.ALLOWANCE_USED
    assert asked == []

    client.patch("/settings", json={"tender_allowance": 2000})  # raised: the next message carries on
    client.post(f"/tenders/{tender_id}/messages", json={"channel": TEAM, "text": "Carry on."})
    wait_for(lambda o: asked, client, tender_id)


def test_someone_the_engineer_wrote_to_is_woken_once_to_reply(client, office):
    """The real tender: the engineer asked Salem to give out the markups redo. He gave it out and reviewed it, all in
    the team room, and the engineer's chat stayed silent."""
    from quantix.office import records as office_records

    tender_id, use = office
    with client.app.state.sessions() as session:
        rania_id = office_records.hire(session, tender_id, "Rania Farouk", "Tender Manager", {}, is_manager=True).id
        session.commit()
    told = []

    def silent_then_replies(messages, info):
        prompt = prompt_of(messages)
        if not returns(messages):
            told.append(prompt)
            if runtime.NOT_REPLIED in prompt:
                return call("message_engineer", text="I gave the redo to Omar.", next_steps=["review Omar's redo"])
            return call("post_to_team", text="Omar, redo the markups for 72 days.")
        return DONE

    use(silent_then_replies)
    client.post(f"/tenders/{tender_id}/messages", json={"channel": rania_id, "text": "Get the markups redone."})
    wait_for(lambda o: len(told) >= 2, client, tender_id)
    time.sleep(0.3)
    assert runtime.NOT_REPLIED not in told[0] and told[1].startswith(runtime.NOT_REPLIED)
    assert sum(runtime.NOT_REPLIED in t for t in told) == 1  # reminded once, not again after replying
    chat = client.get(f"/tenders/{tender_id}/messages", params={"channel": rania_id}).json()
    assert [m["text"] for m in chat][-1] == "I gave the redo to Omar."
