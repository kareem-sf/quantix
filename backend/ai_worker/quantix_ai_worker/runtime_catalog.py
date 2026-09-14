"""Explicit metadata discovery for supported original-client connections."""

from .common import RuntimeUnavailable, owned_client

MAX_MODELS = 2000


def _entry(model_id, display_name, capabilities):
    return {"model_id": model_id, "display_name": display_name, "capabilities": capabilities}


async def codex_models(home, connection):
    from openai_codex import AsyncCodex

    from .codex import codex_config

    if connection["auth_type"] != "client_login":
        raise RuntimeUnavailable(
            "Sign in through the original Codex connection to discover its models. Use the OpenAI API connection for API-key model discovery."
        )
    async with owned_client(AsyncCodex(codex_config(home))) as client:
        account = await client.account()
        account_value = account.model_dump(mode="json", by_alias=True).get("account")
        if not account_value or account_value.get("type") != "chatgpt":
            raise RuntimeUnavailable(
                "Complete the managed ChatGPT sign-in for this Codex connection, then discover its models."
            )
        page = await client.models(include_hidden=False)
        # The pinned public Python wrapper does not expose a cursor argument.
        # Do not misrepresent a first page as a complete catalog.
        if page.next_cursor:
            raise RuntimeUnavailable(
                "Codex returned a paginated catalog that this pinned client cannot finish. Add the required model manually instead."
            )
        if len(page.data) > MAX_MODELS:
            raise RuntimeUnavailable(
                "The Codex catalog exceeds the supported size. Add the required model manually."
            )
        result = []
        for model in page.data:
            if model.hidden:
                continue
            modalities = None
            if "input_modalities" in model.model_fields_set and model.input_modalities is not None:
                modalities = [
                    item.value if hasattr(item, "value") else item
                    for item in model.input_modalities
                ]
            efforts = [
                option.reasoning_effort.value for option in model.supported_reasoning_efforts
            ]
            result.append(
                _entry(
                    model.model,
                    model.display_name,
                    {
                        # Codex documents both tools and per-turn schema output as part
                        # of its agent contract. Other bounds require actual metadata.
                        "tools": True,
                        "structured_output": True,
                        "streaming": True,
                        "images": ("image" in modalities) if modalities is not None else None,
                        "reasoning": list(dict.fromkeys(efforts)),
                        "web_search": False,
                    },
                )
            )
        return result


async def discover_runtime_models(home, connection, credentials):
    if connection["protocol"] == "grok_build":
        from .grok_auth import discover_models

        return await discover_models(home, connection)
    if connection["protocol"] == "codex":
        return await codex_models(home, connection)
    raise RuntimeUnavailable(
        "This original client does not expose a supported model catalog to Quantix. Add the exact model ID and its documented capabilities manually."
    )
