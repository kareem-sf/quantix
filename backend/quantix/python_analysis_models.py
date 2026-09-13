"""Public sandbox status and bounded Python analysis contracts."""

from typing import Literal

from pydantic import BaseModel, ConfigDict, Field


class PythonLimits(BaseModel):
    model_config = ConfigDict(extra="forbid")
    cpus: int = Field(default=2, ge=1, le=4)
    memory_mib: int = Field(default=2048, ge=256, le=4096)
    seconds: int = Field(default=120, ge=1, le=300)
    processes: int = Field(default=64, ge=8, le=128)
    output_mib: int = Field(default=50, ge=1, le=100)


class PythonAnalysisRequest(BaseModel):
    model_config = ConfigDict(extra="forbid")
    code: str = Field(min_length=1, max_length=131072)
    input_ids: list[str] = Field(default_factory=list, max_length=64)
    limits: PythonLimits = Field(default_factory=PythonLimits)


class PythonOutput(BaseModel):
    name: str
    size_bytes: int
    sha256: str


class PythonAnalysisReceipt(BaseModel):
    id: str
    status: Literal["running", "completed", "failed", "cancelled"]
    code_sha256: str
    image_id: str
    library_versions: dict[str, str]
    input_hashes: dict[str, str]
    outputs: list[PythonOutput] = Field(default_factory=list)
    stdout: str = ""
    stderr: str = ""
    detail: str = ""
    limits: PythonLimits
    network: Literal["denied"] = "denied"
    created_at: str
    duration_seconds: float = 0


class SandboxStatus(BaseModel):
    state: Literal[
        "not_installed",
        "prerequisite_required",
        "setup_required",
        "stopped",
        "ready",
        "needs_repair",
        "busy",
    ]
    detail: str
    next_action: str
    machine_name: str
    composition_available: bool
    python_available: bool = False
    image_id: str | None = None
    library_versions: dict[str, str] = Field(default_factory=dict)
    limits: PythonLimits = Field(default_factory=PythonLimits)
    active_runs: int = 0


class SandboxAction(BaseModel):
    model_config = ConfigDict(extra="forbid")
    action: Literal["setup", "repair", "remove", "start", "stop"]
