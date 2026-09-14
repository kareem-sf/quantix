"""Strict public models for local renderer diagnostics."""

from typing import Literal

from pydantic import BaseModel, ConfigDict, Field


class DiagnosticModel(BaseModel):
    model_config = ConfigDict(extra="forbid", str_strip_whitespace=True)


class DiagnosticsStatus(DiagnosticModel):
    available: bool
    directory: str
    detail: str
    session_id: str
    retention_days: int
    max_total_bytes: int
    max_file_bytes: int
    backup_count: int


class DiagnosticEventInput(DiagnosticModel):
    event: Literal["renderer_error", "unhandled_rejection", "react_boundary", "api_network_error"]
    error_type: (
        Literal[
            "Error",
            "TypeError",
            "RangeError",
            "ReferenceError",
            "SyntaxError",
            "URIError",
            "EvalError",
            "AggregateError",
            "Unknown",
        ]
        | None
    ) = None
    source: Literal["app", "api", "renderer"] | None = None
    line: int | None = Field(default=None, ge=0, le=1_000_000)
    column: int | None = Field(default=None, ge=0, le=1_000_000)
    request_id: str | None = Field(default=None, pattern=r"^[a-f0-9]{32}$")


class DiagnosticEventAck(DiagnosticModel):
    recorded: bool
