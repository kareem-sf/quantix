"""Public backup/restore contracts and the versioned archive manifest."""

from typing import Annotated, Literal

from pydantic import BaseModel, ConfigDict, Field

Identifier = Annotated[str, Field(pattern=r"^[a-f0-9]{32}$")]
Digest = Annotated[str, Field(pattern=r"^[a-f0-9]{64}$")]


class BackupModel(BaseModel):
    model_config = ConfigDict(extra="forbid", str_strip_whitespace=True)


class BackupFile(BackupModel):
    path: str = Field(min_length=1, max_length=240)
    size: int = Field(ge=0)
    sha256: Digest


class BackupManifest(BackupModel):
    format_version: Literal[1]
    id: Identifier
    created_at: str
    database_schema: Literal[1, 2, 3, 4]
    tender_count: int = Field(ge=0)
    original_count: int = Field(ge=0)
    unavailable_original_count: int = Field(ge=0)
    output_count: int = Field(ge=0)
    purpose: Literal["manual", "before_restore"]
    files: list[BackupFile] = Field(min_length=1, max_length=50000)


class BackupRecord(BackupModel):
    id: Identifier
    filename: str
    created_at: str
    size: int
    sha256: Digest
    format_version: Literal[1]
    tender_count: int
    original_count: int
    unavailable_original_count: int
    output_count: int
    file_count: int
    purpose: Literal["manual", "before_restore"]


class BackupInspection(BackupModel):
    valid: Literal[True]
    compatible: Literal[True]
    backup: BackupRecord
    total_uncompressed_bytes: int
    detail: str


class InspectBackupRequest(BackupModel):
    path: str = Field(min_length=1, max_length=4096)


class RestoreBackupRequest(InspectBackupRequest):
    expected_sha256: Digest
    engineer_confirmed: Literal[True]
    rationale: str = Field(min_length=1, max_length=4000)


class RestoreReady(BackupModel):
    status: Literal["restore_ready"]
    restore_id: Identifier
    backup_id: Identifier
    restart_required: Literal[True]
    detail: str


class RestoreOutcome(BackupModel):
    restore_id: Identifier
    completed_at: str
    before_restore_backup_id: Identifier | None
    recovery_evidence_path: str | None
    detail: str


class RestoreJournal(BackupModel):
    format_version: Literal[1]
    restore_id: Identifier
    backup_id: Identifier
    archive_sha256: Digest
    accepted_at: str
    engineer_confirmed: Literal[True]
    rationale: str = Field(min_length=1, max_length=4000)
    phase: Literal["staged", "prepared", "installing", "completed"]
    before_restore_backup_id: Identifier | None = None
    completed_at: str | None = None
    recovery_evidence_path: str | None = Field(
        default=None, pattern=r"^backups/[a-f0-9]{32}\.recovery\.zip$"
    )
