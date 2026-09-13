"""The complete Pydantic AI loop runs inside the isolated provider process."""

import base64
import dataclasses
import json
from datetime import UTC, datetime
from urllib.parse import urlsplit

from .common import RuntimeUnavailable
from .remote import SUBMIT_TOOL


def _source_urls(value, destination):
    if dataclasses.is_dataclass(value):
        value = dataclasses.asdict(value)
    if isinstance(value, dict):
        url = value.get("url") or value.get("uri")
        if isinstance(url, str):
            try:
                parsed = urlsplit(url)
                valid = parsed.scheme in {"http", "https"} and parsed.hostname and not parsed.username
            except ValueError:
                valid = False
            if valid and len(url) <= 3000:
                destination[url] = {"url": url, "title": str(value.get("title") or "")[:300],
                                    "retrieved_at": datetime.now(UTC).isoformat(), "cited": True}
        for child in value.values():
            if isinstance(child, (dict, list)):
                _source_urls(child, destination)
    elif isinstance(value, list):
        for child in value:
            _source_urls(child, destination)


def response_usage(response):
    usage = response.usage
    return {"requests": 1, "input_tokens": usage.input_tokens, "output_tokens": usage.output_tokens,
            "usage_complete": usage.input_tokens > 0,
            "total_tokens": usage.input_tokens + usage.output_tokens,
            "cached_input_tokens": usage.cache_read_tokens, "reasoning_tokens": usage.details.get("reasoning_tokens", 0),
            "actual_model": response.model_name,
            "web_search_calls": sum(getattr(p, "part_kind", "") == "builtin-tool-call" and "search" in getattr(p, "tool_name", "") for p in response.parts)}


async def execute_api(route, connection, credentials, context, instruction, output_schema):
    from pydantic_ai import Agent, Tool
    from pydantic_ai.capabilities import NativeTool
    from pydantic_ai.messages import BinaryContent, ModelMessagesTypeAdapter, ToolReturn
    from pydantic_ai.models.wrapper import WrapperModel
    from pydantic_ai.native_tools import WebSearchTool
    from pydantic_ai.output import StructuredDict, ToolOutput
    from pydantic_ai.usage import UsageLimits

    from .provider_factory import model_for_route

    limits = connection.get("_execution_limits", {})
    caps = connection.get("_model", {}).get("capabilities", {})
    sources, requests = {}, []

    class MeteredModel(WrapperModel):
        async def request(self, messages, model_settings, model_request_parameters):
            serialized = ModelMessagesTypeAdapter.dump_json(messages)
            context_window = caps.get("context_window")
            schema_size = len(json.dumps(dataclasses.asdict(model_request_parameters), default=str))
            expanded_context = route.get("web_search") or b'"media_type":"image/' in serialized
            if expanded_context and not context_window and connection["billing"] in {"metered", "unknown"}:
                raise RuntimeUnavailable("Record the model's documented context window before paid image inspection or hosted web research.")
            text_allowance = len(serialized) + schema_size
            estimated_input = int(context_window) if expanded_context and context_window else (
                min(int(context_window), text_allowance) if context_window else text_allowance)
            reservation = await context.control.before_request(estimated_input, route["max_output_tokens"])
            response = await self.wrapped.request(messages, model_settings, model_request_parameters)
            usage = response_usage(response)
            await context.control.on_response(usage, reservation)
            requests.append(usage)
            await context.control.event("model_response", "Model response received.", {"connection_id": connection["id"], **usage})
            if response.model_name and response.model_name != route["model_id"]:
                raise RuntimeUnavailable("The provider reported a different model. Its result was withheld; select the exact approved model identifier before retrying.")
            for part in response.parts:
                if getattr(part, "part_kind", "") == "builtin-tool-return":
                    _source_urls(getattr(part, "content", None), sources)
                elif getattr(part, "part_kind", "") == "text":
                    _source_urls(getattr(part, "provider_details", None), sources)
            return response

    def adapt(definition):
        async def invoke(ctx, **arguments):
            response = await context.source_client.call_tool(definition.name, arguments)
            if response.is_error:
                message = "\n".join(part.text for part in response.content if part.type == "text")
                raise RuntimeUnavailable(message[:1200])
            texts = [part.text for part in response.content if part.type == "text"]
            images = [BinaryContent(data=base64.b64decode(part.data), media_type=part.mime_type)
                      for part in response.content if part.type == "image"]
            if images:
                return ToolReturn(return_value="\n".join(texts), content=images)
            return "\n".join(texts)
        return Tool.from_schema(invoke, definition.name, definition.description, definition.input_schema,
                                takes_ctx=True, sequential=True)

    definitions = (await context.source_client.list_tools()).tools
    definitions = [tool for tool in definitions if tool.name != SUBMIT_TOOL]
    if {tool.name for tool in definitions} != set(context.bridge.names) - {SUBMIT_TOOL}:
        raise RuntimeUnavailable("The scoped tool catalog changed before this model request.")
    native = [NativeTool(WebSearchTool(max_uses=route.get("max_search_calls", 3)))] if route.get("web_search") else []
    async with model_for_route(route, connection, credentials) as binding:
        agent = Agent(MeteredModel(binding.model), instructions=instruction,
                      output_type=ToolOutput(StructuredDict(output_schema)),
                      tools=[adapt(tool) for tool in definitions], capabilities=native,
                      model_settings=binding.settings, retries=1, name="Quantix Tender Office")
        agent.instrument = False
        result = await agent.run("Carry out the supplied instruction and return the complete structured proposal.",
                                 usage_limits=UsageLimits(request_limit=limits.get("max_requests", 12), tool_calls_limit=150))
    usage = result.usage()
    return {"output": result.output, "web_sources": list(sources.values()),
            "usage": {"requests": usage.requests, "input_tokens": usage.input_tokens,
                      "usage_complete": bool(requests) and all(row["usage_complete"] for row in requests),
                      "actual_model": requests[-1]["actual_model"] if requests else None,
                      "output_tokens": usage.output_tokens, "total_tokens": usage.input_tokens + usage.output_tokens,
                      "cached_input_tokens": usage.cache_read_tokens, "request_details": requests,
                      "web_search_calls": sum(r["web_search_calls"] for r in requests),
                      "estimated_cost_usd": None, "cost_basis": "Recorded per-route usage and budget allowances are shown separately."}}


async def execute_runtime(route, connection, credentials, context, instruction, output_schema):
    from .common import RuntimeConnectionFailure, transient_status

    protocol = connection["protocol"]
    if protocol == "codex":
        from .codex import execute_codex as execute
    elif protocol == "claude_agent":
        from .claude import execute_claude as execute
    elif protocol == "copilot":
        from .copilot import execute_copilot as execute
    elif protocol == "gemini_cli":
        from .gemini import execute_gemini as execute
    elif protocol == "grok_build":
        from .grok import execute_grok as execute
    else:
        raise RuntimeUnavailable("This original client supports attended account use only; it cannot run background Tender work.")
    try:
        return await execute(route, connection, credentials, context, instruction, output_schema,
                             before_request=context.control.before_request, on_response=context.control.on_response)
    except (RuntimeConnectionFailure, ValueError, InterruptedError):
        raise
    except (TimeoutError, ConnectionError):
        raise RuntimeConnectionFailure("The original AI client's connection timed out or was interrupted.") from None
    except Exception as error:
        if protocol == "codex":
            from openai_codex.errors import TransportClosedError, is_retryable_error
            if isinstance(error, TransportClosedError) or is_retryable_error(error):
                raise RuntimeConnectionFailure("The Codex connection closed or its server was temporarily unavailable.") from None
        elif protocol == "claude_agent":
            from claude_agent_sdk import CLIConnectionError, CLINotFoundError
            from claude_agent_sdk._errors import ResultError
            if isinstance(error, ResultError) and error.terminal_reason in {"aborted_streaming", "aborted_tools"}:
                raise InterruptedError("The Claude Agent turn was interrupted.") from None
            if (isinstance(error, CLIConnectionError) and not isinstance(error, CLINotFoundError)
                    or isinstance(error, ResultError) and transient_status(error.api_error_status)):
                raise RuntimeConnectionFailure("The Claude Agent connection or service was temporarily unavailable.") from None
        elif protocol == "copilot":
            from copilot._jsonrpc import ProcessExitedError
            if isinstance(error, ProcessExitedError):
                raise RuntimeConnectionFailure("The Copilot connection closed unexpectedly.") from None
        raise RuntimeUnavailable("The original AI client could not complete this request. Review its setup, model and limits.") from None
