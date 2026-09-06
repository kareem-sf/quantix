import json

import pytest
from test_office import call_tool, result_for
from test_office_business import setup  # noqa: F401

from quantix import office
from quantix.office_tools import OfficeContext
from quantix.office_types import OfficeOutput


def proposal(source_id):
    return {
        "kind": "work_item", "title": "Foundation concrete",
        "detail": "Concrete foundation scope from the supplied BOQ.",
        "source_ids": [source_id],
    }


@pytest.mark.asyncio
async def test_manager_map_proposals_require_read_evidence_and_publish_atomically(setup, monkeypatch):
    repo, tid, artifact, estimates, item, quotes, run = setup

    async def provider(agent, prompt, **kwargs):
        kwargs["context"].source(item["source_id"])
        return result_for(OfficeOutput.model_validate({
            "summary": "A work item is mapped for review.",
            "source_ids": [item["source_id"]],
            "project_map_nodes": [proposal(item["source_id"])],
        }))

    monkeypatch.setattr(office.Runner, "run", provider)
    prepared = await office.run_manager(repo, tid, run["id"], run["instruction"], "test-key")
    from quantix.project_map import ProjectMapService

    service = ProjectMapService(repo)
    assert service.view(tid)["nodes"] == []
    with pytest.raises(RuntimeError, match="finalization"):
        with repo.atomic():
            office.publish_prepared(repo, prepared)
            raise RuntimeError("Simulated finalization fault")
    assert service.view(tid)["nodes"] == []
    with repo.atomic():
        published = office.publish_prepared(repo, prepared)
    node = published["project_map_nodes"][0]
    assert node["origin"] == "agent"
    assert node["state"] == "proposed"
    assert node["approval_valid"] is False
    assert service.view(tid)["review_scopes"] == []


@pytest.mark.asyncio
async def test_map_output_cannot_cite_unread_or_superseded_source(setup, monkeypatch):
    repo, tid, artifact, estimates, item, quotes, run = setup

    async def provider(agent, prompt, **kwargs):
        return result_for(OfficeOutput.model_validate({
            "summary": "Unreviewed map proposal",
            "project_map_nodes": [proposal(item["source_id"])],
        }))

    monkeypatch.setattr(office.Runner, "run", provider)
    with pytest.raises(ValueError, match="read"):
        await office.run_manager(repo, tid, run["id"], run["instruction"], "test-key")


@pytest.mark.asyncio
async def test_map_read_tool_keeps_approval_separate_from_review_coverage(setup):
    repo, tid, artifact, estimates, item, quotes, run = setup
    from quantix.project_map import ProjectMapService

    ProjectMapService(repo).propose(tid, proposal(item["source_id"]), origin="agent", run_id=run["id"])
    context = OfficeContext(repo, tid, run["id"])
    result = json.loads(await call_tool(
        office._agent("Tender Manager", "gpt-6-astra"), "inspect_project_map", context,
        {"offset": 0, "limit": 10},
    ))
    assert result["nodes"][0]["state"] == "proposed"
    assert result["coverage"]["reviewed_artifacts_in_full"] == 0
    assert item["source_id"] not in context.seen_sources
