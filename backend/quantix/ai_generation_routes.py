"""Read-only capability/settings preview; never checks credentials or grants work."""
from fastapi import APIRouter

from .ai_connections import AIConnectionService
from .ai_generation import generation_preview
from .ai_generation_models import GenerationPreview, GenerationPreviewRequest


def create_router(repo):
    router = APIRouter(prefix="/api", tags=["AI generation"])
    connections = AIConnectionService(repo)

    @router.post("/ai/connections/{connection_id}/generation-preview", response_model=GenerationPreview)
    def preview(connection_id: str, request: GenerationPreviewRequest):
        with connections.authority_guard(), repo.atomic():
            connection = connections.get(connection_id)
            model = next((item for item in connections.models(connection_id)
                          if item["model_id"] == request.model_id), None)
            if model is None:
                raise ValueError("Add or discover this exact model before checking its settings.")
            return generation_preview({**connection, "_model": model}, request.settings.model_dump())

    return router
