"""Calibrated PDF drawing measurement contracts."""

from typing import Annotated, Literal

from pydantic import Field

from .estimate_models import DecimalText, EstimateModel

Coordinate = Annotated[float, Field(ge=0, le=1, allow_inf_nan=False, strict=True)]
Point = tuple[Coordinate, Coordinate]
Mode = Literal["length", "area", "count"]


class MeasurementInput(EstimateModel):
    artifact_id: str = Field(min_length=1, max_length=100)
    page: int = Field(ge=1, strict=True)
    mode: Mode
    points: list[Point] = Field(min_length=1, max_length=500)
    calibration_points: list[Point] | None = Field(default=None, min_length=2, max_length=2)
    calibration_metres: DecimalText | None = None


class AgentMeasurement(MeasurementInput):
    scope_label: str = Field(min_length=1, max_length=300)
    source_ids: list[str] = Field(min_length=1, max_length=28)


class MeasurementPage(EstimateModel):
    artifact_id: str
    version: int
    content_hash: str
    name: str
    relative_path: str
    page: int
    page_count: int
    page_size: tuple[float, float]
    is_current: bool
