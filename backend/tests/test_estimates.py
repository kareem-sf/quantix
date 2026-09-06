import importlib

import pytest

from quantix.repository import Repository


def cell(address, value, formula=None, cached=None):
    return {
        "coordinate": address,
        "value": value,
        "formula": formula,
        "cached_value": cached,
        "data_type": "f" if formula else "n" if isinstance(value, (int, float)) else "s",
        "number_format": "General",
    }


def seed(repo, tender_id, swapped=False, broken=False, digest="a" * 64):
    header = [
        cell("A1", "Item"),
        cell("B1", "Description"),
        cell("C1", "Quantity" if swapped else "Unit"),
        cell("D1", "Unit" if swapped else "Quantity"),
        cell("E1", "Rate"),
        cell("F1", "Amount"),
    ]
    row = [
        cell("A2", "C01"),
        cell("B2", "Cast concrete foundations"),
        cell("C2", "m3"),
        cell("D2", "=#REF!", "=#REF!") if broken else cell("D2", 12.5),
    ]
    artifact, _ = repo.register_artifact(
        tender_id,
        "Area/Schedule.xlsx",
        digest,
        100,
        {
            "kind": "spreadsheet",
            "status": "extracted",
            "metadata": {},
            "segments": [
                {
                    "locator": "sheet:Works/row:1",
                    "text": "Header",
                    "sheet": "Works",
                    "kind": "spreadsheet_row",
                    "metadata": {"row": 1, "cells": header},
                },
                {
                    "locator": "sheet:Works/row:2",
                    "text": "Concrete row",
                    "sheet": "Works",
                    "kind": "spreadsheet_row",
                    "metadata": {"row": 2, "cells": row},
                },
            ],
        },
    )
    return artifact


@pytest.fixture
def setup(tmp_path):
    repo = Repository(tmp_path)
    tid = repo.create_tender("School foundations")["id"]
    seed(repo, tid)
    service = importlib.import_module("quantix.estimates").EstimateService(repo)
    return repo, tid, service


def approval(**values):
    return {
        "engineer_confirmed": True,
        "rationale": "Checked against the supplied source.",
        **values,
    }


def rate(**values):
    return approval(
        unit_rate="10.10",
        currency="EGP",
        tax_basis="excluding_vat",
        confirm_source=True,
        provenance={
            "basis": "estimated",
            "observed_on": "2026-09-06",
            "source_ids": [],
            "urls": [],
            "geography": "Cairo",
            "conditions": "Engineer estimate",
        },
        **values,
    )


def test_refresh_infers_actual_values_with_reversed_headers_and_preserves_source(tmp_path):
    repo = Repository(tmp_path)
    tid = repo.create_tender("Project")["id"]
    artifact = seed(repo, tid, swapped=True)
    service = importlib.import_module("quantix.estimates").EstimateService(repo)
    view = service.refresh(tid)
    assert len(view["items"]) == 1
    item = view["items"][0]
    assert item["supplied_quantity"] == "12.5"
    assert item["unit"] == "m3"
    assert item["quantity_cell"] == "D2"
    assert item["artifact_id"] == artifact["id"]
    assert any("header" in issue.lower() for issue in item["issues"])
    assert item["confirmed"] is False
    assert view["complete"] is False
    assert repo.get_evidence(tid, item["source_id"])["metadata"]["cells"][3]["value"] == 12.5


def test_decimal_rate_build_up_and_explicit_vat(setup):
    repo, tid, service = setup
    item = service.refresh(tid)["items"][0]
    request = rate(vat_percent="14")
    request.pop("unit_rate")
    request["components"] = [
        {"name": "Material", "quantity": "0.1", "unit_rate": "0.2", "unit": "kg"},
        {"name": "Labour", "quantity": "1", "unit_rate": "10.08", "unit": "hour"},
    ]
    service.update_item(tid, item["id"], request)
    view = service.view(tid)
    assert view["items"][0]["unit_rate"] == "10.10"
    assert view["items"][0]["line_ex_vat"] == "126.25"
    assert view["items"][0]["line_inc_vat"] == "143.93"
    assert view["totals"][0]["total_inc_vat"] == "143.93"
    assert view["complete"] is True


def test_unknown_vat_does_not_become_zero_or_complete(setup):
    repo, tid, service = setup
    item = service.refresh(tid)["items"][0]
    service.update_item(tid, item["id"], rate())
    view = service.view(tid)
    assert view["items"][0]["line_ex_vat"] == "126.25"
    assert view["items"][0]["line_inc_vat"] is None
    assert view["complete"] is False
    assert view["unknown_vat_count"] == 1


def test_measured_quantity_stays_separate_until_explicit_approval(setup):
    repo, tid, service = setup
    item = service.refresh(tid)["items"][0]
    proposal = service.propose_quantity(
        tid,
        item["id"],
        approval(quantity="15", calculation="5 m Ã— 3 m Ã— 1 m", source_ids=[item["source_id"]]),
    )
    assert service.view(tid)["items"][0]["effective_quantity"] == "12.5"
    assert proposal["status"] == "proposed"
    with pytest.raises(ValueError):
        service.approve_quantity(
            tid, proposal["id"], {"engineer_confirmed": False, "rationale": "AI said yes"}
        )
    service.approve_quantity(tid, proposal["id"], approval())
    revised = service.view(tid)["items"][0]
    assert revised["supplied_quantity"] == "12.5"
    assert revised["effective_quantity"] == "15"
    assert revised["quantity_basis"] == "approved_measurement"


def test_broken_formula_quantity_stays_unknown(tmp_path):
    repo = Repository(tmp_path)
    tid = repo.create_tender("Project")["id"]
    seed(repo, tid, broken=True)
    service = importlib.import_module("quantix.estimates").EstimateService(repo)
    item = service.refresh(tid)["items"][0]
    assert item["supplied_quantity"] is None
    assert any("formula" in issue.lower() for issue in item["issues"])


def test_revision_does_not_reuse_old_quantities_rates_or_approval(setup):
    repo, tid, service = setup
    old = service.refresh(tid)["items"][0]
    service.update_item(tid, old["id"], rate(vat_percent="14"))
    seed(repo, tid, digest="b" * 64)
    assert service.view(tid)["refresh_required"] is True
    new = service.refresh(tid)["items"][0]
    assert new["id"] != old["id"]
    assert new["unit_rate"] is None
    assert new["confirmed"] is False
    with pytest.raises((KeyError, ValueError)):
        service.update_item(tid, old["id"], rate())


def test_invalid_price_inputs_or_cross_tender_sources_cannot_mutate(setup):
    repo, tid, service = setup
    item = service.refresh(tid)["items"][0]
    for bad in (
        {"engineer_confirmed": False},
        {"unit_rate": "NaN"},
        {"vat_percent": "-1"},
        {"rationale": " "},
    ):
        with pytest.raises(ValueError):
            service.update_item(tid, item["id"], rate() | bad)
    with pytest.raises((KeyError, ValueError)):
        service.propose_quantity(
            tid, item["id"], approval(quantity="20", calculation="Measured", source_ids=["fake"])
        )
    assert service.view(tid)["items"][0]["unit_rate"] is None


def test_ambiguous_numeric_columns_require_source_cell_selection(tmp_path):
    repo = Repository(tmp_path)
    tid = repo.create_tender("Project")["id"]
    repo.register_artifact(
        tid,
        "Unlabelled.xlsx",
        "c" * 64,
        100,
        {
            "kind": "spreadsheet",
            "status": "extracted",
            "segments": [
                {
                    "locator": "sheet:Scope/row:5",
                    "text": "Unlabelled row",
                    "sheet": "Scope",
                    "kind": "spreadsheet_row",
                    "metadata": {
                        "cells": [
                            cell("B5", "Concrete works"),
                            cell("F5", "m3"),
                            cell("G5", 12),
                            cell("J5", 120),
                        ]
                    },
                }
            ],
        },
    )
    service = importlib.import_module("quantix.estimates").EstimateService(repo)
    item = service.refresh(tid)["items"][0]
    assert item["supplied_quantity"] is None
    with pytest.raises(ValueError):
        service.update_item(tid, item["id"], approval(confirm_source=True))
    selected = service.update_item(
        tid, item["id"], approval(confirm_source=True, quantity_cell="G5")
    )
    assert selected["supplied_quantity"] == "12"


def test_included_vat_is_removed_only_with_established_percentage(setup):
    repo, tid, service = setup
    item = service.refresh(tid)["items"][0]
    service.update_item(
        tid,
        item["id"],
        rate(vat_percent="14") | {"unit_rate": "11.40", "tax_basis": "including_vat"},
    )
    priced = service.view(tid)["items"][0]
    assert priced["line_ex_vat"] == "125.00"
    assert priced["line_inc_vat"] == "142.50"


def test_revised_measurement_evidence_cannot_keep_approved_quantity_active(setup):
    repo, tid, service = setup
    item = service.refresh(tid)["items"][0]
    drawing = {
        "kind": "pdf",
        "status": "extracted",
        "segments": [{"locator": "page:1", "text": "Footing 5 by 3"}],
    }
    artifact, _ = repo.register_artifact(tid, "Footings.pdf", "d" * 64, 100, drawing)
    source_id = repo.artifact_evidence(tid, artifact["id"])[0]["id"]
    proposal = service.propose_quantity(
        tid, item["id"], approval(quantity="15", calculation="5 x 3 x 1", source_ids=[source_id])
    )
    service.approve_quantity(tid, proposal["id"], approval())
    repo.register_artifact(tid, "Footings.pdf", "e" * 64, 100, drawing)
    revised = service.refresh(tid)["items"][0]
    assert revised["effective_quantity"] == "12.5"
    assert revised["quantity_proposals"][0]["status"] == "needs_review"


def test_null_tax_basis_clears_to_unknown_instead_of_breaking_response_schema(setup):
    from quantix.estimate_models import EstimateItem

    repo, tid, service = setup
    item = service.refresh(tid)["items"][0]
    result = service.update_item(tid, item["id"], approval(tax_basis=None))
    assert result["tax_basis"] == "unknown"
    EstimateItem.model_validate(result)


def test_component_arithmetic_preserves_all_accepted_decimal_digits(setup):
    repo, tid, service = setup
    item = service.refresh(tid)["items"][0]
    request = rate(vat_percent="0")
    request.pop("unit_rate")
    request["components"] = [
        {
            "name": "Large allowance",
            "quantity": "999999999999.999999",
            "unit_rate": "999999999999.999999",
            "unit": "lot",
        }
    ]
    result = service.update_item(tid, item["id"], request)
    assert result["unit_rate"] == "999999999999999998000000.000000000001"


def test_broken_boq_quantity_can_use_explicitly_approved_measurement_for_pricing(tmp_path):
    repo = Repository(tmp_path)
    tid = repo.create_tender("Measured foundation")["id"]
    seed(repo, tid, broken=True)
    service = importlib.import_module("quantix.estimates").EstimateService(repo)
    item = service.refresh(tid)["items"][0]
    with pytest.raises(ValueError, match="quantity"):
        service.update_item(tid, item["id"], approval(confirm_source=True))
    proposal = service.propose_quantity(
        tid,
        item["id"],
        approval(
            quantity="6",
            calculation="Reviewed foundation volume: 3 x 2 x 1 metres",
            source_ids=[item["source_id"]],
        ),
    )
    with pytest.raises(ValueError, match="quantity"):
        service.update_item(tid, item["id"], approval(confirm_source=True))
    assert (
        service.approve_quantity(tid, proposal["id"], approval())["items"][0]["confirmed"] is False
    )
    priced = service.update_item(tid, item["id"], rate(vat_percent="14"))
    assert priced["supplied_quantity"] is None
    assert priced["effective_quantity"] == "6"
    assert priced["quantity_basis"] == "approved_measurement"
    assert priced["confirmed"] is True
    assert priced["issues"]
    assert priced["line_ex_vat"] == "60.60"
    assert service.view(tid)["complete"] is True
