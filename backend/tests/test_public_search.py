"""Deferred public search keeps exact reviewed routing and provider attribution."""

from __future__ import annotations

import sqlite3

import pytest
from test_staff_budget import _approved_root

from quantix.manager_runtime import ManagerRunProfiles
from quantix.office_tools import OfficeContext
from quantix.research_models import PublicSearchRequest, ResearchSearchOutput
from quantix.research_search import PublicSearchService


@pytest.mark.asyncio
async def test_deferred_search_runs_after_lease_on_reviewed_route_and_keeps_provider_sources(
    tmp_path, monkeypatch
):
    repo, tender, plan, root, _policy, _route = _approved_root(
        tmp_path, monkeypatch, max_requests=3, search=True
    )
    context = OfficeContext(repo, tender["id"], root["id"])
    context.actor_id = ManagerRunProfiles(repo).get(tender["id"], root["id"]).id
    context.approved_scope = {"plan_id": plan["id"]}
    service = PublicSearchService(repo)
    queued = service.queue(
        context,
        PublicSearchRequest(query="current concrete pump output", limit=3),
        "deferred-search",
    )
    assert queued.status == "deferred"
    assert service.pending(tender["id"], root["id"]) == [queued]
    with pytest.raises(sqlite3.IntegrityError, match="immutable"):
        with repo.atomic() as conn:
            conn.execute(
                "UPDATE public_search_requests SET query='changed' WHERE id=?",
                (queued.id,),
            )

    async def execute_api(
        route,
        _connection,
        _credentials,
        _context,
        _instruction,
        _output_type,
        **options,
    ):
        reservation = await options["before_request"](100, 128)
        await options["on_response"](
            {
                "requests": 1,
                "input_tokens": 100,
                "output_tokens": 30,
                "web_search_calls": 1,
                "usage_complete": True,
            },
            reservation,
        )
        assert route["connection_id"] == queued.connection_id
        return {
            "output": ResearchSearchOutput(
                results=[
                    {
                        "url": "https://provider.example/pump",
                        "title": "Model title",
                        "summary": "A short non-citable search summary.",
                    },
                    {
                        "url": "https://model-invented.example/not-kept",
                        "title": "Invented",
                        "summary": "Must not survive attribution filtering.",
                    },
                ]
            ),
            "usage": {},
            "web_sources": [
                {
                    "url": "https://provider.example/pump",
                    "title": "Provider source",
                    "retrieved_at": "2026-09-13T00:00:00Z",
                    "cited": True,
                }
            ],
        }

    monkeypatch.setattr("quantix.research_search.execute_api", execute_api)
    completed = await service.execute(context, queued.id)
    assert completed.status == "completed"
    assert [item.url for item in completed.results] == ["https://provider.example/pump"]
    assert completed.results[0].provider_metadata is True
    assert completed.results[0].citable is False
    assert service.pending(tender["id"], root["id"]) == []
