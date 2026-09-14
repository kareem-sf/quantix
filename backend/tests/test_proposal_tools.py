import hashlib
import json
import typing

import pytest

from quantix.ai_tools import ToolArgumentError
from quantix.office import compose
from quantix.office_tools import OfficeContext
from quantix.office_types import ManagerAnswer
from quantix.proposal_tools import RULES, ProposalKind, proposal_tools
from quantix.repository import Repository


@pytest.fixture
def workspace(tmp_path):
    repo = Repository(tmp_path)
    tender = repo.create_tender("Synthetic proposals")
    text = "Ground slab concrete shall be C30/37."
    artifact, _ = repo.register_artifact(
        tender["id"], "Specs/concrete.pdf", hashlib.sha256(text.encode()).hexdigest(), len(text),
        {"kind": "pdf", "status": "extracted", "segments": [{"locator": "page:1", "text": text}]},
    )
    source_id = repo.artifact_evidence(tender["id"], artifact["id"])[0]["id"]
    run = repo.create_run(tender["id"], "manager", "Plan the concrete review")
    return OfficeContext(repo, tender["id"], run["id"]), source_id


def _tool(name):
    return next(item for item in proposal_tools() if item.name == name)


def _plan(source_id, title="Concrete review"):
    return {"title": title, "tasks": [{"title": "Check slab", "description": "Check the ground slab.",
                                       "role": "Quantity Surveyor", "source_ids": [source_id]}]}


@pytest.mark.asyncio
@pytest.mark.parametrize("kind", typing.get_args(ProposalKind))
async def test_every_kind_has_rules_and_an_item_schema(workspace, kind):
    context, _ = workspace
    result = json.loads(await _tool("proposal_format").invoke(context, {"kind": kind}))
    assert result["rules"] == RULES[kind]
    assert result["item_schema"]["type"] == "object"


@pytest.mark.asyncio
async def test_propose_checks_fields_and_evidence_when_called(workspace):
    context, source_id = workspace
    propose = _tool("propose")
    with pytest.raises(ToolArgumentError):
        await propose.invoke(context, {"kind": "plan", "items": [{"title": "No tasks"}]})
    with pytest.raises(ToolArgumentError, match="read|evidence|source"):
        await propose.invoke(context, {"kind": "plan", "items": [_plan(source_id)]})
    assert context.proposals == {}


@pytest.mark.asyncio
async def test_each_call_replaces_the_kind_and_an_empty_list_withdraws_it(workspace):
    context, source_id = workspace
    context.seen_sources.add(source_id)
    propose = _tool("propose")
    await propose.invoke(context, {"kind": "plan", "items": [_plan(source_id, "First")]})
    await propose.invoke(context, {"kind": "plan", "items": [_plan(source_id, "Second")]})
    output = compose(ManagerAnswer(summary="Plan proposed.", source_ids=[source_id]), context)
    assert output.plan.title == "Second"
    with pytest.raises(ToolArgumentError, match="exactly one"):
        await propose.invoke(context, {"kind": "plan", "items": [_plan(source_id), _plan(source_id)]})
    await propose.invoke(context, {"kind": "plan", "items": []})
    assert compose(ManagerAnswer(summary="Withdrawn."), context).plan is None
