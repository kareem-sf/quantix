import asyncio
from datetime import UTC, datetime

_RUNTIME_PROTOCOLS = {'codex', 'copilot', 'claude_agent', 'claude_code', 'gemini_cli'}

def _model_entry(item: dict, protocol: str) -> dict | None:
    identifier = item.get("id") or item.get("name") or item.get("modelId")
    if not isinstance(identifier, str) or not identifier.strip():
        return None
    if protocol == "google" and identifier.startswith("models/"):
        identifier = identifier[7:]
    caps = {}
    raw = item.get("capabilities") or {}
    if isinstance(raw, dict):
        for target, source in [("tools", "function_calling"), ("images", "vision"),
                               ("structured_output", "structured_outputs"), ("images", "image_input"), ("pdf", "pdf_input")]:
            value = raw.get(source)
            if isinstance(value, dict):
                value = value.get("supported")
            if type(value) is bool:
                caps[target] = value
        effort = raw.get("effort", {})
        if isinstance(effort, dict):
            caps["reasoning"] = [name for name, entry in effort.items() if isinstance(entry, dict) and entry.get("supported") is True]
    for target, sources in [("context_window", ("context_length", "max_context_length", "max_input_tokens", "inputTokenLimit")),
                            ("max_output_tokens", ("max_tokens", "outputTokenLimit"))]:
        for source in sources:
            value = item.get(source)
            if type(value) is int and value > 0:
                caps[target] = value
                break
    parameters = item.get("supported_parameters")
    if isinstance(parameters, list):
        if "tools" in parameters:
            caps["tools"] = True
        if "structured_outputs" in parameters:
            caps["structured_output"] = True
    architecture = item.get("architecture") or {}
    if isinstance(architecture, dict) and isinstance(architecture.get("input_modalities"), list):
        caps["images"] = "image" in architecture["input_modalities"]
    return {"model_id": identifier, "display_name": item.get("display_name") or item.get("displayName") or item.get("modelName") or identifier, "capabilities": caps}


async def discover_models(connection, credentials) -> list[dict]:
    from .provider_factory import model_for_route

    if connection["protocol"] in _RUNTIME_PROTOCOLS:
        raise ValueError("Use the client's connection action or add a model manually. API model discovery is unavailable for this client.")
    route = {"model_id": "catalog", "max_output_tokens": 8192, "web_search": False}
    async with model_for_route(route, connection, credentials) as binding:
        protocol, client = connection["protocol"], binding.client
        if protocol == "bedrock":
            def fetch_bedrock():
                control = binding.cloud_session.client("bedrock", region_name=connection["settings"].get("region"))
                try:
                    return control.list_foundation_models().get("modelSummaries", [])
                finally:
                    control.close()
            items = await asyncio.to_thread(fetch_bedrock)
        elif protocol == "google":
            items = []
            async for item in await client.aio.models.list():
                items.append(item.model_dump(by_alias=True, exclude_none=True))
                if len(items) >= 2000:
                    raise ValueError("The provider catalog is too large. Add the required model manually.")
        elif protocol in {"openai_chat", "openai_responses", "anthropic"}:
            items = []
            async for item in client.models.list():
                items.append(item.model_dump(exclude_none=True))
                if len(items) >= 2000:
                    raise ValueError("The provider catalog is too large. Add the required model manually.")
        elif protocol == "mistral":
            result = await client.models.list_async()
            items = [item.model_dump(exclude_none=True) for item in result.data or []]
        elif protocol == "cohere":
            items, token, seen = [], None, set()
            while True:
                page = await client.models.list(page_size=1000, page_token=token, endpoint="chat")
                items.extend(item.model_dump(exclude_none=True) for item in page.models or [])
                token = page.next_page_token
                if not token:
                    break
                if token in seen or len(items) >= 2000:
                    raise ValueError("The provider catalog is too large. Add the required model manually.")
                seen.add(token)
        else:
            raise ValueError("Model discovery is unavailable for this protocol. Add a model manually.")
        if len(items) > 2000:
            raise ValueError("The provider catalog is too large. Add the required model manually.")
    result = {}
    for item in items[:2000]:
        entry = _model_entry(item, protocol)
        if entry:
            if connection["provider_id"] == "openrouter" and isinstance(item.get("pricing"), dict):
                from decimal import Decimal, InvalidOperation
                prices = item["pricing"]
                try:
                    if any(Decimal(str(prices.get(key) or "0")) != 0 for key in ("request", "image", "audio")):
                        raise ValueError("A manual rate card is needed for non-token charges.")
                    entry["pricing"] = {
                        "input_per_million": float(Decimal(prices["prompt"]) * 1000000),
                        "output_per_million": float(Decimal(prices["completion"]) * 1000000),
                        "web_search_per_call": float(Decimal(prices["web_search"])) if prices.get("web_search") else None,
                        "source": "https://openrouter.ai/api/v1/models; provider catalog estimate; upstream rates may vary",
                        "as_of": datetime.now(UTC).isoformat(),
                    }
                except (InvalidOperation, ValueError, TypeError, KeyError):
                    entry["pricing"] = None
            result[entry["model_id"]] = entry
    return list(result.values())