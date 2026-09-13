from decimal import Decimal

import pytest

from quantix.calculation_models import CalculationCheckRequest, CalculationRequest
from quantix.calculations import CalculationService
from quantix.execution_context import engineer_identity
from quantix.repository import Repository
from quantix.units import add, multiply, quantity


def test_mass_addition_converts_scales_before_adding():
    result = add(quantity("500", "kg"), quantity("0.5", "tonne"))
    assert result.value == Decimal("1000") and result.unit == "kg"


def test_emission_factor_respects_per_tonne_basis():
    result = multiply(quantity("1000", "kg"), quantity("0.5", "tCO2e/tonne"))
    assert result.value == Decimal("0.5") and result.unit == "tCO2e"


def test_geometry_converts_centimetres_before_multiplication():
    result = multiply(quantity("2", "m"), quantity("30", "cm"))
    assert result.value == Decimal("0.6") and result.unit == "m2"


def test_explicit_imperial_conversion_is_decimal():
    from quantix.units import convert

    assert convert(quantity("3", "ft"), "m").value == Decimal("0.9144")


def test_adding_units_returns_output_and_invalid_math_cannot_pass_review(tmp_path):
    repo = Repository(tmp_path)
    tender = repo.create_tender("Synthetic unit checks")
    service = CalculationService(repo)
    ctx = engineer_identity(tender["id"])
    valid = service.calculate(
        ctx,
        CalculationRequest(
            method_id="add_units",
            method_version="1",
            inputs={"left": "3", "right": "2"},
            units={"left": "m", "right": "m"},
            idempotency_key="add",
        ),
    )
    assert valid.outputs == {"sum": "5.00", "unit": "m"}
    invalid = service.calculate(
        ctx,
        CalculationRequest(
            method_id="add_units",
            method_version="1",
            inputs={"left": "3", "right": "2"},
            units={"left": "m", "right": "kg"},
            idempotency_key="invalid",
        ),
    )
    checked = service.check(
        ctx,
        CalculationCheckRequest(
            calculation_id=invalid.id, method_id="add_units", method_version="1"
        ),
    )
    assert checked["reproducible"] is False


@pytest.mark.parametrize("value", ["NaN", "Infinity", "1e999999999"])
def test_unbounded_or_nonfinite_inputs_are_rejected(value):
    with pytest.raises(ValueError):
        quantity(value, "m")


def test_unsupported_rounding_and_precision_are_rejected():
    import pytest
    from pydantic import ValidationError

    from quantix.calculation_models import CalculationRequest

    values = dict(
        method_id="product",
        method_version="1",
        inputs={"quantity": "2.5", "factor": "1"},
        units={},
        idempotency_key="rounding",
    )
    with pytest.raises(ValidationError):
        CalculationRequest(**values, rounding="FLOOR")
    with pytest.raises(ValidationError):
        CalculationRequest(**values, precision="NaN")


def test_large_values_cancel_without_losing_the_decimal_remainder(tmp_path):
    repo = Repository(tmp_path)
    tender = repo.create_tender("Synthetic decimal precision")
    service = CalculationService(repo)
    result = service.calculate(
        engineer_identity(tender["id"]),
        CalculationRequest(
            method_id="sum",
            method_version="1",
            inputs={
                "values": ["10000000000000000000000000000.01", "-10000000000000000000000000000"]
            },
            units={"quantity": "1"},
            precision="0.01",
            idempotency_key="cancellation",
        ),
    )
    assert result.outputs == {"sum": "0.01", "unit": "1"}


def test_large_conversion_preserves_small_fraction():
    from quantix.units import convert

    result = convert(quantity("10000000000000000000000000.00001", "tonne"), "kg")
    assert result.value == Decimal("10000000000000000000000000000.01")


@pytest.mark.parametrize("unit", ["m ** (2 ** 3)", "m ** 0.5"])
def test_unit_input_cannot_evaluate_nested_or_fractional_expressions(unit):
    with pytest.raises(ValueError, match="unit"):
        quantity("1", unit)
