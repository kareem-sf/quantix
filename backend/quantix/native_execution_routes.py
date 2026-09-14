"""Authenticated local inspection of original-client sessions."""

from fastapi import APIRouter

from .ai_connections import AIConnectionService, is_subscription_profile
from .native_execution import NativeExecutionService, client_capabilities
from .native_execution_models import NativeClientCapability, NativeSessionBinding


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

    return router
