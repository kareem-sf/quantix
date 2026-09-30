"""Every route of the service, read from its own OpenAPI schema so a new route is covered the day it is added: none
answers without the launch token, and on a tender holding work of every kind, every page opens without a server
error, never shows a key, and says not found for what isn't there."""

import io
import itertools
import re
from decimal import Decimal
from pathlib import Path

import pytest
from conftest import TOKEN
from drawings import MAP, make_drawing, plan
from fastapi.testclient import TestClient
from PIL import Image
from test_documents import PDF, make_docx, make_pdf, make_styled_xlsx, make_xlsx, read_all, upload
from test_office import manager_accepts, office_brain, scripted, wait_for

from quantix import settings
from quantix.ai import connections, providers
from quantix.api.app import create_app
from quantix.boq import records as boq
from quantix.documents import meaning, ocr
from quantix.estimate import records as estimate
from quantix.office import records as office
from quantix.office.models import ENGINEER, TEAM, TurnRecord
from quantix.review import queries
from quantix.review import records as reviews
from quantix.subcontract import records as subcontract
from quantix.submission import records as submission
from quantix.takeoff import layers

SCHEMA = create_app(Path("unused"), TOKEN).openapi()  # built without starting the app, as `npm run bindings` does
ROUTES = [(method.upper(), path) for path, operations in SCHEMA["paths"].items() for method in operations]
GETS = [path for method, path in ROUTES if method == "GET"]
KEY, WEB_KEY = "sk-ant-routes-5678wxyz", "fc-routes-1234abcd"  # no answer may contain either
PRICES = make_pdf([["Prices shall be in Saudi Riyals (SAR)", "VAT at 15% shall be shown separately"]])
QUOTE = make_pdf([["Gulf Concrete quotation", "4.2 Reinforced concrete to slab 410.00 SAR per m3"]])
ITT = make_pdf([["Instructions to Tenderers", "7.3 A bid bond of 1% of the tender price shall be submitted."]])
NOTE = "Plant 4.5 m3/hr at 83.25 per hour."
PROBLEM = "The company rules give 5% profit on schools, not 7%."


def image(kind: str) -> bytes:
    buffer = io.BytesIO()
    Image.new("RGB", (40, 30), "white").save(buffer, format=kind)
    return buffer.getvalue()


def seed(client) -> dict[str, list]:
    """A tender with documents of every kind and state, an office that worked on it, and records of every kind."""
    home = client.app.state.home
    tender_id = client.post("/tenders", json={"name": "Synthetic school", "due_date": "2026-10-14"}).json()["id"]
    files = {
        "Conditions.pdf": PDF,  # one page of text, one scan
        "Prices.pdf": PRICES,
        "ITT.pdf": ITT,
        "Quote.pdf": QUOTE,
        "Bill.xlsx": make_xlsx(),
        "Styled.xlsx": make_styled_xlsx(),
        "Letter.docx": make_docx(),
        "Scan.tif": image("TIFF"),
        "Photo.png": image("PNG"),
        "Drawings/A-101.dwg": make_drawing(plan()),
        "Old.doc": b"not read",
    }
    upload(client, tender_id, files)
    read_all(client, tender_id)
    upload(client, tender_id, {"Letter.docx": make_docx() + b"\0"})  # a newer copy: the first is replaced
    read_all(client, tender_id)
    listed = client.get(f"/tenders/{tender_id}/documents").json()
    docs = {d["path"]: d["id"] for d in listed if d["status"] != "replaced"}

    # The office works a request through to a question for the engineer: staff, tasks, messages, turns, a decision
    settings.save(home, office_ai={"connection_id": "scripted", "model": "office-brain"})
    client.app.state.office.model = lambda: scripted(office_brain)
    client.post(f"/tenders/{tender_id}/messages", json={"channel": TEAM, "text": "Check the tender security."})
    wait_for(lambda o: o["waiting"] == 1, client, tender_id)

    bill, drawing = docs["Bill.xlsx"], docs["Drawings/A-101.dwg"]
    with client.app.state.sessions() as session:
        rania, omar = office.manager(session, tender_id), office.find_staff(session, tender_id, "Omar")
        lines = [
            boq.ItemIn(item="3.1", description="Excavation to reduce levels", unit="m3", quantity=Decimal("1240"),
                       document_id=bill, page=1, quote="A2=3.1 | B2=Excavation to reduce levels | C2=m3 | D2=1240"),
            boq.ItemIn(item="4.2", description="خرسانة مُسلّحة للبلاطة", unit="م3", quantity=Decimal("312.4"),
                       document_id=bill, page=1, quote="A3=4.2 | B3=خرسانة مُسلّحة للبلاطة | C3=م3 | D3=312.4"),
        ]  # fmt: skip
        boq.propose_items(session, tender_id, omar, lines)
        boq.propose_fact(session, tender_id, omar, "currency", "SAR", docs["Prices.pdf"], 1, "Saudi Riyals (SAR)")
        session.commit()
    manager_accepts(client, tender_id)
    client.post(f"/tenders/{tender_id}/boq/approve-all")

    with client.app.state.sessions() as session:
        rania, omar = office.manager(session, tender_id), office.find_staff(session, tender_id, "Omar")
        estimate.propose_rate(session, tender_id, omar.id, "3.1", "estimate", NOTE, unit_rate=Decimal("15"))
        [rate] = reviews.pending(session, tender_id)
        lesson = "Price the disposal of surplus excavated material in every excavation rate."
        sent_back = reviews.Verdict(record=rate.ref, accept=False, note="Add the disposal.", lesson=lesson)
        reviews.review(session, home, tender_id, rania, [sent_back], autonomous=False)
        note = "Plant 4.5 m3/hr at 83.25 per hour, disposal at 3 per m3."
        estimate.propose_rate(session, tender_id, omar.id, "3.1", "estimate", note, unit_rate=Decimal("21.50"))
        preliminaries = [estimate.PreliminaryIn(item="Site costs", quantity=Decimal(1), unit="sum", rate=Decimal(900))]
        estimate.propose_markups(
            session, tender_id, omar.id, preliminaries, Decimal("0.05"), Decimal("0.07"), Decimal(0), "Company rules"
        )
        firm = subcontract.add_company(session, omar.id, "Gulf Concrete", "subcontractor", "concrete")
        package = subcontract.create_package(session, tender_id, omar.id, "Concrete", "subcontract", ["4.2"])
        quoted = subcontract.QuoteLine(boq_item="4.2", rate=Decimal("410.00"), page=1, quote="concrete to slab 410.00")
        subcontract.record_quote(session, package, firm, omar.id, docs["Quote.pdf"], [quoted], [])
        subcontract.recommend(session, package, omar.id, firm, "The only quote, and complete.")
        bond = submission.RequirementIn(
            section="Commercial", title="Bid bond", document_id=docs["ITT.pdf"], page=1, quote="7.3 A bid bond of 1%"
        )
        submission.add_requirements(session, tender_id, omar.id, [bond])
        [requirement] = submission.requirements(session, tender_id)
        submission.draft(session, requirement, omar.id, "Bid bond letter", "We enclose a bid bond of 1%.")
        mapped = "From what each layer holds."
        layers.propose(session, home, tender_id, ENGINEER, drawing, MAP, {"WIN-1200": "windows"}, mapped, "approved")
        source = queries.QuerySource(document_id=drawing, page=1, quote="LIVING")
        queries.raise_query(
            session, home, tender_id, omar.id, "clarification", "Living room finish", "No finish is scheduled.",
            "Please confirm the floor finish to the living room.", [source],
        )  # fmt: skip
        pending = next(p for p in reviews.pending(session, tender_id) if p.kind == "markups")
        where = reviews.Source(document_id=docs["Prices.pdf"], page=1, what="The currency")
        reviews.escalate(session, tender_id, rania, pending.ref, PROBLEM, [where], ["Keep 7%", "Use 5%"])
        session.commit()
    # The engineer measures on the drawing; the firm's own records, an AI connection and a web research key
    windows = {"stamp": client.get(f"/documents/{drawing}/drawing").json()["stamp"], "rule": {"blocks": ["WIN-1200"]}}
    entry = {"kind": "unit_rate", "name": "Excavation", "unit": "m3", "rate": "12", "currency": "SAR"}
    done = [
        client.post(
            f"/tenders/{tender_id}/drawing-measurements",
            json={"document_id": drawing, "choice": windows, "kind": "count", "label": "Windows", "unit": "nr"},
        ),
        client.post(f"/tenders/{tender_id}/units", json={"document_id": drawing, "units": "millimetres"}),
        client.post("/library", json=entry | {"source": "Riyadh school 2025", "dated": "2025-11-02"}),
        client.post("/rules", json={"topic": "Rates", "text": "Price disposal off site in every excavation rate."}),
        client.put(
            "/company", json={"name": "Gulf Builders", "address": "Riyadh", "cr_number": "1", "vat_number": "3"}
        ),
        client.put("/company/logo", files={"file": ("logo.png", image("PNG"))}),
        client.post("/ai/connections", json={"provider": "anthropic", "api_key": KEY}),
    ]
    assert all(r.is_success for r in done), [r.text for r in done if not r.is_success]
    connection = done[-1].json()
    connections.set_web_key(home, "firecrawl", WEB_KEY)

    with client.app.state.sessions() as session:
        staff = [s.id for s in office.team(session, tender_id, include_released=True)]
        turns = [t.id for t in session.query(TurnRecord).filter_by(tender_id=tender_id)]
        records = [
            (kind, record.id)
            for kind, model in reviews.CHECKED.items()
            for record in session.query(model).filter_by(tender_id=tender_id)
        ]
    return {
        "tender_id": [tender_id],
        "document_id": [d["id"] for d in listed],  # every kind of document, read, unreadable and replaced
        "number": [1, 2],
        "turn_id": turns,
        "connection_id": [connection["id"]],
        "records": records,
        # query parameters
        "q": ["tender security"],
        "staff_id": staff,
        "channel": [TEAM, *staff],
        "page": [1],
        "hidden": [True],
    }


@pytest.fixture(scope="module")
def seeded(request, tmp_path_factory):
    """One tender for the whole module, seeded once: the meaning model and OCR as conftest sets them for each test,
    and the AI provider listing its models without a network."""

    async def list_models(provider, key, base_url):
        return ["model-a"]

    with pytest.MonkeyPatch.context() as patch:
        folder = request.config.cache.mkdir("quantix-models")
        patch.setattr(meaning, "models_dir", lambda home: folder)
        patch.setattr(ocr.Ocr, "wake", lambda self: None)
        patch.setattr(providers, "list_models", list_models)
        app = create_app(tmp_path_factory.mktemp("home"), TOKEN)
        with TestClient(app, headers={"Authorization": f"Bearer {TOKEN}"}) as client:
            yield client, seed(client)


def combinations(path: str, ids: dict[str, list]) -> list[dict]:
    """Every combination of real ids for the path's parameters."""
    names = re.findall(r"\{(\w+)\}", path)
    if names == ["kind", "record_id"]:  # a record's kind goes with its id
        return [{"kind": kind, "record_id": record} for kind, record in ids["records"]]
    return [dict(zip(names, values, strict=True)) for values in itertools.product(*(ids[n] for n in names))]


def query(path: str, ids: dict[str, list], optional: bool = False) -> str:
    """The route's required query parameters, or all of them, with real values."""
    parameters = SCHEMA["paths"][path]["get"].get("parameters", [])
    names = [p["name"] for p in parameters if p["in"] == "query" and (optional or p.get("required"))]
    return "?" + "&".join(f"{name}={ids[name][0]}" for name in names) if names else ""


# Routes that answer with a server error today: each is a bug, and its tests pass once it is fixed
BUGS = {
    "/documents/{document_id}/pages/{number}/vertices": "Bug: a page past a PDF's last one fails in PDFium "
    "(500, quantix/api/takeoff.py get_vertices), not 404",
}


def known(paths: list[str]) -> list:
    return [pytest.param(p, marks=pytest.mark.xfail(strict=True, reason=BUGS[p])) if p in BUGS else p for p in paths]


@pytest.mark.parametrize("header", [None, "Bearer wrong"])
@pytest.mark.parametrize(("method", "path"), [r for r in ROUTES if r[1] != "/health"])
def test_every_route_but_health_refuses_a_request_without_the_launch_token(seeded, method, path, header):
    client, _ = seeded
    bare = TestClient(client.app, headers={"Authorization": header} if header else {})
    assert bare.request(method, re.sub(r"\{\w+\}", "1", path)).status_code == 401


@pytest.mark.parametrize("path", known(GETS))
def test_every_page_opens_on_a_tender_with_work_of_every_kind_and_never_shows_a_key(seeded, path):
    client, ids = seeded
    for values in combinations(path, ids):
        for url in (path.format(**values) + query(path, ids), path.format(**values) + query(path, ids, True)):
            response = client.get(url)
            assert response.status_code < 500, f"{url}: {response.text}"
            assert KEY not in response.text and WEB_KEY not in response.text


@pytest.mark.parametrize("path", known([p for p in GETS if "{" in p]))
def test_what_isnt_there_is_not_found(seeded, path):
    client, ids = seeded
    found = combinations(path, ids)
    real = next((v for v in found if client.get(path.format(**v) + query(path, ids)).status_code == 200), found[0])
    parameters = SCHEMA["paths"][path]["get"]["parameters"]
    for p in (p for p in parameters if p["in"] == "path"):
        missing = 99999 if p["schema"].get("type") == "integer" else "0" * 32
        url = path.format(**real | {p["name"]: missing}) + query(path, ids)
        response = client.get(url)
        assert response.status_code == 404, f"{url}: {response.status_code} {response.text}"
