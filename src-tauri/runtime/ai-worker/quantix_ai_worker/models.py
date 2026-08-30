"""Provider/model construction for the Quantix AI worker.

Every client is constructed explicitly: API keys are passed as constructor
arguments (never ambient environment), SDK retries are disabled, and the HTTP
transport ignores proxy environment variables.
"""

from typing import Any

from pydantic_ai.capabilities import Thinking
from pydantic_ai.models import Model
from pydantic_ai.models.anthropic import AnthropicModel
from pydantic_ai.models.google import GoogleModel
from pydantic_ai.models.openai import OpenAIChatModel
from pydantic_ai.providers.anthropic import AnthropicProvider
from pydantic_ai.providers.google import GoogleProvider
from pydantic_ai.providers.openai import OpenAIProvider

XAI_BASE_URL = "https://api.x.ai/v1"

EFFORT_LEVELS = ("low", "medium", "high", "xhigh")


class RouteError(Exception):
    """The requested route cannot be constructed from the initialize frame."""


def reasoning_effort(route: str, reasoning: str | None) -> str | None:
    if reasoning is None:
        return None
    if reasoning not in EFFORT_LEVELS:
        raise RouteError(f"unsupported reasoning selection {reasoning!r}")
    if route in {"anthropic", "anthropic_compatible", "google"} and reasoning == "xhigh":
        return "high"
    return reasoning


def build_model(
    route: str,
    base_url: str | None,
    api_key: str,
    model_id: str,
    reasoning: str | None,
    timeout_seconds: float,
) -> Any:
    if not api_key or not model_id:
        raise RouteError("initialize is missing the API key or model id")
    if route in {"openai", "xai", "openai_compatible"}:
        return _build_openai(route, base_url, api_key, model_id, reasoning, timeout_seconds)
    if route in {"anthropic", "anthropic_compatible"}:
        return _build_anthropic(base_url, api_key, model_id, reasoning, timeout_seconds)
    if route == "google":
        return _build_google(api_key, model_id, reasoning, timeout_seconds)
    raise RouteError(f"unsupported provider route {route!r}")


def _http(timeout_seconds: float):
    import httpx2

    return httpx2.AsyncClient(trust_env=False, timeout=timeout_seconds)


def _build_openai(
    route: str,
    base_url: str | None,
    api_key: str,
    model_id: str,
    reasoning: str | None,
    timeout_seconds: float,
):
    from openai import AsyncOpenAI

    from pydantic_ai.providers.openai import OpenAIProvider

    if route == "openai":
        resolved_base_url = base_url
    elif route == "xai":
        resolved_base_url = XAI_BASE_URL
    else:
        if not base_url:
            raise RouteError("openai_compatible requires a base URL")
        resolved_base_url = base_url
    client = AsyncOpenAI(
        api_key=api_key,
        base_url=resolved_base_url,
        max_retries=0,
        http_client=_http(timeout_seconds),
    )
    return OpenAIChatModel(model_id, provider=OpenAIProvider(openai_client=client))


def _build_anthropic(
    base_url: str | None,
    api_key: str,
    model_id: str,
    reasoning: str | None,
    timeout_seconds: float,
):
    from anthropic import AsyncAnthropic

    from pydantic_ai.providers.anthropic import AnthropicProvider

    client = AsyncAnthropic(
        base_url=base_url,
        api_key=api_key,
        max_retries=0,
        http_client=_http(timeout_seconds),
    )
    return AnthropicModel(model_id, provider=AnthropicProvider(anthropic_client=client))


def _build_google(
    api_key: str, model_id: str, reasoning: str | None, timeout_seconds: float
):
    from pydantic_ai.providers.google import GoogleProvider

    return GoogleModel(
        model_id,
        provider=GoogleProvider(api_key=api_key, http_client=_http(timeout_seconds)),
    )


def thinking_capability(route: str, reasoning: str | None):
    effort = reasoning_effort(route, reasoning)
    if effort is None:
        return []
    return [Thinking(effort=effort)]
