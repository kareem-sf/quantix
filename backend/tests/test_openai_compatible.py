"""Any endpoint that speaks the OpenAI API: address, catalog, thinking, search."""

import pytest


def test_candidate_addresses_stay_on_the_same_site():
    from quantix.ai_api_endpoint import candidate_base_urls

    assert candidate_base_urls("https://runware.ai") == [
        "https://runware.ai",
        "https://runware.ai/v1",
        "https://api.runware.ai",
        "https://api.runware.ai/v1",
    ]
    assert candidate_base_urls("https://api.openai.com/v1") == ["https://api.openai.com/v1"]
    assert all(".example.net" not in url for url in candidate_base_urls("https://example.com"))


def test_a_website_is_not_mistaken_for_an_api():
    from quantix.ai_api_endpoint import answers_like_api

    assert answers_like_api(200, "application/json", '{"data": []}')
    # A JSON refusal to serve a model list without a key is an API.
    assert answers_like_api(401, "application/json", '{"error": {"message": "no key"}}')
    assert not answers_like_api(200, "text/html; charset=utf-8", "<!DOCTYPE html><html>")
    assert not answers_like_api(404, "text/html", "<!DOCTYPE html>")
    assert not answers_like_api(200, "application/json", '{"page": "models"}')


@pytest.mark.asyncio
async def test_a_pasted_website_is_saved_as_the_api_address(monkeypatch):
    from quantix.ai_models import ConnectionInput
    from quantix.ai_setup import AISetupService

    async def found(value, **_):
        return "https://api.runware.ai/v1" if "runware" in value else None

    monkeypatch.setattr("quantix.ai_api_endpoint.resolve_api_base_url", found)
    values = ConnectionInput(
        name="Runware",
        provider_id="custom",
        protocol="openai_chat",
        base_url="https://runware.ai",
        billing="unknown",
    )
    assert await AISetupService._api_address(values) == "https://api.runware.ai/v1"
    # An address Quantix cannot confirm is kept exactly as the engineer entered it.
    own = values.model_copy(update={"base_url": "https://ai.mycompany.example/v1"})
    assert await AISetupService._api_address(own) == "https://ai.mycompany.example/v1"
    # Editing a saved account without supplying a key never moves its address.
    saved = {"base_url": "https://runware.ai"}
    assert await AISetupService._api_address(values, saved=saved) == "https://runware.ai"
    with_key = values.model_copy(update={"credentials": {"api_key": "secret"}})
    assert await AISetupService._api_address(with_key, saved=saved) == "https://api.runware.ai/v1"


def test_provider_model_records_carry_their_own_prices_and_limits():
    from quantix.ai_api_catalog import model_entry

    entry = model_entry(
        {
            "id": "openai-gpt-5-4",
            "name": "OpenAI GPT-5.4",
            "context_length": 400000,
            "max_output_tokens": 128000,
            "input_modalities": ["text", "image", "file"],
            "pricing": {
                "prompt": "0.00000125",
                "completion": "0.00001",
                "input_cache_read": "0.000000125",
            },
        },
        "openai_chat",
    )
    assert entry["display_name"] == "OpenAI GPT-5.4"
    capabilities = entry["capabilities"]
    assert capabilities["context_window"] == 400000
    assert capabilities["max_output_tokens"] == 128000
    assert capabilities["images"] is True and capabilities["pdf"] is True
    assert entry["pricing"]["input_per_million"] == 1.25
    assert entry["pricing"]["output_per_million"] == 10
    assert entry["pricing"]["cached_input_per_million"] == 0.125


def test_listed_request_fields_establish_thinking_and_search():
    from quantix.ai_api_catalog import model_entry

    capabilities = model_entry(
        {
            "id": "x",
            "supported_parameters": ["tools", "reasoning", "web_search_options", "response_format"],
        },
        "openai_chat",
    )["capabilities"]
    assert capabilities["reasoning"] == ["low", "medium", "high"]
    assert capabilities["web_search"] is True
    assert capabilities["tools"] is True and capabilities["structured_output"] is True


def test_model_profiles_fill_in_capabilities_only_for_the_model_s_own_vendor():
    from quantix.ai_catalog import documented_capabilities

    assert "minimal" in documented_capabilities("openai", "openai_chat", "gpt-5-mini")["reasoning"]
    # A gateway serving the same family applies its own rules, so nothing about
    # the original model is carried over to it.
    assert documented_capabilities("custom", "openai_chat", "gpt-5-mini") == {}
    assert documented_capabilities("custom", "openai_chat", "llama-3.3-70b") == {}


def test_thinking_effort_is_offered_but_never_chosen_for_an_unknown_endpoint():
    from quantix.ai_thinking import recommended_level, thinking_levels

    connection = {
        "provider_id": "custom",
        "protocol": "openai_chat",
        "auth_type": "api_key",
        "billing": "unknown",
    }
    assert thinking_levels(connection, {"capabilities": {}}) == ["low", "medium", "high"]
    assert recommended_level(connection, {"capabilities": {}}) is None
    recorded = {"capabilities": {"reasoning": ["minimal", "low", "medium", "high"]}}
    assert recommended_level(connection, recorded) == "medium"


def test_hosted_search_is_carried_only_where_the_model_records_it():
    from pydantic_ai.native_tools import WebSearchTool

    from quantix.ai_api_provider import _model_profile

    route = {"model_id": "some-model"}
    with_search = _model_profile("custom", "openai_chat", route, "some-model", {"web_search": True})
    assert with_search.get("openai_chat_supports_web_search") is True
    assert WebSearchTool in with_search.get("supported_native_tools")
    without = _model_profile("custom", "openai_chat", route, "some-model", {})
    assert without.get("openai_chat_supports_web_search") is False
    assert not without.get("supported_native_tools")


def test_a_gateway_may_report_the_same_model_with_its_own_punctuation():
    from quantix.ai_api_provider import same_reported_model

    assert same_reported_model("custom", "openai_chat", "openai-gpt-5-4", "openai:gpt@5.4")
    assert same_reported_model("custom", "openai_chat", "deepseek-v4", "deepseek/deepseek-v4")
    assert not same_reported_model("custom", "openai_chat", "gpt-5-mini", "gpt-5-nano")
    # A first-party provider is held to its exact published identifier.
    assert not same_reported_model("openai", "openai_chat", "gpt-5-mini", "gpt:5@mini")
