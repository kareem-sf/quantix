"""Repeated identical reads and fruitless search runs return a correction instead of looping."""

import hashlib
import json

import pytest

from quantix.office_tools import OfficeContext, source_tools
from quantix.repository import Repository
from quantix.tool_policy import REPEAT_READ_LIMIT, ToolFenceError, dispatch


def _workspace(tmp_path):
    repo = Repository(tmp_path)
    tender = repo.create_tender("Synthetic stall Tender")
    text = "Concrete grade C30/37 for the ground slab."
    repo.register_artifact(
        tender["id"], "Specs/Concrete.pdf", hashlib.sha256(text.encode()).hexdigest(), len(text),
        {"kind": "pdf", "status": "extracted", "segments": [{"locator": "page:1", "page": 1, "text": text}]},
    )
    run = repo.create_run(tender["id"], "manager", "Synthetic")
    return repo, OfficeContext(repo, tender["id"], run["id"])


def _tool(name):
    return next(item for item in source_tools() if item.name == name)


@pytest.mark.asyncio
async def test_identical_reads_are_refused_until_something_is_saved(tmp_path):
    repo, context = _workspace(tmp_path)
    listing = _tool("list_documents")
    for _ in range(REPEAT_READ_LIMIT):
        await dispatch("direct", listing, context, {})
    with pytest.raises(ToolFenceError, match="already ran") as refused:
        await dispatch("direct", listing, context, {})
    assert refused.value.recoverable is True
    # A different request is still answered.
    await dispatch("direct", _tool("inspect_extraction_coverage"), context, {"exceptions_only": False})

    # A saved change can make the same read return something new.
    await dispatch("direct", _tool("save_work_product"), context,
                   {"kind": "note", "title": "Slab note", "content": "Checked."}, invocation_id="save-note")
    assert json.loads(await dispatch("direct", listing, context, {}))["total"] == 1


@pytest.mark.asyncio
async def test_searches_that_find_nothing_new_stop_once_with_a_way_forward(tmp_path):
    repo, context = _workspace(tmp_path)
    search = _tool("search_sources")
    first = json.loads(await dispatch("direct", search, context, {"query": "concrete"}))
    assert first and not first[0].get("already_returned")
    await dispatch("direct", search, context, {"query": "concrete grade"})
    await dispatch("direct", search, context, {"query": "grade slab"})
    with pytest.raises(ToolFenceError, match="found nothing new") as stopped:
        await dispatch("direct", search, context, {"query": "ground slab concrete"})
    assert stopped.value.recoverable is True
    # The stop is one correction, not a permanent block on searching.
    await dispatch("direct", search, context, {"query": "slab"})
