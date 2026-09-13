"""Generation authority and SDK mapping; never calls a provider."""
from types import SimpleNamespace

import pytest
from pydantic import ValidationError


def account(protocol="openai_responses", provider="openai", **caps):
    return {"id": "account", "revision": 2, "protocol": protocol, "provider_id": provider,
            "billing": "metered", "_model": {"model_id": "exact-test-model", "source": "provider",
            "capabilities": {"tools": True, **caps}}}


def test_generation_is_strict_and_sampling_bounds():
    from quantix.ai_generation_models import GenerationSettings
    for settings in ({"temperature": True}, {"temperature": 2.1}, {"top_p": -0.1},
                     {"top_p": float("nan")}, {"max_output_tokens": "1000"},
                     {"api_key": "secret"}, {"native_tools": ["shell"]}):
        with pytest.raises(ValidationError):
            GenerationSettings.model_validate(settings)
    assert GenerationSettings().temperature is None


def test_explicit_sampling_reaches_sdk_and_absent_preserves_default():
    from quantix.ai_api_provider import build_model_settings
    connection = account(temperature=True, top_p=True)
    settings = build_model_settings({"model_id": "exact-test-model", "temperature": .4, "top_p": .8}, connection)
    assert settings["temperature"] == .4 and settings["top_p"] == .8
    assert "temperature" not in build_model_settings({"model_id": "exact-test-model"}, connection)


def test_unknown_explicit_sampling_fails_instead_of_silently_dropping():
    from quantix.ai_api_provider import build_model_settings
    with pytest.raises(ValueError, match="temperature"):
        build_model_settings({"model_id": "exact-test-model", "temperature": .4}, account())


def test_hosted_code_cannot_be_enabled_by_model_metadata_or_route():
    from quantix.ai_generation import native_tools_for
    with pytest.raises(ValueError, match="code"):
        native_tools_for({"native_tools": ["code_execution"]}, account(code_execution=True))


def test_client_sampling_is_explicitly_unavailable():
    from quantix.ai_generation import validate_generation
    with pytest.raises(ValueError, match="temperature"):
        validate_generation({"temperature": .4}, account("codex", "codex", temperature=True))


def test_preview_never_exposes_connection_private_fields():
    from quantix.ai_generation import generation_preview
    connection = account(temperature=True)
    connection.update(credentials={"api_key": "secret"}, base_url="https://private.example")
    result = generation_preview(connection, {"temperature": .5}).model_dump_json()
    assert "secret" not in result and "private.example" not in result
    assert '"effective"' in result and '"temperature":0.5' in result


def test_web_fetch_requires_reviewed_domain_scope_and_rejects_native_output():
    from quantix.ai_generation import native_tools_for, validate_generation
    connection = account("anthropic", "anthropic", web_fetch=True, structured_output=True)
    with pytest.raises(ValueError, match="domain"):
        native_tools_for({"native_tools": ["web_fetch"]}, connection)
    connection["_native_tool_grant"] = fetch_grant()
    tools = native_tools_for({"native_tools": ["web_fetch"], "max_native_tool_calls": 2}, connection,
                             context=SimpleNamespace(tender_id="tender", run_id="run"))
    assert tools[0].max_uses == 2 and tools[0].allowed_domains == ["example.com"]
    with pytest.raises(ValueError, match="native output"):
        validate_generation({"native_tools": ["web_fetch"], "output_mode": "native"}, connection)


def fetch_grant():
    return {"approval_id": "review", "tender_id": "tender", "run_id": "run", "connection_id": "account",
            "connection_revision": 2, "model_id": "exact-test-model", "source_scope_fingerprint": "scope",
            "native_tools": ["web_fetch"], "web_fetch_domains": ["example.com"], "max_calls_per_request": 2}


@pytest.mark.parametrize("change", [{"run_id": "other"}, {"connection_revision": 3}, {"model_id": "other"},
                                   {"native_tools": []}, {"max_calls_per_request": 1}])
def test_native_fetch_grant_must_match_current_request(change):
    from quantix.ai_generation import native_tools_for
    connection = account("anthropic", "anthropic", web_fetch=True)
    connection["_native_tool_grant"] = {**fetch_grant(), **change}
    with pytest.raises(ValueError, match="grant"):
        native_tools_for({"native_tools": ["web_fetch"], "max_native_tool_calls": 2}, connection,
                         context=SimpleNamespace(tender_id="tender", run_id="run"))


def test_new_search_selection_retains_existing_budget_flag():
    from quantix.ai_models import AIRoute
    route = AIRoute(connection_id="a", model_id="exact", native_tools=["web_search"])
    assert route.web_search is True


def test_absent_controls_preserve_historical_route_identity():
    from quantix.ai_models import AIRoute
    from quantix.ai_policy import fingerprint
    historical = {"connection_id": "account", "model_id": "exact", "reasoning": None,
                  "max_output_tokens": 8192, "web_search": False, "max_search_calls": 3}
    normalized = AIRoute.model_validate(historical).model_dump()
    assert fingerprint(normalized) == fingerprint(historical)
    assert AIRoute.model_validate({**historical, "temperature": .5}).model_dump()["temperature"] == .5


def test_sdk_sampling_rejection_is_not_a_silent_default():
    from quantix.ai_generation import validate_sdk_settings
    with pytest.raises(ValueError, match="reasoning"):
        validate_sdk_settings({"temperature": .2}, {"openai_supports_reasoning": True,
                              "openai_reasoning_enabled_by_default": True})
    with pytest.raises(ValueError, match="sampling"):
        validate_sdk_settings({"top_p": .8}, {"anthropic_disallows_sampling_settings": True})


def test_output_modes_use_actual_sdk_wrappers():
    from pydantic import BaseModel
    from pydantic_ai.output import NativeOutput, PromptedOutput, ToolOutput

    from quantix.ai_generation import output_for
    from quantix.ai_generation_models import GenerationSettings
    class Result(BaseModel):
        value: int
    for mode, wrapper in (("auto", ToolOutput), ("tool", ToolOutput), ("native", NativeOutput), ("prompted", PromptedOutput)):
        assert isinstance(output_for(GenerationSettings(output_mode=mode), Result), wrapper)


def test_openai_search_uses_provider_enforced_total_native_call_limit():
    from quantix.ai_api_provider import build_model_settings
    settings = build_model_settings({"web_search": True, "max_search_calls": 2}, account(web_search=True))
    assert settings["extra_body"]["max_tool_calls"] == 2


@pytest.mark.parametrize("protocol,provider", [("google", "google"), ("openai_responses", "xai")])
@pytest.mark.parametrize("legacy", [True, False])
def test_unbounded_native_search_is_rejected_even_for_legacy_route_flag(protocol, provider, legacy):
    from quantix.ai_generation import validate_generation
    route = {"web_search": True} if legacy else {"native_tools": ["web_search"]}
    with pytest.raises(ValueError, match="enforce|call limit"):
        validate_generation(route, account(protocol, provider, web_search=True))


@pytest.mark.parametrize("tools", [[], ["web_search"], ["web_search", "code_execution"]])
def test_openai_combined_limit_never_exceeds_either_reviewed_ceiling(tools):
    from quantix.ai_api_provider import build_model_settings
    from quantix.ai_generation import generation_preview
    connection = account(web_search=True, code_execution=True)
    route = {"web_search": True, "native_tools": tools, "max_search_calls": 7, "max_native_tool_calls": 2}
    assert build_model_settings(route, connection)["extra_body"]["max_tool_calls"] == 2
    preview = generation_preview(connection, route)
    limit = next(item for item in preview.capabilities if item.id == "native_call_limit")
    assert "2" in limit.detail and "shared" in limit.detail


def test_google_fetch_describes_native_restriction_and_distinct_shared_reader():
    from quantix.ai_generation import descriptors
    capabilities = {item.id: item for item in descriptors(account("google", "google", web_fetch=True))}
    assert capabilities["web_fetch"].support == "supported"
    assert capabilities["web_fetch"].runtime_supported is False
    assert "URL Context" in capabilities["web_fetch"].detail
    assert "allowed domains" in capabilities["web_fetch"].detail
    assert "https://ai.google.dev/gemini-api/docs/url-context" in capabilities["web_fetch"].evidence
    assert capabilities["fetch_public_url"].origin == "quantix"


@pytest.mark.parametrize("legacy", [True, False])
def test_anthropic_mixed_native_tools_do_not_silently_allocate_independent_limits(legacy):
    from quantix.ai_generation import validate_generation
    route = {"native_tools": ["web_fetch"] if legacy else ["web_fetch", "web_search"],
             "web_search": legacy, "max_search_calls": 2, "max_native_tool_calls": 5}
    with pytest.raises(ValueError, match="combined limit.*Select one"):
        validate_generation(route, account("anthropic", "anthropic", web_search=True, web_fetch=True))


@pytest.mark.asyncio
@pytest.mark.parametrize("protocol,provider", [("google", "google"), ("openai_responses", "xai")])
async def test_uncapped_search_denied_before_model_transport_or_budget_reservation(monkeypatch, protocol, provider):
    from pydantic import BaseModel

    from quantix.ai_api_engine import run_model
    class Output(BaseModel):
        summary: str
    def forbidden(*args, **kwargs):
        pytest.fail("Unbounded native search reached model transport or spending admission")
    monkeypatch.setattr("quantix.ai_api_engine.model_for_route", forbidden)
    with pytest.raises(ValueError, match="enforce|call limit"):
        await run_model({"model_id": "exact-test-model", "max_output_tokens": 1024,
                         "web_search": True, "native_tools": [], "max_search_calls": 1},
                        account(protocol, provider, web_search=True), {}, None, "Search", Output,
                        definitions=[], before_request=forbidden, on_response=forbidden)


@pytest.mark.asyncio
async def test_reported_search_calls_above_shared_native_cap_are_withheld_and_uncertain():
    from pydantic_ai.messages import ModelRequest, ModelResponse, NativeToolCallPart, UserPromptPart
    from pydantic_ai.models import ModelRequestParameters
    from pydantic_ai.models.function import FunctionModel
    from pydantic_ai.usage import RequestUsage

    from quantix.ai_api_engine import MeteredModel

    reported = []
    def response(messages, info):
        return ModelResponse(parts=[NativeToolCallPart(provider_name="openai", tool_name="web_search",
            tool_call_id=str(index), args={"query": "synthetic"}) for index in range(2)],
            model_name="exact-test-model", usage=RequestUsage(input_tokens=100, output_tokens=20))
    model = MeteredModel(FunctionModel(response, model_name="exact-test-model"),
        route={"model_id": "exact-test-model", "web_search": True, "max_output_tokens": 1024,
               "max_search_calls": 7, "max_native_tool_calls": 1},
        connection=account(web_search=True, context_window=2000), context=None,
        before_request=lambda *_: "held", on_response=lambda usage, _: reported.append(usage))
    with pytest.raises(ValueError, match="exceeded.*native-call"):
        await model.request([ModelRequest(parts=[UserPromptPart("Search")])], {}, ModelRequestParameters())
    assert len(reported) == 1 and reported[0]["usage_complete"] is False
    assert reported[0]["native_call_limit"] == 1 and reported[0]["native_tool_calls"] == 2
    assert reported[0]["native_call_limit_overrun"] is True


@pytest.mark.asyncio
@pytest.mark.parametrize("mode", ["auto", "tool", "native", "prompted"])
async def test_real_agent_loop_applies_settings_validates_output_and_records_each_request(monkeypatch, mode):
    from contextlib import asynccontextmanager

    from pydantic import BaseModel
    from pydantic_ai.messages import ModelResponse, TextPart, ToolCallPart
    from pydantic_ai.models.function import FunctionModel
    from pydantic_ai.usage import RequestUsage

    from quantix.ai_api_engine import run_model
    from quantix.ai_api_provider import APIModelBinding, build_model_settings

    class Result(BaseModel):
        value: int

    observed, admitted, reported = [], [], []
    def response(messages, info):
        observed.append(info.model_settings)
        parts = ([ToolCallPart(info.output_tools[0].name, {"value": 42})]
                 if info.output_tools else [TextPart('{"value":42}')])
        return ModelResponse(parts=parts, model_name="exact-test-model", usage=RequestUsage(input_tokens=100, output_tokens=10))

    @asynccontextmanager
    async def binding(route, connection, credentials):
        yield APIModelBinding(FunctionModel(response, model_name="exact-test-model",
                              profile={"supports_json_schema_output": True}),
                              build_model_settings(route, connection), None)

    monkeypatch.setattr("quantix.ai_api_engine.model_for_route", binding)
    result = await run_model({"model_id": "exact-test-model", "max_output_tokens": 1024, "temperature": .4, "output_mode": mode},
        account(temperature=True, structured_output=True), {}, None, "Return 42.", Result, definitions=[],
        before_request=lambda count, limit: admitted.append((count, limit)) or "reservation",
        on_response=lambda usage, reservation: reported.append((usage, reservation)))
    assert result["output"].value == 42
    assert observed[0]["temperature"] == .4 and observed[0]["max_tokens"] == 1024
    assert len(admitted) == len(reported) == 1 and admitted[0][1] == 1024
    assert reported[0][1] == "reservation" and reported[0][0]["output_tokens"] == 10


def test_generation_preview_endpoint_is_source_backed_and_does_not_change_account(tmp_path):
    from fastapi import FastAPI
    from fastapi.testclient import TestClient

    from quantix.ai_connections import AIConnectionService
    from quantix.ai_generation_routes import create_router
    from quantix.repository import Repository
    repo = Repository(tmp_path)
    connections = AIConnectionService(repo)
    saved = connections.create({"name": "Synthetic", "provider_id": "openai", "protocol": "openai_responses",
                                "auth_type": "api_key", "credentials": {"api_key": "fixture-secret"}, "session_only": True})
    connections.store_discovered_models(saved["id"], [{"model_id": "exact-test-model", "display_name": "Exact fixture",
                                       "capabilities": {"temperature": True}}], saved["revision"])
    before = connections.get(saved["id"])
    app = FastAPI()
    app.include_router(create_router(repo))
    with TestClient(app) as client:
        response = client.post(f'/api/ai/connections/{saved["id"]}/generation-preview',
                               json={"model_id": "exact-test-model", "settings": {"temperature": .4}})
        assert response.status_code == 200
        assert response.json()["effective"]["temperature"] == .4
        assert "fixture-secret" not in response.text and "credentials" not in response.text
        assert connections.get(saved["id"]) == before
