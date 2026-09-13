"""Public metadata for original-client sessions; no client credentials or history."""
from typing import Literal

from pydantic import BaseModel, ConfigDict, Field


class NativeSessionBinding(BaseModel):
    model_config = ConfigDict(extra="forbid", strict=True)
    id: str
    tender_id: str
    run_id: str
    protocol: Literal["codex", "grok_build"]
    connection_id: str
    connection_revision: int = Field(ge=1)
    model_id: str
    runtime_revision: str
    profile_id: str
    profile_version: int = Field(ge=1)
    scope_fingerprint: str
    settings_fingerprint: str
    provider_session_id: str | None = None
    state: Literal["prepared", "running", "completed", "interrupted", "failed", "incompatible"] = "prepared"


class NativeClientCapability(BaseModel):
    model_config = ConfigDict(extra="forbid", strict=True)
    id: str
    origin: Literal["client", "quantix"]
    supported: bool
    detail: str
    source: str
