"""Typed contracts for actual Tender Office messages and handoffs.

Message input contains only choices (recipient IDs and reference IDs).  The
message service resolves every identity, profile version and artifact basis
from the main database before it saves a record.
"""

from __future__ import annotations

from typing import Annotated, Literal, Union

from pydantic import ConfigDict, Field, model_validator

from .office_delivery_models import DeliveryState
from .staff_models import IdentifierText, OfficeModel, TimestampText


class _MessageModel(OfficeModel):
    model_config = ConfigDict(
        extra="forbid",
        frozen=True,
        str_strip_whitespace=True,
    )


ParticipantKind = Literal["manager", "staff"]
MessageKind = Literal[
    "instruction",
    "note",
    "question",
    "reply",
    "finding",
    "handoff",
]


class OfficeParticipant(_MessageModel):
    """A historic speaker or recipient snapshot supplied by the service."""

    id: IdentifierText
    version: int = Field(ge=1)
    display_name: Annotated[str, Field(min_length=1, max_length=120)]
    title: Annotated[str, Field(min_length=1, max_length=200)]
    kind: ParticipantKind
    assignment_id: IdentifierText | None = None


class OfficeRecipientTarget(_MessageModel):
    """A recipient choice; it never carries identity or permission claims."""

    staff_id: IdentifierText
    assignment_id: IdentifierText | None = None


class OfficeSourceReferenceRequest(_MessageModel):
    kind: Literal["source"] = "source"
    source_id: IdentifierText


class OfficeOutputReferenceRequest(_MessageModel):
    kind: Literal["output"] = "output"
    output_id: IdentifierText


class OfficeStaffResultReferenceRequest(_MessageModel):
    kind: Literal["staff_result"] = "staff_result"
    staff_result_id: IdentifierText


OfficeReferenceRequest = Annotated[
    Union[
        OfficeSourceReferenceRequest,
        OfficeOutputReferenceRequest,
        OfficeStaffResultReferenceRequest,
    ],
    Field(discriminator="kind"),
]


class OfficeArtifactReference(_MessageModel):
    """A server-resolved immutable reference attached to one message."""

    kind: Literal["source", "output", "staff_result"]
    id: IdentifierText
    artifact_id: IdentifierText | None = None
    artifact_version: int | None = Field(default=None, ge=1)
    content_hash: str | None = Field(default=None, pattern=r"^[0-9a-fA-F]{64}$")
    locator: Annotated[str, Field(min_length=1, max_length=1000)] | None = None
    sha256: str | None = Field(default=None, pattern=r"^[0-9a-fA-F]{64}$")
    filename: Annotated[str, Field(min_length=1, max_length=500)] | None = None
    assignment_id: IdentifierText | None = None
    staff_id: IdentifierText | None = None
    staff_version: int | None = Field(default=None, ge=1)
    root_run_id: IdentifierText | None = None

    @model_validator(mode="after")
    def validate_shape(self) -> "OfficeArtifactReference":
        if self.kind == "source":
            if not all(
                value is not None
                for value in (
                    self.artifact_id,
                    self.artifact_version,
                    self.content_hash,
                    self.locator,
                )
            ):
                raise ValueError("A source reference must retain its exact artifact basis.")
            if any(
                value is not None
                for value in (
                    self.sha256,
                    self.filename,
                    self.assignment_id,
                    self.staff_id,
                    self.staff_version,
                    self.root_run_id,
                )
            ):
                raise ValueError("A source reference contains fields for another reference kind.")
        elif self.kind == "output":
            if self.sha256 is None or self.filename is None:
                raise ValueError("An output reference must retain its exact file basis.")
            if any(
                value is not None
                for value in (
                    self.artifact_id,
                    self.artifact_version,
                    self.content_hash,
                    self.locator,
                    self.assignment_id,
                    self.staff_id,
                    self.staff_version,
                    self.root_run_id,
                )
            ):
                raise ValueError("An output reference contains fields for another reference kind.")
        else:
            if self.assignment_id is None or self.staff_id is None or self.staff_version is None:
                raise ValueError(
                    "A staff result reference must retain its immutable staff identity."
                )
            if any(
                value is not None
                for value in (
                    self.artifact_id,
                    self.artifact_version,
                    self.content_hash,
                    self.locator,
                    self.sha256,
                    self.filename,
                )
            ):
                raise ValueError(
                    "A staff result reference contains fields for another reference kind."
                )
        return self


class OfficeMessage(_MessageModel):
    """One actual authored exchange between the Manager and staff."""

    id: IdentifierText
    tender_id: IdentifierText
    root_run_id: IdentifierText
    sender: OfficeParticipant
    recipients: Annotated[list[OfficeParticipant], Field(min_length=1, max_length=10)]
    assignment_id: IdentifierText | None = None
    kind: MessageKind
    text: Annotated[str, Field(min_length=1, max_length=18000)]
    artifact_refs: Annotated[list[OfficeArtifactReference], Field(max_length=30)] = Field(
        default_factory=list
    )
    reply_to: IdentifierText | None = None
    created_at: TimestampText
    delivery_state: DeliveryState | None = None
    required_response: str | None = Field(default=None, max_length=500)


class OfficeMessagePage(_MessageModel):
    """A bounded chronological page of retained messages."""

    items: Annotated[list[OfficeMessage], Field(max_length=50)]
    next_cursor: Annotated[str, Field(min_length=1, max_length=1000)] | None = None


__all__ = [
    "MessageKind",
    "OfficeArtifactReference",
    "OfficeMessage",
    "OfficeMessagePage",
    "OfficeOutputReferenceRequest",
    "OfficeParticipant",
    "OfficeRecipientTarget",
    "OfficeReferenceRequest",
    "OfficeReferenceRequest",
    "OfficeSourceReferenceRequest",
    "OfficeStaffResultReferenceRequest",
    "ParticipantKind",
]
