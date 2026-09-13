"""Regression checks from the independent C/D boundary review."""

import asyncio
import os
import signal
import sys
import time

import pytest

from quantix.podman_runtime import PodmanRuntime
from quantix.sandbox_protocol import CommandRunner, SandboxFailure


@pytest.mark.asyncio
async def test_real_monty_callback_uses_source_result_fence_and_rolls_back(tmp_path, monkeypatch):
    from test_tool_policy_reads import _definition, _source, _workspace

    import quantix.tool_policy as policy
    from quantix.code_composition import CodeCompositionService
    from quantix.code_composition_models import CompositionRequest
    from quantix.office_tools import OfficeContext

    repo, tender_id, run_id = _workspace(tmp_path)
    source_id = _source(repo, tender_id)
    context = OfficeContext(repo, tender_id, run_id)
    monkeypatch.setattr(policy, "MAX_RESULT_BYTES", 10)
    result = await CodeCompositionService(repo.home).execute(
        CompositionRequest(
            code="await read_source(source_id=source_id)", inputs={"source_id": source_id}
        ),
        context=context,
        tools=[_definition("read_source")],
    )
    assert result.status == "failed"
    assert not context.seen_sources
    assert not [event for event in repo.run_events(run_id) if event["kind"] == "sources_read"]
    monkeypatch.setattr(policy, "MAX_RESULT_BYTES", 1024 * 1024)
    result = await CodeCompositionService(repo.home).execute(
        CompositionRequest(
            code="await read_source(source_id=source_id)", inputs={"source_id": source_id}
        ),
        context=context,
        tools=[_definition("read_source")],
    )
    assert result.status == "completed"
    assert context.seen_sources == {source_id}


def test_runtime_proof_rejects_changed_binary_or_raised_limit(tmp_path, monkeypatch):
    from types import SimpleNamespace

    from quantix.code_runtime_scope import available_code_runtimes, validate_code_runtime

    repo = SimpleNamespace(home=tmp_path)
    proof = available_code_runtimes(repo)[0]
    changed = proof.model_copy(update={"fingerprint": "0" * 64})
    with pytest.raises(ValueError, match="changed"):
        validate_code_runtime(repo, changed)
    with pytest.raises(ValueError, match="ceiling"):
        validate_code_runtime(repo, proof.model_copy(update={"seconds": 300}))


@pytest.mark.asyncio
async def test_exited_parent_cannot_keep_timeout_open_with_descendant_pipes(tmp_path):
    runner = CommandRunner()
    source = (
        "import subprocess,sys,pathlib,time;time.sleep(.05);"
        "p=subprocess.Popen([sys.executable,'-c','import time;time.sleep(30)']);"
        "pathlib.Path('child.pid').write_text(str(p.pid))"
    )
    started = time.monotonic()
    try:
        with pytest.raises(SandboxFailure, match="time limit"):
            await asyncio.wait_for(
                runner.run([sys.executable, "-c", source], cwd=tmp_path, env={}, timeout=0.25),
                timeout=3,
            )
        assert time.monotonic() - started < 2
        assert not runner.active
    finally:
        marker = tmp_path / "child.pid"
        if marker.exists():
            try:
                os.kill(int(marker.read_text()), signal.SIGTERM)
            except (ProcessLookupError, PermissionError, OSError):
                pass


@pytest.mark.asyncio
async def test_nested_runtime_lease_remains_owned_and_blocks_maintenance(tmp_path):
    runtime = PodmanRuntime(tmp_path)
    async with runtime.operation():
        async with runtime.operation():
            assert asyncio.current_task() in runtime.operations
        assert asyncio.current_task() in runtime.operations
        with pytest.raises(SandboxFailure, match="active|finish"):
            await runtime.action("remove")
    assert not runtime.operations


@pytest.mark.asyncio
async def test_maintenance_lock_denies_new_execution(tmp_path):
    runtime = PodmanRuntime(tmp_path)
    async with runtime.lock:
        with pytest.raises(SandboxFailure, match="setup|maintenance|finish"):
            async with runtime.operation():
                pytest.fail("An execution entered runtime maintenance.")
