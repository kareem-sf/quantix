"""Provider-independent generation preferences and source-backed capability views."""
from typing import Literal

from pydantic import BaseModel, ConfigDict, Field, model_validator

NativeToolName = Literal["web_search", "web_fetch", "code_execution"]


class GenerationSettings(BaseModel):
    model_config = ConfigDict(extra="forbid", strict=True, str_strip_whitespace=True)

    temperature: float | None = Field(default=None, ge=0, le=2, allow_inf_nan=False)
    top_p: float | None = Field(default=None, ge=0, le=1, allow_inf_nan=False)
    reasoning: str | None = Field(default=None, max_length=100)
    max_output_tokens: int = Field(default=8192, ge=128, le=200000)
    output_mode: Literal["auto", "tool", "native", "prompted"] = "auto"
    native_tools: list[NativeToolName] = Field(default_factory=list, max_length=3)
    max_search_calls: int = Field(default=3, ge=1, le=20)
    max_native_tool_calls: int = Field(default=3, ge=1, le=20)

    @model_validator(mode="after")
    def distinct_native_tools(self):
        if len(set(self.native_tools)) != len(self.native_tools):
            raise ValueError("Select each native tool once.")
        return self


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


class NativeUploadArtifact(BaseModel):
    model_config = ConfigDict(extra="forbid", strict=True, frozen=True)
    artifact_id: str = Field(min_length=1, max_length=160)
    version: int = Field(ge=1)
    content_hash: str = Field(pattern=r"^[0-9a-f]{64}$")


class NativeToolGrant(BaseModel):
    """Server-only projection of a reviewed, current execution binding.

    Never accept this model from provider arguments or a generation preview.
    The binding owner revalidates approval/source fingerprints at each dispatch.
    Merely constructing a grant does not enable hosted code accounting.
    """
    model_config = ConfigDict(extra="forbid", strict=True, frozen=True)
    approval_id: str = Field(min_length=1)
    tender_id: str = Field(min_length=1)
    run_id: str = Field(min_length=1)
    connection_id: str = Field(min_length=1)
    connection_revision: int = Field(ge=1)
    model_id: str = Field(min_length=1)
    source_scope_fingerprint: str = Field(min_length=1)
    permitted_source_ids: list[str] = Field(default_factory=list)
    native_tools: list[NativeToolName] = Field(default_factory=list)
    web_fetch_domains: list[str] = Field(default_factory=list, max_length=30)
    max_calls_per_request: int = Field(ge=1, le=20)
    hosted_code_spend_usd: float | None = Field(default=None, gt=0, allow_inf_nan=False)
    uploaded_artifacts: list[NativeUploadArtifact] = Field(default_factory=list, max_length=10)
    route_option_id: str | None = None
    code_execution_price: dict | None = None
