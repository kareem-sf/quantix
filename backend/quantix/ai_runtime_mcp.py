"""One-run, authenticated loopback MCP bridge to Quantix-owned tools."""

import asyncio
import json
import secrets
import socket
from contextlib import asynccontextmanager, contextmanager
from uuid import uuid4

import mcp.types as types
import uvicorn
from mcp.server import Server
from starlette.responses import Response

from .ai_tools import trusted_invocation_id
from .diagnostics import record, record_exception
from .office_tools import safe_text, source_tools
from .run_activity import (
    ActivityRecorder,
    ActivityRecordingError,
    activity_scope,
    register_activity_secrets,
)

SUBMIT_TOOL = "quantix_submit_result"
# A rejected proposal goes back to the client to correct. After this many
# corrections the run stops with the last reason instead of looping.
MAX_SUBMIT_CORRECTIONS = 2


_TOOL_ACTIVITY = {
    "view_document_page": "Reviewing drawing page.",
    "search_sources": "Reading Tender sources.",
    "read_source": "Reading Tender sources.",
    "read_document": "Reading Tender sources.",
    "list_documents": "Reading Tender sources.",
    "search_semantic_sources": "Searching Tender sources.",
    "read_package_map": "Reading the tender package map.",
    "read_whole_document": "Reading a tender document in full.",
    "inspect_tender_records": "Reviewing Tender records.",
    "read_tender_record": "Reviewing Tender records.",
    "inspect_estimate": "Reviewing Tender records.",
    "inspect_quote_requests": "Reviewing Tender records.",
    "read_quote_replies": "Reviewing Tender records.",
    "list_reusable_notes": "Reviewing approved reference notes.",
    "read_reusable_note": "Reviewing approved reference notes.",
    "inspect_project_map": "Reviewing Tender records.",
    "inspect_submission_requirements": "Reviewing Tender records.",
    "inspect_generated_documents": "Reviewing Tender records.",
    "calculate_drawing_measurement": "Reviewing drawing quantities.",
}


def _activity_detail(kind, data):
    if kind == "tool_started":
        tool = data.get("tool") if isinstance(data, dict) else None
        if isinstance(data, dict) and data.get("read_only") is False:
            return "Working on the Tender request."
        return (
            _TOOL_ACTIVITY.get(tool, "Reviewing Tender records.") if isinstance(tool, str) else None
        )
    if kind == "runtime_waiting":
        return "Waiting for the approved AI account."
    if kind == "runtime_model_reported":
        return "Checking the proposal before saving."
    if kind == "model_response":
        round_number = data.get("round") if isinstance(data, dict) else None
        if type(round_number) is int and 1 <= round_number <= 1000:
            return f"Preparing the proposal (model round {round_number})."
        return "Preparing the proposal."
    return None


def project_run_activity(repo, run_id, kind, data=None):
    """Apply only fixed descriptions derived from trusted lifecycle events."""
    detail = _activity_detail(kind, data or {})
    return bool(detail and repo.update_active_run_detail(run_id, detail))


def content_blocks(value) -> list:
    """Preserve image bytes and text as native MCP blocks, never text placeholders."""
    if isinstance(value, str):
        return [types.TextContent(type="text", text=value)]
    if not isinstance(value, list):
        return [types.TextContent(type="text", text=json.dumps(value, ensure_ascii=False))]
    blocks = []
    for item in value:
        if item["type"] == "text":
            blocks.append(types.TextContent(type="text", text=item["text"]))
        elif item["type"] == "image":
            blocks.append(
                types.ImageContent(type="image", mimeType=item["mime_type"], data=item["data"])
            )
        else:
            raise ValueError("The local client received an unsupported evidence content type.")
    return blocks


class RuntimeToolBridge:
    def __init__(
        self,
        context,
        output_type,
        consult=None,
        *,
        max_calls: int = 120,
        image_support: bool = False,
        definitions=None,
        operation: str = "execute",
        validate_output=None,
    ):
        self.context = context
        self.output_type = output_type
        self.operation = operation
        self.validate_output = validate_output
        self.corrections = 0
        from .office_tools import usable_definitions

        self.tools = {
            tool.name: tool
            for tool in usable_definitions(
                source_tools() if definitions is None else definitions, image_support=image_support
            )
        }
        if consult is not None:
            self.tools[consult.name] = consult
        self.connection_check = context is None and set(self.tools) == {"quantix_connection_check"}
        self.output = None
        self.failure = None
        self.calls = 0
        self.max_calls = max_calls
        self.image_support = image_support
        self.closed = False
        self.bridge_session_id = uuid4().hex
        self._lock = asyncio.Lock()
        self.token = secrets.token_urlsafe(36)
        register_activity_secrets(context, [self.token])
        self.url = None
        self.server = Server(
            "quantix",
            version="1.0.0",
            instructions=(
                "Call quantix_connection_check with an empty argument object, then submit its token string using quantix_submit_result. These are tools, not resources. No Tender data or MCP resources are available."
                if self.connection_check
                else "Read this Tender only. All findings and commercial values remain proposals for engineer review."
                if self.tools
                else "Use only the supplied conversation and saved status. Return the structured result."
            ),
            on_list_tools=self._list_tools,
            on_call_tool=self._call_tool,
        )

    @property
    def names(self) -> list[str]:
        return [*self.tools, SUBMIT_TOOL]

    async def _list_tools(self, _ctx, _params):
        tools = [
            types.Tool(
                name=tool.name,
                description=tool.description,
                inputSchema=tool.parameters,
                annotations=types.ToolAnnotations(
                    readOnlyHint=tool.read_only,
                    idempotentHint=tool.idempotent,
                    destructiveHint=False,
                    openWorldHint=False,
                ),
            )
            for tool in self.tools.values()
        ]
        tools.append(
            types.Tool(
                name=SUBMIT_TOOL,
                description=(
                    "Submit the unchanged connection-check value as the structured result. "
                    if self.connection_check
                    else "Submit the complete structured work proposal after reading its evidence. "
                    if self.tools
                    else "Submit the concise structured conversation result. "
                )
                + "This saves no domain decisions and does not approve anything.",
                inputSchema=self.output_type.model_json_schema(),
                annotations=types.ToolAnnotations(
                    readOnlyHint=True, destructiveHint=False, openWorldHint=False
                ),
            )
        )
        return types.ListToolsResult(tools=tools)

    async def _call_tool(self, _ctx, params):
        async with self._lock:
            if self.closed or self.failure:
                return types.CallToolResult(
                    isError=True, content=content_blocks("This Tender tool session has ended.")
                )
            self.calls += 1
            if self.calls > self.max_calls:
                self.failure = ValueError("The local client reached its Tender tool-call limit.")
                return types.CallToolResult(isError=True, content=content_blocks(str(self.failure)))
            try:
                if self.context is not None:
                    run = self.context.repo.get_run(self.context.run_id)
                    if run["status"] not in {"running", "queued"}:
                        raise InterruptedError("This Tender run is no longer active.")
                if params.name == SUBMIT_TOOL:
                    activity = ActivityRecorder(self.context)
                    proposal_operation = activity.start("tool", "Checking the submitted proposal.",
                        {"arguments": params.arguments or {}}, tool=SUBMIT_TOOL,
                        provider_call_id=str(getattr(_ctx, "request_id", "")) or None)
                    if self.output is not None:
                        activity.record(proposal_operation, "tool", "failed", "A proposal was already submitted.")
                        raise ValueError("A result has already been submitted for this run.")
                    try:
                        candidate = self.output_type.model_validate(params.arguments or {})
                    except ValueError:
                        activity.record(proposal_operation, "tool", "failed", "The submitted proposal did not match its required structure.")
                        raise
                    rejected = self._publication_problem(candidate)
                    if rejected is not None:
                        activity.record(proposal_operation, "tool", "failed", "The proposal needs correction.",
                            {"result": [part.model_dump(mode="json") for part in rejected.content]})
                        return rejected
                    self.output = candidate
                    activity.record(proposal_operation, "tool", "completed", "Proposal received for validation.",
                        {"result": "Proposal received for validation. Finish the current response; no further tools are required."})
                    return types.CallToolResult(
                        content=content_blocks(
                            "Proposal received for validation. Finish the current response; no further tools are required."
                        )
                    )
                if self.output is not None:
                    raise ValueError(
                        "The proposal has already been submitted. Finish the current response."
                    )
                tool = self.tools.get(params.name)
                if tool is None:
                    raise ValueError("This tool is not available in the active Tender.")
                if params.name == "view_document_page" and not self.image_support:
                    raise ValueError(
                        "Image support has not been established for this model. Use an approved vision-capable connection for drawing inspection."
                    )
                invocation_id = trusted_invocation_id(
                    self.bridge_session_id,
                    tool.name,
                    getattr(_ctx, "request_id", None),
                )
                if tool.requires_invocation_id and invocation_id is None:
                    raise ValueError("A trusted invocation identity is required for this tool.")
                if self.context is not None:
                    event_data = {
                        "tool": params.name,
                        "read_only": tool.read_only,
                    }
                    project_run_activity(
                        self.context.repo, self.context.run_id, "tool_started", event_data
                    )
                from .tool_policy import ToolFenceError, dispatch

                try:
                    with activity_scope(getattr(self.context, "_activity_parent_operation", None)):
                        result = await dispatch(
                            "mcp", tool, self.context, params.arguments or {}, invocation_id=invocation_id,
                        )
                except ToolFenceError as error:
                    raise ValueError(str(error)) from error
                return types.CallToolResult(content=content_blocks(result))
            except asyncio.CancelledError:
                raise
            except InterruptedError as exc:
                self.failure = exc
                return types.CallToolResult(isError=True, content=content_blocks(str(exc)))
            except ActivityRecordingError as exc:
                self.failure = exc
                return types.CallToolResult(isError=True, content=content_blocks(str(exc)))
            except (KeyError, ValueError) as exc:
                return types.CallToolResult(
                    isError=True, content=content_blocks(safe_text(exc, 1200))
                )
            except Exception:
                self.failure = RuntimeError(
                    "A Tender tool failed. The local-client result was not published."
                )
                return types.CallToolResult(isError=True, content=content_blocks(str(self.failure)))

    def _publication_problem(self, candidate):
        """Run the publication checks while the client can still correct them.

        The same checks run again after the run; here a wrong citation costs one
        more model turn instead of the whole run.
        """

        if self.validate_output is None:
            return None
        try:
            self.validate_output(candidate, [])
        except (KeyError, ValueError) as error:
            reason = safe_text(error.args[0] if error.args else error, 600)
            self.corrections += 1
            if self.corrections > MAX_SUBMIT_CORRECTIONS:
                self.failure = ValueError(
                    f"The AI could not correct its proposal. {reason} Its result was not published."
                )
                return types.CallToolResult(isError=True, content=content_blocks(str(self.failure)))
            return types.CallToolResult(
                isError=True,
                content=content_blocks(
                    f"The proposal was not accepted: {reason} Correct it and submit the complete proposal again."
                ),
            )
        return None

    def _text_result(self, text):
        """Accept the client's own closing message for the no-tools chat pass.

        A local client often answers a greeting as ordinary prose instead of
        calling the submit tool, and losing that answer is what makes the
        Manager look silent.  Only a conversation bridge, which exposes no
        source tools at all, may finish this way, and only as a conversation
        reply: plain text can never classify a request as engineering work or
        publish a record of any kind.
        """

        if self.operation != "conversation" or self.tools or not isinstance(text, str):
            return None
        reply = text.strip()
        if not reply:
            return None
        try:
            return self.output_type.model_validate({"kind": "conversation", "reply": reply[:6000]})
        except ValueError:
            return None

    def result(self, text=None):
        run_id = getattr(self.context, "run_id", None)
        if self.failure:
            record(
                "worker_submission",
                level="warning",
                phase="submission",
                outcome="failed",
                submission_status="rejected",
                tool_calls=self.calls,
                run_id=run_id,
            )
            raise self.failure
        if self.output is None:
            spoken = self._text_result(text)
            if spoken is not None:
                record(
                    "worker_submission",
                    phase="submission",
                    outcome="success",
                    submission_status="spoken",
                    tool_calls=self.calls,
                    run_id=run_id,
                )
                return spoken
            # Without this record a missing submission is indistinguishable
            # from a refusal: the tool-call count shows whether the client ever
            # reached the Tender tools before its turns ran out.
            record(
                "worker_submission",
                level="warning",
                phase="submission",
                outcome="missing",
                submission_status="missing",
                stop_reason="missing",
                tool_calls=self.calls,
                run_id=run_id,
            )
            raise ValueError(
                "The local client ended without submitting the required structured Tender proposal. "
                "Its approved request allowance may have run out before it could answer; "
                "raise the maximum model requests in Work > AI and spending."
            )
        record(
            "worker_submission",
            phase="submission",
            outcome="success",
            submission_status="received",
            tool_calls=self.calls,
            run_id=run_id,
        )
        return self.output

    @asynccontextmanager
    async def serve(self):
        app = self.server.streamable_http_app(json_response=True, stateless_http=True)
        expected = f"Bearer {self.token}".encode()

        async def authenticated(scope, receive, send):
            if scope["type"] == "http":
                headers = dict(scope.get("headers", []))
                if headers.get(b"origin") or not secrets.compare_digest(
                    headers.get(b"authorization", b""), expected
                ):
                    await Response(status_code=403)(scope, receive, send)
                    return
            await app(scope, receive, send)

        started = asyncio.Event()

        class BridgeServer(uvicorn.Server):
            async def startup(self, sockets=None):
                await super().startup(sockets=sockets)
                started.set()

            # This is an embedded server; the owning application owns signals.
            @contextmanager
            def capture_signals(self):
                yield

        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        sock.bind(("127.0.0.1", 0))
        sock.setblocking(False)
        self.url = f"http://127.0.0.1:{sock.getsockname()[1]}/mcp"
        server = BridgeServer(
            uvicorn.Config(
                authenticated,
                host="127.0.0.1",
                log_level="critical",
                access_log=False,
                lifespan="on",
                timeout_graceful_shutdown=3,
            )
        )
        task = asyncio.create_task(server.serve(sockets=[sock]))
        ready = asyncio.create_task(started.wait())
        try:
            done, _ = await asyncio.wait(
                {task, ready}, timeout=10, return_when=asyncio.FIRST_COMPLETED
            )
            if task in done:
                await task
                raise RuntimeError("The Tender tool bridge could not start.")
            if ready not in done or not server.started:
                raise RuntimeError("The Tender tool bridge did not become ready.")
            yield self
        finally:
            self.closed = True
            self.token = ""
            ready.cancel()
            server.should_exit = True
            try:
                await asyncio.wait_for(asyncio.shield(task), 5)
            except (TimeoutError, asyncio.CancelledError):
                task.cancel()
                await asyncio.gather(task, return_exceptions=True)
            finally:
                sock.close()


class WorkerControlBridge(RuntimeToolBridge):
    """Private worker control: never included in the model-visible tool catalog.

    A different port and bearer token separate budgeting, event accounting and
    proposal retrieval from the source/proposal capabilities given to a model.
    The core keeps the real OfficeContext and every repository write.
    """

    def __init__(self, source, connection, route, *, before_request=None, on_response=None):
        self.source = source
        self.context = source.context
        self.connection = connection
        self.route = route
        self.before_request = before_request
        self.on_response = on_response
        self.closed = False
        self.token = secrets.token_urlsafe(36)
        register_activity_secrets(self.context, [self.token])
        self.url = None
        self.reservations = {}
        self.activity_requests = {}
        self.requests = 0
        self._lock = asyncio.Lock()
        self.server = Server(
            "quantix-worker-control",
            version="1.0.0",
            on_list_tools=self._list_tools,
            on_call_tool=self._call_tool,
        )

    async def _list_tools(self, _ctx, _params):
        # Each entry carries its own required list: the closing message on
        # "result" is optional, so a client that submitted a structured
        # proposal still calls it with no arguments at all.
        schemas = {
            "reserve": (
                {
                    "input_allowance": {"type": "integer", "minimum": 0},
                    "output_limit": {"type": "integer", "minimum": 1},
                    "requests": {"type": "integer", "minimum": 1},
                },
                ["input_allowance", "output_limit", "requests"],
            ),
            "usage": (
                {"usage": {"type": "object"}, "reservation_id": {"type": "string"}},
                ["usage", "reservation_id"],
            ),
            "event": (
                {
                    "kind": {"type": "string"},
                    "message": {"type": "string"},
                    "data": {"type": "object"},
                },
                ["kind", "message", "data"],
            ),
            "result": ({"text": {"type": "string"}}, []),
        }
        return types.ListToolsResult(
            tools=[
                types.Tool(
                    name=name,
                    description="Private Quantix worker control operation.",
                    inputSchema={
                        "type": "object",
                        "properties": properties,
                        "required": required,
                        "additionalProperties": False,
                    },
                )
                for name, (properties, required) in schemas.items()
            ]
        )

    async def _call_tool(self, _ctx, params):
        async with self._lock:
            try:
                if self.closed:
                    raise ValueError("The worker control session has ended.")
                arguments = params.arguments or {}
                if params.name == "reserve":
                    ActivityRecorder(self.context).require_recording()
                    if self.source.failure or self.source.closed:
                        raise InterruptedError("The source session is no longer available.")
                    if self.context is not None:
                        run = self.context.repo.get_run(self.context.run_id)
                        if run["status"] not in {"running", "queued"}:
                            raise InterruptedError("This Tender run is no longer active.")
                    allowance, output, requests = (
                        arguments.get(name)
                        for name in ("input_allowance", "output_limit", "requests")
                    )
                    if (
                        any(type(value) is not int for value in (allowance, output, requests))
                        or allowance < 0
                        or output < 1
                        or requests < 1
                    ):
                        raise ValueError("The worker supplied invalid request allowances.")
                    maximum = int(
                        self.connection.get("_execution_limits", {}).get("max_requests", 12)
                    )
                    if (
                        self.requests + requests > maximum
                        or output > self.route["max_output_tokens"]
                    ):
                        # The most common cause of a local client ending with no
                        # proposal: record what it had used against its ceiling.
                        record(
                            "worker_request_limit",
                            level="warning",
                            phase="reserve",
                            outcome="refused",
                            protocol=self.connection.get("protocol"),
                            model=self.route.get("model_id"),
                            requests_used=self.requests,
                            requests=requests,
                            max_requests=maximum,
                            run_id=getattr(self.context, "run_id", None),
                        )
                        raise ValueError("The worker reached the approved model request limit.")
                    reservation = (
                        await self.before_request(allowance, output, requests=requests)
                        if self.before_request
                        else None
                    )
                    key = reservation or uuid4().hex
                    if key in self.reservations:
                        raise ValueError("The budget reservation was already used in this worker.")
                    self.reservations[key] = reservation
                    self.requests += requests
                    value = {"reservation_id": key}
                elif params.name == "usage":
                    key = arguments.get("reservation_id")
                    if key not in self.reservations:
                        raise ValueError("The worker supplied an unknown budget reservation.")
                    usage = arguments.get("usage")
                    if not isinstance(usage, dict):
                        raise ValueError("The worker supplied invalid usage details.")
                    for name in ("requests", "input_tokens", "output_tokens"):
                        if type(usage.get(name)) is not int or usage[name] < 0:
                            raise ValueError("The worker supplied invalid usage counts.")
                    if type(usage.get("usage_complete")) is not bool:
                        raise ValueError("The worker did not establish usage completeness.")
                    native_binding = self.connection.get("_native_session_binding")
                    if native_binding and self.context is not None and usage.get("session_id") is not None:
                        from .native_execution import NativeExecutionService
                        NativeExecutionService(self.context.repo).finish(self.context, native_binding["id"],
                            provider_session_id=usage["session_id"], state="running")
                    if self.on_response:
                        await self.on_response(usage, self.reservations[key])
                    self.reservations.pop(key)
                    value = {"recorded": True}
                elif params.name == "event":
                    kind = arguments.get("kind")
                    if kind not in {"model_response", "runtime_model_reported", "runtime_waiting", "runtime_session", "runtime_tool_activity", "assistant_text_delta", "assistant_reasoning_summary", "runtime_request_activity", "runtime_public_section"}:
                        raise ValueError("This worker event is unsupported.")
                    if kind == "runtime_request_activity":
                        data = arguments.get("data") or {}
                        request_id, phase = data.get("request_id"), data.get("phase")
                        if not isinstance(request_id, str) or len(request_id) > 200 or phase not in {"prepared", "started", "completed", "failed", "cancelled", "observed"}:
                            raise ValueError("The original client supplied an invalid request activity boundary.")
                        recorder = ActivityRecorder(self.context)
                        operation = self.activity_requests.get(request_id)
                        if phase == "prepared":
                            if operation:
                                raise ValueError("This original-client request was already prepared.")
                            payload = dict(data.get("payload") or {})
                            catalogue = await self.source._list_tools(None, None)
                            payload["quantix_tool_catalogue"] = [tool.model_dump(mode="json", by_alias=True) for tool in catalogue.tools]
                            payload["quantix_tool_instructions"] = self.source.server.instructions
                            operation = recorder.start("model_request", "Content supplied by Quantix", payload,
                                phase="prepared", provider=self.connection.get("provider_id"), model=self.route.get("model_id"),
                                capture_status="partial", unavailable_fields=["provider_internal_prompt", "provider_per_sampling_requests"])
                            self.activity_requests[request_id] = operation
                            if self.context is not None:
                                self.context._activity_parent_operation = operation
                            available = self.connection.get("protocol") == "codex"
                            recorder.record(operation, "capability", "observed", "The original client can supply thinking summaries." if available else "This original client does not expose a documented thinking summary stream.",
                                {"supported": available, "detail": "Public summaries are retained when supplied." if available else "The Grok client does not document a public reasoning summary stream."},
                                capture_status="complete" if available else "unavailable", unavailable_fields=[] if available else ["reasoning_summary"])
                        else:
                            if operation is None:
                                raise ValueError("The original-client request has not been prepared.")
                            payload = data.get("payload") or {}
                            if phase == "observed" and isinstance(payload.get("provider_tool_error"), dict):
                                failure = payload["provider_tool_error"]
                                recorder.start("provider_tool", "The original client reported a tool error.", failure,
                                    phase="failed", parent_operation_id=operation, tool=failure.get("tool"),
                                    provider_call_id=failure.get("provider_call_id"), capture_status="partial")
                            else:
                                recorder.record(operation, "model_request", phase, "Original-client request " + phase + ".", payload, capture_status="partial")
                    elif kind in {"assistant_text_delta", "assistant_reasoning_summary", "runtime_public_section"}:
                        from .native_execution import project_client_event
                        data = arguments.get("data") or {}
                        if data.get("request_id") and data["request_id"] not in self.activity_requests:
                            raise ValueError("The original-client public section has no prepared request.")
                        project_client_event(self.context, kind, data, request_operation=self.activity_requests.get(data.get("request_id")))
                    elif kind == "runtime_tool_activity":
                        data = arguments.get("data") or {}
                        tool = data.get("tool")
                        allowed = {*self.source.names, *("quantix__" + name for name in self.source.names), "search_tool", "use_tool"}
                        if tool not in allowed or data.get("phase") not in {"started", "completed"}:
                            raise ValueError("The original client supplied an unsupported tool activity event.")
                        if self.context is not None:
                            values = {"tool": tool, "phase": data["phase"], "origin": "client" if tool in {"search_tool", "use_tool"} else "quantix"}
                            values.update({name: data[name] for name in ("provider_call_id", "request_id") if isinstance(data.get(name), str)})
                            for name in ("actor_id", "assignment_id"):
                                if getattr(self.context, name, None):
                                    values[name] = getattr(self.context, name)
                            if values["origin"] == "client":
                                self.context.repo.event(self.context.run_id, kind, "Finding an approved tool.", values)
                    elif kind == "runtime_session":
                        from .native_execution import NativeExecutionService
                        expected = self.connection.get("_native_session_binding") or {}
                        data = arguments.get("data") or {}
                        if self.context is None or not expected or data.get("binding_id") != expected.get("id"):
                            raise ValueError("The native session event does not belong to this execution.")
                        NativeExecutionService(self.context.repo).finish(self.context, expected["id"],
                            provider_session_id=data.get("provider_session_id"), state="running")
                    elif self.context is not None:
                        message = {
                            "model_response": "Model response received.",
                            "runtime_model_reported": "The original client reported its selected model.",
                            "runtime_waiting": "Waiting for this AI account's current work to finish.",
                        }[kind]
                        data = arguments.get("data") or {}
                        self.context.repo.event(self.context.run_id, kind, message, data)
                        operation = self.activity_requests.get(data.get("request_id"))
                        if operation and kind == "model_response":
                            ActivityRecorder(self.context).record(operation, "model_output", "observed", message,
                                {name: data[name] for name in ("round", "usage", "provider_response_id") if name in data}, capture_status="partial")
                        project_run_activity(self.context.repo, self.context.run_id, kind, data)
                    value = {"recorded": True}
                elif params.name == "result":
                    spoken = arguments.get("text")
                    value = {
                        "output": self.source.result(
                            spoken if isinstance(spoken, str) else None
                        ).model_dump(mode="json")
                    }
                else:
                    raise ValueError("This private worker operation is unsupported.")
                return types.CallToolResult(content=content_blocks(value), structuredContent=value)
            except asyncio.CancelledError:
                raise
            except (ValueError, KeyError, InterruptedError) as exc:
                if isinstance(exc, (InterruptedError, ActivityRecordingError)):
                    self.source.failure = exc
                return types.CallToolResult(
                    isError=True, content=content_blocks(safe_text(exc, 1200))
                )
            except Exception as exc:
                record_exception("worker_control_failed", exc, phase=params.name,
                                 run_id=getattr(self.context, "run_id", None))
                return types.CallToolResult(
                    isError=True,
                    content=content_blocks("The local worker control operation could not finish."),
                )
