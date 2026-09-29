from pathlib import Path
from typing import Annotated, Any, Literal
from urllib.parse import urlparse

import httpx
from fastapi import APIRouter, Depends, HTTPException, Request
from pydantic import BaseModel, Field, StringConstraints, model_validator

from quantix import settings, tenders
from quantix.ai import check, connections, providers
from quantix.api.tenders import DB
from quantix.documents import web
from quantix.office import records
from quantix.review import scorecard

router = APIRouter(tags=["ai"])
Text = Annotated[str, StringConstraints(strip_whitespace=True, min_length=1)]


def home(request: Request) -> Path:
    return request.app.state.home


Home = Annotated[Path, Depends(home)]


class ModelCheck(BaseModel):
    ok: bool
    message: str
    checked_at: str
    sees_images: bool = False  # checks made before this was tested count as not seeing


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
    tender_allowance: int | None  # the most AI tokens one tender's office may use
    notifications: Literal["all", "decisions", "off"]  # Windows notifications while Quantix isn't in front


class SettingsUpdate(BaseModel):
    office_mode: Literal["engineer", "autonomous"] | None = None
    office_ai: OfficeAI | None = None
    tender_allowance: int | None = Field(default=None, ge=1)
    notifications: Literal["all", "decisions", "off"] | None = None


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
    ok, message, sees_images = await check.check_model(model)
    connections.record_check(home, connection_id, body.model, ok, message, sees_images)
    return _out(_connection(home, connection_id))


class WebKeys(BaseModel):
    """The web research keys the engineer added, as hints; None for a service without one."""

    firecrawl: str | None
    tinyfish: str | None


class WebKey(BaseModel):
    api_key: Text


def _web_keys(home: Path) -> WebKeys:
    keys = connections.web_keys(home)
    return WebKeys(**{s: f"…{keys[s][-4:]}" if s in keys else None for s in web.SERVICES})


@router.get("/web/keys")
def get_web_keys(home: Home) -> WebKeys:
    return _web_keys(home)


@router.put("/web/keys/{service}")
def set_web_key(service: Literal["firecrawl", "tinyfish"], body: WebKey, home: Home) -> WebKeys:
    """Keep a web research key once the service accepts it."""
    try:
        web.check_key(service, body.api_key)
    except httpx.HTTPStatusError as error:
        raise HTTPException(status_code=400, detail="The key was refused.") from error
    except httpx.HTTPError as error:
        raise HTTPException(status_code=502, detail="The service couldn't be reached. Try again later.") from error
    connections.set_web_key(home, service, body.api_key)
    return _web_keys(home)


@router.delete("/web/keys/{service}", status_code=204)
def remove_web_key(service: Literal["firecrawl", "tinyfish"], home: Home) -> None:
    connections.set_web_key(home, service, None)


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


class ModelScore(BaseModel):
    model: str
    turns: int
    finished: int  # turns that ended done, rather than cut short or failed
    calls: int
    calls_sent_back: int  # tool calls Quantix sent back with a reason
    accepted: int  # records filed on its turns that the Tender Manager or the engineer accepted
    sent_back: int  # records filed on its turns that were sent back
    tokens: int


class TenderUsage(BaseModel):
    tender_id: str
    name: str
    tokens: int


class Usage(BaseModel):
    models: list[ModelScore]  # the most recently used first
    tenders: list[TenderUsage]


@router.get("/ai/usage")
def usage(session: DB) -> Usage:
    """How each AI model has done in the office, and the tokens each tender's office has used."""
    return Usage(
        models=[ModelScore(**vars(s)) for s in scorecard.scores(session)],
        tenders=[
            TenderUsage(tender_id=t.id, name=t.name, tokens=records.tokens_used(session, t.id))
            for t in tenders.list_tenders(session)
        ],
    )
