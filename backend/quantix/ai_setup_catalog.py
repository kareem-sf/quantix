"""Human-facing service choices backed by the existing explicit provider catalog."""

from .ai_catalog import default_base_url, direct_provider_presets, provider_preset, provider_presets
from .ai_setup_models import SetupMethod, SetupService

_GROUPS = {
    "xai": ("xai", "xAI", "Choose an xAI subscription or API key."),
    "openai": ("openai", "OpenAI", "Choose ChatGPT/Codex subscription access or an OpenAI API key."),
    "anthropic": ("anthropic", "Anthropic", "Connect an Anthropic API key."),
    "google": ("google", "Google", "Connect a Google Gemini API key."),
    "custom": ("custom", "Custom", "Connect a company gateway or another supported AI server."),
}
_METHODS = {
    "xai": ("Use an xAI API key", "Uses your xAI API account and its billing settings."),
    "openai": ("Use an OpenAI API key", "OpenAI charges separately for this usage; it is not included in a ChatGPT subscription."),
    "anthropic": ("Use a Claude API key", "Anthropic charges separately for API usage."),
    "google": ("Use a Gemini API key", "Uses your Google AI account; charges depend on its billing plan."),
    "custom": ("Connect another AI service", "Enter the address and access details supplied by that service."),
}


def setup_services():
    groups = {}
    for preset in direct_provider_presets():
        identifier = preset["id"]
        if identifier not in _GROUPS:
            continue
        group_id, title, detail = _GROUPS.get(identifier, (identifier, preset["name"], "Connect your account for this AI service."))
        group = groups.setdefault(group_id, {"id": group_id, "title": title, "detail": detail,
                     "featured": group_id in {"openai", "anthropic", "google", "xai"}, "methods": []})
        label, method_detail = _METHODS.get(identifier, ("Use my access key", "Uses your account with this provider. Separately billed usage needs a Tender allowance."))
        needs_details = identifier in {"custom", "alibaba", "azure", "bedrock", "google_vertex", "openrouter"}
        group["methods"].append(SetupMethod(id=identifier, title=label, detail=method_detail,
            provider_id=identifier, requires_key=preset["auth_methods"][0] == "api_key",
            requires_details=needs_details, billing=preset["billing"], access_kind="api_key", available=True,
            docs_url=preset["docs_url"]).model_dump())
    subscription_methods = {
        "openai": SetupMethod(
            id="codex", title="Use my ChatGPT subscription",
            detail="Uses the official Codex client and your ChatGPT subscription settings. API billing stays separate.",
            provider_id="codex", requires_key=False, requires_details=False, billing="subscription",
            access_kind="subscription", available=True,
            docs_url="https://learn.chatgpt.com/docs/codex-sdk",
        ).model_dump(),
        "xai": SetupMethod(
            id="grok_build", title="Use my Grok subscription",
            detail="Uses official Grok Build with the subscription account that you sign in to.",
            provider_id="grok_build", requires_key=False, requires_details=False, billing="subscription",
            access_kind="subscription", available=True,
            docs_url="https://docs.x.ai/build/overview",
        ).model_dump(),
    }
    for group_id, method in subscription_methods.items():
        groups[group_id]["methods"].append(method)
    groups["openai"]["subscription_note"] = "ChatGPT subscription access is provided through the original Codex client; it is separate from OpenAI API billing."
    groups["openai"]["subscription_docs_url"] = "https://learn.chatgpt.com/docs/codex-sdk"
    groups["xai"]["subscription_note"] = "Grok subscription access is provided through official Grok Build; any provider extras remain subject to its account settings."
    groups["xai"]["subscription_docs_url"] = "https://docs.x.ai/build/overview"
    groups["anthropic"]["subscription_note"] = "Claude subscription access is not offered here; use an Anthropic API key for Tender work."
    groups["anthropic"]["subscription_docs_url"] = "https://code.claude.com/docs/en/legal-and-compliance"
    groups["google"]["subscription_note"] = "Google subscription access is not offered here; use a Gemini API key. The documented consumer CLI transition and service terms do not establish a supported Quantix route."
    groups["google"]["subscription_docs_url"] = "https://developers.googleblog.com/an-important-update-transitioning-gemini-cli-to-antigravity-cli/"
    groups["custom"]["subscription_note"] = "Custom connections require explicit API credentials; subscription tokens are not accepted."
    order = {"openai": 0, "anthropic": 1, "google": 2, "xai": 3, "custom": 4}
    for group in groups.values():
        group["methods"].sort(key=lambda method: (0 if method["access_kind"] == "subscription" else 1, method["id"]))
    return [SetupService.model_validate(value).model_dump() for value in sorted(groups.values(), key=lambda item: (order.get(item["id"], 10), item["title"]))]


def setup_method(service_id, method_id):
    service = next((s for s in setup_services() if s["id"] == service_id), None)
    method = next((m for m in service["methods"] if m["id"] == method_id), None) if service else None
    if not method or not method["available"]:
        raise ValueError("Choose an available way to connect this AI service.")
    return service, method


def service_for_connection(connection):
    for service in setup_services():
        for method in service["methods"]:
            if method["provider_id"] == connection["provider_id"]:
                return service, method
    preset = next((p for p in provider_presets() if p["id"] == connection["provider_id"]), None)
    if preset is None:
        raise ValueError("This saved AI service is not in the current catalog.")
    method = SetupMethod(
        id=preset["id"],
        title=preset["name"],
        detail="This saved AI method is retained for review but is unavailable for new direct API setup.",
        provider_id=preset["id"],
        requires_key=False,
        requires_details=False,
        billing=preset["billing"],
        available=False,
        unavailable_reason="This saved AI method is retained for review but is unavailable for current setup. Choose one of the five supported provider routes.",
        docs_url=preset["docs_url"],
    ).model_dump()
    service = SetupService(
        id="unsupported",
        title=preset["name"],
        detail="This saved AI method is retained for review.",
        featured=False,
        methods=[method],
    ).model_dump()
    return service, method


def default_connection(method, name):
    preset = provider_preset(method["provider_id"])
    auth = preset["auth_methods"][0]
    return {"name": name, "provider_id": preset["id"], "protocol": preset["default_protocol"],
            "auth_type": auth, "billing": preset["billing"], "settings": {},
            "base_url": default_base_url(preset["id"], preset["default_protocol"])}
