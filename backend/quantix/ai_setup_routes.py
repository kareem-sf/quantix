"""Guided onboarding endpoints; all operate behind the existing local bearer boundary."""

from fastapi import APIRouter

from .ai_models import TenderAIRecord
from .ai_policy import AIPolicyService
from .ai_setup_models import (
    SetupAccount,
    SetupAction,
    SetupCheckCharge,
    SetupCheckPreview,
    SetupConfigure,
    SetupService,
    SetupStart,
    SimpleTenderAIInput,
    TenderThinking,
    TenderThinkingInput,
)
from .ai_subscription import allows_extras
from .ai_thinking import describe, recommended_level, thinking_levels


def create_router(repo, setup, on_tender_ai=None):
    router = APIRouter(prefix="/api", tags=["AI setup"])

    @router.get("/ai/setup/services", response_model=list[SetupService])
    def services():
        return setup.services()

    @router.get("/ai/setup/accounts", response_model=list[SetupAccount])
    def accounts():
        return setup.list()

    @router.post("/ai/setup/accounts", response_model=SetupAccount)
    async def start(request: SetupStart):
        return await setup.start(request)

    @router.get("/ai/setup/accounts/{account_id}", response_model=SetupAccount)
    def get(account_id: str):
        return setup.get(account_id)

    @router.patch("/ai/setup/accounts/{account_id}/configure", response_model=SetupAccount)
    async def configure(account_id: str, request: SetupConfigure):
        return await setup.configure(account_id, request)

    @router.post("/ai/setup/accounts/{account_id}/actions", response_model=SetupAccount)
    async def action(account_id: str, request: SetupAction):
        return await setup.action(account_id, request)

    @router.get("/ai/setup/accounts/{account_id}/check-preview", response_model=SetupCheckPreview)
    async def preview(account_id: str):
        return await setup.check_preview_async(account_id)

    @router.get("/ai/setup/accounts/{account_id}/checks", response_model=list[SetupCheckCharge])
    def check_history(account_id: str):
        setup.connections.get(account_id)
        return setup.store.checks(account_id)

    @router.post("/tenders/{tender_id}/ai-setup", response_model=TenderAIRecord)
    async def tender_setup(tender_id: str, request: SimpleTenderAIInput):
        import anyio

        record = await anyio.to_thread.run_sync(select_tender_ai, repo, setup, tender_id, request)
        # Work that follows the choice (package analysis) is scheduled on the
        # service's event loop, never from a worker thread.
        if on_tender_ai:
            on_tender_ai(tender_id)
        return record

    def manager_model(policy, manager):
        try:
            connection = policy.connections.get(manager["connection_id"])
        except KeyError:
            return None, None
        model = next(
            (
                m
                for m in policy.connections.models(connection["id"])
                if m["model_id"] == manager["model_id"]
            ),
            None,
        )
        return connection, model

    def thinking(policy, tender_id):
        current = policy.get(tender_id)
        manager = current["manager"]
        busy = any(run["status"] in {"queued", "running"} for run in repo.list_runs(tender_id))
        if not manager:
            return {
                "connection_id": None,
                "model_id": None,
                "current": None,
                "options": [],
                "busy": busy,
            }
        connection, model = manager_model(policy, manager)
        levels = thinking_levels(connection, model) if model else []
        saved = manager["reasoning"]
        # A saved level the list no longer offers stays visible, so the chip
        # never claims a setting the Tender is not actually using.
        values = [*levels, *([saved] if saved and saved not in levels else []), None]
        return {
            "connection_id": manager["connection_id"],
            "model_id": manager["model_id"],
            "current": describe(saved),
            "options": [describe(value) for value in values],
            "busy": busy,
        }

    @router.get("/tenders/{tender_id}/ai-thinking", response_model=TenderThinking)
    def tender_thinking(tender_id: str):
        return thinking(AIPolicyService(repo), tender_id)

    @router.post("/tenders/{tender_id}/ai-thinking", response_model=TenderThinking)
    def set_tender_thinking(tender_id: str, request: TenderThinkingInput):
        policy = AIPolicyService(repo)
        with policy.connections.authority_guard(), repo.atomic():
            current = policy.get(tender_id)
            manager = current["manager"]
            if not manager:
                raise ValueError(
                    "Choose the AI for this Tender first, then set how much it thinks."
                )
            connection, model = manager_model(policy, manager)
            if model is None:
                raise ValueError(
                    "Check this AI model in Settings first. Then return to set its thinking."
                )
            if (
                request.reasoning is not None
                and request.reasoning not in thinking_levels(connection, model)
                and request.reasoning != manager["reasoning"]
            ):
                raise ValueError("Choose a thinking level this AI offers.")
            route = manager | {"reasoning": request.reasoning}
            # A specialist that simply follows the Manager keeps following it; a
            # specialist the engineer set separately keeps its own setting.
            specialist = (
                route if current["specialist"] in (None, manager) else current["specialist"]
            )
            label = describe(request.reasoning)["label"]
            policy.update(
                tender_id,
                {
                    "allowed_connection_ids": current["allowed_connection_ids"],
                    "manager": route,
                    "specialist": specialist,
                    "role_routes": current["role_routes"],
                    "fallback_routes": current["fallback_routes"],
                    "run_budget_usd": current["run_budget_usd"],
                    "tender_budget_usd": current["tender_budget_usd"],
                    "max_requests": current["max_requests"],
                    "restore_budget_reviewed": False,
                    "provider_managed_extras": current.get("provider_managed_extras", {}),
                    "engineer_confirmed": True,
                    "rationale": f"Engineer set the Tender Manager's thinking to {label} for {connection['name']}, model {manager['model_id']}.",
                },
            )
        return thinking(policy, tender_id)

    return router


def select_tender_ai(repo, setup, tender_id: str, request: SimpleTenderAIInput):
    """Approve one checked account and model as this Tender's AI (Manager route)."""
    policy = AIPolicyService(repo)
    with policy.connections.authority_guard(), repo.atomic():
        current = policy.get(tender_id)
        account = setup.get(request.account_id)
        connection = account["connection"]
        if (
            connection["protocol"] == "grok_build"
            and request.account_revision != connection["revision"]
        ):
            raise ValueError(
                "This Grok account or its spending choice changed. Review the current Tender confirmation before approving it."
            )
        if account["stage"] != "ready" or account["check"].get("model_id") != request.model_id:
            raise ValueError(
                "Check this AI model in Settings first. Then return to choose it for this Tender."
            )
        model = next(m for m in account["models"] if m["model_id"] == request.model_id)
        metered = account["connection"]["billing"] in {"metered", "unknown"}
        # Switching the Manager between approved accounts keeps the spending
        # allowance this Tender already recorded, so it is entered only once.
        reuse_budget = request.budget_usd is None
        if metered and reuse_budget and not current["tender_budget_usd"]:
            raise ValueError(
                "Enter one spending allowance for this Tender before using separately billed AI."
            )
        route = {
            "connection_id": request.account_id,
            "model_id": request.model_id,
            "reasoning": recommended_level(connection, model),
            "max_output_tokens": min(8192, model["capabilities"].get("max_output_tokens") or 8192),
            "web_search": False,
            "max_search_calls": 3,
        }
        # Changing the Manager or budget does not erase a specialist choice
        # the engineer already made. New simple policies have no overrides.
        specialist = (
            current["specialist"]
            if current["specialist"] and current["specialist"] != current["manager"]
            else route
        )
        # Selecting a subscription must not remove paid-work authority that
        # still belongs to other saved specialist/account choices.
        tender_budget = current["tender_budget_usd"] if reuse_budget else request.budget_usd
        run_budget = current["run_budget_usd"] if reuse_budget else min(request.budget_usd, 1000000)
        if metered and not run_budget:
            run_budget = min(tender_budget, 1000000)
        extras = dict(current.get("provider_managed_extras", {}))
        if allows_extras(connection):
            extras[request.account_id] = connection["revision"]
            spending_detail = "Grok may use purchased credits and provider-managed extras; Quantix cannot guarantee a separate Tender spending cap."
        else:
            extras.pop(request.account_id, None)
            spending_detail = (
                "Grok subscription allowance only; pause when extra spending cannot be ruled out."
                if connection["protocol"] == "grok_build"
                else "Reviewed the displayed spending allowance."
            )
        return policy.update(
            tender_id,
            {
                "allowed_connection_ids": list(
                    dict.fromkeys([*current["allowed_connection_ids"], request.account_id])
                ),
                "manager": route,
                "specialist": specialist,
                "role_routes": current["role_routes"],
                "fallback_routes": current["fallback_routes"],
                "run_budget_usd": run_budget,
                "tender_budget_usd": tender_budget,
                "max_requests": 32,
                "restore_budget_reviewed": request.restore_budget_reviewed,
                "provider_managed_extras": extras,
                "engineer_confirmed": True,
                "rationale": f"Engineer selected Use this AI for the Tender: {connection['name']}, model {request.model_id}; reviewed the displayed data destination. {spending_detail}",
            },
        )
