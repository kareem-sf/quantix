"""Authenticated engineer routes for reusable professional definitions."""

from __future__ import annotations

from fastapi import APIRouter, HTTPException, Query
from pydantic import ValidationError

from .agent_definition_models import (
    AgentDefinitionCreate,
    AgentDefinitionDuplicate,
    AgentDefinitionEdit,
    AgentDefinitionExport,
    AgentDefinitionRecord,
    AgentDefinitionRetire,
)
from .agent_definitions import AgentDefinitionService
from .staff_models import OfficeConflict


def _call(work):
    try:
        return work()
    except ValidationError as error:
        raise HTTPException(
            422, "Check the professional definition fields and try again."
        ) from error
    except KeyError as error:
        raise HTTPException(404, "This professional definition could not be found.") from error
    except (OfficeConflict, ValueError) as error:
        raise HTTPException(409, str(error)[:2000]) from error


def create_router(repo):
    service = AgentDefinitionService(repo)
    router = APIRouter(prefix="/api/agent-definitions", tags=["Agent definitions"])

    @router.get("", response_model=list[AgentDefinitionRecord])
    def list_definitions(include_retired: bool = False):
        return _call(lambda: service.list(include_retired=include_retired))

    @router.post("", response_model=AgentDefinitionRecord)
    def create_definition(command: AgentDefinitionCreate):
        return _call(lambda: service.create(command))

    @router.get("/{definition_id}", response_model=AgentDefinitionRecord)
    def get_definition(
        definition_id: str,
        version: int | None = Query(None, ge=1),
    ):
        return _call(lambda: service.get(definition_id, version))

    @router.patch("/{definition_id}", response_model=AgentDefinitionRecord)
    def edit_definition(definition_id: str, command: AgentDefinitionEdit):
        return _call(lambda: service.revise(definition_id, command))

    @router.get("/{definition_id}/versions", response_model=list[AgentDefinitionRecord])
    def definition_versions(
        definition_id: str,
        offset: int = Query(0, ge=0),
        limit: int = Query(50, ge=1, le=200),
    ):
        return _call(lambda: service.versions(definition_id, offset=offset, limit=limit))

    @router.post("/{definition_id}/duplicate", response_model=AgentDefinitionRecord)
    def duplicate_definition(definition_id: str, command: AgentDefinitionDuplicate):
        return _call(lambda: service.duplicate(definition_id, command))

    @router.post("/{definition_id}/retire", response_model=AgentDefinitionRecord)
    def retire_definition(definition_id: str, command: AgentDefinitionRetire):
        return _call(lambda: service.retire(definition_id, command))

    @router.get("/{definition_id}/export", response_model=AgentDefinitionExport)
    def export_definition(
        definition_id: str,
        version: int | None = Query(None, ge=1),
    ):
        return _call(lambda: service.export(definition_id, version=version))

    return router


__all__ = ["create_router"]
