"""Validated records returned by the generated-staff persistence service."""

from __future__ import annotations

from typing import Annotated

from pydantic import Field

from .staff_models import (
    IdentifierText,
    OfficeModel,
    StaffProfileDraft,
    StaffWorkOrder,
    TimestampText,
)

LifecycleText = Annotated[str, Field(min_length=1, max_length=40)]
PortraitStyleText = Annotated[str, Field(min_length=1, max_length=80)]


class StaffPortrait(OfficeModel):
    """Stable renderer metadata for one generated staff identity."""

    style: PortraitStyleText
    seed: IdentifierText


class StaffProfileRecord(StaffProfileDraft):
    """One immutable generated profile version."""

    id: IdentifierText
    tender_id: IdentifierText
    version: int = Field(ge=1)
    creator_run_id: IdentifierText
    manager_profile_version: int = Field(ge=1)
    created_at: TimestampText
    updated_at: TimestampText
    lifecycle: LifecycleText
    portrait: StaffPortrait
    definition_id: IdentifierText | None = None
    definition_version: int | None = Field(default=None, ge=1)


class StaffWorkOrderRecord(OfficeModel):
    """Immutable assignment snapshot tied to a staff profile version."""

    id: IdentifierText
    tender_id: IdentifierText
    staff_id: IdentifierText
    staff_version: int = Field(ge=1)
    creator_run_id: IdentifierText
    scope_id: IdentifierText
    work_order: StaffWorkOrder
    created_at: TimestampText
    definition_id: IdentifierText | None = None
    definition_version: int | None = Field(default=None, ge=1)


class StaffCreationReceipt(OfficeModel):
    """Result of creating a staff profile and its first assignment."""

    staff: StaffProfileRecord
    work_order: StaffWorkOrderRecord
    replayed: bool


class StaffRevisionReceipt(OfficeModel):
    """Result of creating a new immutable version of an existing profile."""

    staff: StaffProfileRecord
    replayed: bool


class StaffWorkOrderReceipt(OfficeModel):
    """Result of creating an additional immutable work order."""

    work_order: StaffWorkOrderRecord
    replayed: bool


__all__ = [
    "StaffCreationReceipt",
    "StaffProfileRecord",
    "StaffPortrait",
    "StaffRevisionReceipt",
    "StaffWorkOrderReceipt",
    "StaffWorkOrderRecord",
]
