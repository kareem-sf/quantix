"""Immutable contracts for queued staff work and saved staff drafts.

The prepared draft is deliberately a server-side dataclass.  Provider and
HTTP payloads use ``OfficeOutput`` and the existing preparation path; this
wrapper is only the hand-off from a validated staff runtime to the assignment
store.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Literal

from pydantic import ConfigDict, Field, field_validator

from .office_types import OfficeOutput, PreparedOfficeResult
from .staff_models import IdentifierText, OfficeModel, TimestampText
from .staff_routing_models import ArtifactBasis

AssignmentStatus = Literal[
    "queued",
    "running",
    "waiting",
    "completed",
    "failed",
    "cancelled",
    "interrupted",
]
Currentness = Literal["current", "needs_review"]


class _ImmutableOfficeModel(OfficeModel):
    model_config = ConfigDict(
        extra="forbid", frozen=True, str_strip_whitespace=True
    )


class StaffAssignment(_ImmutableOfficeModel):
    """One server-owned assignment state and its immutable work identity."""

    id: IdentifierText
    tender_id: IdentifierText
    root_run_id: IdentifierText
    staff_id: IdentifierText
    staff_version: int = Field(ge=1)
    work_order_id: IdentifierText
    route_binding_id: IdentifierText
    status: AssignmentStatus
    revision: int = Field(ge=1)
    detail: str = Field(min_length=1, max_length=2000)
    result_id: IdentifierText | None = None
    parent_assignment_id: IdentifierText | None = None
    depth: int = Field(default=0, ge=0, le=8)
    prerequisites: list[IdentifierText] = Field(default_factory=list, max_length=50)
    created_at: TimestampText
    updated_at: TimestampText


class StaffSourceBasis(_ImmutableOfficeModel):
    """The exact evidence and artifact basis used by a saved staff draft."""

    source_id: IdentifierText
    artifact_id: IdentifierText
    artifact_version: int = Field(ge=1)
    artifact_hash: str = Field(pattern=r"^[0-9a-fA-F]{64}$")
    locator: str = Field(min_length=1, max_length=1000)

    @field_validator("artifact_hash")
    @classmethod
    def lower_hash(cls, value: str) -> str:
        return value.lower()

    @property
    def artifact_basis(self) -> ArtifactBasis:
        """Expose the routing model used for exact artifact comparisons."""

        return ArtifactBasis(
            artifact_id=self.artifact_id,
            version=self.artifact_version,
            content_hash=self.artifact_hash,
        )


class StaffResult(_ImmutableOfficeModel):
    """An immutable, source-attributable staff draft.

    This is still draft engineering content.  It contains no engineer
    acceptance and cannot be published by this service.
    """

    id: IdentifierText
    tender_id: IdentifierText
    assignment_id: IdentifierText
    root_run_id: IdentifierText
    staff_id: IdentifierText
    staff_version: int = Field(ge=1)
    work_order_id: IdentifierText
    route_binding_id: IdentifierText
    office_output: OfficeOutput
    authored_notes: tuple[str, ...] = Field(default_factory=tuple, max_length=50)
    source_ids_read: tuple[IdentifierText, ...] = Field(default_factory=tuple, max_length=200)
    source_bases: tuple[StaffSourceBasis, ...] = Field(default_factory=tuple, max_length=200)
    web_sources: tuple[dict[str, Any], ...] = Field(default_factory=tuple, max_length=100)
    item_bases: tuple[tuple[str, str], ...] = Field(default_factory=tuple, max_length=200)
    trusted_recipients: tuple[str, ...] = Field(default_factory=tuple, max_length=200)
    source_recipients: tuple[tuple[str, tuple[str, ...]], ...] = Field(
        default_factory=tuple, max_length=200
    )
    approved_plan_id: IdentifierText | None = None
    usage: dict[str, Any] = Field(default_factory=dict)
    created_at: TimestampText
    currentness: Currentness


@dataclass(frozen=True)
class PreparedStaffDraft:
    """Trusted staff-runtime hand-off; never an HTTP or model input.

    ``authored_notes`` is normalized to a tuple so a caller cannot mutate the
    value after it has been admitted to a transaction.
    """

    assignment_id: str
    staff_id: str
    staff_version: int
    route_binding_id: str
    prepared: PreparedOfficeResult
    authored_notes: tuple[str, ...] = ()

    def __post_init__(self) -> None:
        if not isinstance(self.prepared, PreparedOfficeResult):
            raise TypeError("PreparedOfficeResult is required for a staff draft.")
        if not isinstance(self.assignment_id, str) or not self.assignment_id.strip():
            raise ValueError("A staff draft assignment id is required.")
        if not isinstance(self.staff_id, str) or not self.staff_id.strip():
            raise ValueError("A staff draft staff id is required.")
        if not isinstance(self.route_binding_id, str) or not self.route_binding_id.strip():
            raise ValueError("A staff draft route binding id is required.")
        if type(self.staff_version) is not int or self.staff_version < 1:
            raise ValueError("A staff draft staff version must be positive.")
        notes = self.authored_notes
        if isinstance(notes, list):
            notes = tuple(notes)
            object.__setattr__(self, "authored_notes", notes)
        if not isinstance(notes, tuple) or len(notes) > 50:
            raise ValueError("Staff authored notes must contain at most 50 entries.")
        for note in notes:
            if not isinstance(note, str) or not note.strip() or len(note) > 2000:
                raise ValueError("Each staff authored note must be a short nonblank string.")


__all__ = [
    "AssignmentStatus",
    "Currentness",
    "PreparedStaffDraft",
    "StaffAssignment",
    "StaffResult",
    "StaffSourceBasis",
]
