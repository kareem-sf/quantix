"""Prove a model works for office work: it must call a tool with a code only this request contains."""

import secrets

from pydantic_ai import Agent, UsageLimits
from pydantic_ai.models import Model

from quantix.ai.providers import explain


async def check_model(model: Model) -> tuple[bool, str]:
    code = secrets.token_hex(3)
    received: list[str] = []

    def confirm(code: str) -> str:
        """Confirm the code you were given."""
        received.append(code)
        return "Confirmed."

    agent = Agent(
        model,
        instructions="This is a connection check. Call the confirm tool once with the code you are given.",
        tools=[confirm],
    )
    try:
        await agent.run(f"The code is {code}.", usage_limits=UsageLimits(request_limit=3))
    except Exception as error:  # noqa: BLE001  (any failure is reported to the engineer in plain words)
        if code in received:
            return True, "Works, including the tools the office needs."
        return False, explain(error)
    if code in received:
        return True, "Works, including the tools the office needs."
    return False, "The model answered but didn't use the tool the office needs. Choose another model."
