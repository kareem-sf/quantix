"""The office at work, end to end: real runtime, tools and records, with only the model scripted."""

import re
import time

import pytest
from pydantic_ai.messages import ModelResponse, TextPart, ToolCallPart, ToolReturnPart, UserPromptPart
from pydantic_ai.models.function import FunctionModel
from test_documents import PDF, read_all, upload

from quantix import settings
from quantix.office.models import TEAM


def prompt_of(messages) -> str:
    return next(p.content for m in messages for p in m.parts if isinstance(p, UserPromptPart))


def returns(messages) -> list[str]:
    return [str(p.content) for m in messages for p in m.parts if isinstance(p, ToolReturnPart)]


def call(tool: str, **args) -> ModelResponse:
    return ModelResponse(parts=[ToolCallPart(tool, args)])


DONE = ModelResponse(parts=[TextPart("That's everything for now.")])


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
        client.app.state.office.model = lambda: FunctionModel(brain)

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
