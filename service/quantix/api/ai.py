from pathlib import Path
from typing import Annotated, Any, Literal
from urllib.parse import urlparse

from fastapi import APIRouter, Depends, HTTPException, Request
from pydantic import BaseModel, StringConstraints, model_validator

from quantix import settings
from quantix.ai import check, connections, providers

router = APIRouter(tags=["ai"])
Text = Annotated[str, StringConstraints(strip_whitespace=True, min_length=1)]


def home(request: Request) -> Path:
    return request.app.state.home


Home = Annotated[Path, Depends(home)]


class ModelCheck(BaseModel):
    ok: bool
    message: str
    checked_at: str


class ConnectionOut(BaseModel):
    id: str
    provider: providers.Provider
    label: str
    base_url: str | None
    key_hint: str
    checks: dict[str, ModelCheck]


class ConnectionCreate(BaseModel):
    provider: providers.Provider
    api_key: Text
    base_url: str | None = None

    @model_validator(mode="after")
    def needs_address(self) -> "ConnectionCreate":
        if self.provider == "openai_compatible" and not (self.base_url or "").startswith(("http://", "https://")):
            raise ValueError("An OpenAI-compatible service needs its address, starting with https://")
        return self


class CheckRequest(BaseModel):
    model: Text


class OfficeAI(BaseModel):
    connection_id: str
    model: str


class Settings(BaseModel):
    office_mode: Literal["engineer", "autonomous"]
    office_ai: OfficeAI | None


class SettingsUpdate(BaseModel):
    office_mode: Literal["engineer", "autonomous"] | None = None
    office_ai: OfficeAI | None = None


def _out(connection: dict[str, Any]) -> ConnectionOut:
    return ConnectionOut(**connection, key_hint=f"…{connection['api_key'][-4:]}")


def _connection(home: Path, connection_id: str) -> dict[str, Any]:
    connection = connections.get(home, connection_id)
    if connection is None:
        raise HTTPException(status_code=404, detail="AI connection not found.")
    return connection


@router.get("/ai/connections")
def list_connections(home: Home) -> list[ConnectionOut]:
    return [_out(c) for c in connections.all_connections(home)]


@router.post("/ai/connections", status_code=201)
async def add_connection(body: ConnectionCreate, home: Home) -> ConnectionOut:
    # The key must work before it is kept. An OpenAI-compatible service may not list its models;
    # for those, the model check proves the key.
    try:
        await providers.list_models(body.provider, body.api_key, body.base_url)
    except Exception as error:
        if body.provider != "openai_compatible" or providers.explain(error) != providers.MODEL_MISSING:
            raise HTTPException(status_code=400, detail=providers.explain(error)) from error
    label = urlparse(body.base_url).hostname if body.base_url else providers.LABELS[body.provider]
    return _out(connections.add(home, body.provider, label or body.provider, body.api_key, body.base_url))


@router.delete("/ai/connections/{connection_id}", status_code=204)
def remove_connection(connection_id: str, home: Home) -> None:
    _connection(home, connection_id)
    connections.remove(home, connection_id)
    office_ai = settings.load(home)["office_ai"]
    if office_ai and office_ai["connection_id"] == connection_id:
        settings.save(home, office_ai=None)


@router.get("/ai/connections/{connection_id}/models")
async def list_models(connection_id: str, home: Home) -> list[str]:
    c = _connection(home, connection_id)
    try:
        return await providers.list_models(c["provider"], c["api_key"], c["base_url"])
    except Exception as error:
        if c["provider"] == "openai_compatible":
            return []
        raise HTTPException(status_code=400, detail=providers.explain(error)) from error


@router.post("/ai/connections/{connection_id}/checks")
async def check_model(connection_id: str, body: CheckRequest, home: Home) -> ConnectionOut:
    c = _connection(home, connection_id)
    model = providers.build_model(c["provider"], body.model, c["api_key"], c["base_url"])
    ok, message = await check.check_model(model)
    connections.record_check(home, connection_id, body.model, ok, message)
    return _out(_connection(home, connection_id))


@router.get("/settings")
def get_settings(home: Home) -> Settings:
    return Settings(**settings.load(home))


@router.patch("/settings")
def update_settings(body: SettingsUpdate, home: Home) -> Settings:
    values = body.model_dump(exclude_unset=True)
    office_ai = values.get("office_ai")
    if office_ai:
        c = _connection(home, office_ai["connection_id"])
        if not c["checks"].get(office_ai["model"], {}).get("ok"):
            raise HTTPException(status_code=400, detail="Check this model before the office uses it.")
    return Settings(**settings.save(home, **values))
