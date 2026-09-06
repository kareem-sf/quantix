"""Calibrated proposals use preserved synthetic PDFs and real local transactions."""

import hashlib
import importlib
import sqlite3
from dataclasses import asdict

import pytest
from fastapi import FastAPI
from fastapi.testclient import TestClient
from openpyxl import Workbook
from test_documents import pdf_bytes

from quantix.documents import PDFIUM_LOCK, extract_document
from quantix.estimates import EstimateService
from quantix.repository import Repository


def pdf_source(repo, tid, data=None):
    data = data or pdf_bytes([None])
    digest = hashlib.sha256(data).hexdigest()
    path = repo.objects / digest
    path.write_bytes(data)
    artifact, _ = repo.register_artifact(
        tid, "Drawings/Plan.pdf", digest, len(data), asdict(extract_document(path, "Plan.pdf"))
    )
    return artifact


@pytest.fixture
def setup(tmp_path):
    repo = Repository(tmp_path)
    tid = repo.create_tender("Generated measurement checks")["id"]
    artifact = pdf_source(repo, tid)
    module = importlib.import_module("quantix.measurements")
    assert hasattr(module, "MeasurementService"), "Persistent measurements are not implemented"
    return repo, tid, artifact, module.MeasurementService(repo)


def geometry(aid, **changes):
    return {
        "artifact_id": aid,
        "page": 1,
        "mode": "length",
        "points": [[0, 0], [0, 0.6]],
        "calibration_points": [[0, 0], [0.2, 0]],
        "calibration_metres": "4",
        **changes,
    }


def review(**values):
    return {
        "engineer_confirmed": True,
        "rationale": "Checked dimension and marked scope.",
        **values,
    }


def save(service, tid, aid, **changes):
    return service.create(tid, review(scope_label="East boundary", **geometry(aid, **changes)))


def boq(repo, tid, unit="m", broken=False):
    workbook = Workbook()
    sheet = workbook.active
    sheet.append(["Item", "Description", "Unit", "Quantity"])
    sheet.append(["B1", "Boundary fence", unit, "=#REF!" if broken else 12])
    path = repo.home / "synthetic.xlsx"
    workbook.save(path)
    repo.register_artifact(
        tid,
        "BOQ.xlsx",
        hashlib.sha256(path.read_bytes()).hexdigest(),
        path.stat().st_size,
        asdict(extract_document(path, "BOQ.xlsx")),
    )
    return EstimateService(repo).refresh(tid)["items"][0]


def test_saved_measurement_uses_pdf_dimensions_and_survives_restart(setup):
    repo, tid, artifact, service = setup
    result = save(service, tid, artifact["id"])
    assert result["quantity"] == "6"
    assert result["source"]["page_size"] == [200, 100]
    assert result["source"]["content_hash"] == artifact["content_hash"]
    assert result["status"] == "proposed"
    assert result["origin"] == "engineer"
    assert result["points"] == [[0, 0], [0, 0.6]]
    assert result["review_rationale"] == "Checked dimension and marked scope."
    reopened = type(service)(Repository(repo.home))
    assert reopened.get(tid, result["id"]) == result
    with repo.db.connect(write=True) as conn:
        with pytest.raises(sqlite3.IntegrityError, match="immutable"):
            conn.execute("UPDATE measurements SET data_json='{}' WHERE id=?", (result["id"],))


def test_calculation_is_read_only_and_does_not_create_review_or_evidence(setup):
    repo, tid, artifact, service = setup
    result = service.calculate(tid, geometry(artifact["id"]))
    assert result["quantity"] == "6"
    assert service.list(tid) == []
    assert repo.artifact_evidence(tid, artifact["id"]) == []


@pytest.mark.parametrize(
    "changes",
    [{"page": 0}, {"page": 2}, {"page": True}, {"page_size": [100, 100]}, {"pageSize": [100, 100]}],
)
def test_bogus_page_and_caller_dimensions_are_rejected_without_writes(setup, changes):
    _, tid, artifact, service = setup
    with pytest.raises(ValueError):
        save(service, tid, artifact["id"], **changes)
    assert service.list(tid) == []


def test_foreign_and_revised_sources_cannot_be_measured_or_linked(setup):
    repo, tid, artifact, service = setup
    saved = save(service, tid, artifact["id"])
    other = repo.create_tender("Other Tender")["id"]
    with pytest.raises(KeyError):
        service.get(other, saved["id"])
    with pytest.raises(KeyError):
        service.calculate(other, geometry(artifact["id"]))
    pdf_source(repo, tid, pdf_bytes(["Revision two"]))
    assert service.get(tid, saved["id"])["is_current"] is False
    with pytest.raises(ValueError, match="current|revision"):
        save(service, tid, artifact["id"])
    with pytest.raises(ValueError, match="current|revision"):
        service.link(tid, saved["id"], review(item_id="anything"))


def test_changed_source_bytes_fail_even_if_registered_hash_and_version_are_unchanged(setup):
    repo, tid, artifact, service = setup
    saved = save(service, tid, artifact["id"])
    repo.object_path(tid, artifact["id"]).write_bytes(pdf_bytes(["Changed bytes"]))
    with pytest.raises(ValueError, match="hash|changed|match"):
        service.calculate(tid, geometry(artifact["id"]))
    with pytest.raises(ValueError, match="hash|changed|match"):
        service.link(tid, saved["id"], review(item_id="anything"))


def test_count_on_image_only_page_creates_attributable_derived_evidence(setup):
    repo, tid, artifact, service = setup
    result = save(
        service,
        tid,
        artifact["id"],
        mode="count",
        points=[[0.2, 0.3]],
        calibration_points=None,
        calibration_metres=None,
    )
    evidence = repo.get_evidence(tid, result["source_id"])
    assert evidence["page"] == 1
    assert evidence["artifact_id"] == artifact["id"]
    assert evidence["kind"] == "measurement"
    assert evidence["metadata"]["origin"] == "engineer"
    assert "proposal" in evidence["text"].lower()
    assert repo.get_artifact(tid, artifact["id"])["status"] == artifact["status"]


def test_link_creates_proposal_preserves_supplied_quantity_and_requires_separate_approval(setup):
    repo, tid, artifact, service = setup
    item = boq(repo, tid, "lm")
    saved = save(service, tid, artifact["id"])
    proposal = service.link(tid, saved["id"], review(item_id=item["id"]))
    assert proposal["quantity"] == "6"
    assert proposal["source_ids"] == [saved["source_id"], item["source_id"]]
    assert saved["id"] in proposal["calculation"]
    estimates = EstimateService(repo)
    unchanged = estimates.view(tid)["items"][0]
    assert unchanged["supplied_quantity"] == unchanged["effective_quantity"] == "12"
    changed = estimates.approve_quantity(tid, proposal["id"], review())["items"][0]
    assert changed["supplied_quantity"] == "12"
    assert changed["effective_quantity"] == "6"
    assert service.get(tid, saved["id"])["links"][0]["proposal_id"] == proposal["id"]


@pytest.mark.parametrize("unit", ["m2", "m3", "item", "kg"])
def test_incompatible_boq_units_cannot_receive_length(setup, unit):
    repo, tid, artifact, service = setup
    item = boq(repo, tid, unit)
    saved = save(service, tid, artifact["id"])
    with pytest.raises(ValueError, match="unit"):
        service.link(tid, saved["id"], review(item_id=item["id"]))
    assert EstimateService(repo).view(tid)["items"][0]["quantity_proposals"] == []


def test_review_consent_and_rationale_are_required(setup):
    repo, tid, artifact, service = setup
    values = review(scope_label="East boundary", **geometry(artifact["id"]))
    for changes in ({"engineer_confirmed": False}, {"rationale": " "}):
        with pytest.raises(ValueError):
            service.create(tid, values | changes)
    saved = service.create(tid, values)
    item = boq(repo, tid)
    with pytest.raises(ValueError):
        service.link(tid, saved["id"], review(item_id=item["id"], engineer_confirmed=False))


def test_cancelled_pdf_lock_wait_creates_no_records(setup):
    _, tid, artifact, service = setup
    PDFIUM_LOCK.acquire()
    try:
        with pytest.raises(InterruptedError):
            service.calculate(tid, geometry(artifact["id"]), cancelled=lambda: True)
    finally:
        PDFIUM_LOCK.release()
    assert service.list(tid) == []


def test_routes_reject_authoritative_geometry_and_expose_typed_measurement_contract(setup):
    _, tid, artifact, service = setup
    app = FastAPI()
    app.include_router(
        importlib.import_module("quantix.measurement_routes").create_router(service.repo)
    )
    client = TestClient(app)
    path = f"/api/tenders/{tid}/measurements"
    bad = client.post(path + "/calculate", json=geometry(artifact["id"], page_size=[100, 100]))
    assert bad.status_code == 422
    result = client.post(path, json=review(scope_label="Boundary", **geometry(artifact["id"])))
    assert result.status_code == 200
    assert result.json()["quantity"] == "6"
    assert "MeasurementRecord" in app.openapi()["components"]["schemas"]


def test_link_is_idempotent_and_foreign_boq_item_cannot_receive_it(setup):
    repo, tid, artifact, service = setup
    item = boq(repo, tid)
    saved = save(service, tid, artifact["id"])
    first = service.link(tid, saved["id"], review(item_id=item["id"]))
    second = service.link(tid, saved["id"], review(item_id=item["id"]))
    assert first == second
    assert len(EstimateService(repo).view(tid)["items"][0]["quantity_proposals"]) == 1
    other = repo.create_tender("Other Tender")["id"]
    foreign = boq(repo, other)
    with pytest.raises(KeyError):
        service.link(tid, saved["id"], review(item_id=foreign["id"]))


def test_link_rolls_back_quantity_proposal_when_link_persistence_fails(setup):
    repo, tid, artifact, service = setup
    item = boq(repo, tid)
    saved = save(service, tid, artifact["id"])
    with repo.db.connect(write=True) as conn:
        conn.execute(
            "CREATE TRIGGER fail_measurement_link BEFORE INSERT ON measurement_links BEGIN SELECT RAISE(ABORT,'test storage fault'); END"
        )
    with pytest.raises(sqlite3.IntegrityError, match="storage fault"):
        service.link(tid, saved["id"], review(item_id=item["id"]))
    assert EstimateService(repo).view(tid)["items"][0]["quantity_proposals"] == []
    assert service.get(tid, saved["id"])["links"] == []


def test_pdf_revision_blocks_approval_of_an_already_linked_proposal(setup):
    repo, tid, artifact, service = setup
    item = boq(repo, tid)
    saved = save(service, tid, artifact["id"])
    proposal = service.link(tid, saved["id"], review(item_id=item["id"]))
    pdf_source(repo, tid, pdf_bytes(["Revised drawing"]))
    with pytest.raises(ValueError, match="changed"):
        EstimateService(repo).approve_quantity(tid, proposal["id"], review())
    assert EstimateService(repo).view(tid)["items"][0]["effective_quantity"] == "12"


def test_measurement_does_not_claim_source_extraction_or_increment_tender_revision(setup):
    repo, tid, artifact, service = setup
    revision = repo.get_tender(tid)["revision"]
    status = artifact["status"]
    save(service, tid, artifact["id"])
    assert repo.get_tender(tid)["revision"] == revision
    assert repo.get_artifact(tid, artifact["id"])["status"] == status


def test_non_pdf_sources_and_malformed_pdf_bytes_cannot_be_measured(setup):
    repo, tid, artifact, service = setup
    item = boq(repo, tid)
    with pytest.raises(ValueError, match="PDF"):
        service.calculate(tid, geometry(item["artifact_id"]))
    malformed = pdf_source(repo, tid, b"This is not a PDF")
    with pytest.raises(ValueError, match="PDF"):
        service.calculate(tid, geometry(malformed["id"]))


def test_unresolved_supplied_quantity_remains_unconfirmed_after_measurement_approval(setup):
    """Document existing EstimateService boundary: a measurement cannot confirm a broken source cell."""
    repo, tid, artifact, service = setup
    item = boq(repo, tid, broken=True)
    assert item["supplied_quantity"] is None
    saved = save(service, tid, artifact["id"])
    proposal = service.link(tid, saved["id"], review(item_id=item["id"]))
    estimates = EstimateService(repo)
    view = estimates.approve_quantity(tid, proposal["id"], review())
    assert view["items"][0]["effective_quantity"] == "6"
    assert view["items"][0]["confirmed"] is False
    assert view["complete"] is False


@pytest.mark.parametrize(
    "change", ["pdf_bytes", "missing_pdf", "quantity_cell", "unit", "proposal", "evidence"]
)
def test_approval_validation_rechecks_original_and_immutable_link_basis(setup, change):
    repo, tid, artifact, service = setup
    item = boq(repo, tid)
    saved = save(service, tid, artifact["id"])
    proposal = service.link(tid, saved["id"], review(item_id=item["id"]))
    assert hasattr(service, "validate_quantity_proposal"), (
        "Quantity approval needs measurement validation"
    )
    service.validate_quantity_proposal(tid, proposal["id"])
    if change == "pdf_bytes":
        repo.object_path(tid, artifact["id"]).write_bytes(b"Altered PDF bytes")
    elif change == "missing_pdf":
        repo.object_path(tid, artifact["id"]).unlink()
    else:
        with repo.db.connect(write=True) as conn:
            if change == "quantity_cell":
                conn.execute(
                    "UPDATE boq_items SET data_json=json_set(data_json,'$.quantity_cell','Z2') WHERE id=?",
                    (item["id"],),
                )
            elif change == "unit":
                conn.execute(
                    "UPDATE boq_items SET data_json=json_set(data_json,'$.unit','m2') WHERE id=?",
                    (item["id"],),
                )
            elif change == "proposal":
                conn.execute(
                    "UPDATE quantity_proposals SET data_json=json_set(data_json,'$.quantity','999') WHERE id=?",
                    (proposal["id"],),
                )
            else:
                conn.execute("UPDATE evidence SET page=99 WHERE id=?", (saved["source_id"],))
    with pytest.raises(ValueError, match="changed|unavailable|match|current|basis|missing"):
        service.validate_quantity_proposal(tid, proposal["id"])
    with pytest.raises(ValueError, match="changed|unavailable|match|current|basis|missing"):
        EstimateService(repo).approve_quantity(tid, proposal["id"], review())


def test_saved_record_remains_inspectable_with_changed_or_missing_source(setup):
    repo, tid, artifact, service = setup
    saved = save(service, tid, artifact["id"])
    source = repo.object_path(tid, artifact["id"])
    source.write_bytes(b"Altered PDF")
    changed = service.get(tid, saved["id"])
    assert changed["is_current"] is False
    assert changed["quantity"] == "6"
    assert "source_hash_mismatch" in changed["stale_reasons"]
    source.unlink()
    missing = service.list(tid)[0]
    assert missing["source_available"] is False
    assert "source_unavailable" in missing["stale_reasons"]


def test_previously_approved_quantity_is_flagged_when_preserved_drawing_changes(setup):
    repo, tid, artifact, service = setup
    item = boq(repo, tid)
    saved = save(service, tid, artifact["id"])
    proposal = service.link(tid, saved["id"], review(item_id=item["id"]))
    estimates = EstimateService(repo)
    estimates.approve_quantity(tid, proposal["id"], review())
    repo.object_path(tid, artifact["id"]).write_bytes(b"Altered after approval")
    current = estimates.view(tid)["items"][0]
    assert current["quantity_proposals"][0]["status"] == "needs_review"
    assert current["confirmed"] is False
    assert current["quantity_basis"] != "approved_measurement"


def test_drawing_modified_during_calculation_rolls_back_measurement_and_evidence(
    setup, monkeypatch
):
    repo, tid, artifact, service = setup
    module = importlib.import_module("quantix.measurements")
    original_calculation = module.calculate_measurement

    def altered(*args, **kwargs):
        result = original_calculation(*args, **kwargs)
        repo.object_path(tid, artifact["id"]).write_bytes(b"Replaced during calculation")
        return result

    monkeypatch.setattr(module, "calculate_measurement", altered)
    with pytest.raises(ValueError, match="changed|match|current"):
        save(service, tid, artifact["id"])
    assert service.list(tid) == []
    assert repo.artifact_evidence(tid, artifact["id"]) == []
