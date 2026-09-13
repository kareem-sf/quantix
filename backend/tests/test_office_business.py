import hashlib
import json

import pytest
from office_test_support import api_result_for, approve_plan_and_team, invoke_json_tool
from test_catalog_authority import configured_office
from test_estimates import cell
from test_rate_proposals import proposal_payload

from quantix import office
from quantix.correspondence import QuoteService
from quantix.estimates import EstimateService
from quantix.office_research import ResearchRecord
from quantix.office_tools import OfficeContext
from quantix.office_types import OfficeOutput
from quantix.repository import Repository


@pytest.fixture
def setup(tmp_path, monkeypatch):
    repo = Repository(tmp_path)
    tid = repo.create_tender("Concrete tender")["id"]
    configured_office(
        tmp_path,
        monkeypatch,
        repo=repo,
        tender={"id": tid},
        model_id="gpt-6-astra",
        reasoning="xhigh",
    )
    source = b"Synthetic saved BOQ source"
    digest = hashlib.sha256(source).hexdigest()
    (repo.objects / digest).write_bytes(source)
    artifact, _ = repo.register_artifact(
        tid,
        "BOQ.xlsx",
        digest,
        len(source),
        {
            "kind": "spreadsheet",
            "status": "extracted",
            "segments": [
                {
                    "locator": "sheet:BOQ/row:2",
                    "text": "Concrete 12.5 m3. Supplier contact sales@supplier.example.",
                    "kind": "spreadsheet_row",
                    "sheet": "BOQ",
                    "metadata": {
                        "cells": [
                            cell("B2", "Concrete foundations"),
                            cell("C2", "m3"),
                            cell("D2", 12.5),
                        ]
                    },
                }
            ],
        },
    )
    estimates = EstimateService(repo)
    item = estimates.refresh(tid)["items"][0]
    quotes = QuoteService(repo)
    run = repo.create_run(tid, "manager", "Prepare a concrete RFQ and rate allowance")
    return repo, tid, artifact, estimates, item, quotes, run


@pytest.mark.asyncio
async def test_agent_results_prepare_business_proposals_then_publish_atomically(setup, monkeypatch):
    repo, tid, artifact, estimates, item, quotes, run = setup

    async def provider(route, connection, credentials, context, prompt, output_type, **kwargs):
        result = await invoke_json_tool(
            context, "inspect_estimate", {"offset": 0, "limit": 10}, options=kwargs
        )
        assert result["items"][0]["source"]["id"] == item["source_id"]
        return api_result_for(
            OfficeOutput.model_validate(
                {
                    "summary": "Draft request and rate allowance are ready for review.",
                    "source_ids": [item["source_id"]],
                    "quote_drafts": [
                        {
                            "to": ["sales@supplier.example"],
                            "subject": "Concrete quotation",
                            "body": "Please quote the supplied scope.",
                            "attachment_ids": [artifact["id"]],
                            "source_ids": [item["source_id"]],
                        }
                    ],
                    "unit_rate_proposals": [{"item_id": item["id"], **proposal_payload()}],
                }
            )
        )

    monkeypatch.setattr("quantix.ai_execution.execute_api", provider)
    prepared = await office.run_manager(repo, tid, run["id"], run["instruction"])
    assert quotes.list_drafts(tid) == []
    assert estimates.list_rate_proposals(tid) == []
    with repo.atomic():
        published = office.publish_prepared(repo, prepared)
    assert published["quote_drafts"][0]["status"] == "draft"
    assert published["unit_rate_proposals"][0]["status"] == "proposed"
    assert estimates.view(tid)["items"][0]["unit_rate"] is None
    assert estimates.view(tid)["items"][0]["confirmed"] is False


@pytest.mark.asyncio
async def test_unread_item_or_invented_supplier_recipient_cannot_be_published(setup, monkeypatch):
    repo, tid, artifact, estimates, item, quotes, run = setup

    async def provider(route, connection, credentials, context, prompt, output_type, **kwargs):
        return api_result_for(
            OfficeOutput.model_validate(
                {
                    "summary": "Proposed allowance",
                    "unit_rate_proposals": [{"item_id": item["id"], **proposal_payload()}],
                }
            )
        )

    monkeypatch.setattr("quantix.ai_execution.execute_api", provider)
    with pytest.raises(ValueError, match="read|item|basis"):
        await office.run_manager(repo, tid, run["id"], run["instruction"])
    assert estimates.list_rate_proposals(tid) == []


@pytest.mark.asyncio
async def test_semantic_unavailable_is_explicit_and_source_pagination_keeps_match_offset(
    setup, monkeypatch
):
    repo, tid, artifact, estimates, item, quotes, run = setup
    context = OfficeContext(repo, tid, run["id"])
    monkeypatch.setattr(
        repo,
        "search",
        lambda *args, **kwargs: (_ for _ in ()).throw(AssertionError("No keyword fallback")),
    )
    result = await invoke_json_tool(
        context, "search_semantic_sources", {"query": "foundation concrete", "limit": 5}
    )
    assert result["available"] is False
    assert result["status"] in {"model_missing", "not_indexed", "empty"}
    source = await invoke_json_tool(
        context,
        "read_source",
        {"source_id": item["source_id"], "offset": 9, "limit": 12},
    )
    assert source["text_offset"] == 9
    assert source["text"] == "12.5 m3. Sup"
    assert source["next_offset"] == 21


def test_recipient_in_unread_source_tail_is_not_treated_as_read_evidence(setup):
    repo, tid, artifact, estimates, item, quotes, run = setup
    context = OfficeContext(repo, tid, run["id"])
    context.source(item["source_id"], 0, 8)
    output = OfficeOutput(
        summary="Draft prepared",
        quote_drafts=[
            {
                "to": ["sales@supplier.example"],
                "subject": "Concrete RFQ",
                "body": "Please quote.",
                "source_ids": [item["source_id"]],
            }
        ],
    )
    with pytest.raises(ValueError, match="recipient|read"):
        office.prepare_result(output, context, {}, ResearchRecord(context))


@pytest.mark.asyncio
async def test_prepared_business_result_rechecks_item_before_any_publication(setup, monkeypatch):
    from test_estimates import approval

    repo, tid, artifact, estimates, item, quotes, run = setup
    context = OfficeContext(repo, tid, run["id"])
    await invoke_json_tool(context, "inspect_estimate", {"offset": 0, "limit": 10})
    output = OfficeOutput(
        summary="Draft prepared",
        quote_drafts=[
            {
                "to": ["sales@supplier.example"],
                "subject": "Concrete RFQ",
                "body": "Please quote.",
                "source_ids": [item["source_id"]],
            }
        ],
        unit_rate_proposals=[{"item_id": item["id"], **proposal_payload()}],
    )
    prepared = office.prepare_result(output, context, {}, ResearchRecord(context))
    estimates.update_item(tid, item["id"], approval(confirm_source=True))
    with pytest.raises(ValueError, match="changed"):
        with repo.atomic():
            office.publish_prepared(repo, prepared)
    assert quotes.list_drafts(tid) == []
    assert estimates.list_rate_proposals(tid) == []


@pytest.mark.asyncio
async def test_publication_failure_rolls_back_quotes_and_rate_proposals(setup, monkeypatch):
    repo, tid, artifact, estimates, item, quotes, run = setup
    context = OfficeContext(repo, tid, run["id"])
    await invoke_json_tool(context, "inspect_estimate", {"offset": 0, "limit": 10})
    output = OfficeOutput(
        summary="Draft prepared",
        quote_drafts=[
            {
                "to": ["sales@supplier.example"],
                "subject": "Concrete RFQ",
                "body": "Please quote.",
                "source_ids": [item["source_id"]],
            }
        ],
        unit_rate_proposals=[{"item_id": item["id"], **proposal_payload()}],
    )
    prepared = office.prepare_result(output, context, {}, ResearchRecord(context))
    monkeypatch.setattr(
        repo,
        "add_message",
        lambda *args, **kwargs: (_ for _ in ()).throw(ValueError("Publication interrupted")),
    )
    with pytest.raises(ValueError, match="Publication"):
        with repo.atomic():
            office.publish_prepared(repo, prepared)
    assert quotes.list_drafts(tid) == []
    assert estimates.list_rate_proposals(tid) == []


@pytest.mark.asyncio
async def test_reply_tool_returns_citable_text_without_accepting_commercial_data(setup):
    repo, tid, artifact, estimates, item, quotes, run = setup
    quote = quotes.create_draft(
        tid, {"to": ["sales@supplier.example"], "subject": "RFQ", "body": "Please quote."}
    )
    reply = quotes.register_reply(
        tid,
        quote["id"],
        {
            "sender": "sales@supplier.example",
            "received_at": "2026-09-06T12:00:00Z",
            "text": "Quoted rate EGP 125 excluding VAT.",
        },
    )
    context = OfficeContext(repo, tid, run["id"])
    result = await invoke_json_tool(
        context, "read_quote_replies", {"quote_id": quote["id"], "offset": 0, "limit": 5}
    )
    assert "EGP 125" in result["replies"][0]["sources"][0]["text"]
    assert reply["source_ids"][0] in context.seen_sources
    assert estimates.view(tid)["items"][0]["unit_rate"] is None


@pytest.mark.asyncio
async def test_semantic_hit_reads_matching_offset_instead_of_source_start(setup):
    repo, tid, artifact, estimates, item, quotes, run = setup
    text = "A" * 15000 + "MATCHED CLAUSE" + "B" * 200
    with repo.db.connect(write=True) as conn:
        conn.execute("UPDATE evidence SET text=? WHERE id=?", (text, item["source_id"]))

    class SemanticDouble:
        def search(self, *args):
            return [
                {
                    "id": item["source_id"],
                    "score": 0.9,
                    "metadata": {"semantic_match": {"start": 15000, "end": 15014}},
                }
            ]

    context = OfficeContext(repo, tid, run["id"], semantic_service=SemanticDouble())
    result = await invoke_json_tool(
        context, "search_semantic_sources", {"query": "matched clause", "limit": 5}
    )
    assert result["sources"][0]["text"].startswith("MATCHED CLAUSE")
    assert result["sources"][0]["text_offset"] == 15000


@pytest.mark.asyncio
async def test_supplier_named_in_approved_engineer_scope_is_preserved_for_publication(
    setup, monkeypatch
):
    repo, tid, artifact, estimates, item, quotes, run = setup
    plan = repo.create_plan(
        tid,
        "Request quotations",
        [
            {
                "title": "Prepare request",
                "description": "Draft an RFQ",
                "role": "Buyer",
                "source_ids": [item["source_id"]],
            }
        ],
    )
    repo.update_run(run["id"], status="completed")
    approve_plan_and_team(
        repo,
        tid,
        plan,
        "Prepare the request for named.contact@supplier.example; do not send it.",
    )
    run = repo.create_run(tid, "manager", "Prepare the approved request")

    async def provider(route, connection, credentials, context, prompt, output_type, **kwargs):
        await invoke_json_tool(
            context, "read_source", {"source_id": item["source_id"]}, options=kwargs
        )
        return api_result_for(
            OfficeOutput(
                summary="Request drafted for review",
                quote_drafts=[
                    {
                        "to": ["named.contact@supplier.example"],
                        "subject": "Concrete quotation",
                        "body": "Please quote the scope.",
                        "source_ids": [item["source_id"]],
                    }
                ],
            )
        )

    monkeypatch.setattr("quantix.ai_execution.execute_api", provider)
    prepared = await office.run_manager(repo, tid, run["id"], "Prepare the approved request")
    with repo.atomic():
        result = office.publish_prepared(repo, prepared)
    assert result["quote_drafts"][0]["to"] == ["named.contact@supplier.example"]
    assert result["quote_drafts"][0]["status"] == "draft"


@pytest.mark.asyncio
async def test_full_engineer_instruction_and_preferences_reach_manager_and_specialist(
    setup, monkeypatch
):
    repo, tid, artifact, estimates, item, quotes, run = setup
    preferences = "Use metric units. " * 500
    repo.set_setting("preferences", preferences)
    instruction = (
        "Standing engineer preferences:\n"
        + preferences
        + "\nCurrent instruction:\n"
        + "Review scope. " * 1400
        + " Reference C:\\x only. KEEP_THIS_FINAL_ENGINEER_LIMIT"
    )

    async def provider(route, connection, credentials, context, prompt, output_type, **kwargs):
        data = json.loads(prompt)
        assert data["engineer_request"].endswith("KEEP_THIS_FINAL_ENGINEER_LIMIT")
        assert data["standing_engineer_preferences"] == preferences
        return api_result_for(OfficeOutput(summary="Scope recorded for review"))

    monkeypatch.setattr("quantix.ai_execution.execute_api", provider)
    await office.run_manager(repo, tid, run["id"], instruction)
    repo.update_run(run["id"], status="completed")
    plan = repo.create_plan(
        tid,
        "Scope review",
        [
            {
                "title": "Review scope",
                "description": "Review. " * 1000 + "KEEP_THIS_FINAL_ENGINEER_LIMIT",
                "role": "Scope reviewer",
                "source_ids": [item["source_id"]],
            }
        ],
    )
    approved = approve_plan_and_team(repo, tid, plan, "Proceed with the complete written scope.")
    specialist_run = repo.create_run(tid, "task", approved["tasks"][0]["description"])
    await office.run_specialist(repo, tid, specialist_run["id"], approved["tasks"][0])


def test_oversized_worker_instruction_is_rejected_instead_of_clipped(setup):
    repo, tid, artifact, estimates, item, quotes, run = setup
    with pytest.raises(ValueError, match="instruction|limit"):
        office._prompt(OfficeContext(repo, tid, run["id"]), "x" * 40001, None)


@pytest.mark.asyncio
async def test_remaining_findings_and_full_work_records_are_paginated_and_scoped(setup):
    repo, tid, artifact, estimates, item, quotes, run = setup
    for index in range(42):
        repo.add_finding(
            tid, f"Finding {index}", "Source clause to review", "question", [item["source_id"]]
        )
    detail = "Binding recorded context. " * 650 + "FINAL_RECORDED_CONDITION"
    finding = repo.add_finding(tid, "Full finding", detail, "assumption", [item["source_id"]])
    repo.decide_finding(tid, finding["id"], "accept", "Engineer accepts this recorded assumption.")
    context = OfficeContext(repo, tid, run["id"])
    page = await invoke_json_tool(
        context, "inspect_tender_records", {"record_type": "findings", "offset": 40, "limit": 10}
    )
    assert len(page["records"]) == 3
    assert page["records"][-1]["state"] == "accepted"
    assert page["records"][-1]["is_stale"] is False
    offset, pieces = 0, []
    while offset is not None:
        chunk = await invoke_json_tool(
            context,
            "read_tender_record",
            {
                "record_type": "findings",
                "record_id": finding["id"],
                "offset": offset,
                "limit": 4000,
            },
        )
        pieces.append(chunk["json_text"])
        offset = chunk["next_offset"]
    assert json.loads("".join(pieces))["detail"] == detail
    assert context.seen_sources == set()


@pytest.mark.parametrize(
    "recipient,allowed",
    [("public.sales@supplier.example", True), ("invented@supplier.example", False)],
)
def test_validated_web_research_can_propose_supplier_contacts_but_not_invent_them(
    setup, recipient, allowed
):
    repo, tid, artifact, estimates, item, quotes, run = setup
    context = OfficeContext(repo, tid, run["id"])
    research = ResearchRecord(context)
    url = "https://supplier.example/contact"
    research.add_sources([{"url": url, "title": "Supplier price list", "cited": True}])
    output = OfficeOutput(
        summary="Supplier request proposed",
        web_findings=[
            {
                "title": "Public supplier contact",
                "detail": "Supplier contact email: public.sales@supplier.example",
                "urls": [url],
            }
        ],
        quote_drafts=[
            {
                "to": [recipient],
                "subject": "Concrete quotation",
                "body": "Please provide a quotation for review.",
            }
        ],
    )
    if not allowed:
        with pytest.raises(ValueError, match="recipient"):
            office.prepare_result(output, context, {}, research)
        return
    prepared = office.prepare_result(output, context, {}, research)
    with repo.atomic():
        result = office.publish_prepared(repo, prepared)
    assert result["quote_drafts"][0]["status"] == "draft"
    assert result["web_findings"][0]["urls"] == [url]
    assert result["web_sources"][0]["url"] == url
