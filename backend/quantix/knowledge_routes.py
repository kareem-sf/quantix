"""Reusable notes under the authenticated local API; model tools are read-only."""

from fastapi import APIRouter, Query

from .knowledge import KnowledgeService
from .knowledge_models import KnowledgeCategory, KnowledgeCreate, KnowledgeDecision, KnowledgeRecord


def create_router(repo):
    service = KnowledgeService(repo)
    router = APIRouter(prefix="/api", tags=["Approved reusable knowledge"])

    @router.get("/knowledge", response_model=list[KnowledgeRecord])
    def list_notes(
        include_withdrawn: bool = False,
        category: KnowledgeCategory | None = None,
        offset: int = Query(0, ge=0),
        limit: int = Query(100, ge=1, le=100),
    ):
        return service.list(
            include_withdrawn=include_withdrawn, category=category, offset=offset, limit=limit
        )

    @router.post("/knowledge", response_model=KnowledgeRecord)
    def create_note(request: KnowledgeCreate):
        return service.create(request.model_dump(mode="json"))

    @router.get("/knowledge/{knowledge_id}", response_model=KnowledgeRecord)
    def get_note(knowledge_id: str):
        return service.get(knowledge_id)

    @router.post("/knowledge/{knowledge_id}/withdraw", response_model=KnowledgeRecord)
    def withdraw_note(knowledge_id: str, request: KnowledgeDecision):
        return service.withdraw(knowledge_id, request.model_dump())

    return router
