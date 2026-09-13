"""Request-scoped provider bindings for the bundled direct API path.

Only the four supported vendor families and OpenAI-compatible BYOK are
constructed here.  Every client receives explicit credentials and an explicit
endpoint; SDK environment discovery and retry defaults are not used.
"""

from __future__ import annotations

from contextlib import AsyncExitStack, asynccontextmanager
from dataclasses import dataclass, field
from typing import Any

from .ai_api_errors import DirectAPIError, DirectDependencyError

REQUEST_TIMEOUT_SECONDS = 180.0

# Provider-documented, exact alias pairs.  The direct runtime never derives
# aliases from arbitrary prefixes or price-library matching.  OpenAI publishes
# the dated snapshot for this alias on its model page; Anthropic resolves its
# pre-4.6 convenience alias through the authenticated Models API because the
# current documentation intentionally does not publish a guessed date.
OFFICIAL_EXACT_MODEL_ALIASES = {
    "openai": {
        "gpt-4.1-mini": "gpt-4.1-mini-2025-04-14",
    },
}
ANTHROPIC_PRE46_ALIASES = frozenset({"claude-sonnet-4-5"})


@dataclass
class APIModelBinding:
    model: Any
    settings: dict
    client: Any = field(repr=False)


def _key(connection: dict, credentials: dict[str, str]) -> str:
    if connection.get("auth_type") not in {"api_key", "environment"}:
        raise DirectAPIError("Direct API connections require an explicit API key or environment credential.")
    value = credentials.get("api_key") if isinstance(credentials, dict) else None
    if not isinstance(value, str) or not value.strip():
        raise DirectAPIError("This connection has no API key. Enter one or configure its selected environment variable.")
    return value.strip()


def _base_url(connection: dict) -> str:
    provider, protocol = connection["provider_id"], connection["protocol"]
    value = connection.get("base_url")
    defaults = {
        ("openai", "openai_chat"): "https://api.openai.com/v1",
        ("openai", "openai_responses"): "https://api.openai.com/v1",
        ("xai", "openai_chat"): "https://api.x.ai/v1",
        ("xai", "openai_responses"): "https://api.x.ai/v1",
        ("anthropic", "anthropic"): "https://api.anthropic.com",
        ("google", "google"): "https://generativelanguage.googleapis.com",
    }
    value = value or defaults.get((provider, protocol))
    if not value:
        raise DirectAPIError("Enter the provider endpoint for this direct API connection.")
    from .ai_connections import validate_base_url

    validated = validate_base_url(
        value, allow_insecure_http=connection.get("allow_insecure_http", False)
    )
    if not validated:
        raise DirectAPIError("Enter a valid provider endpoint for this direct API connection.")
    return validated


def _budget(value: str, limit: int) -> int:
    try:
        budget = int(value.split(":", 1)[1])
    except (ValueError, IndexError) as error:
        raise DirectAPIError("Use budget:<tokens> for a thinking budget.") from error
    if not 1 <= budget < limit:
        raise DirectAPIError("The thinking budget must be positive and below the output token limit.")
    return budget


def build_model_settings(route: dict, connection: dict) -> dict:
    """Translate only settings documented by the four direct adapters."""

    protocol, provider = connection["protocol"], connection["provider_id"]
    from .ai_generation import bounded_native_call_limit, validate_generation

    generation = validate_generation(route, connection)
    limit = int(route.get("max_output_tokens", 8192))
    if not 1 <= limit <= 200000:
        raise DirectAPIError("The requested output limit is outside the supported range.")
    settings: dict = {"max_tokens": limit, "timeout": REQUEST_TIMEOUT_SECONDS}
    for name in ("temperature", "top_p"):
        value = getattr(generation, name)
        if value is not None:
            settings[name] = value
    # Prompt caching. The tool catalogue and office rules are the same on every
    # step and every run, so a cached prefix is billed at the cached rate.
    # Anthropic caches only marked blocks; OpenAI routes requests that share a
    # cache key to the same cache. Google and xAI cache implicitly.
    if protocol == "anthropic" and provider == "anthropic":
        settings.update(
            anthropic_cache_tool_definitions=True,
            anthropic_cache_instructions=True,
            anthropic_cache_messages=True,
        )
    if provider == "openai" and protocol in {"openai_responses", "openai_chat"}:
        settings["openai_prompt_cache_key"] = "quantix-tender-office"
    if protocol == "openai_responses":
        settings.update(openai_store=False, openai_include_raw_annotations=True)
        if provider == "openai" and route.get("reasoning") not in {"disabled", "none"}:
            from pydantic_ai.providers.openai import OpenAIProvider
            profile = OpenAIProvider.model_profile(str(route.get("model_id") or "")) or {}
            if profile.get("openai_supports_reasoning"):
                settings["openai_reasoning_summary"] = "auto"
        if provider == "openai" and (route.get("web_search") or "web_search" in generation.native_tools):
            settings["openai_include_web_search_sources"] = True
            # WebSearchTool.max_uses is Anthropic-specific. Responses exposes
            # the documented total native-tool limit through SDK extra_body.
        native_limit = bounded_native_call_limit(route, connection)
        if native_limit is not None:
            settings["extra_body"] = {"max_tool_calls": native_limit}
    effort = route.get("reasoning")
    if effort and effort != "default":
        if protocol == "anthropic":
            if effort.startswith("budget:"):
                settings["anthropic_thinking"] = {
                    "type": "enabled",
                    "budget_tokens": _budget(effort, limit),
                }
            elif effort in {"adaptive", "disabled"}:
                settings["anthropic_thinking"] = {"type": effort}
            elif effort in {"low", "medium", "high", "xhigh", "max"}:
                settings.update(anthropic_effort=effort, anthropic_thinking={"type": "adaptive"})
            else:
                raise DirectAPIError("This reasoning option has no supported Anthropic mapping.")
        elif protocol == "google":
            if effort.startswith("budget:"):
                settings["google_thinking_config"] = {
                    "thinking_budget": _budget(effort, limit)
                }
            elif effort in {"minimal", "low", "medium", "high"}:
                settings["google_thinking_config"] = {"thinking_level": effort}
            elif effort == "disabled":
                settings["google_thinking_config"] = {"thinking_budget": 0}
            else:
                raise DirectAPIError("Choose Google's documented thinking level or a token budget.")
        else:
            if effort == "disabled":
                effort = "none"
            if effort not in {"none", "minimal", "low", "medium", "high", "xhigh", "max"}:
                raise DirectAPIError("Choose a documented reasoning effort for this model.")
            settings["openai_reasoning_effort"] = effort
    if provider == "anthropic" and protocol == "anthropic":
        thinking = settings.get("anthropic_thinking")
        if thinking and thinking.get("type") in {"adaptive", "enabled"}:
            thinking["display"] = "summarized"
    if provider == "google" and protocol == "google":
        from pydantic_ai.profiles.google import google_model_profile
        profile = google_model_profile(str(route.get("model_id") or "")) or {}
        if profile.get("supports_thinking") or settings.get("google_thinking_config"):
            settings.setdefault("google_thinking_config", {})["include_thoughts"] = True
    return settings


def supplied_summary_supported(connection: dict, model_id: str = "") -> bool:
    """Only documented public summary mappings; never infer BYOK semantics."""
    provider, protocol = connection.get("provider_id"), connection.get("protocol")
    if provider == "openai":
        if protocol != "openai_responses":
            return False
        if not model_id:
            return True
        from pydantic_ai.providers.openai import OpenAIProvider
        return bool((OpenAIProvider.model_profile(model_id) or {}).get("openai_supports_reasoning"))
    if provider == "google":
        if protocol != "google":
            return False
        from pydantic_ai.profiles.google import google_model_profile
        return bool((google_model_profile(model_id) or {}).get("supports_thinking"))
    if provider == "anthropic":
        # Claude 3.7 exposed full thinking rather than summarized thinking.
        return protocol == "anthropic" and model_id.startswith(("claude-opus-4", "claude-sonnet-4", "claude-haiku-4", "claude-opus-5", "claude-sonnet-5", "claude-fable-5", "claude-mythos-5"))
    return provider == "xai" and protocol in {"openai_responses", "openai_chat"} and model_id == "grok-4.6"


def _openai_profile(provider: str, protocol: str, route: dict):
    """Use an explicit compatibility profile for unknown BYOK model names.

    Pydantic AI's OpenAI profile helper intentionally uses model-family
    prefixes.  That is useful for the first party catalogue but would silently
    guess capabilities for a custom or newly introduced alias.  The direct
    route gets the protocol guarantees only; model capability checks remain
    owned by the saved model record and the setup check.
    """

    from pydantic_ai.profiles.openai import OpenAIModelProfile

    model_name = str(route.get("model_id") or "")
    if provider == "openai":
        from pydantic_ai.providers.openai import OpenAIProvider

        return OpenAIProvider.model_profile(model_name)
    if provider == "xai":
        # xAI's OpenAI-compatible endpoint uses the maintained Grok family
        # profile; the profile only describes wire behavior and never authorizes
        # a model alias or selects one on the user's behalf.
        from pydantic_ai.profiles import merge_profile
        from pydantic_ai.profiles.grok import grok_model_profile

        return merge_profile(
            grok_model_profile(model_name),
            OpenAIModelProfile(openai_system_prompt_role="system"),
        )
    # BYOK compatibility is deliberately conservative.  A check may establish
    # that tools and structured tool output work, but the catalog cannot claim
    # native JSON schema, JSON mode, reasoning or hosted tools in advance.
    return OpenAIModelProfile(
        supports_tools=True,
        supports_json_schema_output=False,
        supports_json_object_output=False,
        supports_inline_system_prompts=True,
        openai_system_prompt_role="system",
        supported_native_tools=frozenset(),
    )


def _model_profile(provider: str, protocol: str, route: dict, model_name: str):
    if protocol in {"openai_chat", "openai_responses"}:
        return _openai_profile(provider, protocol, route)
    if protocol == "anthropic":
        from pydantic_ai.providers.anthropic import AnthropicProvider

        # Anthropic's provider profile is source-backed and handles its JSON
        # schema transformer for known Claude families.  Tool output remains
        # the default mode for unknown model names.
        profile = AnthropicProvider.model_profile(model_name) or {}
        # Dynamic web filtering implicitly enables hosted code. Basic search
        # and fetch retain bounded native requests without widening authority.
        return {**profile, "anthropic_supports_dynamic_filtering": False}
    if protocol == "google":
        from pydantic_ai.providers.google import GoogleProvider

        from .ai_gemini_schema import GeminiSchemaTransformer

        # Gemini refuses schema bounds; see ai_gemini_schema for the evidence.
        profile = GoogleProvider.model_profile(model_name) or {}
        return {**profile, "json_schema_transformer": GeminiSchemaTransformer}
    raise DirectAPIError("This connection protocol is unavailable in the bundled direct API runtime.")


def _normalize_google_model(model_name: str) -> str:
    if model_name.startswith("models/"):
        model_name = model_name[7:]
        if not model_name:
            raise DirectAPIError("Choose the exact Google model ID returned by the provider.")
    return model_name


async def _resolve_anthropic_model(client, model_name: str) -> str:
    if model_name not in ANTHROPIC_PRE46_ALIASES:
        return model_name
    resolved = await client.models.retrieve(model_name)
    resolved_id = getattr(resolved, "id", None)
    if not isinstance(resolved_id, str) or not resolved_id.strip():
        raise DirectAPIError("The provider did not resolve this Anthropic model alias to an exact model ID.")
    return resolved_id.strip()


def _resolve_exact_alias(provider_id: str, model_name: str) -> str:
    return OFFICIAL_EXACT_MODEL_ALIASES.get(provider_id, {}).get(model_name, model_name)


def canonical_model_id(provider_id: str, protocol: str, model_name: str) -> str:
    """Return a comparison identity while retaining the provider's raw ID."""

    if protocol == "google" and model_name.startswith("models/"):
        model_name = model_name[7:]
    return _resolve_exact_alias(provider_id, model_name)


@asynccontextmanager
async def model_for_route(route: dict, connection: dict, credentials: dict[str, str]):
    """Yield a request-scoped Pydantic AI model and SDK client."""

    provider_id, protocol = connection.get("provider_id"), connection.get("protocol")
    if protocol == "google":
        model_name = _normalize_google_model(str(route.get("model_id", "")))
    else:
        model_name = str(route.get("model_id", ""))
    model_name = _resolve_exact_alias(provider_id, model_name)
    if not model_name:
        raise DirectAPIError("Choose an exact model ID before sending a direct API request.")
    if model_name == "catalog":
        model_name = "catalog"
    if protocol not in {"openai_chat", "openai_responses", "anthropic", "google"}:
        raise DirectAPIError("This connection uses an unsupported direct API protocol.")
    base_url = _base_url(connection)
    settings = build_model_settings(route, connection)
    async with AsyncExitStack() as stack:
        try:
            import httpx2
        except ImportError as error:
            raise DirectDependencyError("The bundled HTTP client is missing. Repair the Quantix installation and retry.") from error

        transport = None
        if "code_execution" in route.get("native_tools", []):
            from .native_provider_transport import NativeCodeTransport
            transport = NativeCodeTransport(httpx2.AsyncHTTPTransport(retries=0, trust_env=False))
        http = await stack.enter_async_context(
            httpx2.AsyncClient(
                timeout=REQUEST_TIMEOUT_SECONDS,
                follow_redirects=False,
                trust_env=False,
                transport=transport,
            )
        )
        if protocol in {"openai_chat", "openai_responses"}:
            try:
                from openai import AsyncOpenAI
                from pydantic_ai.models.openai import OpenAIChatModel, OpenAIResponsesModel
                from pydantic_ai.providers.openai import OpenAIProvider
            except ImportError as error:
                raise DirectDependencyError("The bundled OpenAI adapter is missing. Repair the Quantix installation and retry.") from error
            client = await stack.enter_async_context(
                AsyncOpenAI(
                    api_key=_key(connection, credentials),
                    base_url=base_url,
                    max_retries=0,
                    http_client=http,
                    organization="",
                    project="",
                )
            )
            # Prevent account scoping inherited from the OpenAI client's
            # ambient defaults.  The saved connection is the sole authority.
            client.organization = None
            client.project = None
            provider = OpenAIProvider(openai_client=client)
            profile = _model_profile(provider_id, protocol, route, model_name)
            model_type = OpenAIResponsesModel if protocol == "openai_responses" else OpenAIChatModel
            yield APIModelBinding(model_type(model_name, provider=provider, profile=profile), settings, client)
            return
        if protocol == "anthropic":
            try:
                from anthropic import AsyncAnthropic
                from pydantic_ai.models.anthropic import AnthropicModel
                from pydantic_ai.providers.anthropic import AnthropicProvider
            except ImportError as error:
                raise DirectDependencyError("The bundled Anthropic adapter is missing. Repair the Quantix installation and retry.") from error
            client = await stack.enter_async_context(
                AsyncAnthropic(
                    api_key=_key(connection, credentials),
                    base_url=base_url,
                    max_retries=0,
                    http_client=http,
                )
            )
            model_name = await _resolve_anthropic_model(client, model_name)
            provider = AnthropicProvider(anthropic_client=client)
            profile = _model_profile(provider_id, protocol, route, model_name)
            yield APIModelBinding(AnthropicModel(model_name, provider=provider, profile=profile), settings, client)
            return
        try:
            from google import genai
            from google.genai.types import HttpOptions, HttpRetryOptions
            from pydantic_ai.models.google import GoogleModel
            from pydantic_ai.providers.google import GoogleProvider
        except ImportError as error:
            raise DirectDependencyError("The bundled Google adapter is missing. Repair the Quantix installation and retry.") from error
        try:
            client = genai.Client(
                api_key=_key(connection, credentials),
                vertexai=False,
                http_options=HttpOptions(
                    base_url=base_url,
                    httpx_async_client=http,
                    timeout=int(REQUEST_TIMEOUT_SECONDS * 1000),
                    retry_options=HttpRetryOptions(attempts=1),
                ),
            )
        except Exception as error:
            raise DirectAPIError("The bundled Google adapter could not be configured. Check the API key and endpoint.") from error
        stack.callback(client.close)
        stack.push_async_callback(client.aio.aclose)
        provider = GoogleProvider(client=client)
        profile = _model_profile(provider_id, protocol, route, model_name)
        yield APIModelBinding(GoogleModel(model_name, provider=provider, profile=profile), settings, client)
