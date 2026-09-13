"""T013: unified capability fence across direct, MCP and nested calls."""

from __future__ import annotations

import pytest

from quantix.execution_context import OfficeExecutionIdentity


def test_t013(run_case):
    result = run_case(
        "T013",
        {"scenario": "unified_tool_fence", "entrypoints": ["direct", "mcp", "nested"]},
    )
    assert result["denials"] == 3
    assert result["scope_widened"] is False
    assert result["accepted_invalid_outputs"] == 0


def test_t013_timeout_preserves_request_identity(run_case):
    result = run_case(
        "T013",
        {"scenario": "unified_tool_fence", "entrypoints": ["nested"]},
    )
    assert result["timeout_uncertain"] is True
    assert result["timeout_request_id"] == "t013-timeout-kept"


def test_t013_identity_is_server_only():
    identity = OfficeExecutionIdentity(
        tender_id="tender",
        actor_kind="staff",
        actor_id="staff-1",
        root_run_id="run-1",
        budget_scope_id="run-1",
        assignment_id="assign-1",
        profile_version=1,
        route_binding_id="bind-1",
        instruction_revision_id=None,
        grant_fingerprint=None,
        ownership_epoch=None,
        trusted_invocation_id="qti-test",
    )
    with pytest.raises(AttributeError):
        identity.tender_id = "other"  # type: ignore[misc]
