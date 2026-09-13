"""Authenticated API routes for the engineer-customizable Tender Manager."""

from fastapi import APIRouter, HTTPException

from .manager_profile import ManagerProfileService, OfficeConflict
from .staff_models import ManagerProfile, ManagerProfileEdit


def create_router(repo):
    """Build the Manager routes around one profile service for this app."""

    service = ManagerProfileService(repo)
    router = APIRouter(prefix="/api", tags=["Manager"])

    @router.get("/manager-profile", response_model=ManagerProfile)
    def get_manager_profile():
        return service.get()

    @router.patch("/manager-profile", response_model=ManagerProfile)
    def update_manager_profile(edit: ManagerProfileEdit):
        try:
            return service.update(edit)
        except OfficeConflict as error:
            raise HTTPException(
                status_code=409,
                detail="The Tender Manager changed. Refresh before editing.",
            ) from error

    return router


__all__ = ["create_router"]
