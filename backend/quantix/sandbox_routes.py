"""Engineer runtime setup and scoped, read-only execution receipts.

Mount this router behind the application's normal Authorization dependency.
Code execution remains exclusively inside reviewed tool invocation fences.
"""

import json
import re

from fastapi import APIRouter, HTTPException, Query
from fastapi.responses import FileResponse

from .local_code_inspection import LocalCodeInspection
from .local_code_models import LocalCodeDetail, LocalCodePage
from .podman_runtime import PodmanRuntime, get_code_runtime
from .python_analysis_models import PythonAnalysisReceipt, SandboxAction, SandboxStatus
from .sandbox_protocol import SandboxFailure, safe_name, sha256


def create_router(repo, *, runtime: PodmanRuntime | None = None):
    runtime = runtime or get_code_runtime(repo)
    router = APIRouter(prefix="/api", tags=["Local Python"])
    inspection = LocalCodeInspection(repo)

    def inspect(work):
        try:
            return work()
        except KeyError as error:
            raise HTTPException(
                404, "This local code record was not found in the selected work."
            ) from error
        except OSError as error:
            raise HTTPException(
                409,
                "A saved local code file is unavailable. Restore its receipt-linked files and retry.",
            ) from error
        except ValueError as error:
            raise HTTPException(
                409, "The saved local code evidence could not be verified. " + str(error)[:1000]
            ) from error

    @router.get("/tenders/{tender_id}/local-code-runs", response_model=LocalCodePage)
    def local_runs(
        tender_id: str,
        root_run_id: str,
        actor_id: str | None = None,
        assignment_id: str | None = None,
        cursor: str | None = Query(default=None, max_length=4096),
        limit: int = Query(default=25, ge=1, le=50),
    ):
        return inspect(
            lambda: inspection.page(
                tender_id,
                root_run_id,
                actor_id=actor_id,
                assignment_id=assignment_id,
                cursor=cursor,
                limit=limit,
            )
        )

    @router.get(
        "/tenders/{tender_id}/local-code-runs/{engine}/{identifier}", response_model=LocalCodeDetail
    )
    def local_detail(
        tender_id: str,
        engine: str,
        identifier: str,
        root_run_id: str,
        actor_id: str | None = None,
        assignment_id: str | None = None,
    ):
        return inspect(
            lambda: inspection.detail(
                tender_id,
                root_run_id,
                engine,
                identifier,
                actor_id=actor_id,
                assignment_id=assignment_id,
            )
        )

    @router.get("/tenders/{tender_id}/local-code-runs/{engine}/{identifier}/files/{kind}/{name}")
    def local_file(
        tender_id: str,
        engine: str,
        identifier: str,
        kind: str,
        name: str,
        root_run_id: str,
        actor_id: str | None = None,
        assignment_id: str | None = None,
    ):
        path = inspect(
            lambda: inspection.file(
                tender_id,
                root_run_id,
                engine,
                identifier,
                kind,
                name,
                actor_id=actor_id,
                assignment_id=assignment_id,
            )
        )
        return FileResponse(
            path,
            media_type="application/octet-stream",
            filename=name,
            headers={"X-Content-Type-Options": "nosniff"},
        )

    @router.get("/sandbox", response_model=SandboxStatus)
    async def status():
        return await runtime.status()

    @router.post("/sandbox/actions", response_model=SandboxStatus)
    async def action(request: SandboxAction):
        try:
            return await runtime.action(request.action)
        except (SandboxFailure, OSError) as error:
            raise HTTPException(409, str(error)[:2000]) from error

    def receipt(tender_id, identifier):
        if not re.fullmatch(r"[0-9a-f]{32}", identifier):
            raise HTTPException(404, "The Python receipt was not found.")
        path = repo.home / "code" / "runs" / identifier / "receipt.json"
        try:
            record = json.loads(path.read_text(encoding="utf-8"))
        except (OSError, ValueError) as error:
            raise HTTPException(404, "The Python receipt was not found.") from error
        if record.get("tender_id") != tender_id:
            raise HTTPException(404, "The Python receipt was not found in this Tender.")
        return record, path.parent

    @router.get(
        "/tenders/{tender_id}/python-runs/{identifier}", response_model=PythonAnalysisReceipt
    )
    def read_receipt(tender_id: str, identifier: str):
        return receipt(tender_id, identifier)[0]

    @router.get("/tenders/{tender_id}/python-runs/{identifier}/outputs/{filename}")
    def output(
        tender_id: str, identifier: str, filename: str, download: bool = Query(default=True)
    ):
        record, folder = receipt(tender_id, identifier)
        try:
            safe_name(filename)
        except ValueError as error:
            raise HTTPException(404, "The Python output was not found.") from error
        selected = next((item for item in record["outputs"] if item["name"] == filename), None)
        path = folder / "outputs" / filename
        if not selected or not path.is_file() or path.is_symlink():
            raise HTTPException(404, "The Python output was not found.")
        if (
            path.stat().st_size != selected["size_bytes"]
            or sha256(path.read_bytes()) != selected["sha256"]
        ):
            raise HTTPException(409, "The saved Python output changed and needs review.")
        # Always download generated bytes. HTML/SVG/scripts must not execute in
        # the application's authenticated origin, even when download=false.
        return FileResponse(
            path,
            media_type="application/octet-stream",
            filename=filename,
            headers={"X-Content-Type-Options": "nosniff"},
        )

    return router
