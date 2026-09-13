import pytest

from quantix.measurements import calculate_measurement


def test_length_uses_actual_page_aspect_and_printed_calibration():
    result = calculate_measurement(
        "length",
        [1000, 500],
        [[0, 0], [0, 0.6]],
        calibration_points=[[0, 0], [0.2, 0]],
        calibration_metres="4",
    )
    assert result["quantity"] == "6"
    assert result["unit"] == "m"
    assert result["calibration_metres"] == "4"


def test_polygon_area_is_independent_of_vertex_order_and_uses_squared_scale():
    points = [[0.1, 0.1], [0.3, 0.1], [0.3, 0.5], [0.1, 0.5]]
    values = dict(calibration_points=[[0, 0], [0.2, 0]], calibration_metres="4")
    result = calculate_measurement("area", [1000, 500], points, **values)
    reversed_result = calculate_measurement("area", [1000, 500], points[::-1], **values)
    assert result["quantity"] == reversed_result["quantity"] == "16"
    assert result["unit"] == "m2"


def test_count_requires_distinct_located_marks_and_no_scale():
    result = calculate_measurement("count", [1000, 500], [[0.1, 0.2], [0.2, 0.2]])
    assert result["quantity"] == "2"
    assert result["unit"] == "nr"
    assert result["calibration_metres"] is None


@pytest.mark.parametrize(
    "points",
    [
        [[0, 0], [1, 1], [1, 0], [0, 1]],
        [[0, 0], [0.5, 0], [1, 0]],
        [[0, 0], [0.5, 0], [0.5, 0.5], [0.5, 0]],
    ],
)
def test_invalid_polygons_are_never_saved_as_plausible_areas(points):
    with pytest.raises(ValueError):
        calculate_measurement(
            "area",
            [1000, 500],
            points,
            calibration_points=[[0, 0], [0.2, 0]],
            calibration_metres="4",
        )


@pytest.mark.parametrize(
    "points", [[[0, 0], [float("nan"), 1]], [[0, 0], [1.1, 0]], [[0, 0], [0, 0]]]
)
def test_invalid_coordinates_are_rejected(points):
    with pytest.raises(ValueError):
        calculate_measurement(
            "length",
            [1000, 500],
            points,
            calibration_points=[[0, 0], [0.2, 0]],
            calibration_metres="4",
        )


@pytest.mark.parametrize(
    "calibration,metres",
    [([[0, 0], [0, 0]], "4"), ([[0, 0], [1, 0]], "0"), ([[0, 0], [1, 0]], "NaN")],
)
def test_missing_or_degenerate_calibration_is_rejected(calibration, metres):
    with pytest.raises(ValueError):
        calculate_measurement(
            "length",
            [1000, 500],
            [[0, 0], [0.5, 0]],
            calibration_points=calibration,
            calibration_metres=metres,
        )


def test_diagonal_polyline_and_fractional_lengths_keep_precision():
    result = calculate_measurement(
        "length",
        [1000, 1000],
        [[0, 0], [0.3, 0.4], [0.6, 0.4]],
        calibration_points=[[0, 0], [1, 0]],
        calibration_metres="10",
    )
    assert result["quantity"] == "8"


def test_quantity_never_claims_engineer_review_or_scale_accuracy():
    result = calculate_measurement("count", [100, 100], [[0.5, 0.5]])
    assert "quantity" in result
    assert "precision_note" in result
    assert "review" in result["precision_note"].lower()
