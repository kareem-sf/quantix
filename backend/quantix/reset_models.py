"""Public reset contracts; private credential metadata never reaches the renderer."""

from typing import Literal

from pydantic import BaseModel, ConfigDict, Field

ResetState = Literal["cleaning_credentials", "credential_error", "ready", "deleting", "failed"]


class ResetPreview(BaseModel):
    supported: bool
    home: str
    tender_count: int
    artifact_count: int
    account_count: int
    backup_count: int
    blockers: list[str]
    fingerprint: str


class ResetRequest(BaseModel):
    model_config = ConfigDict(extra="forbid")
    fingerprint: str = Field(pattern=r"^[a-f0-9]{64}$")
    confirmation: Literal["RESET"]


class ResetReceipt(BaseModel):
    reset_id: str
    state: ResetState
    detail: str


class ResetStatus(ResetReceipt):
    credentials_cleared: bool
    fingerprint: str
