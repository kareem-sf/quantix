"""Shared grounded-research and working-memory tools for Manager and staff."""

from __future__ import annotations

import asyncio
import hashlib
import json
from functools import partial

from .ai_connections import AIConnectionService
from .ai_tools import ToolContext, tool
from .execution_context import identity_from_office_context
from .memory_models import WorkingMemoryCommand, WorkingMemoryDraft
from .memory_service import MemoryService
from .office_tools import OfficeContext
from .research_budget import ResearchBudgetService
from .research_models import (
    MarketObservationCommand,
    MarketObservationDraft,
    PublicFetchCommand,
    PublicFetchRequest,
    PublicSearchRequest,
    ResearchCitationCommand,
    ResearchCitationDraft,
)
from .research_search import PublicSearchService
from .research_service import ResearchService


def _key(operation: str, *parts: str) -> str:
    value = "\x00".join((operation, *parts))
    return f"research-{hashlib.sha256(value.encode('utf-8')).hexdigest()}"


async def _settle_thread(work):
    """Drain an owned blocking operation before propagating cancellation."""

    task = asyncio.create_task(asyncio.to_thread(work))
    cancelled = False
    while True:
        try:
            return await asyncio.shield(task), cancelled, None
        except asyncio.CancelledError:
            cancelled = True
        except BaseException as error:
            return None, cancelled, error


def research_tools(repo=None, *, service: ResearchService | None = None) -> list:
    """Build schemas without a repository; resolve state from trusted context."""

    def active_repo(context: OfficeContext):
        actual = getattr(context, "repo", None)
        if actual is None:
            raise ValueError("The research tool context has no active repository.")
        if repo is not None and actual is not repo:
            raise ValueError("The research tool context does not match its repository.")
        if service is not None and service.repo is not actual:
            raise ValueError("The injected research service belongs to another repository.")
        return actual

    def research_for(context: OfficeContext) -> ResearchService:
        return service or ResearchService(active_repo(context))

    def write_guard(context: OfficeContext):
        return AIConnectionService(active_repo(context)).authority_guard

    def commit_check(
        context: OfficeContext, tool_id: str, *, source_ids: list[str] | None = None
    ):
        def validate() -> None:
            if context.repo.get_run(context.run_id)["status"] not in {
                "queued",
                "running",
            }:
                raise InterruptedError("This Tender run is no longer active.")
            context.require_tool(tool_id)
            context.ensure_scope_current()
            for source_id in source_ids or []:
                context.ensure_evidence_allowed(source_id, tool_id=None)
                if not context.has_seen_source(source_id):
                    raise ValueError(
                        "Read each source before using it in working memory."
                    )

        return validate

    @tool(read_only=False, idempotent=True, requires_invocation_id=True)
    async def search_public_sources(
        ctx: ToolContext[OfficeContext], request: PublicSearchRequest
    ) -> str:
        """Queue bounded provider-attributed public search after the active AI lease closes. Results are not citable until an exact page passage is fetched and cited."""

        context = ctx.context
        prepared = PublicSearchRequest.model_validate(request)
        public_search = PublicSearchService(active_repo(context))
        queued = public_search.queue(
            context,
            prepared,
            _key("search_public_sources", context.run_id, ctx.invocation_id),
        )
        return json.dumps(queued.model_dump(mode="json"), ensure_ascii=False)

    @tool
    async def fetch_public_url(ctx: ToolContext[OfficeContext], request: PublicFetchRequest) -> str:
        """Open one public HTTP(S) source safely and return its exact saved passages. The URL is not a citation until cite_public_passages succeeds."""

        context = ctx.context
        context.require_tool("fetch_public_url")
        context.ensure_scope_current()
        prepared = PublicFetchRequest.model_validate(request)
        research = research_for(context)
        research.validate_public_url(prepared.url)
        key = _key(
            "fetch_public_url",
            context.run_id,
            context.actor_id or "manager",
            context.assignment_id or "manager",
            prepared.url,
            str(prepared.max_bytes),
        )
        budget = ResearchBudgetService(active_repo(context))
        reservation = budget.reserve(context, url=prepared.url, idempotency_key=key)
        receipt, cancelled, failure = await _settle_thread(
            partial(
                research.fetch,
                identity_from_office_context(context),
                PublicFetchCommand(**prepared.model_dump(mode="json"), idempotency_key=key),
                write_guard=write_guard(context),
                validate_commit=commit_check(context, "fetch_public_url"),
            )
        )
        if failure is not None:
            budget.complete(reservation["id"], receipt_id=None, failed=True)
            if cancelled:
                raise asyncio.CancelledError from failure
            raise failure
        budget.complete(reservation["id"], receipt_id=receipt.id)
        if cancelled:
            raise asyncio.CancelledError
        commit_check(context, "fetch_public_url")()
        return json.dumps(receipt.model_dump(mode="json"), ensure_ascii=False)

    @tool
    async def fetch_rendered_public_url(
        ctx: ToolContext[OfficeContext], request: PublicFetchRequest
    ) -> str:
        """Open a rendered page in the isolated Podman reader when ordinary public fetch is insufficient. The saved result still needs an exact citation."""

        from .browser_research import BrowserResearchRuntime

        context = ctx.context
        context.require_tool("fetch_rendered_public_url")
        context.ensure_scope_current()
        prepared = PublicFetchRequest.model_validate(request)
        research = research_for(context)
        research.validate_public_url(prepared.url)
        key = _key(
            "fetch_rendered_public_url",
            context.run_id,
            context.actor_id or "manager",
            context.assignment_id or "manager",
            prepared.url,
            str(prepared.max_bytes),
        )
        budget = ResearchBudgetService(active_repo(context))
        reservation = budget.reserve(context, url=prepared.url, idempotency_key=key)
        try:
            receipt = await research.fetch_with_browser(
                identity_from_office_context(context),
                PublicFetchCommand(**prepared.model_dump(mode="json"), idempotency_key=key),
                BrowserResearchRuntime(active_repo(context)),
                write_guard=write_guard(context),
                validate_commit=commit_check(context, "fetch_rendered_public_url"),
            )
        except BaseException:
            budget.complete(reservation["id"], receipt_id=None, failed=True)
            raise
        budget.complete(reservation["id"], receipt_id=receipt.id)
        commit_check(context, "fetch_rendered_public_url")()
        return json.dumps(receipt.model_dump(mode="json"), ensure_ascii=False)

    @tool(read_only=False, idempotent=True, requires_invocation_id=True)
    async def cite_public_passages(
        ctx: ToolContext[OfficeContext], citation: ResearchCitationDraft
    ) -> str:
        """Cite exact passages from one saved public-research receipt for this Tender."""

        context = ctx.context
        context.require_tool("cite_public_passages")
        context.ensure_scope_current()
        prepared = ResearchCitationDraft.model_validate(citation)
        research = research_for(context)
        saved = research.cite(
            identity_from_office_context(context),
            ResearchCitationCommand(
                **prepared.model_dump(mode="json"),
                idempotency_key=_key("cite_public_passages", context.run_id, ctx.invocation_id),
            ),
            write_guard=write_guard(context),
            validate_commit=commit_check(context, "cite_public_passages"),
        )
        commit_check(context, "cite_public_passages")()
        context.research_state.setdefault("public_citations", {})[saved.url] = {
            "url": saved.url,
            "title": saved.title,
            "retrieved_at": saved.retrieved_at,
            "cited": True,
            "citation_id": saved.id,
            "passage_ids": saved.passage_ids,
            "content_sha256": saved.content_sha256,
        }
        context.emit_event(
            "public_passages_cited",
            "Exact public source passages were cited.",
            {
                "citation_id": saved.id,
                "receipt_id": saved.receipt_id,
                "passage_count": len(saved.passage_ids),
            },
        )
        return json.dumps(saved.model_dump(mode="json"), ensure_ascii=False)

    @tool(read_only=False, idempotent=True, requires_invocation_id=True)
    async def record_market_observation(
        ctx: ToolContext[OfficeContext], observation: MarketObservationDraft
    ) -> str:
        """Record a structured cited quotation, published price or estimate as a Tender draft."""

        context = ctx.context
        context.require_tool("record_market_observation")
        context.ensure_scope_current()
        prepared = MarketObservationDraft.model_validate(observation)
        research = research_for(context)
        saved = research.record_market_observation(
            identity_from_office_context(context),
            MarketObservationCommand(
                **prepared.model_dump(mode="json"),
                idempotency_key=_key(
                    "record_market_observation", context.run_id, ctx.invocation_id
                ),
            ),
            write_guard=write_guard(context),
            validate_commit=commit_check(context, "record_market_observation"),
        )
        commit_check(context, "record_market_observation")()
        return json.dumps(saved.model_dump(mode="json"), ensure_ascii=False)

    @tool
    async def list_research_receipts(
        ctx: ToolContext[OfficeContext], offset: int = 0, limit: int = 20
    ) -> str:
        """List bounded saved public-source receipts and whether exact passages were cited."""

        context = ctx.context
        context.require_tool("list_research_receipts")
        context.ensure_scope_current()
        research = research_for(context)
        if context.is_staff:
            items, excluded = research.list_for_identity(
                identity_from_office_context(context), offset=offset, limit=limit
            )
        else:
            items, excluded = (
                research.list(context.tender_id, offset=offset, limit=limit),
                0,
            )
        context.ensure_scope_current()
        return json.dumps(
            {
                "items": [item.model_dump(mode="json") for item in items],
                "excluded_count": excluded,
            },
            ensure_ascii=False,
        )

    @tool(read_only=False, idempotent=True, requires_invocation_id=True)
    async def save_working_memory(ctx: ToolContext[OfficeContext], note: WorkingMemoryDraft) -> str:
        """Save a scratch note or assumption with only sources this actor actually inspected."""

        context = ctx.context
        context.require_tool("save_working_memory")
        context.ensure_scope_current()
        prepared = WorkingMemoryDraft.model_validate(note)
        memory = MemoryService(active_repo(context))
        saved = memory.save(
            identity_from_office_context(context),
            WorkingMemoryCommand(
                **prepared.model_dump(mode="json"),
                idempotency_key=_key("save_working_memory", context.run_id, ctx.invocation_id),
            ),
            inspected_source_ids=set(context.seen_sources),
            write_guard=write_guard(context),
            validate_commit=commit_check(
                context, "save_working_memory", source_ids=prepared.source_ids
            ),
        )
        commit_check(
            context, "save_working_memory", source_ids=prepared.source_ids
        )()
        return json.dumps(saved.model_dump(mode="json"), ensure_ascii=False)

    @tool
    async def list_working_memory(
        ctx: ToolContext[OfficeContext], offset: int = 0, limit: int = 20
    ) -> str:
        """List Tender scratch notes and assumptions with source-revision status."""

        context = ctx.context
        context.require_tool("list_working_memory")
        context.ensure_scope_current()
        memory = MemoryService(active_repo(context))
        if context.is_staff:
            candidates, excluded = memory.list_for_identity(
                identity_from_office_context(context), offset=offset, limit=limit
            )
            items = [
                item
                for item in candidates
                if all(context.evidence_allowed(source_id) for source_id in item.source_ids)
            ]
            excluded += len(candidates) - len(items)
        else:
            items, excluded = (
                memory.list(context.tender_id, offset=offset, limit=limit),
                0,
            )
        context.ensure_scope_current()
        return json.dumps(
            {
                "items": [item.model_dump(mode="json") for item in items],
                "excluded_count": excluded,
            },
            ensure_ascii=False,
        )

    return [
        search_public_sources,
        fetch_public_url,
        fetch_rendered_public_url,
        cite_public_passages,
        record_market_observation,
        list_research_receipts,
        save_working_memory,
        list_working_memory,
    ]


__all__ = ["research_tools"]
