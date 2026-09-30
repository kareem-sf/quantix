"""The service's entry points: `python -m quantix` serves one data home on loopback with the launch token, and
`python -m quantix.openapi` writes the schema the interface's API types are generated from."""

import json
import logging
import runpy
import sys
from pathlib import Path

import pytest
import uvicorn
from conftest import TOKEN
from fastapi.testclient import TestClient

from quantix import __main__ as service
from quantix.api.app import create_app


@pytest.fixture
def served(monkeypatch):
    """What `python -m quantix` hands to the server, which isn't started."""
    runs = []
    monkeypatch.setattr(uvicorn, "run", lambda app, **options: runs.append((app, options)))
    monkeypatch.setattr(logging, "basicConfig", lambda **options: None)
    level = logging.getLogger("quantix").level
    yield runs
    logging.getLogger("quantix").setLevel(level)


def test_the_service_serves_its_data_home_on_loopback_with_the_launch_token(served, monkeypatch, tmp_path):
    monkeypatch.setattr(sys, "argv", ["quantix", "--port", "8765"])
    monkeypatch.setenv("QUANTIX_TOKEN", TOKEN)
    monkeypatch.setenv("QUANTIX_HOME", str(tmp_path))
    service.main()

    [(app, options)] = served
    assert options == {"host": "127.0.0.1", "port": 8765, "log_level": "warning"}
    assert app.state.home == tmp_path
    assert logging.getLogger("quantix").level == logging.INFO  # what the office does, for diagnosis
    with TestClient(app) as c:
        assert c.get("/tenders").status_code == 401
        assert c.get("/tenders", headers={"Authorization": f"Bearer {TOKEN}"}).json() == []
    assert (tmp_path / "quantix.sqlite").exists()


def test_the_data_home_is_dot_quantix_unless_another_is_given(served, monkeypatch, tmp_path):
    monkeypatch.setattr(sys, "argv", ["quantix", "--port", "8765"])
    monkeypatch.setenv("QUANTIX_TOKEN", TOKEN)
    monkeypatch.delenv("QUANTIX_HOME", raising=False)
    monkeypatch.setattr(Path, "home", lambda: tmp_path)
    service.main()
    assert served[0][0].state.home == tmp_path / ".quantix"
    assert not (tmp_path / ".quantix").exists()  # nothing is opened until the server starts


def test_the_service_needs_a_port_and_the_launch_token(served, monkeypatch):
    monkeypatch.setattr(sys, "argv", ["quantix"])
    monkeypatch.setenv("QUANTIX_TOKEN", TOKEN)
    with pytest.raises(SystemExit) as no_port:
        service.main()
    assert no_port.value.code == 2  # argparse's own usage error

    monkeypatch.setattr(sys, "argv", ["quantix", "--port", "8765"])
    monkeypatch.delenv("QUANTIX_TOKEN", raising=False)
    with pytest.raises(SystemExit, match="QUANTIX_TOKEN must be set."):
        service.main()
    assert served == []


def test_the_schema_file_is_the_services_own_schema(monkeypatch, tmp_path):
    target = tmp_path / "openapi.json"
    monkeypatch.setattr(sys, "argv", ["quantix.openapi", str(target)])
    monkeypatch.setattr(Path, "home", lambda: tmp_path)
    runpy.run_module("quantix.openapi", run_name="__main__")  # as `npm run bindings` runs it

    assert json.loads(target.read_text(encoding="utf-8")) == create_app(tmp_path, TOKEN).openapi()
    assert target.read_text(encoding="utf-8").endswith("}\n")
    assert not (tmp_path / ".quantix").exists()  # the schema is built without opening the data home
