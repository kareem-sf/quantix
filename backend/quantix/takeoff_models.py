"""Quantity takeoff lines from the drawings, cross-checked against the BOQ."""

from typing import Literal

from pydantic import BaseModel, ConfigDict, Field, model_validator

from .estimate_models import DecimalText

TakeoffMethod = Literal["dimensions", "schedule", "scaled", "counted"]
Comparison = Literal[
    "matches", "differs", "unit_differs", "no_boq_quantity", "not_in_boq", "not_on_drawings"
]
TakeoffStatus = Literal["proposed", "accepted", "rejected"]


class TakeoffLineProposal(BaseModel):
    model_config = ConfigDict(extra="forbid", str_strip_whitespace=True)

    description: str = Field(min_length=1, max_length=500)
    location: str = Field(default="", max_length=300)
    unit: str = Field(min_length=1, max_length=20)
    quantity: DecimalText | None = None
    method: TakeoffMethod | None = None
    working: str = Field(min_length=1, max_length=4000)
    source_ids: list[str] = Field(min_length=1, max_length=30)
    boq_item_id: str | None = Field(default=None, max_length=100)

    @model_validator(mode="after")
    def complete(self):
        if self.quantity is None and self.boq_item_id is None:
            raise ValueError(
                "Give the quantity, or the BOQ item that cannot be found on the drawings."
            )
        if self.quantity is not None and self.method is None:
            raise ValueError(
                "State how the quantity was taken: dimensions, schedule, scaled or counted."
            )
        if len(set(self.source_ids)) != len(self.source_ids):
            raise ValueError("Cite each drawing source once.")
        return self


class TakeoffBoqItem(BaseModel):
    description: str
    unit: str
    quantity: str | None


class TakeoffLine(BaseModel):
    id: str
    tender_id: str
    run_id: str
    assignment_id: str | None = None
    author: str
    description: str
    location: str
    unit: str
    quantity: str | None
    method: TakeoffMethod | None
    working: str
    source_ids: list[str]
    boq_item_id: str | None
    boq: TakeoffBoqItem | None
    comparison: Comparison
    difference: str | None = None
    difference_percent: str | None = None
    status: TakeoffStatus
    review_note: str = ""
    reviewed_at: str | None = None
    is_current: bool
    created_at: str
    updated_at: str


class TakeoffReview(BaseModel):
    model_config = ConfigDict(extra="forbid", str_strip_whitespace=True)

    decision: Literal["accepted", "rejected"]
    note: str = Field(default="", max_length=2000)
