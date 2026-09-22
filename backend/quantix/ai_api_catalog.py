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


# Request fields an OpenAI-compatible provider lists when a model accepts them.
# OpenRouter documents this list and other gateways copy its shape.
_REASONING_PARAMETERS = frozenset(
    {"reasoning", "reasoning_effort", "include_reasoning", "thinking"}
)
_SEARCH_PARAMETERS = frozenset({"web_search", "web_search_options"})
# The efforts the OpenAI API documents; a provider that reasons accepts these.
STANDARD_REASONING = ["low", "medium", "high"]


def _price(value) -> float | None:
    """A per-token price from a provider's model list, in USD per million."""

    try:
        number = float(value)
    except (TypeError, ValueError):
        return None
    if number < 0 or number != number or number in {float("inf"), float("-inf")}:
        return None
    return number * 1_000_000


def catalog_pricing(item: dict) -> dict | None:
    """The provider's own published token prices for this model, if it lists them.

    OpenRouter-style catalogs carry ``pricing`` in USD per token. Those are the
    rates for the very endpoint being billed, so they are used as the model's
    price card and named as the provider's own figures.
    """

    from datetime import date

    from .ai_models import PriceCard

    prices = item.get("pricing")
    if not isinstance(prices, dict):
        return None
    input_price = _price(prices.get("prompt") or prices.get("input"))
    output_price = _price(prices.get("completion") or prices.get("output"))
    if input_price is None or output_price is None:
        return None
    search = _price(prices.get("web_search"))
    return PriceCard(
        input_per_million=input_price,
        output_per_million=output_price,
        cached_input_per_million=_price(prices.get("input_cache_read")),
        web_search_per_call=search / 1_000_000 if search is not None else None,
        source="Published by the provider in its own model list. Confirm it against your provider account.",
        as_of=date.today().isoformat(),
    ).model_dump(mode="json")


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
        ("web_search", "web_search"),
    ):
        value = raw.get(source)
        if isinstance(value, dict):
            value = value.get("supported")
        if type(value) is bool:
            capabilities[target] = value
    if raw.get("reasoning") is True or raw.get("thinking") is True:
        capabilities["reasoning"] = list(STANDARD_REASONING)
    if isinstance(raw.get("reasoning"), list):
        levels = [value for value in raw["reasoning"] if isinstance(value, str) and value]
        if levels:
            capabilities["reasoning"] = levels
    for target, sources in (
        (
            "context_window",
            ("context_length", "max_context_length", "max_input_tokens", "inputTokenLimit"),
        ),
        (
            "max_output_tokens",
            ("max_tokens", "max_output_tokens", "max_completion_tokens", "outputTokenLimit"),
        ),
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
        if "structured_outputs" in parameters or "response_format" in parameters:
            capabilities["structured_output"] = True
        for name in ("temperature", "top_p"):
            if name in parameters:
                capabilities[name] = True
        if _SEARCH_PARAMETERS.intersection(parameters):
            capabilities["web_search"] = True
        if _REASONING_PARAMETERS.intersection(parameters):
            capabilities.setdefault("reasoning", list(STANDARD_REASONING))
    modalities = item.get("input_modalities")
    architecture = item.get("architecture")
    if isinstance(architecture, dict) and isinstance(architecture.get("input_modalities"), list):
        modalities = architecture["input_modalities"]
    if isinstance(modalities, list):
        capabilities["images"] = "image" in modalities
        if "file" in modalities or "pdf" in modalities:
            capabilities["pdf"] = True
    name = (
        item.get("display_name")
        or item.get("displayName")
        or item.get("modelName")
        or item.get("name")
        or identifier
    )
    entry = {"model_id": identifier, "display_name": str(name), "capabilities": capabilities}
    pricing = catalog_pricing(item)
    if pricing is not None:
        entry["pricing"] = pricing
    return entry


async def address_problem(connection: dict) -> str | None:
    """Explain a saved address that is not an OpenAI-compatible API.

    A website answers a model-list request with a page, which the provider SDK
    reports as an unreadable reply. Saying which address does answer is the
    only useful thing Quantix can tell the engineer here.
    """

    if connection.get("protocol") not in {"openai_chat", "openai_responses"}:
        return None
    from .ai_api_endpoint import ADDRESS_HELP, resolve_api_base_url
    from .ai_api_provider import _base_url

    try:
        saved = _base_url(connection)
    except DirectAPIError:
        return None
    found = await resolve_api_base_url(
        saved, allow_insecure_http=connection.get("allow_insecure_http", False)
    )
    if found == saved:
        return None
    if found:
        return (
            f"{saved} is not an AI API. This provider answers at {found}. "
            "Open More options, save that address, and try again."
        )
    return ADDRESS_HELP


async def discover_models(connection: dict, credentials: dict[str, str]) -> list[dict]:
    """Fetch provider metadata with a fresh explicitly authenticated client."""

    if connection.get("protocol") not in {"openai_chat", "openai_responses", "anthropic", "google"}:
        raise DirectAPIError(
            "Model discovery is unavailable for this direct API protocol. Add a model manually."
        )
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
                            raise DirectAPIError(
                                "The provider catalog is too large. Add the required model manually."
                            )
                else:
                    async for item in client.models.list():
                        items.append(_dump(item))
                        if len(items) > MAX_CATALOG_MODELS:
                            raise DirectAPIError(
                                "The provider catalog is too large. Add the required model manually."
                            )
    except asyncio.CancelledError:
        raise
    except (DirectAPIError, DirectDependencyError):
        raise
    except Exception as error:
        detail = await address_problem(connection)
        if detail:
            raise DirectAPIError(detail) from None
        # Catalog requests carry no Tender content, so the provider's own words
        # ("invalid key", "no access to this endpoint") reach the engineer.
        raise provider_failure(error, reveal=True) from None

    result: dict[str, dict] = {}
    for item in items:
        entry = model_entry(item, connection["protocol"])
        if not entry:
            continue
        model_id = entry["model_id"]
        # The provider's own record is the authority: documented and profile
        # capabilities only fill in what it did not say, never overrule a denial.
        entry["capabilities"].update(
            {
                key: value
                for key, value in documented_capabilities(
                    connection["provider_id"], connection["protocol"], model_id
                ).items()
                if entry["capabilities"].get(key) in (None, [], {})
            }
        )
        entry.setdefault("pricing", None)
        if entry["pricing"] is None:
            entry["pricing"] = catalog_price(connection["provider_id"], model_id)
        result[model_id] = entry
    return list(result.values())
