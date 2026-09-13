"""Truthful review tests: prose is not an engineering calculation."""

from __future__ import annotations

from quantix.calculation_models import CalculationRequest
from quantix.calculations import CalculationService
from quantix.execution_context import OfficeExecutionIdentity
from quantix.office_reviews import OfficeReviewService, ReviewDraft
from quantix.repository import Repository


def _identity(tender_id: str) -> OfficeExecutionIdentity:
    return OfficeExecutionIdentity(
        tender_id=tender_id,
        actor_kind="manager",
        actor_id="manager",
        root_run_id=None,
        budget_scope_id=None,
        assignment_id=None,
        profile_version=None,
        route_binding_id=None,
        instruction_revision_id=None,
        grant_fingerprint=None,
        ownership_epoch=None,
        trusted_invocation_id=None,
    )


def test_unsupported_review_prose_creates_no_finding_or_acceptance(tmp_path):
    repo = Repository(tmp_path)
    tender = repo.create_tender("Review truth Tender")
    review = OfficeReviewService(repo).start(
        _identity(tender["id"]),
        ReviewDraft(
            subject_id="estimate-1",
            workings="Area 100 m2. Deduction rule unclear. Duplicated allowance totals 110.00.",
            hide_author_conclusion=True,
            idempotency_key="unsupported-prose",
        ),
        author_total="110.00",
    )

    assert review.findings == []
    assert review.status == "incomplete"
    assert review.limitations
    assert "prose" in review.limitations[0].lower()
    assert repo.list_findings(tender["id"]) == []


def test_recorded_calculation_is_recomputed_from_typed_inputs(tmp_path):
    repo = Repository(tmp_path)
    tender = repo.create_tender("Calculation review Tender")
    identity = _identity(tender["id"])
    calculation = CalculationService(repo).calculate(
        identity,
        CalculationRequest(
            method_id="sum",
            method_version="1",
            inputs={"values": ["100", "25"]},
            units={"quantity": "m2"},
            precision="0.01",
            rounding="HALF_UP",
            idempotency_key="sum-125",
        ),
    )

    review = OfficeReviewService(repo).start(
        identity,
        ReviewDraft(
            subject_id="estimate-1",
            workings="The wording may describe a quantity, but it is not the calculation.",
            calculation_id=calculation.id,
            hide_author_conclusion=True,
            idempotency_key="recorded-calculation",
        ),
    )

    assert review.status == "checked"
    assert len(review.findings) == 1
    assert review.findings[0].topic == "calculation"
    assert review.findings[0].agreed is True
    assert "125.00" in review.findings[0].detail
    assert repo.list_findings(tender["id"]) == []


def test_product_review_uses_recorded_dimensions_and_lineage(tmp_path):
    repo = Repository(tmp_path)
    tender = repo.create_tender("Dimensional review Tender")
    identity = _identity(tender["id"])
    calculation = CalculationService(repo).calculate(
        identity,
        CalculationRequest(
            method_id="product",
            method_version="v2",
            inputs={"quantity": "2", "factor": "3"},
            units={"quantity": "m", "factor": "m"},
            precision="0.01",
            rounding="HALF_UP",
            idempotency_key="area-product",
        ),
    )

    review = OfficeReviewService(repo).start(
        identity,
        ReviewDraft(
            subject_id="unrelated-subject",
            workings="Check the recorded dimensional calculation.",
            calculation_id=calculation.id,
            hide_author_conclusion=True,
            idempotency_key="area-review",
        ),
    )

    assert review.status == "checked"
    assert review.findings[0].agreed is True
    with repo.db.connect() as conn:
        saved = conn.execute(
            "SELECT calculation_id,calculation_method_id,calculation_method_version,checked_basis_fingerprint,subject_id "
            "FROM office_review_sessions WHERE id=?",
            (review.id,),
        ).fetchone()
    assert saved["calculation_id"] == calculation.id
    assert saved["calculation_method_id"] == "product"
    assert saved["calculation_method_version"] == "v2"
    assert saved["checked_basis_fingerprint"] == calculation.basis_fingerprint
    assert saved["subject_id"] == "unrelated-subject"
    assert review.calculation_id == calculation.id
    assert review.calculation_method_id == "product"
    assert review.calculation_method_version == "v2"
    assert review.checked_basis_fingerprint == calculation.basis_fingerprint


def test_emission_factor_product_review_uses_composed_unit(tmp_path):
    repo = Repository(tmp_path)
    tender = repo.create_tender("Emission review Tender")
    identity = _identity(tender["id"])
    calculation = CalculationService(repo).calculate(
        identity,
        CalculationRequest(
            method_id="emission_factor",
            method_version="v1",
            inputs={"quantity": "2", "factor": "3"},
            units={"quantity": "tonne", "factor": "tCO2e/tonne"},
            precision="0.01",
            rounding="HALF_UP",
            idempotency_key="emission-product",
        ),
    )

    review = OfficeReviewService(repo).start(
        identity,
        ReviewDraft(
            subject_id="estimate-emissions",
            workings="Check the recorded emissions arithmetic.",
            calculation_id=calculation.id,
            idempotency_key="emission-review",
        ),
    )

    assert review.status == "checked"
    assert review.findings[0].agreed is True


def test_calculation_from_another_tender_is_not_reviewable(tmp_path):
    repo = Repository(tmp_path)
    first = repo.create_tender("First Tender")
    second = repo.create_tender("Second Tender")
    calculation = CalculationService(repo).calculate(
        _identity(first["id"]),
        CalculationRequest(
            method_id="difference",
            method_version="1",
            inputs={"left": "10", "right": "4"},
            units={"left": "m", "right": "m"},
            precision="0.01",
            rounding="HALF_UP",
            idempotency_key="difference-6",
        ),
    )

    try:
        OfficeReviewService(repo).start(
            _identity(second["id"]),
            ReviewDraft(
                subject_id="estimate-2",
                workings="Check the recorded arithmetic.",
                calculation_id=calculation.id,
                hide_author_conclusion=True,
                idempotency_key="cross-tender-calculation",
            ),
        )
    except ValueError as error:
        assert "selected Tender" in str(error)
    else:
        raise AssertionError("A calculation from another Tender must be refused.")


def test_changed_recorded_output_requires_review_without_accepting_it(tmp_path):
    repo = Repository(tmp_path)
    tender = repo.create_tender("Stale calculation Tender")
    identity = _identity(tender["id"])
    calculation = CalculationService(repo).calculate(
        identity,
        CalculationRequest(
            method_id="sum",
            method_version="1",
            inputs={"values": ["100", "25"]},
            units={"quantity": "m2"},
            precision="0.01",
            rounding="HALF_UP",
            idempotency_key="stale-sum",
        ),
    )
    with repo.atomic() as conn:
        conn.execute(
            "UPDATE office_calculations SET outputs_json=? WHERE id=?",
            ('{"sum":"999.00","unit":"m2"}', calculation.id),
        )

    review = OfficeReviewService(repo).start(
        identity,
        ReviewDraft(
            subject_id="estimate-stale",
            workings="A saved total changed after calculation.",
            calculation_id=calculation.id,
            hide_author_conclusion=True,
            idempotency_key="stale-review",
        ),
    )

    assert review.status == "needs_review"
    assert len(review.findings) == 1
    assert review.findings[0].agreed is False
