"""The five API-key providers: build a Pydantic AI model, list the account's models and explain failures."""

from typing import Literal

from pydantic_ai.models import Model

Provider = Literal["anthropic", "openai", "google", "xai", "openai_compatible"]

LABELS: dict[str, str] = {
    "anthropic": "Anthropic",
    "openai": "OpenAI",
    "google": "Google",
    "xai": "xAI",
    "openai_compatible": "OpenAI-compatible service",
}


def build_model(provider: Provider, model: str, key: str, base_url: str | None) -> Model:
    if provider == "anthropic":
        from pydantic_ai.models.anthropic import AnthropicModel
        from pydantic_ai.providers.anthropic import AnthropicProvider

        return AnthropicModel(model, provider=AnthropicProvider(api_key=key))
    if provider == "openai":
        from pydantic_ai.models.openai import OpenAIResponsesModel
        from pydantic_ai.providers.openai import OpenAIProvider

        return OpenAIResponsesModel(model, provider=OpenAIProvider(api_key=key))
    if provider == "google":
        from pydantic_ai.models.google import GoogleModel
        from pydantic_ai.providers.google import GoogleProvider

        return GoogleModel(model, provider=GoogleProvider(api_key=key))
    if provider == "xai":
        from pydantic_ai.models.xai import XaiModel
        from pydantic_ai.providers.xai import XaiProvider

        return XaiModel(model, provider=XaiProvider(api_key=key))
    from pydantic_ai.models.openai import OpenAIChatModel
    from pydantic_ai.providers.openai import OpenAIProvider

    return OpenAIChatModel(model, provider=OpenAIProvider(base_url=base_url, api_key=key))


async def list_models(provider: Provider, key: str, base_url: str | None) -> list[str]:
    """The account's chat models, newest names first where the service orders them."""
    if provider == "anthropic":
        import anthropic

        client = anthropic.AsyncAnthropic(api_key=key)
        return [m.id async for m in client.models.list(limit=100)]
    if provider == "google":
        from google import genai

        pager = await genai.Client(api_key=key).aio.models.list()
        return [
            m.name.removeprefix("models/")
            async for m in pager
            if m.name and "generateContent" in (m.supported_actions or [])
        ]
    if provider == "xai":
        from xai_sdk.aio.client import Client

        return [m.name for m in await Client(api_key=key).models.list_language_models()]
    import openai

    client = openai.AsyncOpenAI(api_key=key, base_url=base_url if provider == "openai_compatible" else None)
    return sorted([m.id async for m in client.models.list()], reverse=True)


MODEL_MISSING = "This model isn't available on this account."


def explain(error: BaseException) -> str:
    """A plain sentence for the engineer. Never includes the key or the raw response."""
    status = _status(error)
    text = str(error).lower()
    if status in (401, 403, "UNAUTHENTICATED", "PERMISSION_DENIED") or "api key" in text or "api_key" in text:
        return "The key was refused. Check it and try again."
    if status in (404, "NOT_FOUND"):
        return MODEL_MISSING
    if status in (402, 429, "RESOURCE_EXHAUSTED"):
        return "The service is limiting requests, or the account is out of credit. Try again later."
    if status in ("UNAVAILABLE", "DEADLINE_EXCEEDED") or "connect" in type(error).__name__.lower():
        return "Couldn't reach the service. Check the internet connection and the address."
    return f"The service answered with an error ({status or type(error).__name__})."


def _status(error: BaseException) -> int | str | None:
    for attribute in ("status_code", "code", "status"):
        value = getattr(error, attribute, None)
        if callable(value):
            try:
                value = value()
            except TypeError:
                continue
        name = getattr(value, "name", None)
        if isinstance(name, str):
            return name
        if isinstance(value, int | str):
            return value
    return None
