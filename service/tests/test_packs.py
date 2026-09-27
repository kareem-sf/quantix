"""Each person's tools in a turn: a small core, the packs for the work they were hired for, and the other packs
hidden until they load them. The Tender Manager's packs never produce records."""

from decimal import Decimal
from types import SimpleNamespace

import test_estimate
from pydantic_ai.messages import ModelResponse, RetryPromptPart, TextPart, ToolCallPart, ToolReturnPart
from test_office import scripted, wait_for

from quantix import settings
from quantix.estimate import records as estimate
from quantix.office import packs
from quantix.office import records as office

tender = test_estimate.tender  # the fixture: a school's BOQ with an estimator, Priya


def person(is_manager=False, work=None) -> SimpleNamespace:
    return SimpleNamespace(is_manager=is_manager, profile={"work": work} if work else {})


def names(tools) -> set[str]:
    return {t.__name__ for t in tools}


def test_staff_have_their_own_work_at_hand_and_can_load_the_rest():
    core, work = packs.loadout(person(work=["pricing"]), team_size=2, sees_images=True)
    assert {"message_engineer", "open_record", "read_page", "calculate", "precheck", "withdraw"} <= names(core)
    loaded = {p.id for p in work if not p.defer_loading}
    assert loaded == {"pricing"}
    pricing = next(p for p in work if p.id == "pricing")
    assert {"propose_rate", "check_rate", "what_if", "apply_buildup"} <= set(pricing.toolset.tools)
    assert "Never work out an amount yourself" in pricing.get_instructions()
    # someone hired before packs existed does every kind of work, as before
    _, legacy = packs.loadout(person(), team_size=2, sees_images=True)
    assert all(not p.defer_loading for p in legacy)


def test_the_manager_checks_but_never_produces_and_hires_while_the_team_is_small():
    core, work = packs.loadout(person(is_manager=True), team_size=0, sees_images=True)
    produced = {t.__name__ for w in packs.WORK.values() for t in w.produces}
    reachable = names(core) | {name for p in work for name in p.toolset.tools}
    assert not reachable & produced
    assert {"review_queue", "review", "escalate", "assign_task", "estimate_summary"} <= names(core)
    assert {"check_rate", "suggest_library"} <= set(next(p for p in work if p.id == "pricing").toolset.tools)
    assert not next(p for p in work if p.id == "team").defer_loading  # an empty team: hiring is at hand
    _, later = packs.loadout(person(is_manager=True), team_size=3, sees_images=True)
    assert next(p for p in later if p.id == "team").defer_loading
    assert all(p.defer_loading for p in later)


def test_an_ai_that_cannot_see_has_no_image_tools():
    seeing = {"view_page", "find_on_page", "set_scale", "measure"}
    for member in (person(), person(is_manager=True)):
        core, work = packs.loadout(member, team_size=3, sees_images=False)
        reachable = names(core) | {name for p in work for name in p.toolset.tools}
        assert not reachable & seeing and "read_page" in reachable


def test_a_tool_from_another_pack_is_loaded_before_use(client, tender, tmp_path):
    tender_id, priya, _ = tender
    with client.app.state.sessions() as session:
        omar = office.hire(session, tender_id, "Omar Haddad", "Quantity Surveyor", {"work": ["boq"]})
        office.hire(session, tender_id, "Rania Farouk", "Tender Manager", {}, is_manager=True)
        session.commit()
        omar_id = omar.id
    settings.save(tmp_path, office_ai={"connection_id": "scripted", "model": "brain"})
    seen: dict[str, list] = {}
    price = {
        "boq_item": "3.1",
        "basis": "estimate",
        "unit_rate": "18.50",
        "note": "Plant 4.5 m3/hr at 83.25 per hour; no disposal off site.",
    }

    def brain(messages, info):
        if "You are Omar Haddad" not in info.instructions:
            return ModelResponse(parts=[TextPart("Done.")])
        refused = [str(p.content) for m in messages for p in m.parts if isinstance(p, RetryPromptPart)]
        done = [str(p.content) for m in messages for p in m.parts if isinstance(p, ToolReturnPart)]
        seen["refused"], seen["done"], seen["tools"] = refused, done, [t.name for t in info.function_tools]
        if not refused:
            return ModelResponse(parts=[ToolCallPart("propose_rate", price)])  # not his work: hidden
        if not done:
            return ModelResponse(parts=[ToolCallPart("load_capability", {"id": "pricing"})])
        if len(done) == 1:
            return ModelResponse(parts=[ToolCallPart("propose_rate", price)])
        return ModelResponse(parts=[TextPart("Done.")])

    client.app.state.office.model = lambda: scripted(brain)
    client.post(f"/tenders/{tender_id}/messages", json={"channel": omar_id, "text": "Omar, price the excavation."})
    wait_for(lambda o: len(seen.get("done", [])) == 2, client, tender_id)
    assert "it belongs to capability 'pricing'. Call `load_capability` for it first" in seen["refused"][0]
    assert "Pricing: build a rate up from labour, plant, material and subcontract" in seen["done"][0]
    assert seen["done"][1] == "Item 3.1 priced at 18.50 per unit, for the Tender Manager's review."
    assert "propose_rate" in seen["tools"]
    with client.app.state.sessions() as session:
        from quantix.boq import records as boq

        rate = estimate.current_rate(session, boq.find_item(session, tender_id, "3.1").id)
        assert (rate.proposed_by, rate.unit_rate) == (omar_id, Decimal("18.50"))
