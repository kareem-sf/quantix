"""Instruction revisions bound to a work root. T010 extends this store."""

from __future__ import annotations

from typing import Literal

from pydantic import Field

from .staff_models import IdentifierText, OfficeModel, TimestampText


class InstructionRevision(OfficeModel):
    id: IdentifierText
    root_id: IdentifierText
    original_engineer_message_id: IdentifierText | None = None
    revision: int
    content: str
    basis_fingerprint: str
    created_at: TimestampText


SteeringKind = Literal["question", "constraint", "replace", "urgent", "cancel"]


class InstructionRevisionRequest(OfficeModel):
    kind: SteeringKind
    content: str = Field(min_length=1, max_length=4000)
    selection: str = Field(default="", max_length=2000)
    idempotency_key: IdentifierText


class InstructionAdmission(OfficeModel):
    id: IdentifierText
    root_id: IdentifierText
    kind: SteeringKind
    state: Literal["admitted", "applied"] = "admitted"
    replayed: bool = False
    created_at: TimestampText
