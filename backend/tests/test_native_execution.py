"""Native capability and session authority use synthetic records only."""
import pytest
from pydantic import ValidationError


def test_unknown_optional_price_and_streaming_do_not_change_old_saved_shapes():
    from quantix.ai_models import ModelCapabilities, PriceCard
    assert "streaming" not in ModelCapabilities().model_dump()
    PriceCard(input_per_million=1, output_per_million=2, source="fixture", as_of="2026-09-13")
    with pytest.raises(ValidationError):
        PriceCard(input_per_million=1, output_per_million=2, source="fixture", as_of="2026-09-13", code_execution_per_session=.5)


def test_price_only_edit_preserves_observed_capability_origin(tmp_path):
    from quantix.ai_connections import AIConnectionService
    from quantix.repository import Repository
    service = AIConnectionService(Repository(tmp_path))
    account = service.create({"name": "Synthetic", "provider_id": "openai", "protocol": "openai_responses",
                              "auth_type": "api_key", "credentials": {"api_key": "fixture"}, "session_only": True})
    model = {"model_id": "synthetic-exact", "display_name": "Synthetic", "capabilities": {"tools": True, "images": True}}
    service.save_model(account["id"], model, source="provider")
    price = {"input_per_million": 1, "output_per_million": 2, "source": "fixture", "as_of": "2026-09-12"}
    changed = service.save_model(account["id"], {**model, "pricing": price})
    assert changed["source"] == "provider"
    changed = service.save_model(account["id"], {**model, "capabilities": {"tools": True, "images": False}, "pricing": price})
    assert changed["source"] == "manual"


def test_classification_clears_search_without_changing_engineering_preferences():
    from quantix.ai_api_provider import build_model_settings
    from quantix.conversation import classification_route
    approved = {"model_id": "fixture", "max_output_tokens": 10000, "reasoning": "budget:8192", "web_search": True}
    model = {"capabilities": {"reasoning": ["budget:8192"]}}
    connection = {"provider_id": "anthropic", "protocol": "anthropic", "_model": model}
    classified = classification_route(approved, connection, model)
    assert classified["web_search"] is False
    assert 8192 < classified["max_output_tokens"] <= approved["max_output_tokens"]
    assert approved["web_search"] is True and approved["reasoning"] == "budget:8192"
    assert build_model_settings(classified, connection)["anthropic_thinking"]["budget_tokens"] == 8192
