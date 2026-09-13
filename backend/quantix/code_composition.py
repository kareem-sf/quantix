"""Monty subprocess composition of a controller-supplied exact tool set.

No host objects, mounts, environment or arbitrary callbacks are exposed. The
controller must pass its already-authorized ToolDefinitions and live context.
Every call goes through ToolDefinition.invoke, including mutation identities.
"""

from __future__ import annotations

import asyncio
import importlib.metadata
import inspect
import json
import sys
import time
import uuid
from dataclasses import replace
from datetime import UTC, datetime
from pathlib import Path

from .ai_tools import ToolDefinition, trusted_invocation_id
from .code_composition_models import CompositionReceipt, CompositionRequest
from .local_code_records import write_local_record
from .sandbox_protocol import SandboxFailure, sha256

MONTY_VERSION = "0.0.23"


def monty_binary() -> Path:
    version = importlib.metadata.version("pydantic-monty-runtime")
    if version != MONTY_VERSION:
        raise SandboxFailure(f"Install the reviewed Monty runtime {MONTY_VERSION}.")
    scripts = Path(sys.executable).parent
    binary = scripts / ("monty.exe" if sys.platform == "win32" else "monty")
    if not binary.is_file():
        raise SandboxFailure("Install the bundled Monty runtime to compose tools.")
    return binary.resolve()


class CodeCompositionService:
    def __init__(self, home: Path):
        self.home = Path(home)

    async def execute(
        self,
        request: CompositionRequest,
        *,
        context,
        tools: list[ToolDefinition],
        authority_check=None,
        limits=None,
    ) -> CompositionReceipt:
        from pydantic_monty import AsyncMonty, CollectString

        from .tool_policy import dispatch

        seconds, memory, tool_limit = (
            (limits.seconds, limits.memory_mib, limits.tool_calls) if limits else (10, 64, 64)
        )
        if not (1 <= seconds <= 10 and 16 <= memory <= 64 and 0 <= tool_limit <= 64):
            raise ValueError("Composition limits exceed the reviewed interpreter ceiling.")
        if authority_check:
            authority_check()

        binary = monty_binary()
        encoded = json.dumps(request.inputs, ensure_ascii=False, allow_nan=False).encode()
        if len(encoded) > 1024 * 1024:
            raise ValueError("The composition inputs exceed 1 MiB. Select a smaller window.")
        if len({tool.name for tool in tools}) != len(tools):
            raise ValueError("Composition tool names must be unique.")
        identifier = uuid.uuid4().hex
        started = time.monotonic()
        receipt = CompositionReceipt(
            id=identifier,
            status="failed",
            engine_version=MONTY_VERSION,
            runtime_sha256=sha256(binary.read_bytes()),
            code_sha256=sha256(request.code.encode()),
            inputs_sha256=sha256(encoded),
            tool_names=[tool.name for tool in tools],
            created_at=datetime.now(UTC).isoformat(),
            duration_seconds=0,
        )
        printed = CollectString(max_bytes=1024 * 1024)
        calls = []

        def persist(phase, *, running=True):
            write_local_record(
                self.home / "code" / "receipts" / f"{identifier}.json",
                {
                    **receipt.model_dump(mode="json"),
                    "status": "running" if running else receipt.status,
                    "code": request.code,
                    "inputs": request.inputs,
                    "inputs_encoded_json": encoded.decode("utf-8"),
                    "calls": calls,
                    "limits": {"seconds": seconds, "memory_mib": memory, "tool_calls": tool_limit},
                },
                context=context,
                engine="monty",
                phase=phase,
                runtime_fingerprint=getattr(limits, "fingerprint", None),
            )

        persist("starting")

        def expose(definition):
            async def structured_result(tool_context, **arguments):
                result = definition.function(tool_context, **arguments)
                if inspect.isawaitable(result):
                    result = await result
                if hasattr(result, "model_dump"):
                    result = result.model_dump(mode="json")
                if isinstance(result, str):
                    try:
                        result = json.loads(result)
                    except json.JSONDecodeError:
                        pass
                return result

            # Preserve exact argument/effect metadata and invocation identity.
            # Conversion happens inside the nested fence, before validation and
            # deferred-read commit, not after a source receipt has been saved.
            nested_definition = replace(definition, function=structured_result)

            async def invoke(**arguments):
                if receipt.tool_calls >= tool_limit:
                    raise SandboxFailure("The composition reached its 64 tool-call limit.")
                if authority_check:
                    authority_check()
                receipt.tool_calls += 1
                invocation = trusted_invocation_id(identifier, definition.name, receipt.tool_calls)
                call = {
                    "tool": definition.name,
                    "arguments_sha256": sha256(
                        json.dumps(arguments, sort_keys=True, allow_nan=False).encode()
                    ),
                    "invocation_id": invocation,
                    "status": "attempted",
                }
                calls.append(call)
                persist("calling_reviewed_tool")
                result = await dispatch(
                    "nested", nested_definition, context, arguments, invocation_id=invocation
                )
                if hasattr(result, "model_dump"):
                    result = result.model_dump(mode="json")
                data = json.dumps(result, allow_nan=False).encode()
                if len(data) > 1024 * 1024:
                    raise SandboxFailure("A tool returned too much data. Select a smaller window.")
                call.update(status="completed", result_sha256=sha256(data))
                persist("composing")
                return json.loads(data)

            return invoke

        try:
            async with asyncio.timeout(seconds):
                async with AsyncMonty(
                    binary_path=str(binary),
                    min_processes=1,
                    max_processes=1,
                    request_timeout=seconds,
                    max_checkouts_per_worker=1,
                ) as pool:
                    async with pool.checkout(
                        limits={
                            "max_memory": memory * 1024 * 1024,
                            "max_duration_secs": seconds,
                            "max_suspensions": 192,
                            "max_recursion_depth": 100,
                        }
                    ) as session:
                        output = await session.feed_run(
                            request.code,
                            inputs=request.inputs,
                            external_lookup={tool.name: expose(tool) for tool in tools},
                            print_callback=printed,
                        )
                        data = json.dumps(output, allow_nan=False).encode()
                        if len(data) + len(printed.output.encode()) > 1024 * 1024:
                            raise SandboxFailure("The composition result exceeds 1 MiB.")
                        receipt.output = json.loads(data)
                        if authority_check:
                            authority_check()
                        receipt.status = "completed"
        except asyncio.CancelledError:
            receipt.status = "cancelled"
            receipt.detail = (
                "Tool composition was stopped. Inspect attempted tool receipts before retrying."
            )
            raise
        except Exception as error:
            receipt.detail = str(error)[:2000] or "The composition could not complete."
        finally:
            receipt.printed = printed.output
            receipt.duration_seconds = round(time.monotonic() - started, 4)
            persist(receipt.status, running=False)
        return receipt
