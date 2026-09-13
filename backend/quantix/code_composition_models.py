"""Bounded local composition; input values are data, never authority."""

from typing import Any, Literal

from pydantic import BaseModel, ConfigDict, Field


class CompositionRequest(BaseModel):
    model_config = ConfigDict(extra="forbid")
    code: str = Field(min_length=1, max_length=65536)
    inputs: dict[str, Any] = Field(default_factory=dict)


class CompositionReceipt(BaseModel):
    id: str
    status: Literal["completed", "failed", "cancelled"]
    engine: str = "pydantic-monty"
    engine_version: str
    runtime_sha256: str
    code_sha256: str
    inputs_sha256: str
    tool_calls: int = 0
    tool_names: list[str] = Field(default_factory=list)
    output: Any = None
    printed: str = ""
    detail: str = ""
    created_at: str
    duration_seconds: float
