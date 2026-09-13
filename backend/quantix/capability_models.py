"""Immutable capability metadata and invocation receipts for the unified tool fence."""

from __future__ import annotations

import json
import math
from typing import Any, Literal

from pydantic import BaseModel, ConfigDict, Field

ExecutionStatus = Literal[
    "completed",
    "partial",
    "needs_input",
    "blocked",
    "failed",
    "uncertain_external_outcome",
]


class CapabilityModel(BaseModel):
    model_config = ConfigDict(extra="forbid", str_strip_whitespace=True)


class ExecutionReceipt(CapabilityModel):
    """Server-owned outcome of one capability invocation."""

    status: ExecutionStatus
    capability_id: str = Field(min_length=1, max_length=500)
    capability_version: int = Field(ge=1)
    request_id: str = Field(min_length=1, max_length=160)
    evidence: list[dict[str, Any]] = Field(default_factory=list, max_length=200)
    artifacts: list[dict[str, Any]] = Field(default_factory=list, max_length=200)
    changed_records: list[dict[str, Any]] = Field(default_factory=list, max_length=200)
    usage: dict[str, Any] = Field(default_factory=dict)
    limitations: list[str] = Field(default_factory=list, max_length=50)
    result: Any = None


MAX_RESULT_BYTES = 1024 * 1024


def _validate_json_shape(value: Any, path: str = "result") -> None:
    """Reject values that cannot be trusted as a bounded JSON tool result."""

    if value is None or isinstance(value, (str, bool, int)):
        return
    if isinstance(value, float):
        if not math.isfinite(value):
            raise ValueError(f"The tool result contains a non-finite number at {path}.")
        return
    if isinstance(value, (list, tuple)):
        for index, item in enumerate(value):
            _validate_json_shape(item, f"{path}[{index}]")
        return
    if isinstance(value, dict):
        for key, item in value.items():
            if not isinstance(key, str):
                raise ValueError(f"The tool result contains a non-string key at {path}.")
            _validate_json_shape(item, f"{path}.{key}")
        return
    raise ValueError(f"The tool result contains an unsupported value at {path}.")


def validate_result_value(value: Any, *, max_bytes: int = MAX_RESULT_BYTES) -> Any:
    """Validate a trusted tool result without treating data keys as authority."""

    _validate_json_shape(value)
    try:
        encoded = json.dumps(value, ensure_ascii=False, allow_nan=False)
    except (TypeError, ValueError) as error:
        raise ValueError("The tool result is not valid JSON data.") from error
    if len(encoded.encode("utf-8")) > max_bytes:
        raise ValueError("The tool result exceeds the admitted payload size.")
    return value
