"""The calculator every member of the office works figures out with, so no sum is done in a model's head."""

import re
from decimal import Decimal

import pytest

from quantix.core import calculate

D = Decimal


@pytest.mark.parametrize(
    ("expression", "values", "expected"),
    [
        ("1 + 2 * 3", None, D(7)),
        ("(1 + 2) * 3", None, D(9)),
        ("10 / 4", None, D("2.5")),
        ("7 - 10", None, D(-3)),
        ("-2 ** 2", None, D(-4)),  # the power first, as on paper
        ("(-2) ** 2", None, D(4)),
        ("+4 - -1", None, D(5)),
        ("2 ** 10", None, D(1024)),
        ("2 ** -2", None, D("0.25")),
        ("2 ** 2.0", None, D(4)),  # a whole power written with a point
        ("0.1 + 0.2", None, D("0.3")),  # exactly, not 0.30000000000000004
        ("1.1 * 1.1", None, D("1.21")),
        ("6 × 7 ÷ 2", None, D(21)),  # the signs on a drawing or a calculator
        ("L * W * D", {"L": D(420), "W": D(12), "D": D("0.3")}, D(1512)),
        ("rate * (1 + wastage)", {"rate": 0.1, "wastage": 0.05}, D("0.105")),  # a float value as it is written
        ("qty * 0", {"qty": D(980)}, D(0)),
        ("min(3, 1, 2) + max(4, 9)", None, D(10)),
        ("max(-1)", None, D(-1)),
        ("abs(-2.5)", None, D("2.5")),
        ("sqrt(16) + sqrt(0)", None, D(4)),
        ("sqrt(2)", None, D("1.414213562373095048801688724")),
        ("1 / 3", None, D("0.3333333333333333333333333333")),
        ("3600000 * 0.003", None, D("10800.000")),  # a premium on the sum insured
        ("123456789012 * 1000", None, D("123456789012000")),
        ("1e308 * 10", None, D("1E+309")),  # past what a float can hold, still a number
    ],
)
def test_expressions_are_worked_out_exactly_in_decimals(expression, values, expected):
    assert calculate.evaluate(expression, values) == expected


@pytest.mark.parametrize(
    ("expression", "message"),
    [
        ("1 / 0", "That divides by zero."),
        ("1 / 0.0", "That divides by zero."),
        ("L / (W - W)", "That divides by zero."),
        ("2 ** 0.5", "Powers are whole numbers up to 10; use sqrt for a square root."),
        ("2 ** 11", "Powers are whole numbers up to 10"),
        ("2 ** -11", "Powers are whole numbers up to 10"),
        ("10 ** 10 ** 10", "Powers are whole numbers up to 10"),
        ("0 ** 0", "The numbers are out of range."),
        ("1e400 - 1e400", "The numbers are out of range."),
        ("X * 2", "X has no value: give it in values."),
        ("2 +", "That isn't an expression Quantix can work out."),
        ("L = 3", "That isn't an expression Quantix can work out."),
        ("7 // 2", "Use numbers, named values"),
        ("7 % 2", "Use numbers, named values"),
        ("1 << 2", "Use numbers, named values"),
        ("1 < 2", "Use numbers, named values"),
        ("True + 1", "Use numbers, named values"),
        ("1j", "Use numbers, named values"),
        ("'12' * 3", "Use numbers, named values"),
        ("round(2.5)", "Use numbers, named values"),
        ("sqrt(-1)", "Use numbers, named values"),
        ("sqrt(4, 9)", "Use numbers, named values"),
        ("abs(1, 2)", "Use numbers, named values"),
        ("min()", "Use numbers, named values"),
        ("max(a=1)", "Use numbers, named values"),
        ("L.real", "Use numbers, named values"),
        ("__import__('os').getcwd()", "Use numbers, named values"),
        ("[1, 2]", "Use numbers, named values"),
        ("(lambda: 1)()", "Use numbers, named values"),
    ],
)
def test_what_the_calculator_refuses_it_explains(expression, message):
    with pytest.raises(ValueError, match=re.escape(message)):
        calculate.evaluate(expression, {"L": D(3), "W": D(2)})


def test_a_long_calculation_is_broken_into_steps():
    assert calculate.evaluate("+".join(["1"] * 250)) == 250  # 499 characters
    with pytest.raises(ValueError, match="Break a long calculation into steps of up to 500 characters."):
        calculate.evaluate("+".join(["1"] * 251))


@pytest.mark.xfail(strict=True, reason="Bug: an infinite result comes back as a figure instead of an error")
@pytest.mark.parametrize(
    ("expression", "message"),
    [("1e400", "The numbers are out of range."), ("0 ** -1", "That divides by zero.")],
)
def test_a_result_that_is_not_a_number_is_refused(expression, message):
    with pytest.raises(ValueError, match=message):
        calculate.evaluate(expression)


@pytest.mark.xfail(strict=True, reason="Bug: decimal.Overflow escapes evaluate, which catches only OverflowError")
def test_a_result_beyond_any_number_is_out_of_range():
    with pytest.raises(ValueError, match="The numbers are out of range."):
        calculate.evaluate("((((((10 ** 10) ** 10) ** 10) ** 10) ** 10) ** 10)")


@pytest.mark.parametrize(
    ("value", "written"),
    [
        (D("1512"), "1,512"),
        (D("1512.000"), "1,512"),
        (D("0.3"), "0.3"),
        (D("1234567.891"), "1,234,567.891"),
        (D("2.5"), "2.5"),
        (D("0.33333"), "0.3333"),
        (D("0.66666"), "0.6667"),
        (D("0.9999999999999999999999999999"), "1"),
        (D("-1234.5"), "-1,234.5"),
        (D("0"), "0"),
        (D("100"), "100"),
    ],
)
def test_a_result_is_written_to_four_places_without_trailing_zeros(value, written):
    assert calculate.plain(value) == written


@pytest.mark.xfail(strict=True, reason="Bug: plain() quantizes past the 28-digit context, raising InvalidOperation")
def test_a_very_large_result_is_still_written_out():
    assert calculate.plain(calculate.evaluate("10 ** 10 * 10 ** 10 * 10 ** 5")) == "10,000,000,000,000,000,000,000,000"


def grid(*rows) -> list[list[Decimal]]:
    return [[D(str(v)) for v in row] for row in rows]


def test_a_square_all_in_cut_or_all_in_fill_is_its_area_times_its_mean_depth():
    level = grid((101, 101.5), (100.5, 101))
    cut, fill, squares = calculate.grid_volumes(D(10), level, grid((100, 100), (100, 100)))
    assert (cut, fill, squares) == (D(100), D(0), 1)  # 100 m2 × (1 + 1.5 + 0.5 + 1) / 4
    cut, fill, squares = calculate.grid_volumes(D(10), grid((99, 99), (98, 99)), grid((100, 100), (100, 100)))
    assert (cut, fill, squares) == (D(0), D(125), 1)  # 100 m2 × (1 + 1 + 2 + 1) / 4
    level = grid((101, 100), (100, 100))  # corners on the formation count as neither
    assert calculate.grid_volumes(D(10), level, grid((100, 100), (100, 100))) == (D(25), D(0), 1)
    flat = grid((100, 100), (100, 100))
    assert calculate.grid_volumes(D(10), flat, flat) == (D(0), D(0), 1)


def test_a_square_crossing_the_formation_splits_its_cut_and_fill():
    level = grid((101, 101, 100), (101, 99, 99))
    formation = grid((100, 100, 100), (100, 100, 100))
    cut, fill, squares = calculate.grid_volumes(D(6), level, formation)
    # first square: depths 1, 1, 1, -1, so cut 36 × 3² / (4 × 4) = 20.25 and fill 36 × 1² / 16 = 2.25
    # second square: depths 1, 0, -1, -1, so cut 36 × 1² / (4 × 3) = 3 and fill 36 × 2² / 12 = 12
    assert (cut, fill, squares) == (D("23.25"), D("14.25"), 2)


def test_the_formation_can_vary_across_the_grid():
    level = grid((101, 101.5, 102), (101, 101.5, 102), (101, 101.5, 102))
    formation = grid((100, 100.5, 101), (100, 100.5, 101), (100, 100.5, 101))  # a slope, 1 m below the ground
    assert calculate.grid_volumes(D("2.5"), level, formation) == (D(25), D(0), 4)  # 4 squares of 6.25 m2 × 1 m


@pytest.mark.parametrize(
    ("spacing", "level", "formation", "message"),
    [
        (D(10), grid((100, 100)), grid((100, 100)), "at least 2 by 2"),
        (D(10), grid((100,), (100,)), grid((100,), (100,)), "at least 2 by 2"),
        (D(10), [], [], "at least 2 by 2"),
        (D(10), grid((100, 100), (100,)), grid((100, 100), (100, 100)), "rows of the same length"),
        (D(10), grid((100, 100), (100, 100)), grid((100, 100)), "formation levels on the same grid"),
        (D(10), grid((100, 100), (100, 100)), grid((100, 100), (100, 100, 100)), "formation levels on the same grid"),
        (D(0), grid((100, 100), (100, 100)), grid((100, 100), (100, 100)), "spacing must be more than zero"),
        (D(-5), grid((100, 100), (100, 100)), grid((100, 100), (100, 100)), "spacing must be more than zero"),
    ],
)
def test_a_grid_that_does_not_fit_is_refused(spacing, level, formation, message):
    with pytest.raises(ValueError, match=message):
        calculate.grid_volumes(spacing, level, formation)
