"""Exact handoff records for staff-result transfer and bounded reading."""

from __future__ import annotations

from typing import Any, Literal

from pydantic import Field

from .staff_models import IdentifierText, OfficeModel, TimestampText

Applicability = Literal["current", "needs_review", "historical"]


class TransferRequest(OfficeModel):
    result_id: IdentifierText
    recipient_staff_id: IdentifierText
    purpose: str = Field(min_length=1, max_length=2000)
    expected_basis_fingerprint: str = Field(min_length=1, max_length=64)
    idempotency_key: IdentifierText


class Handoff(OfficeModel):
    id: IdentifierText
    tender_id: IdentifierText
    sender_staff_id: IdentifierText
    recipient_staff_id: IdentifierText
    result_id: IdentifierText
    purpose: str
    basis_fingerprint: str
    applicability: Applicability = "current"
    created_at: TimestampText


class HandoffSelection(OfficeModel):
    handoff_id: IdentifierText
    offset: int = Field(default=0, ge=0)
    limit: int = Field(default=50, ge=1, le=200)


class HandoffPayload(OfficeModel):
    handoff: Handoff
    items: list[dict[str, Any]] = Field(default_factory=list)
    next_offset: int | None = None
    total_rows: int = Field(ge=0)
    exact_versions: dict[str, Any] = Field(default_factory=dict)
    applicability: Applicability


class HandoffView(OfficeModel):
    handoff: Handoff
    direction: Literal["received", "sent"]
    counterpart_staff_id: IdentifierText
    counterpart_display_name: str = Field(min_length=1, max_length=300)


class HandoffPage(OfficeModel):
    items: list[HandoffView] = Field(default_factory=list, max_length=50)
    next_cursor: str | None = Field(default=None, max_length=1000)
    total: int = Field(default=0, ge=0)
