"""Pydantic DTOs for the durable Tender-scoped office event feed."""

from __future__ import annotations

from typing import Any, Literal

from pydantic import Field

from .staff_models import OfficeModel

OFFICE_EVENT_TYPES = (
    "staff_created",
    "profile_updated",
    "assignment_queued",
    "assignment_started",
    "assignment_waiting",
    "assignment_completed",
    "assignment_failed",
    "assignment_cancelled",
    "assignment_interrupted",
    "message_posted",
    "artifact_shared",
    "scope_changed",
    "source_inspected",
)

OfficeEventType = Literal[
    "staff_created",
    "profile_updated",
    "assignment_queued",
    "assignment_started",
    "assignment_waiting",
    "assignment_completed",
    "assignment_failed",
    "assignment_cancelled",
    "assignment_interrupted",
    "message_posted",
    "artifact_shared",
    "scope_changed",
    "source_inspected",
]


class OfficeEvent(OfficeModel):
    """One immutable, server-created office activity record."""

    event_id: str = Field(min_length=1, max_length=500)
    sequence: int = Field(ge=1)
    tender_id: str = Field(min_length=1, max_length=500)
    event_type: OfficeEventType
    actor_id: str | None = Field(default=None, min_length=1, max_length=500)
    assignment_id: str | None = Field(default=None, min_length=1, max_length=500)
    record_ref: dict[str, Any] | None = None
    payload: dict[str, Any] | None = None
    occurred_at: str = Field(min_length=1, max_length=120)


class OfficeEventPage(OfficeModel):
    """A bounded page after an optional event-ID cursor."""

    items: list[OfficeEvent] = Field(default_factory=list, max_length=200)
    cursor: str | None = Field(default=None, max_length=500)
    has_more: bool = False
    reset_required: bool = False


__all__ = ["OFFICE_EVENT_TYPES", "OfficeEvent", "OfficeEventPage", "OfficeEventType"]
