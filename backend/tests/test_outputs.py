import hashlib
import importlib
import json
from zipfile import ZipFile

import pytest
from docx import Document
from fastapi import FastAPI
from fastapi.testclient import TestClient
from openpyxl import load_workbook
from test_estimates import approval, rate, seed

from quantix.estimates import EstimateService
from quantix.repository import Repository


@pytest.fixture
def setup(tmp_path):
    repo = Repository(tmp_path)
    tid = repo.create_tender("School foundations")["id"]
    seed(repo, tid)
    estimate = EstimateService(repo)
    estimate.refresh(tid)
    outputs = importlib.import_module("quantix.outputs").OutputService(repo)
    return repo, tid, estimate, outputs


def test_excel_contains_real_formulas_source_ids_and_incomplete_state(setup):
    repo, tid, estimate, outputs = setup
    item = estimate.view(tid)["items"][0]
    record = outputs.generate(tid, approval(kind="boq_xlsx"))
    path = outputs.path(tid, record["id"])
    with ZipFile(path) as archive:
        assert archive.testzip() is None
        assert not any("vbaProject" in name for name in archive.namelist())
    book = load_workbook(path, data_only=False)
    assert book.sheetnames == ["Summary", "BOQ", "Rate build-ups", "Sources"]
    assert book["BOQ"]["G7"].data_type == "f"
    assert book["BOQ"]["D7"].value == 12.5
    assert book["BOQ"]["E7"].value is None
    assert "incomplete" in book["Summary"]["B3"].value.lower()
    assert item["source_id"] in str(list(book["Sources"].values))
    assert record["status"] == "draft"
    assert record["pricing_complete"] is False
    assert record["sha256"] == hashlib.sha256(path.read_bytes()).hexdigest()
    manifest = json.loads(path.with_suffix(".json").read_text(encoding="utf-8"))
    assert manifest["source_ids"] == [item["source_id"]]


def test_priced_excel_snapshot_reconciles_decimal_totals_and_build_up(setup):
    repo, tid, estimate, outputs = setup
    item = estimate.view(tid)["items"][0]
    request = rate(vat_percent="14")
    request.pop("unit_rate")
    request["components"] = [
        {"name": "Concrete", "quantity": "1", "unit_rate": "10.10", "unit": "m3"}
    ]
    estimate.update_item(tid, item["id"], request)
    record = outputs.generate(tid, approval(kind="boq_xlsx"))
    book = load_workbook(outputs.path(tid, record["id"]), data_only=False)
    assert book["Summary"]["D7"].value == 143.93
    assert book["BOQ"]["E7"].value == 12.5
    assert book["BOQ"]["H7"].value == 0.14
    assert book["Rate build-ups"]["E7"].data_type == "f"
    assert record["pricing_complete"] is True


def test_word_report_preserves_findings_states_and_sources(setup):
    repo, tid, estimate, outputs = setup
    source_id = estimate.view(tid)["items"][0]["source_id"]
    repo.add_finding(
        tid,
        "Groundwater allowance",
        "Dewatering extent remains unknown.",
        "assumption",
        [source_id],
    )
    repo.add_message(
        tid, "manager", "Concrete scope needs geotechnical clarification.", [source_id]
    )
    record = outputs.generate(tid, approval(kind="analysis_docx"))
    doc = Document(outputs.path(tid, record["id"]))
    text = "\n".join(p.text for p in doc.paragraphs)
    assert "Groundwater allowance" in text
    assert "proposed" in text
    assert "Concrete scope needs geotechnical clarification." in text
    assert source_id in text
    assert "Draft" in text
    assert "incomplete" in text.lower()
    assert doc.paragraphs[0].style.name == "Title"


def test_download_is_tender_scoped_and_rejects_modified_output(setup):
    repo, tid, estimate, outputs = setup
    record = outputs.generate(tid, approval(kind="boq_xlsx"))
    other = repo.create_tender("Other tender")["id"]
    with pytest.raises(KeyError):
        outputs.path(other, record["id"])
    outputs.path(tid, record["id"]).write_bytes(b"modified")
    with pytest.raises(ValueError, match="integrity|changed"):
        outputs.path(tid, record["id"])


def test_routes_require_explicit_engineer_decisions_and_publish_schemas(setup):
    repo, tid, estimate, outputs = setup
    router = importlib.import_module("quantix.estimate_routes").create_router(repo)
    app = FastAPI()
    app.include_router(router)
    client = TestClient(app)
    item = estimate.view(tid)["items"][0]
    assert client.get(f"/api/tenders/{tid}/estimate").status_code == 200
    denied = client.patch(
        f"/api/tenders/{tid}/estimate/items/{item['id']}", json={"unit_rate": "5"}
    )
    assert denied.status_code == 422
    created = client.post(f"/api/tenders/{tid}/outputs", json=approval(kind="boq_xlsx"))
    assert created.status_code == 200
    downloaded = client.get(f"/api/tenders/{tid}/outputs/{created.json()['id']}/download")
    assert downloaded.status_code == 200
    assert downloaded.content[:2] == b"PK"
    assert "EstimateView" in app.openapi()["components"]["schemas"]
