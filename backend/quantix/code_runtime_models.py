"""Exact reviewed runtime proof and server-enforced resource ceilings."""

from typing import Literal

from pydantic import Field

from .staff_models import OfficeModel


class ReviewedCodeRuntime(OfficeModel):
    engine: Literal["monty", "python"]
    fingerprint: str = Field(pattern=r"^[a-f0-9]{64}$")
    version: str
    image_id: str | None = None
    library_versions: dict[str, str] = Field(default_factory=dict)
    seconds: int = Field(ge=1, le=300)
    memory_mib: int = Field(ge=16, le=4096)
    cpus: int = Field(ge=1, le=4)
    processes: int = Field(ge=1, le=128)
    output_mib: int = Field(ge=1, le=100)
    tool_calls: int = Field(ge=0, le=64)
