"""Reproducible calculation records. Drafts do not change accepted BOQ bases."""

from __future__ import annotations

from typing import Literal

from pydantic import Field

from .staff_models import IdentifierText, OfficeModel


class CalculationRequest(OfficeModel):
    method_id: IdentifierText
    method_version: IdentifierText
    inputs: dict
    units: dict
    assumptions: list[str] = []
    precision: str = Field(default="0.01", pattern=r"^(1|0\.0{0,11}1)$")
    rounding: Literal["HALF_UP"] = "HALF_UP"
    idempotency_key: IdentifierText


class CalculationRecord(OfficeModel):
    id: IdentifierText
    tender_id: IdentifierText | None
    method_id: IdentifierText
    method_version: IdentifierText
    formula_hash: IdentifierText
    typed_inputs: dict
    units: dict
    assumptions: list[str]
    precision: str
    rounding: Literal["HALF_UP"]
    outputs: dict
    checks: list[str]
    basis_fingerprint: IdentifierText
    status: str
    dimension_error: bool = False
    limitations: list[str] = Field(default_factory=list)
    created_at: str


class CalculationCheckRequest(OfficeModel):
    calculation_id: IdentifierText
    method_id: IdentifierText
    method_version: IdentifierText
