import json
import os
import sys
import threading

import httpx2
from openai import AsyncOpenAI

from pydantic_ai import (
    DeferredToolRequests,
    DeferredToolResults,
    ExternalToolset,
    ToolDefinition,
    ToolFailed,
)
from pydantic_ai.output import StructuredDict

MAX_LINE_BYTES = 8 * 1024 * 1024
OPENAI_COMPATIBLE_BASE_URLS = {
    "openai": None,
    "xai": "https://api.x.ai/v1",
}


class ProtocolError(Exception):
    pass


class Worker:
    def __init__(self, request):
        self.request = request
        self.cancelled = threading.Event()
        self.emitted_bytes = 0
        self.finished = False

    def emit(self, frame):
        if self.finished:
            raise ProtocolError("frame after terminal")
        line = json.dumps(frame, separators=(",", ":"), ensure_ascii=False)
        self.emitted_bytes += len(line.encode("utf-8"))
        if self.emitted_bytes > int(self.request["budgets"]["max_output_bytes"]):
            self.finished = True
            self.write_line(
                {
                    "kind": "failure",
                    "category": "budget",
                    "message": "output exceeded the size limit",
                }
            )
            raise SystemExit(0)
        self.write_line(frame)

    @staticmethod
    def write_line(frame):
        sys.stdout.write(
            json.dumps(frame, separators=(",", ":"), ensure_ascii=False) + "\n"
        )
        sys.stdout.flush()

    def finish(self, frame):
        if self.finished:
            raise ProtocolError("second terminal frame")
        self.emit(frame)
        self.finished = True

    def failure(self, category, message):
        self.finish({"kind": "failure", "category": category, "message": message})


def read_frames(cancelled):
    for raw in sys.stdin:
        if cancelled is not None and cancelled.is_set():
            return
        line = raw.strip()
        if not line:
            continue
        if len(line.encode("utf-8")) > MAX_LINE_BYTES:
            raise ProtocolError("input frame exceeded the size limit")
        yield json.loads(line)


def watchdog(seconds):
    timer = threading.Timer(seconds, os._exit, args=(29,))
    timer.daemon = True
    timer.start()


def build_model(request):
    route = request["route"]
    base_url = request.get("base_url")
    api_key = request["api_key"]
    timeout_ms = int(request["budgets"]["timeout_ms"])
    http_client = httpx2.AsyncClient(
        trust_env=False,
        timeout=httpx2.Timeout(timeout_ms / 1000),
    )
    if route in OPENAI_COMPATIBLE_BASE_URLS:
        from pydantic_ai.models.openai import OpenAIChatModel
        from pydantic_ai.providers.openai import OpenAIProvider

        client = AsyncOpenAI(
            api_key=api_key,
            base_url=base_url or OPENAI_COMPATIBLE_BASE_URLS[route],
            max_retries=0,
            http_client=http_client,
        )
        return OpenAIChatModel(
            request["model_id"],
            provider=OpenAIProvider(openai_client=client),
        )
    if route in ("anthropic", "anthropic_compatible"):
        from anthropic import AsyncAnthropic
        from pydantic_ai.models.anthropic import AnthropicModel
        from pydantic_ai.providers.anthropic import AnthropicProvider

        client = AsyncAnthropic(
            api_key=api_key,
            base_url=base_url,
            max_retries=0,
            http_client=http_client,
        )
        return AnthropicModel(
            request["model_id"],
            provider=AnthropicProvider(anthropic_client=client),
        )
    if route == "google":
        from pydantic_ai.models.google import GoogleModel
        from pydantic_ai.providers.google import GoogleProvider

        return GoogleModel(
            request["model_id"],
            provider=GoogleProvider(api_key=api_key),
        )
    raise ProtocolError(f"unknown route {route}")


def build_model_settings(request):
    reasoning = request.get("reasoning")
    route = request["route"]
    if reasoning is None:
        return None
    if route in ("openai", "xai", "openai_compatible"):
        return {"openai_reasoning_effort": reasoning}
    raise ProtocolError("reasoning control is not supported on this route yet")


def run_probe(worker):
    from pydantic_ai import Agent

    agent = Agent(
        build_model(worker.request),
        model_settings=build_model_settings(worker.request),
        retries=0,
    )
    result = agent.run_sync("Reply with exactly: OK", output_type=[str])
    usage = result.usage
    worker.finish(
        {
            "kind": "result",
            "output": None,
            "text": result.output,
            "usage": {
                "input_tokens": usage.input_tokens,
                "output_tokens": usage.output_tokens,
            },
        }
    )


def run_turn(worker):
    from pydantic_ai import Agent

    request = worker.request
    toolset = ExternalToolset(
        [
            ToolDefinition(
                name=descriptor["name"],
                description=descriptor.get("description"),
                parameters_json_schema=descriptor.get("parameters")
                or {"type": "object", "properties": {}},
            )
            for descriptor in request.get("tools") or []
        ]
    )
    output_type = [str, DeferredToolRequests]
    if request.get("output_schema"):
        output_type = [
            StructuredDict(request["output_schema"], name="quantix_output"),
            DeferredToolRequests,
        ]
    agent = Agent(
        build_model(request),
        instructions=request.get("instructions") or None,
        model_settings=build_model_settings(request),
        retries=0,
    )
    messages = None
    deferred_results = None
    rounds = 0
    max_rounds = int(request["budgets"]["max_tool_rounds"])
    while True:
        if worker.cancelled.is_set():
            worker.failure("protocol", "cancelled")
            return
        run = agent.run_sync(
            None if messages is not None else request["input"],
            message_history=messages,
            deferred_tool_results=deferred_results,
            output_type=output_type,
            toolsets=[toolset],
        )
        if isinstance(run.output, DeferredToolRequests):
            rounds += 1
            if rounds > max_rounds:
                worker.failure("budget", "tool round limit reached")
                return
            messages = run.all_messages()
            deferred_results = gather_results(worker, run.output)
            continue
        usage = run.usage
        worker.emit(
            {
                "kind": "usage",
                "input_tokens": usage.input_tokens,
                "output_tokens": usage.output_tokens,
            }
        )
        structured = bool(request.get("output_schema"))
        worker.finish(
            {
                "kind": "result",
                "output": run.output if structured else None,
                "text": "" if structured else run.output,
                "usage": {
                    "input_tokens": usage.input_tokens,
                    "output_tokens": usage.output_tokens,
                },
            }
        )
        return


def gather_results(worker, requests):
    results = DeferredToolResults()
    pending = {call.tool_call_id: call for call in requests.calls}
    for call in requests.calls:
        worker.emit(
            {
                "kind": "approval_request",
                "tool_call_id": call.tool_call_id,
                "tool_name": call.tool_name,
                "arguments": call.args,
            }
        )
    remaining = set(pending)
    while remaining:
        frame = next(read_frames(worker.cancelled), None)
        if frame is None:
            raise ProtocolError("input ended while tool calls were pending")
        if frame.get("kind") == "cancel":
            worker.cancelled.set()
            raise ProtocolError("cancelled")
        if frame.get("kind") != "approval":
            raise ProtocolError(f"unexpected frame {frame.get('kind')}")
        tool_call_id = frame.get("tool_call_id")
        if tool_call_id not in remaining:
            raise ProtocolError("approval for an unknown tool call")
        remaining.discard(tool_call_id)
        if frame.get("approved"):
            results.calls[tool_call_id] = frame.get("result")
        else:
            results.calls[tool_call_id] = ToolFailed(
                frame.get("denial_message") or "Denied by Quantix"
            )
    return results


def run_operation(request, worker):
    worker.emit({"kind": "ready"})
    if request["op"] == "probe":
        run_probe(worker)
    else:
        run_turn(worker)


def main():
    request = None
    for frame in read_frames(None):
        if request is not None:
            raise ProtocolError("duplicate initialize frame")
        if frame.get("kind") != "initialize":
            raise ProtocolError("first frame must be initialize")
        request = frame
        worker = Worker(request)
        watchdog(int(request["budgets"]["timeout_ms"]) / 1000 + 60)
        try:
            run_operation(request, worker)
        except ProtocolError:
            raise
        except SystemExit:
            raise
        except Exception as error:
            worker.failure("provider", f"{type(error).__name__}: {error}")
        return
    raise ProtocolError("no input")


if __name__ == "__main__":
    try:
        main()
    except ProtocolError as error:
        sys.stderr.write(f"protocol error: {error}\n")
        sys.exit(2)
