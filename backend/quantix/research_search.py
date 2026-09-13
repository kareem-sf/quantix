"""Deferred native public search through exact reviewed direct-API routes."""

from __future__ import annotations

import json

from .ai_connections import is_subscription_profile
from .ai_direct import supports_direct
from .ai_execution import execute_api
from .db import dump, new_id, now
from .manager_runtime import ManagerRunProfiles
from .office_tools import redact_text
from .research_models import (
    PublicFetchRequest,
    PublicSearchHit,
    PublicSearchReceipt,
    PublicSearchRequest,
    ResearchSearchOutput,
)
from .research_search_receipts import (
    request_fingerprint,
    source_scope_fingerprint,
    verify_search_request,
)
from .staff_budget import OfficeBudgetMeter
from .staff_routing import StaffRoutingService

_SCHEMA = (
    """
    CREATE TABLE IF NOT EXISTS public_search_requests(
        id TEXT PRIMARY KEY,
        tender_id TEXT NOT NULL REFERENCES tenders(id),
        root_run_id TEXT NOT NULL REFERENCES runs(id),
        actor_id TEXT NOT NULL,
        requested_by_actor_id TEXT NOT NULL,
        requested_by_assignment_id TEXT,
        requested_by_route_binding_id TEXT,
        requested_by_staff_version INTEGER,
        assignment_id TEXT,
        route_binding_id TEXT,
        plan_id TEXT NOT NULL,
        route_option_id TEXT NOT NULL,
        connection_id TEXT NOT NULL,
        review_fingerprint TEXT NOT NULL,
        source_scope_fingerprint TEXT NOT NULL,
        request_hash TEXT NOT NULL,
        query TEXT NOT NULL,
        result_limit INTEGER NOT NULL,
        status TEXT NOT NULL CHECK(status IN ('deferred','completed','failed')),
        results_json TEXT NOT NULL,
        usage_json TEXT NOT NULL DEFAULT '{}',
        detail TEXT NOT NULL,
        idempotency_key TEXT NOT NULL,
        created_at TEXT NOT NULL,
        completed_at TEXT,
        UNIQUE(tender_id,root_run_id,idempotency_key)
    )
    """,
    """
    CREATE INDEX IF NOT EXISTS public_search_requests_root
    ON public_search_requests(tender_id,root_run_id,status,created_at)
    """,
)

_TRIGGERS = (
    """
    CREATE TRIGGER IF NOT EXISTS public_search_request_identity_immutable
    BEFORE UPDATE ON public_search_requests
    WHEN OLD.id IS NOT NEW.id
      OR OLD.tender_id IS NOT NEW.tender_id
      OR OLD.root_run_id IS NOT NEW.root_run_id
      OR OLD.actor_id IS NOT NEW.actor_id
      OR OLD.requested_by_actor_id IS NOT NEW.requested_by_actor_id
      OR OLD.requested_by_assignment_id IS NOT NEW.requested_by_assignment_id
      OR OLD.requested_by_route_binding_id IS NOT NEW.requested_by_route_binding_id
      OR OLD.requested_by_staff_version IS NOT NEW.requested_by_staff_version
      OR OLD.plan_id IS NOT NEW.plan_id
      OR OLD.route_option_id IS NOT NEW.route_option_id
      OR OLD.connection_id IS NOT NEW.connection_id
      OR OLD.review_fingerprint IS NOT NEW.review_fingerprint
      OR OLD.source_scope_fingerprint IS NOT NEW.source_scope_fingerprint
      OR OLD.request_hash IS NOT NEW.request_hash
      OR OLD.query IS NOT NEW.query
      OR OLD.result_limit IS NOT NEW.result_limit
      OR OLD.idempotency_key IS NOT NEW.idempotency_key
      OR OLD.created_at IS NOT NEW.created_at
    BEGIN SELECT RAISE(ABORT,'Public search request identity is immutable'); END
    """,
    """
    CREATE TRIGGER IF NOT EXISTS public_search_requests_no_delete
    BEFORE DELETE ON public_search_requests
    BEGIN SELECT RAISE(ABORT,'Public search requests are retained'); END
    """,
)


class PublicSearchService:
    def __init__(self, repo):
        self.repo = repo
        self.routing = StaffRoutingService(repo)
        self.policy = self.routing.policy
        with repo.atomic() as conn:
            for statement in _SCHEMA:
                conn.execute(statement)
            columns = {
                row["name"] for row in conn.execute("PRAGMA table_info(public_search_requests)")
            }
            for name, definition in (
                ("requested_by_actor_id", "TEXT NOT NULL DEFAULT 'manager'"),
                ("requested_by_assignment_id", "TEXT"),
                ("requested_by_route_binding_id", "TEXT"),
                ("requested_by_staff_version", "INTEGER"),
                ("usage_json", "TEXT NOT NULL DEFAULT '{}'"),
                ("review_fingerprint", "TEXT NOT NULL DEFAULT ''"),
                ("source_scope_fingerprint", "TEXT NOT NULL DEFAULT ''"),
                ("request_hash", "TEXT NOT NULL DEFAULT ''"),
            ):
                if name not in columns:
                    conn.execute(
                        f"ALTER TABLE public_search_requests ADD COLUMN {name} {definition}"
                    )
            for statement in _TRIGGERS:
                conn.execute(statement)

    @staticmethod
    def _plan_id(context) -> str:
        binding = getattr(context, "route_binding", None)
        if binding is not None and getattr(binding, "plan_id", None):
            return binding.plan_id
        approved = getattr(context, "approved_scope", None) or {}
        value = approved.get("plan_id") if isinstance(approved, dict) else None
        if not isinstance(value, str) or not value:
            raise ValueError("Public search needs an approved work review.")
        return value

    @staticmethod
    def _receipt(row) -> PublicSearchReceipt:
        return PublicSearchReceipt(
            id=row["id"],
            tender_id=row["tender_id"],
            root_run_id=row["root_run_id"],
            actor_id=row["actor_id"],
            requested_by_actor_id=row["requested_by_actor_id"],
            requested_by_assignment_id=row["requested_by_assignment_id"],
            requested_by_route_binding_id=row["requested_by_route_binding_id"],
            requested_by_staff_version=row["requested_by_staff_version"],
            query=row["query"],
            limit=row["result_limit"],
            status=row["status"],
            route_option_id=row["route_option_id"],
            connection_id=row["connection_id"],
            review_fingerprint=row["review_fingerprint"],
            source_scope_fingerprint=row["source_scope_fingerprint"],
            request_hash=row["request_hash"],
            results=json.loads(row["results_json"]),
            usage=json.loads(row["usage_json"]),
            created_at=row["created_at"],
            completed_at=row["completed_at"],
            detail=row["detail"],
        )

    def get(self, tender_id: str, request_id: str) -> PublicSearchReceipt:
        with self.repo.db.connect() as conn:
            row = conn.execute(
                "SELECT * FROM public_search_requests WHERE tender_id=? AND id=?",
                (tender_id, request_id),
            ).fetchone()
        if row is None:
            raise KeyError("This public search request could not be found.")
        return self._receipt(row)

    def pending(self, tender_id: str, root_run_id: str) -> list[PublicSearchReceipt]:
        with self.repo.db.connect() as conn:
            rows = conn.execute(
                """
                SELECT * FROM public_search_requests
                WHERE tender_id=? AND root_run_id=? AND status='deferred'
                ORDER BY created_at,id
                """,
                (tender_id, root_run_id),
            ).fetchall()
        return [self._receipt(row) for row in rows]

    def fail(
        self,
        tender_id: str,
        request_id: str,
        error: BaseException,
        *,
        usage: dict | None = None,
    ) -> PublicSearchReceipt:
        with self.repo.atomic() as conn:
            if usage is None:
                conn.execute(
                    """
                    UPDATE public_search_requests
                    SET status='failed',detail=?,completed_at=?
                    WHERE tender_id=? AND id=? AND status='deferred'
                    """,
                    (redact_text(error)[:1000], now(), tender_id, request_id),
                )
            else:
                conn.execute(
                    """
                    UPDATE public_search_requests
                    SET status='failed',usage_json=?,detail=?,completed_at=?
                    WHERE tender_id=? AND id=? AND status='deferred'
                    """,
                    (
                        dump(usage),
                        redact_text(error)[:1000],
                        now(),
                        tender_id,
                        request_id,
                    ),
                )
        return self.get(tender_id, request_id)

    def queue(
        self, context, request: PublicSearchRequest | dict, idempotency_key: str
    ) -> PublicSearchReceipt:
        request = PublicSearchRequest.model_validate(request)
        context.require_tool("search_public_sources")
        context.ensure_scope_current()
        tender_id, root_run_id = context.tender_id, context.run_id
        requested_by_actor_id = context.actor_id or "manager"
        requested_by_assignment_id = getattr(context, "assignment_id", None)
        requested_by_route_binding_id = getattr(context, "route_binding_id", None)
        requested_by_staff_version = getattr(context, "staff_version", None)
        actor_id = ManagerRunProfiles(self.repo).get(tender_id, root_run_id).id
        plan_id = self._plan_id(context)
        with self.policy.connections.authority_guard(), self.repo.atomic() as conn:
            existing = conn.execute(
                """
                SELECT * FROM public_search_requests
                WHERE tender_id=? AND root_run_id=? AND idempotency_key=?
                """,
                (tender_id, root_run_id, idempotency_key),
            ).fetchone()
            if existing is not None:
                if existing["query"] != request.query or existing["result_limit"] != request.limit:
                    raise ValueError(
                        "This public-search key was already used for a different query."
                    )
            grant, policy, _run = self.routing._validate_root_in_conn(
                conn, tender_id, root_run_id, plan_id
            )
            team_table = conn.execute(
                """
                SELECT 1 FROM sqlite_master
                WHERE type='table' AND name='plan_ai_team'
                """
            ).fetchone()
            team_row = (
                conn.execute(
                    "SELECT data_json FROM plan_ai_team WHERE tender_id=? AND plan_id=?",
                    (tender_id, plan_id),
                ).fetchone()
                if team_table is not None
                else None
            )
            team = json.loads(team_row[0]) if team_row is not None else {}
            manager_route = (
                team.get("manager") if team.get("status") == "approved" else policy.get("manager")
            )
            approved_route_id = next(
                (
                    item.id
                    for item in grant.envelope.route_options
                    if manager_route and OfficeBudgetMeter._same_route(item.route, manager_route)
                ),
                None,
            )
            candidates = []
            for option in grant.envelope.route_options:
                route = option.route
                if not route.web_search:
                    continue
                if set(route.native_tools).difference({"web_search"}):
                    continue
                connection = self.policy.connections.get(route.connection_id)
                if is_subscription_profile(connection) or not supports_direct(connection):
                    continue
                self.routing._validate_option(option, policy)
                candidates.append((option, connection))
            if not candidates:
                raise ValueError(
                    "No reviewed direct-API route can run deferred public search. Choose a search-capable route and review the work again."
                )
            candidates.sort(
                key=lambda item: (item[0].id != approved_route_id, item[0].id)
            )
            option, connection = candidates[0]
            identifier, stamp = new_id(), now()
            scope_fingerprint = source_scope_fingerprint(grant)
            request_identity = {
                "tender_id": tender_id,
                "root_run_id": root_run_id,
                "actor_id": actor_id,
                "requested_by_actor_id": requested_by_actor_id,
                "requested_by_assignment_id": requested_by_assignment_id,
                "requested_by_route_binding_id": requested_by_route_binding_id,
                "requested_by_staff_version": requested_by_staff_version,
                "plan_id": plan_id,
                "route_option_id": option.id,
                "connection_id": connection["id"],
                "query": request.query,
                "result_limit": request.limit,
                "review_fingerprint": grant.review_fingerprint,
                "source_scope_fingerprint": scope_fingerprint,
            }
            request_hash = request_fingerprint(request_identity)
            if existing is not None:
                verify_search_request(
                    conn,
                    existing["id"],
                    tender_id=tender_id,
                    root_run_id=root_run_id,
                    plan_id=plan_id,
                    route_option_id=option.id,
                    connection_id=connection["id"],
                    manager_id=actor_id,
                    review_fingerprint=grant.review_fingerprint,
                    source_scope_fingerprint=scope_fingerprint,
                )
                if existing["request_hash"] != request_hash:
                    raise ValueError("The deferred public search request identity changed.")
                return self._receipt(existing)
            conn.execute(
                """
                INSERT INTO public_search_requests(
                    id,tender_id,root_run_id,actor_id,requested_by_actor_id,
                    requested_by_assignment_id,requested_by_route_binding_id,
                    requested_by_staff_version,assignment_id,route_binding_id,plan_id,
                    route_option_id,connection_id,review_fingerprint,
                    source_scope_fingerprint,request_hash,query,result_limit,status,results_json,
                    usage_json,detail,idempotency_key,created_at,completed_at
                ) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,'deferred','[]','{}',?,?,?,NULL)
                """,
                (
                    identifier,
                    tender_id,
                    root_run_id,
                    actor_id,
                    requested_by_actor_id,
                    requested_by_assignment_id,
                    requested_by_route_binding_id,
                    requested_by_staff_version,
                    None,
                    None,
                    plan_id,
                    option.id,
                    connection["id"],
                    grant.review_fingerprint,
                    scope_fingerprint,
                    request_hash,
                    request.query,
                    request.limit,
                    "Queued until the active AI account lease is released.",
                    idempotency_key,
                    stamp,
                ),
            )
            queued = self._receipt(
                conn.execute(
                    "SELECT * FROM public_search_requests WHERE id=?", (identifier,)
                ).fetchone()
            )
            context.emit_event(
                "public_search_deferred",
                "Public source search is queued until the active AI lease closes.",
                {
                    "research_request_id": queued.id,
                    "route_option_id": queued.route_option_id,
                    "status": queued.status,
                },
            )
            return queued

    async def execute(self, context, request_id: str) -> PublicSearchReceipt:
        queued = self.get(context.tender_id, request_id)
        if queued.status == "completed":
            return queued
        if queued.status == "failed":
            raise ValueError(queued.detail)
        if queued.root_run_id != context.run_id or queued.actor_id != (
            context.actor_id or "manager"
        ):
            raise ValueError("The deferred public search belongs to another actor root.")
        from .office_instructions import OfficeInstructionService

        if any(
            item.kind == "cancel"
            for item in OfficeInstructionService(self.repo).pending_for_turn(
                context.tender_id, context.run_id
            )
        ):
            raise ValueError("The engineer cancelled this root before public search started.")
        plan_id = self._plan_id(context)
        with self.policy.connections.authority_guard(), self.repo.atomic() as conn:
            grant, policy, _run = self.routing._validate_root_in_conn(
                conn, context.tender_id, context.run_id, plan_id
            )
            if queued.requested_by_assignment_id is not None:
                required = (
                    queued.requested_by_route_binding_id,
                    queued.requested_by_staff_version,
                )
                if any(value is None for value in required):
                    raise ValueError("The staff research request identity is incomplete.")
                assignment = conn.execute(
                    "SELECT * FROM office_assignments WHERE tender_id=? AND id=?",
                    (context.tender_id, queued.requested_by_assignment_id),
                ).fetchone()
                if assignment is None or assignment["status"] != "completed":
                    raise ValueError(
                        "The originating staff assignment did not complete before public search."
                    )
                expected_assignment = {
                    "root_run_id": context.run_id,
                    "staff_id": queued.requested_by_actor_id,
                    "staff_version": queued.requested_by_staff_version,
                    "route_binding_id": queued.requested_by_route_binding_id,
                }
                if any(assignment[field] != value for field, value in expected_assignment.items()):
                    raise ValueError("The originating staff research identity changed.")
                from .staff_lifecycle import ensure_staff_can_admit_work

                ensure_staff_can_admit_work(
                    conn,
                    context.tender_id,
                    queued.requested_by_actor_id,
                    assignment_id=queued.requested_by_assignment_id,
                )
                origin_binding = conn.execute(
                    "SELECT * FROM office_route_bindings WHERE tender_id=? AND id=?",
                    (context.tender_id, queued.requested_by_route_binding_id),
                ).fetchone()
                if (
                    origin_binding is None
                    or origin_binding["root_run_id"] != context.run_id
                    or origin_binding["plan_id"] != plan_id
                    or origin_binding["grant_id"] != grant.id
                    or origin_binding["staff_id"] != queued.requested_by_actor_id
                    or origin_binding["staff_version"] != queued.requested_by_staff_version
                ):
                    raise ValueError(
                        "The originating staff route is outside the current reviewed grant."
                    )
            option = next(
                (
                    item
                    for item in grant.envelope.route_options
                    if item.id == queued.route_option_id
                ),
                None,
            )
            if option is None:
                raise ValueError("The reviewed public-search route is no longer available.")
            route, _model = self.routing._validate_option(option, policy)
            connection = self.policy.connections.get(route["connection_id"])
            if is_subscription_profile(connection) or not supports_direct(connection):
                raise ValueError("Deferred public search needs a reviewed direct-API route.")
            verify_search_request(
                conn,
                request_id,
                tender_id=context.tender_id,
                root_run_id=context.run_id,
                plan_id=plan_id,
                route_option_id=option.id,
                connection_id=connection["id"],
                manager_id=context.actor_id or "manager",
                review_fingerprint=grant.review_fingerprint,
                source_scope_fingerprint=source_scope_fingerprint(grant),
            )
        meter = OfficeBudgetMeter(
            self.policy,
            context.tender_id,
            context.run_id,
            route,
            plan_id=plan_id,
            assignment_id=getattr(context, "assignment_id", None) if context.is_staff else None,
            staff_id=getattr(context, "actor_id", None) if context.is_staff else None,
            binding_id=getattr(context, "route_binding_id", None) if context.is_staff else None,
            research_route_option_id=queued.route_option_id,
            research_request_id=request_id,
        )

        system = (
            "Search public sources for the bounded construction-tender question. "
            "Return only relevant source URLs, titles and short summaries. Do not treat a "
            "URL or summary as an exact quotation; Quantix will retain only URLs confirmed "
            "by provider source metadata."
        )
        response_usage: dict = {}
        try:
            with self.policy.connections.lease(connection["id"]):
                response = await execute_api(
                    route,
                    connection,
                    self.policy.connections.credentials(connection["id"]),
                    context,
                    queued.query,
                    ResearchSearchOutput,
                    definitions=[],
                    operation="public_research_search",
                    system_instructions=system,
                    before_request=meter.before_request,
                    on_response=meter.on_response,
                )
            response_usage = response.get("usage") or {}
            provider_sources = {
                item["url"]: item
                for item in response.get("web_sources", [])
                if isinstance(item, dict) and isinstance(item.get("url"), str)
            }
            output = ResearchSearchOutput.model_validate(response["output"])
            summaries = {item.url: item for item in output.results}
            results = []
            for url, source in provider_sources.items():
                try:
                    PublicFetchRequest(url=url)
                except ValueError:
                    continue
                candidate = summaries.get(url)
                results.append(
                    PublicSearchHit(
                        url=url,
                        title=str(source.get("title") or (candidate.title if candidate else ""))[
                            :300
                        ],
                        summary=(candidate.summary if candidate else "")[:2000],
                        retrieved_at=str(source.get("retrieved_at") or now()),
                    )
                )
                if len(results) >= queued.limit:
                    break
            if not results:
                raise ValueError("The provider returned no attributable public source metadata.")
            stamp = now()
            with self.repo.atomic() as conn:
                conn.execute(
                    """
                    UPDATE public_search_requests
                    SET status='completed',results_json=?,usage_json=?,detail=?,completed_at=?
                    WHERE id=? AND status='deferred'
                    """,
                    (
                        dump([item.model_dump(mode="json") for item in results]),
                        dump(response.get("usage") or {}),
                        "Provider-attributed source metadata saved. Open a source to capture exact citable passages.",
                        stamp,
                        request_id,
                    ),
                )
            self.policy.connections.mark_used(connection["id"])
            completed = self.get(context.tender_id, request_id)
            context.emit_event(
                "public_search_completed",
                "Provider-attributed public source results are ready.",
                {
                    "research_request_id": request_id,
                    "status": completed.status,
                    "result_count": len(completed.results),
                },
            )
            return completed
        except BaseException as error:
            meter.interrupted(error)
            self.fail(context.tender_id, request_id, error, usage=response_usage)
            context.emit_event(
                "public_search_failed",
                "Public source search needs attention.",
                {"research_request_id": request_id, "status": "failed"},
            )
            raise


__all__ = [
    "PublicSearchService",
    "source_scope_fingerprint",
    "verify_search_request",
]
