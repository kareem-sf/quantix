"""Figures and look-ups Quantix gives the office on request: arithmetic, cut and fill, where the money is, what a
change would do, how a rate compares, reusing a build-up, the library, the conversation and what changed."""

import threading
from datetime import date
from decimal import Decimal
from types import SimpleNamespace

import pytest
import test_estimate
from pydantic_ai import ModelRetry

from quantix.boq import records as boq
from quantix.core import calculate
from quantix.estimate import records as estimate
from quantix.estimate.models import LibraryResource
from quantix.office import records as office
from quantix.office import tools
from quantix.office.models import Decision

tender = test_estimate.tender  # the fixture: a school's BOQ with an estimator, Priya
NOTE = "Fixed by a gang of four at 16 hours a tonne; rebar at the supplier price with 5% wastage."
LINES = [estimate.LineIn(**line) for line in test_estimate.BUILD_UP]


def turn(client, tender_id, staff_id) -> SimpleNamespace:
    state = client.app.state
    return SimpleNamespace(
        deps=tools.Turn(state.home, state.sessions, tender_id, staff_id, False, threading.Event()), tool_call_id="call"
    )


@pytest.fixture
def priced(client, tender):
    """3.1 at a unit rate and 4.3 built up, both approved, with Rania as the Tender Manager."""
    tender_id, priya, _ = tender
    with client.app.state.sessions() as session:
        rania = office.hire(session, tender_id, "Rania Farouk", "Tender Manager", {}, is_manager=True)
        excavation = estimate.propose_rate(session, tender_id, priya, "3.1", "estimate", NOTE, unit_rate=Decimal(18))
        rebar = estimate.propose_rate(session, tender_id, priya, "4.3", "estimate", NOTE, lines=LINES)
        estimate.approve(session, excavation)
        estimate.approve(session, rebar)
        session.commit()
        return tender_id, priya, rania.id


def test_sums_are_worked_out_by_quantix():
    assert calculate.evaluate("L * W * D", {"L": Decimal(420), "W": Decimal(12), "D": Decimal("0.3")}) == 1512
    assert calculate.evaluate("sqrt(9) + max(2, 5) × 2") == 13
    for bad, message in (("L * 2", "L has no value"), ("__import__('os')", "Use numbers"), ("1/0", "divides by zero")):
        with pytest.raises(ValueError, match=message):
            calculate.evaluate(bad)


def test_cut_and_fill_from_a_grid_of_levels():
    level = [[Decimal(v) for v in row] for row in ((101, 101, 99), (101, 101, 99))]
    formation = [[Decimal(100)] * 3 for _ in range(2)]
    cut, fill, squares = calculate.grid_volumes(Decimal(10), level, formation)
    # one square all in cut (1 m over 100 m2), one crossing: cut 100 × 2² / (4 × 4), fill the same
    assert (cut, fill, squares) == (Decimal(125), Decimal(25), 2)


def test_where_the_money_is_and_what_a_change_would_do(client, priced):
    tender_id, priya, _ = priced
    ctx = turn(client, tender_id, priya)
    text = tools.price_breakdown(ctx)
    assert text.startswith("Net 120,332.80 SAR")  # 1,240 × 18 + 28.1 × 3,488
    assert "- 4.3 Slab reinforcement: 98,012.80 (81.5%)" in text
    assert "- material: 69,884.70" in text  # 28.1 t × (2,415 + 72) and "- unit rates: 22,320.00" in text

    dearer = tools.what_if(ctx, [tools.analysis.Change(resource="rebar", factor=Decimal("1.1"))])
    assert dearer.splitlines()[0] == "Net 120,332.80 would be 127,118.95 (+6,786.15, 5.6%)."
    assert "- 4.3: rate 3488.00 → 3729.50 per t, amount +6,786.15" in dearer
    everything = tools.what_if(ctx, [tools.analysis.Change(factor=Decimal("0.9"))])
    assert "2 lines move" in everything
    with client.app.state.sessions() as session:
        assert estimate.summary(session, tender_id).net == Decimal("120332.80")  # nothing changed


def test_a_rate_beside_the_library_and_similar_lines(client, priced):
    tender_id, priya, _ = priced
    with client.app.state.sessions() as session:
        session.add(
            LibraryResource(kind="unit_rate", name="Slab reinforcement B500B", unit="t", rate=Decimal(3200),
                            currency="SAR", source="Depot tender", dated=date(2024, 1, 10))
        )  # fmt: skip
        session.commit()
    text = tools.check_rate(turn(client, tender_id, priya), "4.3")
    assert text.startswith("4.3 Slab reinforcement: ours 3488.00 per t (estimate).")
    assert "- Library: Slab reinforcement B500B 3,200 per t, dated Jan 2024" in text
    assert "months old: check it is still current: ours is 9% above" in text


def test_an_approved_build_up_prices_the_same_work_elsewhere(client, priced):
    tender_id, priya, _ = priced
    with client.app.state.sessions() as session:
        slab = boq.find_item(session, tender_id, "4.3")
        session.add(
            boq.BoqItem(tender_id=tender_id, item="4.4", description="Wall reinforcement", unit="t", quantity=10,
                        document_id=slab.document_id, page=1, quote=slab.quote, position=9, proposed_by=priya,
                        status="approved")
        )  # fmt: skip
        session.commit()
    report = tools.apply_buildup(turn(client, tender_id, priya), "4.3", ["4.4", "3.1"], "Same bar, same gang.")
    assert report == (
        "Priced 1 lines with the build-up of 4.3, for the Tender Manager's review.\nNot priced: 3.1: its unit is m3, "
        "not t"
    )
    with client.app.state.sessions() as session:
        rate = estimate.current_rate(session, boq.find_item(session, tender_id, "4.4").id)
        assert (rate.status, estimate.rate_of(rate)) == ("proposed", Decimal("3488.00"))
        assert rate.note.startswith("Same build-up as 4.3: Same bar, same gang.")


def test_the_manager_suggests_keeping_a_rate_and_the_engineer_decides(client, priced):
    tender_id, _, rania = priced
    ctx = turn(client, tender_id, rania)
    assert tools.suggest_library(ctx, "4.3", "A clean build-up we will reuse.") == (
        "The engineer will decide whether to keep it in the library."
    )
    assert tools.suggest_library(ctx, "4.3", "Again.") == "The engineer has already been asked about this rate."
    [decision] = client.get(f"/tenders/{tender_id}/decisions").json()
    assert decision["options"] == ["Keep it in the library", "Don't keep it"]
    client.post(f"/decisions/{decision['id']}/answer", json={"answer": "Keep it in the library"})
    kept = client.get("/library", params={"q": "Rebar"}).json()
    assert [(r["name"], r["rate"]) for r in kept][:1] == [("Rebar B500B cut and bent", "2300.0000")]
    with client.app.state.sessions() as session:
        assert session.query(Decision).one().status == "answered"
    with pytest.raises(ModelRetry, match="Only an approved rate"):
        tools.suggest_library(ctx, "6.3", "Not priced.")


def test_the_conversation_what_changed_and_my_own_checks(client, priced):
    tender_id, priya, rania = priced
    client.post(f"/tenders/{tender_id}/messages", json={"channel": "team", "text": "Use the Riyadh depot rates."})
    with client.app.state.sessions() as session:
        estimate.propose_rate(session, tender_id, priya, "6.3", "estimate", NOTE, unit_rate=Decimal(35))
        session.commit()
    ctx = turn(client, tender_id, priya)
    found = tools.search_conversation(ctx, "riyadh depot")
    assert "The engineer in the team room: Use the Riyadh depot rates." in found
    changed = tools.what_changed(turn(client, tender_id, rania))
    assert changed.splitlines()[1] == "Filed: Priya 1 rate"
    assert "6.3" in tools.precheck(ctx) and "by Priya" in tools.precheck(ctx)
    assert tools.precheck(turn(client, tender_id, rania)) == "Nothing of yours is waiting for the Tender Manager."


def test_the_boq_a_part_at_a_time_and_the_estimate_with_its_markups(client, priced):
    tender_id, priya, _ = priced
    ctx = turn(client, tender_id, priya)
    assert "3 BOQ items; lines 2 to 3:" in tools.list_boq(ctx, start=2)
    assert tools.list_boq(ctx, section="Roofing") == "No BOQ line is in a bill called “Roofing”."
    lines = [tools.estimate.PreliminaryIn(item="Site office", quantity=Decimal(3), unit="month", rate=Decimal(5000))]
    with client.app.state.sessions() as session:
        estimate.propose_markups(session, tender_id, priya, lines, Decimal("0.06"), Decimal("0.05"), Decimal(0), NOTE)
        session.commit()
    assert "1 preliminary items, overheads 6.0%, profit 5.0%, adjustment 0." in tools.estimate_summary(ctx)
    assert tools.takeoff_summary(ctx).startswith("Not measured yet, 3 lines with a quantity: 3.1, 4.3, 6.3")
