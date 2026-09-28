"""PDF drawings printed from CAD, measured by their own lines: strokes joined back into objects, pens as layers, words
and their letters, clipping, the Takeoff screen's copy, choosing and measuring objects with the page's scale, the
region the engineer clicks inside, the office's tools, newer copies, and points measured on a CAD drawing."""

import math
from decimal import Decimal

import numpy as np
import pytest
from drawings import make_drawing, plan
from pydantic_ai.messages import ModelResponse, TextPart, ToolCallPart, ToolReturnPart
from test_documents import read_all, upload
from test_office import scripted, wait_for
from test_revisions import newer

from quantix import settings
from quantix.documents import cad, library, vectors
from quantix.documents.models import Document
from quantix.office import records as office
from quantix.office.models import ENGINEER
from quantix.review import checks
from quantix.takeoff import drawings
from quantix.takeoff import records as takeoff
from quantix.takeoff.models import Measurement

WIDTH, HEIGHT = 842, 595
K = 0.5523  # a quarter circle's control points, as a share of its radius


def _circle(cx: float, cy: float, r: float) -> str:
    k = K * r
    return (
        f"{cx + r} {cy} m {cx + r} {cy + k} {cx + k} {cy + r} {cx} {cy + r} c "
        f"{cx - k} {cy + r} {cx - r} {cy + k} {cx - r} {cy} c {cx - r} {cy - k} {cx - k} {cy - r} {cx} {cy - r} c "
        f"{cx + k} {cy - r} {cx + r} {cy - k} {cx + r} {cy} c S"
    )


def plan_page() -> str:
    """A site plan as CAD prints it."""
    ops = [
        "1 0 0 RG 1 w",  # the site boundary in red, one stroke per side
        "100 100 m 500 100 l S",
        "500 100 m 500 400 l S",
        "500 400 m 100 400 l S",
        "100 400 m 100 100 l S",
        "0 0.8 0 RG 0.5 w",  # a road edge in green, printed as three strokes
        "100 450 m 300 450 l S",
        "300 450 m 300 500 l S",
        "300 500 m 500 500 l S",
        "0 0 1 RG 1 w",  # cable routes in blue meeting at a tee: three lines, not one
        "600 100 m 700 100 l S",
        "700 100 m 800 100 l S",
        "700 100 m 700 200 l S",
        "0 0 0 RG 0.5 w",
        _circle(650, 300, 20),  # a manhole
        "0 0 0 rg 600 400 m 640 400 l 640 440 l 600 440 l h f",  # a filled column
        "q 100 100 50 50 re W n 0 1 1 RG 0.25 w 90 90 m 160 160 l S Q",  # a hatch line cut to its boundary
        "1 1 1 RG 2 w 0 0 m 842 595 l S",  # white: it doesn't print
        "0.5 0.5 0.5 RG 0.25 w",  # a dashed fence: 200 dashes
        *[f"{100 + 3 * i} 560 m {102 + 3 * i} 560 l S" for i in range(200)],
        "0 0 0 RG 0.25 w",  # the letters of MH-1, printed as strokes
        "621 480 m 621 485 l S 621 485 m 624 482 l S 624 482 m 627 485 l S 627 485 m 627 480 l S",
        "BT /F1 8 Tf 3 Tr 620 480 Td (MH-1) Tj ET",  # and the word itself, written invisibly over them
        "BT /F1 10 Tf 0 Tr 280 80 Td (20.00) Tj ET",  # the dimension of the south boundary
    ]
    return "\n".join(ops)


def layered_page() -> str:
    """A page from a PDF that keeps the drawing's layers."""
    return "\n".join(
        [
            "/OC /L1 BDC 0 0 0 RG 1 w",
            *[f"{100 + 2 * i} 100 m {101 + 2 * i} 100 l S" for i in range(210)],
            "EMC /OC /L2 BDC 100 200 m 400 200 l S EMC",
        ]
    )


def make_vector_pdf(pages: list[str]) -> bytes:
    """A PDF whose pages draw the given content streams, with the two layers (C-ROAD and C-KERB) the second page
    uses; an empty page stands in for a scan."""
    objects = [
        "<< /Type /Catalog /Pages 2 0 R /OCProperties << /OCGs [4 0 R 5 0 R] /D << /Order [4 0 R 5 0 R] >> >> >>",
        "",
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
        "<< /Type /OCG /Name (C-ROAD) >>",
        "<< /Type /OCG /Name (C-KERB) >>",
    ]
    kids = []
    for stream in pages:
        objects.append(f"<< /Length {len(stream)} >>\nstream\n{stream}\nendstream")
        content = len(objects)
        objects.append(
            f"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {WIDTH} {HEIGHT}] /Resources << /Font << /F1 3 0 R >> "
            f"/Properties << /L1 4 0 R /L2 5 0 R >> >> /Contents {content} 0 R >>"
        )
        kids.append(f"{len(objects)} 0 R")
    objects[1] = f"<< /Type /Pages /Kids [{' '.join(kids)}] /Count {len(kids)} >>"
    out, offsets = "%PDF-1.5\n", []
    for number, body in enumerate(objects, start=1):
        offsets.append(len(out))
        out += f"{number} 0 obj\n{body}\nendobj\n"
    xref = len(out)
    out += f"xref\n0 {len(objects) + 1}\n0000000000 65535 f \n" + "".join(f"{o:010d} 00000 n \n" for o in offsets)
    out += f"trailer\n<< /Size {len(objects) + 1} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n"
    return out.encode("latin-1")


SITE = make_vector_pdf([plan_page(), layered_page(), ""])


def _by_type(d: cad.Drawing, page: int, kind: str) -> list[int]:
    return [i for i in range(len(d.index)) if d.index[i, 0] == page and d.type_of(i) == kind]


def _find(d: cad.Drawing, layer: str, kind: str | None = None) -> list[int]:
    return [
        i for i in range(len(d.index)) if d.layer_of(i).startswith(layer) and (kind is None or d.type_of(i) == kind)
    ]


def test_a_pdf_drawing_is_read_back_into_objects(tmp_path):
    files = tmp_path / "files"
    files.mkdir()
    stored = files / "site.pdf"
    stored.write_bytes(SITE)
    assert vectors.line_count(stored, 1) >= vectors.MIN_LINES and vectors.line_count(stored, 3) == 0
    d = vectors.page_drawing(stored, 1)
    assert d.space(1).label == "page 1" and d.space(1).extents == (0.0, 0.0, WIDTH, HEIGHT)

    boundary = _find(d, "red FF0000 · 0.35 mm")  # four strokes, one closed outline
    assert len(boundary) == 1 and d.flags(boundary[0]) & cad.CLOSED
    assert d.area(boundary[0]) == pytest.approx(400 * 300) and d.length(boundary[0]) == pytest.approx(1400)
    road = _find(d, "green 00CC00 · 0.18 mm")
    assert len(road) == 1 and not d.flags(road[0]) & cad.CLOSED and d.length(road[0]) == pytest.approx(450)
    assert len(_find(d, "blue 0000FF", "Line")) == 3  # three lines meet at the tee, so none is joined
    (manhole,) = _by_type(d, 1, "Circle")
    assert d.area(manhole) == pytest.approx(math.pi * 400, rel=1e-3)
    (column,) = _by_type(d, 1, "Hatch")
    assert d.area(column) == pytest.approx(1600) and d.layer_of(column) == "black 000000 · fill"
    (hatch,) = _find(d, "cyan 00FFFF")  # cut to its 50 × 50 boundary
    assert d.length(hatch) == pytest.approx(50 * math.sqrt(2)) and d.bbox(hatch) == pytest.approx((100, 100, 150, 150))
    assert not _find(d, "white")  # white doesn't print
    assert len(_find(d, "grey 808080")) == 200  # dashes stay dashes

    words = {d.texts[i].text: i for i in _by_type(d, 1, "Text")}
    assert set(words) == {"MH-1", "20.00"}
    mh = words["MH-1"]
    assert d.flags(mh) & cad.INVISIBLE and len(d.parts(mh)) == 4  # its letters are its own
    assert d.layer_of(mh) == "black 000000 · 0.09 mm" and d.length(mh) == 0
    assert not d.flags(words["20.00"]) & cad.INVISIBLE
    assert not [i for i in _find(d, "black 000000 · 0.09 mm") if d.type_of(i) != "Text"]  # no loose letter strokes

    layered = vectors.page_drawing(stored, 2)
    assert sorted(set(layered.layer_names)) == ["C-KERB", "C-ROAD"]
    assert len(_find(layered, "C-KERB")) == 1


@pytest.fixture
def site(client):
    tender_id = client.post("/tenders", json={"name": "Synthetic substation"}).json()["id"]
    upload(client, tender_id, {"Drawings/Site.pdf": SITE})
    drawing = read_all(client, tender_id)["Drawings/Site.pdf"]["id"]
    with client.app.state.sessions() as session:
        qs = office.hire(session, tender_id, "Omar Haddad", "Quantity Surveyor", {})
        session.commit()
        qs_id = qs.id
    return tender_id, drawing, qs_id


def _scale(client, tender_id, drawing):
    body = {
        "document_id": drawing,
        "page": 1,
        "line": [[100, HEIGHT - 100], [500, HEIGHT - 100]],
        "length_m": 20,
        "dimension": "20.00",
    }
    assert client.post(f"/tenders/{tender_id}/scales", json=body).status_code == 201  # 1 point = 0.05 m


def test_the_engineer_measures_a_pdf_drawings_objects(client, site):
    tender_id, drawing, _ = site
    sheet = client.get(f"/documents/{drawing}/pages/1/sheet").json()
    assert sheet["kind"] == "pdf" and sheet["lines"] is True
    assert client.get(f"/documents/{drawing}/pages/3/sheet").json()["lines"] is False  # a scan
    info = client.get(f"/documents/{drawing}/drawing", params={"page": 1}).json()
    assert {"name": "red FF0000 · 0.35 mm", "colour": "#FF0000"}.items() <= next(
        layer for layer in info["layers"] if layer["name"].startswith("red")
    ).items()
    assert info["pages"][0]["kind"] == "sheet" and info["units"] is None
    screen = client.get(f"/documents/{drawing}/pages/1/screen")
    assert screen.content[:4] == b"QXD2"
    scan = client.get(f"/documents/{drawing}/pages/3/screen")
    assert scan.status_code == 400 and "isn't drawn in lines" in scan.json()["detail"]

    stamp = info["stamp"]
    with client.app.state.sessions() as session:
        d = drawings.open_page(library.home_of(session), session.get(Document, drawing), 1)
    on_screen = list(cad.objects_on_screen(d, 1))
    boundary = on_screen.index(_find(d, "red")[0])
    chosen = client.post(f"/documents/{drawing}/pages/1/choose", json={"stamp": stamp, "objects": [boundary]})
    assert chosen.json()["length_m"] is None  # no scale yet: no metres

    _scale(client, tender_id, drawing)
    chosen = client.post(f"/documents/{drawing}/pages/1/choose", json={"stamp": stamp, "objects": [boundary]}).json()
    assert chosen["area_m2"] == 300.0 and chosen["length_m"] == 70.0 and chosen["keys"] == [d.keys[on_screen[boundary]]]

    body = {
        "document_id": drawing,
        "page": 1,
        "kind": "area",
        "label": "Site levelling",
        "unit": "m2",
        "choice": {"stamp": stamp, "objects": [boundary]},
    }
    made = client.post(f"/tenders/{tender_id}/drawing-measurements", json=body)
    assert made.status_code == 201 and made.json()["quantity"] == "300.000"
    listed = client.get(f"/tenders/{tender_id}/takeoff").json()
    (m,) = listed["measurements"]
    assert m["page"] == 1 and m["object_count"] == 1 and m["objects"] == [boundary] and m["quantity"] == "300.000"
    assert listed["sheets"][0]["lines"] is True

    # the region the engineer clicks inside, closed off by the lines themselves
    region = {"stamp": stamp, "point": [300, 250]}
    found = client.post(f"/documents/{drawing}/pages/1/region", json=region).json()
    assert found["area_m2"] == pytest.approx(300.0) and found["perimeter_m"] == pytest.approx(70.0)
    gap = client.post(f"/documents/{drawing}/pages/1/region", json={**region, "hidden": ["red FF0000 · 0.35 mm"]})
    assert gap.status_code == 400 and "has a gap" in gap.json()["detail"]
    stale = client.post(f"/documents/{drawing}/pages/1/region", json={**region, "stamp": "old"})
    assert stale.status_code == 409


def _by_rule(session, site, page: int, kind: str, label: str, rule: cad.Rule, unit: str) -> Measurement:
    tender_id, drawing, qs_id = site
    home = library.home_of(session)
    return takeoff.measure_drawing(session, home, tender_id, qs_id, drawing, page, kind, label, rule, unit, None, None)


def test_the_office_takes_off_from_a_pdf_drawing_by_rule(client, site):
    tender_id, drawing, qs_id = site
    _scale(client, tender_id, drawing)
    with client.app.state.sessions() as session:
        with pytest.raises(ValueError, match="isn't drawn in lines"):
            _by_rule(session, site, 3, "length", "Road", cad.Rule(types=["Line"]), "m")
        with pytest.raises(ValueError, match="A volume is taken only from a CAD drawing"):
            _by_rule(session, site, 1, "volume", "Road", cad.Rule(types=["Line"]), "m3")
        road = _by_rule(session, site, 1, "length", "Road edge", cad.Rule(layers=["green*"]), "m")
        manholes = _by_rule(session, site, 1, "count", "Manholes", cad.Rule(types=["Circle"]), "nr")
        session.commit()
        assert takeoff.quantity(session, road) == Decimal("22.500")  # 450 points at 0.05 m
        assert takeoff.quantity(session, manholes) == Decimal("1")
        assert road.page == 1 and len(road.entities) == 1
        found = [f.message for f in checks.for_record(session, library.home_of(session), "measurement", road)]
        assert not [f for f in found if "aren't in" in f]


def test_a_newer_copy_keeps_object_measurements_where_its_lines_are_unchanged(client, site):
    tender_id, drawing, qs_id = site
    _scale(client, tender_id, drawing)
    with client.app.state.sessions() as session:
        road = _by_rule(session, site, 1, "length", "Road edge", cad.Rule(layers=["green*"]), "m")
        takeoff.approve(session, road)
        session.commit()
        road_id = road.id
    upload(client, tender_id, {"Drawings/Site.pdf": SITE + b"\n% Addendum 1: the same drawing\n"})
    reissued = newer(client, tender_id, "Drawings/Site.pdf")
    with client.app.state.sessions() as session:
        road = session.get(Measurement, road_id)
        assert road.document_id == reissued["id"]  # the same lines: it rests on the newer copy
        assert takeoff.quantity(session, road) == Decimal("22.500")  # and so does the scale it uses


def test_the_engineer_measures_points_on_a_cad_drawing(client):
    tender_id = client.post("/tenders", json={"name": "Synthetic flat"}).json()["id"]
    upload(client, tender_id, {"A-101.dwg": make_drawing(plan())})
    drawing = read_all(client, tender_id)["A-101.dwg"]["id"]
    units = client.post(f"/tenders/{tender_id}/units", json={"document_id": drawing, "units": "millimetres"})
    assert units.status_code == 201
    body = {
        "document_id": drawing,
        "page": 1,
        "kind": "area",
        "label": "Bedroom screed",
        "unit": "m2",
        "points": [[5050, 200], [9800, 200], [9800, 7800], [5050, 7800]],
    }
    made = client.post(f"/tenders/{tender_id}/measurements", json=body)
    assert made.status_code == 201 and made.json()["quantity"] == "36.100"  # 4.75 m × 7.6 m
    layout = client.post(f"/tenders/{tender_id}/measurements", json={**body, "page": 2})
    assert layout.status_code == 400 and "model space" in layout.json()["detail"]
    off = client.post(f"/tenders/{tender_id}/measurements", json={**body, "points": [[0, 0], [90000, 0], [0, 1]]})
    assert off.status_code == 400 and "off" in off.json()["detail"]
    with client.app.state.sessions() as session:
        qs = office.hire(session, tender_id, "Omar Haddad", "Quantity Surveyor", {})
        with pytest.raises(ValueError, match="measure its objects with measure_drawing"):
            takeoff.measure(session, tender_id, qs.id, drawing, 1, "length", "Wall", [[0, 0], [10, 0]], "m", None, None)


def test_a_cad_drawing_keeps_its_colours(tmp_path):
    spec = {
        "insunits": 4,
        "layers": ["WALLS", "NOTES"],
        "layer_colours": {"WALLS": 1, "NOTES": 7},
        "blocks": [
            {
                "name": "LAMP",
                "base": [0, 0],
                "entities": [{"type": "circle", "centre": [0, 0], "radius": 100, "colour": 0}],
            }
        ],
        "entities": [
            {"type": "line", "layer": "WALLS", "from": [0, 0], "to": [1000, 0]},
            {"type": "line", "layer": "WALLS", "from": [0, 100], "to": [1000, 100], "colour": 5},
            {"type": "insert", "block": "LAMP", "at": [500, 500], "layer": "WALLS", "colour": 3},
            {"type": "line", "layer": "NOTES", "from": [0, 200], "to": [1000, 200]},
        ],
    }
    files = tmp_path / "files"
    files.mkdir()
    stored = files / "lamps.dwg"
    stored.write_bytes(make_drawing(spec))
    d = cad.drawing(stored)
    colour = {(d.type_of(i), round(d.bbox(i)[1])): int(d.colours[i]) for i in range(len(d.index)) if d.index[i, 0] == 1}
    assert colour[("Line", 0)] == 0xFF0000  # by layer: WALLS is red
    assert colour[("Line", 100)] == 0x0000FF  # its own colour
    assert colour[("Circle", 400)] == 0x00FF00  # by block: the lamp is placed in green
    assert colour[("Line", 200)] == 0xFFFFFF
    shown = cad.screen_colours(d, np.array([i for i in range(len(d.index)) if d.type_of(i) == "Line"]))
    assert 0xFFFFFF not in shown.tolist()  # white prints in ink on the screen's paper


def test_staff_take_off_a_pdf_drawing_through_their_tools(client, site, tmp_path):
    tender_id, drawing, qs_id = site
    _scale(client, tender_id, drawing)
    settings.save(tmp_path, office_ai={"connection_id": "scripted", "model": "brain"})
    seen: list[str] = []
    persona = {
        "name": "Rania Farouk",
        "discipline": "Civil",
        "experience_years": 19,
        "background": "Priced substations.",
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
        road = {"layers": ["green*"]}
        steps = [
            ToolCallPart("drawing_overview", {"document_id": drawing, "page": 1}),
            ToolCallPart("query_drawing", {"document_id": drawing, "page": 1, "rule": road}),
            ToolCallPart(
                "measure_drawing",
                {"document_id": drawing, "page": 1, "kind": "length", "label": "Road edge", "rule": road, "unit": "m"},
            ),
        ]
        seen[:] = done
        return ModelResponse(parts=[steps[len(done)]] if len(done) < len(steps) else [TextPart("Done.")])

    client.app.state.office.model = lambda: scripted(brain)
    client.app.state.office.sees_images = lambda: False
    with client.app.state.sessions() as session:
        office.post(session, tender_id, ENGINEER, qs_id, "Omar, take the road edge off the site plan.")
        session.commit()
    client.app.state.office.engineer_spoke(tender_id)
    wait_for(
        lambda o: [w["kind"] for w in client.get(f"/tenders/{tender_id}/review").json()].count("measurement") == 1,
        client,
        tender_id,
    )
    assert seen[0].startswith("Site.pdf, page 1: page 1.")
    assert "Scale: about 1:142 (approved)" in seen[0] and "- green 00CC00 · 0.18 mm: 1 Polyline" in seen[0]
    assert "length 450.000 drawing units = 22.500 m" in seen[1]
    assert seen[2] == "Measured Road edge: 1 objects, 22.500 m (Quantix's figure)."
