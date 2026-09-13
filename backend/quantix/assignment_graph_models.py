"""Adaptive work-graph drafts and saved versions."""

from __future__ import annotations

from typing import Literal

from pydantic import Field

from .staff_models import IdentifierText, OfficeModel, TimestampText

NodeState = Literal["ready", "blocked", "running", "completed", "failed", "cancelled"]


class GraphNodeDraft(OfficeModel):
    key: str = Field(min_length=1, max_length=80)
    brief: str = Field(min_length=1, max_length=4000)
    expected_output: str = Field(min_length=1, max_length=2000)
    completion_criteria: str = Field(min_length=1, max_length=2000)
    owner_staff_id: IdentifierText | None = None
    prerequisite_keys: list[str] = Field(default_factory=list, max_length=50)


class GraphDraft(OfficeModel):
    outcome: str = Field(min_length=1, max_length=2000)
    root_run_id: IdentifierText
    nodes: list[GraphNodeDraft] = Field(min_length=1, max_length=50)
    engineer_message_id: IdentifierText | None = None
    idempotency_key: IdentifierText


class GraphRevisionRequest(OfficeModel):
    expected_revision: int = Field(ge=1)
    root_run_id: IdentifierText
    reason: str = Field(min_length=1, max_length=2000)
    nodes: list[GraphNodeDraft] = Field(min_length=1, max_length=50)
    idempotency_key: IdentifierText


class WorkNode(OfficeModel):
    id: IdentifierText
    key: str
    owner_staff_id: IdentifierText | None = None
    expected_output: str
    completion_criteria: str
    prerequisite_ids: list[IdentifierText] = Field(default_factory=list)
    state: NodeState = "ready"
    brief: str


class WorkGraphVersion(OfficeModel):
    id: IdentifierText
    root_id: IdentifierText
    revision: int
    instruction_revision_id: IdentifierText
    nodes: list[WorkNode]
    edges: list[list[IdentifierText]]
    basis_fingerprint: str
    created_at: TimestampText


NodeDerivedState = Literal[
    "ready", "waiting", "working", "completed", "needs_owner", "needs_attempt"
]


class NodeStatus(OfficeModel):
    key: str = Field(min_length=1, max_length=80)
    derived: NodeDerivedState
    waiting_on: list[str] = Field(default_factory=list, max_length=50)
    remedy: str = Field(min_length=1, max_length=500)


class WorkGraphStatus(OfficeModel):
    graph: WorkGraphVersion
    nodes: list[NodeStatus] = Field(default_factory=list, max_length=50)
