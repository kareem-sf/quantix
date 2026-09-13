"""Capability validation. Preferences never establish source or spending authority."""
from __future__ import annotations

import hashlib
import json
import re

from .ai_generation_models import (
    CapabilityDescriptor,
    GenerationPreview,
    GenerationSettings,
    NativeToolGrant,
)

DIRECT_PROTOCOLS = {"openai_chat", "openai_responses", "anthropic", "google"}
SDK_EVIDENCE = "Bundled pydantic-ai-slim 2.40.0 model profile and adapter; inspected 2026-09-12"
FETCH_DOC = "https://platform.claude.com/docs/en/agents-and-tools/tool-use/web-fetch-tool"
GOOGLE_FETCH_DOC = "https://ai.google.dev/gemini-api/docs/url-context"
XAI_SEARCH_DOC = "https://docs.x.ai/developers/tools/web-search"
XAI_LIMIT_DOC = "https://docs.x.ai/developers/tools/tool-usage-details"


def bounded_native_call_limit(route: dict, connection: dict) -> int | None:
    """OpenAI's one server-side ceiling covers all selected native tools."""
    if connection.get("provider_id") != "openai" or connection.get("protocol") != "openai_responses":
        return None
    names = route.get("native_tools") or []
    search = route.get("web_search") or "web_search" in names
    if not search and "code_execution" not in names:
        return None
    selected = settings_from_route(route)
    return min(selected.max_native_tool_calls, selected.max_search_calls) if search else selected.max_native_tool_calls


def settings_from_route(route: dict) -> GenerationSettings:
    values = {key: value for key, value in route.items() if key in GenerationSettings.model_fields}
    # Internal no-tool classification/check paths historically use zero to
    # denote disabled search. Public route/settings inputs still require 1–20.
    if values.get("max_search_calls") == 0 and not route.get("web_search") and "web_search" not in values.get("native_tools", []):
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
        documented = {"native_output": True, "code_execution": protocol == "openai_responses", "streaming": True}
    values = []
    for name in ("temperature", "top_p", "native_output", "web_search", "web_fetch", "code_execution"):
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
                restriction = ("Google Search is a native model feature, but this adapter cannot enforce the reviewed per-request call limit. "
                               "Use the shared public URL reader, or explicitly review a bounded research route on another supported account.")
                evidence.append("https://ai.google.dev/gemini-api/docs/google-search")
            elif connection.get("provider_id") == "xai":
                restriction = ("xAI web search includes page browsing, but this adapter cannot enforce the reviewed individual-call limit. "
                               "A turn limit can include parallel calls. Use the shared public URL reader or an explicitly reviewed bounded research route.")
                evidence.extend([XAI_SEARCH_DOC, XAI_LIMIT_DOC])
            elif not runtime:
                restriction = "This adapter has no documented enforceable native-search call limit. Use the shared public URL reader or explicitly review a bounded research route."
        elif name == "web_fetch":
            runtime = protocol == "anthropic"
            requirements = ["Reviewed public domain scope and a recorded model context window."]
            if protocol == "google":
                restriction = ("Google URL Context is available in the SDK, but its adapter does not apply allowed domains, fetch-call limits or content-token limits. "
                               "Quantix therefore uses its separately approved public URL reader for controlled page access.")
                evidence.append(GOOGLE_FETCH_DOC)
            elif connection.get("provider_id") == "xai":
                restriction = "xAI page browsing belongs to native web search, whose individual-call limit is not enforceable here. Use the shared public URL reader for exact saved passages."
                evidence.extend([XAI_SEARCH_DOC, XAI_LIMIT_DOC])
            else:
                evidence.append(FETCH_DOC)
        elif name == "code_execution":
            runtime = (protocol == "openai_responses" and connection.get("provider_id") == "openai"
                       and (not connection.get("base_url") or connection["base_url"].rstrip("/") == "https://api.openai.com/v1"))
            requirements = ["Reviewed hosted-code destination, exact uploaded sources and spending allowance.",
                            "A dated session rate and a shared root charge reservation are required; other adapters remain unavailable."]
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
    values.append(CapabilityDescriptor(id="fetch_public_url", origin="quantix", support="supported",
                  runtime_supported=True, detail="The shared public URL reader checks public destinations, bounds response bytes and saves exact passages under the shared root allowance.",
                  requirements=["Select the fetch_public_url tool in the reviewed work scope. Each fetch needs remaining research allowance; saved passages must be cited separately."]))
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
    requested.extend(selected.native_tools)
    if route.get("web_search") and "web_search" not in requested:
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
        if "web_fetch" in selected.native_tools and (
            route.get("web_search") or "web_search" in selected.native_tools
        ):
            raise ValueError("Anthropic exposes separate native search and fetch limits, not one enforceable combined limit. Select one native tool and use the separately reviewed Quantix public URL reader for page retrieval.")
        if selected.temperature is not None and selected.temperature > 1:
            raise ValueError("Anthropic temperature must be between 0 and 1.")
        if selected.temperature is not None and selected.top_p is not None:
            raise ValueError("Choose temperature or top_p for Anthropic, not both.")
        if selected.reasoning not in {None, "default", "disabled"} and selected.temperature is not None:
            raise ValueError("Explicit temperature cannot be combined with Anthropic thinking.")
        if selected.output_mode == "native" and "web_fetch" in selected.native_tools:
            raise ValueError("Anthropic web citations cannot be combined with native output.")
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


def native_tools_for(route: dict, connection: dict, *, context=None) -> list:
    from pydantic_ai.native_tools import CodeExecutionTool, WebFetchTool, WebSearchTool

    selected = validate_generation(route, connection)
    names = set(selected.native_tools)
    if route.get("web_search"):
        names.add("web_search")
    tools = []
    if "web_search" in names:
        if (connection.get("_model") or {}).get("capabilities", {}).get("web_search") is not True:
            raise ValueError("Hosted web search is not established for this model.")
        tools.append(WebSearchTool(max_uses=selected.max_search_calls))
    if "web_fetch" in names:
        raw_grant = connection.get("_native_tool_grant")
        if not raw_grant:
            raise ValueError("Review the allowed public domain scope and native-tool grant before provider web fetch.")
        try:
            grant = NativeToolGrant.model_validate(raw_grant)
        except ValueError:
            raise ValueError("The native-tool grant is incomplete. Review this work again.") from None
        if context is None or any((
            grant.tender_id != getattr(context, "tender_id", None),
            grant.run_id != getattr(context, "run_id", None),
            grant.connection_id != connection.get("id"),
            grant.connection_revision != connection.get("revision"),
            grant.model_id != (connection.get("_model") or {}).get("model_id"),
            "web_fetch" not in grant.native_tools,
            selected.max_native_tool_calls > grant.max_calls_per_request,
        )):
            raise ValueError("The native-tool grant does not match this request. Review the work again.")
        domains = grant.web_fetch_domains
        if not isinstance(domains, list) or not domains or len(domains) > 30 or any(
            not isinstance(domain, str) or not re.fullmatch(r"[a-zA-Z0-9](?:[a-zA-Z0-9.-]{0,251}[a-zA-Z0-9])?", domain)
            or "." not in domain for domain in domains
        ):
            raise ValueError("Review the allowed public domain scope before provider web fetch.")
        tools.append(WebFetchTool(max_uses=selected.max_native_tool_calls, allowed_domains=domains,
                                 max_content_tokens=selected.max_output_tokens, enable_citations=True))
    if "code_execution" in names:
        try:
            grant = NativeToolGrant.model_validate(connection.get("_native_tool_grant"))
        except ValueError:
            raise ValueError("A current hosted-code upload and spending grant is required.") from None
        if context is None or any((grant.tender_id != getattr(context, "tender_id", None),
            grant.run_id != getattr(context, "run_id", None), grant.connection_id != connection.get("id"),
            grant.connection_revision != connection.get("revision"), grant.model_id != (connection.get("_model") or {}).get("model_id"),
            "code_execution" not in grant.native_tools, selected.max_native_tool_calls > grant.max_calls_per_request,
            not grant.hosted_code_spend_usd, not grant.code_execution_price)):
            raise ValueError("The hosted-code grant does not match this execution or its selected input files.")
        tools.append(CodeExecutionTool())
    return tools


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
            detail=f"A shared limit of {native_limit} native calls per request applies to all selected OpenAI native tools together. It cannot exceed either the native-call ceiling or the applicable search ceiling.",
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
