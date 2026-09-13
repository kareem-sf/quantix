"""HTTP routes for the immutable plan review and its combined approval."""

from fastapi import APIRouter, Query

from .plan_review import PlanReviewService
from .plan_review_models import (
    DelegationArtifactOptionPage,
    DelegationProposal,
    DelegationProposalEdit,
    PlanApprovalResult,
    PlanReview,
    PlanReviewApproval,
)


def create_router(
    repo,
    review_service: PlanReviewService | None = None,
    *,
    save_runs_in_transaction=None,
    schedule_after_commit=None,
):
    service = review_service or PlanReviewService(
        repo,
        save_runs_in_transaction=save_runs_in_transaction,
        schedule_after_commit=schedule_after_commit,
    )
    router = APIRouter(prefix="/api", tags=["Plan review"])

    @router.get("/tenders/{tender_id}/plans/{plan_id}/review", response_model=PlanReview)
    def review(tender_id: str, plan_id: str):
        return service.review(tender_id, plan_id)

    @router.patch(
        "/tenders/{tender_id}/plans/{plan_id}/delegation",
        response_model=DelegationProposal,
    )
    def update_delegation(
        tender_id: str,
        plan_id: str,
        request: DelegationProposalEdit,
    ):
        return service.update_delegation(
            tender_id,
            plan_id,
            request.model_dump(mode="json"),
        )

    @router.get(
        "/tenders/{tender_id}/plans/{plan_id}/delegation/artifacts",
        response_model=DelegationArtifactOptionPage,
    )
    def delegation_artifacts(
        tender_id: str,
        plan_id: str,
        offset: int = Query(default=0, ge=0),
        limit: int = Query(default=50, ge=1, le=100),
        query: str | None = Query(default=None, max_length=200),
    ):
        return service.artifact_options(
            tender_id,
            plan_id,
            offset=offset,
            limit=limit,
            query=query,
        )

    @router.post(
        "/tenders/{tender_id}/plans/{plan_id}/review/approve-and-start",
        response_model=PlanApprovalResult,
    )
    async def approve_and_start(tender_id: str, plan_id: str, request: PlanReviewApproval):
        return service.approve_and_start(tender_id, plan_id, request.model_dump(mode="json"))

    return router
