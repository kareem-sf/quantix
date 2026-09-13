"""Typed provider and controller outcomes for one dynamic staff assignment.

The provider is allowed to return one of two bounded, explicit outcomes.  A
completed branch contains the ordinary Office draft and authored notes; the
question branch contains only the actual question addressed to the Manager.
Neither branch carries actor, root, route, source receipt, or budget identity.
Those values are supplied by the server-owned assignment and execution
context.
"""

from __future__ import annotations

from typing import Annotated, Literal, Union

from pydantic import Field

from .office_types import OfficeOutput
from .staff_assignment_models import StaffAssignment
from .staff_models import IdentifierText, OfficeModel

ShortNote = Annotated[str, Field(min_length=1, max_length=2000)]
QuestionText = Annotated[str, Field(min_length=1, max_length=4000)]


class StaffCompletedDraft(OfficeModel):
    """A provider response containing a draft for Manager review."""

    kind: Literal["completed"] = "completed"
    output: OfficeOutput
    authored_notes: list[ShortNote] = Field(default_factory=list, max_length=50)


class StaffQuestion(OfficeModel):
    """A provider response that needs a bounded clarification from the Manager."""

    kind: Literal["question"] = "question"
    question: QuestionText


StaffProviderResult = Annotated[
    Union[StaffCompletedDraft, StaffQuestion], Field(discriminator="kind")
]


class StaffProviderOutput(OfficeModel):
    """Object-root provider schema containing the discriminated result."""

    result: StaffProviderResult


class StaffAssignmentOutcome(OfficeModel):
    """Server-owned result of attempting one staff assignment."""

    tender_id: IdentifierText
    assignment: StaffAssignment
    result_id: IdentifierText | None = None
    question_message_id: IdentifierText | None = None
    usage: dict = Field(default_factory=dict)


__all__ = [
    "StaffAssignmentOutcome",
    "StaffCompletedDraft",
    "StaffProviderOutput",
    "StaffProviderResult",
    "StaffQuestion",
]
