"""Read-only engineer inspection of existing local execution receipts."""

from typing import Any, Literal

from pydantic import BaseModel, Field


class LocalCodeRun(BaseModel):
    id: str
    engine: Literal["monty", "python"]
    root_run_id: str
    actor_id: str | None = None
    assignment_id: str | None = None
    legacy_attribution: bool = False
    status: Literal["running", "completed", "failed", "cancelled", "interrupted"]
    phase: str
    created_at: str
    duration_seconds: float = 0
    detail: str = ""
    root_active: bool = False


class LocalCodePage(BaseModel):
    items: list[LocalCodeRun]
    next_cursor: str | None = None


class LocalCodeFile(BaseModel):
    name: str
    size_bytes: int
    sha256: str
    id: str | None = None
    source_version: str | None = None


class LocalCodeDetail(BaseModel):
    run: LocalCodeRun
    code: str
    code_sha256: str
    record_integrity: Literal["verified", "legacy_hashes_only"]
    limits: dict[str, Any]
    runtime: dict[str, Any]
    inputs: list[LocalCodeFile] = Field(default_factory=list)
    input_values: dict[str, Any] | None = None
    inputs_sha256: str | None = None
    outputs: list[LocalCodeFile] = Field(default_factory=list)
    logs: list[LocalCodeFile] = Field(default_factory=list)
    stdout: str = ""
    stderr: str = ""
    printed: str = ""
    output: Any = None
    calls: list[dict[str, Any]] = Field(default_factory=list)
