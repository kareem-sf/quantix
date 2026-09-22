"""Documented connection presets; model capabilities are never inferred from a brand."""

from .ai_models import ProviderPreset


def _preset(
    identifier,
    name,
    protocols,
    base_url,
    docs_url,
    *,
    auth=None,
    billing="metered",
    fields=(),
    notes=(),
    kind="api",
):
    return ProviderPreset(
        id=identifier,
        name=name,
        kind=kind,
        protocols=protocols,
        default_protocol=protocols[0],
        base_url=base_url,
        auth_methods=auth or ["api_key", "environment"],
        billing=billing,
        docs_url=docs_url,
        settings_fields=list(fields),
        notes=list(notes),
    ).model_dump(mode="json")


_PRESETS = [
    _preset(
        "openai",
        "OpenAI",
        ["openai_responses", "openai_chat"],
        "https://api.openai.com/v1",
        "https://developers.openai.com/api/docs",
        notes=[
            "API usage has separate billing from a ChatGPT subscription.",
            "Model names from the provider do not establish tool or image support.",
        ],
    ),
    _preset(
        "anthropic",
        "Anthropic Claude",
        ["anthropic"],
        "https://api.anthropic.com",
        "https://platform.claude.com/docs/en/api/overview",
        notes=[
            "Document citations and strict JSON output cannot be combined in one request.",
            "Native search availability depends on the model.",
        ],
    ),
    _preset(
        "google",
        "Google Gemini API",
        ["google"],
        None,
        "https://ai.google.dev/gemini-api/docs/api-key",
        notes=[
            "Use the current API key instructions in Google AI Studio.",
            "Unpaid and billing-enabled projects have different data terms.",
            "Google Search has separate billing and retention rules.",
        ],
    ),
    _preset(
        "xai",
        "xAI Grok",
        ["openai_responses", "openai_chat"],
        "https://api.x.ai/v1",
        "https://docs.x.ai/developers/tools/overview",
        notes=[
            "Hosted web search and X search are separate tools.",
            "Reasoning and JSON Schema support depend on the model.",
        ],
    ),
    _preset(
        "custom",
        "Custom OpenAI-compatible endpoint",
        ["openai_chat", "openai_responses"],
        None,
        "https://ai.pydantic.dev/models/openai/",
        billing="unknown",
        notes=[
            "Enter an endpoint you trust that implements the OpenAI API.",
            "Compatibility does not prove model capabilities. Add a model manually when discovery is unsupported.",
        ],
    ),
    _preset(
        "codex",
        "ChatGPT subscription (Codex)",
        ["codex"],
        None,
        "https://developers.openai.com/codex/sdk/",
        kind="runtime",
        auth=["client_login"],
        billing="subscription",
        fields=["runtime_timeout_seconds", "max_turns"],
        notes=[
            "Uses your ChatGPT plan through the official Codex client. For API-key billing, use an OpenAI API connection.",
            "Install and sign in only through an explicit connection action.",
        ],
    ),
    _preset(
        "grok_build",
        "Grok subscription",
        ["grok_build"],
        None,
        "https://docs.x.ai/build/overview",
        kind="runtime",
        auth=["client_login"],
        billing="subscription",
        fields=["runtime_timeout_seconds", "max_turns", "allow_provider_managed_extras"],
        notes=[
            "Uses your eligible Grok subscription through the official Grok Build software.",
            "Subscription allowance only is the default. Extra credits and top-ups require an explicit account preference and Tender approval.",
            "This connection supports Quantix document tools. Native Grok web and X search are not connected yet.",
        ],
    ),
]


def provider_presets() -> list[dict]:
    """Return independent JSON dictionaries; callers cannot alter global presets."""
    return [ProviderPreset.model_validate(preset).model_dump(mode="json") for preset in _PRESETS]


def direct_provider_presets() -> list[dict]:
    """Return only the five providers supported by the bundled API runtime."""
    protocols = {
        "openai": ["openai_responses", "openai_chat"],
        "anthropic": ["anthropic"],
        "google": ["google"],
        "xai": ["openai_responses", "openai_chat"],
        "custom": ["openai_chat", "openai_responses"],
    }
    result = []
    for preset in provider_presets():
        identifier = preset["id"]
        if identifier not in protocols:
            continue
        value = dict(preset)
        value["protocols"] = protocols[identifier]
        value["default_protocol"] = protocols[identifier][0]
        value["auth_methods"] = ["api_key", "environment"]
        value["billing"] = "unknown" if identifier == "custom" else "metered"
        result.append(ProviderPreset.model_validate(value).model_dump(mode="json"))
    return result


def provider_preset(identifier: str) -> dict:
    for preset in provider_presets():
        if preset["id"] == identifier:
            return preset
    raise ValueError("Choose a supported provider or a custom endpoint.")


def default_base_url(provider_id: str, protocol: str) -> str | None:
    """Protocol changes never reuse a URL intended for another wire format."""
    return provider_preset(provider_id)["base_url"]


def catalog_price(provider_id: str, model_id: str) -> dict | None:
    """Conservative text/token price from bundled data, with no background updater.

    The largest published tier/time rate is used for reservations. Unknown models
    and deployment aliases have no implied price and need a manually entered card.
    """
    from genai_prices import __version__
    from genai_prices.data import providers
    from genai_prices.types import ModelPrice, TieredPrices

    from .ai_models import PriceCard

    if provider_id == "openai" and model_id == "gpt-6-astra":
        return PriceCard(
            input_per_million=25,
            output_per_million=75,
            cached_input_per_million=2,
            web_search_per_call=0.01,
            source="https://developers.openai.com/api/docs/models/gpt-6-astra; conservative Standard ceiling including long-context cache writes; not an invoice rate",
            as_of="2026-09-07",
        ).model_dump(mode="json")
    if provider_id == "google" and model_id == "gemini-3.8-flash":
        return PriceCard(
            input_per_million=1.5,
            output_per_million=7.5,
            cached_input_per_million=0.15,
            source="https://ai.google.dev/gemini-api/docs/pricing; 2027 list prices used as a conservative ceiling (Google charges half until 2026-12-31); not an invoice rate",
            as_of="2026-09-11",
        ).model_dump(mode="json")
    mapped = {"xai": "x-ai"}.get(provider_id, provider_id)
    if mapped in {
        "azure",
        "google_vertex",
        "bedrock",
        "custom",
        "ollama",
        "lmstudio",
        "openrouter",
    }:
        return None
    provider = next((item for item in providers if item.id == mapped), None)
    if provider is None:
        return None
    model = next((item for item in provider.models if item.id == model_id), None)
    # Direct setup must not assign a rate card to an unknown deployment alias
    # merely because the offline price library has a loose family matcher.
    # Provider discovery and explicit model entry retain the exact requested ID.
    if model is None:
        return None
    if model is None:
        return None
    prices = (
        [model.prices]
        if isinstance(model.prices, ModelPrice)
        else [item.prices for item in model.prices]
    )

    def highest(names):
        values = []
        for price in prices:
            for name in names:
                value = getattr(price, name, None)
                if isinstance(value, TieredPrices):
                    values.extend([value.base, *(tier.price for tier in value.tiers)])
                elif value is not None:
                    values.append(value)
        return float(max(values)) if values else None

    input_price = highest(
        ["input_mtok", "cache_write_mtok", "cache_write_1h_mtok", "cache_write_5m_mtok"]
    )
    output_price = highest(["output_mtok", "output_reasoning_mtok"])
    if input_price is None or output_price is None:
        return None
    search = highest(["web_searches_kcount"])
    return PriceCard(
        input_per_million=input_price,
        output_per_million=output_price,
        cached_input_per_million=highest(["cache_read_mtok"]),
        web_search_per_call=search / 1000 if search is not None else None,
        source=f"Bundled genai-prices {__version__}; conservative maximum tier/time rates. "
        + (
            provider.pricing_urls[0]
            if provider.pricing_urls
            else "https://github.com/pydantic/genai-prices"
        ),
        as_of=f"bundled genai-prices {__version__}",
    ).model_dump(mode="json")


def profile_capabilities(provider_id: str, protocol: str, model_id: str) -> dict:
    """What the bundled model-profile library establishes about this exact model.

    Pydantic AI maintains a profile per model family, including which models
    accept a reasoning effort and which can search the web. Using it keeps every
    provider working without Quantix keeping its own table of model names. Only
    what a profile confirms is returned; a profile default never denies a
    capability the provider itself reported.
    """

    if protocol not in {"openai_chat", "openai_responses"} or not model_id:
        return {}
    if provider_id == "custom":
        # A gateway serves a model under its own name and its own rules: it may
        # refuse an effort, or refuse thinking together with tools, however the
        # original model behaves. Only what this endpoint itself reports counts.
        return {}
    from pydantic_ai.native_tools import WebSearchTool
    from pydantic_ai.providers.litellm import LiteLLMProvider

    try:
        profile = LiteLLMProvider.model_profile(model_id) or {}
    except Exception:
        return {}
    result: dict = {}
    if profile.get("openai_supports_reasoning"):
        # Every OpenAI-compatible reasoning model takes these three. The extra
        # efforts belong to an exact model on its own vendor's endpoint: a
        # gateway serving the same family often refuses them.
        levels = ["low", "medium", "high"]
        if provider_id == "openai":
            if profile.get("openai_supports_minimal_reasoning_effort"):
                levels.insert(0, "minimal")
            if profile.get("openai_supports_reasoning_effort_none"):
                levels.insert(0, "none")
        result["reasoning"] = levels
    # The chat flag is a per-model fact. On the Responses API every profile
    # carries the hosted tools by default, so that only means anything for
    # OpenAI's own endpoint; other endpoints must report their own search.
    searches = profile.get("openai_chat_supports_web_search") or (
        provider_id == "openai"
        and protocol == "openai_responses"
        and WebSearchTool in (profile.get("supported_native_tools") or ())
    )
    if searches:
        result["web_search"] = True
    window = profile.get("context_window")
    if isinstance(window, int) and window > 0:
        result["context_window"] = window
    return result


def documented_capabilities(provider_id: str, protocol: str, model_id: str) -> dict:
    """Only exact, source-confirmed models have initial capability metadata."""
    if protocol == "grok_build":
        # This adapter's scoped tool surface does not wire Grok-native search.
        return {"web_search": False}
    if provider_id == "openai" and model_id == "gpt-6-astra":
        return {
            "tools": True,
            "structured_output": True,
            "images": True,
            "web_search": protocol == "openai_responses",
            "reasoning": ["low", "medium", "high", "xhigh", "max"],
            "context_window": 1050000,
            "max_output_tokens": 128000,
        }
    if provider_id == "google" and model_id == "gemini-3.8-flash":
        # https://ai.google.dev/gemini-api/docs/models/gemini-3.8-flash, checked 2026-09-11.
        return {
            "tools": True,
            "structured_output": True,
            "images": True,
            "context_window": 1048576,
            "max_output_tokens": 65536,
        }
    return profile_capabilities(provider_id, protocol, model_id)
