"""Reviewed delegation grants and exact route bindings for generated staff."""

from __future__ import annotations

import hashlib
import json
import re
from typing import Any

from .agent_definitions import AgentDefinitionService
from .ai_connections import is_supported_profile
from .ai_generation_models import GenerationSettings
from .ai_policy import AIPolicyService
from .ai_readiness import require_ready
from .ai_subscription import allows_extras
from .db import new_id, now
from .manager_runtime import ManagerRunProfiles
from .staff_capabilities import (
    CapabilityUnavailable,
    get_capability,
    resolve_capabilities,
)
from .staff_models import ManagerCreationContext, OfficeConflict
from .staff_routing_models import (
    ArtifactBasis,
    DelegationEnvelope,
    DelegationGrant,
    DelegationRouteOption,
    RouteBinding,
    ToolCapability,
    canonical_json,
    envelope_fingerprint,
    model_revision_fingerprint,
    route_option_id,
)
from .staff_store import StaffStore

_IDENTIFIER_RE = re.compile(r"^[A-Za-z0-9][A-Za-z0-9_.:/-]{0,159}$")
_KEY_RE = re.compile(r"^[A-Za-z0-9._~-]{1,160}$")
_ACTIVE_RUN_STATUSES = {"queued", "running"}
_MISSING_RECEIPT = "The explicit delegation approval receipt is missing its reviewed envelope."
_IMAGE_REQUIRED_TOOLS = {"view_document_page"}


_SCHEMA_STATEMENTS = (
    """
    CREATE TABLE IF NOT EXISTS office_delegation_grants (
        id TEXT PRIMARY KEY,
        tender_id TEXT NOT NULL REFERENCES tenders(id),
        plan_id TEXT NOT NULL REFERENCES plans(id),
        review_fingerprint TEXT NOT NULL,
        policy_revision INTEGER NOT NULL CHECK (policy_revision >= 0),
        envelope_json TEXT NOT NULL,
        created_at TEXT NOT NULL,
        UNIQUE(tender_id, plan_id, review_fingerprint)
    )
    """,
    """
    CREATE INDEX IF NOT EXISTS office_delegation_grants_plan
    ON office_delegation_grants(tender_id, plan_id, created_at)
    """,
    """
    CREATE TABLE IF NOT EXISTS office_route_bindings (
        id TEXT PRIMARY KEY,
        tender_id TEXT NOT NULL REFERENCES tenders(id),
        grant_id TEXT NOT NULL REFERENCES office_delegation_grants(id),
        plan_id TEXT NOT NULL REFERENCES plans(id),
        root_run_id TEXT NOT NULL REFERENCES runs(id),
        staff_id TEXT NOT NULL REFERENCES office_staff(id),
        staff_version INTEGER NOT NULL CHECK (staff_version >= 1),
        definition_id TEXT,
        definition_version INTEGER CHECK (definition_version >= 1),
        work_order_id TEXT NOT NULL REFERENCES office_work_orders(id),
        route_option_id TEXT NOT NULL,
        route_json TEXT NOT NULL,
        connection_revision INTEGER NOT NULL CHECK (connection_revision >= 1),
        model_revision TEXT NOT NULL,
        artifacts_json TEXT NOT NULL,
        tools_json TEXT NOT NULL,
        allowed_draft_outputs_json TEXT NOT NULL,
        created_at TEXT NOT NULL
    )
    """,
    """
    CREATE INDEX IF NOT EXISTS office_route_bindings_root
    ON office_route_bindings(tender_id, root_run_id, created_at)
    """,
    """
    CREATE TABLE IF NOT EXISTS office_route_binding_receipts (
        id TEXT PRIMARY KEY,
        tender_id TEXT NOT NULL REFERENCES tenders(id),
        root_run_id TEXT NOT NULL REFERENCES runs(id),
        idempotency_key TEXT NOT NULL,
        payload_hash TEXT NOT NULL,
        binding_id TEXT NOT NULL REFERENCES office_route_bindings(id),
        created_at TEXT NOT NULL,
        UNIQUE(tender_id, root_run_id, idempotency_key)
    )
    """,
    """
    CREATE INDEX IF NOT EXISTS office_route_binding_receipts_root
    ON office_route_binding_receipts(tender_id, root_run_id)
    """,
    """
    CREATE TRIGGER IF NOT EXISTS office_delegation_grants_immutable
    BEFORE UPDATE ON office_delegation_grants
    BEGIN SELECT RAISE(ABORT, 'Delegation grants are immutable'); END
    """,
    """
    CREATE TRIGGER IF NOT EXISTS office_delegation_grants_no_delete
    BEFORE DELETE ON office_delegation_grants
    BEGIN SELECT RAISE(ABORT, 'Delegation grants are immutable'); END
    """,
    """
    CREATE TRIGGER IF NOT EXISTS office_route_bindings_immutable
    BEFORE UPDATE ON office_route_bindings
    BEGIN SELECT RAISE(ABORT, 'Route bindings are immutable'); END
    """,
    """
    CREATE TRIGGER IF NOT EXISTS office_route_bindings_no_delete
    BEFORE DELETE ON office_route_bindings
    BEGIN SELECT RAISE(ABORT, 'Route bindings are immutable'); END
    """,
    """
    CREATE TRIGGER IF NOT EXISTS office_route_binding_receipts_immutable
    BEFORE UPDATE ON office_route_binding_receipts
    BEGIN SELECT RAISE(ABORT, 'Route binding receipts are immutable'); END
    """,
    """
    CREATE TRIGGER IF NOT EXISTS office_route_binding_receipts_no_delete
    BEFORE DELETE ON office_route_binding_receipts
    BEGIN SELECT RAISE(ABORT, 'Route binding receipts are immutable'); END
    """,
)


def _identifier(value: str, label: str) -> str:
    if not isinstance(value, str) or _IDENTIFIER_RE.fullmatch(value) is None:
        raise ValueError(f"{label} is invalid.")
    return value


def _key(value: str) -> str:
    if not isinstance(value, str) or _KEY_RE.fullmatch(value) is None:
        raise ValueError("The idempotency key is invalid.")
    return value


def _payload_hash(value: dict[str, Any]) -> str:
    return hashlib.sha256(canonical_json(value).encode("utf-8")).hexdigest()


def _model(value: Any, cls):
    return value if isinstance(value, cls) else cls.model_validate(value)


class StaffRoutingService:
    """Persist and revalidate explicit delegation authority.

    The service does not execute a provider or choose a route. It only admits
    exact reviewed records and validates them at the synchronous dispatch
    boundary.
    """

    def __init__(self, repo):
        self.repo = repo
        self.policy = AIPolicyService(repo)
        self.staff = StaffStore(repo)
        self.definitions = AgentDefinitionService(repo)
        self.manager_runs = ManagerRunProfiles(repo)
        with repo.atomic() as conn:
            for statement in _SCHEMA_STATEMENTS:
                conn.execute(statement)
            columns = {row["name"] for row in conn.execute("PRAGMA table_info(office_route_bindings)")}
            if "code_runtimes_json" not in columns:
                conn.execute("ALTER TABLE office_route_bindings ADD COLUMN code_runtimes_json TEXT NOT NULL DEFAULT '[]'")
            if "definition_id" not in columns:
                conn.execute("ALTER TABLE office_route_bindings ADD COLUMN definition_id TEXT")
            if "definition_version" not in columns:
                conn.execute(
                    "ALTER TABLE office_route_bindings ADD COLUMN definition_version INTEGER"
                )

    @staticmethod
    def _parse_grant(row) -> DelegationGrant:
        return DelegationGrant(
            id=row["id"],
            tender_id=row["tender_id"],
            plan_id=row["plan_id"],
            review_fingerprint=row["review_fingerprint"],
            policy_revision=row["policy_revision"],
            envelope=json.loads(row["envelope_json"]),
            created_at=row["created_at"],
        )

    @staticmethod
    def _parse_binding(row) -> RouteBinding:
        return RouteBinding(
            id=row["id"],
            tender_id=row["tender_id"],
            grant_id=row["grant_id"],
            plan_id=row["plan_id"],
            root_run_id=row["root_run_id"],
            staff_id=row["staff_id"],
            staff_version=row["staff_version"],
            definition_id=row["definition_id"],
            definition_version=row["definition_version"],
            work_order_id=row["work_order_id"],
            route_option_id=row["route_option_id"],
            route=json.loads(row["route_json"]),
            connection_revision=row["connection_revision"],
            model_revision=row["model_revision"],
            artifacts=json.loads(row["artifacts_json"]),
            tools=json.loads(row["tools_json"]),
            code_runtimes=json.loads(row["code_runtimes_json"]),
            allowed_draft_outputs=json.loads(row["allowed_draft_outputs_json"]),
            created_at=row["created_at"],
        )

    @staticmethod
    def _plan(conn, tender_id: str, plan_id: str):
        row = conn.execute(
            "SELECT * FROM plans WHERE tender_id=? AND id=?", (tender_id, plan_id)
        ).fetchone()
        if row is None:
            raise KeyError("This work plan does not belong to the selected Tender.")
        return row

    @staticmethod
    def _tender(conn, tender_id: str):
        row = conn.execute("SELECT * FROM tenders WHERE id=?", (tender_id,)).fetchone()
        if row is None:
            raise KeyError("This item could not be found in the selected Tender.")
        return row

    @staticmethod
    def _current_plan(conn, tender_id: str, plan_id: str):
        plan = StaffRoutingService._plan(conn, tender_id, plan_id)
        tender = StaffRoutingService._tender(conn, tender_id)
        if plan["status"] != "approved":
            raise ValueError("Dynamic staff work requires an explicitly approved current plan.")
        basis = conn.execute(
            "SELECT revision FROM plan_basis WHERE tender_id=? AND plan_id=?",
            (tender_id, plan_id),
        ).fetchone()
        if basis is None or basis["revision"] != tender["revision"]:
            raise ValueError("The Tender sources changed after this plan review. Review the current sources again.")
        return plan, tender

    @staticmethod
    def _active_run(conn, tender_id: str, run_id: str):
        row = conn.execute(
            "SELECT * FROM runs WHERE tender_id=? AND id=?", (tender_id, run_id)
        ).fetchone()
        if row is None:
            raise KeyError("The delegated work root does not belong to this Tender.")
        if row["status"] not in _ACTIVE_RUN_STATUSES:
            raise OfficeConflict("The delegated work root is no longer active.")
        return row

    @staticmethod
    def _receipt_table_exists(conn) -> bool:
        return conn.execute(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name='plan_review_approvals'"
        ).fetchone() is not None

    @staticmethod
    def _receipt_envelope(conn, tender_id: str, plan_id: str, review_fingerprint: str):
        if not StaffRoutingService._receipt_table_exists(conn):
            raise ValueError(_MISSING_RECEIPT)
        row = conn.execute(
            """
            SELECT fingerprint,review_json
            FROM plan_review_approvals
            WHERE tender_id=? AND plan_id=? AND fingerprint=?
            ORDER BY created_at DESC LIMIT 1
            """,
            (tender_id, plan_id, review_fingerprint),
        ).fetchone()
        if row is None:
            raise ValueError("The explicit delegation approval receipt could not be found.")
        try:
            review = json.loads(row["review_json"])
            # Task 11 owns the PlanReview field and calls it ``delegation``.
            # Absence never grants legacy exact-task approvals any delegation
            # power.
            value = review.get("delegation")
            if value is None:
                raise ValueError
            return DelegationEnvelope.model_validate(value)
        except (TypeError, ValueError, json.JSONDecodeError) as error:
            raise ValueError(_MISSING_RECEIPT) from error

    @staticmethod
    def _receipt_has_root_intent(
        conn,
        tender_id: str,
        plan_id: str,
        review_fingerprint: str,
        root_run_id: str,
        root_kind: str,
    ) -> bool:
        if not StaffRoutingService._receipt_table_exists(conn):
            return False
        row = conn.execute(
            """
            SELECT work_intents_json
            FROM plan_review_approvals
            WHERE tender_id=? AND plan_id=? AND fingerprint=?
            ORDER BY created_at DESC LIMIT 1
            """,
            (tender_id, plan_id, review_fingerprint),
        ).fetchone()
        if row is None:
            return False
        try:
            intents = json.loads(row["work_intents_json"])
        except (TypeError, ValueError, json.JSONDecodeError):
            return False
        if not isinstance(intents, list):
            return False
        return any(
            isinstance(intent, dict)
            and intent.get("run_id") == root_run_id
            and intent.get("tender_id") == tender_id
            and intent.get("plan_id") == plan_id
            and intent.get("kind") in {"manager", "conversation", "task"}
            and intent.get("kind") == root_kind
            for intent in intents
        )

    def _assert_manager_pin(self, tender_id: str, root_run_id: str, expected_version: int | None = None) -> None:
        try:
            pinned = self.manager_runs.get(tender_id, root_run_id)
        except (KeyError, ValueError) as error:
            raise ValueError("The delegated work root has no immutable Tender Manager profile pin.") from error
        if expected_version is not None and pinned.version != expected_version:
            raise ValueError("The delegated work root uses a different Tender Manager profile version.")

    def _validate_root_origin(self, conn, tender_id: str, root_run_id: str, grant: DelegationGrant) -> None:
        run = self._active_run(conn, tender_id, root_run_id)
        if self._receipt_has_root_intent(
            conn, tender_id, grant.plan_id, grant.review_fingerprint, root_run_id, run["kind"]
        ):
            return
        if run["kind"] not in {"manager", "conversation"}:
            raise ValueError("The delegated root is not an approved Manager or conversation run.")
        engineer_message = conn.execute(
            "SELECT 1 FROM messages WHERE tender_id=? AND run_id=? AND role='engineer' AND created_at>? LIMIT 1",
            (tender_id, root_run_id, grant.created_at),
        ).fetchone()
        if engineer_message is None:
            from .office_resume import valid_resume_origin

            if not valid_resume_origin(conn, tender_id, run, grant):
                raise ValueError("The delegated root has no explicit engineer instruction or recorded Resume action.")

    @staticmethod
    def _verify_grant_receipt(conn, grant: DelegationGrant) -> None:
        receipt = StaffRoutingService._receipt_envelope(
            conn, grant.tender_id, grant.plan_id, grant.review_fingerprint
        )
        if envelope_fingerprint(receipt) != envelope_fingerprint(grant.envelope):
            raise ValueError("The saved plan review does not contain the exact delegation envelope.")

    @staticmethod
    def _artifact(conn, tender_id: str, basis: ArtifactBasis):
        row = conn.execute(
            "SELECT id,version,content_hash,is_current FROM artifacts WHERE tender_id=? AND id=?",
            (tender_id, basis.artifact_id),
        ).fetchone()
        if row is None:
            raise ValueError("A reviewed source artifact is missing from this Tender.")
        if not row["is_current"] or row["version"] != basis.version or row["content_hash"].lower() != basis.content_hash.lower():
            raise ValueError("A reviewed source artifact changed. Review the current Tender sources again.")
        return row

    @staticmethod
    def _evidence_artifact(conn, tender_id: str, evidence_id: str):
        row = conn.execute(
            """
            SELECT e.id AS evidence_id,e.is_current AS evidence_current,a.id,a.version,a.content_hash,a.is_current
            FROM evidence e JOIN artifacts a ON a.id=e.artifact_id
            WHERE a.tender_id=? AND e.id=?
            """,
            (tender_id, evidence_id),
        ).fetchone()
        if row is None:
            raise ValueError("A staff work-order source does not belong to this Tender.")
        if not row["is_current"]:
            raise ValueError("A staff work-order source was superseded. Review current evidence.")
        if row["evidence_current"] is not None and not row["evidence_current"]:
            raise ValueError(
                "A staff work-order source was superseded by a later extraction. Review current evidence."
            )
        return row

    @staticmethod
    def _validate_catalog(tools: list[ToolCapability]) -> list[ToolCapability]:
        result: list[ToolCapability] = []
        for tool in tools:
            try:
                current = get_capability(tool.id)
            except CapabilityUnavailable as error:
                raise ValueError(str(error)) from error
            if tool.version != current.version or tool.description != current.description or tool.read_only != current.read_only:
                raise ValueError(f"The reviewed capability '{tool.id}' changed. Review the delegation envelope again.")
            result.append(current)
        return result

    @staticmethod
    def _validate_envelope_shape(envelope: DelegationEnvelope) -> None:
        if not 1 <= envelope.max_depth <= 8:
            raise ValueError("The reviewed delegation depth is outside the supported range.")
        if not 1 <= envelope.max_concurrency <= 8:
            raise ValueError("The reviewed concurrency cap is outside the supported range.")
        StaffRoutingService._validate_catalog(envelope.tools)
        for option in envelope.route_options:
            if route_option_id(option) != option.id:
                raise ValueError("A reviewed route option has an invalid identity.")
            if model_revision_fingerprint(option.model) != option.model_revision.lower():
                raise ValueError("A reviewed route option has a stale model capability revision.")

    def _validate_budget(self, envelope: DelegationEnvelope, policy: dict, options: list[DelegationRouteOption]) -> None:
        if envelope.max_requests > int(policy.get("max_requests") or 0):
            raise ValueError("The delegation request allowance exceeds the Tender allowance.")
        if envelope.run_budget_usd is not None and (
            policy.get("run_budget_usd") is None or envelope.run_budget_usd > float(policy["run_budget_usd"])
        ):
            raise ValueError("The delegated run budget exceeds the Tender allowance.")
        if envelope.tender_budget_usd is not None and (
            policy.get("tender_budget_usd") is None or envelope.tender_budget_usd > float(policy["tender_budget_usd"])
        ):
            raise ValueError("The delegated Tender budget exceeds the Tender allowance.")
        paid = any(item.billing in {"metered", "unknown"} or item.provider_managed_extras for item in options)
        if paid and (envelope.run_budget_usd is None or envelope.tender_budget_usd is None):
            raise ValueError("Metered delegated work needs explicit run and Tender budgets.")
        needed_search = max(
            (item.route.max_search_calls for item in options if item.route.web_search), default=0
        )
        if envelope.max_search_calls < needed_search:
            raise ValueError("The aggregate search allowance is below the selected route requirement.")

    def _validate_option(
        self,
        option: DelegationRouteOption,
        policy: dict,
        *,
        require_readiness: bool = True,
        required_tools: set[str] | None = None,
    ):
        try:
            route = self.policy.validate_route(option.route.model_dump(mode="json"), policy["allowed_connection_ids"])
            connection = self.policy.connections.get(route["connection_id"])
        except (KeyError, ValueError) as error:
            raise ValueError(str(error)) from error
        if not is_supported_profile(connection):
            raise ValueError("The selected AI account is not supported for Tender execution.")
        if connection["revision"] != option.connection_revision:
            raise ValueError("The selected AI account changed. Review the delegation route again.")
        model = next((item for item in self.policy.connections.models(connection["id"]) if item["model_id"] == route["model_id"]), None)
        if model is None:
            raise ValueError("The selected model is no longer available. Review the delegation route again.")
        if model.get("model_id") != route["model_id"]:
            raise ValueError("The reviewed model does not match the selected route.")
        if model_revision_fingerprint(model) != option.model_revision.lower():
            raise ValueError("The selected model capabilities changed. Review the delegation route again.")
        if required_tools and _IMAGE_REQUIRED_TOOLS.intersection(required_tools) and model.get("capabilities", {}).get("images") is not True:
            raise ValueError("The selected model does not have the image capability required by the requested tool.")
        if connection["billing"] in {"metered", "unknown"}:
            pricing = model.get("pricing")
            if not pricing or (route["web_search"] and pricing.get("web_search_per_call") is None):
                raise ValueError("The selected paid model has incomplete pricing. Review its documented prices before dispatch.")
        destination = connection.get("base_url") or f"Official {connection.get('provider_id', 'AI')} client"
        if (
            option.account_name != connection["name"]
            or option.provider != connection["provider_id"]
            or option.data_destination != destination
            or option.billing != connection["billing"]
        ):
            raise ValueError("The reviewed route account or destination changed. Review the delegation route again.")
        if option.readiness != "ready":
            raise ValueError("The selected model was not ready in the reviewed plan.")
        if require_readiness:
            try:
                require_ready(self.repo, connection, route["model_id"])
            except (KeyError, ValueError) as error:
                raise ValueError(str(error)) from error
        if allows_extras(connection):
            if not option.provider_managed_extras or policy.get("provider_managed_extras", {}).get(connection["id"]) != connection["revision"]:
                raise ValueError("Provider-managed extras are not covered by this Tender's reviewed authority.")
        elif option.provider_managed_extras:
            raise ValueError("The route records provider-managed extras for an account that does not support them.")
        return route, model

    def _validate_envelope_current(self, conn, tender_id: str, envelope: DelegationEnvelope, policy: dict) -> None:
        self._validate_envelope_shape(envelope)
        from .ai_native_tools import selection_for_route
        from .code_runtime_scope import validate_code_runtime

        if len({item.engine for item in envelope.code_runtimes}) != len(envelope.code_runtimes):
            raise ValueError("Select each code runtime once.")
        for runtime in envelope.code_runtimes:
            validate_code_runtime(self.repo, runtime)

        if set(envelope.native_tools) - {item.id for item in envelope.route_options}:
            raise ValueError("Native tools require an exact reviewed AI route.")
        for basis in envelope.artifacts:
            self._artifact(conn, tender_id, basis)
        for option in envelope.route_options:
            route, model = self._validate_option(option, policy)
            selection_for_route(envelope, option, route, model)
        self._validate_budget(envelope, policy, envelope.route_options)

    def _approved_grant_in_conn(self, conn, tender_id: str, plan_id: str) -> DelegationGrant:
        row = conn.execute(
            "SELECT * FROM office_delegation_grants WHERE tender_id=? AND plan_id=? ORDER BY created_at DESC,id DESC LIMIT 1",
            (tender_id, plan_id),
        ).fetchone()
        if row is None:
            raise ValueError("No explicit delegation grant exists for this approved work plan.")
        grant = self._parse_grant(row)
        self._current_plan(conn, tender_id, plan_id)
        self._verify_grant_receipt(conn, grant)
        return grant

    def save_reviewed_grant(
        self,
        tender_id: str,
        plan_id: str,
        review_fingerprint: str,
        policy_revision: int,
        envelope: DelegationEnvelope | dict,
    ) -> DelegationGrant:
        """Save a grant during plan approval's existing outer transaction.

        The approval receipt can be inserted later in that same transaction;
        dispatch methods require it after commit.
        """

        _identifier(tender_id, "Tender id")
        _identifier(plan_id, "Plan id")
        if not isinstance(review_fingerprint, str) or re.fullmatch(r"[0-9a-fA-F]{64}", review_fingerprint) is None:
            raise ValueError("The plan review fingerprint is invalid.")
        if type(policy_revision) is not int or policy_revision < 0:
            raise ValueError("The reviewed policy revision is invalid.")
        parsed = _model(envelope, DelegationEnvelope)
        with self.policy.connections.authority_guard(), self.repo.atomic() as conn:
            self._plan(conn, tender_id, plan_id)
            policy = self.policy.get(tender_id)
            if policy["revision"] != policy_revision:
                raise ValueError("The Tender AI permissions changed during plan approval. Review the delegation again.")
            self._validate_envelope_current(conn, tender_id, parsed, policy)
            existing = conn.execute(
                "SELECT * FROM office_delegation_grants WHERE tender_id=? AND plan_id=? AND review_fingerprint=?",
                (tender_id, plan_id, review_fingerprint.lower()),
            ).fetchone()
            if existing is not None:
                old = self._parse_grant(existing)
                if old.policy_revision != policy_revision or envelope_fingerprint(old.envelope) != envelope_fingerprint(parsed):
                    raise OfficeConflict("This plan review fingerprint is already bound to a different delegation envelope.")
                return old
            stamp = now()
            grant = DelegationGrant(
                id=new_id(),
                tender_id=tender_id,
                plan_id=plan_id,
                review_fingerprint=review_fingerprint.lower(),
                policy_revision=policy_revision,
                envelope=parsed,
                created_at=stamp,
            )
            conn.execute(
                "INSERT INTO office_delegation_grants(id,tender_id,plan_id,review_fingerprint,policy_revision,envelope_json,created_at) VALUES(?,?,?,?,?,?,?)",
                (grant.id, tender_id, plan_id, grant.review_fingerprint, policy_revision, canonical_json(parsed), stamp),
            )
            return grant

    def approved_grant(self, tender_id: str, plan_id: str) -> DelegationGrant:
        _identifier(tender_id, "Tender id")
        _identifier(plan_id, "Plan id")
        with self.policy.connections.authority_guard(), self.repo.atomic() as conn:
            return self._approved_grant_in_conn(conn, tender_id, plan_id)

    def _validate_root_in_conn(self, conn, tender_id: str, root_run_id: str, plan_id: str):
        """Revalidate one approved delegated root in the caller's transaction.

        This method intentionally only reads authority.  It never creates a
        grant or renews a route, and it is kept separate from ``bind`` and
        ``validate_binding`` so budget admission can perform its scope check
        and usage reservation under one authority lock/SQLite transaction.
        """

        run = self._active_run(conn, tender_id, root_run_id)
        self._current_plan(conn, tender_id, plan_id)
        grant = self._approved_grant_in_conn(conn, tender_id, plan_id)
        if grant.plan_id != plan_id:
            raise ValueError("The delegated work root is outside its approved work plan.")
        self._validate_root_origin(conn, tender_id, root_run_id, grant)
        self._assert_manager_pin(tender_id, root_run_id)
        policy = self.policy.get(tender_id)
        if grant.policy_revision != policy["revision"]:
            raise ValueError("The Tender AI permissions changed. Review the delegation before dispatch.")
        self._validate_envelope_current(conn, tender_id, grant.envelope, policy)
        if policy.get("restore_reconciliation_required"):
            paid = False
            for option in grant.envelope.route_options:
                connection = self.policy.connections.get(option.route.connection_id)
                if connection["billing"] in {"metered", "unknown"} or allows_extras(connection):
                    paid = True
                    break
            if paid:
                raise ValueError("Review spending after restoring this workspace before continuing paid AI work.")
        return grant, policy, run

    def validate_root(self, tender_id: str, root_run_id: str, plan_id: str) -> DelegationGrant:
        """Validate an active, explicitly approved Manager root.

        A root is valid only when its immutable plan-review receipt, Manager
        profile pin, current Tender sources, route/model readiness and current
        AI policy all still match.  This is a read-only validation; it cannot
        establish a new spending grant from a raw run.
        """

        _identifier(tender_id, "Tender id")
        _identifier(root_run_id, "Root run id")
        _identifier(plan_id, "Plan id")
        with self.policy.connections.authority_guard(), self.repo.atomic() as conn:
            grant, _policy, _run = self._validate_root_in_conn(conn, tender_id, root_run_id, plan_id)
            return grant

    def _validate_context(self, conn, context: ManagerCreationContext):
        if not isinstance(context, ManagerCreationContext):
            raise TypeError("ManagerCreationContext is required for staff route binding.")
        _identifier(context.tender_id, "Tender id")
        _identifier(context.run_id, "Root run id")
        _identifier(context.scope_id, "Plan scope id")
        if type(context.manager_profile_version) is not int or context.manager_profile_version < 1:
            raise ValueError("The Manager profile version is invalid.")
        self._active_run(conn, context.tender_id, context.run_id)
        self._current_plan(conn, context.tender_id, context.scope_id)
        self._assert_manager_pin(
            context.tender_id, context.run_id, context.manager_profile_version
        )
        try:
            self.staff.manager_profiles.profile_version(context.manager_profile_version)
        except KeyError as error:
            raise ValueError("The Manager profile version could not be found.") from error

    def _validate_sources(self, conn, tender_id: str, source_ids: list[str], envelope: DelegationEnvelope):
        allowed = {item.artifact_id: item for item in envelope.artifacts}
        for source_id in source_ids:
            source = self._evidence_artifact(conn, tender_id, source_id)
            basis = allowed.get(source["id"])
            if basis is None:
                raise ValueError("A staff work-order source is outside the reviewed delegation scope.")
            if source["version"] != basis.version or source["content_hash"].lower() != basis.content_hash.lower():
                raise ValueError("A staff work-order source changed from the reviewed delegation scope.")

    def _validate_definition_preferences(self, profile, route) -> None:
        """Require non-neutral library preferences in the reviewed route.

        The approved route remains the execution record. This check never
        merges or changes it after the engineer's review.
        """

        if profile.definition_id is None:
            return
        if profile.definition_version is None:
            raise ValueError(
                "The saved professional definition reference is incomplete."
            )
        from .ai_models import AIRoute

        route = AIRoute.model_validate(route)
        definition = self.definitions.get(
            profile.definition_id, profile.definition_version
        )
        requested = definition.generation_settings
        neutral = GenerationSettings()
        mismatches: list[str] = []
        for field, label in (
            ("temperature", "temperature"),
            ("top_p", "sampling probability"),
            ("reasoning", "thinking level"),
        ):
            value = getattr(requested, field)
            if value is not None and value != getattr(route, field):
                mismatches.append(label)
        if (
            requested.max_output_tokens != neutral.max_output_tokens
            and requested.max_output_tokens != route.max_output_tokens
        ):
            mismatches.append("output token limit")
        if (
            requested.output_mode != neutral.output_mode
            and requested.output_mode != route.output_mode
        ):
            mismatches.append("structured response method")

        requested_native = set(requested.native_tools)
        route_native = set(route.native_tools)
        if route.web_search:
            route_native.add("web_search")
        if not requested_native.issubset(route_native):
            mismatches.append("native tools")
        if "web_search" in requested_native and (
            requested.max_search_calls != route.max_search_calls
        ):
            mismatches.append("search call limit")
        if requested_native and (
            requested.max_native_tool_calls != route.max_native_tool_calls
        ):
            mismatches.append("native tool call limit")
        if mismatches:
            fields = ", ".join(dict.fromkeys(mismatches))
            raise ValueError(
                "The selected route does not match the definition preferences "
                f"for {fields}. Update the Tender AI route and review the delegation again."
            )

    def _binding_receipt(self, conn, tender_id: str, root_run_id: str, key: str):
        return conn.execute(
            "SELECT * FROM office_route_binding_receipts WHERE tender_id=? AND root_run_id=? AND idempotency_key=?",
            (tender_id, root_run_id, key),
        ).fetchone()

    def _binding_payload(self, context: ManagerCreationContext, staff_id: str, work_order_id: str, route_option_id_value: str):
        return {
            "tender_id": context.tender_id,
            "root_run_id": context.run_id,
            "manager_profile_version": context.manager_profile_version,
            "scope_id": context.scope_id,
            "staff_id": staff_id,
            "work_order_id": work_order_id,
            "route_option_id": route_option_id_value,
        }

    def bind(
        self,
        context: ManagerCreationContext,
        staff_id: str,
        work_order_id: str,
        route_option_id: str,
        idempotency_key: str,
    ) -> RouteBinding:
        """Bind one generated profile/work order to one reviewed route option."""

        if not isinstance(context, ManagerCreationContext):
            raise TypeError("ManagerCreationContext is required for staff route binding.")
        _identifier(staff_id, "Staff id")
        _identifier(work_order_id, "Work-order id")
        _identifier(route_option_id, "Route option id")
        key = _key(idempotency_key)
        payload = self._binding_payload(context, staff_id, work_order_id, route_option_id)
        payload_hash = _payload_hash(payload)
        with self.policy.connections.authority_guard(), self.repo.atomic() as conn:
            replay = self._binding_receipt(conn, context.tender_id, context.run_id, key)
            if replay is not None:
                if replay["payload_hash"] != payload_hash:
                    raise OfficeConflict("This binding idempotency key was already used for different work.")
                self._validate_context(conn, context)
                grant = self._approved_grant_in_conn(conn, context.tender_id, context.scope_id)
                self._validate_root_origin(conn, context.tender_id, context.run_id, grant)
                policy = self.policy.get(context.tender_id)
                if grant.policy_revision != policy["revision"]:
                    raise ValueError("The Tender AI permissions changed. Review the delegation before dispatch.")
                self._validate_envelope_current(conn, context.tender_id, grant.envelope, policy)
                row = conn.execute("SELECT * FROM office_route_bindings WHERE id=?", (replay["binding_id"],)).fetchone()
                if row is None:
                    raise RuntimeError("The binding receipt points to a missing immutable binding.")
                return self._parse_binding(row)

            self._validate_context(conn, context)
            grant = self._approved_grant_in_conn(conn, context.tender_id, context.scope_id)
            self._validate_root_origin(conn, context.tender_id, context.run_id, grant)
            policy = self.policy.get(context.tender_id)
            if grant.policy_revision != policy["revision"]:
                raise ValueError("The Tender AI permissions changed. Review the delegation before dispatch.")
            self._validate_envelope_current(conn, context.tender_id, grant.envelope, policy)
            from .staff_lifecycle import ensure_staff_can_admit_work
            ensure_staff_can_admit_work(conn, context.tender_id, staff_id)
            profile = self.staff.get_staff(context.tender_id, staff_id)
            work_order = self.staff.get_work_order(context.tender_id, work_order_id)
            if work_order.staff_id != staff_id:
                raise ValueError("The work order belongs to another generated staff profile.")
            profile = self.staff.get_staff(context.tender_id, staff_id, work_order.staff_version)
            if work_order.scope_id == "":
                raise ValueError("The generated work order has no creation scope.")
            requested = resolve_capabilities(list(profile.requested_tool_ids))
            granted = {tool.id: tool for tool in grant.envelope.tools}
            for tool in requested:
                if tool.id not in granted:
                    raise ValueError(f"The requested capability '{tool.id}' is outside the reviewed delegation.")
            resolved_tools = [granted[tool.id] for tool in requested]
            self._validate_sources(conn, context.tender_id, work_order.work_order.source_ids, grant.envelope)
            option = next((item for item in grant.envelope.route_options if item.id == route_option_id), None)
            if option is None:
                raise ValueError("Choose one of the exact route alternatives displayed in the approved review.")
            route, _model = self._validate_option(
                option, policy, required_tools={tool.id for tool in resolved_tools}
            )
            self._validate_definition_preferences(profile, route)
            staff_count = conn.execute(
                "SELECT COUNT(DISTINCT staff_id) AS count FROM office_route_bindings WHERE tender_id=? AND root_run_id=?",
                (context.tender_id, context.run_id),
            ).fetchone()["count"]
            assignment_count = conn.execute(
                "SELECT COUNT(*) AS count FROM office_route_bindings WHERE tender_id=? AND root_run_id=?",
                (context.tender_id, context.run_id),
            ).fetchone()["count"]
            staff_already_bound = conn.execute(
                "SELECT 1 FROM office_route_bindings WHERE tender_id=? AND root_run_id=? AND staff_id=?",
                (context.tender_id, context.run_id, staff_id),
            ).fetchone() is not None
            if staff_count >= grant.envelope.max_staff and not staff_already_bound:
                raise ValueError("The delegation has reached its maximum staff count.")
            if assignment_count >= grant.envelope.max_assignments:
                raise ValueError("The delegation has reached its maximum assignment count.")
            stamp = now()
            binding = RouteBinding(
                id=new_id(),
                tender_id=context.tender_id,
                grant_id=grant.id,
                plan_id=context.scope_id,
                root_run_id=context.run_id,
                staff_id=staff_id,
                staff_version=work_order.staff_version,
                definition_id=profile.definition_id,
                definition_version=profile.definition_version,
                work_order_id=work_order_id,
                route_option_id=option.id,
                route=route,
                connection_revision=option.connection_revision,
                model_revision=option.model_revision,
                artifacts=grant.envelope.artifacts,
                tools=resolved_tools,
                code_runtimes=grant.envelope.code_runtimes,
                allowed_draft_outputs=grant.envelope.allowed_draft_outputs,
                created_at=stamp,
            )
            conn.execute(
                """
                INSERT INTO office_route_bindings(
                    id,tender_id,grant_id,plan_id,root_run_id,staff_id,staff_version,
                    definition_id,definition_version,work_order_id,route_option_id,route_json,
                    connection_revision,model_revision,
                    artifacts_json,tools_json,code_runtimes_json,allowed_draft_outputs_json,created_at
                ) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)
                """,
                (
                    binding.id,
                    binding.tender_id,
                    binding.grant_id,
                    binding.plan_id,
                    binding.root_run_id,
                    binding.staff_id,
                    binding.staff_version,
                    binding.definition_id,
                    binding.definition_version,
                    binding.work_order_id,
                    binding.route_option_id,
                    canonical_json(binding.route),
                    binding.connection_revision,
                    binding.model_revision,
                    canonical_json(binding.artifacts),
                    canonical_json(binding.tools),
                    canonical_json(binding.code_runtimes),
                    canonical_json(binding.allowed_draft_outputs),
                    stamp,
                ),
            )
            conn.execute(
                "INSERT INTO office_route_binding_receipts(id,tender_id,root_run_id,idempotency_key,payload_hash,binding_id,created_at) VALUES(?,?,?,?,?,?,?)",
                (new_id(), context.tender_id, context.run_id, key, payload_hash, binding.id, stamp),
            )
            return binding

    def validate_binding(self, tender_id: str, binding_id: str) -> RouteBinding:
        """Revalidate all authority before dispatch; this method makes no provider call."""

        _identifier(tender_id, "Tender id")
        _identifier(binding_id, "Binding id")
        with self.policy.connections.authority_guard(), self.repo.atomic() as conn:
            row = conn.execute(
                "SELECT * FROM office_route_bindings WHERE tender_id=? AND id=?",
                (tender_id, binding_id),
            ).fetchone()
            if row is None:
                raise KeyError("This route binding does not belong to the selected Tender.")
            binding = self._parse_binding(row)
            self._active_run(conn, tender_id, binding.root_run_id)
            grant_row = conn.execute(
                "SELECT * FROM office_delegation_grants WHERE tender_id=? AND id=?",
                (tender_id, binding.grant_id),
            ).fetchone()
            if grant_row is None:
                raise ValueError("The route binding's delegation grant is missing.")
            grant = self._parse_grant(grant_row)
            if grant.plan_id != binding.plan_id:
                raise ValueError("The route binding is outside its approved plan scope.")
            self._current_plan(conn, tender_id, binding.plan_id)
            self._verify_grant_receipt(conn, grant)
            self._validate_root_origin(conn, tender_id, binding.root_run_id, grant)
            policy = self.policy.get(tender_id)
            if grant.policy_revision != policy["revision"]:
                raise ValueError("The Tender AI permissions changed. Review the delegation before dispatch.")
            self._validate_envelope_current(conn, tender_id, grant.envelope, policy)
            option = next((item for item in grant.envelope.route_options if item.id == binding.route_option_id), None)
            if option is None or binding.route != option.route:
                raise ValueError("The route binding no longer matches an approved route option.")
            if binding.connection_revision != option.connection_revision or binding.model_revision != option.model_revision:
                raise ValueError("The route binding's model or account revision is stale.")
            self._validate_catalog(binding.tools)
            if binding.artifacts != grant.envelope.artifacts:
                raise ValueError("The route binding's artifact scope changed.")
            if binding.allowed_draft_outputs != grant.envelope.allowed_draft_outputs:
                raise ValueError("The route binding's draft authority changed.")
            for basis in binding.artifacts:
                self._artifact(conn, tender_id, basis)
            profile = self.staff.get_staff(tender_id, binding.staff_id, binding.staff_version)
            work_order = self.staff.get_work_order(tender_id, binding.work_order_id)
            if work_order.staff_id != binding.staff_id or work_order.staff_version != binding.staff_version:
                raise ValueError("The route binding's generated work order changed.")
            if profile.version != binding.staff_version:
                raise ValueError("The route binding's generated profile version is unavailable.")
            if (
                binding.definition_id != profile.definition_id
                or binding.definition_version != profile.definition_version
            ):
                raise ValueError(
                    "The route binding's professional definition reference changed."
                )
            self._validate_definition_preferences(profile, binding.route)
            self._assert_manager_pin(tender_id, binding.root_run_id)
            if binding.code_runtimes != grant.envelope.code_runtimes:
                raise ValueError("The staff code runtimes no longer match their exact reviewed scope.")
            requested = resolve_capabilities(list(profile.requested_tool_ids))
            self._validate_option(
                option, policy, required_tools={tool.id for tool in requested}
            )
            if [tool.id for tool in binding.tools] != [tool.id for tool in requested]:
                raise ValueError("The route binding's capability scope does not match the fixed profile.")
            envelope_tools = {tool.id: tool for tool in grant.envelope.tools}
            for tool in binding.tools:
                approved = envelope_tools.get(tool.id)
                if approved is None or tool != approved:
                    raise ValueError("The route binding's capability is outside the approved envelope.")
            if any(tool.id not in envelope_tools for tool in requested):
                raise ValueError("The generated profile requests a capability outside the route binding.")
            self._validate_sources(conn, tender_id, work_order.work_order.source_ids, grant.envelope)
            return binding

    def bind_child(
        self,
        parent,
        work_order_id: str,
        route_option_id: str | None,
        capability_ids: list[str],
        idempotency_key: str,
    ) -> RouteBinding:
        """Bind one descendant assignment inside its parent's reviewed grant.

        The child inherits the same Tender, root run, plan, account/model
        route and source scope; requested capabilities must be a subset of the
        parent's pinned tools, and depth is enforced against the reviewed
        envelope. A child never widens authority: another colleague, route or
        source scope stays the Manager's explicit bind action.
        """

        if getattr(parent, "is_staff", False) is not True:
            raise ValueError("Only an active staff assignment can request a child.")
        tender_id = parent.tender_id
        _identifier(work_order_id, "Work-order id")
        key = _key(idempotency_key)
        requested = list(dict.fromkeys(capability_ids or []))
        with self.policy.connections.authority_guard(), self.repo.atomic() as conn:
            parent_binding = self.validate_binding(tender_id, parent.route_binding_id)
            if (
                parent_binding.staff_id != parent.actor_id
                or parent_binding.staff_version != parent.staff_version
            ):
                raise OfficeConflict("The parent staff identity does not match its route binding.")
            assignment = conn.execute(
                "SELECT * FROM office_assignments WHERE tender_id=? AND id=?",
                (tender_id, parent.assignment_id),
            ).fetchone()
            if assignment is None or assignment["route_binding_id"] != parent_binding.id:
                raise OfficeConflict("The parent assignment does not match its route binding.")
            if assignment["status"] not in {"queued", "running", "waiting"}:
                raise OfficeConflict("Request a child only while the parent assignment is still active.")
            grant = self._approved_grant_in_conn(conn, tender_id, parent_binding.plan_id)
            policy = self.policy.get(tender_id)
            if grant.policy_revision != policy["revision"]:
                raise ValueError("The Tender AI permissions changed. Review the delegation before dispatch.")
            self._validate_envelope_current(conn, tender_id, grant.envelope, policy)
            depth = int(assignment["depth"] or 0) + 1
            if depth > grant.envelope.max_depth:
                raise ValueError(
                    f"Depth {depth} is outside the reviewed cap of {grant.envelope.max_depth}."
                )
            from .staff_lifecycle import ensure_staff_can_admit_work
            ensure_staff_can_admit_work(conn, tender_id, parent_binding.staff_id)
            work_order = self.staff.get_work_order(tender_id, work_order_id)
            if work_order.staff_id != parent_binding.staff_id:
                raise ValueError("A child assignment stays with its parent colleague.")
            profile = self.staff.get_staff(tender_id, parent_binding.staff_id, work_order.staff_version)
            if profile.version != work_order.staff_version:
                raise ValueError("The child work order targets a superseded profile version.")
            parent_tools = {tool.id: tool for tool in parent_binding.tools}
            unknown = [item for item in requested if item not in parent_tools]
            if unknown:
                raise ValueError(
                    f"The requested capability '{unknown[0]}' is outside the parent assignment scope."
                )
            resolved_tools = [parent_tools[item] for item in (requested or list(parent_tools))]
            option_id = route_option_id or parent_binding.route_option_id
            if option_id != parent_binding.route_option_id:
                raise ValueError("A child keeps its parent route. Ask the Manager to re-route work.")
            option = next((item for item in grant.envelope.route_options if item.id == option_id), None)
            if option is None:
                raise ValueError("Choose one of the exact route alternatives displayed in the approved review.")
            route, _model = self._validate_option(
                option, policy, required_tools={tool.id for tool in resolved_tools}
            )
            self._validate_definition_preferences(profile, route)
            assignment_count = conn.execute(
                "SELECT COUNT(*) AS count FROM office_route_bindings WHERE tender_id=? AND root_run_id=?",
                (tender_id, parent_binding.root_run_id),
            ).fetchone()["count"]
            if assignment_count >= grant.envelope.max_assignments:
                raise ValueError("The delegation has reached its maximum assignment count.")
            self._validate_sources(conn, tender_id, work_order.work_order.source_ids, grant.envelope)
            payload_hash = _payload_hash(
                {
                    "tender_id": tender_id,
                    "root_run_id": parent_binding.root_run_id,
                    "staff_id": parent_binding.staff_id,
                    "work_order_id": work_order_id,
                    "route_option_id": option_id,
                    "tools": [tool.id for tool in resolved_tools],
                    "parent_assignment_id": assignment["id"],
                }
            )
            replay = self._binding_receipt(conn, tender_id, parent_binding.root_run_id, key)
            if replay is not None:
                if replay["payload_hash"] != payload_hash:
                    raise OfficeConflict("This binding idempotency key was already used for different work.")
                row = conn.execute(
                    "SELECT * FROM office_route_bindings WHERE id=?", (replay["binding_id"],)
                ).fetchone()
                if row is None:
                    raise RuntimeError("The binding receipt points to a missing immutable binding.")
                return self._parse_binding(row)
            stamp = now()
            binding = RouteBinding(
                id=new_id(),
                tender_id=tender_id,
                grant_id=grant.id,
                plan_id=parent_binding.plan_id,
                root_run_id=parent_binding.root_run_id,
                staff_id=parent_binding.staff_id,
                staff_version=work_order.staff_version,
                definition_id=profile.definition_id,
                definition_version=profile.definition_version,
                work_order_id=work_order_id,
                route_option_id=option.id,
                route=route,
                connection_revision=option.connection_revision,
                model_revision=option.model_revision,
                artifacts=parent_binding.artifacts,
                tools=resolved_tools,
                code_runtimes=parent_binding.code_runtimes,
                allowed_draft_outputs=parent_binding.allowed_draft_outputs,
                created_at=stamp,
            )
            conn.execute(
                """
                INSERT INTO office_route_bindings(
                    id,tender_id,grant_id,plan_id,root_run_id,staff_id,staff_version,
                    definition_id,definition_version,work_order_id,route_option_id,route_json,
                    connection_revision,model_revision,
                    artifacts_json,tools_json,code_runtimes_json,allowed_draft_outputs_json,created_at
                ) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)
                """,
                (
                    binding.id,
                    binding.tender_id,
                    binding.grant_id,
                    binding.plan_id,
                    binding.root_run_id,
                    binding.staff_id,
                    binding.staff_version,
                    binding.definition_id,
                    binding.definition_version,
                    binding.work_order_id,
                    binding.route_option_id,
                    canonical_json(binding.route),
                    binding.connection_revision,
                    binding.model_revision,
                    canonical_json(binding.artifacts),
                    canonical_json(binding.tools),
                    canonical_json(binding.code_runtimes),
                    canonical_json(binding.allowed_draft_outputs),
                    stamp,
                ),
            )
            conn.execute(
                "INSERT INTO office_route_binding_receipts(id,tender_id,root_run_id,idempotency_key,payload_hash,binding_id,created_at) VALUES(?,?,?,?,?,?,?)",
                (new_id(), tender_id, parent_binding.root_run_id, key, payload_hash, binding.id, stamp),
            )
            return binding
