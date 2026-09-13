"""Authenticated local connection management and tender AI controls."""

from fastapi import APIRouter

from .ai_connections import AIConnectionService, is_supported_profile
from .ai_models import (
    AIReconcile,
    AITeam,
    AITeamApproval,
    AIUsageRecord,
    ConnectionInput,
    ConnectionRecord,
    ModelInput,
    ModelRecord,
    ProviderPreset,
    RuntimeStatus,
    TenderAIInput,
    TenderAIRecord,
)
from .ai_policy import AIPolicyService
from .models import MutationReceipt


def create_router(repo, runtimes, setup):
    router = APIRouter(prefix="/api", tags=["AI connections"])
    connections, policies = AIConnectionService(repo), AIPolicyService(repo)

    def runtime_view(account):
        active = account["active"]
        state = "login_pending" if active and account["stage"] == "needs_sign_in" else "installing" if active else account["stage"]
        return {"connection_id": account["id"], "installed": account["software"]["state"] == "ready", "state": state,
                "detail": account["detail"], "login_url": account["login_url"], "user_code": account["user_code"], "docs_url": None}

    @router.get("/ai/providers", response_model=list[ProviderPreset])
    def presets():
        return connections.presets()

    @router.get("/ai/connections", response_model=list[ConnectionRecord])
    def connection_list():
        return connections.list()

    @router.post("/ai/connections", response_model=ConnectionRecord)
    def create(request: ConnectionInput):
        return connections.create(request)

    @router.get("/ai/connections/{connection_id}", response_model=ConnectionRecord)
    def get(connection_id: str):
        return connections.get(connection_id)

    @router.put("/ai/connections/{connection_id}", response_model=ConnectionRecord)
    def update(connection_id: str, request: ConnectionInput):
        return connections.update(connection_id, request)

    @router.delete("/ai/connections/{connection_id}", response_model=MutationReceipt)
    async def delete(connection_id: str):
        if setup._busy(connection_id):
            raise ValueError("Cancel this account's setup before removing it.")
        if setup.worker is not None:
            await setup.worker.cancel_account(connection_id)
        connections.delete(connection_id)
        return {"ok": True}

    @router.get("/ai/connections/{connection_id}/models", response_model=list[ModelRecord])
    def models(connection_id: str):
        return connections.models(connection_id)

    @router.post("/ai/connections/{connection_id}/models", response_model=ModelRecord)
    def save_model(connection_id: str, request: ModelInput):
        return connections.save_model(connection_id, request)

    @router.post("/ai/connections/{connection_id}/discover", response_model=list[ModelRecord])
    async def discover(connection_id: str):
        if not is_supported_profile(connections.get(connection_id)):
            raise ValueError("This saved AI account is retired. Choose one of the five supported provider routes.")
        return await connections.discover(connection_id)

    @router.get("/ai/connections/{connection_id}/runtime", response_model=RuntimeStatus)
    async def runtime(connection_id: str):
        return runtime_view(setup.get(connection_id))

    @router.post("/ai/connections/{connection_id}/install", response_model=RuntimeStatus)
    async def install(connection_id: str):
        return runtime_view(await setup.action(connection_id, {"action": "prepare"}))

    @router.post("/ai/connections/{connection_id}/login", response_model=RuntimeStatus)
    async def login(connection_id: str):
        return runtime_view(await setup.action(connection_id, {"action": "sign_in"}))

    @router.post("/ai/connections/{connection_id}/logout", response_model=RuntimeStatus)
    async def logout(connection_id: str):
        return runtime_view(await setup.action(connection_id, {"action": "sign_out"}))

    @router.get("/tenders/{tender_id}/ai-policy", response_model=TenderAIRecord)
    def policy(tender_id: str):
        return policies.get(tender_id)

    @router.put("/tenders/{tender_id}/ai-policy", response_model=TenderAIRecord)
    def update_policy(tender_id: str, request: TenderAIInput):
        return policies.update(tender_id, request)

    @router.get("/tenders/{tender_id}/plans/{plan_id}/ai-team", response_model=AITeam)
    def team(tender_id: str, plan_id: str):
        return policies.team(tender_id, plan_id)

    @router.post("/tenders/{tender_id}/plans/{plan_id}/ai-team/refresh", response_model=AITeam)
    def refresh_team(tender_id: str, plan_id: str):
        return policies.propose_team(tender_id, plan_id, refresh=True)

    @router.post("/tenders/{tender_id}/plans/{plan_id}/ai-team/approve", response_model=AITeam)
    def approve_team(tender_id: str, plan_id: str, request: AITeamApproval):
        return policies.approve_team(tender_id, plan_id, request.fingerprint, request.rationale,
                                     require_approved_plan=True)

    @router.get("/tenders/{tender_id}/ai-usage", response_model=list[AIUsageRecord])
    def usage(tender_id: str):
        # Stored rows also carry internal search accounting that is not part of the record.
        fields = AIUsageRecord.model_fields
        return [{key: value for key, value in row.items() if key in fields} for row in policies.usage(tender_id)]

    @router.post("/tenders/{tender_id}/ai-usage/{usage_id}/reconcile", response_model=MutationReceipt)
    def reconcile(tender_id: str, usage_id: str, request: AIReconcile):
        return policies.reconcile(tender_id, usage_id, request)

    return router
