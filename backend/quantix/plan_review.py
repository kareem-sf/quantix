"""Read-only plan review and atomic approval of a displayed AI team.

The review is deliberately computed from current records on every read.  Its
fingerprint covers the plan, source revision, routing assignments, account
and model evidence, budgets, usage reservations and restore state.  Approval
recomputes that same object while holding the connection authority guard and
the repository transaction, so a stale screen cannot grant a different route.
"""

from __future__ import annotations

import hashlib
import json
from collections.abc import Callable
from typing import Any
from urllib.parse import quote

from .ai_connections import is_subscription_profile
from .ai_models import AIRoute
from .ai_policy import AIPolicyService
from .ai_readiness import ready_evidence
from .ai_subscription import allows_extras
from .db import dump, new_id, now
from .delegation_proposals import DelegationProposalService
from .plan_review_models import (
    DelegationArtifactOption,
    DelegationArtifactOptionPage,
    DelegationOptions,
    DelegationProposal,
    DelegationProposalEdit,
    PlanApprovalResult,
    PlanReview,
    PlanReviewApproval,
    PlanReviewBlocker,
    PlanReviewChange,
    PlanReviewRoute,
    PlanReviewScope,
    PlanReviewSnapshot,
    PlanReviewTask,
    WorkIntent,
)
from .staff_capabilities import capability_catalog
from .staff_routing_models import (
    ArtifactBasis,
    DelegationEnvelope,
    DelegationRouteOption,
    model_revision_fingerprint,
    route_option_id,
)

_DELEGATION_OUTPUT_KINDS = (
    "summary",
    "findings",
    "plan",
    "web_findings",
    "price_proposals",
    "quote_drafts",
    "unit_rate_proposals",
    "project_map_nodes",
    "submission_requirements",
    "programme_proposal",
    "drawing_measurements",
    "draft_documents",
    "quantity_proposals",
    "boq_item_proposals",
)
_DELEGATION_ARTIFACT_PAGE_SIZE = 50


def _fingerprint(value: Any) -> str:
    return hashlib.sha256(dump(value).encode("utf-8")).hexdigest()


def _review_fingerprint(value: Any) -> str:
    """Hash review authority while ignoring observation freshness fields."""
    def stable(item):
        if isinstance(item, dict):
            return {
                key: stable(value)
                for key, value in item.items()
                if key not in {"updated_at", "as_of"}
            }
        if isinstance(item, list):
            return [stable(value) for value in item]
        return item

    return _fingerprint(stable(value))


class PlanReviewService:
    """Build and approve a plan review without changing state during reads."""

    def __init__(
        self,
        repo,
        *,
        policy: AIPolicyService | None = None,
        save_runs_in_transaction: Callable[[str, str, dict, PlanReview], list[dict]] | None = None,
        schedule_after_commit: Callable[[str, str, list[dict]], Any] | None = None,
    ):
        self.repo = repo
        self.policy = policy or AIPolicyService(repo)
        self.connections = self.policy.connections
        self.proposals = DelegationProposalService(repo)
        self.save_runs_in_transaction = save_runs_in_transaction
        self.schedule_after_commit = schedule_after_commit
        with repo.db.connect(write=True) as conn:
            conn.execute(
                """CREATE TABLE IF NOT EXISTS plan_review_approvals(
                    id TEXT PRIMARY KEY,
                    tender_id TEXT NOT NULL REFERENCES tenders(id),
                    plan_id TEXT NOT NULL REFERENCES plans(id),
                    fingerprint TEXT NOT NULL,
                    rationale TEXT NOT NULL,
                    review_json TEXT NOT NULL,
                    work_intents_json TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    UNIQUE(plan_id, fingerprint)
                )"""
            )

    @staticmethod
    def _saved_team(conn, tender_id: str, plan_id: str) -> dict | None:
        row = conn.execute(
            "SELECT data_json FROM plan_ai_team WHERE tender_id=? AND plan_id=?",
            (tender_id, plan_id),
        ).fetchone()
        return json.loads(row[0]) if row else None

    @staticmethod
    def _active_runs(conn, tender_id: str) -> list[str]:
        return [
            row[0]
            for row in conn.execute(
                "SELECT id FROM runs WHERE tender_id=? AND status IN ('queued','running') ORDER BY created_at,id",
                (tender_id,),
            )
        ]

    @staticmethod
    def _source_ids(plan: dict) -> list[str]:
        return sorted({source_id for task in plan["tasks"] for source_id in task.get("source_ids", [])})

    def _candidate_team(self, conn, tender_id: str, plan: dict, policy: dict) -> tuple[dict, bool]:
        """Return the displayed team and whether it was already persisted.

        Existing task assignments win over policy defaults. This is the key
        refresh rule: a policy edit cannot silently replace a specialist's
        requested reasoning, output, search or route settings.
        """

        saved = self._saved_team(conn, tender_id, plan["id"])
        if saved is not None:
            return {
                "manager": saved.get("manager"),
                "specialists": list(saved.get("specialists", [])),
                "fallback_routes": list(saved.get("fallback_routes", [])),
                "warnings": list(saved.get("warnings", [])),
                "policy_revision": saved.get("policy_revision", 0),
                "status": saved.get("status", "proposed"),
            }, True

        specialists = []
        for task in plan["tasks"]:
            route = (
                policy.get("role_routes", {}).get(task["role"])
                or policy.get("specialist")
                or policy.get("manager")
            )
            if route:
                specialists.append(
                    {
                        "task_id": task["id"],
                        "role": task["role"],
                        "route": route,
                        "rationale": "Proposed from the Tender's approved route choices.",
                    }
                )
        return {
            "manager": policy.get("manager"),
            "specialists": specialists,
            "fallback_routes": list(policy.get("fallback_routes", [])),
            "warnings": [],
            "policy_revision": policy.get("revision", 0),
            "status": "proposed",
        }, False

    @staticmethod
    def _route_entries(team: dict, plan: dict) -> list[dict]:
        entries: list[dict] = []
        if team.get("manager"):
            entries.append({"kind": "manager", "task_id": None, "role": "Tender Manager", "route": team["manager"]})
        task_roles = {task["id"]: task["role"] for task in plan["tasks"]}
        for member in team.get("specialists", []):
            route = member.get("route")
            if not route:
                continue
            task_id = member.get("task_id")
            entries.append(
                {
                    "kind": "specialist",
                    "task_id": task_id,
                    "role": member.get("role") or task_roles.get(task_id),
                    "route": route,
                }
            )
        for route in team.get("fallback_routes", []):
            if route:
                entries.append({"kind": "fallback", "task_id": None, "role": "Saved alternative", "route": route})
        return entries

    def _readiness(self, connection: dict, model_id: str) -> tuple[str, dict | None]:
        if connection.get("credential_state") == "missing":
            return "missing", None
        try:
            evidence = ready_evidence(self.repo, connection, model_id)
            return ("ready", evidence) if evidence else ("unknown", None)
        except (KeyError, ValueError, OSError):
            return "unknown", None

    @staticmethod
    def _blocker(code: str, detail: str, target: str, **values) -> dict:
        return PlanReviewBlocker(
            code=code,
            detail=detail,
            repair_target=target,
            **values,
        ).model_dump(mode="json")

    def _route_review(
        self,
        policy: dict,
        entry: dict,
        blockers: list[dict],
        changes: list[dict],
    ) -> tuple[dict, dict]:
        raw_route = AIRoute.model_validate(entry["route"]).model_dump(mode="json")
        connection_id = raw_route["connection_id"]
        settings_target = f"/settings?section=accounts&connection={connection_id}"
        policy_target = f"/tenders/{policy.get('tender_id', '')}/work?view=ai"
        try:
            connection = self.connections.get(connection_id)
        except KeyError:
            blockers.append(
                self._blocker(
                    "connection",
                    "The displayed AI account is no longer available.",
                    settings_target,
                    connection_id=connection_id,
                    model_id=raw_route["model_id"],
                    task_id=entry.get("task_id"),
                )
            )
            return {
                "kind": entry["kind"],
                "task_id": entry.get("task_id"),
                "role": entry.get("role"),
                "route": raw_route,
                "connection_revision": 0,
                "model": {},
                "readiness": "missing",
                "account_name": "Account unavailable",
                "provider": "unknown",
                "data_destination": "Unavailable",
                "billing": "unknown",
                "spending_detail": "The account is unavailable.",
            }, {"entry": entry, "connection": None, "model": None}

        model = next(
            (item for item in self.connections.models(connection_id) if item["model_id"] == raw_route["model_id"]),
            None,
        )
        if model is None:
            blockers.append(
                self._blocker(
                    "model_check",
                    "The displayed model is not in the saved account catalog.",
                    settings_target,
                    connection_id=connection_id,
                    model_id=raw_route["model_id"],
                    task_id=entry.get("task_id"),
                )
            )
            model = {}

        if connection.get("billing") in {"metered", "unknown"} and model:
            pricing = model.get("pricing")
            missing_price = not pricing
            missing_search_price = bool(raw_route["web_search"] and pricing and pricing.get("web_search_per_call") is None)
            if missing_price or missing_search_price:
                detail = (
                    "Add documented model input and output prices in the account's More options before starting Tender work."
                    if missing_price else
                    "Add the documented online-search price in the model's More options before starting this research route."
                )
                blockers.append(self._blocker(
                    "spending", detail, settings_target + f"&model={quote(raw_route['model_id'], safe='')}",
                    connection_id=connection_id, model_id=raw_route["model_id"], task_id=entry.get("task_id"),
                ))

        policy_versions = policy.get("_connection_versions", {})
        if policy_versions.get(connection_id) != connection["revision"]:
            changes.append(
                PlanReviewChange(
                    code="connection_changed",
                    detail="The AI account changed after its Tender permission was saved.",
                    before=policy_versions.get(connection_id),
                    after=connection["revision"],
                ).model_dump(mode="json")
            )
        if connection_id not in policy.get("allowed_connection_ids", []):
            blockers.append(
                self._blocker(
                    "configuration",
                    "This displayed route is not currently allowed to receive Tender content.",
                    policy_target,
                    connection_id=connection_id,
                    model_id=raw_route["model_id"],
                    task_id=entry.get("task_id"),
                )
            )

        try:
            self.policy.validate_route(raw_route, policy.get("allowed_connection_ids", []))
        except (KeyError, ValueError) as error:
            text = str(error)
            code = "model_check"
            target = settings_target
            if "approved" in text.lower() or "allowed" in text.lower() or "disabled" in text.lower():
                code = "configuration"
                target = policy_target
            blockers.append(
                self._blocker(
                    code,
                    text,
                    target,
                    connection_id=connection_id,
                    model_id=raw_route["model_id"],
                    task_id=entry.get("task_id"),
                )
            )

        readiness, _ = self._readiness(connection, raw_route["model_id"])
        if readiness != "ready":
            if connection.get("credential_state") == "missing" and is_subscription_profile(connection):
                code, target, detail = "sign_in", settings_target, "Sign in through the official AI client before approving this plan."
            elif connection.get("credential_state") == "missing":
                code, target, detail = "configuration", settings_target, "Enter the AI account credential before approving this plan."
            else:
                code, target, detail = "model_check", settings_target, "Check this AI model in Settings before approving this plan."
            blockers.append(
                self._blocker(
                    code,
                    detail,
                    target,
                    connection_id=connection_id,
                    model_id=raw_route["model_id"],
                    task_id=entry.get("task_id"),
                )
            )

        destination = connection.get("base_url") or f"Official {connection.get('provider_id', 'AI')} client"
        billing = connection.get("billing", "unknown")
        provider_extras = allows_extras(connection)
        if provider_extras:
            spending_detail = "Grok paid extras are enabled. Approving this review permits provider charges beyond the subscription allowance; Quantix cannot guarantee a Tender spending cap for these extras."
        elif billing == "metered":
            spending_detail = "Provider API usage is separately billed and uses the Tender spending allowance."
        elif billing == "unknown":
            spending_detail = "The provider price is not confirmed; review the Tender spending allowance before work."
        elif billing == "subscription":
            spending_detail = "The original client uses its subscription allowance; provider extras remain separately controlled."
        else:
            spending_detail = "This route has no recorded provider billing."
        return {
            "kind": entry["kind"],
            "task_id": entry.get("task_id"),
            "role": entry.get("role"),
            "route": raw_route,
            "connection_revision": connection["revision"],
            "model": model,
            "readiness": readiness,
            "account_name": connection["name"],
            "provider": connection["provider_id"],
            "data_destination": destination,
            "billing": billing,
            "provider_managed_extras": provider_extras,
            "spending_detail": spending_detail,
        }, {"entry": entry, "connection": connection, "model": model}

    @staticmethod
    def _artifact_option(item: dict) -> DelegationArtifactOption:
        return DelegationArtifactOption(
            artifact_id=item["id"],
            name=item["name"],
            display_name=item["name"],
            relative_path=item["relative_path"],
            version=item["version"],
            content_hash=item["content_hash"],
            status=item["status"],
        )

    def artifact_options(
        self,
        tender_id: str,
        plan_id: str,
        *,
        offset: int = 0,
        limit: int = _DELEGATION_ARTIFACT_PAGE_SIZE,
        query: str | None = None,
    ) -> DelegationArtifactOptionPage:
        """Return a bounded searchable page of every current source choice."""
        if type(offset) is not int or offset < 0:
            raise ValueError("The source choice offset must be a nonnegative integer.")
        if type(limit) is not int or not 1 <= limit <= 100:
            raise ValueError("The source choice page size must be between 1 and 100.")
        if query is not None and (not isinstance(query, str) or len(query) > 200):
            raise ValueError("The source choice search text must be 200 characters or fewer.")
        self.repo.get_plan(tender_id, plan_id)
        normalized_query = query.strip().casefold() if query else ""
        current = self.repo.list_artifacts(tender_id, current_only=True)
        if normalized_query:
            current = [
                item
                for item in current
                if normalized_query in item["name"].casefold()
                or normalized_query in item["relative_path"].casefold()
            ]
        page = current[offset : offset + limit]
        end = offset + len(page)
        return DelegationArtifactOptionPage(
            items=[self._artifact_option(item) for item in page],
            next_offset=end if end < len(current) else None,
            total=len(current),
        )

    def _delegation_options(
        self,
        tender_id: str,
        plan: dict,
        policy: dict,
        routes: list[dict],
    ) -> tuple[DelegationOptions, dict, list[dict]]:
        """Build the current typed choices and a stable candidate identity.

        This method only reads current Tender/catalog/account state.  It never
        materializes a proposal or a grant.  The returned internal maps keep
        the review builder from reconstructing authority from display text.
        """
        blockers: list[dict] = []
        current_artifacts = self.repo.list_artifacts(tender_id, current_only=True)
        all_artifact_options = [self._artifact_option(item) for item in current_artifacts]
        artifact_options = all_artifact_options[:_DELEGATION_ARTIFACT_PAGE_SIZE]

        capabilities = list(capability_catalog())
        from .code_runtime_scope import available_code_runtimes

        code_options = available_code_runtimes(self.repo)
        if len(capabilities) > 100:
            blockers.append(
                self._blocker(
                    "configuration",
                    "The installed Tender tool catalog is larger than the supported delegation selection.",
                    "/settings?section=ai",
                )
            )
            capabilities = capabilities[:100]

        route_by_id: dict[str, DelegationRouteOption] = {}
        for displayed in routes:
            if not displayed.get("model") or not displayed.get("connection_revision"):
                continue
            payload = {
                "id": "candidate",
                "route": displayed["route"],
                "connection_revision": displayed["connection_revision"],
                "model_revision": model_revision_fingerprint(displayed["model"]),
                "model": displayed["model"],
                "account_name": displayed["account_name"],
                "provider": displayed["provider"],
                "data_destination": displayed["data_destination"],
                "billing": displayed["billing"],
                "provider_managed_extras": displayed.get("provider_managed_extras", False),
                "readiness": displayed.get("readiness", "unknown"),
            }
            option_id = route_option_id(payload)
            payload["id"] = option_id
            option = DelegationRouteOption.model_validate(payload)
            route_by_id.setdefault(option.id, option)
        route_options = list(route_by_id.values())
        if len(route_options) > 20:
            blockers.append(
                self._blocker(
                    "configuration",
                    "The displayed AI route alternatives exceed the supported delegation selection. Reduce the route choices and review again.",
                    f"/tenders/{tender_id}/work?view=ai",
                )
            )
            route_options = route_options[:20]

        proposal = self.proposals.get(tender_id, plan["id"])
        artifact_json = [item.model_dump(mode="json") for item in all_artifact_options]
        candidate_payload = {
            "artifacts": artifact_json,
            "tools": [item.model_dump(mode="json") for item in capabilities],
            "route_options": [item.model_dump(mode="json") for item in route_options],
        }
        candidate_fingerprint = _review_fingerprint(candidate_payload)
        if code_options:
            candidate_payload["code_runtimes"] = [item.model_dump(mode="json") for item in code_options]
            candidate_fingerprint = _review_fingerprint(candidate_payload)
        options = DelegationOptions(
            artifacts=artifact_options,
            tools=capabilities,
            code_runtimes=code_options,
            route_options=route_options,
            supported_draft_outputs=sorted(_DELEGATION_OUTPUT_KINDS),
            selected=proposal,
        )
        return options, {
            "artifacts": {item.artifact_id: item for item in all_artifact_options},
            "tools": {item.id: item for item in capabilities},
            "routes": {item.id: item for item in route_options},
            "candidate_fingerprint": candidate_fingerprint,
            "proposal": proposal,
            "candidate_payload": candidate_payload,
            "policy": policy,
            "plan": plan,
            "tender_id": tender_id,
        }, blockers

    def _delegation_envelope(
        self,
        tender_id: str,
        plan: dict,
        policy: dict,
        state: dict,
        blockers: list[dict],
    ) -> DelegationEnvelope | None:
        """Resolve saved selections against the current candidate records."""
        proposal: DelegationProposal | None = state["proposal"]
        current_artifacts: dict[str, DelegationArtifactOption] = state["artifacts"]
        current_tools = state["tools"]
        current_routes = state["routes"]

        if len(current_artifacts) > 500 and (
            proposal is None or proposal.source_scope == "reviewed_tender"
        ):
            blockers.append(
                self._blocker(
                    "sources",
                    "The complete Tender source scope exceeds 500 artifacts. Choose selected sources from the source picker before delegating work.",
                    f"/tenders/{tender_id}/plans/{plan['id']}/review",
                )
            )
            return None

        if proposal is None:
            source_scope = "reviewed_tender"
            artifact_ids = list(current_artifacts)
            tool_ids = list(current_tools)
            native_tools = {}
            code_runtimes = []
            output_kinds = list(_DELEGATION_OUTPUT_KINDS)
            route_ids = list(current_routes)
            max_staff, max_assignments = 4, 12
            max_depth, max_concurrency = 2, 2
            max_requests = int(policy.get("max_requests") or 0)
            max_search_calls = min(
                10000,
                max_requests
                * max(
                    (item.route.max_search_calls for item in current_routes.values() if item.route.web_search),
                    default=0,
                ),
            )
        else:
            source_scope = proposal.source_scope
            artifact_ids = list(proposal.artifact_ids)
            tool_ids = list(proposal.tool_ids)
            native_tools = proposal.native_tools
            code_runtimes = proposal.code_runtimes
            output_kinds = list(proposal.allowed_draft_outputs)
            route_ids = list(proposal.route_option_ids)
            max_staff = proposal.max_staff
            max_assignments = proposal.max_assignments
            max_depth = proposal.max_depth
            max_concurrency = proposal.max_concurrency
            max_requests = proposal.max_requests
            max_search_calls = proposal.max_search_calls
            saved_candidate_fingerprint = self.proposals.candidate_fingerprint(tender_id, plan["id"])
            if saved_candidate_fingerprint and saved_candidate_fingerprint != state["candidate_fingerprint"]:
                # Specific selected-record checks below provide the actionable
                # source/tool/route message. This catches a versioned catalog
                # description change where an ID itself still exists.
                if not any(item["code"] == "configuration" and "catalog" in item["detail"] for item in blockers):
                    blockers.append(
                        self._blocker(
                            "configuration",
                            "The available delegation tools or AI route choices changed after these options were saved. Review the current choices again.",
                            f"/tenders/{tender_id}/work?view=ai",
                        )
                    )

        if source_scope == "selected_sources" and not artifact_ids:
            blockers.append(
                self._blocker(
                    "sources",
                    "Select at least one current Tender source before delegating work.",
                    f"/tenders/{tender_id}/documents",
                )
            )
        if source_scope == "reviewed_tender" and set(artifact_ids) != set(current_artifacts):
            blockers.append(
                self._blocker(
                    "sources",
                    "The saved delegation source scope no longer covers the current reviewed Tender sources. Review the source selection again.",
                    f"/tenders/{tender_id}/documents",
                )
            )
        selected_artifacts = []
        for identifier in artifact_ids:
            option = current_artifacts.get(identifier)
            if option is None:
                blockers.append(
                    self._blocker(
                        "sources",
                        "A saved delegation source is no longer a current Tender artifact. Review the source selection again.",
                        f"/tenders/{tender_id}/documents",
                    )
                )
                continue
            selected_artifacts.append(
                ArtifactBasis(
                    artifact_id=option.artifact_id,
                    version=option.version,
                    content_hash=option.content_hash,
                )
            )

        selected_tools = []
        for identifier in tool_ids:
            option = current_tools.get(identifier)
            if option is None:
                blockers.append(
                    self._blocker(
                        "configuration",
                        "A saved delegation capability is no longer in the current Tender tool catalog. Review the delegation options again.",
                        "/settings?section=ai",
                    )
                )
                continue
            selected_tools.append(option)

        valid_outputs = set(_DELEGATION_OUTPUT_KINDS)
        invalid_outputs = [item for item in output_kinds if item not in valid_outputs]
        if invalid_outputs:
            blockers.append(
                self._blocker(
                    "configuration",
                    f"The delegation output '{invalid_outputs[0]}' is not an available Tender Office draft output.",
                    f"/tenders/{tender_id}/work?view=ai",
                )
            )
            output_kinds = [item for item in output_kinds if item in valid_outputs]

        selected_routes = []
        for identifier in route_ids:
            option = current_routes.get(identifier)
            if option is None:
                blockers.append(
                    self._blocker(
                        "configuration",
                        "A saved delegation AI route is no longer one of the current displayed route choices. Review the route selection again.",
                        f"/tenders/{tender_id}/work?view=ai",
                    )
                )
                continue
            selected_routes.append(option)

        if not selected_routes:
            # DelegationEnvelope requires an actual route. Keep the displayed
            # review typed and actionable while avoiding a silent default route.
            return None
        if max_requests > int(policy.get("max_requests") or 0):
            blockers.append(
                self._blocker(
                    "spending",
                    "The delegation request allowance exceeds the Tender allowance.",
                    f"/tenders/{tender_id}/work?view=ai",
                )
            )
        required_search = max(
            (item.route.max_search_calls for item in selected_routes if item.route.web_search),
            default=0,
        )
        if max_search_calls < required_search:
            blockers.append(
                self._blocker(
                    "spending",
                    "The aggregate online-search allowance is below the selected route's search ceiling.",
                    f"/tenders/{tender_id}/work?view=ai",
                )
            )
        if not output_kinds:
            return None
        try:
            envelope = DelegationEnvelope(
                purpose=f"Coordinate the current Tender work plan: {plan['title']}",
                source_scope=source_scope,
                artifacts=selected_artifacts,
                tools=selected_tools,
                native_tools=native_tools,
                code_runtimes=code_runtimes,
                allowed_draft_outputs=output_kinds,
                route_options=selected_routes,
                max_staff=max_staff,
                max_assignments=max_assignments,
                max_depth=max_depth,
                max_concurrency=max_concurrency,
                max_requests=max_requests,
                max_search_calls=max_search_calls,
                run_budget_usd=policy.get("run_budget_usd"),
                tender_budget_usd=policy.get("tender_budget_usd"),
            )
            from .ai_native_tools import selection_for_route
            from .code_runtime_scope import validate_code_runtime

            if len({item.engine for item in envelope.code_runtimes}) != len(envelope.code_runtimes):
                raise ValueError("Select each code runtime once.")
            for runtime in envelope.code_runtimes:
                validate_code_runtime(self.repo, runtime)
            if set(envelope.native_tools) - {item.id for item in envelope.route_options}:
                raise ValueError("Native tools must name a selected reviewed AI route.")
            for option in envelope.route_options:
                selection_for_route(envelope, option, option.route.model_dump(mode="json"), option.model)
            return envelope
        except ValueError as error:
            blockers.append(
                self._blocker(
                    "configuration",
                    f"The delegation selections are outside the supported bounded office scope: {error}",
                    f"/tenders/{tender_id}/work?view=ai",
                )
            )
            return None

    def _approved_receipt_review(self, tender_id: str, plan_id: str) -> PlanReview | None:
        """Return the immutable displayed scope for an already approved plan."""
        with self.repo.db.connect() as conn:
            row = conn.execute(
                "SELECT review_json FROM plan_review_approvals WHERE tender_id=? AND plan_id=? ORDER BY created_at DESC LIMIT 1",
                (tender_id, plan_id),
            ).fetchone()
        if row is None:
            return None
        try:
            committed = PlanReview.model_validate_json(row["review_json"])
        except ValueError:
            # A malformed legacy receipt cannot become a new authority scope.
            return None
        fresh_scope_blocker = self._blocker(
            "plan",
            "This work plan is already approved. Request a new proposed work plan before changing its office scope.",
            f"/tenders/{tender_id}/work",
        )
        blockers = list(committed.blockers)
        if not any(item.code == "plan" for item in blockers):
            blockers.append(PlanReviewBlocker.model_validate(fresh_scope_blocker))
        if committed.delegation is None:
            summary = "This task plan is already approved. Request a new proposed work plan before using adaptive staffing."
        else:
            summary = (
                "This work plan is already approved with its committed delegation scope. "
                "The Tender Manager may assign or reuse generated colleagues within those reviewed limits."
            )
        return committed.model_copy(
            update={"ai_summary": summary, "blockers": blockers, "can_approve": False, "plan_status": "approved"}
        )

    def _superseded_plan_guidance(self, tender_id: str, plan_id: str) -> str:
        current = next(
            (
                item
                for item in self.repo.list_plans(tender_id)
                if item["id"] != plan_id and item["status"] in {"proposed", "approved"}
            ),
            None,
        )
        if current is not None:
            return (
                "This work plan was superseded. Its review is historical only. "
                f"Review the current Tender work plan (version {current['version']}) before continuing."
            )
        return (
            "This work plan was superseded. Its review is historical only. "
            "Request a new proposed work plan before continuing."
        )

    def _superseded_receipt_review(self, tender_id: str, plan_id: str) -> PlanReview | None:
        """Return a superseded plan's saved review as historical reference only."""
        with self.repo.db.connect() as conn:
            row = conn.execute(
                "SELECT review_json FROM plan_review_approvals WHERE tender_id=? AND plan_id=? ORDER BY created_at DESC LIMIT 1",
                (tender_id, plan_id),
            ).fetchone()
        if row is None:
            return None
        try:
            committed = PlanReview.model_validate_json(row["review_json"])
        except ValueError:
            # A malformed legacy receipt cannot become a new authority scope.
            return None
        guidance = self._superseded_plan_guidance(tender_id, plan_id)
        historical_detail = (
            f"{guidance} The saved delegation scope is historical reference only and cannot authorize new assignments."
        )
        # Any plan blocker saved with the old approval may describe the old
        # approval state. Replace it with the superseded-state direction.
        blockers = [item for item in committed.blockers if item.code != "plan"]
        blockers.append(
            PlanReviewBlocker.model_validate(
                self._blocker("plan", historical_detail, f"/tenders/{tender_id}/work")
            )
        )
        summary = (
            f"{guidance} The saved delegation scope is historical reference only and cannot authorize new assignments."
            if committed.delegation is not None
            else guidance
        )
        return committed.model_copy(
            update={"ai_summary": summary, "blockers": blockers, "can_approve": False, "plan_status": "superseded"}
        )

    def _approved_without_scope(self, tender: dict, plan: dict, *, superseded: bool = False) -> PlanReview:
        """Represent an approved or superseded plan with no delegation receipt."""
        policy = self.policy.get(tender["id"])
        scope = PlanReviewScope(
            title=plan["title"],
            plan_version=plan["version"],
            tender_revision=tender["revision"],
            source_ids=self._source_ids(plan),
            source_revision=tender["revision"],
        )
        tasks = [
            PlanReviewTask(
                id=task["id"],
                title=task["title"],
                description=task["description"],
                role=task["role"],
                status=task["status"],
                source_ids=task.get("source_ids", []),
                route=None,
            ).model_dump(mode="json")
            for task in plan["tasks"]
        ]
        snapshot = PlanReviewSnapshot(
            tender_revision=tender["revision"],
            source_revision=tender["revision"],
            policy_revision=policy.get("revision", 0),
            allowed_connection_ids=list(policy.get("allowed_connection_ids", [])),
            max_requests=policy.get("max_requests", 12),
            spent_usd=float(policy.get("spent_usd") or 0),
            reserved_usd=float(policy.get("reserved_usd") or 0),
            usage_fingerprint=_fingerprint([]),
            restore_reconciliation_required=bool(policy.get("restore_reconciliation_required")),
            spend_history_may_be_incomplete=bool(policy.get("spend_history_may_be_incomplete")),
        )
        blocker = self._blocker(
            "plan",
            (
                self._superseded_plan_guidance(tender["id"], plan["id"])
                if superseded
                else "This work plan is already approved. Request a new proposed work plan before using adaptive staffing."
            ),
            f"/tenders/{tender['id']}/work",
        )
        preliminary = {
            "tender_id": tender["id"],
            "plan_id": plan["id"],
            "plan_version": plan["version"],
            "plan_status": plan["status"],
            "scope": scope.model_dump(mode="json"),
            "tasks": tasks,
            "routes": [],
            "delegation": None,
            "delegation_proposal_version": 0,
            "delegation_options": None,
            "ai_summary": (
                self._superseded_plan_guidance(tender["id"], plan["id"])
                if superseded
                else "This task plan is already approved. Request a new proposed work plan before using adaptive staffing."
            ),
            "meaningful_changes": [],
            "blockers": [blocker],
            "snapshot": snapshot.model_dump(mode="json"),
            "can_approve": False,
        }
        return PlanReview(
            **preliminary,
            fingerprint=_review_fingerprint(preliminary),
            reviewed_at=now(),
        )

    def _build_review(self, tender_id: str, plan_id: str) -> PlanReview:
        tender = self.repo.get_tender(tender_id)
        plan = self.repo.get_plan(tender_id, plan_id)
        if plan["status"] == "approved":
            return self._approved_receipt_review(tender_id, plan_id) or self._approved_without_scope(tender, plan)
        if plan["status"] == "superseded":
            return self._superseded_receipt_review(tender_id, plan_id) or self._approved_without_scope(
                tender, plan, superseded=True
            )
        policy = self.policy.get(tender_id) | {"tender_id": tender_id}
        with self.repo.db.connect() as conn:
            row = conn.execute(
                "SELECT data_json FROM tender_ai_policy WHERE tender_id=?", (tender_id,)
            ).fetchone()
            if row:
                # Connection revisions are intentionally private on the
                # normal TenderAIRecord, but are part of this internal review
                # snapshot and therefore of its approval fingerprint.
                policy["_connection_versions"] = json.loads(row[0]).get("_connection_versions", {})
            team, saved = self._candidate_team(conn, tender_id, plan, policy)

        blockers: list[dict] = []
        changes: list[dict] = []
        source_ids = self._source_ids(plan)
        active_run_ids: list[str]
        with self.repo.db.connect() as conn:
            active_run_ids = self._active_runs(conn, tender_id)
            usage_rows = [
                {"id": row["id"], "data": json.loads(row["data_json"]), "created_at": row["created_at"]}
                for row in conn.execute(
                    "SELECT id,data_json,created_at FROM ai_usage WHERE tender_id=? ORDER BY created_at,id",
                    (tender_id,),
                )
            ] if conn.execute(
                "SELECT 1 FROM sqlite_master WHERE type='table' AND name='ai_usage'"
            ).fetchone() else []
            usage_fingerprint = _fingerprint(usage_rows)
            if plan["status"] != "proposed":
                blockers.append(
                    self._blocker(
                        "plan",
                        "Only the current proposed work plan can be approved and started.",
                        f"/tenders/{tender_id}/work",
                    )
                )
            if active_run_ids:
                blockers.append(
                    self._blocker(
                        "running_work",
                        "Finish or stop current Tender work before approving this plan.",
                        f"/tenders/{tender_id}/work",
                    )
                )
            basis = conn.execute(
                "SELECT revision FROM plan_basis WHERE tender_id=? AND plan_id=?",
                (tender_id, plan_id),
            ).fetchone()
            basis_revision = basis[0] if basis else None
            if basis_revision is None or basis_revision != tender["revision"]:
                blockers.append(
                    self._blocker(
                        "sources",
                        "The Tender sources changed since this plan was prepared. Request a revised plan.",
                        f"/tenders/{tender_id}/documents",
                    )
                )
            try:
                self.repo._check_sources(conn, tender_id, source_ids)
            except (KeyError, ValueError) as error:
                blockers.append(
                    self._blocker(
                        "sources",
                        str(error),
                        f"/tenders/{tender_id}/documents",
                    )
                )

        if not team.get("manager"):
            blockers.append(
                self._blocker(
                    "configuration",
                    "Choose a Tender Manager AI route before approving this plan.",
                    f"/tenders/{tender_id}/work?view=ai",
                )
            )
        if not policy.get("revision"):
            blockers.append(
                self._blocker(
                    "configuration",
                    "Approve AI routes and budgets for this Tender before approving its work plan.",
                    f"/tenders/{tender_id}/work?view=ai",
                )
            )
        if not saved:
            changes.append(
                PlanReviewChange(
                    code="legacy_team_review",
                    detail="This plan has no current saved AI team. The displayed account and task routes require explicit approval.",
                ).model_dump(mode="json")
            )
        elif team.get("policy_revision") != policy.get("revision"):
            changes.append(
                PlanReviewChange(
                    code="policy_changed",
                    detail="Tender AI permissions changed after this plan's team was displayed.",
                    before=team.get("policy_revision"),
                    after=policy.get("revision"),
                ).model_dump(mode="json")
            )

        entries = self._route_entries(team, plan)
        routes: list[dict] = []
        route_context: list[dict] = []
        for entry in entries:
            displayed, context = self._route_review(policy, entry, blockers, changes)
            routes.append(PlanReviewRoute.model_validate(displayed).model_dump(mode="json"))
            route_context.append(context)

        delegation_options, delegation_state, delegation_blockers = self._delegation_options(
            tender_id, plan, policy, routes
        )
        blockers.extend(delegation_blockers)
        delegation = self._delegation_envelope(
            tender_id, plan, policy, delegation_state, blockers
        )

        paid_routes = [
            item["connection"]
            for item in route_context
            if item.get("connection") and item["connection"].get("billing") in {"metered", "unknown"}
        ]
        paid_routes.extend(
            item["connection"]
            for item in route_context
            if item.get("connection")
            and item["connection"].get("protocol") == "grok_build"
            and item["connection"].get("settings", {}).get("allow_provider_managed_extras") is True
        )
        paid_exposure = bool(paid_routes)
        spent, reserved = float(policy.get("spent_usd") or 0), float(policy.get("reserved_usd") or 0)
        run_budget, tender_budget = policy.get("run_budget_usd"), policy.get("tender_budget_usd")
        if paid_exposure and (not run_budget or not tender_budget):
            blockers.append(
                self._blocker(
                    "spending",
                    "Set positive per-run and Tender spending allowances before starting separately billed AI work.",
                    f"/tenders/{tender_id}/work?view=ai",
                )
            )
        if paid_exposure and tender_budget is not None and spent + reserved >= float(tender_budget):
            blockers.append(
                self._blocker(
                    "spending",
                    "The Tender spending allowance is already used or reserved.",
                    f"/tenders/{tender_id}/work?view=ai",
                )
            )
        if paid_exposure and policy.get("restore_reconciliation_required"):
            blockers.append(
                self._blocker(
                    "spending",
                    "Review spending in the provider accounts after restoring this workspace before starting paid work.",
                    f"/tenders/{tender_id}/work?view=ai",
                )
            )
        # The current account flag is displayed permission to be granted by
        # the explicit combined approval, not a prerequisite grant. Reading
        # this review leaves every existing permission unchanged.
        displayed_accounts = {item["connection"]["id"]: item["connection"]
                              for item in route_context if item.get("connection")}
        candidate_extras = {identifier: connection["revision"]
                            for identifier, connection in displayed_accounts.items() if allows_extras(connection)}
        for identifier, connection in displayed_accounts.items():
            previous = policy.get("provider_managed_extras", {}).get(identifier)
            proposed = candidate_extras.get(identifier)
            if previous != proposed:
                changes.append(PlanReviewChange(
                    code="provider_extras_changed",
                    detail=(f"Approving this review permits Grok paid extras for {connection['name']} at its displayed account settings. Quantix cannot guarantee a Tender spending cap for these extras."
                            if proposed is not None else
                            f"Approving this review removes the previous Grok paid-extras permission for {connection['name']}."),
                    before={"connection_id": identifier, "revision": previous},
                    after={"connection_id": identifier, "revision": proposed},
                ).model_dump(mode="json"))

        if not entries and policy.get("manager"):
            changes.append(
                PlanReviewChange(
                    code="route_unavailable",
                    detail="The saved team did not retain a usable displayed route.",
                ).model_dump(mode="json")
            )

        task_routes: dict[str, dict] = {}
        for route in routes:
            if route["kind"] == "specialist" and route.get("task_id"):
                task_routes[route["task_id"]] = route["route"]
        review_tasks = [
            PlanReviewTask(
                id=task["id"],
                title=task["title"],
                description=task["description"],
                role=task["role"],
                status=task["status"],
                source_ids=task.get("source_ids", []),
                route=task_routes.get(task["id"]),
            ).model_dump(mode="json")
            for task in plan["tasks"]
        ]

        allowed_destinations = []
        for connection_id in policy.get("allowed_connection_ids", []):
            try:
                connection = self.connections.get(connection_id)
                destination = connection.get("base_url") or f"Official {connection.get('provider_id', 'AI')} client"
                billing = connection.get("billing", "unknown")
                allowed_destinations.append(
                    {
                        "connection_id": connection_id,
                        "name": connection["name"],
                        "provider": connection["provider_id"],
                        "data_destination": destination,
                        "billing": billing,
                        "spending_detail": (
                            "Grok paid extras are enabled; Quantix cannot guarantee a Tender spending cap for these extras."
                            if allows_extras(connection)
                            else "Provider API usage is separately billed and uses the Tender spending allowance."
                            if billing == "metered"
                            else "The original client uses its subscription allowance; provider extras remain separately controlled."
                            if billing == "subscription"
                            else "Review the provider price and Tender spending allowance before work."
                        ),
                        "revision": connection["revision"],
                        "provider_managed_extras": allows_extras(connection),
                    }
                )
            except KeyError:
                blockers.append(
                    self._blocker(
                        "connection",
                        "An AI account allowed for this Tender is no longer available.",
                        "/settings?section=accounts",
                        connection_id=connection_id,
                    )
                )
        if len(task_routes) != len(plan["tasks"]):
            blockers.append(
                self._blocker(
                    "configuration",
                    "Each work task needs a displayed specialist route before approval.",
                    f"/tenders/{tender_id}/work?view=ai",
                )
            )

        route_intents = [
            {
                "kind": route["kind"],
                "task_id": route.get("task_id"),
                "role": route.get("role"),
                "route": route["route"],
                "connection_revision": route["connection_revision"],
                "model": route["model"],
                "readiness": route["readiness"],
                "account_name": route["account_name"],
                "provider": route["provider"],
                "data_destination": route["data_destination"],
                "billing": route["billing"],
                "provider_managed_extras": route["provider_managed_extras"],
                "spending_detail": route["spending_detail"],
            }
            for route in routes
        ]
        connection_revisions = {
            route["route"]["connection_id"]: route["connection_revision"]
            for route in routes
            if route["connection_revision"]
        }
        model_revisions = {
            f"{route['route']['connection_id']}:{route['route']['model_id']}": _review_fingerprint(route["model"])
            for route in routes
            if route["model"]
        }
        snapshot = PlanReviewSnapshot(
            tender_revision=tender["revision"],
            source_revision=tender["revision"],
            policy_revision=policy.get("revision", 0),
            allowed_connection_ids=list(policy.get("allowed_connection_ids", [])),
            connection_revisions=connection_revisions,
            model_revisions=model_revisions,
            route_intents=route_intents,
            run_budget_usd=run_budget,
            tender_budget_usd=tender_budget,
            max_requests=policy.get("max_requests", 12),
            spent_usd=spent,
            reserved_usd=reserved,
            usage_fingerprint=usage_fingerprint,
            provider_managed_extras=candidate_extras,
            allowed_destinations=allowed_destinations,
            restore_reconciliation_required=bool(policy.get("restore_reconciliation_required")),
            spend_history_may_be_incomplete=bool(policy.get("spend_history_may_be_incomplete")),
            active_run_ids=active_run_ids,
        )
        manager = next((route for route in routes if route["kind"] == "manager"), None)
        manager_name = "the selected Tender Manager route"
        if manager and manager["model"]:
            manager_name = manager["model"].get("display_name") or manager["route"]["model_id"]
        ai_summary = (
            f"The Tender Manager uses {manager_name}. The Manager will assign or reuse generated colleagues within the displayed sources, tools and AI routes. The limits are {(delegation.max_staff if delegation else 4)} colleagues, {(delegation.max_assignments if delegation else 12)} assignments, delegation depth {(delegation.max_depth if delegation else 2)} and {(delegation.max_concurrency if delegation else 2)} simultaneous assignments. Shared client accounts may run one at a time."
            if manager
            else "No Tender Manager route is currently displayed. Choose one before approving this plan."
        )
        scope = PlanReviewScope(
            title=plan["title"],
            plan_version=plan["version"],
            tender_revision=tender["revision"],
            source_ids=source_ids,
            source_revision=tender["revision"],
        )
        blockers = list({(item["code"], item["detail"], item["repair_target"], item.get("task_id")): item for item in blockers}.values())
        changes = list({(item["code"], item["detail"]): item for item in changes}.values())
        preliminary = {
            "tender_id": tender_id,
            "plan_id": plan_id,
            "plan_version": plan["version"],
            "plan_status": plan["status"],
            "scope": scope.model_dump(mode="json"),
            "tasks": review_tasks,
            "routes": routes,
            "delegation": delegation.model_dump(mode="json") if delegation else None,
            "delegation_proposal_version": (
                delegation_state["proposal"].version if delegation_state["proposal"] else 0
            ),
            "delegation_options": delegation_options.model_dump(mode="json"),
            "ai_summary": ai_summary,
            "meaningful_changes": changes,
            "blockers": blockers,
            "snapshot": snapshot.model_dump(mode="json"),
            "can_approve": not blockers,
        }
        return PlanReview(
            **preliminary,
            fingerprint=_review_fingerprint(preliminary),
            reviewed_at=now(),
        )

    def review(self, tender_id: str, plan_id: str) -> dict:
        """Return a fresh review. This method creates no grants or decisions."""

        return self._build_review(tender_id, plan_id).model_dump(mode="json")

    def update_delegation(self, tender_id: str, plan_id: str, values: dict) -> dict:
        """Save engineer selections and return the typed CAS proposal.

        The candidate records are resolved immediately so an edit cannot save
        an unknown source, capability or route identifier.  This mutation
        creates no delegation grant and does not approve or queue the plan.
        """
        edit = values if isinstance(values, DelegationProposalEdit) else DelegationProposalEdit.model_validate(values)
        review = self._build_review(tender_id, plan_id)
        selected = review.delegation_options
        if selected is None:
            raise ValueError("Delegation choices are unavailable until the current Tender routes are reviewed.")
        current_artifact_options = [
            self._artifact_option(item)
            for item in self.repo.list_artifacts(tender_id, current_only=True)
        ]
        state = {
            "artifacts": {item.artifact_id: item for item in current_artifact_options},
            "tools": {item.id: item for item in selected.tools},
            "routes": {item.id: item for item in selected.route_options},
        }
        proposal = selected.selected
        if edit.expected_version != (proposal.version if proposal else 0):
            raise ValueError(
                "The delegation options changed since they were displayed. Refresh the plan review and try again."
            )
        candidate_payload = {
            "artifacts": [item.model_dump(mode="json") for item in current_artifact_options],
            "tools": [item.model_dump(mode="json") for item in selected.tools],
            "route_options": [item.model_dump(mode="json") for item in selected.route_options],
        }
        candidate_fingerprint = _review_fingerprint(candidate_payload)
        if selected.code_runtimes:
            candidate_payload["code_runtimes"] = [item.model_dump(mode="json") for item in selected.code_runtimes]
            candidate_fingerprint = _review_fingerprint(candidate_payload)
        if edit.source_scope == "selected_sources" and not edit.artifact_ids:
            raise ValueError("Select at least one current Tender source before delegating work.")
        if edit.source_scope == "reviewed_tender" and set(edit.artifact_ids) != set(state["artifacts"]):
            raise ValueError("Reviewed Tender scope must include the current source package, or choose selected sources.")
        unknown_artifacts = [item for item in edit.artifact_ids if item not in state["artifacts"]]
        if unknown_artifacts:
            raise ValueError("Choose current Tender source artifacts from the displayed delegation options.")
        unknown_tools = [item for item in edit.tool_ids if item not in state["tools"]]
        if unknown_tools:
            raise ValueError("Choose capabilities from the current Tender tool catalog.")
        unknown_outputs = [item for item in edit.allowed_draft_outputs if item not in _DELEGATION_OUTPUT_KINDS]
        if unknown_outputs:
            raise ValueError(f"The delegation output '{unknown_outputs[0]}' is unavailable.")
        unknown_routes = [item for item in edit.route_option_ids if item not in state["routes"]]
        if unknown_routes:
            raise ValueError("Choose an AI route from the current displayed delegation options.")
        if set(edit.native_tools) - set(edit.route_option_ids):
            raise ValueError("Native tools must name a selected AI route.")
        for choice in edit.native_tools.values():
            if set(choice.uploaded_artifact_ids) - set(edit.artifact_ids):
                raise ValueError("Hosted-code uploads must be selected original files in this work scope.")
        from .code_runtime_scope import validate_code_runtime

        if len({item.engine for item in edit.code_runtimes}) != len(edit.code_runtimes):
            raise ValueError("Select each code runtime once.")
        for runtime in edit.code_runtimes:
            validate_code_runtime(self.repo, runtime)
        return self.proposals.save(
            tender_id,
            plan_id,
            edit,
            candidate_fingerprint=candidate_fingerprint,
        ).model_dump(mode="json")

    @staticmethod
    def _work_intents_from_runs(plan: dict, queued: list[dict]) -> list[dict]:
        """Normalize exact queue records into immutable approval intents.

        The queue owns the kind and task identity.  In particular, a Manager
        root is represented by an explicit ``kind=manager`` record with a
        null task ID; matching its instruction to a task is never sufficient.
        """
        by_task = {task["id"]: task for task in plan["tasks"]}
        intents = []
        for value in queued:
            run = value.get("run", value)
            if not isinstance(run, dict) or not run.get("id"):
                raise ValueError("The plan queue returned a run without an identity.")
            supplied_kind = value.get("kind")
            run_kind = run.get("kind")
            if supplied_kind is not None and run_kind is not None and supplied_kind != run_kind:
                raise ValueError("The plan queue returned conflicting work intent kinds.")
            kind = run_kind if run_kind is not None else supplied_kind
            if kind not in {"task", "manager"}:
                raise ValueError("The plan queue returned an unsupported work intent kind.")
            task_id = value.get("task_id")
            if kind == "manager":
                if task_id is not None:
                    raise ValueError("A Manager work intent cannot carry a task ID.")
                task = None
            else:
                if not isinstance(task_id, str) or task_id not in by_task:
                    raise ValueError("The plan queue returned an unassigned task run.")
                task = by_task[task_id]
            run_tender_id = run.get("tender_id", plan["tender_id"])
            run_plan_id = run.get("plan_id", plan["id"])
            if run_tender_id != plan["tender_id"] or run_plan_id != plan["id"]:
                raise ValueError("The plan queue returned a run outside the approved Tender plan.")
            intents.append(
                WorkIntent(
                    id=value.get("id") or value.get("intent_id") or run.get("intent_id") or new_id(),
                    run_id=run["id"],
                    tender_id=run_tender_id,
                    plan_id=run_plan_id,
                    task_id=task_id,
                    kind=kind,
                    instruction=run.get("instruction", task["description"] if task else ""),
                    status="queued",
                    created_at=run.get("created_at", task["updated_at"] if task else now()),
                ).model_dump(mode="json")
            )
        task_intents = [intent for intent in intents if intent["kind"] == "task"]
        has_manager_root = any(intent["kind"] == "manager" for intent in intents)
        if not has_manager_root and {intent["task_id"] for intent in task_intents} != set(by_task):
            raise ValueError("The plan queue did not create one run for every approved task.")
        return intents

    def _manager_intent_from_runs(self, plan: dict, queued: list[dict]) -> list[dict]:
        """Validate the single orchestration root for a new delegated approval."""
        error = "The plan queue must return exactly one pinned Manager root."
        if not isinstance(queued, list) or len(queued) != 1 or not isinstance(queued[0], dict):
            raise ValueError(error)
        value = queued[0]
        run_payload = value.get("run", value)
        if not isinstance(run_payload, dict) or not isinstance(run_payload.get("id"), str):
            raise ValueError(error)
        if value.get("kind") not in {None, "manager"} or value.get("task_id") is not None:
            raise ValueError(error)
        try:
            run = self.repo.get_run(run_payload["id"])
        except KeyError as exc:
            raise ValueError(error) from exc
        if (
            run["tender_id"] != plan["tender_id"]
            or run["kind"] != "manager"
            or run["status"] != "queued"
        ):
            raise ValueError(error)
        from .manager_runtime import ManagerRunProfiles

        try:
            ManagerRunProfiles(self.repo).get(plan["tender_id"], run["id"])
        except (KeyError, ValueError) as exc:
            raise ValueError("The approved Manager root has no captured Manager profile pin.") from exc
        return [
            WorkIntent(
                id=value.get("id") or value.get("intent_id") or new_id(),
                run_id=run["id"],
                tender_id=run["tender_id"],
                plan_id=plan["id"],
                task_id=None,
                kind="manager",
                instruction=run["instruction"],
                status="queued",
                created_at=run["created_at"],
            ).model_dump(mode="json")
        ]

    def _idempotent_result(self, conn, tender_id: str, plan_id: str, fingerprint: str) -> dict | None:
        row = conn.execute(
            "SELECT * FROM plan_review_approvals WHERE tender_id=? AND plan_id=? ORDER BY created_at DESC LIMIT 1",
            (tender_id, plan_id),
        ).fetchone()
        if row is None:
            return None
        if row["fingerprint"] != fingerprint:
            raise ValueError("This plan was approved from a different review. Refresh the current plan review before trying again.")
        return PlanApprovalResult(
            plan=self.repo.get_plan(tender_id, plan_id),
            review=PlanReview.model_validate_json(row["review_json"]),
            work_intents=json.loads(row["work_intents_json"]),
        ).model_dump(mode="json")

    @staticmethod
    def _team_for_approval(review: PlanReview, *, policy_revision: int) -> dict:
        manager = next((item.route.model_dump(mode="json") for item in review.routes if item.kind == "manager"), None)
        specialists = [
            {
                "task_id": item.task_id,
                "role": item.role or "specialist",
                "route": item.route.model_dump(mode="json"),
                "rationale": "Displayed in the engineer's plan review.",
            }
            for item in review.routes
            if item.kind == "specialist" and item.task_id
        ]
        snapshots = [
            {
                "route": item.route.model_dump(mode="json"),
                "connection_revision": item.connection_revision,
                "model": item.model,
            }
            for item in review.routes
        ]
        return {
            "plan_id": review.plan_id,
            "policy_revision": policy_revision,
            "manager": manager,
            "specialists": specialists,
            "fallback_routes": [item.route.model_dump(mode="json") for item in review.routes if item.kind == "fallback"],
            "status": "approved",
            "warnings": [],
            "fingerprint": review.fingerprint,
            "_snapshots": snapshots,
        }

    def _renew_displayed_grants(self, conn, review: PlanReview) -> int:
        """Renew only account grants represented by the reviewed routes.

        A legacy policy may have an old private connection revision even while
        the account and current model check are valid. Explicit approval of
        this displayed review renews that route proof. Unreviewed allowed
        connections and extras remain untouched.
        """
        row = conn.execute(
            "SELECT revision,data_json FROM tender_ai_policy WHERE tender_id=?",
            (review.tender_id,),
        ).fetchone()
        if row is None:
            raise ValueError("Approve the Tender AI routes before approving this plan.")
        data = json.loads(row["data_json"])
        displayed_ids = {item.route.connection_id for item in review.routes}
        versions = dict(data.get("_connection_versions", {}))
        for connection_id in displayed_ids:
            revision = review.snapshot.connection_revisions.get(connection_id)
            if revision is not None:
                versions[connection_id] = revision
        changed = versions != data.get("_connection_versions", {})
        data["_connection_versions"] = versions

        extras = dict(data.get("provider_managed_extras", {}))
        reviewed_extras = review.snapshot.provider_managed_extras
        for connection_id in displayed_ids:
            connection = self.connections.get(connection_id)
            if allows_extras(connection):
                revision = reviewed_extras.get(connection_id)
                if revision != connection["revision"] or not all(
                    item.provider_managed_extras for item in review.routes if item.route.connection_id == connection_id
                ):
                    raise ValueError("Review the displayed Grok spending settings before approving this plan.")
                if extras.get(connection_id) != revision:
                    extras[connection_id] = revision
                    changed = True
            elif connection_id in extras:
                extras.pop(connection_id)
                changed = True
        if extras or "provider_managed_extras" in data:
            data["provider_managed_extras"] = extras
        if changed:
            revision = row["revision"] + 1
            conn.execute(
                "UPDATE tender_ai_policy SET revision=?,data_json=?,updated_at=? WHERE tender_id=?",
                (revision, dump(data), now(), review.tender_id),
            )
            return revision
        return row["revision"]

    def approve_and_start(self, tender_id: str, plan_id: str, values: dict) -> dict:
        request = PlanReviewApproval.model_validate(values)
        schedule: list[dict] | None = None
        with self.connections.authority_guard(), self.repo.atomic() as conn:
            existing = self._idempotent_result(conn, tender_id, plan_id, request.fingerprint)
            if existing is not None:
                return existing
            review = self._build_review(tender_id, plan_id)
            if request.fingerprint != review.fingerprint:
                raise ValueError("The plan review changed. Refresh the review before approving and starting tasks.")
            if not review.can_approve:
                raise ValueError("Resolve the displayed plan review blockers before approving and starting tasks.")
            if self.save_runs_in_transaction is None:
                raise ValueError("The Tender work queue is not ready. Restart Quantix and try approving the plan again.")

            policy_revision = self._renew_displayed_grants(conn, review)
            if review.delegation is None:
                raise ValueError("The reviewed plan has no complete delegation envelope. Refresh the plan review.")
            # L10's grant hook validates the exact typed envelope and writes
            # it under this same outer authority/SQLite transaction.  The
            # receipt is written below, before commit, so a queue failure
            # rolls back both records together.
            from .staff_routing import StaffRoutingService

            StaffRoutingService(self.repo).save_reviewed_grant(
                tender_id,
                plan_id,
                review.fingerprint,
                policy_revision,
                review.delegation,
            )
            team = self._team_for_approval(review, policy_revision=policy_revision)
            conn.execute(
                "INSERT INTO plan_ai_team(plan_id,tender_id,data_json) VALUES(?,?,?) ON CONFLICT(plan_id) DO UPDATE SET tender_id=excluded.tender_id,data_json=excluded.data_json",
                (plan_id, tender_id, dump(team)),
            )
            conn.execute(
                "INSERT INTO ai_team_history VALUES(?,?,?,?,?,?)",
                (new_id(), plan_id, tender_id, dump(team), request.rationale, now()),
            )
            plan = self.repo.approve_plan(tender_id, plan_id, request.rationale)
            stamp = now()
            queued = self.save_runs_in_transaction(tender_id, plan_id, plan, review)
            intents = self._manager_intent_from_runs(plan, queued)
            conn.execute(
                "INSERT INTO plan_review_approvals VALUES(?,?,?,?,?,?,?,?)",
                (new_id(), tender_id, plan_id, request.fingerprint, request.rationale, review.model_dump_json(), dump(intents), stamp),
            )
            result = PlanApprovalResult(plan=plan, review=review, work_intents=intents).model_dump(mode="json")
            schedule = intents

        # Scheduling is intentionally after the transaction commits. The
        # sibling job owner may supply this hook while keeping queue records
        # and task publication in its own bounded integration surface.
        if self.schedule_after_commit is not None and schedule is not None:
            self.schedule_after_commit(tender_id, plan_id, schedule)
        return result
