"""Quantix AI worker main loop.

One operation per process: the host sends an initialize frame, the worker
streams progress frames, and emits exactly one terminal frame. The worker
never executes tools - every host tool call is deferred, relayed to the host,
and resolved with the host's approval verdict and result.
"""

import asyncio
import contextlib
import os
import sys
import threading

from pydantic import ValidationError
from pydantic_ai import (
    Agent,
    CallDeferred,
    DeferredToolRequests,
    DeferredToolResults,
    Tool,
    ToolDenied,
    UsageLimits,
)
from pydantic_ai.capabilities import HandleDeferredToolCalls
from pydantic_ai.exceptions import UsageLimitExceeded, UserError
from pydantic_ai.messages import (
    FunctionToolCallEvent,
    FunctionToolResultEvent,
    PartDeltaEvent,
    PartStartEvent,
    TextPartDelta,
    ThinkingPartDelta,
)
from pydantic_ai.output import StructuredDict

from .models import RouteError, build_model, thinking_capability
from .protocol import ProtocolError, ProtocolOutput, parse_frame

TERMINAL_RESULT = "result"
TERMINAL_FAILURE = "failure"

MAX_TOOLS = 64


class Cancelled(Exception):
    """The host asked the worker to stop."""


class WorkerState:
    def __init__(self, output: ProtocolOutput) -> None:
        self.output = output
        self.approvals: dict[str, asyncio.Future] = {}
        self.cancel_event = asyncio.Event()
        self.protocol_error: str | None = None

    def fail_pending_approvals(self, message: str) -> None:
        for future in self.approvals.values():
            if not future.done():
                future.set_exception(ProtocolError(message))
        self.approvals.clear()


def main() -> int:
    try:
        return asyncio.run(run_worker())
    except ProtocolError:
        return 3
    except Cancelled:
        return 0


def _pump_stdin(loop: asyncio.AbstractEventLoop, frames: asyncio.Queue) -> None:
    buffer = b""
    try:
        while True:
            chunk = os.read(0, 65_536)
            if not chunk:
                break
            buffer += chunk
            while True:
                newline = buffer.find(b"\n")
                if newline < 0:
                    break
                line = buffer[:newline].strip()
                buffer = buffer[newline + 1 :]
                if line:
                    loop.call_soon_threadsafe(frames.put_nowait, ("line", line))
    finally:
        loop.call_soon_threadsafe(frames.put_nowait, ("eof", b""))


async def run_worker() -> int:
    output = ProtocolOutput()
    state = WorkerState(output)
    frames: asyncio.Queue = asyncio.Queue()
    loop = asyncio.get_running_loop()
    threading.Thread(target=_pump_stdin, args=(loop, frames), daemon=True).start()

    try:
        initialize = await _read_initialize(state, frames)
        output.send({"kind": "ready"})
        if state.cancel_event.is_set():
            raise Cancelled()
        router = asyncio.create_task(_route_input(state, frames))
        try:
            if initialize["op"] == "probe":
                await run_operation(state, initialize, probe=True)
            else:
                await run_operation(state, initialize, probe=False)
        finally:
            router.cancel()
            with contextlib.suppress(asyncio.CancelledError):
                await router
        _require_quiet_after_terminal(state, frames)
        return 0
    except Cancelled:
        return 0
    except ProtocolError as error:
        output.send({"kind": "failure", "category": "protocol", "message": str(error)})
        return 3
    except BaseException as error:
        if isinstance(error, Cancelled):
            raise
        category, message = classify_failure(error)
        output.send({"kind": "failure", "category": category, "message": message})
        return 2


def _require_quiet_after_terminal(state: WorkerState, frames: asyncio.Queue) -> None:
    if state.protocol_error:
        raise ProtocolError(state.protocol_error)
    while not frames.empty():
        kind, _payload = frames.get_nowait()
        if kind == "eof":
            return
        if kind == "line":
            raise ProtocolError("host input arrived after the terminal frame")


async def _route_input(state: WorkerState, frames: asyncio.Queue) -> None:
    while True:
        kind, payload = await frames.get()
        if kind == "eof":
            state.protocol_error = "the host closed input during the run"
            state.fail_pending_approvals(state.protocol_error)
            return
        try:
            frame = parse_frame(payload)
            frame_kind = frame["kind"]
            if frame_kind == "cancel":
                state.cancel_event.set()
                state.fail_pending_approvals("The host cancelled the run.")
            elif frame_kind == "approval":
                _resolve_approval(state, frame)
            else:
                raise ProtocolError(f"unexpected host frame kind {frame_kind!r}")
        except ProtocolError as error:
            state.protocol_error = str(error)
            state.fail_pending_approvals(str(error))
            return


def _resolve_approval(state: WorkerState, frame: dict) -> None:
    tool_call_id = frame.get("tool_call_id")
    if not isinstance(tool_call_id, str) or not tool_call_id:
        raise ProtocolError("approval frame is missing its tool_call_id")
    future = state.approvals.get(tool_call_id)
    if future is None or future.done():
        raise ProtocolError("approval frame answered nothing pending")
    if not isinstance(frame.get("approved"), bool):
        raise ProtocolError("approval frame is missing an approved flag")
    if frame["approved"]:
        future.set_result({"approved": True, "result": frame.get("result")})
    else:
        denial = frame.get("denial_message")
        future.set_result(
            {"approved": False, "denial_message": denial if isinstance(denial, str) else None}
        )


async def _await_approval(state: WorkerState, tool_call_id: str) -> dict:
    loop = asyncio.get_running_loop()
    future: asyncio.Future = loop.create_future()
    state.approvals[tool_call_id] = future
    try:
        return await asyncio.shield(future)
    finally:
        state.approvals.pop(tool_call_id, None)


def _validate_initialize(frame: dict) -> None:
    if frame.get("op") not in ("probe", "turn"):
        raise ProtocolError("initialize op must be probe or turn")
    if not isinstance(frame.get("route"), str) or not frame["route"]:
        raise ProtocolError("initialize is missing the provider route")
    if not isinstance(frame.get("api_key"), str) or not frame["api_key"]:
        raise ProtocolError("initialize is missing the API key")
    if not isinstance(frame.get("model_id"), str) or not frame["model_id"]:
        raise ProtocolError("initialize is missing the model id")
    reasoning = frame.get("reasoning")
    if reasoning is not None and reasoning not in ("low", "medium", "high", "xhigh"):
        raise ProtocolError("initialize reasoning must be null, low, medium, high, or xhigh")
    if not isinstance(frame.get("instructions"), str):
        raise ProtocolError("initialize is missing instructions")
    schema = frame.get("output_schema")
    if schema is not None and not isinstance(schema, dict):
        raise ProtocolError("initialize output schema must be an object or null")
    tools = frame.get("tools")
    if not isinstance(tools, list) or len(tools) > MAX_TOOLS:
        raise ProtocolError("initialize tools must be a bounded list")
    for tool in tools:
        if (
            not isinstance(tool, dict)
            or not isinstance(tool.get("name"), str)
            or not tool["name"]
            or not isinstance(tool.get("description"), str)
            or not isinstance(tool.get("parameters"), dict)
        ):
            raise ProtocolError("a host tool definition is malformed")
    budgets = frame.get("budgets")
    if not isinstance(budgets, dict):
        raise ProtocolError("initialize is missing budgets")
    for key in ("max_tool_rounds", "max_output_bytes", "timeout_ms"):
        value = budgets.get(key)
        if not isinstance(key, str) or not isinstance(budgets.get(key), int) or value < 1:
            raise ProtocolError(f"initialize budget {key} is invalid")
    if frame.get("mode") not in ("gated", "autonomous"):
        raise ProtocolError("initialize mode must be gated or autonomous")
    if not isinstance(frame.get("input"), str):
        raise ProtocolError("initialize is missing the run input")


def host_tools(tools: list[dict]) -> list:
    registered = []
    for spec in tools:

        def deferred(_ctx, **kwargs):
            raise CallDeferred

        registered.append(
            Tool.from_schema(
                function=deferred,
                name=spec["name"],
                description=spec["description"],
                json_schema=spec["parameters"],
                takes_ctx=True,
            )
        )
    return registered


def _host_tool_relay(state: WorkerState):
    async def handle(_ctx, requests: DeferredToolRequests) -> DeferredToolResults:
        results = DeferredToolResults()
        for call in list(requests.calls) + list(requests.approvals):
            arguments = call.args if isinstance(call.args, dict) else {}
            state.output.send(
                {
                    "kind": "approval_request",
                    "tool_call_id": call.tool_call_id,
                    "tool_name": call.tool_name,
                    "arguments": arguments,
                }
            )
            _emit_event(
                state,
                "tool_call_started",
                tool_call_id=call.tool_call_id,
                tool_name=call.tool_name,
            )
            verdict = await _await_approval(state, call.tool_call_id)
            if verdict["approved"]:
                results.calls[call.tool_call_id] = verdict.get("result")
            else:
                denial = verdict.get("denial_message")
                results.calls[call.tool_call_id] = ToolDenied(
                    denial or "Quantix denied this tool call."
                )
        return results

    return handle


def build_agent(state: WorkerState, initialize: dict) -> Agent:
    route = initialize["route"]
    schema = initialize.get("output_schema")
    output_type: object = StructuredDict(schema) if isinstance(schema, dict) else str

    if os.environ.get("QUANTIX_AI_WORKER_TEST_MODEL") == "1":
        from pydantic_ai.models.test import TestModel

        model = TestModel()
    else:
        model = build_model(
            route,
            initialize.get("base_url"),
            initialize["api_key"],
            initialize["model_id"],
            initialize.get("reasoning"),
            max(1, int(initialize["budgets"]["timeout_ms"])) / 1000,
        )

    return Agent(
        model,
        instructions=initialize["instructions"],
        output_type=output_type,
        retries=0,
        capabilities=[
            *thinking_capability(route, initialize.get("reasoning")),
            HandleDeferredToolCalls(handler=_host_tool_relay(state)),
        ],
        tools=host_tools(initialize.get("tools") or []),
    )


def _emit_event(state: WorkerState, event: str, **fields) -> None:
    state.output.send({"kind": "event", "event": event, **fields})


class RunTracker:
    def __init__(self, state: WorkerState) -> None:
        self.state = state
        self.text_parts: list[str] = []
        self.saw_streaming = False
        self.saw_reasoning = False
        self.saw_tool_round = False

    def text(self) -> str:
        return "".join(self.text_parts)


async def run_operation(state: WorkerState, initialize: dict, probe: bool) -> None:
    budgets = initialize["budgets"]
    timeout_seconds = max(1, int(budgets["timeout_ms"])) / 1000
    max_tool_rounds = max(1, int(budgets["max_tool_rounds"]))

    state.output.send({"kind": "event", "event": "turn_started"})
    tracker = RunTracker(state)
    agent = build_agent(state, initialize)

    try:
        async with asyncio.timeout(timeout_seconds):
            async with agent.iter(
                initialize["input"],
                usage_limits=UsageLimits(request_limit=max_tool_rounds + 1),
            ) as run:
                async for node in run:
                    await _walk_node(state, tracker, run, node)
                output_value = run.result.output
                usage_value = run.usage
    except TimeoutError as error:
        raise TimeoutError("The provider turn exceeded its time budget.") from error

    input_tokens = int(getattr(usage_value, "input_tokens", 0) or 0)
    output_tokens = int(getattr(usage_value, "output_tokens", 0) or 0)
    state.output.send(
        {"kind": "usage", "input_tokens": input_tokens, "output_tokens": output_tokens}
    )

    if isinstance(output_value, str):
        text = tracker.text() or output_value
        structured: object | None = None
    else:
        text = tracker.text()
        structured = _json_safe(output_value)

    if probe:
        observed = {
            "streamed": tracker.saw_streaming,
            "structured_output": structured is not None,
            "tool_round_completed": tracker.saw_tool_round,
        }
    else:
        observed = structured

    state.output.send(
        {
            "kind": TERMINAL_RESULT,
            "output": observed,
            "text": text,
            "usage": {"input_tokens": input_tokens, "output_tokens": output_tokens},
        }
    )


def _json_safe(value):
    if isinstance(value, dict):
        return {str(key): _json_safe(item) for key, item in value.items()}
    if isinstance(value, (list, tuple)):
        return [_json_safe(item) for item in value]
    if value is None or isinstance(value, (bool, int, float, str)):
        return value
    return str(value)


async def _walk_node(state: WorkerState, tracker: RunTracker, run, node) -> None:
    if Agent.is_model_request_node(node):
        async with node.stream(run.ctx) as stream:
            async for event in stream:
                tracker.saw_streaming = True
                _track_stream_event(state, tracker, event)
    elif Agent.is_call_tools_node(node):
        async with node.stream(run.ctx) as stream:
            async for event in stream:
                _track_call_event(state, tracker, event)


def _track_stream_event(state: WorkerState, tracker: RunTracker, event) -> None:
    from pydantic_ai.messages import PartDeltaEvent, PartStartEvent, TextPartDelta

    if isinstance(event, PartDeltaEvent) and isinstance(event.delta, TextPartDelta):
        if event.delta.content_delta:
            tracker.text_parts.append(event.delta.content_delta)
            _emit_event(state, "text_delta", text=event.delta.content_delta)
    elif ThinkingPartDelta is not None and isinstance(
        event, PartDeltaEvent
    ) and isinstance(event.delta, ThinkingPartDelta):
        if event.delta.content_delta:
            tracker.saw_reasoning = True
            _emit_event(state, "reasoning_delta", text=event.delta.content_delta)
    elif isinstance(event, PartStartEvent):
        content = getattr(event.part, "content", None)
        if isinstance(content, str) and content:
            tracker.text_parts.append(content)
            _emit_event(state, "text_delta", text=content)


def _track_call_event(state: WorkerState, tracker: RunTracker, event) -> None:
    if isinstance(event, FunctionToolCallEvent):
        tracker.saw_tool_round = True
        part = event.part
        _emit_event(
            state,
            "tool_call_started",
            tool_call_id=part.tool_call_id,
            tool_name=part.tool_name,
        )
    elif isinstance(event, FunctionToolResultEvent):
        part = event.part
        tool_call_id = getattr(part, "tool_call_id", None)
        _emit_event(
            state,
            "tool_call_completed",
            tool_call_id=tool_call_id or "",
            tool_name=getattr(part, "tool_name", "") or "",
        )


def classify_failure(error: BaseException) -> tuple[str, str]:
    if isinstance(error, Cancelled):
        raise error
    if isinstance(error, (UserError, RouteError)):
        return "protocol", _failure_message(error)
    if isinstance(error, UsageLimitExceeded):
        return "budget", _failure_message(error)
    if isinstance(error, ValidationError):
        return "invalid_output", _failure_message(error)
    if isinstance(error, (asyncio.TimeoutError, TimeoutError)):
        return "network", _failure_message(error)
    status = _http_status(error)
    if status is not None:
        if status in (401, 403):
            return "auth", _failure_message(error)
        if status == 429:
            return "rate_limited", _failure_message(error)
        if 400 <= status < 500:
            return "invalid_output", _failure_message(error)
        return "network", _failure_message(error)
    if isinstance(error, OSError):
        return "network", _failure_message(error)
    return "provider", _failure_message(error)


def _failure_message(error: BaseException) -> str:
    text = str(error).strip() or type(error).__name__
    return _redact(text)[:2_000]


def _redact(message: str) -> str:
    return " ".join(
        "[redacted]" if token.lower().startswith(("sk-", "key=")) else token
        for token in message.split(" ")
    )


def _http_status(error: BaseException) -> int | None:
    status = getattr(error, "status_code", None)
    if isinstance(status, int):
        return status
    response = getattr(error, "response", None)
    status = getattr(response, "status_code", None)
    if isinstance(status, int):
        return status
    return None


async def _read_initialize(state: WorkerState, frames: asyncio.Queue) -> dict:
    while True:
        kind, payload = await frames.get()
        if kind == "eof":
            raise ProtocolError("the host closed input before initialize")
        try:
            frame = parse_frame(payload)
        except ProtocolError as error:
            raise ProtocolError(str(error)) from error
        if frame["kind"] == "cancel":
            raise Cancelled()
        if frame["kind"] != "initialize":
            raise ProtocolError(f"expected initialize, received {frame['kind']!r}")
        _validate_initialize(frame)
        return frame
