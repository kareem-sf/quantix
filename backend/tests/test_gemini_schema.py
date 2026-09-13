"""Gemini receives schemas without the bounds its API rejects."""

import json

from quantix.ai_api_provider import _model_profile
from quantix.ai_gemini_schema import GeminiSchemaTransformer
from quantix.office_types import OfficeOutput

BOUNDS = {"minLength", "maxLength", "minItems", "maxItems", "minimum", "maximum"}


def _keys(node):
    if isinstance(node, dict):
        for key, value in node.items():
            yield key
            yield from _keys(value)
    elif isinstance(node, list):
        for value in node:
            yield from _keys(value)


def test_bounds_move_into_descriptions():
    schema = {
        "type": "object",
        "properties": {
            "title": {"type": "string", "minLength": 1, "maxLength": 200, "description": "Short title"},
            "tags": {"type": "array", "items": {"type": "string"}, "maxItems": 5},
            "count": {"type": "integer", "minimum": 0, "maximum": 9},
        },
        "required": ["title"],
    }

    result = GeminiSchemaTransformer(schema).walk()

    assert not BOUNDS & set(_keys(result))
    assert result["properties"]["title"]["description"] == "Short title (at least 1 characters; at most 200 characters)"
    assert result["properties"]["tags"]["description"] == "At most 5 items"
    assert result["properties"]["count"]["description"] == "Minimum 0; maximum 9"
    assert result["required"] == ["title"]


def test_manager_output_schema_is_gemini_compatible():
    result = GeminiSchemaTransformer(OfficeOutput.model_json_schema()).walk()

    assert not BOUNDS & set(_keys(result))
    # Structure the API accepted in the isolation run is preserved.
    assert set(result["properties"]) == set(OfficeOutput.model_json_schema()["properties"])
    assert json.dumps(result)


def test_google_routes_use_the_gemini_transformer():
    profile = _model_profile("google", "google", {"model_id": "gemini-3.8-flash"}, "gemini-3.8-flash")

    assert profile["json_schema_transformer"] is GeminiSchemaTransformer
    # Google's own capability flags are kept.
    assert profile["google_supports_thinking_level"] is True
