"""CAD drawings: reading, units at the gate, takeoff from the drawing's own objects, the layer map and rooms, the
drawing's own checks, newer copies, tender queries and the office's tools."""

import io
import math
from decimal import Decimal

import openpyxl
import pytest
from drawings import MAP, make_drawing, plan
from pydantic_ai.messages import ModelResponse, TextPart, ToolCallPart, ToolReturnPart
from test_documents import read_all, upload
from test_office import scripted, wait_for

from quantix import settings
from quantix.boq import records as boq
from quantix.documents import cad, library
from quantix.office import packs
from quantix.office import records as office
from quantix.office.models import ENGINEER
from quantix.review import checks, queries
from quantix.review import records as reviews
from quantix.takeoff import drawings, layers
from quantix.takeoff import records as takeoff
from quantix.takeoff.models import Measurement

PLAN = make_drawing(plan())


def bill() -> bytes:
    workbook = openpyxl.Workbook()
    sheet = workbook.active
    rows = (
        ["8.1", "Aluminium windows", "nr", 3],
        ["8.2", "Porcelain floor tiles to bedroom", "m2", 30],
        ["8.3", "Skirting", "m", 50],
        ["8.4", "Blockwork walls", "m", 70.2],
    )
    for row in rows:
        sheet.append(row)
    buffer = io.BytesIO()
    workbook.save(buffer)
    return buffer.getvalue()


@pytest.fixture
def flat(client):
    tender_id = client.post("/tenders", json={"name": "Synthetic flat"}).json()["id"]
    upload(client, tender_id, {"Drawings/A-101.dwg": PLAN, "Bill.xlsx": bill()})
    documents = read_all(client, tender_id)
    drawing, workbook = documents["Drawings/A-101.dwg"]["id"], documents["Bill.xlsx"]["id"]
    with client.app.state.sessions() as session:
        qs = office.hire(session, tender_id, "Omar Haddad", "Quantity Surveyor", {})
        rows = [
            ("8.1", "Aluminium windows", "nr", "3", "A1=8.1 | B1=Aluminium windows | C1=nr | D1=3"),
            (
                "8.2",
                "Porcelain floor tiles to bedroom",
                "m2",
                "30",
                "A2=8.2 | B2=Porcelain floor tiles to bedroom | C2=m2 | D2=30",
            ),
            ("8.3", "Skirting", "m", "50", "A3=8.3 | B3=Skirting | C3=m | D3=50"),
            ("8.4", "Blockwork walls", "m", "70.2", "A4=8.4 | B4=Blockwork walls | C4=m | D4=70.2"),
        ]
        lines = [
            boq.ItemIn(item=i, description=d, unit=u, quantity=Decimal(q), document_id=workbook, page=1, quote=quote)
            for i, d, u, q, quote in rows
        ]
        assert boq.propose_items(session, tender_id, qs, lines).startswith("Saved 4")
        session.commit()
        qs_id = qs.id
    return tender_id, drawing, qs_id


def measure(client, tender_id, drawing, rule, **body):
    choice = {"stamp": stamp(client, drawing), "rule": rule}
    return client.post(
        f"/tenders/{tender_id}/drawing-measurements", json={"document_id": drawing, "choice": choice, **body}
    )


def stamp(client, drawing):
    return client.get(f"/documents/{drawing}/drawing").json()["stamp"]


def approve_map(client, tender_id, drawing):
    with client.app.state.sessions() as session:
        layer_map = layers.propose(
            session,
            client.app.state.home,
            tender_id,
            ENGINEER,
            drawing,
            MAP,
            {"DOOR-900": "doors", "WIN-1200": "windows"},
            "From what each layer holds.",
            "approved",
        )
        session.commit()
        return layer_map.id


def test_a_drawing_is_read_into_pages_of_its_words(client):
    tender_id = client.post("/tenders", json={"name": "Synthetic flat"}).json()["id"]
    upload(client, tender_id, {"A-101.dwg": PLAN, "A-102.dxf": make_drawing(plan(), ".dxf")})
    documents = read_all(client, tender_id)
    for name in ("A-101.dwg", "A-102.dxf"):
        d = documents[name]
        assert (d["kind"], d["status"], d["page_count"]) == ("cad", "read", 2)
        model = client.get(f"/documents/{d['id']}/pages/1").json()["text"]
        assert model.startswith("Model space") and "LIVING" in model and "BEDROOM" in model
        assert "Dimension texts written by hand: 9800" in model
        layout = client.get(f"/documents/{d['id']}/pages/2").json()["text"]
        assert layout.startswith("Layout: A-101") and "ALL DIMENSIONS ARE IN MILLIMETRES" in layout
    assert any(hit["name"] == "A-101.dwg" for hit in client.get(f"/tenders/{tender_id}/search?q=bedroom").json())
    image = client.get(f"/documents/{documents['A-101.dwg']['id']}/pages/1/image")
    assert image.status_code == 200 and image.content.startswith(b"\x89PNG")


def test_a_clipped_block_counts_only_what_shows(tmp_path):
    """A block reference clipped to part of its block (CAD's XCLIP) is measured as the drawing shows it: lines cut
    where they leave the clip, an area kept inside it, and what lies wholly outside left out."""
    grid = {
        "name": "GRID",
        "base": [0, 0],
        "entities": [
            {"type": "line", "from": [0, 0], "to": [10000, 0]},
            {"type": "line", "from": [0, 5000], "to": [10000, 5000]},
            {"type": "line", "from": [9000, 0], "to": [9000, 5000]},
            {"type": "hatch", "loops": [[[0, 0], [10000, 0], [10000, 5000], [0, 5000]]]},
        ],
    }
    clipped = {
        "type": "insert",
        "block": "GRID",
        "layer": "GRID",
        "at": [1000, 1000],
        "rotation": 90,
        "clip": [[-5000, 500], [2000, 500], [2000, 9000], [-5000, 9000]],
    }
    stored = tmp_path / "files" / "grid.dwg"
    stored.parent.mkdir()
    stored.write_bytes(make_drawing({"insunits": 4, "layers": ["GRID"], "blocks": [grid], "entities": [clipped]}))
    d = cad.drawing(stored)
    assert d.info["read"]["clipped"] == 1
    lines = d.totals(d.select(1, cad.Rule(layers=["GRID"], types=["Line"])))
    assert (lines["count"], lines["length"]) == (2, 16000.0)
    hatch = d.totals(d.select(1, cad.Rule(layers=["GRID"], types=["Hatch"])))
    assert hatch["area"] == pytest.approx(40_000_000)


def test_a_drawing_it_refers_to_is_placed_once_the_package_holds_it(client):
    """A drawing that refers to another (an xref) warns until the package holds that drawing; then its objects are
    placed where the drawing refers to it, on layers named after it, and measured with the rest."""
    base = {
        "insunits": 4,
        "layers": ["A-WALL"],
        "entities": [
            {"type": "line", "layer": "A-WALL", "from": [0, 0], "to": [10000, 0]},
            {"type": "line", "layer": "A-WALL", "from": [10000, 0], "to": [10000, 8000]},
        ],
    }
    host = {
        "insunits": 4,
        "layers": ["X-REF", "A-FURN"],
        "blocks": [{"name": "BASE-PLAN", "base": [0, 0], "xref": "..\\Base\\Base-Plan.dwg", "entities": []}],
        "entities": [
            {"type": "insert", "block": "BASE-PLAN", "layer": "X-REF", "at": [100000, 50000]},
            {"type": "circle", "layer": "A-FURN", "centre": [102000, 52000], "radius": 500},
        ],
    }
    tender_id = client.post("/tenders", json={"name": "Synthetic xref"}).json()["id"]
    upload(client, tender_id, {"Plans/Host.dwg": make_drawing(host)})
    host_id = read_all(client, tender_id)["Plans/Host.dwg"]["id"]
    checks = client.get(f"/tenders/{tender_id}/checks?document_id={host_id}").json()
    assert any("refers to another drawing, ..\\Base\\Base-Plan.dwg" in p["message"] for p in checks)
    before = client.get(f"/documents/{host_id}/drawing").json()
    assert "BASE-PLAN|A-WALL" not in {layer["name"] for layer in before["layers"]}

    upload(client, tender_id, {"Base/Base-Plan.dwg": make_drawing(base)})
    read_all(client, tender_id)
    checks = client.get(f"/tenders/{tender_id}/checks?document_id={host_id}").json()
    assert not any("refers to another drawing" in p["message"] for p in checks)
    after = client.get(f"/documents/{host_id}/drawing").json()
    assert "BASE-PLAN|A-WALL" in {layer["name"] for layer in after["layers"]}
    assert after["stamp"] != before["stamp"]  # the objects on the screen are numbered again
    client.post(f"/tenders/{tender_id}/units", json={"document_id": host_id, "units": "millimetres"})
    walls = client.post(
        f"/documents/{host_id}/pages/1/choose",
        json={"stamp": after["stamp"], "rule": {"layers": ["BASE-PLAN|A-WALL"]}},
    ).json()
    assert (walls["count"], walls["length_m"]) == (2, 18.0)
    assert all(key.count("/") == 1 for key in walls["keys"])  # inside the reference to the base plan


def test_3d_solids_are_measured_by_volume(client):
    """A 3D solid's volume comes from its shape, placed as its block is (a rung at twice the size holds eight times
    the steel), and a density turns it into a weight."""
    bar = {"type": "cylinder", "at": [0, 0, 0], "radius": 10, "height": 500}
    rung = {"name": "RUNG", "base": [0, 0], "entities": [bar]}
    spec = {
        "insunits": 4,
        "layers": ["S-STEEL"],
        "blocks": [rung],
        "entities": [
            {"type": "box", "layer": "S-STEEL", "at": [1000, 1000, 50], "size": [2000, 300, 100]},
            {"type": "insert", "block": "RUNG", "layer": "S-STEEL", "at": [5000, 0]},
            {"type": "insert", "block": "RUNG", "layer": "S-STEEL", "at": [6000, 0], "scale": 2},
        ],
    }
    tender_id = client.post("/tenders", json={"name": "Synthetic steel"}).json()["id"]
    upload(client, tender_id, {"S-101.dwg": make_drawing(spec)})
    drawing = read_all(client, tender_id)["S-101.dwg"]["id"]
    client.post(f"/tenders/{tender_id}/units", json={"document_id": drawing, "units": "millimetres"})
    chosen = client.post(
        f"/documents/{drawing}/pages/1/choose",
        json={"stamp": stamp(client, drawing), "rule": {"types": ["Solid3D"]}},
    ).json()
    volume = 0.06 + math.pi * 0.01**2 * 0.5 * (1 + 8)
    assert chosen["count"] == 3 and chosen["volume_m3"] == pytest.approx(volume, abs=0.0005)
    steel = measure(
        client, tender_id, drawing, {"layers": ["S-STEEL"]}, kind="volume", label="Steel", unit="kg", multiplier="7850"
    )
    assert steel.status_code == 201, steel.text
    assert float(steel.json()["quantity"]) == pytest.approx(volume * 7850, abs=0.01)
    with client.app.state.sessions() as session:
        taken = session.get(Measurement, steel.json()["id"]).entities
    assert len(taken) == 3 and all("/" in key for key in taken[1:])  # the solids, not the references around them
    weightless = measure(client, tender_id, drawing, {"layers": ["S-STEEL"]}, kind="volume", label="Steel", unit="kg")
    assert weightless.status_code == 400 and "density" in weightless.text


def test_quantities_come_from_the_objects_once_the_units_are_approved(client, flat):
    tender_id, drawing, _ = flat
    windows = measure(client, tender_id, drawing, {"blocks": ["WIN-1200"]}, kind="count", label="Windows", unit="nr")
    assert windows.status_code == 201 and windows.json()["quantity"] == "3"  # a count needs no units
    tiles = measure(
        client, tender_id, drawing, {"layers": ["A-FLOR"]}, kind="area", label="Tiles", unit="m2", boq_item="8.2"
    ).json()
    assert tiles["quantity"] is None  # no units yet: nothing measured has an area

    units = client.post(f"/tenders/{tender_id}/units", json={"document_id": drawing, "units": "millimetres"})
    assert units.json()["name"] == "millimetres" and units.json()["status"] == "approved"

    w1 = measure(
        client, tender_id, drawing, {"attributes": {"TYPE": "W1"}}, kind="count", label="W1", unit="nr", boq_item="8.1"
    )
    walls = measure(
        client, tender_id, drawing, {"layers": ["A-WALL"]}, kind="length", label="Walls", unit="m", boq_item="8.4"
    )
    skirting = measure(
        client, tender_id, drawing, {"layers": ["A-SKRT*"]}, kind="length", label="Skirting", unit="m", boq_item="8.3"
    )
    assert w1.json()["quantity"] == "2" and skirting.json()["quantity"] == "0.000"  # nothing drawn on A-SKRT
    assert walls.json()["quantity"] == "70.200"
    takeoff_view = client.get(f"/tenders/{tender_id}/takeoff").json()
    by_label = {m["label"]: m for m in takeoff_view["measurements"]}
    assert by_label["Tiles"]["quantity"] == "35.100" and by_label["Tiles"]["object_count"] == 1
    assert by_label["Windows"]["objects"] and by_label["Skirting"]["object_count"] == 0
    results = {c["item"]: c["result"] for c in takeoff_view["comparison"]}
    assert results == {
        "8.1": "differs",
        "8.2": "differs",
        "8.3": "not_on_drawings",
        "8.4": "matches",
        None: "not_in_boq",
    }
    sheet = next(s for s in takeoff_view["sheets"] if s["document_id"] == drawing)
    assert (sheet["kind"], sheet["units"]) == ("cad", "millimetres")


def test_the_office_must_set_the_units_the_drawing_states(client, flat):
    tender_id, drawing, qs_id = flat
    home = client.app.state.home
    with client.app.state.sessions() as session:
        with pytest.raises(ValueError, match="header says its units are millimetres"):
            takeoff.set_units(session, home, tender_id, qs_id, drawing, 1.0)
        record = takeoff.set_units(session, home, tender_id, qs_id, drawing, 0.001)
        assert record.status == "proposed" and record.dimension == "millimetres, as the drawing's header says"
        detail = reviews.details(session, reviews.Pending("scale", record, qs_id, record.created_at))
        assert "The drawing's header says its units are millimetres." in detail


def test_the_layer_map_finds_the_rooms(client, flat):
    tender_id, drawing, _ = flat
    client.post(f"/tenders/{tender_id}/units", json={"document_id": drawing, "units": "millimetres"})
    approve_map(client, tender_id, drawing)
    rooms = {r["name"]: r for r in client.get(f"/documents/{drawing}/rooms").json()}
    assert set(rooms) == {"LIVING", "BEDROOM"}
    assert rooms["BEDROOM"]["area_m2"] == pytest.approx(36.1)  # the door swing stays in the room
    assert rooms["LIVING"]["area_m2"] == pytest.approx(36.19)  # the doorway to the far jamb, 100 x 900
    assert rooms["BEDROOM"]["perimeter_m"] == pytest.approx(2 * (4.75 + 7.6))
    bedroom = measure(
        client, tender_id, drawing, {"types": ["Room"], "rooms": ["bedroom"]}, kind="area", label="Bed", unit="m2"
    )
    assert bedroom.json()["quantity"] == "36.100"
    inside = measure(
        client, tender_id, drawing, {"layers": ["A-FLOR"], "rooms": ["BEDROOM"]}, kind="area", label="F", unit="m2"
    )
    assert inside.json()["quantity"] == "35.100"


def test_the_drawing_checks_find_what_to_look_at(client, flat):
    tender_id, drawing, _ = flat
    client.post(f"/tenders/{tender_id}/units", json={"document_id": drawing, "units": "millimetres"})
    approve_map(client, tender_id, drawing)
    measure(client, tender_id, drawing, {"blocks": ["WIN-1200"]}, kind="count", label="Windows", unit="nr")
    problems = client.get(f"/tenders/{tender_id}/checks?document_id={drawing}").json()
    found = [p["message"] for p in problems]
    twice = next(p for p in problems if "on layer A-TEMP" in p["message"])
    assert len(twice["objects"]) == len(twice["screen_objects"]) == 1  # one click shows it on the drawing
    assert any("“9800” where the drawing measures 10,000" in m for m in found)
    assert any("1 lines on layer A-TEMP" in m for m in found)
    assert any("Services cross fire-rated walls or floors at 1 place in A-101.dwg" in m for m in found)
    unmeasured = [m for m in found if "nothing measures them yet" in m]
    assert any("Doors (DOOR-900: 1 object)" in m for m in unmeasured)
    assert any("Floor finishes (A-FLOR" in m for m in unmeasured)
    assert not any("Windows and glazing" in m for m in unmeasured)  # measured


def test_a_symbol_drawn_as_loose_lines_is_found(client):
    """A door drawn as loose lines instead of the door block (exploded), turned a quarter, is found; a lone line of
    the same length as the door's leaf is not."""
    exploded = [
        {"type": "line", "layer": "A-DOOR", "from": [1000, 5000], "to": [1000, 5900]},
        {"type": "arc", "layer": "A-DOOR", "centre": [1000, 5000], "radius": 900, "start": 90, "end": 180},
        {"type": "line", "layer": "A-DOOR", "from": [7000, 6000], "to": [7900, 6000]},
    ]
    tender_id = client.post("/tenders", json={"name": "Synthetic flat"}).json()["id"]
    upload(client, tender_id, {"A-101.dwg": make_drawing(plan(exploded))})
    drawing = read_all(client, tender_id)["A-101.dwg"]["id"]
    approve_map(client, tender_id, drawing)
    problems = client.get(f"/tenders/{tender_id}/checks?document_id={drawing}").json()
    loose = [p for p in problems if "drawn as loose lines" in p["message"]]
    assert [p["message"] for p in loose] == [
        "1 copy of DOOR-900 (doors) in A-101.dwg is drawn as loose lines, not as the block, so a count of DOOR-900 "
        "misses it."
    ]
    assert len(loose[0]["objects"]) == 2


def test_the_boq_checks(client, flat):
    tender_id, drawing, _ = flat
    client.post(f"/tenders/{tender_id}/units", json={"document_id": drawing, "units": "millimetres"})
    measure(client, tender_id, drawing, {"layers": ["A-SKRT"]}, kind="length", label="S", unit="m", boq_item="8.3")
    found = [p["message"] for p in client.get(f"/tenders/{tender_id}/checks").json()]
    assert any(m.startswith("BOQ line 8.3 (Skirting) is billed but the office found nothing of it") for m in found)


def test_a_newer_copy_keeps_only_what_is_unchanged(client, flat):
    tender_id, drawing, _ = flat
    client.post(f"/tenders/{tender_id}/units", json={"document_id": drawing, "units": "millimetres"})
    measure(client, tender_id, drawing, {"layers": ["A-FLOR"]}, kind="area", label="Tiles", unit="m2", boq_item="8.2")
    w1 = measure(client, tender_id, drawing, {"attributes": {"TYPE": "W1"}}, kind="count", label="W1", unit="nr")
    assert w1.json()["quantity"] == "2"
    extra = [{"type": "insert", "block": "WIN-1200", "layer": "A-GLAZ", "at": [8000, 0], "attributes": {"TYPE": "W1"}}]
    upload(client, tender_id, {"Drawings/A-101.dwg": make_drawing(plan(extra))})
    documents = read_all(client, tender_id)
    newer = documents["Drawings/A-101.dwg"]["id"]
    assert newer != drawing
    moved = {m["label"]: m for m in client.get(f"/tenders/{tender_id}/takeoff").json()["measurements"]}
    assert moved["Tiles"]["document_id"] == newer  # the hatch is the same in the newer copy
    assert moved["W1"]["document_id"] == drawing  # the rule takes a window more there: do it again
    with client.app.state.sessions() as session:
        stale = takeoff.measurements(session, tender_id)
        w1_record = next(m for m in stale if m.label == "W1")
        found = checks.for_record(session, client.app.state.home, "measurement", w1_record)
        assert any("objects were added that its rule takes" in f.message for f in found)


def test_a_query_rests_on_its_sources_and_quantix_figures(client, flat):
    tender_id, drawing, qs_id = flat
    home = client.app.state.home
    client.post(f"/tenders/{tender_id}/units", json={"document_id": drawing, "units": "millimetres"})
    with client.app.state.sessions() as session:
        with pytest.raises(ValueError, match="is not on A-101.dwg, page 2"):
            queries.raise_query(
                session,
                home,
                tender_id,
                qs_id,
                "clarification",
                "Dimension disagrees with the drawing",
                "The overall dimension is written by hand.",
                "The overall length is written as 9800 where the plan is drawn 10000: please confirm the length.",
                [queries.QuerySource(document_id=drawing, page=2, quote="9800")],
            )
        keys = [k for k in drawings.open_drawing(home, session.get(library.Document, drawing)).keys if k]
        query = queries.raise_query(
            session,
            home,
            tender_id,
            qs_id,
            "missing",
            "Skirting to both rooms",
            "Skirting runs round both rooms but 12 m of it is not billed.",
            "The drawings show skirting round both rooms; no BOQ item covers it. Please add an item or confirm.",
            [
                queries.QuerySource(document_id=drawing, page=1, quote="LIVING"),
                queries.QuerySource(document_id=drawing, page=1, objects=keys[:2]),
            ],
        )
        found = queries.problems(session, query)
        assert any(key.startswith("query-figures") and "12" in message for key, _, message in found)
        assert any(key.startswith("query-covered") and "8.3" in message for key, _, message in found)
        session.commit()
    listed = client.get(f"/tenders/{tender_id}/queries").json()
    assert listed[0]["kind_label"] == "Missing from the BOQ" and listed[0]["sources"][1]["objects"] == keys[:2]


def test_staff_take_off_a_drawing_through_their_tools(client, flat, tmp_path):
    tender_id, drawing, qs_id = flat
    settings.save(tmp_path, office_ai={"connection_id": "scripted", "model": "brain"})
    seen: list[str] = []
    persona = {
        "name": "Rania Farouk",
        "discipline": "Civil",
        "experience_years": 19,
        "background": "Priced civil works for schools and clinics.",
        "working_style": "Checks every figure twice.",
        "opinions": "Distrusts quantities nobody has measured.",
        "voice": "Short and direct.",
    }

    def brain(messages, info):
        if info.output_tools:  # the Tender Manager's persona
            return ModelResponse(parts=[ToolCallPart(info.output_tools[0].name, persona)])
        if "You are Omar Haddad" not in info.instructions:
            return ModelResponse(parts=[TextPart("Done.")])
        done = [str(p.content) for m in messages for p in m.parts if isinstance(p, ToolReturnPart)]
        steps = [
            ToolCallPart("drawing_overview", {"document_id": drawing}),
            ToolCallPart("set_drawing_units", {"document_id": drawing, "units": "mm"}),
            ToolCallPart(
                "query_drawing",
                {"document_id": drawing, "rule": {"blocks": ["WIN-1200"]}, "group_by": "attribute:TYPE"},
            ),
            ToolCallPart(
                "measure_drawing",
                {
                    "document_id": drawing,
                    "kind": "count",
                    "label": "Windows",
                    "rule": {"blocks": ["WIN-1200"]},
                    "unit": "nr",
                    "boq_item": "8.1",
                },
            ),
            ToolCallPart("find_problems", {"document_id": drawing}),
        ]
        seen[:] = done
        return (
            ModelResponse(parts=[steps[len(done)]])
            if len(done) < len(steps)
            else ModelResponse(parts=[TextPart("Done.")])
        )

    client.app.state.office.model = lambda: scripted(brain)
    client.app.state.office.sees_images = lambda: False  # none of these needs an AI that reads images
    with client.app.state.sessions() as session:
        office.post(session, tender_id, ENGINEER, qs_id, "Omar, count the windows on A-101.")
        session.commit()
    client.app.state.office.engineer_spoke(tender_id)
    wait_for(
        lambda o: [w["kind"] for w in client.get(f"/tenders/{tender_id}/review").json()].count("measurement") == 1,
        client,
        tender_id,
    )
    assert seen[0].startswith("A-101.dwg, page 1: model space.")
    assert "- A-WALL: 6 Line, 1 Polyline" in seen[0] and "WIN-1200 ×3" in seen[0]
    assert seen[1] == "Units set as millimetres, for the Tender Manager's review and the engineer's approval."
    assert "- W1: 2 objects" in seen[2] and "- W2: 1 object" in seen[2]
    assert seen[3] == "Measured Windows: 3 objects, 3 nr (Quantix's figure)."
    assert "“9800” where the drawing measures 10,000" in seen[4]


def test_the_drawing_tools_need_no_eyes():
    drawings_pack = packs.WORK["drawings"]
    names = {t.__name__ for t in drawings_pack.reads + drawings_pack.produces}
    seeing = {t.__name__ for t in packs.SEEING}
    assert names - seeing == {
        "drawing_overview",
        "query_drawing",
        "find_problems",
        "set_drawing_units",
        "measure_drawing",
        "propose_layer_map",
        "raise_query",
    }


def test_the_screen_copy_and_choosing_objects(client, flat):
    tender_id, drawing, _ = flat
    client.post(f"/tenders/{tender_id}/units", json={"document_id": drawing, "units": "millimetres"})
    copy = client.get(f"/documents/{drawing}/pages/1/screen").content
    assert copy[:4] == b"QXD2"
    size = int.from_bytes(copy[4:8], "little")
    header = __import__("json").loads(copy[8 : 8 + size])
    assert header["objects"] > 30 and header["segments"] > 30 and "A-WALL" in header["layers"]
    chosen = client.post(
        f"/documents/{drawing}/pages/1/choose", json={"stamp": header["stamp"], "rule": {"layers": ["A-FLOR"]}}
    ).json()
    assert chosen["count"] == 1 and chosen["area_m2"] == pytest.approx(35.1)
    again = client.post(
        f"/documents/{drawing}/pages/1/choose", json={"stamp": header["stamp"], "objects": chosen["objects"]}
    )
    assert again.json()["keys"] == chosen["keys"]
    stale = client.post(f"/documents/{drawing}/pages/1/choose", json={"stamp": "old", "objects": [1]})
    assert stale.status_code == 409


def test_the_reader_is_the_one_quantix_built():
    assert cad.reader().exists()
