"""Delivery, acknowledgement and coordination records for office exchanges."""

from __future__ import annotations

from typing import Literal

from pydantic import Field

from .staff_models import IdentifierText, OfficeModel, TimestampText

DeliveryState = Literal["delivered", "acknowledged", "inspected"]
CoordinationKind = Literal[
    "calculation_handoff",
    "review_challenge",
    "change_notice",
    "work_transfer",
    "escalation",
    "question",
]


class ReceiptRequest(OfficeModel):
    message_id: IdentifierText
    recipient_assignment_id: IdentifierText | None = None
    extent: str = Field(default="received", max_length=200)
    idempotency_key: IdentifierText


class DeliveryReceipt(OfficeModel):
    message_id: IdentifierText
    recipient_assignment_id: IdentifierText | None = None
    state: DeliveryState
    extent: str
    required_response: str | None = Field(default=None, max_length=500)
    at: TimestampText
    replayed: bool = False


class CoordinationRequest(OfficeModel):
    kind: CoordinationKind
    text: str = Field(min_length=1, max_length=18000)
    root_run_id: IdentifierText
    recipient_staff_id: IdentifierText
    recipient_assignment_id: IdentifierText | None = None
    source_ids: list[IdentifierText] = Field(default_factory=list, max_length=30)
    source_versions: list[int] = Field(default_factory=list, max_length=30)
    reply_to: IdentifierText | None = None
    required_response: str | None = Field(default=None, max_length=500)
    idempotency_key: IdentifierText
    checked: bool | None = None
    accepted: bool | None = None
