"""Capability validation. Preferences never establish source or spending authority."""
from __future__ import annotations

import hashlib
import json

from .ai_generation_models import CapabilityDescriptor, GenerationPreview, GenerationSettings

DIRECT_PROTOCOLS = {"openai_chat", "openai_responses", "anthropic", "google"}
SDK_EVIDENCE = "Bundled pydantic-ai-slim 2.40.0 model profile and adapter; inspected 2026-09-12"
XAI_SEARCH_DOC = "https://docs.x.ai/developers/tools/web-search"
XAI_LIMIT_DOC = "https://docs.x.ai/developers/tools/tool-usage-details"


def bounded_native_call_limit(route: dict, connection: dict) -> int | None:
    """OpenAI enforces one server-side ceiling on hosted search calls per request."""
    if connection.get("provider_id") != "openai" or connection.get("protocol") != "openai_responses":
        return None
    if not route.get("web_search"):
        return None
    return settings_from_route(route).max_search_calls


def settings_from_route(route: dict) -> GenerationSettings:
    values = {key: value for key, value in route.items() if key in GenerationSettings.model_fields}
    # Internal no-tool classification/check paths historically use zero to
    # denote disabled search. Public route/settings inputs still require 1–20.
    if values.get("max_search_calls") == 0 and not route.get("web_search"):
        values.pop("max_search_calls")
    return GenerationSettings.model_validate(values)


def descriptors(connection: dict) -> list[CapabilityDescriptor]:
    model = connection.get("_model") or {}
    recorded = model.get("capabilities") or {}
    protocol = connection.get("protocol")
    direct = protocol in DIRECT_PROTOCOLS
    trusted = model.get("source") in {"provider", "catalog"}
    documented = {}
    if connection.get("provider_id") == "openai" and model.get("model_id") == "gpt-6-astra":
        # Exact model page checked 2026-09-12; a price-only manual edit does not
        # erase independently documented capabilities.
        documented = {"native_output": True, "streaming": True}
    values = []
    for name in ("temperature", "top_p", "native_output", "web_search"):
        value = recorded.get("structured_output" if name == "native_output" else name)
        # Existing search checks/grants keep their meaning. New observations
        # must come from discovery/documented catalog, not a profile assertion.
        if name != "web_search" and not trusted and value is not False:
            value = None
        if name in documented and recorded.get(name) is not False:
            value = documented[name]
        runtime = direct
        requirements = []
        evidence = [SDK_EVIDENCE]
        restriction = None
        if name in documented:
            evidence.append("https://developers.openai.com/api/docs/models/gpt-6-astra")
        if name == "web_search":
            runtime = protocol == "anthropic" or (
                protocol == "openai_responses" and connection.get("provider_id") == "openai"
            )
            requirements = ["Reviewed online research permission and search spending allowance."]
            if protocol == "google":
                restriction = "Google Search is a native model feature, but this adapter cannot enforce the reviewed per-request call limit."
                evidence.append("https://ai.google.dev/gemini-api/docs/google-search")
            elif connection.get("provider_id") == "xai":
                restriction = ("xAI web search includes page browsing, but this adapter cannot enforce the reviewed individual-call limit. "
                               "A turn limit can include parallel calls.")
                evidence.extend([XAI_SEARCH_DOC, XAI_LIMIT_DOC])
            elif not runtime:
                restriction = "This adapter has no documented enforceable native-search call limit."
        support = "supported" if value is True else "unsupported" if value is False else "unknown"
        if restriction:
            detail = restriction
        elif not runtime:
            detail = f"{name.replace('_', ' ').capitalize()} is unavailable through this execution adapter."
        elif value is None:
            detail = "Support has not been established for this exact model."
        else:
            detail = "Recorded model support; request combinations and work permission are checked separately."
        values.append(CapabilityDescriptor(id=name, origin="provider" if direct else "client",
                      support=support, runtime_supported=runtime, detail=detail,
                      requirements=requirements, evidence=evidence))
    values.append(CapabilityDescriptor(id="validated_output", origin="quantix", support="supported",
                  runtime_supported=True, detail="Quantix validates every structured result before publication."))
    if protocol == "codex":
        values.append(CapabilityDescriptor(id="output_token_limit", origin="client", support="unsupported",
                      runtime_supported=False, detail="Codex does not expose a hard output-token limit. This saved value is a local allowance only; choose a direct API for a provider-enforced ceiling."))
    streaming = recorded.get("streaming", documented.get("streaming"))
    values.append(CapabilityDescriptor(id="streaming", origin="provider" if direct else "client",
                  support="supported" if streaming is True else "unsupported" if streaming is False else "unknown",
                  runtime_supported=True, detail="An explicitly nonstreaming model uses completed responses; no artificial deltas are generated."))
    return values


def validate_generation(route: dict, connection: dict) -> GenerationSettings:
    selected = settings_from_route(route)
    available = {item.id: item for item in descriptors(connection)}
    requested = [name for name in ("temperature", "top_p") if getattr(selected, name) is not None]
    if route.get("web_search"):
        requested.append("web_search")
    if selected.output_mode == "native":
        requested.append("native_output")
    for name in requested:
        item = available[name]
        if not item.runtime_supported or item.support != "supported":
            requirements = " ".join(item.requirements)
            raise ValueError(f"Requested {name} cannot be applied. {item.detail} {requirements}".strip())
    protocol = connection.get("protocol")
    if protocol not in DIRECT_PROTOCOLS and selected.output_mode != "auto":
        raise ValueError("This original client does not expose the requested output mode.")
    if protocol == "anthropic":
        if selected.temperature is not None and selected.temperature > 1:
            raise ValueError("Anthropic temperature must be between 0 and 1.")
        if selected.temperature is not None and selected.top_p is not None:
            raise ValueError("Choose temperature or top_p for Anthropic, not both.")
        if selected.reasoning not in {None, "default", "disabled"} and selected.temperature is not None:
            raise ValueError("Explicit temperature cannot be combined with Anthropic thinking.")
    limit = (connection.get("_model") or {}).get("capabilities", {}).get("max_output_tokens")
    if limit and selected.max_output_tokens > limit:
        raise ValueError("The output limit exceeds this model's recorded limit.")
    return selected


def validate_sdk_settings(settings: dict, profile: dict) -> None:
    """Fail before Pydantic AI's intentional parameter-dropping paths."""
    sampling = [name for name in ("temperature", "top_p") if name in settings]
    if not sampling:
        return
    unsupported = profile.get("openai_unsupported_model_settings", ())
    if any(name in unsupported for name in sampling) or profile.get("anthropic_disallows_sampling_settings"):
        raise ValueError("This exact model does not accept the requested sampling settings.")
    if profile.get("openai_supports_reasoning"):
        effort = settings.get("openai_reasoning_effort")
        active = effort != "none" if effort is not None else profile.get("openai_reasoning_enabled_by_default", False)
        if not profile.get("openai_supports_reasoning_effort_none") or active:
            raise ValueError("This model cannot apply temperature or top_p with the selected reasoning setting.")


def native_tools_for(route: dict, connection: dict) -> list:
    from pydantic_ai.native_tools import WebSearchTool

    selected = validate_generation(route, connection)
    if not route.get("web_search"):
        return []
    if (connection.get("_model") or {}).get("capabilities", {}).get("web_search") is not True:
        raise ValueError("Hosted web search is not established for this model.")
    return [WebSearchTool(max_uses=selected.max_search_calls)]


def output_for(settings: GenerationSettings, output_type, *, retries=None):
    from pydantic_ai.output import NativeOutput, PromptedOutput, StructuredDict, ToolOutput

    schema = StructuredDict(output_type.model_json_schema())
    if settings.output_mode == "native":
        return NativeOutput(schema)
    if settings.output_mode == "prompted":
        return PromptedOutput(schema)
    return ToolOutput(schema, max_retries=retries)


def generation_preview(connection: dict, route: dict) -> GenerationPreview:
    selected = settings_from_route(route)
    capabilities = descriptors(connection)
    native_limit = bounded_native_call_limit(route, connection)
    if native_limit is not None:
        capabilities.append(CapabilityDescriptor(id="native_call_limit", origin="provider", support="supported",
            runtime_supported=True,
            detail=f"OpenAI allows at most {native_limit} hosted search calls per request.",
            evidence=["https://developers.openai.com/api/reference/resources/responses/methods/create"]))
    blockers = []
    effective = None
    try:
        effective = validate_generation(route, connection)
        if connection.get("protocol") in DIRECT_PROTOCOLS:
            from .ai_api_provider import _model_profile, build_model_settings
            model_id = (connection.get("_model") or {}).get("model_id", "")
            profile = _model_profile(connection["provider_id"], connection["protocol"],
                                     {**route, "model_id": model_id}, model_id) or {}
            validate_sdk_settings(build_model_settings(route, connection), profile)
            if selected.output_mode == "native" and not profile.get("supports_json_schema_output"):
                raise ValueError("This exact model adapter does not support native output with JSON Schema.")
            if connection["protocol"] == "google" and selected.output_mode == "native" and not profile.get("google_supports_tool_combination"):
                raise ValueError("This model cannot combine native output with office tools.")
    except ValueError as error:
        blockers.append(str(error))
        effective = None
    material = {"connection_revision": connection.get("revision", 0),
                "model": connection.get("_model"), "capabilities": [item.model_dump() for item in capabilities]}
    revision = hashlib.sha256(json.dumps(material, sort_keys=True, default=str).encode()).hexdigest()
    return GenerationPreview(connection_id=connection.get("id", ""),
        connection_revision=connection.get("revision", 0), model_id=(connection.get("_model") or {}).get("model_id", ""),
        revision=revision, requested=selected, effective=effective, capabilities=capabilities, blockers=blockers)
