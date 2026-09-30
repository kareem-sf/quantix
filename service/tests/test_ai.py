import asyncio
import json
import re
from types import SimpleNamespace

import anthropic
import grpc
import openai
import pydantic_ai.providers.anthropic
import pydantic_ai.providers.google
import pydantic_ai.providers.openai
import pydantic_ai.providers.xai
import pytest
import xai_sdk.aio.client
from google import genai
from pydantic_ai import BinaryContent
from pydantic_ai.exceptions import ModelAPIError
from pydantic_ai.messages import ModelResponse, TextPart, ToolCallPart, UserPromptPart
from pydantic_ai.models.anthropic import AnthropicModel
from pydantic_ai.models.function import FunctionModel
from pydantic_ai.models.google import GoogleModel
from pydantic_ai.models.openai import OpenAIChatModel, OpenAIResponsesModel
from pydantic_ai.models.xai import XaiModel

from quantix.ai import check, providers


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

    requests = []
    use_model(monkeypatch, lambda messages, info: requests.append(1) or follows_instructions(messages, info))
    checked = client.post(f"/ai/connections/{connection['id']}/checks", json={"model": "model-a"}).json()
    assert checked["checks"]["model-a"]["ok"] is True
    assert len(requests) == 2  # the tool call is the proof, then one request with an image; no closing replies


def sees(messages, info):
    """Follows instructions, and reads the check's number from the image (the test fixes it at 123)."""
    if any(isinstance(c, BinaryContent) for m in messages for p in m.parts for c in getattr(p, "content", []) or []):
        return ModelResponse(parts=[TextPart("123")])
    return follows_instructions(messages, info)


def test_the_check_finds_out_whether_a_model_reads_images(client, models, monkeypatch):
    connection = add(client).json()
    monkeypatch.setattr(check.secrets, "randbelow", lambda n: 23)

    use_model(monkeypatch, follows_instructions)  # answers "Done." to the image: can't see it
    blind = client.post(f"/ai/connections/{connection['id']}/checks", json={"model": "model-a"}).json()
    assert blind["checks"]["model-a"]["sees_images"] is False
    assert blind["checks"]["model-a"]["message"].endswith("won't look at drawings or scans with it.")

    use_model(monkeypatch, sees)
    seeing = client.post(f"/ai/connections/{connection['id']}/checks", json={"model": "model-a"}).json()
    assert seeing["checks"]["model-a"] | {"checked_at": ""} == {
        "ok": True,
        "message": "Works, including the tools the office needs.",
        "sees_images": True,
        "checked_at": "",
    }


def test_the_office_only_uses_a_checked_model(client, models, monkeypatch, tmp_path):
    assert client.get("/settings").json() == {
        "office_mode": "engineer",
        "office_ai": None,
        "tender_allowance": None,
        "notifications": "all",
    }
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
        "tender_allowance": None,
        "notifications": "all",
    }
    assert json.loads((tmp_path / "settings.json").read_text(encoding="utf-8"))["office_mode"] == "autonomous"
    assert client.patch("/settings", json={"notifications": "decisions"}).json()["notifications"] == "decisions"
    assert client.patch("/settings", json={"notifications": "sometimes"}).status_code == 422


def test_removing_a_connection_clears_the_office_ai(client, models, monkeypatch):
    connection = add(client).json()
    use_model(monkeypatch, follows_instructions)
    client.post(f"/ai/connections/{connection['id']}/checks", json={"model": "model-a"})
    client.patch("/settings", json={"office_ai": {"connection_id": connection["id"], "model": "model-a"}})

    assert client.delete(f"/ai/connections/{connection['id']}").status_code == 204
    assert client.get("/ai/connections").json() == []
    assert client.get("/settings").json()["office_ai"] is None
    assert client.delete(f"/ai/connections/{connection['id']}").status_code == 404


def test_removing_another_connection_keeps_the_office_ai(client, models, monkeypatch):
    kept, other = add(client).json(), add(client, provider="openai").json()
    use_model(monkeypatch, follows_instructions)
    client.post(f"/ai/connections/{kept['id']}/checks", json={"model": "model-a"})
    office_ai = {"connection_id": kept["id"], "model": "model-a"}
    client.patch("/settings", json={"office_ai": office_ai})

    assert client.delete(f"/ai/connections/{other['id']}").status_code == 204
    assert client.get("/settings").json()["office_ai"] == office_ai
    assert [c["id"] for c in client.get("/ai/connections").json()] == [kept["id"]]


def grpc_error(code: grpc.StatusCode) -> Exception:
    """xAI's client fails with gRPC errors, whose status is a method."""
    return type("AioRpcError", (Exception,), {"code": lambda self: code})()


@pytest.mark.parametrize(
    ("error", "message"),
    [
        (Refused(), "The key was refused. Check it and try again."),
        (NotFound(), "This model isn't available on this account."),
        (type("Limited", (Exception,), {"status_code": 429})(), "The service is limiting requests"),
        (type("APIConnectionError", (Exception,), {})(), "Couldn't reach the service."),
        (type("APITimeoutError", (Exception,), {})(), "The AI service took too long to answer."),
        (type("ClientError", (Exception,), {"code": 402})(), "The service is limiting requests"),
        (type("ClientError", (Exception,), {"status": "PERMISSION_DENIED"})(), "The key was refused."),
        (ValueError("Missing API key for this project"), "The key was refused."),
        (grpc_error(grpc.StatusCode.UNAUTHENTICATED), "The key was refused."),
        (grpc_error(grpc.StatusCode.NOT_FOUND), "This model isn't available on this account."),
        (grpc_error(grpc.StatusCode.RESOURCE_EXHAUSTED), "The service is limiting requests"),
        (grpc_error(grpc.StatusCode.UNAVAILABLE), "Couldn't reach the service."),
        (grpc_error(grpc.StatusCode.DEADLINE_EXCEEDED), "Couldn't reach the service."),
        (type("Upstream", (Exception,), {"status_code": 500})(), "The service answered with an error (500)."),
        (type("Odd", (Exception,), {"code": lambda self, detail: 1})(), "The service answered with an error (Odd)."),
    ],
)
def test_failures_are_explained_plainly(error, message):
    assert providers.explain(error).startswith(message)


def test_an_explanation_never_repeats_the_key_or_the_services_answer():
    for error in (
        RuntimeError('{"error": "invalid x-api-key sk-test-1234abcd"}'),
        type("Upstream", (Exception,), {"status_code": 503})('{"error": "overloaded", "key": "sk-test-1234abcd"}'),
        Refused("Incorrect API key provided: sk-test-1234abcd"),
    ):
        explained = providers.explain(error)
        assert "sk-test" not in explained and "{" not in explained


@pytest.mark.parametrize(
    ("provider", "base_url", "module", "name", "built"),
    [
        ("anthropic", None, pydantic_ai.providers.anthropic, "AnthropicProvider", AnthropicModel),
        ("openai", None, pydantic_ai.providers.openai, "OpenAIProvider", OpenAIResponsesModel),
        ("google", None, pydantic_ai.providers.google, "GoogleProvider", GoogleModel),
        ("xai", None, pydantic_ai.providers.xai, "XaiProvider", XaiModel),
        (
            "openai_compatible",
            "https://api.example.com/v1",
            pydantic_ai.providers.openai,
            "OpenAIProvider",
            OpenAIChatModel,
        ),
    ],
)
def test_each_provider_builds_its_own_model_from_the_connections_key(
    monkeypatch, provider, base_url, module, name, built
):
    real, given = getattr(module, name), []
    monkeypatch.setattr(module, name, lambda **options: given.append(options) or real(**options))
    model = providers.build_model(provider, "model-a", "sk-test-1234abcd", base_url)
    assert type(model) is built and model.model_name == "model-a"
    assert given == [{"api_key": "sk-test-1234abcd"} | ({"base_url": base_url} if base_url else {})]


def listing(*items):
    async def each():
        for item in items:
            yield item

    return each()


def test_each_provider_lists_the_accounts_chat_models_with_its_own_key(monkeypatch):
    made = []

    def client(models):
        def make(**options):
            made.append(options)
            return SimpleNamespace(models=models, aio=SimpleNamespace(models=models))

        return make

    async def google_models():
        return listing(
            SimpleNamespace(name="models/gemini-pro", supported_actions=["generateContent", "countTokens"]),
            SimpleNamespace(name="models/text-embedding", supported_actions=["embedContent"]),
            SimpleNamespace(name="models/aqa", supported_actions=None),
        )

    async def grok_models():
        return [SimpleNamespace(name="grok-4"), SimpleNamespace(name="grok-3-mini")]

    claude = [SimpleNamespace(id=i) for i in ("claude-sonnet-5", "claude-haiku-5")]
    ids = [SimpleNamespace(id=i) for i in ("gpt-4o", "o3", "gpt-5")]
    monkeypatch.setattr(anthropic, "AsyncAnthropic", client(SimpleNamespace(list=lambda limit: listing(*claude))))
    monkeypatch.setattr(openai, "AsyncOpenAI", client(SimpleNamespace(list=lambda: listing(*ids))))
    monkeypatch.setattr(genai, "Client", client(SimpleNamespace(list=google_models)))
    monkeypatch.setattr(xai_sdk.aio.client, "Client", client(SimpleNamespace(list_language_models=grok_models)))

    def listed(provider, base_url=None):
        return asyncio.run(providers.list_models(provider, "sk-test-1234abcd", base_url))

    assert listed("anthropic") == ["claude-sonnet-5", "claude-haiku-5"]  # as the service orders them
    assert listed("google") == ["gemini-pro"]  # only models that chat
    assert listed("xai") == ["grok-4", "grok-3-mini"]
    assert listed("openai") == ["o3", "gpt-5", "gpt-4o"]  # newest names first
    assert listed("openai_compatible", "https://api.example.com/v1") == ["o3", "gpt-5", "gpt-4o"]
    key = {"api_key": "sk-test-1234abcd"}
    assert made == [key, key, key, key | {"base_url": None}, key | {"base_url": "https://api.example.com/v1"}]


@pytest.mark.parametrize("provider", ["codex", "chatgpt", "grok"])
def test_a_subscription_is_never_taken_as_an_api_key(client, models, tmp_path, provider):
    """ChatGPT/Codex and Grok subscriptions work only through their official clients: their tokens are no API key."""
    assert add(client, provider=provider, api_key="subscription-token").status_code == 422
    assert not (tmp_path / "auth.json").exists()


def test_a_connection_whose_key_stopped_working_says_so(client, models):
    connection = add(client).json()
    models.error = Refused("invalid x-api-key")
    refused = client.get(f"/ai/connections/{connection['id']}/models")
    assert (refused.status_code, refused.json()["detail"]) == (400, "The key was refused. Check it and try again.")


def test_a_model_that_fails_the_check_says_why_in_plain_words(client, models, monkeypatch):
    connection = add(client).json()

    def refuses(messages, info):
        raise Refused("invalid x-api-key sk-test-1234abcd")

    use_model(monkeypatch, refuses)
    checked = client.post(f"/ai/connections/{connection['id']}/checks", json={"model": "model-a"}).json()
    assert checked["checks"]["model-a"] | {"checked_at": ""} == {
        "ok": False,
        "message": "The key was refused. Check it and try again.",
        "sees_images": False,
        "checked_at": "",
    }


def test_a_wrapped_provider_error_is_explained_from_its_cause():
    try:
        try:
            raise type("APITimeoutError", (Exception,), {})()
        except Exception as timeout:
            raise ModelAPIError("zai-glm", "Request timed out.") from timeout
    except ModelAPIError as wrapped:
        assert providers.explain(wrapped).startswith("The AI service took too long to answer.")
