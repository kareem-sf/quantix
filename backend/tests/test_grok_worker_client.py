"""Focused checks for preserving sanitized worker failures through MCP groups."""

import sys
from pathlib import Path
from types import SimpleNamespace

sys.path.insert(0, str(Path(__file__).parents[1] / "ai_worker"))

from quantix_ai_worker.common import RuntimeConnectionFailure as WorkerRuntimeConnectionFailure
from quantix_ai_worker.common import RuntimeUnavailable as WorkerRuntimeUnavailable
from quantix_ai_worker.grok import (
    GROK_CHECK_MAX_ROUNDS as WORKER_GROK_CHECK_MAX_ROUNDS,
)
from quantix_ai_worker.grok import (
    _safe_failure_message,
    classify_provider_error,
    headless_command,
    model_identity_matches,
    update_usage,
)
from quantix_ai_worker.grok_common import GROK_CHECK_MAX_ROUNDS, configure
from quantix_ai_worker.server import GROK_CHECK_MAX_ROUNDS as SERVER_GROK_CHECK_MAX_ROUNDS
from quantix_ai_worker.server import _nested_known_failure as _nested_worker_failure

from quantix.ai_runtime_common import RuntimeConnectionFailure, RuntimeUnavailable
from quantix.ai_setup import GROK_CHECK_MAX_ROUNDS as SETUP_GROK_CHECK_MAX_ROUNDS
from quantix.ai_worker_client import (
    GROK_CHECK_MAX_ROUNDS as CORE_GROK_CHECK_MAX_ROUNDS,
)
from quantix.ai_worker_client import (
    ModelWorkerFailure,
    _ConnectionCheckOutput,
    _nested_known_failure,
    connection_check_instruction,
)
from quantix.diagnostics import DiagnosticWriter


def test_nested_runtime_failure_survives_mcp_exception_group():
    failure = RuntimeUnavailable("The selected model is unavailable.")
    wrapped = ExceptionGroup("MCP cleanup", [ExceptionGroup("stdio", [failure])])

    assert _nested_known_failure(wrapped) is failure


def test_nested_connection_failure_survives_mcp_exception_group():
    failure = RuntimeConnectionFailure("The worker connection closed.")
    wrapped = ExceptionGroup("MCP cleanup", [failure])

    assert _nested_known_failure(wrapped) is failure


def test_worker_boundary_preserves_sanitized_nested_runtime_failure():
    failure = WorkerRuntimeUnavailable("The Grok operation could not complete.")
    wrapped = ExceptionGroup("execution cleanup", [ExceptionGroup("stdio", [failure])])

    assert _nested_worker_failure(wrapped) is failure


def test_worker_boundary_preserves_sanitized_nested_connection_failure():
    failure = WorkerRuntimeConnectionFailure("The provider connection closed.")
    wrapped = ExceptionGroup("execution cleanup", [failure])

    assert _nested_worker_failure(wrapped) is failure


def test_unknown_exception_group_stays_unknown_for_generic_boundary():
    wrapped = ExceptionGroup("MCP cleanup", [OSError("transport detail")])

    assert _nested_known_failure(wrapped) is None
    assert _nested_known_failure(ModelWorkerFailure("already sanitized")) is not None


def test_headless_command_uses_top_level_profile_and_headless_options():
    command = headless_command(
        ["grok"],
        {"model_id": "grok-4.5", "reasoning": None},
        3,
        "C:/private/profile.md",
        "C:/private/prompt.txt",
    )

    assert command[0] == "grok"
    assert command[command.index("--model") + 1] == "grok-4.5"
    assert "--agent" in command
    assert command[command.index("--agent") + 1] == "C:/private/profile.md"
    assert "--prompt-file" in command
    assert "--no-leader" in command
    assert "--no-auto-update" in command
    assert "--agent-profile" not in command


def test_grok_build_public_model_accepts_only_explicit_reported_alias():
    assert model_identity_matches("grok-4.5", "grok-4.5")
    assert model_identity_matches("grok-4.5", "grok-4.5-build")
    assert not model_identity_matches("grok-4.5", "grok-4.6-build")
    assert not model_identity_matches("grok-4.5", "grok-4.5-latest")
    assert not model_identity_matches("grok-4.6", "grok-4.6-build")
    assert not model_identity_matches("grok-4.5", None)


def test_usage_accepts_explicit_build_alias_and_rejects_other_model():
    usage = {
        "requests": 0,
        "input_tokens": 0,
        "output_tokens": 0,
        "usage_complete": False,
    }
    update_usage(
        usage,
        {
            "usage": {
                "input_tokens": 10,
                "output_tokens": 5,
                "cache_read_input_tokens": 0,
                "cache_creation_input_tokens": 0,
            },
            "modelUsage": {"grok-4.5-build": {}},
            "num_turns": 1,
        },
        "grok-4.5",
        3,
    )
    assert usage["actual_model"] == "grok-4.5-build"
    assert usage["usage_complete"] is True

    try:
        update_usage(
            usage,
            {
                "usage": {
                    "input_tokens": 10,
                    "output_tokens": 5,
                    "cache_read_input_tokens": 0,
                    "cache_creation_input_tokens": 0,
                },
                "modelUsage": {"grok-4.6-build": {}},
                "num_turns": 1,
            },
            "grok-4.5",
            3,
        )
    except ValueError as error:
        assert "different or additional model" in str(error)
    else:
        raise AssertionError("A different model identity must remain rejected")


def test_grok_check_round_limit_is_mirrored_across_setup_core_and_worker():
    assert GROK_CHECK_MAX_ROUNDS == 5
    assert SETUP_GROK_CHECK_MAX_ROUNDS == GROK_CHECK_MAX_ROUNDS
    assert CORE_GROK_CHECK_MAX_ROUNDS == GROK_CHECK_MAX_ROUNDS
    assert WORKER_GROK_CHECK_MAX_ROUNDS == GROK_CHECK_MAX_ROUNDS
    assert SERVER_GROK_CHECK_MAX_ROUNDS == GROK_CHECK_MAX_ROUNDS


def test_grok_bridge_only_scopes_read_denial_and_keeps_account_deny_strict(tmp_path):
    bridge = SimpleNamespace(url="http://127.0.0.1:43123/mcp")
    configure(tmp_path / "bridge", model="grok-4.5", bridge=bridge)
    bridge_config = (tmp_path / "bridge" / "grok" / "config.toml").read_text()
    assert '"Read(**)"' in bridge_config
    assert '"Read", "Write"' not in bridge_config
    assert 'allow = ["MCPTool(quantix__*)"]' in bridge_config

    configure(tmp_path / "account", model="grok-4.5")
    account_config = (tmp_path / "account" / "grok" / "config.toml").read_text()
    assert (
        'deny = ["Bash", "Read", "Write", "Edit", "Grep", "WebFetch", "MCPTool"]' in account_config
    )


def test_grok_check_instruction_requires_joint_discovery_and_one_unchanged_submission():
    instruction = connection_check_instruction("grok_build")
    lowered = instruction.lower()
    assert "discover both quantix tools together" in lowered
    assert "call quantix_connection_check exactly once" in lowered
    assert "submit its returned value unchanged" in lowered
    assert "finish without further tools" in lowered
    assert "value =" not in lowered


def test_non_grok_check_instruction_keeps_existing_contract():
    instruction = connection_check_instruction("openai")
    assert instruction == (
        "Perform this connection check only. Call quantix_connection_check exactly once, "
        "then return its value unchanged in a structured object with the single property value. "
        "Do not use any other account capabilities, tools or web access."
    )


def test_grok_check_host_limit_and_instruction_are_sent_to_worker():
    observed = {}
    client = __import__("quantix.ai_worker_client", fromlist=["AIWorkerClient"]).AIWorkerClient(
        SimpleNamespace()
    )

    async def fake_execute(*args, **kwargs):
        observed["connection"] = args[1]
        observed["instruction"] = args[4]
        return {
            "output": _ConnectionCheckOutput(value="opaque"),
            "usage": {"actual_model": "grok-4.5"},
        }

    client._execute = fake_execute

    import asyncio

    result = asyncio.run(
        client.check(
            {"connection_id": "connection", "model_id": "grok-4.5", "max_output_tokens": 1024},
            {"protocol": "grok_build"},
            {},
        )
    )

    assert observed["connection"]["_execution_limits"] == {
        "max_requests": 5,
        "max_output_tokens": 1024,
    }
    assert observed["instruction"] == connection_check_instruction("grok_build")
    assert result["output_supported"] is False


def test_grok_failure_messages_separate_round_exhaustion_and_provider_error():
    max_rounds = _safe_failure_message("max_turns", operation="check")
    provider = _safe_failure_message("provider_error", operation="check")
    assert "5-round" in max_rounds
    assert "provider error" in provider
    assert max_rounds != provider
    assert "raw provider detail" not in provider


def test_grok_error_classification_keeps_only_allowlisted_category_and_status():
    details = classify_provider_error(
        {
            "type": "error",
            "category": "rate_limit_exceeded",
            "status_code": 429,
            "message": "raw provider detail with sk-secret and customer.txt",
        }
    )

    assert details == {"category": "rate_limit", "status_code": 429}
    message = _safe_failure_message(details["category"], operation="execute")
    assert "provider limits" in message.lower() or "reset time" in message.lower()
    assert "raw provider detail" not in message
    assert "sk-secret" not in message
    assert "customer.txt" not in message


def test_grok_unknown_error_keeps_generic_failure_and_no_status():
    details = classify_provider_error(
        {
            "type": "error",
            "category": "provider-specific-secret",
            "status_code": "500",
            "message": "private response body",
        }
    )

    assert details == {"category": "unknown", "status_code": None}
    assert _safe_failure_message(details["category"], operation="check") == (
        "Grok reported a provider error during the connection check. Retry the access check."
    )


def test_grok_forbidden_status_is_access_denied_not_authentication_failure():
    details = classify_provider_error({"type": "error", "status_code": 403})

    assert details == {"category": "access_denied", "status_code": 403}
    assert "subscription and model permissions" in _safe_failure_message(
        details["category"], operation="execute"
    )
    assert classify_provider_error({"type": "error", "status_code": 401}) == {
        "category": "authentication_failed",
        "status_code": 401,
    }


def test_diagnostics_preserves_documented_grok_terminal_reasons(tmp_path):
    writer = DiagnosticWriter(tmp_path, component="test")
    try:
        for reason in ("max_tokens", "max_turn_requests", "refusal", "max_turns", "error"):
            assert writer._field("stop_reason", reason) == reason
    finally:
        writer.close()
