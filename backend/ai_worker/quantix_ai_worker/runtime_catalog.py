"""Explicit metadata discovery for supported original-client connections."""

import os

from .common import RuntimeUnavailable, child_environment, owned_client

MAX_MODELS = 2000


def _positive(value):
    return value if type(value) is int and value > 0 else None


def _boolean(value):
    if isinstance(value, dict):
        value = value.get("supported")
    return value if type(value) is bool else None


def _entry(model_id, display_name, capabilities):
    return {"model_id": model_id, "display_name": display_name, "capabilities": capabilities}


async def codex_models(home, connection):
    from openai_codex import AsyncCodex

    from .codex import codex_config

    if connection["auth_type"] != "client_login":
        raise RuntimeUnavailable("Sign in through the original Codex connection to discover its models. Use the OpenAI API connection for API-key model discovery.")
    async with owned_client(AsyncCodex(codex_config(home))) as client:
        account = await client.account()
        account_value = account.model_dump(mode="json", by_alias=True).get("account")
        if not account_value or account_value.get("type") != "chatgpt":
            raise RuntimeUnavailable("Complete the managed ChatGPT sign-in for this Codex connection, then discover its models.")
        page = await client.models(include_hidden=False)
        # The pinned public Python wrapper does not expose a cursor argument.
        # Do not misrepresent a first page as a complete catalog.
        if page.next_cursor:
            raise RuntimeUnavailable("Codex returned a paginated catalog that this pinned client cannot finish. Add the required model manually instead.")
        if len(page.data) > MAX_MODELS:
            raise RuntimeUnavailable("The Codex catalog exceeds the supported size. Add the required model manually.")
        result = []
        for model in page.data:
            if model.hidden:
                continue
            modalities = None
            if "input_modalities" in model.model_fields_set and model.input_modalities is not None:
                modalities = [item.value if hasattr(item, "value") else item for item in model.input_modalities]
            efforts = [option.reasoning_effort.value for option in model.supported_reasoning_efforts]
            result.append(_entry(model.model, model.display_name, {
                # Codex documents both tools and per-turn schema output as part
                # of its agent contract. Other bounds require actual metadata.
                "tools": True, "structured_output": True,
                "streaming": True,
                "images": ("image" in modalities) if modalities is not None else None,
                "reasoning": list(dict.fromkeys(efforts)),
                "web_search": False,
            }))
        return result


async def copilot_models(home, connection, credentials):
    from copilot import CopilotClient, RuntimeConnection

    from .accounts import copilot_runtime

    if connection["auth_type"] not in {"client_login", "api_key", "environment"}:
        raise RuntimeUnavailable("Use original Copilot sign-in or an explicitly supplied GitHub token to discover models.")
    token = credentials.get("api_key") if connection["auth_type"] != "client_login" else None
    if connection["auth_type"] != "client_login" and not token:
        raise RuntimeUnavailable("Add the GitHub token for this connection before discovering Copilot models.")

    async with owned_client(CopilotClient(
        connection=RuntimeConnection.for_stdio(path=str(copilot_runtime(home))),
        working_directory=str(home), base_directory=str(home / "copilot"),
        env=child_environment(home), github_token=token,
        use_logged_in_user=connection["auth_type"] == "client_login",
        enable_remote_sessions=False, log_level="none",
    ), close_method="stop", force_method="force_stop") as client:
        auth = await client.get_auth_status()
        if not auth.isAuthenticated:
            raise RuntimeUnavailable("Complete the original Copilot sign-in before discovering models.")
        models = await client.list_models()
        if len(models) > MAX_MODELS:
            raise RuntimeUnavailable("The Copilot catalog exceeds the supported size. Add the required model manually.")
        result = []
        for model in models:
            if model.policy and model.policy.state == "disabled":
                continue
            result.append(_entry(model.id, model.name, {
                "tools": True,
                # The SDK defaults a missing vision field to False, so only a
                # positive assertion can be distinguished from missing data.
                "images": True if model.capabilities.supports.vision is True else None,
                "context_window": _positive(model.capabilities.limits.max_context_window_tokens),
                "reasoning": list(dict.fromkeys(model.supported_reasoning_efforts or [])),
                "web_search": False,
                # Tool parameter schemas do not establish native final-response
                # schema support; retain the public default None for that field.
            }))
        return result


async def claude_models(connection, credentials):
    import httpx2
    from anthropic import AsyncAnthropic

    if connection["auth_type"] not in {"api_key", "environment"} or not credentials.get("api_key"):
        raise RuntimeUnavailable("Claude Agent model discovery requires this connection's Anthropic API key. Claude subscription credentials are not used.")
    if os.environ.get("ANTHROPIC_CUSTOM_HEADERS"):
        raise RuntimeUnavailable("Remove the service's ANTHROPIC_CUSTOM_HEADERS override before discovering this connection. Its account headers must come from the selected connection only.")
    result = []
    async with httpx2.AsyncClient(timeout=30, follow_redirects=False, trust_env=False) as http:
        async with AsyncAnthropic(
            api_key=credentials["api_key"], webhook_key="",
            base_url=connection.get("base_url") or "https://api.anthropic.com",
            http_client=http, max_retries=0,
        ) as client:
            async for model in client.models.list(limit=1000):
                if len(result) >= MAX_MODELS:
                    raise RuntimeUnavailable("The Claude API catalog exceeds the supported size. Add the required model manually.")
                raw = model.model_dump(mode="json", exclude_unset=True)
                features = raw.get("capabilities") or {}
                effort = features.get("effort") or {}
                levels = [level for level in ("low", "medium", "high", "xhigh", "max")
                          if _boolean(effort.get(level)) is True] if effort.get("supported") is True else []
                result.append(_entry(model.id, model.display_name, {
                    "tools": _boolean(features.get("function_calling")),
                    "structured_output": _boolean(features.get("structured_outputs")),
                    "images": _boolean(features.get("image_input")),
                    "pdf": _boolean(features.get("pdf_input")),
                    "context_window": _positive(raw.get("max_input_tokens")),
                    "max_output_tokens": _positive(raw.get("max_tokens")),
                    "reasoning": levels, "web_search": False,
                }))
    return result


async def discover_runtime_models(home, connection, credentials):
    if connection["protocol"] == "grok_build":
        from .grok_auth import discover_models
        return await discover_models(home, connection)
    if connection["protocol"] == "gemini_cli":
        from .gemini_auth import discover_models
        return await discover_models(home, connection)
    if connection["protocol"] == "codex":
        return await codex_models(home, connection)
    if connection["protocol"] == "copilot":
        return await copilot_models(home, connection, credentials)
    if connection["protocol"] == "claude_agent":
        return await claude_models(connection, credentials)
    raise RuntimeUnavailable("This original client does not expose a supported model catalog to Quantix. Add the exact model ID and its documented capabilities manually.")
