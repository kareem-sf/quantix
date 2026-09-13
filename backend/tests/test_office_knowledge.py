import json

import pytest
from office_test_support import invoke_tool

from quantix.knowledge import KnowledgeService
from quantix.office_tools import OfficeContext
from quantix.repository import Repository


@pytest.fixture
def workspace(tmp_path):
    repo = Repository(tmp_path)
    tid = repo.create_tender("Current tender")["id"]
    run = repo.create_run(tid, "manager")
    context = OfficeContext(repo, tid, run["id"])
    return repo, context


async def invoke(context, name, args):
    return json.loads(await invoke_tool(context, name, args))


@pytest.mark.asyncio
async def test_office_reads_approved_notes_without_treating_them_as_tender_evidence(workspace):
    repo, context = workspace
    service = KnowledgeService(repo)
    note = service.create(
        dict(
            title="Check lead times",
            content="Ask for delivery lead times.",
            category="method",
            engineer_confirmed=True,
            rationale="Office method",
        )
    )
    result = await invoke(context, "list_reusable_notes", dict(category=None, offset=0, limit=10))
    assert result["notes"][0]["id"] == note["id"]
    assert result["current_tender_evidence"] is False
    assert context.seen_sources == set()
    from quantix.office_tools import source_tools

    assert all(
        "approve" not in tool.name and "withdraw" not in tool.name for tool in source_tools()
    )


@pytest.mark.asyncio
async def test_dated_price_notes_keep_revalidation_warning_and_full_content_pages(workspace):
    repo, context = workspace
    note = KnowledgeService(repo).create(
        dict(
            title="Old allowance",
            content="x" * 9000 + "Final condition.",
            category="price",
            engineer_confirmed=True,
            rationale="Reference only",
            verified_on="2026-01-01",
        )
    )
    result = await invoke(
        context, "read_reusable_note", dict(knowledge_id=note["id"], offset=8000, limit=2000)
    )
    assert result["content"].endswith("Final condition.")
    assert result["commercial_revalidation_required"] is True
    assert result["needs_recheck"] is True
    assert result["next_offset"] is None


@pytest.mark.asyncio
async def test_withdrawn_notes_are_not_suggested_but_retained_with_their_state(workspace):
    repo, context = workspace
    service = KnowledgeService(repo)
    note = service.create(
        dict(
            title="Old method",
            content="Withdrawn practice",
            category="method",
            engineer_confirmed=True,
            rationale="Original approval",
        )
    )
    service.withdraw(note["id"], dict(engineer_confirmed=True, rationale="Replaced"))
    result = await invoke(context, "list_reusable_notes", dict(category=None, offset=0, limit=10))
    assert result["notes"] == []
    record = await invoke(
        context, "read_reusable_note", dict(knowledge_id=note["id"], offset=0, limit=8000)
    )
    assert record["status"] == "withdrawn" and record["needs_recheck"] is True
