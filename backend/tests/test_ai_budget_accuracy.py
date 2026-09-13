"""Metered AI estimates stay upper bounds without charging cached or already counted input twice."""

import pytest
from pydantic_ai.messages import ModelRequest, ModelResponse, TextPart, UserPromptPart
from pydantic_ai.models import ModelRequestParameters
from pydantic_ai.models.function import FunctionModel
from pydantic_ai.usage import RequestUsage

from quantix.ai_api_engine import MeteredModel
from quantix.ai_policy import route_usage_cost

_CONNECTION = {
    "billing": "metered",
    "provider_id": "google",
    "protocol": "google",
    "_model": {"capabilities": {"context_window": 1_000_000}},
}
_PRICES = {"input_per_million": 1.5, "output_per_million": 7.5, "cached_input_per_million": 0.15}
_ROUTE = {"model_id": "synthetic-model", "max_output_tokens": 50, "web_search": False}


def test_cached_input_uses_the_cached_rate_only_when_it_is_priced():
    priced = {"pricing": _PRICES}
    unpriced = {"pricing": {**_PRICES, "cached_input_per_million": None}}

    assert route_usage_cost(_CONNECTION, priced, _ROUTE, 100_000, 1_000, cached_input_tokens=60_000) == pytest.approx(0.0765)
    assert route_usage_cost(_CONNECTION, unpriced, _ROUTE, 100_000, 1_000, cached_input_tokens=60_000) == pytest.approx(0.1575)
    assert route_usage_cost(_CONNECTION, priced, _ROUTE, 100_000, 1_000, cached_input_tokens=200_000) == pytest.approx(0.1575)
    assert route_usage_cost(_CONNECTION, priced, _ROUTE, 100_000, 1_000) == pytest.approx(0.1575)


@pytest.mark.asyncio
async def test_later_requests_reserve_the_counted_prompt_plus_new_bytes():
    reservations = []

    def reply(_messages, _info):
        return ModelResponse(
            parts=[TextPart("ok")],
            usage=RequestUsage(input_tokens=120, output_tokens=5),
            model_name="synthetic-model",
        )

    model = MeteredModel(
        FunctionModel(reply, model_name="synthetic-model"),
        route=_ROUTE,
        connection=_CONNECTION,
        context=None,
        before_request=lambda tokens, _output: reservations.append(tokens) or "reservation",
        on_response=lambda _usage, _reservation: None,
    )
    first = [ModelRequest(parts=[UserPromptPart("x" * 8000)])]
    response = await model.request(first, None, ModelRequestParameters())
    second = [*first, response, ModelRequest(parts=[UserPromptPart("y" * 200)])]
    await model.request(second, None, ModelRequestParameters())

    assert reservations[0] > 8000
    assert 120 < reservations[1] < 2000
