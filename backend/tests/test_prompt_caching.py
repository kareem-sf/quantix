"""The static instructions stay byte-identical across turns so providers can cache them."""

import asyncio

from quantix import ai_direct
from quantix.ai_execution import TURN_CONTEXT_MARKER, execute_api
from quantix.office import INSTRUCTIONS


def test_static_instructions_are_separated_from_the_turn_context(monkeypatch):
    seen = []

    async def capture(self, route, connection, credentials, context, instruction, output_type, **kwargs):
        seen.append(instruction)
        return {"output": {}, "usage": {}}

    monkeypatch.setattr(ai_direct.DirectAPIService, "execute", capture)
    monkeypatch.setattr(ai_direct, "supports_direct", lambda connection: True)
    monkeypatch.setattr(ai_direct.DirectAPIService, "__init__", lambda self, repo: None)

    class Context:
        repo = None

    connection = {"auth_type": "api_key", "protocol": "openai_responses", "provider_id": "openai"}
    for turn in ('{"engineer_request": "What is the bid bond?", "today_utc": "2026-09-13"}',
                 '{"engineer_request": "List every submission form.", "today_utc": "2026-09-14"}'):
        asyncio.run(execute_api({}, connection, {}, Context(), turn, dict, system_instructions=INSTRUCTIONS))

    static = [instruction.partition(TURN_CONTEXT_MARKER)[0] for instruction in seen]
    turns = [instruction.partition(TURN_CONTEXT_MARKER)[2] for instruction in seen]
    assert static[0] == static[1] == INSTRUCTIONS
    assert "bid bond" in turns[0] and "submission form" in turns[1]
