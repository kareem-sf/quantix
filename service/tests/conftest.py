import pytest
from fastapi.testclient import TestClient

from quantix.api.app import create_app
from quantix.documents import meaning, ocr

TOKEN = "test-token"


@pytest.fixture(autouse=True)
def shared_model(request, monkeypatch):
    """Every test's data home uses one copy of the meaning model, fetched on the first run."""
    folder = request.config.cache.mkdir("quantix-models")
    monkeypatch.setattr(meaning, "models_dir", lambda home: folder)


@pytest.fixture(autouse=True)
def ocr_only_where_tested(request, monkeypatch):
    """OCR reads scans only in the tests about it: elsewhere a blank page would keep it busy in the background."""
    if request.module.__name__ != "test_ocr":
        monkeypatch.setattr(ocr.Ocr, "wake", lambda self: None)


@pytest.fixture
def client(tmp_path):
    with TestClient(create_app(tmp_path, TOKEN), headers={"Authorization": f"Bearer {TOKEN}"}) as c:
        yield c
