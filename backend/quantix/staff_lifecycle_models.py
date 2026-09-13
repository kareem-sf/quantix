"""Request/response models for staff identity lifecycle and working preferences."""

from __future__ import annotations

from typing import Literal

from pydantic import Field

from .staff_models import IdentifierText, OfficeModel
from .staff_store_models import StaffProfileRecord

LifecycleTarget = Literal["available", "retired", "archived"]
PreferenceApplicability = Literal["subsequent", "explicit_revision"]


class StaffLifecycleRequest(OfficeModel):
    staff_id: IdentifierText
    expected_version: int = Field(ge=1)
    target: LifecycleTarget
    reason: str = Field(min_length=1, max_length=2000)
    idempotency_key: IdentifierText


class StaffPreferenceRequest(OfficeModel):
    staff_id: IdentifierText
    expected_version: int = Field(ge=1)
    preferences: list[str] = Field(min_length=1, max_length=20)
    applies_from: PreferenceApplicability = "subsequent"
    idempotency_key: IdentifierText


class StaffLifecycleReceipt(OfficeModel):
    staff: StaffProfileRecord
    replayed: bool = False
