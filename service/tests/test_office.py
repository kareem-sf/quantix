"""The office at work, end to end: real runtime, tools and records, with only the model scripted."""

import re
import time

import pytest
from pydantic_ai.exceptions import ModelAPIError
from pydantic_ai.messages import (
    ModelResponse,
    RetryPromptPart,
    TextPart,
    ToolCallPart,
    ToolReturnPart,
    UserPromptPart,
)
from pydantic_ai.models.function import DeltaToolCall, FunctionModel
from test_documents import PDF, read_all, upload

from quantix import settings
from quantix.office import runtime
from quantix.office.models import TEAM


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
                "complete_task", task_id=task_id, result="Tender security is one percent (Conditions.pdf, page 1)."
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
    state = wait_for(lambda o: o["state"] == "idle" and o["staff"], client, tender_id)
    [rania] = state["staff"]
    chat = client.get(f"/tenders/{tender_id}/messages", params={"channel": rania["id"]}).json()
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
    state = wait_for(lambda o: o["staff"] and o["state"] == "idle", client, tender_id)
    chat = client.get(f"/tenders/{tender_id}/messages", params={"channel": state["staff"][0]["id"]}).json()
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
        if not returns(messages):
            return call("view_page", document_id=document_id, page=1)
        seen[:] = returns(messages)
        return DONE

    use(looks)
    client.app.state.office.sees_images = lambda: False
    client.post(f"/tenders/{tender_id}/messages", json={"channel": TEAM, "text": "Look at the conditions."})
    wait_for(lambda o: o["staff"] and seen, client, tender_id)
    assert seen[0].startswith("The office's AI can't read images, so it can't look at drawings or scans.")


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
        session.commit()
        brief = agents.situation(session, nora, [])
    assert "The engineer has decided:\n- Zero-quantity lines: Price them rate-only." in brief


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
    assert "Where the tender stands:\n- BOQ: 0 lines, 0 approved." in brief
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
        return call("hire", name="Hadi Nassar", role="Estimator", **profile)

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
        steps = [call("message_engineer", text="Third.")] if answered else []
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


def test_only_the_manager_brings_decisions_to_the_engineer():
    from quantix.office import tools

    assert "ask_engineer" in [t.__name__ for t in tools.MANAGER]
    assert "ask_engineer" not in [t.__name__ for t in tools.STAFF]  # staff raise it with the Manager instead


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
