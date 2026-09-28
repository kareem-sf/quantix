"""Prove a model works for office work: it must call a tool with a code only this request contains, and it is
asked to read a number from an image, because some services accept images for models that cannot see them."""

import io
import secrets

from PIL import Image, ImageDraw, ImageFont
from pydantic_ai import Agent, BinaryContent, UsageLimits
from pydantic_ai.models import Model
from pydantic_ai.settings import ModelSettings

from quantix.ai.providers import explain


class _Confirmed(Exception):
    """Ends the check as soon as the tool is called: the model's closing reply proves nothing more."""


CANT_SEE = " It can't read images, so the office won't look at drawings or scans with it."


async def check_model(model: Model) -> tuple[bool, str, bool]:
    """Works for the office, a plain message, and whether it can read images."""
    ok, message = await _uses_tools(model)
    if not ok:
        return False, message, False
    sees = await _reads_images(model)
    return True, message + ("" if sees else CANT_SEE), sees


async def _reads_images(model: Model) -> bool:
    number = str(secrets.randbelow(900) + 100)
    image = Image.new("RGB", (420, 180), "white")
    ImageDraw.Draw(image).text((60, 30), number, fill="black", font=ImageFont.load_default(size=110))
    png = io.BytesIO()
    image.save(png, format="PNG")
    agent = Agent(model, instructions="Read images exactly.", model_settings=ModelSettings(timeout=120.0))
    question = "What number is written in this image? Reply with the digits only."
    try:
        result = await agent.run([question, BinaryContent(png.getvalue(), media_type="image/png")])
    except Exception:  # noqa: BLE001  (a model that refuses images simply can't read them)
        return False
    return number in str(result.output)


async def _uses_tools(model: Model) -> tuple[bool, str]:
    code = secrets.token_hex(3)
    received: list[str] = []

    def confirm(code: str) -> str:
        """Confirm the code you were given."""
        received.append(code)
        raise _Confirmed()

    agent = Agent(
        model,
        instructions="This is a connection check. Call the confirm tool once with the code you are given.",
        tools=[confirm],
        model_settings=ModelSettings(timeout=120.0),
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
