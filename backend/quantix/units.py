"""Decimal engineering units using Pint's maintained conversion definitions."""

from __future__ import annotations

import re
from contextlib import contextmanager
from dataclasses import dataclass
from decimal import Decimal, localcontext
from fractions import Fraction
from functools import lru_cache

from pint import UnitRegistry
from pint.errors import PintError

# Accepted inputs have at most 256 characters and adjusted exponents within
# +/-100. This covers their aligned sums, products and unit-scale conversions
# without relying on the process/thread's ambient Decimal precision.
ARITHMETIC_PRECISION = 1024
_NAMED_UNIT = r"(?:[A-Za-zµμ°][A-Za-z0-9_µμ°]{0,63}|1)"
_UNIT_FACTOR = _NAMED_UNIT + r"(?:\s*(?:\*\*|\^)\s*[+-]?\d{1,2})?"
_UNIT_EXPRESSION = re.compile(_UNIT_FACTOR + r"(?:\s*[*/]\s*" + _UNIT_FACTOR + r")*")


@contextmanager
def arithmetic_precision():
    with localcontext() as context:
        context.prec = ARITHMETIC_PRECISION
        yield


@lru_cache(maxsize=1)
def _registry():
    # Unit definitions can contain exact ratios such as yard / 3. Parsing
    # those as Decimal caches a rounded scale before any calculation begins.
    registry = UnitRegistry(non_int_type=Fraction)
    registry.define("kgCO2e = [emissions]")
    registry.define("tCO2e = 1000 * kgCO2e")
    registry.define("each = [] = ea = item")
    # Separate currency dimensions never imply an exchange rate.
    for currency in (
        "USD",
        "EGP",
        "EUR",
        "GBP",
        "AED",
        "SAR",
        "QAR",
        "KWD",
        "BHD",
        "OMR",
        "CAD",
        "AUD",
    ):
        registry.define(f"{currency} = [currency_{currency}]")
    return registry


def _unit(unit: str):
    if not isinstance(unit, str) or not unit.strip() or len(unit) > 128:
        raise DimensionError("Choose a bounded, named engineering unit.")
    if not _UNIT_EXPRESSION.fullmatch(unit.strip()):
        raise DimensionError(
            "Use named units joined with * or / and integer powers from -12 to 12."
        )
    if any(abs(int(power)) > 12 for power in re.findall(r"(?:\*\*|\^)\s*([+-]?\d+)", unit)):
        raise DimensionError("The unit exponent is outside the engineering range.")
    aliases = {"m2": "(meter ** 2)", "m3": "(meter ** 3)", "lm": "meter"}
    normalized = re.sub(r"\b(?:m2|m3|lm)\b", lambda match: aliases[match[0]], unit.strip())
    try:
        return _registry().parse_units(normalized)
    except (PintError, ValueError, TypeError, SyntaxError) as error:
        raise DimensionError(f"The unit '{unit}' is unknown or cannot be used here.") from error


@dataclass(frozen=True)
class Quantity:
    value: Decimal
    unit: str
    dimension: tuple[tuple[str, int], ...]


class DimensionError(ValueError):
    """Raised when two quantities cannot be combined."""


def parse_unit(unit: str) -> tuple[tuple[str, int], ...]:
    return tuple(sorted((name, int(power)) for name, power in _unit(unit).dimensionality.items()))


def quantity(value, unit: str) -> Quantity:
    text = str(value)
    if len(text) > 256:
        raise DimensionError("The numerical input is too large.")
    try:
        number = Decimal(text)
    except ArithmeticError as error:
        raise DimensionError("Use a finite decimal numerical value.") from error
    if not number.is_finite() or (number and abs(number.adjusted()) > 100):
        raise DimensionError("Use a finite value within the engineering numerical range.")
    return Quantity(value=number, unit=unit, dimension=parse_unit(unit))


def _computed(value, unit: str) -> Quantity:
    if isinstance(value, Fraction):
        value = Decimal(value.numerator) / Decimal(value.denominator)
    else:
        value = Decimal(value)
    if not value.is_finite() or (value and abs(value.adjusted()) > 100):
        raise DimensionError("The calculated value is outside the engineering numerical range.")
    return Quantity(value=value, unit=unit, dimension=parse_unit(unit))


@arithmetic_precision()
def convert(value: Quantity, target_unit: str) -> Quantity:
    try:
        converted = (
            _registry().Quantity(Fraction(value.value), _unit(value.unit)).to(_unit(target_unit))
        )
        return _computed(converted.magnitude, target_unit)
    except (PintError, ArithmeticError) as error:
        raise DimensionError(
            "These units cannot be converted without another physical basis or exchange rate."
        ) from error


@arithmetic_precision()
def add(left: Quantity, right: Quantity) -> Quantity:
    if left.dimension != right.dimension:
        raise DimensionError("These units cannot be added.")
    right = convert(right, left.unit)
    return _computed(left.value + right.value, left.unit)


@arithmetic_precision()
def multiply(left: Quantity, right: Quantity) -> Quantity:
    try:
        result = _registry().Quantity(
            Fraction(left.value), _unit(left.unit)
        ) * _registry().Quantity(Fraction(right.value), _unit(right.unit))
        target = left.unit if not right.dimension else right.unit if not left.dimension else None
        if target is None:
            target = next(
                (
                    unit
                    for unit in ("m", "m2", "m3", "kg", "tCO2e", "1")
                    if _unit(unit).dimensionality == result.dimensionality
                ),
                None,
            )
        if target is None:
            result = result.to_base_units()
            target = format(result.units, "~")
        else:
            result = result.to(_unit(target))
        return _computed(result.magnitude, target)
    except (PintError, ArithmeticError) as error:
        raise DimensionError("These units cannot be multiplied in this calculation.") from error


@arithmetic_precision()
def subtract(left: Quantity, right: Quantity) -> Quantity:
    if left.dimension != right.dimension:
        raise DimensionError("These units cannot be subtracted.")
    right = convert(right, left.unit)
    return _computed(left.value - right.value, left.unit)
