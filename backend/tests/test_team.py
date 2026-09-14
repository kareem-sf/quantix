import hashlib

import pytest
from test_catalog_authority import configured_office

from quantix.ai_tools import ToolArgumentError
from quantix.office_tools import OfficeContext
from quantix.repository import Repository
from quantix.team import TeamService
from quantix.team_models import StaffDraft
from quantix.team_runtime import run_queued
from quantix.team_tools import team_tools


def _source(repo, tender_id, text, path="Specs/concrete.pdf"):
    artifact, _ = repo.register_artifact(
        tender_id, path, hashlib.sha256(text.encode()).hexdigest(), len(text.encode()),
        {"kind": "pdf", "status": "extracted", "segments": [{"locator": "page:1", "text": text}]},
    )
    return repo.artifact_evidence(tender_id, artifact["id"])[0]


def _draft(name="Samir Haddad", role="Senior Quantity Surveyor"):
    return StaffDraft(name=name, role=role, specialisms=["BOQ audit", "Concrete works"],
                      background="Fifteen years pricing public buildings.", working_style="Checks every quantity twice.")


@pytest.fixture
def office(tmp_path, monkeypatch):
    repo = Repository(tmp_path)
    tender = repo.create_tender("Synthetic team tender")
    _, _, _, connection, _, policy, route = configured_office(tmp_path, monkeypatch, repo=repo, tender=tender)
    run = repo.create_run(tender["id"], "manager", "Audit the concrete scope")
    evidence = _source(repo, tender["id"], "Ground slab concrete shall be C30/37, 250 mm thick.")
    return repo, tender, run, evidence, route


def _tool(definitions, name):
    return next(definition for definition in definitions if definition.name == name)


def test_hiring_rejects_duplicates_and_retired_staff_cannot_take_work(office):
    repo, tender, run, evidence, route = office
    team = TeamService(repo)
    member = team.hire(tender["id"], run["id"], _draft())
    with pytest.raises(ValueError, match="already on this tender's team"):
        team.hire(tender["id"], run["id"], _draft())
    team.retire(tender["id"], member.id)
    with pytest.raises(ValueError, match="retired"):
        team.assign(tender["id"], run["id"], member.id, title="Check slab", brief="Check the slab.",
                    expected_result="Quantities", source_ids=[], route=route)


@pytest.mark.asyncio
async def test_manager_tools_hire_and_assign_and_refuse_staff_callers(office):
    repo, tender, run, evidence, route = office
    tools = team_tools(repo, tender["id"], run["id"])
    manager = OfficeContext(repo, tender["id"], run["id"])
    hired = await _tool(tools, "hire_staff").invoke(manager, {
        "name": "Samir Haddad", "role": "Senior Quantity Surveyor", "specialisms": ["BOQ audit"],
        "background": "Pricing public buildings.", "working_style": "Methodical."})
    staff_id = __import__("json").loads(hired)["staff_id"]
    with pytest.raises(ToolArgumentError, match="available_models"):
        await _tool(tools, "assign_work").invoke(manager, {
            "staff_id": staff_id, "title": "Slab check", "brief": "Check the ground slab.",
            "expected_result": "Concrete grade and thickness", "model": "unknown/model"})
    await _tool(tools, "assign_work").invoke(manager, {
        "staff_id": staff_id, "title": "Slab check", "brief": "Check the ground slab.",
        "expected_result": "Concrete grade and thickness", "source_ids": [evidence["id"]]})
    queued = TeamService(repo).queued(tender["id"], run["id"])
    assert [a.title for a in queued] == ["Slab check"]
    assert (queued[0].connection_id, queued[0].model_id) == (route["connection_id"], route["model_id"])
    staff_context = OfficeContext(repo, tender["id"], run["id"], actor_id=staff_id, assignment_id=queued[0].id)
    with pytest.raises(ValueError, match="Only the Tender Manager"):
        await _tool(tools, "list_team").invoke(staff_context, {})


@pytest.mark.asyncio
async def test_staff_question_then_answer_then_cited_result(office, monkeypatch):
    repo, tender, run, evidence, route = office
    team = TeamService(repo)
    member = team.hire(tender["id"], run["id"], _draft())
    assignment = team.assign(tender["id"], run["id"], member.id, title="Slab check", brief="Check the ground slab.",
                             expected_result="Grade and thickness", source_ids=[evidence["id"]], route=route)
    packets = []

    async def provider(route, connection, credentials, context, prompt, output_type, **kwargs):
        packets.append(prompt)
        if len(packets) == 1:
            return {"output": {"kind": "question", "question": "Which slab: ground or roof?"},
                    "usage": {"requests": 1, "input_tokens": 10, "output_tokens": 5}, "web_sources": []}
        from quantix.tool_policy import dispatch

        read = next(item for item in kwargs["definitions"] if item.name == "read_source")
        # Through the real tool fence, exactly as the direct and client engines call tools.
        await dispatch("direct", read, context, {"source_id": evidence["id"]}, invocation_id="staff-read-1")
        return {"output": {"kind": "completed", "summary": "Ground slab is C30/37 and 250 mm.",
                           "findings": [{"title": "Slab grade", "detail": "C30/37, 250 mm.", "kind": "requirement",
                                         "source_ids": [evidence["id"]]}],
                           "source_ids": [evidence["id"]]},
                "usage": {"requests": 2, "input_tokens": 30, "output_tokens": 12}, "web_sources": []}

    monkeypatch.setattr("quantix.ai_execution.execute_api", provider)
    [waiting] = await run_queued(repo, tender["id"], run["id"])
    assert waiting.status == "waiting" and waiting.question == "Which slab: ground or roof?"
    team.answer(tender["id"], assignment.id, "The ground slab.")
    [done] = await run_queued(repo, tender["id"], run["id"])
    assert "The ground slab." in packets[1]
    assert done.status == "completed"
    assert done.result.findings[0].source_ids == [evidence["id"]]
    assert done.usage["requests"] == 3
    kinds = [event["kind"] for event in repo.run_events(run["id"])]
    assert "staff_question" in kinds and "staff_completed" in kinds


@pytest.mark.asyncio
async def test_unread_citation_or_provider_failure_fails_only_that_assignment(office, monkeypatch):
    repo, tender, run, evidence, route = office
    team = TeamService(repo)
    first = team.hire(tender["id"], run["id"], _draft())
    second = team.hire(tender["id"], run["id"], _draft(name="Lina Farouk", role="Planning Engineer"))
    cited = team.assign(tender["id"], run["id"], first.id, title="Cite unread", brief="Report the grade.",
                        expected_result="Grade", source_ids=[], route=route)
    broken = team.assign(tender["id"], run["id"], second.id, title="Provider fails", brief="Draft programme.",
                         expected_result="Programme", source_ids=[], route=route)

    async def provider(route, connection, credentials, context, prompt, output_type, **kwargs):
        if context.assignment_id == broken.id:
            raise RuntimeError("The provider is unavailable.")
        return {"output": {"kind": "completed", "summary": "Grade is C30/37.", "source_ids": [evidence["id"]]},
                "usage": {"requests": 1}, "web_sources": []}

    monkeypatch.setattr("quantix.ai_execution.execute_api", provider)
    outcomes = {a.id: a for a in await run_queued(repo, tender["id"], run["id"])}
    assert outcomes[cited.id].status == "failed"
    assert "not read" in outcomes[cited.id].detail
    assert outcomes[broken.id].status == "failed"
    assert "provider is unavailable" in outcomes[broken.id].detail


def test_stop_and_restart_cancel_open_work(office):
    repo, tender, run, evidence, route = office
    team = TeamService(repo)
    member = team.hire(tender["id"], run["id"], _draft())
    team.assign(tender["id"], run["id"], member.id, title="A", brief="A", expected_result="A", source_ids=[], route=route)
    assert team.cancel_run(tender["id"], run["id"], "Stopped by the engineer.") == 1
    team.assign(tender["id"], run["id"], member.id, title="B", brief="B", expected_result="B", source_ids=[], route=route)
    assert TeamService(repo).recover_interrupted() == 1
    assert {a.status for a in team.list(tender["id"])} == {"cancelled"}


@pytest.mark.asyncio
async def test_manager_hires_assigns_and_reads_the_result_in_its_next_turn(office, monkeypatch):
    import json

    from quantix.office import run_manager
    from quantix.tool_policy import dispatch

    repo, tender, run, evidence, route = office
    manager_prompts = []

    async def provider(route, connection, credentials, context, prompt, output_type, **kwargs):
        tools = {item.name: item for item in kwargs["definitions"]}
        if context.assignment_id:
            await dispatch("direct", tools["read_source"], context, {"source_id": evidence["id"]}, invocation_id="s-read")
            return {"output": {"kind": "completed", "summary": "Slab is C30/37, 250 mm.", "source_ids": [evidence["id"]]},
                    "usage": {"requests": 2}, "web_sources": []}
        manager_prompts.append(json.loads(prompt.split("---- Current request and context ----", 1)[-1]))
        if len(manager_prompts) == 1:
            hired = json.loads(await dispatch("direct", tools["hire_staff"], context, {
                "name": "Samir Haddad", "role": "Senior Quantity Surveyor", "specialisms": ["Concrete"],
                "background": "Public buildings.", "working_style": "Careful."}, invocation_id="m-hire"))
            await dispatch("direct", tools["assign_work"], context, {
                "staff_id": hired["staff_id"], "title": "Slab check", "brief": "Report the ground slab grade.",
                "expected_result": "Grade and thickness with sources"}, invocation_id="m-assign")
            return {"output": {"summary": "Samir is checking the slab."}, "usage": {"requests": 1}, "web_sources": []}
        await dispatch("direct", tools["read_source"], context, {"source_id": evidence["id"]}, invocation_id="m-read")
        return {"output": {"summary": "The ground slab is C30/37, 250 mm thick.", "source_ids": [evidence["id"]]},
                "usage": {"requests": 1}, "web_sources": []}

    monkeypatch.setattr("quantix.ai_execution.execute_api", provider)
    result = await run_manager(repo, tender["id"], run["id"], "What concrete grade is the ground slab?")
    assert len(manager_prompts) == 2
    [update] = manager_prompts[1]["team_updates_not_source_inspection"]
    assert update["status"] == "completed" and update["staff"] == "Samir Haddad"
    assert manager_prompts[1]["team"][0]["name"] == "Samir Haddad"
    assert result.output.source_ids == [evidence["id"]]
    assert result.usage["requests"] == 4
