"""What keeps Tender AI work from spending more tokens than it needs."""

import asyncio
import json
import sys
from pathlib import Path
from types import SimpleNamespace

import mcp.types as mcp_types
import pytest
from fastapi import FastAPI
from fastapi.testclient import TestClient
from pydantic import BaseModel
from pydantic_ai.messages import ModelRequest, ModelResponse, TextPart, ToolReturnPart
from test_catalog_authority import configured_office
from test_tender_ai_setup import _account, _choose, _Setup

from quantix.ai_api_engine import HISTORY_TOOL_CHARS, TRIMMED_NOTE, trim_tool_history
from quantix.ai_policy import AIPolicyService
from quantix.ai_runtime_mcp import SUBMIT_TOOL, RuntimeToolBridge
from quantix.ai_setup_routes import create_router
from quantix.ai_thinking import light_level, recommended_level, thinking_levels
from quantix.office_tools import SEARCH_EXCERPT_CHARS, OfficeContext, source_tools
from quantix.repository import Repository
from quantix.tool_policy import dispatch

DIRECT = {"auth_type": "api_key", "billing": "metered"}


def _direct(provider, protocol):
    return {**DIRECT, "provider_id": provider, "protocol": protocol}


def test_every_provider_offers_its_documented_thinking_levels():
    bare = {"capabilities": {}}
    assert thinking_levels(_direct("google", "google"), bare) == ["low", "medium", "high"]
    assert thinking_levels(_direct("xai", "openai_responses"), bare) == ["low", "medium", "high"]
    assert thinking_levels(_direct("openai", "openai_responses"), bare) == ["low", "medium", "high"]
    assert thinking_levels(_direct("anthropic", "anthropic"), bare) == ["disabled", "low", "medium", "high"]
    # An OpenAI-compatible endpoint documents nothing, so nothing is assumed.
    assert thinking_levels(_direct("custom", "openai_chat"), bare) == []

    codex = {"provider_id": "codex", "protocol": "codex", "auth_type": "client_login", "billing": "subscription"}
    reported = {"capabilities": {"reasoning": ["xhigh", "low", "high", "medium"]}}
    assert thinking_levels(codex, reported) == ["low", "medium", "high", "xhigh"]
    assert thinking_levels(codex, bare) == []
    # A model's own reported list always wins over the vendor default.
    assert thinking_levels(_direct("openai", "openai_chat"), {"capabilities": {"reasoning": ["xhigh"]}}) == ["xhigh"]


def test_new_routes_default_to_medium_and_routing_uses_the_lightest_level():
    codex = {"provider_id": "codex", "protocol": "codex", "auth_type": "client_login", "billing": "subscription"}
    reported = {"capabilities": {"reasoning": ["low", "medium", "high", "xhigh"]}}
    assert recommended_level(codex, reported) == "medium"
    assert light_level(codex, reported) == "low"
    assert light_level(_direct("anthropic", "anthropic"), {"capabilities": {}}) == "disabled"
    assert recommended_level(_direct("custom", "openai_chat"), {"capabilities": {}}) is None


def _thinking_client(tmp_path):
    repo = Repository(tmp_path)
    tender = repo.create_tender("Synthetic tender")
    account = _account(repo, "Synthetic API")
    app = FastAPI()
    app.include_router(create_router(repo, _Setup(account)))
    client = TestClient(app)
    assert _choose(client, tender["id"], account, budget=20).status_code == 200
    return repo, tender["id"], client


def test_the_prompt_box_sets_the_managers_thinking(tmp_path):
    repo, tender_id, client = _thinking_client(tmp_path)

    shown = client.get(f"/api/tenders/{tender_id}/ai-thinking").json()
    assert shown["current"]["value"] == "medium"
    assert [option["value"] for option in shown["options"]] == ["low", "medium", "high", None]
    assert shown["busy"] is False

    changed = client.post(
        f"/api/tenders/{tender_id}/ai-thinking",
        json={"reasoning": "low", "engineer_confirmed": True},
    ).json()
    assert changed["current"]["label"] == "Low"
    policy = AIPolicyService(repo).get(tender_id)
    assert policy["manager"]["reasoning"] == "low"
    # The specialist simply followed the Manager, so it follows the change.
    assert policy["specialist"]["reasoning"] == "low"
    assert policy["tender_budget_usd"] == 20

    with pytest.raises(ValueError, match="thinking level this AI offers"):
        client.post(
            f"/api/tenders/{tender_id}/ai-thinking",
            json={"reasoning": "xhigh", "engineer_confirmed": True},
        )


def test_thinking_cannot_change_while_work_runs(tmp_path):
    repo, tender_id, client = _thinking_client(tmp_path)
    run = repo.create_run(tender_id, "manager", "Review")
    repo.update_run(run["id"], status="running")

    assert client.get(f"/api/tenders/{tender_id}/ai-thinking").json()["busy"] is True
    with pytest.raises(ValueError, match="Stop the current tender work"):
        client.post(
            f"/api/tenders/{tender_id}/ai-thinking",
            json={"reasoning": "high", "engineer_confirmed": True},
        )


@pytest.mark.asyncio
async def test_routing_a_message_uses_the_lightest_thinking(tmp_path, monkeypatch):
    from quantix.conversation import run_conversation

    repo, tender, *_ = configured_office(tmp_path, monkeypatch, reasoning="high")
    run = repo.create_run(tender["id"], "conversation", "Hello")
    monkeypatch.setattr("quantix.ai_thinking.thinking_levels", lambda *_: ["low", "high"])
    seen = {}

    async def provider(route, *args, **kwargs):
        seen["reasoning"] = route["reasoning"]
        raise asyncio.CancelledError

    monkeypatch.setattr("quantix.ai_execution.execute_api", provider)
    with pytest.raises(asyncio.CancelledError):
        await run_conversation(repo, tender["id"], run["id"], "Hello")
    assert seen["reasoning"] == "low"
    assert AIPolicyService(repo).get(tender["id"])["manager"]["reasoning"] == "high"


@pytest.mark.parametrize("reasoning", ["high", "none", "disabled"])
def test_classifier_uses_shared_documented_levels_without_raising_approved_cap(reasoning):
    from quantix.conversation import classification_route

    approved = {"reasoning": reasoning, "max_output_tokens": 1024, "web_search": True}
    result = classification_route(approved, _direct("openai", "openai_responses"),
                                  {"capabilities": {}})
    assert result["reasoning"] == ("low" if reasoning == "high" else reasoning)
    assert result["max_output_tokens"] == 1024
    assert result["web_search"] is False
    assert approved["reasoning"] == reasoning and approved["web_search"] is True


class _Proposal(BaseModel):
    summary: str


def _submit(summary):
    return mcp_types.CallToolRequestParams(name=SUBMIT_TOOL, arguments={"summary": summary})


@pytest.mark.asyncio
async def test_a_rejected_submission_goes_back_to_the_client_to_correct():
    checked = []

    def check(candidate, web_sources):
        checked.append(candidate.summary)
        if candidate.summary != "Corrected":
            raise ValueError("The response cites evidence that was not read in this Tender run.")

    bridge = RuntimeToolBridge(None, _Proposal, definitions=[], validate_output=check)
    rejected = await bridge._call_tool(SimpleNamespace(request_id=None), _submit("Wrong citation"))
    assert rejected.is_error is True
    assert "not read in this Tender run" in rejected.content[0].text
    assert bridge.output is None and bridge.failure is None

    accepted = await bridge._call_tool(SimpleNamespace(request_id=None), _submit("Corrected"))
    assert accepted.is_error is False
    assert bridge.result().summary == "Corrected"
    assert checked == ["Wrong citation", "Corrected"]


@pytest.mark.asyncio
async def test_repeated_rejections_stop_with_the_real_reason():
    def check(candidate, web_sources):
        raise ValueError("Factual findings require Tender source evidence.")

    bridge = RuntimeToolBridge(None, _Proposal, definitions=[], validate_output=check)
    for _ in range(3):
        await bridge._call_tool(SimpleNamespace(request_id=None), _submit("Still wrong"))
    with pytest.raises(ValueError, match="could not correct its proposal. Factual findings"):
        bridge.result()


def _returns(*sizes):
    messages = []
    for index, size in enumerate(sizes):
        evidence = f"{index:032x}"
        content = json.dumps([{"id": evidence, "text": "x" * size}])
        messages.append(ModelResponse(parts=[TextPart("step")]))
        messages.append(ModelRequest(parts=[ToolReturnPart("read_source", content, tool_call_id=f"call-{index}")]))
    return messages


def test_short_histories_are_sent_unchanged():
    messages = _returns(1000, 1000, 1000)
    assert trim_tool_history(messages) is messages


def test_older_tool_output_becomes_a_note_that_keeps_its_evidence_ids():
    context = SimpleNamespace(returned_reads={("a", 0, 10)})
    big = HISTORY_TOOL_CHARS // 2
    messages = _returns(big, big, big, 500)

    trimmed = trim_tool_history(messages, context)

    contents = [part.content for message in trimmed if isinstance(message, ModelRequest) for part in message.parts]
    assert contents[0].startswith(TRIMMED_NOTE) and f"{0:032x}" in contents[0]
    assert contents[1].startswith(TRIMMED_NOTE) and f"{1:032x}" in contents[1]
    # The latest steps stay verbatim, and the call/return pairing is intact.
    assert contents[2] == messages[5].parts[0].content
    assert contents[3] == messages[7].parts[0].content
    assert [message.parts[0].tool_call_id for message in trimmed if isinstance(message, ModelRequest)] == [
        "call-0", "call-1", "call-2", "call-3",
    ]
    # Passages the model can no longer see may be sent again.
    assert context.returned_reads == set()
    # A trimmed history stays as it is on the next step.
    assert trim_tool_history(trimmed, context) is trimmed


def _reading_workspace(tmp_path, text):
    repo = Repository(tmp_path)
    tender = repo.create_tender("Reading Tender")
    run = repo.create_run(tender["id"], "manager", "Find the bid bond")
    repo.update_run(run["id"], status="running")
    artifact, _ = repo.register_artifact(
        tender["id"],
        "Conditions/general.pdf",
        "b" * 64,
        len(text),
        {"kind": "pdf", "status": "extracted", "segments": [{"locator": "Page 4", "text": text, "page": 4}]},
    )
    evidence = repo.artifact_evidence(tender["id"], artifact["id"])[0]
    return repo, OfficeContext(repo, tender["id"], run["id"]), artifact, evidence


def _call(context, name, payload, invocation):
    definition = next(item for item in source_tools() if item.name == name)
    return json.loads(asyncio.run(dispatch("nested", definition, context, payload, invocation_id=invocation)))


def test_search_returns_an_excerpt_around_the_match_not_the_whole_passage(tmp_path):
    text = ("General conditions apply. " * 400) + "The bid bond is two percent of the tender value. " + ("Other clauses. " * 400)
    _repo, context, _artifact, evidence = _reading_workspace(tmp_path, text)

    hits = _call(context, "search_sources", {"query": "bid bond"}, "search-1")

    assert len(hits) == 1
    assert hits[0]["id"] == evidence["id"]
    assert "bid bond is two percent" in hits[0]["text"]
    assert len(hits[0]["text"]) <= SEARCH_EXCERPT_CHARS
    assert hits[0]["text_length"] == len(text)
    assert hits[0]["text_is_partial"] is True
    # An excerpt is still an actual read that can be cited.
    assert context.has_seen_source(evidence["id"])


def test_text_already_returned_in_the_run_is_not_sent_again(tmp_path):
    _repo, context, artifact, evidence = _reading_workspace(tmp_path, "Concrete strength 30 MPa. " * 500)

    first = _call(context, "read_source", {"source_id": evidence["id"]}, "read-1")
    assert len(first["text"]) == 4000

    repeat = _call(context, "read_source", {"source_id": evidence["id"]}, "read-2")
    assert repeat == {"id": evidence["id"], "text_offset": 0, "already_returned": True}

    again = _call(context, "read_source", {"source_id": evidence["id"], "reread": True}, "read-3")
    assert again["text"] == first["text"]

    pages = _call(context, "read_document", {"artifact_id": artifact["id"]}, "document-1")
    assert len(pages[0]["text"]) == 3000
    assert pages[0]["next_offset"] == 3000


def test_codex_usage_keeps_its_own_token_counts():
    sys.path.insert(0, str(Path(__file__).parents[1] / "ai_worker"))
    from quantix_ai_worker.codex import record_token_usage

    def report(input_tokens, cached, output, reasoning):
        total = SimpleNamespace(
            input_tokens=input_tokens,
            cached_input_tokens=cached,
            output_tokens=output,
            reasoning_output_tokens=reasoning,
            total_tokens=input_tokens + output,
        )
        return SimpleNamespace(total=total, last=total)

    usage = {"requests": 0, "input_tokens": 0, "output_tokens": 0, "usage_complete": False}
    record_token_usage(usage, report(5000, 0, 200, 150))
    record_token_usage(usage, report(11000, 4800, 450, 300))

    assert usage["requests"] == 2
    assert usage["input_tokens"] == 11000
    assert usage["cached_input_tokens"] == 4800
    assert usage["output_tokens"] == 450
    assert usage["reasoning_tokens"] == 300
    assert usage["usage_complete"] is True


def test_a_passage_is_resent_at_most_once_and_oversized_searches_are_capped(tmp_path):
    _repo, context, _artifact, evidence = _reading_workspace(tmp_path, "The bid bond is two percent. " * 50)
    first = _call(context, "read_source", {"source_id": evidence["id"]}, "r1")
    again = _call(context, "read_source", {"source_id": evidence["id"], "reread": True}, "r2")
    assert again["text"] == first["text"]
    third = _call(context, "read_source", {"source_id": evidence["id"], "reread": True, "limit": 3000}, "r3")
    assert third["already_returned"] is True and "already sent again" in third["note"]
    hits = _call(context, "search_sources", {"query": "bid bond", "limit": 80, "exact": True}, "s1")
    assert len(hits) <= 20
