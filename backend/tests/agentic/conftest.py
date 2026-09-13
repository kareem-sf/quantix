"""Isolated FastAPI/repository environment for agentic acceptance drivers."""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path

import pytest
from fastapi.testclient import TestClient

from .cases import CaseRegistry, built_registry
from .fixtures import FakeClock, RecordingProvider


@dataclass
class CaseEnvironment:
    client: TestClient
    repo: object
    provider: RecordingProvider
    clock: FakeClock
    home: Path
    tmp_path: Path


@pytest.fixture
def case_registry() -> CaseRegistry:
    return built_registry()


@pytest.fixture
def case_environment(tmp_path, monkeypatch):
    monkeypatch.delenv("OPENAI_API_KEY", raising=False)
    monkeypatch.setattr("keyring.get_password", lambda *_: None)
    provider = RecordingProvider()

    async def recorded(*args, **kwargs):
        return await provider.execute(*args, **kwargs)

    monkeypatch.setattr("quantix.ai_execution.execute_api", recorded)
    monkeypatch.setattr(
        "quantix.ai_direct.direct_runtime_status",
        lambda _: {
            "state": "ready",
            "version": "synthetic",
            "component_id": "direct-api",
            "detail": "Ready",
            "progress": 100,
        },
    )
    from quantix.api import create_app

    home = tmp_path / "home"
    app = create_app(home, "agentic-test-session")
    with TestClient(app) as client:
        client.headers["Authorization"] = "Bearer agentic-test-session"
        yield CaseEnvironment(
            client=client,
            repo=app.state.repo,
            provider=provider,
            clock=FakeClock(),
            home=home,
            tmp_path=tmp_path,
        )


@pytest.fixture
def run_case(case_environment, case_registry):
    return lambda case_id, inputs: case_registry.run(case_environment, case_id, inputs)
