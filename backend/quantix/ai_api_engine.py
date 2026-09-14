"""Pydantic AI loop used by the bundled direct API service.

The loop keeps Quantix tools local to the application and selected provider
tools behind route-specific limits. Streaming drafts never publish records.
"""

from __future__ import annotations

import asyncio
import base64
import dataclasses
import inspect
import json
import re
import time
from contextlib import asynccontextmanager
from datetime import UTC, datetime
from urllib.parse import urlsplit
from uuid import uuid4

from .ai_api_errors import (
    DirectAPIError,
    DirectBudgetError,
    DirectProviderError,
    preserve_control_failure,
    provider_failure,
)
from .ai_api_provider import model_for_route, supplied_summary_supported
from .ai_tools import trusted_invocation_id
from .diagnostics import record, record_exception
from .run_activity import (
    ActivityRecorder,
    ActivityRecordingError,
    activity_scope,
    current_operation,
    register_activity_secrets,
)

CHECK_INPUT_ALLOWANCE_BYTES = 16384
CHECK_MAX_OUTPUT_TOKENS = 1024
CHECK_MAX_REQUESTS = 2
CHECK_DEADLINE_SECONDS = 90
# Corrections a model may make to a proposal that failed the publication checks.
OUTPUT_CORRECTIONS = 2
# Every step re-sends the whole history. Once earlier tool output passes this
# many characters, results older than the latest steps become a short note.
HISTORY_TOOL_CHARS = 60000
HISTORY_KEEP_RECENT_STEPS = 2
TRIMMED_NOTE = "[Earlier tool output removed to keep requests small.]"
_EVIDENCE_ID = re.compile(r'"id":\s*"([0-9a-f]{32})"')

try:
    from pydantic_ai.models.wrapper import WrapperModel
except ImportError:  # pragma: no cover - status remains usable without bundles

    class WrapperModel:  # type: ignore[no-redef]
        def __init__(self, wrapped):
            self.wrapped = wrapped


def _source_urls(value, destination: dict[str, dict]) -> None:
    if dataclasses.is_dataclass(value):
        value = dataclasses.asdict(value)
    if isinstance(value, dict):
        url = value.get("url") or value.get("uri")
        if isinstance(url, str):
            try:
                parsed = urlsplit(url)
                valid = (
                    parsed.scheme in {"http", "https"} and parsed.hostname and not parsed.username
                )
            except ValueError:
                valid = False
            if valid and len(url) <= 3000:
                destination[url] = {
                    "url": url,
                    "title": str(value.get("title") or "")[:300],
                    "retrieved_at": datetime.now(UTC).isoformat(),
                    "cited": True,
                }
        for child in value.values():
            if isinstance(child, (dict, list, tuple)):
                _source_urls(child, destination)
    elif isinstance(value, (list, tuple)):
        for child in value:
            _source_urls(child, destination)


def trim_tool_history(messages: list, context=None) -> list:
    """Replace older tool output with a note once the history grows large.

    Evidence read earlier stays read for citation, so the note keeps its IDs.
    The replacement is kept for the rest of the run, which keeps the history's
    prefix stable for provider caching after the one-off change.
    """

    from pydantic_ai.messages import ModelRequest, ToolReturnPart

    returns = [
        (index, position)
        for index, message in enumerate(messages)
        if isinstance(message, ModelRequest)
        for position, part in enumerate(message.parts)
        if isinstance(part, ToolReturnPart)
        and isinstance(part.content, str)
        and not part.content.startswith(TRIMMED_NOTE)
    ]
    if sum(len(messages[index].parts[position].content) for index, position in returns) <= HISTORY_TOOL_CHARS:
        return messages
    recent = set(sorted({index for index, _ in returns})[-HISTORY_KEEP_RECENT_STEPS:])
    replaced: dict[int, list] = {}
    for index, position in returns:
        if index in recent:
            continue
        part = messages[index].parts[position]
        ids = list(dict.fromkeys(_EVIDENCE_ID.findall(part.content)))[:40]
        note = (
            f"{TRIMMED_NOTE} Evidence IDs it returned stay valid to cite: {', '.join(ids)}. "
            "Read a passage again with read_source if you need its exact text."
            if ids
            else f"{TRIMMED_NOTE} Call {part.tool_name} again if you still need it."
        )
        replaced.setdefault(index, list(messages[index].parts))[position] = dataclasses.replace(
            part, content=note
        )
    if not replaced:
        return messages
    if context is not None and hasattr(context, "returned_reads"):
        # The model can no longer see those passages, so they may be sent again.
        context.returned_reads.clear()
    return [
        dataclasses.replace(message, parts=replaced[index]) if index in replaced else message
        for index, message in enumerate(messages)
    ]


def response_usage(response) -> dict:
    usage = response.usage
    details = getattr(usage, "details", {}) or {}
    input_tokens = getattr(usage, "input_tokens", 0)
    output_tokens = getattr(usage, "output_tokens", 0)
    input_tokens = int(input_tokens or 0)
    output_tokens = int(output_tokens or 0)
    parts = getattr(response, "parts", ()) or ()
    return {
        "requests": 1,
        "input_tokens": input_tokens,
        "output_tokens": output_tokens,
        "usage_complete": input_tokens > 0 and getattr(response, "state", "complete") == "complete",
        "total_tokens": input_tokens + output_tokens,
        "cached_input_tokens": getattr(usage, "cache_read_tokens", None),
        "reasoning_tokens": int(details.get("reasoning_tokens", 0) or 0),
        "actual_model": getattr(response, "model_name", None),
        "web_search_calls": sum(
            getattr(part, "part_kind", "") == "builtin-tool-call"
            and "search" in getattr(part, "tool_name", "")
            for part in parts
        ),
    }


def _check_active(context) -> None:
    if context is None:
        return
    run = context.repo.get_run(context.run_id)
    if run["status"] not in {"running", "queued"}:
        raise InterruptedError("This Tender run is no longer active.")


def public_summary(part, connection, model_id=""):
    if not supplied_summary_supported(connection, model_id):
        return None
    if connection.get("provider_id") == "xai" and model_id == "grok-4.6":
        # xAI documents reasoning_text.delta as summarized content for this
        # exact model. The OpenAI adapter stores it separately from summaries.
        raw = (getattr(part, "provider_details", None) or {}).get("raw_content")
        if not part.content and isinstance(raw, list) and all(isinstance(item, str) for item in raw):
            return "\n".join(raw)
    return part.content


class DraftStreamEvents:
    """Project supplied stream parts into complete, explicitly tentative text.

    Only known user-facing output fields are exposed. Partial JSON, local tool
    arguments, signatures and raw reasoning stay inside the provider loop.
    """

    def __init__(self, context, connection, output_type, *, request_operation=current_operation):
        self.context = context
        self.connection = connection
        fields = output_type.model_fields
        if "summary" in fields:
            self.paths = [("summary",), ("question",)] if "question" in fields else [("summary",)]
        else:
            self.paths = []
        self.total_chars = 0
        self.saved_events = 0
        self.ever_text = False
        self.recorder = ActivityRecorder(context)
        self.request_id = None
        self.sections = {}
        self.request_operation = request_operation

    def _save(self, kind, text, *, reset=False, section_id=None, complete=False):
        if not text:
            return
        # Stop and the append are serialized in one local transaction. A late
        # provider chunk cannot add visible activity after its run is stopped.
        with self.context.repo.atomic():
            _check_active(self.context)
            identity = {name: getattr(self.context, name) for name in ("assignment_id", "actor_id")
                        if isinstance(getattr(self.context, name, None), str) and getattr(self.context, name)}
            values = {"text": text, **identity, **({"reset": True} if reset else {})}
            if self.request_id:
                values.update(request_id=self.request_id, section_id=str(section_id), activity_operation_id=self.request_id)
            from .activity_privacy import sanitize
            values, _, _ = sanitize(values, getattr(self.context, "_activity_secrets", ()))
            self.context.repo.event(self.context.run_id, kind, "AI response draft" if kind == "assistant_text_delta"
                                    else "AI reasoning summary", values)
            if self.request_id:
                category = "draft" if kind == "assistant_text_delta" else "reasoning_summary"
                key = (category, section_id)
                operation = self.sections.get(key)
                payload = {"text": values["text"], "section_id": str(section_id), "request_id": self.request_id, "delta": not complete}
                if operation is None:
                    operation = self.recorder.start(category, "AI response draft" if category == "draft" else "AI reasoning summary",
                        payload, parent_operation_id=self.request_id, phase="completed" if complete else "started")
                    self.sections[key] = operation
                else:
                    self.recorder.record(operation, category, "completed" if complete else "delta", "AI response draft" if category == "draft" else "AI reasoning summary", payload)
        self.total_chars += len(text)
        self.saved_events += 1

    def _readable(self, part):
        from pydantic_ai.messages import TextPart, ToolCallPart
        from pydantic_core import from_json

        raw = part.args if isinstance(part, ToolCallPart) else part.content if isinstance(part, TextPart) else None
        if isinstance(raw, str):
            try:
                raw = from_json(raw, allow_partial="trailing-strings")
            except ValueError:
                return None
        for path in self.paths:
            value = raw
            for key in path:
                value = value.get(key) if isinstance(value, dict) else None
            if isinstance(value, str):
                return value
        return None

    async def handle(self, _run_context, events):
        from pydantic_ai.messages import (
            FinalResultEvent,
            PartDeltaEvent,
            PartEndEvent,
            PartStartEvent,
            TextPart,
            ThinkingPart,
            ToolCallPart,
        )

        parts = {}
        self.request_id = self.request_operation()
        self.sections = {}
        output_call = None
        text_result = False
        output_index = None
        previous = ""
        pending = ""
        request_has_text = False
        reset_pending = False
        last_flush = time.monotonic()
        summaries = {}
        summary_last_flush = time.monotonic()
        native_operations = {}
        native_seen = set()

        def flush():
            nonlocal pending, request_has_text, last_flush, reset_pending
            while pending:
                self._save("assistant_text_delta", pending[:2048],
                           reset=reset_pending or (self.ever_text and not request_has_text), section_id=output_index)
                pending = pending[2048:]
                request_has_text = self.ever_text = True
                reset_pending = False
            last_flush = time.monotonic()

        async for event in events:
            _check_active(self.context)
            self.request_id = self.request_operation()
            if isinstance(event, PartStartEvent):
                parts[event.index] = event.part
            elif isinstance(event, PartDeltaEvent) and event.index in parts:
                parts[event.index] = event.delta.apply(parts[event.index])
            elif isinstance(event, PartEndEvent):
                parts[event.index] = event.part
            elif isinstance(event, FinalResultEvent):
                output_call = event.tool_call_id
                text_result = event.tool_name is None
                output_index = next((index for index, part in parts.items()
                                     if (text_result and isinstance(part, TextPart)) or
                                     (isinstance(part, ToolCallPart) and output_call and part.tool_call_id == output_call)), None)
            else:
                continue
            from .provider_tool_activity import observe_provider_tool

            if isinstance(event, (PartStartEvent, PartEndEvent)):
                observe_provider_tool(self.recorder, self.request_id, event.part, native_operations, native_seen)
            for index, part in parts.items():
                if index == output_index:
                    readable = self._readable(part)
                    if readable is not None and readable != previous:
                        if readable.startswith(previous):
                            pending += readable[len(previous):]
                        else:
                            pending = readable
                            reset_pending = request_has_text
                        previous = readable
                # In the first-party Responses adapter content maps only the
                # provider's summary events. Raw reasoning lives separately in
                # provider_details and encrypted content in signature.
                elif isinstance(part, ThinkingPart):
                    summary = public_summary(part, self.connection, self.connection.get("_activity_model_id", ""))
                    if summary and summary != summaries.get(index):
                        if len(summary) - len(summaries.get(index, "")) >= 256 or time.monotonic() - summary_last_flush >= .2 or isinstance(event, PartEndEvent):
                            # Legacy stream receives snapshots; activity stores
                            # only the new fragment until the final section.
                            if self.request_id:
                                self._save("assistant_reasoning_summary", summary[len(summaries.get(index, "")):], section_id=index)
                            else:
                                self._save("assistant_reasoning_summary", summary, section_id=index)
                            summaries[index] = summary
                            summary_last_flush = time.monotonic()
            if len(pending) >= 256 or time.monotonic() - last_flush >= .2:
                flush()
        flush()
        if self.request_id:
            for index, part in parts.items():
                if isinstance(part, ThinkingPart):
                    summary = public_summary(part, self.connection, self.connection.get("_activity_model_id", ""))
                    if summary:
                        self._save("assistant_reasoning_summary", summary, section_id=index, complete=True)
            if previous:
                operation = self.sections.get(("draft", output_index))
                if operation:
                    self.recorder.record(operation, "draft", "completed", "AI response draft", {"text": previous, "request_id": self.request_id, "section_id": str(output_index), "delta": False})


def _tool_return(value):
    """Preserve native image content returned by visual source tools."""

    if not isinstance(value, list):
        return value
    images, text = [], []
    for item in value:
        if not isinstance(item, dict) or item.get("type") not in {"text", "image"}:
            return value
        if item["type"] == "text":
            text.append(str(item.get("text") or ""))
        else:
            try:
                from pydantic_ai.messages import BinaryContent, ToolReturn

                images.append(
                    BinaryContent(
                        data=base64.b64decode(item["data"], validate=True),
                        media_type=item["mime_type"],
                    )
                )
            except (KeyError, ValueError) as error:
                raise DirectAPIError("A drawing source returned an invalid image.") from error
    if images:
        from pydantic_ai.messages import ToolReturn

        return ToolReturn(return_value="\n".join(text), content=images)
    return "\n".join(text)


class LocalToolBridge:
    """Guard and invoke the closed set of Quantix-owned local tools."""

    def __init__(self, context, output_type, definitions, *, image_support=False, max_calls=150):
        self.context = context
        self.output_type = output_type
        self.definitions = {tool.name: tool for tool in definitions}
        self.image_support = image_support
        self.max_calls = max_calls
        self.calls = 0
        self.closed = False
        self.bridge_session_id = uuid4().hex
        self.request_operation = lambda: current_operation()

    def adapt(self, definition):
        async def invoke(ctx, **arguments):
            if self.closed:
                raise InterruptedError("This Tender tool session has ended.")
            _check_active(self.context)
            self.calls += 1
            if self.calls > self.max_calls:
                raise DirectAPIError("The local Tender tool-call limit was reached.")
            if definition.name == "view_document_page" and not self.image_support:
                raise DirectAPIError(
                    "Image support has not been established for this model. Use an approved vision-capable connection for drawing inspection."
                )
            tool_call_id = getattr(ctx, "tool_call_id", None)
            if not isinstance(tool_call_id, str):
                tool_call_id = None
            invocation_id = trusted_invocation_id(
                self.bridge_session_id,
                definition.name,
                tool_call_id,
            )
            if definition.requires_invocation_id and invocation_id is None:
                raise ValueError("A trusted invocation identity is required for this tool.")
            try:
                from .tool_policy import ToolFenceError, dispatch

                with activity_scope(self.request_operation()):
                    value = await dispatch(
                        "direct", definition, self.context, arguments, invocation_id=invocation_id,
                    )
            except asyncio.CancelledError:
                raise
            except InterruptedError:
                raise
            except ActivityRecordingError:
                raise
            except ToolFenceError as error:
                # A tool's complaint about its own arguments or scope is the
                # model's to correct; refused authority stops the run with the
                # real reason instead of a provider failure.
                if error.recoverable:
                    from pydantic_ai import ModelRetry

                    raise ModelRetry(str(error)) from error
                raise DirectAPIError(f"A Tender tool call was refused. {error}") from error
            except (KeyError, ValueError):
                raise
            except Exception as error:
                raise DirectAPIError(
                    "A Tender tool failed. The result was not published."
                ) from error
            return _tool_return(value)

        return invoke


class MeteredModel(WrapperModel):
    """A thin Pydantic AI model wrapper with per-request Quantix controls."""

    def __init__(
        self,
        wrapped,
        *,
        route,
        connection,
        context,
        before_request,
        on_response,
        check_tool=None,
        check_mode=False,
        sources=None,
        requests=None,
        expected_model=None,
    ):
        super().__init__(wrapped)
        self.route = route
        self.connection = connection
        self.context = context
        self.before_request_callback = before_request
        self.on_response_callback = on_response
        self.check_tool = check_tool
        self.check_mode = check_mode
        self.sources = sources if sources is not None else {}
        self.requests = requests if requests is not None else []
        self.expected_model = expected_model
        # (input bytes, reported input tokens) of the last fully reported request.
        self._calibration = None
        self.activity = ActivityRecorder(None if check_mode else context)
        self.last_request_id = None

    @asynccontextmanager
    async def _activity_request(self, messages, settings, parameters):
        from pydantic_ai.messages import ModelMessagesTypeAdapter

        operation = self.activity.start("model_request", "Content supplied by Quantix",
            {"messages": json.loads(ModelMessagesTypeAdapter.dump_json(messages)),
             "settings": settings or {}, "parameters": dataclasses.asdict(self._parameters(parameters)),
             "billing": self.connection.get("billing"), "protocol": self.connection.get("protocol")},
            provider=self.connection.get("provider_id"), model=self.route.get("model_id"), phase="prepared")
        self.last_request_id = operation
        available = supplied_summary_supported(self.connection, self.route.get("model_id", ""))
        self.activity.record(operation, "capability", "observed", "Thinking summaries are supported by this connection." if available else "No documented thinking summary stream for this connection.",
            {"supported": available, "detail": "Public summaries are retained when supplied." if available else "This provider route does not expose a documented public reasoning summary."},
            capture_status="complete" if available else "unavailable", unavailable_fields=[] if available else ["reasoning_summary"])
        started = time.monotonic()
        with activity_scope(operation):
            try:
                yield
            except BaseException as error:
                if not isinstance(error, ActivityRecordingError):
                    cancelled = isinstance(error, (asyncio.CancelledError, InterruptedError))
                    self.activity.record(operation, "model_request", "cancelled" if cancelled else "failed",
                        "AI request interrupted." if cancelled else "AI request failed.",
                        {"error": str(error) if isinstance(error, (DirectAPIError, InterruptedError)) else "The provider request did not complete."},
                        elapsed_ms=int((time.monotonic() - started) * 1000), capture_status="partial")
                raise
            else:
                self.activity.record(operation, "model_request", "completed", "AI request completed.",
                    elapsed_ms=int((time.monotonic() - started) * 1000))

    async def request(self, messages, model_settings, model_request_parameters):
        async with self._activity_request(messages, model_settings, model_request_parameters):
            return await self._perform_request(messages, model_settings, model_request_parameters)

    @asynccontextmanager
    async def request_stream(self, messages, model_settings, model_request_parameters, run_context=None):
        async with self._activity_request(messages, model_settings, model_request_parameters):
            async with self._perform_request_stream(messages, model_settings, model_request_parameters, run_context) as stream:
                yield stream

    def _request_started(self, parameters):
        self.activity.record(current_operation(), "model_request", "started", "Sending the approved AI request.",
            {"parameters": dataclasses.asdict(parameters)})

    def __getattr__(self, name):
        return getattr(self.wrapped, name)

    async def __aenter__(self):
        await self.wrapped.__aenter__()
        return self

    async def __aexit__(self, *args):
        return await self.wrapped.__aexit__(*args)

    def _parameters(self, parameters):
        if not self.check_mode:
            return parameters
        functions = list(parameters.function_tools)
        if self.check_tool is not None and self.check_tool.calls >= 1:
            functions = [tool for tool in functions if tool.name != self.check_tool.name]
        output_tools = (
            list(parameters.output_tools) if self.check_tool and self.check_tool.calls >= 1 else []
        )
        return dataclasses.replace(parameters, function_tools=functions, output_tools=output_tools)

    async def _admit_request(self, messages, model_request_parameters):
        _check_active(self.context)
        parameters = self._parameters(model_request_parameters)
        from pydantic_ai.messages import ModelMessagesTypeAdapter

        serialized = ModelMessagesTypeAdapter.dump_json(messages)
        overhead = json.dumps(
            dataclasses.asdict(parameters), ensure_ascii=False, separators=(",", ":"), default=str
        ).encode("utf-8")
        input_bytes = len(serialized) + len(overhead)
        capabilities = self.connection.get("_model", {}).get("capabilities", {})
        context_window = capabilities.get("context_window")
        expanded_context = (
            bool(self.route.get("web_search"))
            or b'"media_type":"image/' in serialized
        )
        if (
            expanded_context
            and not context_window
            and self.connection.get("billing") in {"metered", "unknown"}
        ):
            raise DirectAPIError(
                "Record the model's documented context window before paid image inspection or hosted web research."
            )
        if self.check_mode and input_bytes > CHECK_INPUT_ALLOWANCE_BYTES:
            raise DirectAPIError(
                f"The connection check sample exceeds its {CHECK_INPUT_ALLOWANCE_BYTES}-byte input allowance."
            )
        if self.before_request_callback is None:
            raise DirectAPIError("A budget callback is required before an AI request.")
        estimated_input = input_bytes
        if self._calibration is not None and input_bytes >= self._calibration[0]:
            # History only grows between requests: the provider already counted the
            # earlier prompt, and each added byte is at most one more token.
            counted_bytes, counted_tokens = self._calibration
            estimated_input = min(input_bytes, counted_tokens + input_bytes - counted_bytes)
        input_allowance = (
            int(context_window)
            if expanded_context and context_window
            else (min(int(context_window), estimated_input) if context_window else estimated_input)
        )
        try:
            reserved = self.before_request_callback(
                input_allowance, int(self.route["max_output_tokens"])
            )
        except asyncio.CancelledError:
            raise
        except DirectAPIError:
            raise
        except Exception as error:
            # Budget callbacks are Quantix-owned and may carry a useful
            # refusal reason.  They are translated separately from provider
            # failures and never pass SDK response text through this boundary.
            raise DirectBudgetError(str(error)[:600]) from None
        if inspect.isawaitable(reserved):
            try:
                reserved = await reserved
            except asyncio.CancelledError:
                raise
            except DirectAPIError:
                raise
            except Exception as error:
                raise DirectBudgetError(str(error)[:600]) from None
        return parameters, reserved, input_bytes, expanded_context

    async def _perform_request(self, messages, model_settings, model_request_parameters):
        parameters, reserved, input_bytes, expanded_context = await self._admit_request(
            messages, model_request_parameters
        )
        self._request_started(parameters)
        try:
            response = await self.wrapped.request(messages, model_settings, parameters)
        except asyncio.CancelledError:
            raise
        except (DirectAPIError, DirectProviderError, InterruptedError, ActivityRecordingError):
            raise
        except Exception as error:
            raise provider_failure(error) from None
        return await self._finalize_response(response, reserved, input_bytes, expanded_context)

    @asynccontextmanager
    async def _perform_request_stream(self, messages, model_settings, model_request_parameters, run_context=None):
        """One reservation per actual streamed request, including interrupted ones."""
        import anyio

        parameters, reserved, input_bytes, expanded_context = await self._admit_request(
            messages, model_request_parameters
        )
        self._request_started(parameters)
        try:
            async with self.wrapped.request_stream(messages, model_settings, parameters, run_context) as stream:
                failed = False
                try:
                    yield stream
                except BaseException:
                    failed = True
                    raise
                finally:
                    response = stream.get()
                    incomplete = failed or response.state != "complete"
                    # A bounded shield allows durable partial usage recording
                    # under Pydantic AI's cancellation scope. No new request or
                    # publication occurs during this cleanup.
                    with anyio.CancelScope(shield=True):
                        try:
                            try:
                                if incomplete and (failed or response.state != "suspended"):
                                    await asyncio.wait_for(stream.cancel(), timeout=5)
                            finally:
                                # A failed transport close must not discard
                                # already observed partial usage.
                                await asyncio.wait_for(self._finalize_response(
                                    response, reserved, input_bytes, expanded_context,
                                    force_incomplete=incomplete,
                                ), timeout=5)
                        except BaseException:
                            if not failed:
                                raise
                            # Keep cancellation/provider failure as the cause.
                            # The existing reservation remains authoritative if
                            # cleanup could not report its incomplete usage.
                            record("direct_api_stream_cleanup", outcome="uncertain")
        except asyncio.CancelledError:
            raise
        except (DirectAPIError, DirectProviderError, InterruptedError, ActivityRecordingError):
            raise
        except Exception as error:
            raise provider_failure(error) from None

    async def _finalize_response(self, response, reserved, input_bytes, expanded_context, *, force_incomplete=False):
        usage = response_usage(response)
        from .ai_generation import bounded_native_call_limit

        native_limit = bounded_native_call_limit(self.route, self.connection)
        native_calls = sum(getattr(part, "part_kind", "") == "builtin-tool-call"
                           for part in getattr(response, "parts", ()) or ())
        native_overrun = native_limit is not None and native_calls > native_limit
        if native_limit is not None:
            usage.update(native_tool_calls=native_calls, native_call_limit=native_limit,
                         native_call_limit_overrun=native_overrun)
        if native_overrun:
            usage["usage_complete"] = False
        if force_incomplete:
            usage["usage_complete"] = False
        if usage["usage_complete"] and not expanded_context:
            self._calibration = (input_bytes, usage["input_tokens"])
        if self.on_response_callback is not None:
            try:
                reported = self.on_response_callback(usage, reserved)
                if inspect.isawaitable(reported):
                    await reported
            except asyncio.CancelledError:
                raise
            except DirectAPIError:
                raise
            except Exception as error:
                raise DirectBudgetError(str(error)[:600]) from None
        self.requests.append(usage)
        public_parts = []
        for part in getattr(response, "parts", ()) or ():
            kind = getattr(part, "part_kind", "")
            if kind == "thinking":
                summary = public_summary(part, self.connection, self.route.get("model_id", ""))
                if summary:
                    public_parts.append({"part_kind": "reasoning_summary", "content": summary, "id": getattr(part, "id", None)})
            elif kind in {"text", "tool-call", "builtin-tool-call", "builtin-tool-return", "file"}:
                public_parts.append({name: getattr(part, name) for name in
                    ("part_kind", "content", "tool_name", "tool_call_id", "args", "id") if hasattr(part, name)})
        self.activity.record(current_operation(), "model_output", "observed", "Provider supplied output and usage.",
            {"parts": public_parts, "usage": usage, "provider_response_id": getattr(response, "provider_response_id", None),
             "finish_reason": getattr(response, "finish_reason", None)},
            capture_status="partial" if force_incomplete else "complete")
        if native_overrun:
            raise DirectAPIError("The provider exceeded the reviewed shared native-call limit. Its result was withheld and the request allowance remains reserved.")
        for part in getattr(response, "parts", ()) or ():
            if getattr(part, "part_kind", "") == "builtin-tool-return":
                _source_urls(getattr(part, "content", None), self.sources)
            elif getattr(part, "part_kind", "") == "text":
                _source_urls(getattr(part, "provider_details", None), self.sources)
        actual_model = usage.get("actual_model")
        requested_model = self.expected_model or self.route.get("model_id")
        from .ai_api_provider import same_reported_model

        if (
            isinstance(actual_model, str) and actual_model
            and isinstance(requested_model, str) and requested_model
            and not same_reported_model(
                self.connection.get("provider_id", ""),
                self.connection.get("protocol", ""),
                requested_model,
                actual_model,
            )
        ):
            raise DirectAPIError(
                "The provider reported a different model. Its result was withheld; select the exact approved model identifier before retrying."
            )
        return response


async def _run_model(
    route: dict,
    connection: dict,
    credentials: dict[str, str],
    context,
    instruction: str,
    output_type,
    *,
    consult=None,
    before_request=None,
    on_response=None,
    definitions=None,
    operation="execute",
    validate_output=None,
) -> dict:
    """Run one direct model loop and return validated output and attribution."""

    from pydantic_ai import Agent, ModelRetry, Tool
    from pydantic_ai.capabilities import NativeTool, ProcessHistory
    from pydantic_ai.usage import UsageLimits

    from .ai_generation import (
        native_tools_for,
        output_for,
        validate_generation,
        validate_sdk_settings,
    )

    if not hasattr(output_type, "model_validate") or not hasattr(output_type, "model_json_schema"):
        raise DirectAPIError("The direct API output type must be a Pydantic model.")
    if definitions is None:
        if operation == "check":
            definitions = []
        else:
            from .office_tools import source_tools

            definitions = source_tools()
    definitions = list(definitions)
    if consult is not None:
        definitions.append(consult)
    names = [definition.name for definition in definitions]
    if len(set(names)) != len(names):
        raise DirectAPIError("The direct API tool catalog contains duplicate names.")
    limits = connection.get("_execution_limits", {})
    max_requests = int(limits.get("max_requests", 12))
    max_output = int(route.get("max_output_tokens", 8192))
    if max_requests < 1 or max_output < 1:
        raise DirectAPIError("The direct API request limits are invalid.")
    check_mode = operation == "check"
    if check_mode and (max_requests > CHECK_MAX_REQUESTS or max_output > CHECK_MAX_OUTPUT_TOKENS):
        raise DirectAPIError("The connection check exceeds its small approved request limits.")
    image_support = connection.get("_model", {}).get("capabilities", {}).get("images") is True
    from .office_tools import usable_definitions

    # A tool the model cannot use would fail the whole run when called.
    definitions = usable_definitions(definitions, image_support=image_support)
    bridge = LocalToolBridge(
        context,
        output_type,
        definitions,
        image_support=image_support,
        max_calls=min(1000, max_requests * 10),
    )
    tools = [
        Tool.from_schema(
            bridge.adapt(definition),
            definition.name,
            definition.description,
            definition.parameters,
            takes_ctx=True,
            sequential=True,
        )
        for definition in definitions
    ]
    try:
        generation = validate_generation(route, connection)
        native = [NativeTool(tool) for tool in native_tools_for(route, connection)]
    except ValueError as error:
        raise DirectAPIError(str(error)) from None
    check_tool = next(
        (definition for definition in definitions if definition.name == "quantix_connection_check"),
        None,
    )
    sources: dict[str, dict] = {}
    request_details: list[dict] = []
    chosen = dict(route)
    chosen["max_output_tokens"] = (
        min(max_output, CHECK_MAX_OUTPUT_TOKENS) if check_mode else max_output
    )
    async with model_for_route(chosen, connection, credentials) as binding:
        try:
            validate_sdk_settings(binding.settings, binding.model.profile)
            if generation.output_mode == "native" and not binding.model.profile.get("supports_json_schema_output"):
                raise ValueError("This model adapter does not support native output with JSON Schema.")
            if generation.output_mode == "native" and tools and connection.get("protocol") == "google" and not binding.model.profile.get("google_supports_tool_combination"):
                raise ValueError("This model adapter cannot combine native output with office tools.")
        except ValueError as error:
            raise DirectAPIError(str(error)) from None
        wrapped = MeteredModel(
            binding.model,
            route=chosen,
            connection=connection,
            context=context,
            before_request=before_request,
            on_response=on_response,
            check_tool=check_tool,
            check_mode=check_mode,
            sources=sources,
            requests=request_details,
            expected_model=binding.model.model_name,
        )
        bridge.request_operation = lambda: wrapped.last_request_id
        from .ai_execution import TURN_CONTEXT_MARKER

        # Only the static part is the system instruction; the changing request and
        # context travel in the user message, so the cached prefix stays identical.
        static, marker, turn_context = instruction.partition(TURN_CONTEXT_MARKER)
        agent = Agent(
            wrapped,
            instructions=static if marker else instruction,
            output_type=output_for(generation, output_type,
                                  retries=OUTPUT_CORRECTIONS if validate_output is not None else None),
            tools=tools,
            capabilities=[*native, ProcessHistory(lambda messages: trim_tool_history(messages, context))],
            model_settings=binding.settings,
            # One correction per tool call: a recoverable complaint about the
            # model's own arguments is worth a retry, a loop is not.
            retries=1,
            name="Quantix Tender Office",
        )
        agent.instrument = False
        from pydantic_ai.exceptions import UnexpectedModelBehavior

        rejections: list[str] = []
        if validate_output is not None:

            @agent.output_validator
            def publication_checks(data):
                # The same checks run again before publication. Running them
                # here lets the model correct one wrong citation instead of
                # losing every request the run has already paid for.
                try:
                    validate_output(output_type.model_validate(data), list(sources.values()))
                except (KeyError, ValueError) as error:
                    reason = str(error.args[0] if error.args else error)[:1200]
                    rejections.append(reason)
                    raise ModelRetry(
                        f"The proposal was not accepted: {reason} Correct it and return the complete proposal again."
                    ) from None
                return data

        try:
            streaming = (connection.get("_model") or {}).get("capabilities", {}).get("streaming")
            stream_events = DraftStreamEvents(context, {**connection, "_activity_model_id": chosen["model_id"]}, output_type,
                request_operation=lambda: wrapped.last_request_id) if context is not None and not check_mode and streaming is not False else None
            execution = agent.run(
                (f"{turn_context}\n\nCarry out this request and return the complete structured proposal."
                 if marker else "Carry out the supplied instruction and return the complete structured proposal."),
                deps=context,
                event_stream_handler=stream_events.handle if stream_events is not None else None,
                usage_limits=UsageLimits(
                    request_limit=max_requests,
                    tool_calls_limit=min(1000, max_requests * 10),
                ),
            )
            result = await execution
        except asyncio.CancelledError:
            raise
        except (DirectAPIError, DirectProviderError, InterruptedError, ActivityRecordingError):
            raise
        except UnexpectedModelBehavior:
            # Exhausted model retries are the model's failure, never the
            # provider's credentials or endpoint.
            if rejections:
                raise DirectAPIError(
                    f"The AI could not correct its proposal. {rejections[-1][:600]} Its result was not published."
                ) from None
            raise DirectAPIError(
                "The AI could not complete this step after repeated attempts. Its result was not published."
            ) from None
        except Exception as error:
            preserved = preserve_control_failure(error)
            if preserved is not None:
                raise preserved
            raise provider_failure(error) from None
        finally:
            bridge.closed = True
    try:
        output = output_type.model_validate(result.output)
    except Exception:
        raise DirectAPIError(
            "The AI response did not match the required structured output. Its result was not published."
        ) from None
    usage = result.usage
    return {
        "output": output,
        "web_sources": list(sources.values()),
        "usage": {
            "requests": int(getattr(usage, "requests", len(request_details)) or 0),
            "input_tokens": int(getattr(usage, "input_tokens", 0) or 0),
            "output_tokens": int(getattr(usage, "output_tokens", 0) or 0),
            "total_tokens": int(getattr(usage, "input_tokens", 0) or 0)
            + int(getattr(usage, "output_tokens", 0) or 0),
            "cached_input_tokens": getattr(usage, "cache_read_tokens", None),
            "usage_complete": bool(request_details)
            and all(detail.get("usage_complete", False) for detail in request_details),
            "actual_model": request_details[-1].get("actual_model") if request_details else None,
            "request_details": request_details,
            "web_search_calls": sum(
                detail.get("web_search_calls", 0) for detail in request_details
            ),
            "estimated_cost_usd": None,
            "cost_basis": "Recorded per-route usage and budget allowances are shown separately.",
        },
    }


async def run_model(
    route: dict,
    connection: dict,
    credentials: dict[str, str],
    context,
    instruction: str,
    output_type,
    *,
    consult=None,
    before_request=None,
    on_response=None,
    definitions=None,
    operation="execute",
    validate_output=None,
) -> dict:
    """Run one direct operation with bounded safe diagnostics."""

    register_activity_secrets(context, credentials.values())
    started = time.monotonic()
    fields = {
        "phase": operation,
        "outcome": "started",
        "protocol": connection.get("protocol"),
        "connection_id": connection.get("id"),
        "model": route.get("model_id"),
    }
    if context is not None:
        fields["run_id"] = context.run_id
    record("direct_api_operation", **fields)
    try:
        result = await _run_model(
            route,
            connection,
            credentials,
            context,
            instruction,
            output_type,
            consult=consult,
            before_request=before_request,
            on_response=on_response,
            definitions=definitions,
            operation=operation,
            validate_output=validate_output,
        )
    except asyncio.CancelledError:
        record(
            "direct_api_operation",
            phase=operation,
            outcome="cancelled",
            protocol=connection.get("protocol"),
            connection_id=connection.get("id"),
            model=route.get("model_id"),
            duration_ms=int((time.monotonic() - started) * 1000),
            run_id=context.run_id if context is not None else None,
        )
        raise
    except BaseException as error:
        record_exception(
            "direct_api_operation_failed",
            error,
            phase=operation,
            protocol=connection.get("protocol"),
            connection_id=connection.get("id"),
            model=route.get("model_id"),
            duration_ms=int((time.monotonic() - started) * 1000),
            run_id=context.run_id if context is not None else None,
        )
        raise
    record(
        "direct_api_operation",
        phase=operation,
        outcome="completed",
        protocol=connection.get("protocol"),
        connection_id=connection.get("id"),
        model=route.get("model_id"),
        duration_ms=int((time.monotonic() - started) * 1000),
        requests=result.get("usage", {}).get("requests"),
        run_id=context.run_id if context is not None else None,
    )
    return result
