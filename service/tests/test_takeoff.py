import io
from decimal import Decimal

import openpyxl
import pytest
from pydantic_ai.messages import ModelResponse, TextPart, ToolCallPart, ToolReturnPart
from pydantic_ai.models.function import FunctionModel
from test_documents import make_pdf, read_all, upload
from test_office import wait_for

from quantix import settings
from quantix.boq import records as boq
from quantix.office import records as office

DRAWING = make_pdf([["A-101 GROUND FLOOR PLAN", "Grid A to E 40.00", "Scale 1:100"]])  # a 612 x 792 point page


def bill() -> bytes:
    workbook = openpyxl.Workbook()
    sheet = workbook.active
    for row in (["5.1", "External wall", "م", 40.5], ["6.1", "Ground slab", "m2", 50], ["7.1", "Doors", "nr", 5]):
        sheet.append(row)
    buffer = io.BytesIO()
    workbook.save(buffer)
    return buffer.getvalue()


@pytest.fixture
def sheet(client):
    tender_id = client.post("/tenders", json={"name": "Synthetic school"}).json()["id"]
    upload(client, tender_id, {"A-101.pdf": DRAWING, "Bill.xlsx": bill()})
    documents = read_all(client, tender_id)
    drawing, workbook = documents["A-101.pdf"]["id"], documents["Bill.xlsx"]["id"]
    with client.app.state.sessions() as session:
        qs = office.hire(session, tender_id, "Omar Haddad", "Quantity Surveyor", {})
        rows = [
            ("5.1", "External wall", "م", "40.5", "A1=5.1 | B1=External wall | C1=م | D1=40.5"),
            ("6.1", "Ground slab", "m2", "50", "A2=6.1 | B2=Ground slab | C2=m2 | D2=50"),
            ("7.1", "Doors", "nr", "5", "A3=7.1 | B3=Doors | C3=nr | D3=5"),
        ]
        lines = [
            boq.ItemIn(item=i, description=d, unit=u, quantity=Decimal(q), document_id=workbook, page=1, quote=quote)
            for i, d, u, q, quote in rows
        ]
        assert boq.propose_items(session, tender_id, qs, lines, False).startswith("Saved 3")
        session.commit()
        qs_id = qs.id
    return tender_id, drawing, qs_id


def scale(client, tender_id, drawing, length_m=40.0, dimension="40.00"):
    body = {
        "document_id": drawing,
        "page": 1,
        "line": [[100, 100], [500, 100]],
        "length_m": length_m,
        "dimension": dimension,
    }
    return client.post(f"/tenders/{tender_id}/scales", json=body)


def measure(client, tender_id, drawing, **body):
    return client.post(f"/tenders/{tender_id}/measurements", json={"document_id": drawing, "page": 1, **body})


def test_quantities_come_from_the_geometry_and_the_scale(client, sheet):
    tender_id, drawing, _ = sheet
    assert scale(client, tender_id, drawing).json()["metres_per_point"] == pytest.approx(0.1)

    wall = measure(
        client,
        tender_id,
        drawing,
        kind="length",
        label="External wall",
        unit="m",
        points=[[100, 200], [300, 200], [300, 400]],
        boq_item="5.1",
    ).json()
    slab = measure(
        client,
        tender_id,
        drawing,
        kind="area",
        label="Ground slab",
        unit="m2",
        points=[[100, 500], [200, 500], [200, 550], [100, 550]],
        boq_item="6.1",
    ).json()
    concrete = measure(
        client,
        tender_id,
        drawing,
        kind="area",
        label="Slab concrete",
        unit="m3",
        multiplier="0.2",
        points=[[100, 500], [200, 500], [200, 550], [100, 550]],
    ).json()
    doors = measure(
        client,
        tender_id,
        drawing,
        kind="count",
        label="Doors",
        unit="nr",
        points=[[50, 50], [60, 60], [70, 70]],
        boq_item="7.1",
    ).json()
    assert [wall["quantity"], slab["quantity"], concrete["quantity"], doors["quantity"]] == [
        "40.000",
        "50.000",
        "10.000",
        "3",
    ]
    assert wall["status"] == "approved" and wall["boq_item"] == "5.1"

    takeoff = client.get(f"/tenders/{tender_id}/takeoff").json()
    assert [(s["name"], s["page"], s["width"]) for s in takeoff["sheets"]] == [("A-101.pdf", 1, 612.0)]
    results = {(r["item"], r["description"]): (r["result"], r["difference"]) for r in takeoff["comparison"]}
    assert results[("5.1", "External wall")] == ("matches", "-0.0123")  # 40 m against 40.5 in the Arabic unit م
    assert results[("6.1", "Ground slab")] == ("matches", "0.0000")
    assert results[("7.1", "Doors")] == ("differs", "-0.4000")
    assert results[(None, "Slab concrete")] == ("not_in_boq", None)


def test_a_new_scale_recomputes_the_sheet(client, sheet):
    tender_id, drawing, _ = sheet
    scale(client, tender_id, drawing)
    wall = measure(
        client, tender_id, drawing, kind="length", label="Wall", unit="m", points=[[100, 200], [500, 200]]
    ).json()
    assert wall["quantity"] == "40.000"
    scale(client, tender_id, drawing, length_m=80.0)
    [again] = client.get(f"/tenders/{tender_id}/takeoff").json()["measurements"]
    assert again["quantity"] == "80.000"


def test_the_rules_are_explained_when_broken(client, sheet):
    tender_id, drawing, _ = sheet
    refused = scale(client, tender_id, drawing, dimension="37.50")
    assert refused.status_code == 400 and "“37.50” is not printed on A-101.pdf, page 1" in refused.json()["detail"]
    assert (
        measure(client, tender_id, drawing, kind="length", label="x", unit="m", points=[[0, 0], [700, 0]])
        .json()["detail"]
        .startswith("A point is off the page")
    )
    no_thickness = measure(
        client, tender_id, drawing, kind="area", label="x", unit="m3", points=[[0, 0], [1, 0], [1, 1]]
    )
    assert no_thickness.json()["detail"].startswith("Give a height")
    wrong_item = measure(
        client, tender_id, drawing, kind="count", label="x", unit="nr", points=[[1, 1]], boq_item="9.9"
    )
    assert wrong_item.json()["detail"] == "There is no BOQ item 9.9. Use list_boq to see the items."
    unscaled = measure(
        client, tender_id, drawing, kind="length", label="Wall", unit="m", points=[[1, 1], [2, 2]]
    ).json()
    assert unscaled["quantity"] is None


def test_the_engineer_can_remove_a_measurement_to_redo_it(client, sheet):
    tender_id, drawing, _ = sheet
    scale(client, tender_id, drawing)
    m = measure(client, tender_id, drawing, kind="count", label="Doors", unit="nr", points=[[1, 1]]).json()
    assert client.delete(f"/measurements/{m['id']}").status_code == 204
    assert client.get(f"/tenders/{tender_id}/takeoff").json()["measurements"] == []


def test_staff_measure_through_their_tools(client, sheet, tmp_path):
    tender_id, drawing, qs_id = sheet
    settings.save(tmp_path, office_ai={"connection_id": "scripted", "model": "brain"})
    px = 1600 / 612  # view_page pixels per page point
    persona = {
        "name": "Rania Farouk",
        "discipline": "Civil",
        "experience_years": 19,
        "background": "b",
        "working_style": "w",
        "opinions": "o",
        "voice": "v",
    }
    seen: list[str] = []

    def brain(messages, info):
        if info.output_tools:
            return ModelResponse(parts=[ToolCallPart(info.output_tools[0].name, persona)])
        if "You are Omar Haddad" not in info.instructions:
            return ModelResponse(parts=[TextPart("Done.")])
        done = [str(p.content) for m in messages for p in m.parts if isinstance(p, ToolReturnPart)]
        steps = [
            ToolCallPart("find_on_page", {"document_id": drawing, "page": 1, "text": "40.00"}),
            ToolCallPart(
                "set_scale",
                {
                    "document_id": drawing,
                    "page": 1,
                    "from_xy": [100 * px, 100 * px],
                    "to_xy": [500 * px, 100 * px],
                    "length_m": 40,
                    "dimension_text": "40.00",
                },
            ),
            ToolCallPart(
                "measure",
                {
                    "document_id": drawing,
                    "page": 1,
                    "kind": "length",
                    "label": "External wall",
                    "points": [[100 * px, 200 * px], [500 * px, 200 * px]],
                    "unit": "m",
                    "boq_item": "5.1",
                },
            ),
        ]
        seen[:] = done
        return (
            ModelResponse(parts=[steps[len(done)]])
            if len(done) < len(steps)
            else ModelResponse(parts=[TextPart("Done.")])
        )

    client.app.state.office.model = lambda: FunctionModel(brain)
    with client.app.state.sessions() as session:
        office.post(session, tender_id, "engineer", qs_id, "Omar, measure the external wall on A-101.")
        session.commit()
    client.app.state.office.engineer_spoke(tender_id)
    wait_for(lambda o: client.get(f"/tenders/{tender_id}/gates").json()["takeoff"] == 2, client, tender_id)

    assert seen[0].startswith("“40.00” at left")
    assert seen[2] == "Measured External wall: 40.000 m."
    [m] = client.get(f"/tenders/{tender_id}/takeoff").json()["measurements"]
    assert (m["status"], m["quantity"], m["boq_item"]) == ("proposed", "40.000", "5.1")


def test_sloppy_points_snap_onto_the_drawing(client):
    """A plan drawn 140 x 70 mm with a 40.00 dimension along the long side: 40 m by 20 m, 800 m²."""
    from pathlib import Path

    from quantix.takeoff.records import snap

    plan = (Path(__file__).parent / "fixtures" / "synthetic-plan.pdf").read_bytes()
    tender_id = client.post("/tenders", json={"name": "Synthetic school"}).json()["id"]
    upload(client, tender_id, {"A-102.pdf": plan})
    drawing = read_all(client, tender_id)["A-102.pdf"]["id"]

    vertices = client.get(f"/documents/{drawing}/pages/1/vertices").json()
    assert [85.04, 170.08] in vertices and [481.89, 368.51] in vertices
    assert snap([[88, 172], [300, 300]], tuple(map(tuple, vertices))) == [[85.04, 170.08], [300, 300]]

    ends = snap([[82, 139], [484, 144]], tuple(map(tuple, vertices)))
    body = {"document_id": drawing, "page": 1, "line": ends, "length_m": 40, "dimension": "40.00"}
    assert client.post(f"/tenders/{tender_id}/scales", json=body).status_code == 201
    corners = snap([[88, 173], [479, 167], [484, 371], [82, 366]], tuple(map(tuple, vertices)))
    slab = measure(client, tender_id, drawing, kind="area", label="Slab", unit="m2", points=corners).json()
    assert slab["quantity"] == "800.020"  # the PDF stores 2-decimal coordinates: 40.000 m by 20.0005 m
