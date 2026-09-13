"""Explicit MCP capabilities; no database or repository crosses this boundary."""

from contextlib import AsyncExitStack, asynccontextmanager
from dataclasses import dataclass
from pathlib import Path
from urllib.parse import urlsplit

import httpx2
from mcp.client import Client
from mcp.client.streamable_http import streamable_http_client

from .common import RuntimeUnavailable

SUBMIT_TOOL = "quantix_submit_result"


def value_of(result):
    if result.is_error:
        # Only the authenticated Quantix control service supplies this message.
        text = next((part.text for part in result.content if part.type == "text"), "The scoped operation failed.")
        raise RuntimeUnavailable(text[:1200])
    if result.structured_content is None:
        raise RuntimeUnavailable("The local control service returned no structured result.")
    return result.structured_content


async def connect(stack, endpoint, *, timeout=180):
    parsed = urlsplit(endpoint["url"])
    if (parsed.scheme != "http" or parsed.hostname != "127.0.0.1" or not parsed.port
            or parsed.username or parsed.password or parsed.query or parsed.fragment):
        raise RuntimeUnavailable("The scoped control address must be local to Quantix.")
    http = await stack.enter_async_context(httpx2.AsyncClient(
        headers={"Authorization": f"Bearer {endpoint['token']}"},
        timeout=timeout, follow_redirects=False, trust_env=False,
    ))
    return await stack.enter_async_context(Client(
        streamable_http_client(endpoint["url"], http_client=http), cache=None,
        read_timeout_seconds=timeout,
    ))


class RemoteBridge:
    """Only the source endpoint is offered to native client model tools."""

    def __init__(self, endpoint, control):
        self.url = endpoint["url"]
        self.token = endpoint["token"]
        self.names = list(endpoint["names"])
        self.control = control
        self.closed = False

    @asynccontextmanager
    async def serve(self):
        try:
            yield self
        finally:
            self.closed = True

    async def result(self, text=None):
        """Fetch the validated proposal, offering the client's closing message.

        The text is only ever used when the run is the bounded conversation
        pass and no structured proposal was submitted; the control service
        decides, never the client.
        """

        spoken = text.strip() if isinstance(text, str) else ""
        return (await self.control.call("result", {"text": spoken} if spoken else {}))["output"]


class Control:
    def __init__(self, client):
        self.client = client

    async def call(self, name, values):
        return value_of(await self.client.call_tool(name, values))

    async def before_request(self, input_allowance, output_limit, requests=1):
        value = await self.call("reserve", {
            "input_allowance": input_allowance, "output_limit": output_limit, "requests": requests,
        })
        return value["reservation_id"]

    async def on_response(self, usage, reservation_id):
        await self.call("usage", {"usage": usage, "reservation_id": reservation_id})

    async def event(self, kind, message, data):
        await self.call("event", {"kind": kind, "message": message, "data": data})


@dataclass
class ExecutionContext:
    account_home: Path
    operation_id: str
    bridge: RemoteBridge
    source_client: Client
    control: Control
    session_binding: dict | None = None


@asynccontextmanager
async def execution_context(payload):
    async with AsyncExitStack() as stack:
        control = Control(await connect(stack, payload["control"]))
        source_client = await connect(stack, payload["source"], timeout=1900)
        yield ExecutionContext(
            account_home=Path(payload["account_home"]), operation_id=payload["operation_id"],
            bridge=RemoteBridge(payload["source"], control), source_client=source_client, control=control,
            session_binding=payload.get("session_binding"),
        )
