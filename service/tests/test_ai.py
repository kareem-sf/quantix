import json
import re

import pytest
from pydantic_ai.messages import ModelResponse, TextPart, ToolCallPart, UserPromptPart
from pydantic_ai.models.function import FunctionModel

from quantix.ai import providers


class Refused(Exception):
    status_code = 401


class NotFound(Exception):
    status_code = 404


@pytest.fixture
def models(monkeypatch):
    """The provider's model list; set `.error` to make listing fail."""

    class Listing:
        names = ["model-b", "model-a"]
        error: Exception | None = None

    async def list_models(provider, key, base_url):
        if Listing.error:
            raise Listing.error
        return Listing.names

    monkeypatch.setattr(providers, "list_models", list_models)
    return Listing


def use_model(monkeypatch, respond):
    monkeypatch.setattr(providers, "build_model", lambda *args: FunctionModel(respond))


def follows_instructions(messages, info):
    if len(messages) == 1:
        prompt = next(p.content for p in messages[0].parts if isinstance(p, UserPromptPart))
        code = re.search(r"code is (\w+)", prompt).group(1)
        return ModelResponse(parts=[ToolCallPart("confirm", {"code": code})])
    return ModelResponse(parts=[TextPart("Done.")])


def ignores_tools(messages, info):
    return ModelResponse(parts=[TextPart("Hello!")])


def add(client, **body):
    body = {"provider": "anthropic", "api_key": "sk-test-1234abcd", **body}
    return client.post("/ai/connections", json=body)


def test_a_working_key_is_kept_in_auth_json_and_never_returned(client, models, tmp_path):
    response = add(client)
    assert response.status_code == 201
    connection = response.json()
    assert connection["label"] == "Anthropic"
    assert connection["key_hint"] == "…abcd"
    assert "api_key" not in connection

    saved = json.loads((tmp_path / "auth.json").read_text(encoding="utf-8"))
    assert saved["connections"][0]["api_key"] == "sk-test-1234abcd"
    assert client.get("/ai/connections").json() == [connection]
    assert client.get(f"/ai/connections/{connection['id']}/models").json() == ["model-b", "model-a"]


def test_a_refused_key_is_not_kept(client, models, tmp_path):
    models.error = Refused("invalid x-api-key")
    response = add(client)
    assert response.status_code == 400
    assert response.json()["detail"] == "The key was refused. Check it and try again."
    assert not (tmp_path / "auth.json").exists()


def test_an_openai_compatible_service_needs_an_address(client, models):
    assert add(client, provider="openai_compatible").status_code == 422
    models.error = NotFound("no /models here")
    connection = add(client, provider="openai_compatible", base_url="https://api.example.com/v1").json()
    assert connection["label"] == "api.example.com"
    assert client.get(f"/ai/connections/{connection['id']}/models").json() == []


def test_model_check_needs_real_tool_use(client, models, monkeypatch):
    connection = add(client).json()

    use_model(monkeypatch, ignores_tools)
    checked = client.post(f"/ai/connections/{connection['id']}/checks", json={"model": "model-a"}).json()
    assert checked["checks"]["model-a"]["ok"] is False
    assert "didn't use the tool" in checked["checks"]["model-a"]["message"]

    use_model(monkeypatch, follows_instructions)
    checked = client.post(f"/ai/connections/{connection['id']}/checks", json={"model": "model-a"}).json()
    assert checked["checks"]["model-a"]["ok"] is True


def test_the_office_only_uses_a_checked_model(client, models, monkeypatch, tmp_path):
    assert client.get("/settings").json() == {"office_mode": "engineer", "office_ai": None}
    connection = add(client).json()
    office_ai = {"connection_id": connection["id"], "model": "model-a"}

    refused = client.patch("/settings", json={"office_ai": office_ai})
    assert refused.status_code == 400
    assert refused.json()["detail"] == "Check this model before the office uses it."

    use_model(monkeypatch, follows_instructions)
    client.post(f"/ai/connections/{connection['id']}/checks", json={"model": "model-a"})
    assert client.patch("/settings", json={"office_ai": office_ai}).json()["office_ai"] == office_ai
    assert client.patch("/settings", json={"office_mode": "autonomous"}).json() == {
        "office_mode": "autonomous",
        "office_ai": office_ai,
    }
    assert json.loads((tmp_path / "settings.json").read_text(encoding="utf-8"))["office_mode"] == "autonomous"


def test_removing_a_connection_clears_the_office_ai(client, models, monkeypatch):
    connection = add(client).json()
    use_model(monkeypatch, follows_instructions)
    client.post(f"/ai/connections/{connection['id']}/checks", json={"model": "model-a"})
    client.patch("/settings", json={"office_ai": {"connection_id": connection["id"], "model": "model-a"}})

    assert client.delete(f"/ai/connections/{connection['id']}").status_code == 204
    assert client.get("/ai/connections").json() == []
    assert client.get("/settings").json()["office_ai"] is None
    assert client.delete(f"/ai/connections/{connection['id']}").status_code == 404


@pytest.mark.parametrize(
    ("error", "message"),
    [
        (Refused(), "The key was refused. Check it and try again."),
        (NotFound(), "This model isn't available on this account."),
        (type("Limited", (Exception,), {"status_code": 429})(), "The service is limiting requests"),
        (type("APIConnectionError", (Exception,), {})(), "Couldn't reach the service."),
    ],
)
def test_failures_are_explained_plainly(error, message):
    assert providers.explain(error).startswith(message)
