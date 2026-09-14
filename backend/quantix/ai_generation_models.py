"""Provider-independent generation preferences and source-backed capability views."""
from typing import Literal

from pydantic import BaseModel, ConfigDict, Field


class GenerationSettings(BaseModel):
    model_config = ConfigDict(extra="forbid", strict=True, str_strip_whitespace=True)

    temperature: float | None = Field(default=None, ge=0, le=2, allow_inf_nan=False)
    top_p: float | None = Field(default=None, ge=0, le=1, allow_inf_nan=False)
    reasoning: str | None = Field(default=None, max_length=100)
    max_output_tokens: int = Field(default=8192, ge=128, le=200000)
    output_mode: Literal["auto", "tool", "native", "prompted"] = "auto"
    max_search_calls: int = Field(default=3, ge=1, le=20)


class CapabilityDescriptor(BaseModel):
    model_config = ConfigDict(extra="forbid", strict=True)
    id: str
    origin: Literal["provider", "client", "quantix"]
    support: Literal["supported", "unsupported", "unknown"]
    runtime_supported: bool
    detail: str
    requirements: list[str] = Field(default_factory=list)
    evidence: list[str] = Field(default_factory=list)


class GenerationPreviewRequest(BaseModel):
    model_config = ConfigDict(extra="forbid", strict=True)
    model_id: str = Field(min_length=1, max_length=300)
    settings: GenerationSettings = Field(default_factory=GenerationSettings)


class GenerationPreview(BaseModel):
    model_config = ConfigDict(extra="forbid", strict=True)
    connection_id: str
    connection_revision: int
    model_id: str
    revision: str
    requested: GenerationSettings
    effective: GenerationSettings | None
    capabilities: list[CapabilityDescriptor]
    blockers: list[str]
