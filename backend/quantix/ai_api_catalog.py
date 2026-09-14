"""Authenticated model catalog discovery for direct API connections."""

from __future__ import annotations

import asyncio

from .ai_api_errors import DirectAPIError, DirectDependencyError, provider_failure
from .ai_api_provider import model_for_route
from .ai_catalog import catalog_price, documented_capabilities

CATALOG_DEADLINE_SECONDS = 90
MAX_CATALOG_MODELS = 2000


def _dump(item) -> dict:
    if isinstance(item, dict):
        return dict(item)
    method = getattr(item, "model_dump", None)
    if callable(method):
        return method(by_alias=True, exclude_none=True)
    return {}


def model_entry(item: dict, protocol: str) -> dict | None:
    """Convert one provider model record without guessing its capabilities."""

    identifier = item.get("id") or item.get("name") or item.get("modelId")
    if not isinstance(identifier, str) or not identifier.strip():
        return None
    identifier = identifier.strip()
    if identifier == "catalog":
        return None
    if protocol == "google" and identifier.startswith("models/"):
        # Google documents this resource prefix.  Strip this exact prefix only;
        # arbitrary aliases and prefix matching remain unresolved.
        identifier = identifier[7:]
    raw = item.get("capabilities") if isinstance(item.get("capabilities"), dict) else {}
    capabilities: dict = {}
    for target, source in (
        ("tools", "function_calling"),
        ("images", "vision"),
        ("structured_output", "structured_outputs"),
        ("images", "image_input"),
        ("pdf", "pdf_input"),
        ("temperature", "temperature"),
        ("top_p", "top_p"),
    ):
        value = raw.get(source)
        if isinstance(value, dict):
            value = value.get("supported")
        if type(value) is bool:
            capabilities[target] = value
    for target, sources in (
        ("context_window", ("context_length", "max_context_length", "max_input_tokens", "inputTokenLimit")),
        ("max_output_tokens", ("max_tokens", "outputTokenLimit")),
    ):
        for source in sources:
            value = item.get(source)
            if type(value) is int and value > 0:
                capabilities[target] = value
                break
    parameters = item.get("supported_parameters")
    if isinstance(parameters, list):
        if "tools" in parameters:
            capabilities["tools"] = True
        if "structured_outputs" in parameters:
            capabilities["structured_output"] = True
        for name in ("temperature", "top_p"):
            if name in parameters:
                capabilities[name] = True
    architecture = item.get("architecture")
    if isinstance(architecture, dict) and isinstance(architecture.get("input_modalities"), list):
        capabilities["images"] = "image" in architecture["input_modalities"]
    name = item.get("display_name") or item.get("displayName") or item.get("modelName") or identifier
    return {"model_id": identifier, "display_name": str(name), "capabilities": capabilities}


async def discover_models(connection: dict, credentials: dict[str, str]) -> list[dict]:
    """Fetch provider metadata with a fresh explicitly authenticated client."""

    if connection.get("protocol") not in {"openai_chat", "openai_responses", "anthropic", "google"}:
        raise DirectAPIError("Model discovery is unavailable for this direct API protocol. Add a model manually.")
    route = {"model_id": "catalog", "max_output_tokens": 128, "web_search": False}
    try:
        async with asyncio.timeout(CATALOG_DEADLINE_SECONDS):
            async with model_for_route(route, connection, credentials) as binding:
                protocol, client = connection["protocol"], binding.client
                items: list[dict] = []
                if protocol == "google":
                    async for item in await client.aio.models.list():
                        items.append(_dump(item))
                        if len(items) > MAX_CATALOG_MODELS:
                            raise DirectAPIError("The provider catalog is too large. Add the required model manually.")
                else:
                    async for item in client.models.list():
                        items.append(_dump(item))
                        if len(items) > MAX_CATALOG_MODELS:
                            raise DirectAPIError("The provider catalog is too large. Add the required model manually.")
    except asyncio.CancelledError:
        raise
    except (DirectAPIError, DirectDependencyError):
        raise
    except Exception as error:
        raise provider_failure(error) from None

    result: dict[str, dict] = {}
    for item in items:
        entry = model_entry(item, connection["protocol"])
        if not entry:
            continue
        model_id = entry["model_id"]
        entry["capabilities"].update(
            documented_capabilities(connection["provider_id"], connection["protocol"], model_id)
        )
        entry["pricing"] = catalog_price(connection["provider_id"], model_id)
        result[model_id] = entry
    return list(result.values())
