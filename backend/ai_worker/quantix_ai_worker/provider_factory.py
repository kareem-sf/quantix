"""Construct request-scoped Pydantic AI models from an explicit saved connection.

There is no authentication, discovery or inference at import time. The caller owns
Tender policy checks, model capability checks, tool selection and budget hooks.
"""

import asyncio
import json
import os
from contextlib import AsyncExitStack, asynccontextmanager
from dataclasses import dataclass, field
from typing import Any


@dataclass
class APIModelBinding:
    model: Any
    settings: dict
    client: Any = field(repr=False)
    cloud_session: Any = field(default=None, repr=False)


def _required_key(connection, credentials):
    if connection["auth_type"] == "none":
        return "quantix-local-no-auth"
    key = credentials.get("api_key")
    if not key:
        raise ValueError("This connection has no API key. Enter one or configure its selected environment variable.")
    return key


async def _azure_token_provider(stack, options, credentials):
    from azure.identity.aio import (
        ClientSecretCredential,
        DefaultAzureCredential,
        get_bearer_token_provider,
    )

    if credentials.get("client_secret"):
        if not options.get("tenant_id") or not options.get("client_id"):
            raise ValueError("Azure service principal credentials need a tenant ID and client ID.")
        identity = ClientSecretCredential(options["tenant_id"], options["client_id"], credentials["client_secret"])
    else:
        identity = DefaultAzureCredential(exclude_interactive_browser_credential=True,
                                          managed_identity_client_id=options.get("client_id"))
    await stack.enter_async_context(identity)
    return get_bearer_token_provider(identity, "https://cognitiveservices.azure.com/.default")


def _budget(value: str, limit: int) -> int:
    try:
        budget = int(value.split(":", 1)[1])
    except (ValueError, IndexError) as error:
        raise ValueError("Use budget:<tokens> for a thinking budget.") from error
    if not 1 <= budget < limit:
        raise ValueError("The thinking budget must be positive and below the output token limit.")
    return budget


def build_model_settings(route: dict, connection: dict) -> dict:
    """Keep explicit effort values exact; never use the SDK's nearest-level mapping."""
    protocol, provider = connection["protocol"], connection["provider_id"]
    limit = route.get("max_output_tokens", 8192)
    settings = {"max_tokens": limit, "timeout": 180.0}
    extra = {}
    if protocol == "openai_responses":
        settings["openai_store"] = False
        settings["openai_include_raw_annotations"] = True
        if provider == "openai" and route.get("web_search"):
            settings["openai_include_web_search_sources"] = True
    if provider == "openrouter":
        choices = connection["settings"]
        approved = choices.get("upstream_providers", [])
        if not approved and route["model_id"] != "catalog":
            raise ValueError("Select approved OpenRouter upstream providers before starting Tender work.")
        routing = {"require_parameters": True, "allow_fallbacks": False,
                   "data_collection": choices.get("data_collection", "deny"), "zdr": choices.get("zdr", False)}
        if approved:
            routing.update(only=approved, order=approved)
        if protocol == "openai_chat":
            settings["openrouter_provider"] = routing
        else:
            extra["provider"] = routing
    effort = route.get("reasoning")
    if effort and effort != "default":
        if protocol == "anthropic":
            if effort.startswith("budget:"):
                settings["anthropic_thinking"] = {"type": "enabled", "budget_tokens": _budget(effort, limit)}
            elif effort in {"adaptive", "disabled"}:
                settings["anthropic_thinking"] = {"type": effort}
            elif effort in {"low", "medium", "high", "xhigh", "max"}:
                settings.update(anthropic_effort=effort, anthropic_thinking={"type": "adaptive"})
            else:
                raise ValueError("This reasoning option has no supported Anthropic mapping.")
        elif protocol == "google":
            if effort.startswith("budget:"):
                settings["google_thinking_config"] = {"thinking_budget": _budget(effort, limit)}
            elif effort in {"minimal", "low", "medium", "high"}:
                settings["google_thinking_config"] = {"thinking_level": effort}
            elif effort == "disabled":
                settings["google_thinking_config"] = {"thinking_budget": 0}
            else:
                raise ValueError("Choose Google's documented thinking level or a token budget. Effort is not automatically downgraded.")
        elif protocol == "mistral":
            if effort not in {"high", "none", "disabled"}:
                raise ValueError("This Mistral adapter accepts high or disabled reasoning; other levels are not translated.")
            settings["thinking"] = "high" if effort == "high" else False
        elif protocol == "cohere":
            raise ValueError("The Cohere adapter cannot set this reasoning option. Use the model's default behavior.")
        elif protocol == "bedrock":
            model_id = route["model_id"].lower()
            if "anthropic." in model_id:
                if effort.startswith("budget:"):
                    native = {"thinking": {"type": "enabled", "budget_tokens": _budget(effort, limit)}}
                elif effort in {"low", "medium", "high", "xhigh", "max"}:
                    native = {"thinking": {"type": "adaptive"}, "output_config": {"effort": effort}}
                elif effort in {"adaptive", "disabled"}:
                    native = {"thinking": {"type": effort}}
                else:
                    raise ValueError("This Bedrock Claude reasoning setting is unsupported.")
            elif "openai." in model_id:
                native = {"reasoning_effort": effort}
            elif "qwen." in model_id and effort in {"low", "high"}:
                native = {"reasoning_config": effort}
            else:
                raise ValueError("This Bedrock model needs its default reasoning setting; no exact mapping is configured.")
            settings["bedrock_additional_model_requests_fields"] = native
        elif provider == "openrouter":
            reasoning = {"effort": effort}
            if protocol == "openai_chat":
                settings["openrouter_reasoning"] = reasoning
            else:
                extra["reasoning"] = reasoning
        elif provider in {"zai", "minimax", "alibaba"}:
            if effort not in {"enabled", "disabled", "adaptive"}:
                raise ValueError("This provider uses enabled/disabled thinking. A different effort cannot be substituted.")
            if provider == "alibaba":
                extra["enable_thinking"] = effort != "disabled"
            else:
                extra["thinking"] = {"type": "adaptive" if provider == "minimax" and effort != "disabled" else effort}
        else:
            settings["openai_reasoning_effort"] = effort
    if provider == "deepseek" and protocol == "openai_chat" and effort:
        extra["thinking"] = {"type": "disabled" if effort in {"none", "disabled"} else "enabled"}
    if extra:
        settings["extra_body"] = extra
    return settings


def _compatible_provider(identifier, client):
    from pydantic_ai.providers.openai import OpenAIProvider

    if identifier == "deepseek":
        from pydantic_ai.providers.deepseek import DeepSeekProvider
        return DeepSeekProvider(openai_client=client)
    if identifier == "kimi":
        from pydantic_ai.providers.moonshotai import MoonshotAIProvider
        return MoonshotAIProvider(openai_client=client)
    if identifier == "alibaba":
        from pydantic_ai.providers.alibaba import AlibabaProvider
        return AlibabaProvider(openai_client=client)
    if identifier == "openrouter":
        from pydantic_ai.providers.openrouter import OpenRouterProvider
        return OpenRouterProvider(openai_client=client)
    if identifier == "ollama":
        from pydantic_ai.providers.ollama import OllamaProvider
        return OllamaProvider(openai_client=client)
    if identifier == "zai":
        from pydantic_ai.providers.zai import ZaiProvider
        return ZaiProvider(openai_client=client)
    return OpenAIProvider(openai_client=client)


def _compatible_profile(identifier, model_name, provider):
    from pydantic_ai.profiles import merge_profile
    from pydantic_ai.profiles.openai import OpenAIModelProfile

    if identifier == "xai":
        from pydantic_ai.profiles.grok import grok_model_profile
        profile = grok_model_profile(model_name)
    elif identifier == "minimax":
        # MiniMax's default format keeps complete thinking in <think> tags.
        profile = OpenAIModelProfile(openai_chat_send_back_thinking_parts="tags")
    elif identifier == "custom":
        profile = OpenAIModelProfile(supports_json_schema_output=False, supports_json_object_output=False,
                                     supports_image_output=False, openai_system_prompt_role="system")
    else:
        profile = provider.model_profile(model_name)
    if identifier not in {"openai", "azure"}:
        profile = merge_profile(profile, OpenAIModelProfile(openai_system_prompt_role="system"))
    return profile


@asynccontextmanager
async def model_for_route(route: dict, connection: dict, credentials: dict):
    """Yield an APIModelBinding and close all owned clients on success or cancellation."""
    from .validation import validate_base_url

    if not connection["enabled"]:
        raise ValueError("This AI connection is disabled.")
    protocol, provider_id = connection["protocol"], connection["provider_id"]
    model_name = route["model_id"]
    options = connection["settings"]
    base_url = validate_base_url(connection.get("base_url"), allow_insecure_http=connection.get("allow_insecure_http", False))
    settings = build_model_settings(route, connection)
    async with AsyncExitStack() as stack:
        if protocol in {"openai_chat", "openai_responses"}:
            import httpx2
            from openai import AsyncOpenAI
            from pydantic_ai.models.openai import OpenAIChatModel, OpenAIResponsesModel

            http = await stack.enter_async_context(httpx2.AsyncClient(timeout=180, follow_redirects=False, trust_env=False))
            if connection["auth_type"] == "azure_identity":
                key = await _azure_token_provider(stack, options, credentials)
            else:
                key = _required_key(connection, credentials)
            client = await stack.enter_async_context(AsyncOpenAI(
                api_key=key, base_url=base_url, max_retries=0, http_client=http,
                organization="", project="",
            ))
            # Empty constructor values suppress ambient account scoping; None
            # then omits the headers instead of transmitting blank account IDs.
            client.organization = None
            client.project = None
            provider = _compatible_provider(provider_id, client)
            if model_name == "catalog":
                # Discovery does not need a meaningful model profile and never runs inference.
                profile = {}
            else:
                profile = _compatible_profile(provider_id, model_name, provider)
            if provider_id == "openrouter" and protocol == "openai_chat":
                from pydantic_ai.models.openrouter import OpenRouterModel
                model = OpenRouterModel(model_name, provider=provider, profile=profile)
            else:
                model_type = OpenAIResponsesModel if protocol == "openai_responses" else OpenAIChatModel
                model = model_type(model_name, provider=provider, profile=profile)
            yield APIModelBinding(model, settings, client)
        elif protocol == "anthropic":
            import httpx2
            from anthropic import AsyncAnthropic, AsyncAnthropicFoundry
            from pydantic_ai.models.anthropic import AnthropicModel
            from pydantic_ai.providers.anthropic import AnthropicProvider

            if os.environ.get("ANTHROPIC_CUSTOM_HEADERS"):
                raise ValueError("Remove the service's ANTHROPIC_CUSTOM_HEADERS override before using this connection. Account headers must come from the selected connection only.")
            http = await stack.enter_async_context(httpx2.AsyncClient(timeout=180, follow_redirects=False, trust_env=False))
            if provider_id == "azure":
                if os.environ.get("ANTHROPIC_FOUNDRY_RESOURCE") is not None:
                    raise ValueError("Remove the service's ANTHROPIC_FOUNDRY_RESOURCE override before using an explicit Foundry endpoint.")
                authentication = ({"api_key": "", "azure_ad_token_provider": await _azure_token_provider(stack, options, credentials)}
                                  if connection["auth_type"] == "azure_identity"
                                  else {"api_key": _required_key(connection, credentials)})
                client = await stack.enter_async_context(AsyncAnthropicFoundry(
                    **authentication, base_url=base_url, max_retries=0, http_client=http))
            else:
                client = await stack.enter_async_context(AsyncAnthropic(api_key=_required_key(connection, credentials),
                                base_url=base_url, max_retries=0, http_client=http))
            model = AnthropicModel(model_name, provider=AnthropicProvider(anthropic_client=client))
            yield APIModelBinding(model, settings, client)
        elif protocol == "google":
            import httpx2
            from google import genai
            from google.genai.types import HttpOptions, HttpRetryOptions
            from pydantic_ai.models.google import GoogleModel
            from pydantic_ai.providers.google import GoogleProvider
            from pydantic_ai.providers.google_cloud import GoogleCloudProvider

            http = await stack.enter_async_context(httpx2.AsyncClient(timeout=180, follow_redirects=False, trust_env=False))
            cloud = provider_id == "google_vertex"
            kwargs = {"vertexai": cloud}
            if connection["auth_type"] == "google_identity":
                if not options.get("project") or not options.get("location"):
                    raise ValueError("Google Cloud identity needs an explicit project and location.")
                kwargs.update(project=options["project"], location=options["location"])
                if credentials.get("service_account_json"):
                    from google.oauth2.service_account import Credentials
                    kwargs["credentials"] = Credentials.from_service_account_info(
                        json.loads(credentials["service_account_json"]), scopes=["https://www.googleapis.com/auth/cloud-platform"])
                location = options["location"]
                if not all(c.isalnum() or c == "-" for c in location):
                    raise ValueError("Enter a valid Google Cloud location.")
                endpoint = "https://aiplatform.googleapis.com" if location == "global" else (
                    f"https://aiplatform.{location}.rep.googleapis.com" if location in {"us", "eu"}
                    else f"https://{location}-aiplatform.googleapis.com")
            else:
                kwargs["api_key"] = _required_key(connection, credentials)
                endpoint = "https://aiplatform.googleapis.com" if cloud else "https://generativelanguage.googleapis.com"
                if cloud and (options.get("project") or options.get("location")):
                    raise ValueError("Google Cloud API-key mode uses Express access. Select Google identity for project/location access.")
            client = genai.Client(**kwargs, http_options=HttpOptions(
                base_url=base_url or endpoint, httpx_async_client=http, timeout=180000,
                retry_options=HttpRetryOptions(attempts=1),
            ))
            stack.callback(client.close)
            stack.push_async_callback(client.aio.aclose)
            provider = GoogleCloudProvider(client=client) if cloud else GoogleProvider(client=client)
            yield APIModelBinding(GoogleModel(model_name, provider=provider), settings, client)
        elif protocol == "bedrock":
            import boto3
            from botocore.config import Config
            from pydantic_ai.models.bedrock import BedrockConverseModel
            from pydantic_ai.providers.bedrock import BedrockProvider

            if not options.get("region"):
                raise ValueError("Choose an AWS region for this connection.")
            session = boto3.Session(region_name=options["region"], profile_name=options.get("aws_profile"),
                    **{key: credentials[key] for key in ("aws_access_key_id", "aws_secret_access_key", "aws_session_token") if key in credentials})
            client = await asyncio.to_thread(session.client, "bedrock-runtime", endpoint_url=base_url,
                                             config=Config(connect_timeout=20, read_timeout=180,
                                                           retries={"total_max_attempts": 1}))
            stack.callback(client.close)
            provider = BedrockProvider(bedrock_client=client)
            yield APIModelBinding(BedrockConverseModel(model_name, provider=provider), settings, client, session)
        elif protocol == "mistral":
            import httpx2
            from mistralai.client import Mistral
            from pydantic_ai.models.mistral import MistralModel
            from pydantic_ai.providers.mistral import MistralProvider

            http = await stack.enter_async_context(httpx2.AsyncClient(timeout=180, follow_redirects=False, trust_env=False))
            client = await stack.enter_async_context(Mistral(api_key=_required_key(connection, credentials),
                             server_url=base_url or "https://api.mistral.ai", async_client=http, retry_config=None))
            stack.callback(client.__exit__, None, None, None)
            yield APIModelBinding(MistralModel(model_name, provider=MistralProvider(mistral_client=client)), settings, client)
        elif protocol == "cohere":
            import httpx
            from cohere import AsyncClientV2
            from pydantic_ai.models.cohere import CohereModel
            from pydantic_ai.providers.cohere import CohereProvider

            http = await stack.enter_async_context(httpx.AsyncClient(timeout=180, follow_redirects=False, trust_env=False))
            client = AsyncClientV2(api_key=_required_key(connection, credentials),
                                    base_url=base_url or "https://api.cohere.com", httpx_client=http, max_retries=0)
            yield APIModelBinding(CohereModel(model_name, provider=CohereProvider(cohere_client=client)), settings, client)
        else:
            raise ValueError("This connection uses an official client runtime, not a direct API model.")
