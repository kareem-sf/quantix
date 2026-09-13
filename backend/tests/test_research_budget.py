"""Local public fetches share the durable reviewed root search allowance."""

from __future__ import annotations

import pytest
from test_staff_budget import _approved_root

from quantix.office_tools import OfficeContext
from quantix.research_budget import ResearchBudgetService, research_search_usage


def test_public_fetch_reservations_are_idempotent_and_bounded(tmp_path, monkeypatch):
    repo, tender, plan, root, _policy, _route = _approved_root(
        tmp_path, monkeypatch, max_requests=3, search=True
    )
    context = OfficeContext(repo, tender["id"], root["id"])
    context.actor_id = "manager"
    context.approved_scope = {"plan_id": plan["id"]}
    service = ResearchBudgetService(repo)
    with repo.db.connect() as conn:
        grant = service.routing._approved_grant_in_conn(conn, tender["id"], plan["id"])
        limit = grant.envelope.max_search_calls

    first = service.reserve(
        context, url="https://public.example/source", idempotency_key="public-1"
    )
    replay = service.reserve(
        context, url="https://public.example/source", idempotency_key="public-1"
    )
    assert replay["id"] == first["id"]
    service.complete(first["id"], receipt_id="receipt-1")
    for index in range(1, limit):
        service.reserve(
            context,
            url=f"https://public.example/source-{index}",
            idempotency_key=f"public-{index + 1}",
        )
    with pytest.raises(ValueError, match="allowance"):
        service.reserve(
            context,
            url="https://public.example/over-limit",
            idempotency_key="public-over-limit",
        )
    with repo.db.connect() as conn:
        assert research_search_usage(conn, tender["id"], root["id"]) == limit
