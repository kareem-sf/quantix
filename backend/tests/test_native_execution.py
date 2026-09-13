"""Native capability and session authority use synthetic records only."""
import pytest
from pydantic import ValidationError


def test_native_selection_requires_documented_cost_and_exact_uploads():
    from quantix.ai_native_tools import ReviewedNativeTools
    with pytest.raises(ValidationError):
        ReviewedNativeTools(native_tools=["code_execution"])
    with pytest.raises(ValidationError):
        ReviewedNativeTools(native_tools=["web_fetch"], web_fetch_domains=["127.0.0.1"])
    assert ReviewedNativeTools().native_tools == []


def test_unknown_optional_price_and_streaming_do_not_change_old_saved_shapes():
    from quantix.ai_models import ModelCapabilities, PriceCard
    assert "streaming" not in ModelCapabilities().model_dump()
    card = PriceCard(input_per_million=1, output_per_million=2, source="fixture", as_of="2026-09-13")
    assert "code_execution_per_session" not in card.model_dump()
    with pytest.raises(ValidationError):
        PriceCard(input_per_million=1, output_per_million=2, source="fixture", as_of="2026-09-13", code_execution_per_session=.5)


def test_hosted_code_cost_is_separate_and_included_in_root_request_reservation():
    from quantix.ai_policy import route_request_cost
    model = {"pricing": {"input_per_million": 1, "output_per_million": 2,
        "code_execution_per_session": .25, "code_execution_source": "https://example.com/prices", "code_execution_as_of": "2026-09-12"}}
    route = {"native_tools": ["code_execution"], "max_native_tool_calls": 2}
    assert float(route_request_cost({"billing": "metered"}, model, route, 1000, 1000)) == pytest.approx(.503)


def test_price_only_edit_preserves_observed_capability_origin(tmp_path):
    from quantix.ai_connections import AIConnectionService
    from quantix.repository import Repository
    service = AIConnectionService(Repository(tmp_path))
    account = service.create({"name": "Synthetic", "provider_id": "openai", "protocol": "openai_responses",
                              "auth_type": "api_key", "credentials": {"api_key": "fixture"}, "session_only": True})
    model = {"model_id": "synthetic-exact", "display_name": "Synthetic", "capabilities": {"tools": True, "code_execution": True}}
    service.save_model(account["id"], model, source="provider")
    price = {"input_per_million": 1, "output_per_million": 2, "source": "fixture", "as_of": "2026-09-12"}
    changed = service.save_model(account["id"], {**model, "pricing": price})
    assert changed["source"] == "provider"
    changed = service.save_model(account["id"], {**model, "capabilities": {"tools": True, "code_execution": False}, "pricing": price})
    assert changed["source"] == "manual"


def test_classification_clears_native_tools_without_changing_engineering_preferences():
    from quantix.ai_api_provider import build_model_settings
    from quantix.conversation import classification_route
    approved = {"model_id": "fixture", "max_output_tokens": 10000, "reasoning": "budget:8192", "native_tools": ["code_execution"], "web_search": True}
    model = {"capabilities": {"reasoning": ["budget:8192"]}}
    connection = {"provider_id": "anthropic", "protocol": "anthropic", "_model": model}
    classified = classification_route(approved, connection, model)
    assert classified["native_tools"] == [] and classified["web_search"] is False
    assert 8192 < classified["max_output_tokens"] <= approved["max_output_tokens"]
    assert approved["native_tools"] == ["code_execution"] and approved["reasoning"] == "budget:8192"
    assert build_model_settings(classified, connection)["anthropic_thinking"]["budget_tokens"] == 8192
