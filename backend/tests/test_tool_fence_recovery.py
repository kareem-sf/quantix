"""A tool's complaint about its own call is the model's to correct; refused authority is not."""

from types import SimpleNamespace

import pytest
from pydantic_ai.exceptions import ModelRetry

from quantix.ai_api_engine import LocalToolBridge
from quantix.ai_api_errors import DirectAPIError
from quantix.capability_models import ExecutionReceipt
from quantix.tool_policy import ToolFenceError


def _fence(detail: str, *, recoverable: bool) -> ToolFenceError:
    return ToolFenceError(
        ExecutionReceipt(
            status="blocked",
            capability_id="read_document",
            capability_version=1,
            request_id="synthetic-request",
            limitations=[detail],
        ),
        recoverable=recoverable,
    )


def _invoke_with(monkeypatch, error: ToolFenceError):
    definition = SimpleNamespace(
        name="read_document", requires_invocation_id=False, description="synthetic", parameters={}
    )

    async def refuse(*_args, **_kwargs):
        raise error

    monkeypatch.setattr("quantix.tool_policy.dispatch", refuse)
    return LocalToolBridge(None, None, [definition]).adapt(definition)


@pytest.mark.asyncio
async def test_tool_argument_complaints_go_back_to_the_model(monkeypatch):
    invoke = _invoke_with(
        monkeypatch, _fence("Use a nonnegative offset and a limit from 1 to 20.", recoverable=True)
    )

    with pytest.raises(ModelRetry, match="limit from 1 to 20"):
        await invoke(SimpleNamespace(tool_call_id=None), artifact_id="a", offset=0, limit=50)


@pytest.mark.asyncio
async def test_refused_authority_reports_its_reason_not_a_provider_failure(monkeypatch):
    invoke = _invoke_with(
        monkeypatch,
        _fence("The tool 'read_document' is not granted for this staff assignment.", recoverable=False),
    )

    with pytest.raises(DirectAPIError, match="not granted"):
        await invoke(SimpleNamespace(tool_call_id=None), artifact_id="a")


def test_only_tool_side_complaints_are_marked_recoverable():
    assert _fence("Use a nonnegative offset.", recoverable=True).recoverable is True
    assert _fence("Refused.", recoverable=False).recoverable is False
