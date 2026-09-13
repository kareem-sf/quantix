"""Documented connection presets; model capabilities are never inferred from a brand."""

from .ai_models import ProviderPreset


def _preset(identifier, name, protocols, base_url, docs_url, *, auth=None,
            billing="metered", fields=(), notes=(), kind="api"):
    return ProviderPreset(
        id=identifier, name=name, kind=kind, protocols=protocols,
        default_protocol=protocols[0], base_url=base_url,
        auth_methods=auth or ["api_key", "environment"], billing=billing,
        docs_url=docs_url, settings_fields=list(fields), notes=list(notes),
    ).model_dump(mode="json")


_PRESETS = [
    _preset("openai", "OpenAI", ["openai_responses", "openai_chat"],
            "https://api.openai.com/v1", "https://developers.openai.com/api/docs",
            notes=["API usage has separate billing from a ChatGPT subscription.",
                   "Model names from the provider do not establish tool or image support."]),
    _preset("anthropic", "Anthropic Claude", ["anthropic"],
            "https://api.anthropic.com", "https://platform.claude.com/docs/en/api/overview",
            notes=["Document citations and strict JSON output cannot be combined in one request.",
                   "Native search availability depends on the model and hosting platform."]),
    _preset("google", "Google Gemini API", ["google", "openai_chat"], None,
            "https://ai.google.dev/gemini-api/docs/api-key",
            notes=["Use the current API key instructions in Google AI Studio.",
                   "Unpaid and billing-enabled projects have different data terms.",
                   "Google Search has separate billing and retention rules."]),
    _preset("google_vertex", "Google Cloud / Vertex AI", ["google"], None,
            "https://docs.cloud.google.com/gemini-enterprise-agent-platform/models/start",
            auth=["google_identity", "api_key", "environment"], fields=["project", "location"],
            notes=["Use your Google Cloud project and location. Application Default Credentials are supported.",
                   "A global location may process requests outside the resource region."]),
    _preset("xai", "xAI Grok", ["openai_responses", "openai_chat"],
            "https://api.x.ai/v1", "https://docs.x.ai/developers/tools/overview",
            notes=["Hosted web search and X search are separate tools.",
                   "Reasoning and JSON Schema support depend on the model."]),
    _preset("deepseek", "DeepSeek", ["openai_responses", "openai_chat", "anthropic"],
            "https://api.deepseek.com", "https://api-docs.deepseek.com/guides/responses_api/",
            notes=["Responses is stateless. Some unsupported parameters are silently ignored by the provider.",
                   "Only vision models read image input. Prices may vary by time and cache usage."]),
    _preset("mistral", "Mistral", ["mistral", "openai_chat"],
            "https://api.mistral.ai", "https://docs.mistral.ai/api/endpoint/models",
            notes=["Hosted search uses the separate Conversations API and is unavailable through this chat connection.",
                   "Vision and structured output must be selected for the actual model."]),
    _preset("kimi", "Moonshot Kimi", ["openai_chat", "openai_responses", "anthropic"],
            "https://api.moonshot.ai/v1", "https://platform.kimi.ai/docs/overview",
            notes=["JSON Schema support differs by model.",
                   "Kimi search has a provider-specific continuation format; generic chat does not enable it."]),
    _preset("alibaba", "Alibaba Qwen / Model Studio", ["openai_chat", "openai_responses", "anthropic"], None,
            "https://www.alibabacloud.com/help/en/model-studio/get-api-key",
            fields=["region", "billing_product"],
            notes=["Copy the exact API Host for your region, workspace and selected protocol.",
                   "Pay-as-you-go, Token Plan and Coding Plan use separate keys and endpoints.",
                   "JSON Schema, thinking and image support differ by model. Storage and inference regions may differ."]),
    _preset("zai", "Z.ai GLM", ["openai_chat"], "https://api.z.ai/api/paas/v4",
            "https://docs.z.ai/api-reference/introduction", fields=["billing_product"],
            notes=["The general API is separate from the Coding Plan.",
                   "The published chat reference documents JSON mode; strict JSON Schema is not assumed."]),
    _preset("minimax", "MiniMax", ["anthropic", "openai_chat", "openai_responses"],
            "https://api.minimax.io/anthropic", "https://platform.minimax.io/docs/guides/text-generation",
            fields=["billing_product"],
            notes=["The API key and endpoint must match your billing product.",
                   "Tool conversations must preserve reasoning continuation data. Hosted search is beta."]),
    _preset("cohere", "Cohere", ["cohere", "openai_chat"], None,
            "https://docs.cohere.com/docs/compatibility-api",
            notes=["Native document citations are unavailable in the OpenAI compatibility API.",
                   "Native response formatting cannot be combined with tools or documents in all models."]),
    _preset("openrouter", "OpenRouter", ["openai_chat", "openai_responses"],
            "https://openrouter.ai/api/v1", "https://openrouter.ai/docs/guides/routing/provider-selection",
            fields=["upstream_providers", "allow_fallbacks", "data_collection", "zdr"],
            notes=["Choose approved upstream providers. Parameter support is required and upstream fallback is off by default.",
                   "Prompts pass through OpenRouter and the selected upstream. BYOK does not remove this routing.",
                   "Search can add another service and separate charges."]),
    _preset("bedrock", "Amazon Bedrock", ["bedrock", "openai_responses", "openai_chat", "anthropic"], None,
            "https://docs.aws.amazon.com/bedrock/latest/userguide/apis.html",
            auth=["aws_identity", "api_key", "environment"], fields=["region", "aws_profile"],
            notes=["Choose the model or inference profile and AWS region. Native access uses AWS credentials.",
                   "Compatible endpoints require their exact runtime or mantle URL and supported authentication.",
                   "Hosted tools and data retention differ by endpoint, model and account policy."]),
    _preset("azure", "Azure / Microsoft Foundry", ["openai_responses", "openai_chat", "anthropic"], None,
            "https://learn.microsoft.com/en-us/azure/foundry/foundry-models/concepts/endpoints",
            auth=["api_key", "environment", "azure_identity"], fields=["region", "tenant_id", "client_id"],
            notes=["Use your resource endpoint and the deployment name as the model ID.",
                   "OpenAI endpoints end in /openai/v1/. Responses availability depends on the deployment.",
                   "Global and DataZone deployments can process outside the resource region."]),
    _preset("ollama", "Ollama", ["openai_chat", "openai_responses"],
            "http://127.0.0.1:11434/v1", "https://docs.ollama.com/api/openai-compatibility",
            auth=["none", "api_key", "environment"], billing="local",
            notes=["Capabilities depend on the installed model and actual runtime context setting.",
                   "Responses is stateless. Disable Ollama cloud features when local-only work is required."]),
    _preset("lmstudio", "LM Studio", ["openai_chat", "openai_responses", "anthropic"],
            "http://127.0.0.1:1234/v1", "https://lmstudio.ai/docs/developer/openai-compat",
            auth=["none", "api_key", "environment"], billing="local",
            notes=["Capabilities depend on the loaded model. API token authentication is optional in LM Studio.",
                   "Remote MCP tools and remote models are separate data destinations."]),
    _preset("custom", "Custom API endpoint", ["openai_chat", "openai_responses", "anthropic", "google", "mistral", "cohere"], None,
            "https://ai.pydantic.dev/models/overview/", auth=["api_key", "environment", "none"], billing="unknown",
            notes=["Enter an endpoint you trust and its documented protocol.",
                   "Compatibility does not prove model capabilities. Add a model manually when discovery is unsupported."]),
]

_PRESETS.append(_preset(
    "grok_build", "Grok subscription", ["grok_build"], None,
    "https://docs.x.ai/build/overview", kind="runtime", auth=["client_login"], billing="subscription",
    fields=["runtime_timeout_seconds", "max_turns", "allow_provider_managed_extras"],
    notes=["Uses your eligible Grok subscription through the official Grok Build software.",
           "Subscription allowance only is the default. Extra credits and top-ups require an explicit account preference and Tender approval.",
           "This connection supports Quantix document tools. Native Grok web and X search are not connected yet."],
))

for identifier, name, docs in [
    ("codex", "OpenAI Codex client", "https://developers.openai.com/codex/sdk/"),
    ("copilot", "GitHub Copilot client", "https://github.com/github/copilot-sdk"),
    ("gemini_cli", "Google Gemini CLI", "https://geminicli.com/docs/"),
    ("claude_agent", "Claude Agent SDK", "https://platform.claude.com/docs/en/agent-sdk/overview"),
    ("claude_code", "Claude Code attended handoff", "https://code.claude.com/docs/en/overview"),
]:
    authentication = ["api_key", "environment"] if identifier == "claude_agent" else (
        ["client_login", "api_key", "environment"] if identifier == "copilot" else ["client_login"])
    access_note = {
        "codex": "Managed ChatGPT access runs through the original Codex runtime. For API-key billing, use an OpenAI API connection.",
        "copilot": "Original sign-in or a supported GitHub token uses the account's Copilot entitlement; a token is not a model-provider API key.",
        "gemini_cli": "Original Google subscription sign-in is supported. For metered access, use a Gemini API or Google Cloud connection.",
        "claude_agent": "Background Tender work uses your Anthropic API key and metered billing. A Claude subscription is not an API key.",
        "claude_code": "Attended original-client access only. This connection cannot execute background Tender work.",
    }[identifier]
    _PRESETS.append(_preset(
        identifier, name, [identifier], None, docs, kind="runtime", billing="metered" if identifier == "claude_agent" else "subscription",
        auth=authentication,
        fields=["executable_path", "node_path", "terminal_path", "runtime_timeout_seconds", "max_turns"],
        notes=[access_note, "Install and sign in only through an explicit connection action.",
               "The client's supported permissions, output and billing determine available work."],
    ))


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
    overrides = {
        ("google", "openai_chat"): "https://generativelanguage.googleapis.com/v1beta/openai/",
        ("mistral", "openai_chat"): "https://api.mistral.ai/v1",
        ("deepseek", "anthropic"): "https://api.deepseek.com/anthropic",
        ("kimi", "anthropic"): "https://api.moonshot.ai/anthropic",
        ("minimax", "openai_chat"): "https://api.minimax.io/v1",
        ("minimax", "openai_responses"): "https://api.minimax.io/v1",
        ("cohere", "openai_chat"): "https://api.cohere.ai/compatibility/v1",
        ("lmstudio", "anthropic"): "http://127.0.0.1:1234",
    }
    return overrides.get((provider_id, protocol), provider_preset(provider_id)["base_url"])


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
        return PriceCard(input_per_million=25, output_per_million=75,
                         cached_input_per_million=2, web_search_per_call=0.01,
                         source="https://developers.openai.com/api/docs/models/gpt-6-astra; conservative Standard ceiling including long-context cache writes; not an invoice rate",
                         as_of="2026-09-07").model_dump(mode="json")
    if provider_id == "google" and model_id == "gemini-3.8-flash":
        return PriceCard(
            input_per_million=1.5,
            output_per_million=7.5,
            cached_input_per_million=0.15,
            source="https://ai.google.dev/gemini-api/docs/pricing; 2027 list prices used as a conservative ceiling (Google charges half until 2026-12-31); not an invoice rate",
            as_of="2026-09-11",
        ).model_dump(mode="json")
    mapped = {"xai": "x-ai"}.get(provider_id, provider_id)
    if mapped in {"azure", "google_vertex", "bedrock", "custom", "ollama", "lmstudio", "openrouter"}:
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
    prices = [model.prices] if isinstance(model.prices, ModelPrice) else [item.prices for item in model.prices]

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

    input_price = highest(["input_mtok", "cache_write_mtok", "cache_write_1h_mtok", "cache_write_5m_mtok"])
    output_price = highest(["output_mtok", "output_reasoning_mtok"])
    if input_price is None or output_price is None:
        return None
    search = highest(["web_searches_kcount"])
    return PriceCard(
        input_per_million=input_price, output_per_million=output_price,
        cached_input_per_million=highest(["cache_read_mtok"]),
        web_search_per_call=search / 1000 if search is not None else None,
        source=f"Bundled genai-prices {__version__}; conservative maximum tier/time rates. "
               + (provider.pricing_urls[0] if provider.pricing_urls else "https://github.com/pydantic/genai-prices"),
        as_of=f"bundled genai-prices {__version__}",
    ).model_dump(mode="json")


def documented_capabilities(provider_id: str, protocol: str, model_id: str) -> dict:
    """Only exact, source-confirmed models have initial capability metadata."""
    if protocol == "grok_build":
        # This adapter's scoped tool surface does not wire Grok-native search.
        return {"web_search": False}
    if provider_id == "openai" and model_id == "gpt-6-astra":
        return {"tools": True, "structured_output": True, "images": True,
                "web_search": protocol == "openai_responses", "reasoning": ["low", "medium", "high", "xhigh", "max"],
                "context_window": 1050000, "max_output_tokens": 128000}
    if provider_id == "google" and model_id == "gemini-3.8-flash":
        # https://ai.google.dev/gemini-api/docs/models/gemini-3.8-flash, checked 2026-09-11.
        return {"tools": True, "structured_output": True, "images": True,
                "context_window": 1048576, "max_output_tokens": 65536}
    return {}
