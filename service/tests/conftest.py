import pytest
from fastapi.testclient import TestClient

from quantix.api.app import create_app

TOKEN = "test-token"


@pytest.fixture
def client(tmp_path):
    with TestClient(create_app(tmp_path, TOKEN), headers={"Authorization": f"Bearer {TOKEN}"}) as c:
        yield c
