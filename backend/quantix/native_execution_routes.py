"""Authenticated local inspection and explicit native cleanup reconciliation."""
from typing import Literal

from fastapi import APIRouter
from fastapi.responses import FileResponse
from pydantic import BaseModel, ConfigDict, Field

from .ai_connections import AIConnectionService, is_subscription_profile
from .models import MutationReceipt
from .native_execution import NativeExecutionService, client_capabilities
from .native_execution_models import NativeClientCapability, NativeSessionBinding
from .native_hosted_code import NativeArtifact, NativeArtifactService


class NativeCleanupReceipt(BaseModel):
    model_config = ConfigDict(extra="forbid")
    id: str
    run_id: str
    connection_id: str
    connection_revision: int
    kind: Literal["upload", "container"]
    state: Literal["pending", "uncertain"]
    created_at: str


class NativeCleanupReview(BaseModel):
    model_config = ConfigDict(extra="forbid", strict=True)
    engineer_confirmed: Literal[True]
    rationale: str = Field(min_length=1, max_length=4000)


def create_router(repo):
    router = APIRouter(prefix="/api", tags=["Native AI execution"])

    @router.get("/ai/connections/{connection_id}/native-capabilities", response_model=list[NativeClientCapability])
    def capabilities(connection_id: str):
        connection = AIConnectionService(repo).get(connection_id)
        if not is_subscription_profile(connection):
            raise ValueError("This connection does not use a supported original client.")
        return client_capabilities(connection["protocol"])

    @router.get("/tenders/{tender_id}/native-sessions", response_model=list[NativeSessionBinding])
    def sessions(tender_id: str):
        return NativeExecutionService(repo).list(tender_id)

    @router.get("/tenders/{tender_id}/runs/{run_id}/native-artifacts", response_model=list[NativeArtifact])
    def artifacts(tender_id: str, run_id: str):
        return NativeArtifactService(repo).list(tender_id, run_id)

    @router.get("/tenders/{tender_id}/native-artifacts/{artifact_id}/content")
    def artifact_content(tender_id: str, artifact_id: str):
        path = NativeArtifactService(repo).path(tender_id, artifact_id)
        return FileResponse(path, filename=path.name, media_type="application/octet-stream")

    @router.get("/tenders/{tender_id}/native-cleanup", response_model=list[NativeCleanupReceipt])
    def cleanup(tender_id: str):
        return NativeArtifactService(repo).pending_cleanup(tender_id)

    @router.post("/tenders/{tender_id}/native-cleanup/{receipt_id}/review", response_model=MutationReceipt)
    def review_cleanup(tender_id: str, receipt_id: str, request: NativeCleanupReview):
        return NativeArtifactService(repo).reconcile_cleanup(tender_id, receipt_id,
            engineer_confirmed=request.engineer_confirmed, rationale=request.rationale)

    return router
