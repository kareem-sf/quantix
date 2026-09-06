"""Backup HTTP operations; database restoration happens only at offline startup."""

from fastapi import APIRouter
from fastapi.responses import FileResponse

from .backup import BackupService
from .backup_models import (
    BackupInspection,
    BackupRecord,
    InspectBackupRequest,
    RestoreBackupRequest,
    RestoreOutcome,
    RestoreReady,
)


def create_router(repo):
    service = BackupService(repo)
    router = APIRouter(prefix="/api", tags=["Workspace backups"])

    @router.get("/backups", response_model=list[BackupRecord])
    def list_backups():
        return service.list()

    @router.post("/backups", response_model=BackupRecord)
    def create_backup():
        return service.create()

    @router.get("/backups/pending", response_model=RestoreReady | None)
    def pending_restore():
        return service.pending_restore()

    @router.get("/backups/latest", response_model=RestoreOutcome | None)
    def latest_restore():
        return service.latest_restore()

    @router.post("/backups/inspect", response_model=BackupInspection)
    def inspect_backup(request: InspectBackupRequest):
        return service.inspect(request.path)

    @router.post("/backups/restore", response_model=RestoreReady)
    def stage_restore(request: RestoreBackupRequest):
        return service.stage_restore(
            request.path,
            request.engineer_confirmed,
            request.rationale,
            expected_sha256=request.expected_sha256,
        )

    @router.get("/backups/{backup_id}/download", response_class=FileResponse)
    def download(backup_id: str):
        path = service.path(backup_id)
        return FileResponse(path, filename=path.name, media_type="application/zip")

    return router
