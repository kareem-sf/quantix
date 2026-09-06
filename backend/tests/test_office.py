import asyncio
import importlib
from types import SimpleNamespace

import pytest


class MemoryRepository:
    def setting(self, key, default=None):
        return default

    def atomic(self):
        from contextlib import nullcontext

        return nullcontext()

    """Contract fake until the independently owned repository is integrated."""

    def __init__(self):
        self.replies = []
        self.findings = []
        self.plans = []
        self.events = []
        self.evidence = {
            "source-1": {
                "id": "source-1",
                "artifact_id": "artifact-1",
                "artifact_name": "Spec.pdf",
                "relative_path": "Civil/Spec.pdf",
                "locator": "Page 2",
                "page": 2,
                "text": "The slab concrete shall achieve 35 MPa.",
                "kind": "text",
            },
            "source-2": {
                "id": "source-2",
                "artifact_id": "artifact-1",
                "artifact_name": "Spec.pdf",
                "relative_path": "Civil/Spec.pdf",
                "locator": "Page 3",
                "page": 3,
                "text": "The engineer must approve substitutions.",
                "kind": "text",
            },
        }

    def get_tender(self, tender_id):
        if tender_id != "tender-1":
            raise KeyError(tender_id)
        return {"id": tender_id, "name": "School extension"}

    def get_run(self, run_id):
        return {"id": run_id, "tender_id": "tender-1", "status": "running"}

    def overview(self, tender_id):
        self.get_tender(tender_id)
        return {
            "tender": self.get_tender(tender_id),
            "artifacts": 1,
            "evidence": 2,
            "coverage": {
                "registered": 1,
                "extracted": 1,
                "needs_attention": 0,
                "unsupported": 0,
                "failed": 0,
            },
            "areas": ["Civil"],
            "findings": [],
            "plan": None,
            "active_runs": [],
            "boq_count": 0,
            "private_home": r"C:\Users\private\Quantix",
        }

    def list_artifacts(self, tender_id):
        self.get_tender(tender_id)
        return [
            {
                "id": "artifact-1",
                "name": "Spec.pdf",
                "relative_path": "Civil/Spec.pdf",
                "status": "extracted",
                "kind": "pdf",
                "area": "Civil",
                "metadata": {"private_path": r"C:\private\original.pdf"},
            }
        ]

    def messages(self, tender_id):
        return []

    def list_tasks(self, tender_id):
        return []

    def list_plans(self, tender_id):
        self.get_tender(tender_id)
        return self.plans

    def approved_scope(self, tender_id, plan_id):
        self.get_tender(tender_id)
        if plan_id != "plan-1":
            raise KeyError(plan_id)
        return {
            "plan_id": plan_id,
            "title": "Concrete review",
            "tender_revision": 1,
            "rationale": "Review the specified strength only.",
        }

    def get_artifact(self, tender_id, artifact_id):
        self.get_tender(tender_id)
        if artifact_id != "artifact-1":
            raise KeyError(artifact_id)
        return {"id": artifact_id, "version": 1, "is_current": True}

    def search(self, tender_id, query, limit=20):
        self.get_tender(tender_id)
        return [self.evidence["source-1"]]

    def get_evidence(self, tender_id, evidence_id):
        self.get_tender(tender_id)
        return self.evidence[evidence_id]

    def artifact_evidence(self, tender_id, artifact_id, offset=0, limit=50):
        self.get_tender(tender_id)
        if artifact_id != "artifact-1":
            raise KeyError(artifact_id)
        return list(self.evidence.values())[offset : offset + limit]

    def add_message(self, tender_id, role, content, source_ids=None, run_id=None):
        row = dict(role=role, content=content, source_ids=source_ids, run_id=run_id)
        self.replies.append(row)
        return row

    def add_finding(self, tender_id, title, detail, kind, source_ids, origin="agent", run_id=None):
        row = dict(
            title=title,
            detail=detail,
            kind=kind,
            source_ids=source_ids,
            origin=origin,
            run_id=run_id,
            state="proposed",
        )
        self.findings.append(row)
        return row

    def create_plan(self, tender_id, title, tasks, run_id=None):
        row = dict(title=title, tasks=tasks, run_id=run_id, status="proposed")
        self.plans.append(row)
        return row

    def event(self, run_id, kind, message, data=None):
        self.events.append(dict(run_id=run_id, kind=kind, message=message, data=data))


def result_for(output, raw_responses=None):
    return SimpleNamespace(
        final_output=output,
        interruptions=[],
        raw_responses=raw_responses or [],
        context_wrapper=SimpleNamespace(
            usage=SimpleNamespace(
                requests=1,
                input_tokens=100,
                output_tokens=50,
                total_tokens=150,
                input_tokens_details=SimpleNamespace(cached_tokens=10),
                output_tokens_details=SimpleNamespace(reasoning_tokens=25),
            )
        ),
    )


def office_module():
    return importlib.import_module("quantix.office")


def test_derived_measurement_source_keeps_its_proposal_origin_and_status():
    from quantix.office_tools import OfficeContext

    repo = MemoryRepository()
    repo.evidence["source-1"].update(
        kind="measurement",
        metadata={
            "origin": "engineer",
            "measurement_id": "measurement-1",
            "status": "proposed",
            "method": "engineer_marked_calibrated_proposal",
            "source_version": 1,
            "source_hash": "a" * 64,
        },
    )
    result = OfficeContext(repo, "tender-1", "run-1").source("source-1")
    assert result["kind"] == "measurement"
    assert result["metadata"]["origin"] == "engineer"
    assert result["metadata"]["status"] == "proposed"
    assert result["metadata"]["measurement_id"] == "measurement-1"
    assert result["metadata"]["source_version"] == "1"


async def call_tool(agent, name, context, arguments):
    import json

    from agents.tool_context import ToolContext

    tool = next(tool for tool in agent.tools if tool.name == name)
    encoded = json.dumps(arguments)
    return await tool.on_invoke_tool(
        ToolContext(context, tool_name=name, tool_call_id="call-1", tool_arguments=encoded), encoded
    )


@pytest.mark.asyncio
async def test_manager_prepares_source_bound_proposals_without_publishing(monkeypatch):
    office = office_module()
    repo = MemoryRepository()

    async def provider(agent, prompt, **kwargs):
        await call_tool(
            agent, "search_sources", kwargs["context"], {"query": "concrete", "limit": 5}
        )
        return result_for(
            office.OfficeOutput.model_validate(
                {
                    "summary": "The concrete specification requires review.",
                    "source_ids": ["source-1"],
                    "findings": [
                        {
                            "title": "Concrete grade",
                            "detail": "Specified strength is 35 MPa.",
                            "kind": "requirement",
                            "source_ids": ["source-1"],
                        }
                    ],
                    "plan": {
                        "title": "Concrete package review",
                        "tasks": [
                            {
                                "title": "Check concrete scope",
                                "description": "Review concrete clauses and BOQ.",
                                "role": "Concrete specification reviewer",
                                "source_ids": ["source-1"],
                            }
                        ],
                    },
                }
            )
        )

    monkeypatch.setattr(office.Runner, "run", provider)
    result = await office.run_manager(repo, "tender-1", "run-1", "Review the package", "test-key")
    assert result.output.source_ids == ["source-1"]
    assert result.output.findings[0].kind == "requirement"
    assert result.output.plan.tasks[0].role == "Concrete specification reviewer"
    assert result.usage["input_tokens"] == 100
    assert result.output.summary == "The concrete specification requires review."
    assert repo.replies == repo.findings == repo.plans == []


@pytest.mark.asyncio
@pytest.mark.parametrize("source_id", ["fabricated", "source-2"])
async def test_unread_or_fabricated_source_rejects_entire_output_before_writes(
    monkeypatch, source_id
):
    office = office_module()
    repo = MemoryRepository()

    async def provider(agent, prompt, **kwargs):
        return result_for(office.OfficeOutput(summary="Unsupported fact", source_ids=[source_id]))

    monkeypatch.setattr(office.Runner, "run", provider)
    with pytest.raises(ValueError, match="source|evidence"):
        await office.run_manager(repo, "tender-1", "run-1", "Review", "test-key")
    assert repo.replies == []
    assert repo.findings == []
    assert repo.plans == []


@pytest.mark.asyncio
async def test_production_agent_has_bounded_turns_strict_scoped_tools_and_private_client(
    monkeypatch,
):
    office = office_module()
    repo = MemoryRepository()

    async def provider(agent, prompt, **kwargs):
        assert agent.model == "gpt-6-astra"
        assert agent.model_settings.reasoning.effort == "xhigh"
        assert 1 <= kwargs["max_turns"] <= 16
        assert kwargs["run_config"].tracing_disabled is True
        assert "test-key" not in prompt
        assert "C:\\" not in prompt
        for tool in agent.tools:
            if hasattr(tool, "params_json_schema"):
                assert tool.strict_json_schema is True
                assert tool.params_json_schema["additionalProperties"] is False
                assert "tender_id" not in tool.params_json_schema["properties"]
        assert any(tool.name == "web_search" for tool in agent.tools)
        return result_for(office.OfficeOutput(summary="No project evidence has been analysed."))

    monkeypatch.setattr(office.Runner, "run", provider)
    await office.run_manager(repo, "tender-1", "run-1", "Review", "test-key")


@pytest.mark.asyncio
async def test_specialist_uses_saved_task_sources_and_returns_proposals(monkeypatch):
    office = office_module()
    repo = MemoryRepository()

    async def provider(agent, prompt, **kwargs):
        assert "35 MPa" in prompt
        assert "Concrete reviewer" in agent.name
        return result_for(
            office.OfficeOutput(
                summary="Concrete grade checked against the clause.", source_ids=["source-1"]
            )
        )

    monkeypatch.setattr(office.Runner, "run", provider)
    task = {
        "id": "task-1",
        "plan_id": "plan-1",
        "title": "Review concrete",
        "description": "Check specified strength",
        "role": "Concrete reviewer",
        "source_ids": ["source-1"],
    }
    result = await office.run_specialist(repo, "tender-1", "run-1", task, "test-key")
    assert result.output.source_ids == ["source-1"]
    assert result.run_id == "run-1"
    assert repo.replies == []


@pytest.mark.asyncio
async def test_cancellation_propagates_without_a_success_reply(monkeypatch):
    office = office_module()
    repo = MemoryRepository()

    async def provider(*args, **kwargs):
        raise asyncio.CancelledError

    monkeypatch.setattr(office.Runner, "run", provider)
    with pytest.raises(asyncio.CancelledError):
        await office.run_manager(repo, "tender-1", "run-1", "Review", "test-key")
    assert repo.replies == []


@pytest.mark.asyncio
async def test_run_cannot_be_used_for_another_tender(monkeypatch):
    office = office_module()
    repo = MemoryRepository()
    repo.get_run = lambda run_id: {"id": run_id, "tender_id": "other-tender"}
    with pytest.raises(ValueError, match="Tender|tender"):
        await office.run_manager(repo, "tender-1", "run-1", "Review", "test-key")
    assert repo.replies == []


def web_response(url="https://supplier.example/products/rebar"):
    from agents.items import ModelResponse
    from agents.usage import Usage
    from openai.types.responses import ResponseOutputMessage

    message = ResponseOutputMessage(
        id="msg-web",
        role="assistant",
        status="completed",
        type="message",
        content=[
            {
                "type": "output_text",
                "text": "Published rebar price.",
                "annotations": [
                    {
                        "type": "url_citation",
                        "url": url,
                        "title": "Supplier price list",
                        "start_index": 0,
                        "end_index": 21,
                    },
                ],
            }
        ],
    )
    return ModelResponse(
        output=[message],
        usage=Usage(requests=1, input_tokens=100, output_tokens=50, total_tokens=150),
        response_id="resp-web",
    )


def price_proposal(url="https://supplier.example/products/rebar", basis="observed"):
    return {
        "item": "Rebar",
        "amount": "38000.00",
        "currency": "EGP",
        "unit": "tonne",
        "location": "Cairo",
        "tax_basis": "Not stated",
        "basis": basis,
        "observed_on": "2026-09-06",
        "valid_until": None,
        "conditions": "Published listing; delivery and availability require confirmation.",
        "urls": [url],
    }


@pytest.mark.asyncio
async def test_research_retains_provider_citations_and_unapproved_price_basis(monkeypatch):
    office = office_module()
    repo = MemoryRepository()
    url = "https://supplier.example/products/rebar"

    async def provider(agent, prompt, **kwargs):
        return result_for(
            office.OfficeOutput.model_validate(
                {
                    "summary": "A published rebar price is available; commercial terms need confirmation.",
                    "web_findings": [
                        {
                            "title": "Rebar market reference",
                            "detail": "Published supplier listing.",
                            "urls": [url],
                        }
                    ],
                    "price_proposals": [price_proposal()],
                }
            ),
            [web_response()],
        )

    monkeypatch.setattr(office.Runner, "run", provider)
    result = await office.run_manager(
        repo, "tender-1", "run-1", "Research current prices", "test-key"
    )
    assert result.web_sources[0]["url"] == url
    assert result.web_sources[0]["retrieved_at"]
    assert result.output.price_proposals[0].basis == "observed"
    assert result.output.price_proposals[0].tax_basis == "Not stated"
    assert repo.replies == []
    assert not any(event["kind"] == "research_proposed" for event in repo.events)


@pytest.mark.asyncio
async def test_fabricated_web_price_source_rejects_output_before_writes(monkeypatch):
    office = office_module()
    repo = MemoryRepository()

    async def provider(agent, prompt, **kwargs):
        return result_for(
            office.OfficeOutput.model_validate(
                {
                    "summary": "A price was found.",
                    "price_proposals": [price_proposal("https://invented.example/price")],
                }
            ),
            [web_response()],
        )

    monkeypatch.setattr(office.Runner, "run", provider)
    with pytest.raises(ValueError, match="URL|source|search"):
        await office.run_manager(repo, "tender-1", "run-1", "Research", "test-key")
    assert repo.replies == []
    assert repo.findings == []


@pytest.mark.asyncio
async def test_model_output_cannot_include_approval_fields(monkeypatch):
    office = office_module()
    repo = MemoryRepository()

    async def provider(agent, prompt, **kwargs):
        return result_for({"summary": "Approved", "approved": True})

    monkeypatch.setattr(office.Runner, "run", provider)
    with pytest.raises(ValueError):
        await office.run_manager(repo, "tender-1", "run-1", "Review", "test-key")
    assert repo.replies == []


@pytest.mark.asyncio
async def test_manager_can_consult_scoped_specialist_without_recursive_delegation(monkeypatch):
    import json

    office = office_module()
    repo = MemoryRepository()

    async def provider(starting_agent, input, **kwargs):
        if starting_agent.name == "Tender Manager":
            answer = await call_tool(
                starting_agent,
                "consult_specialist",
                kwargs["context"],
                {
                    "role": "Concrete specification reviewer",
                    "brief": "Check concrete requirements",
                    "source_ids": ["source-1"],
                },
            )
            assert json.loads(answer)["source_ids"] == ["source-1"]
            return result_for(
                office.OfficeOutput(summary="Concrete review is ready.", source_ids=["source-1"])
            )
        assert "35 MPa" in input
        assert "Concrete specification reviewer" in input
        assert starting_agent.model_settings.reasoning.effort == "xhigh"
        assert not any(tool.name == "consult_specialist" for tool in starting_agent.tools)
        assert kwargs["max_turns"] <= 8
        return result_for(
            office.OfficeOutput(
                summary="Concrete strength is specified as 35 MPa.", source_ids=["source-1"]
            )
        )

    monkeypatch.setattr(office.Runner, "run", provider)
    result = await office.run_manager(repo, "tender-1", "run-1", "Review concrete", "test-key")
    assert result.output.source_ids == ["source-1"]
    assert any(event["kind"] == "specialist_result" for event in repo.events)
    assert repo.replies == []


@pytest.mark.asyncio
async def test_source_read_keeps_spreadsheet_formula_context_and_marks_truncation(monkeypatch):
    import json

    office = office_module()
    repo = MemoryRepository()
    repo.evidence["source-1"]["text"] = "x" * 15000
    repo.evidence["source-1"]["metadata"] = {
        "cells": [
            {
                "coordinate": "F12",
                "value": "=D12*E12",
                "cached_value": None,
                "formula": "=D12*E12",
                "data_type": "f",
                "number_format": "0.00",
            }
        ],
        "private_path": r"C:\private\pricing.xlsx",
    }

    async def provider(agent, prompt, **kwargs):
        source = json.loads(
            await call_tool(agent, "read_source", kwargs["context"], {"source_id": "source-1"})
        )
        assert source["text_is_partial"] is True
        assert source["metadata"]["cells"][0]["formula"] == "=D12*E12"
        assert "private_path" not in source["metadata"]
        return result_for(office.OfficeOutput(summary="This source needs further inspection."))

    monkeypatch.setattr(office.Runner, "run", provider)
    await office.run_manager(repo, "tender-1", "run-1", "Inspect", "test-key")


@pytest.mark.asyncio
async def test_existing_engineer_decisions_and_plan_survive_manager_context(monkeypatch):
    import json

    office = office_module()
    repo = MemoryRepository()
    original = repo.overview

    def overview(tender_id):
        return original(tender_id) | {
            "findings": [
                {
                    "title": "Finish selection",
                    "detail": "Use supplier A pending quote.",
                    "state": "accepted",
                    "is_stale": False,
                    "source_ids": ["source-1"],
                }
            ],
            "plan": {
                "title": "Approved finish review",
                "status": "approved",
                "tasks": [
                    {
                        "title": "Check delivery",
                        "description": "Research availability",
                        "role": "Buyer",
                        "status": "ready",
                        "source_ids": [],
                    }
                ],
            },
        }

    repo.overview = overview

    async def provider(agent, prompt, **kwargs):
        context = json.loads(prompt)
        assert context["existing_findings"][0]["state"] == "accepted"
        assert context["plan"]["status"] == "approved"
        assert context["plan"]["tasks"][0]["title"] == "Check delivery"
        return result_for(office.OfficeOutput(summary="The approved work remains visible."))

    monkeypatch.setattr(office.Runner, "run", provider)
    await office.run_manager(repo, "tender-1", "run-1", "Where are we?", "test-key")


@pytest.mark.asyncio
async def test_summary_cannot_bypass_web_source_validation(monkeypatch):
    office = office_module()
    repo = MemoryRepository()

    async def provider(agent, prompt, **kwargs):
        return result_for(
            office.OfficeOutput(summary="Price source: https://invented.example/price")
        )

    monkeypatch.setattr(office.Runner, "run", provider)
    with pytest.raises(ValueError, match="URL|source|search"):
        await office.run_manager(repo, "tender-1", "run-1", "Research", "test-key")
    assert repo.replies == []


def test_empty_proposals_are_rejected_before_repository_writes():
    office = office_module()
    with pytest.raises(ValueError):
        office.OfficeOutput(summary="   ")
