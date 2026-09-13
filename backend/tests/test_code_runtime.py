import asyncio
import hashlib
import json
import sys
from types import SimpleNamespace

import pytest

from quantix.ai_tools import tool
from quantix.code_composition import CodeCompositionService
from quantix.code_composition_models import CompositionRequest
from quantix.python_analysis_models import PythonLimits
from quantix.sandbox_protocol import CommandResult, CommandRunner, SandboxFailure, safe_name


@pytest.mark.asyncio
async def test_real_monty_composes_only_given_tools(tmp_path):
    observed = []

    @tool
    def rate(ctx, quantity: int) -> int:
        observed.append(ctx.context)
        return quantity * 7

    service = CodeCompositionService(tmp_path)
    result = await service.execute(
        CompositionRequest(code="await rate(quantity=6)"), context="approved-context", tools=[rate]
    )
    assert result.output == 42
    assert observed == ["approved-context"]
    assert result.tool_calls == 1
    assert result.code_sha256 == hashlib.sha256(b"await rate(quantity=6)").hexdigest()
    assert (tmp_path / "code" / "receipts" / f"{result.id}.json").is_file()


@pytest.mark.asyncio
async def test_real_monty_denies_host_files_and_missing_tools(tmp_path):
    service = CodeCompositionService(tmp_path)
    for code in ["open('/etc/passwd').read()", "unknown_tool()", "import subprocess"]:
        result = await service.execute(CompositionRequest(code=code), context=None, tools=[])
        assert result.status == "failed"
        assert result.output is None


@pytest.mark.asyncio
async def test_real_monty_memory_limit(tmp_path):
    result = await CodeCompositionService(tmp_path).execute(
        CompositionRequest(code="'x' * 10**12"), context=None, tools=[]
    )
    assert result.status == "failed"


@pytest.mark.asyncio
async def test_command_output_limit_and_timeout_reap(tmp_path):
    runner = CommandRunner()
    with pytest.raises(SandboxFailure, match="output"):
        await runner.run(
            [sys.executable, "-c", "print('x'*1000000)"],
            cwd=tmp_path,
            env={},
            timeout=5,
            max_output=100,
        )
    with pytest.raises(SandboxFailure, match="time"):
        await runner.run(
            [sys.executable, "-c", "import time; time.sleep(10)"],
            cwd=tmp_path,
            env={},
            timeout=0.05,
        )
    assert not runner.active


@pytest.mark.asyncio
async def test_command_cancel_reaps(tmp_path):
    runner = CommandRunner()
    task = asyncio.create_task(
        runner.run(
            [sys.executable, "-c", "import time; time.sleep(10)"], cwd=tmp_path, env={}, timeout=30
        )
    )
    while not runner.active:
        await asyncio.sleep(0.01)
    task.cancel()
    with pytest.raises(asyncio.CancelledError):
        await task
    assert not runner.active


def test_limits_and_flat_names():
    assert PythonLimits().memory_mib == 2048
    for name in ["../x", "C:\\x", "/tmp/x", "x/y", "CON", "a:b", "a\x00b"]:
        with pytest.raises(ValueError):
            safe_name(name)
    assert safe_name("quantities.xlsx") == "quantities.xlsx"
    with pytest.raises(ValueError):
        PythonLimits(memory_mib=65536)


def test_container_policy_has_no_host_mounts_or_network(tmp_path):
    from quantix.podman_runtime import PodmanRuntime

    runtime = PodmanRuntime(tmp_path)
    args = runtime.container_args(
        "quantix-run-abc", "sha256:" + "a" * 64, input_volume="quantix-input-" + "b" * 32
    )
    for required in [
        "--network=none",
        "--read-only",
        "--cap-drop=ALL",
        "--security-opt=no-new-privileges",
        "--user=1000:1000",
    ]:
        assert required in args
    mounts = [args[i + 1] for i, arg in enumerate(args) if arg == "--mount"]
    assert mounts == ["type=volume,source=quantix-input-" + "b" * 32 + ",target=/inputs,ro=true"]
    assert "--privileged" not in args
    assert not any("host" in arg for arg in args)


def test_runtime_environment_cannot_inherit_secrets_or_connections(tmp_path, monkeypatch):
    from quantix.podman_runtime import PodmanRuntime

    monkeypatch.setenv("OPENAI_API_KEY", "private-secret")
    monkeypatch.setenv("CONTAINER_HOST", "ssh://other-machine")
    monkeypatch.setenv("SSH_AUTH_SOCK", "/private/socket")
    runtime = PodmanRuntime(tmp_path)
    env = runtime.environment()
    for name in ("OPENAI_API_KEY", "CONTAINER_HOST", "SSH_AUTH_SOCK"):
        assert name not in env
    for name in ("HOME", "USERPROFILE", "XDG_DATA_HOME", "XDG_CACHE_HOME", "TMP"):
        assert str(tmp_path) in env[name]


class FakeRuntime:
    def __init__(self, home, *, output=None, fail=False):
        from quantix.podman_runtime import PodmanRuntime

        self.real = PodmanRuntime(home)
        self.lock = asyncio.Lock()
        self.active_runs = 0
        self.calls = []
        self.output = output or {"returncode": 0, "stdout": "42", "stderr": "", "outputs": []}
        self.fail = fail
        self.manifest_path = home / "manifest.json"

    async def status(self):
        return SimpleNamespace(python_available=True)

    def operation(self):
        return self.real.operation()

    def manifest(self):
        return {
            "image_id": "sha256:" + "a" * 64,
            "libraries": {"numpy": "2.2.6"},
            "qualified": True,
        }

    def container_args(self, *args, **kwargs):
        return self.real.container_args(*args, **kwargs)

    async def command(self, *args, **kwargs):
        self.calls.append((args, kwargs))
        if args[0] == "run" and "--read-only-tmpfs=false" in args:
            if self.fail:
                raise SandboxFailure("The code exceeded its time limit.")
            return CommandResult(0, json.dumps(self.output).encode(), b"")
        return CommandResult(0, b"", b"")


@pytest.mark.asyncio
async def test_python_snapshots_and_cleanup_after_failure(tmp_path):
    from quantix.python_analysis import PythonAnalysisService, SelectedPythonInput
    from quantix.python_analysis_models import PythonAnalysisRequest

    runtime = FakeRuntime(tmp_path, fail=True)
    source = SelectedPythonInput(
        "source-1", "quantities.csv", b"1,2", hashlib.sha256(b"1,2").hexdigest(), "v1"
    )
    result = await PythonAnalysisService(tmp_path, runtime=runtime).execute(
        PythonAnalysisRequest(code="print(42)", input_ids=[source.id]),
        inputs=[source],
        context=SimpleNamespace(tender_id="tender-1", run_id="run-1"),
        authority_check=lambda: None,
    )
    assert result.status == "failed"
    assert runtime.active_runs == 0
    assert len([args for args, _ in runtime.calls if args[:2] == ("rm", "--force")]) == 2
    assert runtime.calls[-1][0][:2] == ("volume", "rm")
    assert (
        tmp_path / "code" / "runs" / result.id / "inputs" / source.name
    ).read_bytes() == source.content
    record = json.loads((tmp_path / "code" / "runs" / result.id / "receipt.json").read_text())
    assert record["tender_id"] == "tender-1"
    assert record["inputs"][0]["source_version"] == "v1"


@pytest.mark.asyncio
async def test_python_rejects_bad_snapshots_before_command(tmp_path):
    from quantix.python_analysis import PythonAnalysisService, SelectedPythonInput
    from quantix.python_analysis_models import PythonAnalysisRequest

    runtime = FakeRuntime(tmp_path)
    with pytest.raises(ValueError, match="changed"):
        await PythonAnalysisService(tmp_path, runtime=runtime).execute(
            PythonAnalysisRequest(code="1", input_ids=["s"]),
            inputs=[SelectedPythonInput("s", "a.csv", b"data", "wrong", "v1")],
            context=None,
            authority_check=lambda: None,
        )
    assert runtime.calls == []


@pytest.mark.asyncio
async def test_python_output_traversal_fails_and_reaps(tmp_path):
    from quantix.python_analysis import PythonAnalysisService
    from quantix.python_analysis_models import PythonAnalysisRequest

    runtime = FakeRuntime(
        tmp_path,
        output={
            "returncode": 0,
            "stdout": "",
            "stderr": "",
            "outputs": [{"name": "../escape", "data": "YQ=="}],
        },
    )
    result = await PythonAnalysisService(tmp_path, runtime=runtime).execute(
        PythonAnalysisRequest(code="1"), inputs=[], context=None, authority_check=lambda: None
    )
    assert result.status == "failed"
    assert not (tmp_path / "escape").exists()
    assert runtime.active_runs == 0


@pytest.mark.asyncio
async def test_monty_current_authority_checked_each_call(tmp_path):
    @tool
    def count(ctx, value: int):
        return value

    checks = 0

    def deny():
        nonlocal checks
        checks += 1
        if checks > 1:
            raise ValueError("The reviewed work was stopped.")

    result = await CodeCompositionService(tmp_path).execute(
        CompositionRequest(code="await count(value=1)"),
        context=None,
        tools=[count],
        authority_check=deny,
    )
    assert result.status == "failed"
    assert result.tool_calls == 0
    assert checks == 2


@pytest.mark.asyncio
async def test_shared_runtime_close_cancels_and_drains_owner(tmp_path):
    from quantix.podman_runtime import get_code_runtime

    repo = SimpleNamespace(home=tmp_path)
    runtime = get_code_runtime(repo)
    assert get_code_runtime(repo) is runtime
    entered, cleaned = asyncio.Event(), asyncio.Event()

    async def work():
        async with runtime.operation():
            entered.set()
            try:
                await asyncio.sleep(30)
            finally:
                cleaned.set()

    task = asyncio.create_task(work())
    await entered.wait()
    await runtime.close()
    assert task.cancelled()
    assert cleaned.is_set()
    assert not runtime.operations
    with pytest.raises(SandboxFailure, match="shutting down"):
        async with runtime.operation():
            pass


@pytest.mark.asyncio
async def test_command_runner_close_reaps_active_command(tmp_path):
    runner = CommandRunner()
    task = asyncio.create_task(
        runner.run(
            [sys.executable, "-c", "import time;time.sleep(30)"], cwd=tmp_path, env={}, timeout=35
        )
    )
    while not runner.active:
        await asyncio.sleep(0.01)
    await runner.close()
    assert task.cancelled()
    assert not runner.active
    assert not runner.tasks
