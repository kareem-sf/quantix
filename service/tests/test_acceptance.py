"""A synthetic tender goes end to end through every gate: the real office runtime with a scripted model for the
staff, and the engineer deciding through the API, from the package to the built submission."""

import re

import openpyxl
from pydantic_ai.messages import ModelResponse, TextPart, ToolCallPart, ToolReturnPart, UserPromptPart
from pydantic_ai.models.function import FunctionModel
from test_documents import make_pdf, read_all, upload
from test_office import wait_for
from test_submission import client_bill

from quantix import settings
from quantix.office.models import TEAM

ITT = make_pdf(
    [
        [
            "Instructions to Tenderers",
            "Prices shall be in Saudi Riyals (SAR).",
            "VAT at 15% shall be shown separately.",
            "7.1 The priced bill of quantities shall be submitted in the format provided.",
            "7.6 The tenderer shall submit a method statement for concrete works.",
        ]
    ]
)
GULF = make_pdf([["Gulf Waterproofing quotation", "6.3 Waterproofing 35.00 SAR per m2", "Excludes protection board"]])
NAJD = make_pdf([["Najd Contracting offer", "Item 6.3 waterproofing 36.50 SAR"]])
RANIA = {
    "name": "Rania Farouk",
    "discipline": "Civil",
    "experience_years": 19,
    "background": "Led tenders for schools and clinics.",
    "working_style": "Plans the week on Sunday.",
    "opinions": "Distrusts quantities nobody has measured.",
    "voice": "Short and direct.",
}


def new_for_me(messages) -> str:
    prompt = next(str(p.content) for m in messages for p in m.parts if isinstance(p, UserPromptPart))
    return prompt.split("New for you:")[-1]


def phases(docs):
    bill, itt, gulf, najd = (docs[n]["id"] for n in ("Bill.xlsx", "ITT.pdf", "Gulf.pdf", "Najd.pdf"))
    rows = [("3.1", "Excavation", "m3", "1240"), ("4.3", "Slab reinforcement", "t", "28.1")]
    rows.append(("6.3", "Waterproofing", "m2", "980"))
    items = [
        {
            "item": i,
            "description": d,
            "unit": u,
            "quantity": q,
            "document_id": bill,
            "page": 1,
            "quote": f"A{n}={i} | B{n}={d} | C{n}={u} | D{n}={q}",
        }
        for n, (i, d, u, q) in enumerate(rows, start=2)
    ]
    fact = {"document_id": itt, "page": 1}
    estimate_note = "Plant output 4.5 m3/hr at 83.25 per hour; no disposal off site."
    return {
        "enter the BOQ": [
            ("propose_boq_items", {"items": items}),
            ("propose_fact", {"kind": "currency", "value": "SAR", "quote": "Saudi Riyals (SAR)", **fact}),
            ("propose_fact", {"kind": "vat", "value": "15%", "quote": "VAT at 15%", **fact}),
            ("complete_task", {"task_id": "<task>", "result": "3 BOQ items and 2 facts, ITT.pdf page 1."}),
        ],
        "PRICE": [
            ("propose_rate", {"boq_item": "3.1", "basis": "estimate", "unit_rate": "18.50", "note": estimate_note}),
            ("propose_rate", {"boq_item": "4.3", "basis": "estimate", "unit_rate": "3488.00", "note": estimate_note}),
            (
                "propose_markups",
                {"preliminaries": "0.10", "overheads": "0", "profit": "0", "adjustment": "0", "note": "Site costs."},
            ),
        ],
        "SUBCONTRACT": [
            ("add_company", {"name": "Gulf Waterproofing", "kind": "subcontractor", "trades": "Waterproofing"}),
            ("add_company", {"name": "Najd Contracting", "kind": "subcontractor", "trades": "Waterproofing"}),
            ("create_package", {"name": "Waterproofing", "kind": "subcontract", "boq_items": ["6.3"]}),
            (
                "record_quote",
                {
                    "package": "Waterproofing",
                    "company": "Gulf Waterproofing",
                    "document_id": gulf,
                    "lines": [{"boq_item": "6.3", "rate": "35.00", "page": 1, "quote": "6.3 Waterproofing 35.00"}],
                    "exclusions": [
                        {"description": "Protection board", "amount": "2000", "page": 1, "quote": "Excludes protection"}
                    ],
                },
            ),
            (
                "record_quote",
                {
                    "package": "Waterproofing",
                    "company": "Najd Contracting",
                    "document_id": najd,
                    "lines": [{"boq_item": "6.3", "rate": "36.50", "page": 1, "quote": "6.3 waterproofing 36.50"}],
                },
            ),
            ("levelling", {"package": "Waterproofing"}),
            (
                "recommend_quote",
                {"package": "Waterproofing", "company": "Najd Contracting", "reason": "Lowest once levelled."},
            ),
        ],
        "SUBMISSION": [
            (
                "add_requirements",
                {
                    "requirements": [
                        {
                            "section": "Commercial",
                            "title": "Priced bill of quantities",
                            "quote": "7.1 The priced bill of quantities",
                            **fact,
                        },
                        {
                            "section": "Technical",
                            "title": "Method statement for concrete works",
                            "quote": "7.6 The tenderer shall submit a method statement",
                            **fact,
                        },
                    ]
                },
            ),
            (
                "draft_document",
                {"requirement": "Method statement for concrete works", "title": "Method statement", "text": "Pour."},
            ),
            (
                "set_pricing_columns",
                {
                    "document_id": bill,
                    "sheet": 1,
                    "rate_column": "E",
                    "amount_column": "F",
                    "header_quote": "E1=Rate | F1=Amount",
                },
            ),
        ],
    }


def test_a_synthetic_tender_goes_through_every_gate_to_a_built_package(client, tmp_path):
    tender_id = client.post("/tenders", json={"name": "Synthetic school", "due_date": "2026-10-14"}).json()["id"]
    upload(client, tender_id, {"Bill.xlsx": client_bill(), "ITT.pdf": ITT, "Gulf.pdf": GULF, "Najd.pdf": NAJD})
    script = phases(read_all(client, tender_id))
    settings.save(tmp_path, office_ai={"connection_id": "scripted", "model": "brain"})
    replies: list[str] = []

    def brain(messages, info):
        if info.output_tools:  # appointing the Tender Manager
            return ModelResponse(parts=[ToolCallPart(info.output_tools[0].name, RANIA)])
        done = [str(p.content) for m in messages for p in m.parts if isinstance(p, ToolReturnPart)]
        new = new_for_me(messages)
        if "You are Rania Farouk" in info.instructions and "Please price this tender" in new:
            steps = [
                (
                    "hire",
                    {
                        "name": "Layla Nasser",
                        "role": "Estimator",
                        "discipline": "Quantity surveying",
                        "experience_years": 12,
                        "background": "Priced schools across the region.",
                        "working_style": "Checks every figure twice.",
                        "opinions": "Won't guess a rate she can source.",
                        "voice": "Plain and precise.",
                    },
                ),
                ("assign_task", {"staff_name": "Layla Nasser", "title": "enter the BOQ", "brief": "BOQ and facts."}),
            ]
        elif "You are Layla Nasser" in info.instructions:
            phase = next((key for key in script if key in new), None)
            steps = script.get(phase, [])
            replies[:] = done
        else:
            steps = []
        if len(done) >= len(steps):
            return ModelResponse(parts=[TextPart("Done.")])
        name, args = steps[len(done)]
        if args.get("task_id") == "<task>":
            prompt = next(str(p.content) for m in messages for p in m.parts if isinstance(p, UserPromptPart))
            args = {**args, "task_id": re.search(r"Your open tasks:\n- (\w+):", prompt).group(1)}
        return ModelResponse(parts=[ToolCallPart(name, args)])

    client.app.state.office.model = lambda: FunctionModel(brain)
    gates = lambda: client.get(f"/tenders/{tender_id}/gates").json()  # noqa: E731

    def tell_layla(text):
        client.post(f"/tenders/{tender_id}/messages", json={"channel": layla, "text": text})

    # The engineer asks; the Manager hires and briefs; Layla enters the BOQ and the tender's facts.
    client.post(f"/tenders/{tender_id}/messages", json={"channel": TEAM, "text": "Please price this tender."})
    state = wait_for(lambda o: gates()["boq"] == 3 and gates()["facts"] == 2, client, tender_id)
    layla = next(m["id"] for m in state["staff"] if m["name"] == "Layla Nasser")
    assert client.post(f"/tenders/{tender_id}/boq/approve-all").json() == {"approved": 3}
    for fact in client.get(f"/tenders/{tender_id}/boq").json()["facts"]:
        client.post(f"/facts/{fact['id']}/decision", json={"approve": True})

    # Pricing gate.
    tell_layla("PRICE: please price 3.1 and 4.3 and propose the markups.")
    wait_for(lambda o: gates()["pricing"] == 3, client, tender_id)
    client.post(f"/tenders/{tender_id}/rates/approve-all")
    markups = client.get(f"/tenders/{tender_id}/estimate").json()["markups"]
    client.post(f"/markups/{markups['id']}/decision", json={"approve": True})

    # Subcontract gate: Gulf 34,300.00 + 2,000.00 exclusion = 36,300.00; Najd 35,770.00 ranks first.
    tell_layla("SUBCONTRACT: get the waterproofing quoted and levelled.")
    wait_for(lambda o: gates()["subcontract"] == 1, client, tender_id)
    [package] = client.get(f"/tenders/{tender_id}/packages").json()
    assert [(q["company"], q["levelled_total"], q["rank"]) for q in package["quotes"]] == [
        ("Gulf Waterproofing", "36300.00", 2),
        ("Najd Contracting", "35770.00", 1),
    ]
    client.post(f"/packages/{package['id']}/choice", json={"quote_id": package["recommended_quote_id"]})

    # Submission gate.
    tell_layla("SUBMISSION: prepare the submission checklist.")
    wait_for(lambda o: gates()["submission"] == 1, client, tender_id)
    checklist = client.get(f"/tenders/{tender_id}/submission").json()["requirements"]
    priced_boq, method = checklist
    client.post(f"/drafts/{method['draft']['id']}/decision", json={"approve": True})
    client.post(f"/requirements/{priced_boq['id']}/ready", json={"ready": True, "note": "Attached as the workbook."})
    assert gates() == {"boq": 0, "facts": 0, "takeoff": 0, "pricing": 0, "subcontract": 0, "submission": 0}

    # Net 22,940.00 + 98,012.80 + 35,770.00 = 156,722.80; preliminaries 10% = 15,672.28; total 172,395.08.
    summary = client.get(f"/tenders/{tender_id}/estimate").json()["summary"]
    assert (summary["net"], summary["total"], summary["vat"], summary["total_with_vat"]) == (
        "156722.80",
        "172395.08",
        "25859.26",
        "198254.34",
    )
    built = client.post(f"/tenders/{tender_id}/export", json={"spread_markups": True}).json()
    assert (built["priced_total"], built["summary_total"], built["not_ready"]) == ("172395.08", "172395.08", [])
    sheet = openpyxl.load_workbook(tmp_path / "exports" / built["folder"] / "Priced Bill.xlsx").active
    assert [sheet[f"E{r}"].value for r in (2, 3, 4)] == [20.35, 3836.8, 40.15]
    assert len(replies) == 3 and replies[-1] == "Rates will go in column E and amounts in column F."
