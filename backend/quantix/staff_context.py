"""Fresh, attributable execution contexts for generated Tender staff."""

from __future__ import annotations

import json
import re
from typing import Any

from .office_tools import OfficeContext, redact_prompt_data
from .staff_context_models import StaffAuthoredMaterial, StaffScope
from .staff_routing import StaffRoutingService
from .staff_store import StaffStore

_IDENTIFIER_RE = re.compile(r"^[A-Za-z0-9][A-Za-z0-9_.:/-]{0,159}$")
_ACTIVE_ASSIGNMENT_STATES = {"queued", "running", "waiting"}


_SCHEMA_STATEMENTS = (
    """
    CREATE TABLE IF NOT EXISTS staff_source_receipts (
        id TEXT PRIMARY KEY,
        tender_id TEXT NOT NULL REFERENCES tenders(id),
        actor_id TEXT NOT NULL REFERENCES office_staff(id),
        profile_id TEXT NOT NULL REFERENCES office_staff(id),
        profile_version INTEGER NOT NULL CHECK(profile_version >= 1),
        assignment_id TEXT NOT NULL,
        route_binding_id TEXT NOT NULL REFERENCES office_route_bindings(id),
        root_run_id TEXT NOT NULL REFERENCES runs(id),
        source_id TEXT NOT NULL REFERENCES evidence(id),
        artifact_id TEXT NOT NULL REFERENCES artifacts(id),
        artifact_version INTEGER NOT NULL CHECK(artifact_version >= 1),
        content_hash TEXT NOT NULL,
        locator TEXT NOT NULL,
        text_offset INTEGER,
        text_length INTEGER,
        page INTEGER,
        region_json TEXT,
        visible_cells_json TEXT NOT NULL DEFAULT '[]',
        method TEXT NOT NULL,
        created_at TEXT NOT NULL
    )
    """,
    """
    CREATE INDEX IF NOT EXISTS staff_source_receipts_assignment
    ON staff_source_receipts(tender_id, assignment_id, created_at, id)
    """,
)

_TRIGGER_STATEMENTS = (
    """
    CREATE TRIGGER IF NOT EXISTS staff_source_receipts_immutable_update
    BEFORE UPDATE ON staff_source_receipts
    BEGIN SELECT RAISE(ABORT, 'Staff source receipts are immutable'); END
    """,
    """
    CREATE TRIGGER IF NOT EXISTS staff_source_receipts_immutable_delete
    BEFORE DELETE ON staff_source_receipts
    BEGIN SELECT RAISE(ABORT, 'Staff source receipts are immutable'); END
    """,
)


def _identifier(value: str, label: str) -> str:
    if not isinstance(value, str) or _IDENTIFIER_RE.fullmatch(value) is None:
        raise ValueError(f"{label} is invalid.")
    return value


def _table_exists(conn, name: str) -> bool:
    return conn.execute(
        "SELECT 1 FROM sqlite_master WHERE type='table' AND name=?", (name,)
    ).fetchone() is not None


def _assignment_row(repo, assignment_id: str):
    with repo.db.connect() as conn:
        if not _table_exists(conn, "office_assignments"):
            raise KeyError("The staff assignment store is not available.")
        row = conn.execute(
            "SELECT * FROM office_assignments WHERE id=?", (assignment_id,)
        ).fetchone()
        if row is None:
            raise KeyError("This staff assignment could not be found.")
        required = {
            "id",
            "tender_id",
            "root_run_id",
            "staff_id",
            "staff_version",
            "work_order_id",
            "route_binding_id",
            "status",
        }
        missing = required.difference(row.keys())
        if missing:
            raise ValueError("The saved staff assignment is incomplete.")
        return dict(row)


def _ensure_receipt_schema(repo) -> None:
    with repo.atomic() as conn:
        for statement in _SCHEMA_STATEMENTS:
            conn.execute(statement)
        for statement in _TRIGGER_STATEMENTS:
            conn.execute(statement)


def _material(repo, *, tender_id: str, actor_id: str, assignment_id: str) -> StaffAuthoredMaterial:
    """Use actual authored and exactly addressed office exchanges."""
    from .office_messages import OfficeMessageService

    messages = OfficeMessageService(repo).for_assignment(tender_id, assignment_id)
    history = [redact_prompt_data(message.model_dump(mode="json")) for message in messages]
    notes = [message for message in history if message["kind"] == "note" and message["sender"]["id"] == actor_id]
    return StaffAuthoredMaterial(notes=notes, history=history)


class StaffContextService:
    """Build and validate a new context for one persisted assignment."""

    def __init__(self, repo):
        self.repo = repo
        _ensure_receipt_schema(repo)

    def build(self, binding_id: str, assignment_id: str) -> OfficeContext:
        _identifier(binding_id, "Route binding id")
        _identifier(assignment_id, "Assignment id")
        assignment = _assignment_row(self.repo, assignment_id)
        if assignment["status"] not in _ACTIVE_ASSIGNMENT_STATES:
            raise ValueError("The staff assignment is no longer active.")

        binding = StaffRoutingService(self.repo).validate_binding(
            assignment["tender_id"], binding_id
        )
        identities = (
            ("tender_id", binding.tender_id, assignment["tender_id"]),
            ("root run", binding.root_run_id, assignment["root_run_id"]),
            ("staff", binding.staff_id, assignment["staff_id"]),
            ("staff version", binding.staff_version, assignment["staff_version"]),
            ("work order", binding.work_order_id, assignment["work_order_id"]),
            ("route binding", binding.id, assignment["route_binding_id"]),
        )
        for label, expected, actual in identities:
            if expected != actual:
                raise ValueError(f"The saved staff assignment has a mismatched {label} identity.")

        staff = StaffStore(self.repo).get_staff(
            binding.tender_id, binding.staff_id, binding.staff_version
        )
        work_order = StaffStore(self.repo).get_work_order(
            binding.tender_id, binding.work_order_id
        )
        if work_order.staff_id != staff.id or work_order.staff_version != staff.version:
            raise ValueError("The staff work order does not match its profile version.")
        material = _material(
            self.repo, tender_id=binding.tender_id, actor_id=staff.id, assignment_id=assignment_id
        )
        scope = StaffScope(
            artifacts=list(binding.artifacts),
            tools=list(binding.tools),
            allowed_draft_outputs=list(binding.allowed_draft_outputs),
        )
        grant = StaffRoutingService(self.repo).approved_grant(
            binding.tender_id, binding.plan_id
        )
        if binding.artifacts != grant.envelope.artifacts:
            raise ValueError("The route binding's source scope does not match the approved delegation.")
        scope = scope.model_copy(update={"source_scope": grant.envelope.source_scope})
        context = OfficeContext(
            repo=self.repo,
            tender_id=binding.tender_id,
            run_id=binding.root_run_id,
            approved_scope=self.repo.approved_scope(binding.tender_id, binding.plan_id),
            actor_id=staff.id,
            staff_version=staff.version,
            assignment_id=assignment_id,
            route_binding_id=binding.id,
            staff_profile=staff,
            staff_work_order=work_order,
            route_binding=binding,
            staff_scope=scope,
            reviewed_artifacts={basis.artifact_id: basis for basis in binding.artifacts},
            reviewed_tools=list(binding.tools),
            notes=list(material.notes),
            history=list(material.history),
        )
        return context

    def list_receipts(self, tender_id: str, assignment_id: str, *, offset: int = 0, limit: int = 200):
        """List immutable receipts for one server-owned assignment."""

        _identifier(tender_id, "Tender id")
        _identifier(assignment_id, "Assignment id")
        if type(offset) is not int or offset < 0 or type(limit) is not int or not 1 <= limit <= 200:
            raise ValueError("Use a nonnegative offset and a receipt limit from 1 to 200.")
        with self.repo.db.connect() as conn:
            rows = conn.execute(
                """
                SELECT * FROM staff_source_receipts
                WHERE tender_id=? AND assignment_id=?
                ORDER BY created_at,id LIMIT ? OFFSET ?
                """,
                (tender_id, assignment_id, limit, offset),
            ).fetchall()
        return [self._receipt_model(row) for row in rows]

    @staticmethod
    def _receipt_model(row):
        from .staff_context_models import StaffSourceReceipt

        return StaffSourceReceipt(
                id=row["id"],
                tender_id=row["tender_id"],
                actor_id=row["actor_id"],
                profile_id=row["profile_id"],
                profile_version=row["profile_version"],
                assignment_id=row["assignment_id"],
                route_binding_id=row["route_binding_id"],
                root_run_id=row["root_run_id"],
                source_id=row["source_id"],
                artifact_id=row["artifact_id"],
                artifact_version=row["artifact_version"],
                content_hash=row["content_hash"],
                locator=row["locator"],
                text_offset=row["text_offset"],
                text_length=row["text_length"],
                page=row["page"],
                region=json.loads(row["region_json"]) if row["region_json"] else None,
                visible_cells=json.loads(row["visible_cells_json"] or "[]"),
                method=row["method"],
                created_at=row["created_at"],
            )

    def list_visual_receipts(
        self,
        tender_id: str,
        *,
        actor_id: str,
        profile_version: int,
        assignment_id: str,
        route_binding_id: str,
        root_run_id: str,
        artifact_id: str,
        page: int,
        batch_size: int = 200,
    ):
        """Page only exact actor/assignment visual proof rows to exhaustion."""

        for value, label in (
            (tender_id, "Tender id"),
            (actor_id, "Actor id"),
            (assignment_id, "Assignment id"),
            (route_binding_id, "Route binding id"),
            (root_run_id, "Root run id"),
            (artifact_id, "Artifact id"),
        ):
            _identifier(value, label)
        if type(profile_version) is not int or profile_version < 1:
            raise ValueError("The staff profile version is invalid.")
        if type(page) is not int or page < 1:
            raise ValueError("The visual proof page is invalid.")
        if type(batch_size) is not int or not 1 <= batch_size <= 200:
            raise ValueError("The visual proof batch size must be between 1 and 200.")
        result, offset = [], 0
        with self.repo.db.connect() as conn:
            initial_total = conn.execute(
                """
                SELECT COUNT(*) FROM staff_source_receipts
                WHERE tender_id=? AND actor_id=? AND profile_id=?
                  AND profile_version=? AND assignment_id=?
                  AND route_binding_id=? AND root_run_id=?
                  AND artifact_id=? AND page=? AND method='visual'
                """,
                (
                    tender_id,
                    actor_id,
                    actor_id,
                    profile_version,
                    assignment_id,
                    route_binding_id,
                    root_run_id,
                    artifact_id,
                    page,
                ),
            ).fetchone()[0]
        while offset < initial_total:
            with self.repo.db.connect() as conn:
                rows = conn.execute(
                    """
                    SELECT * FROM staff_source_receipts
                    WHERE tender_id=? AND actor_id=? AND profile_id=?
                      AND profile_version=? AND assignment_id=?
                      AND route_binding_id=? AND root_run_id=?
                      AND artifact_id=? AND page=? AND method='visual'
                    ORDER BY created_at,id LIMIT ? OFFSET ?
                    """,
                    (
                        tender_id,
                        actor_id,
                        actor_id,
                        profile_version,
                        assignment_id,
                        route_binding_id,
                        root_run_id,
                        artifact_id,
                        page,
                        min(batch_size, initial_total - offset),
                        offset,
                    ),
                ).fetchall()
            result.extend(self._receipt_model(row) for row in rows)
            if len(rows) < min(batch_size, initial_total - offset):
                return result
            offset += len(rows)
        return result


def build_staff_context(repo, binding_id: str, assignment_id: str) -> OfficeContext:
    """Build a fresh context from the server-owned assignment identity."""

    return StaffContextService(repo).build(binding_id, assignment_id)


def staff_prompt_context(context: OfficeContext) -> dict[str, Any]:
    """Return the bounded staff prompt material for this actor only."""

    if not context.is_staff:
        raise ValueError("A staff assignment context is required.")
    profile = context.staff_profile
    work_order = context.staff_work_order
    binding = context.route_binding
    staff_scope = context.staff_scope
    if profile is None or work_order is None or binding is None:
        raise ValueError("The staff assignment context is incomplete.")
    scope = {
        "artifacts": [basis.model_dump(mode="json") for basis in context.reviewed_artifacts.values()],
        "tools": [tool.id for tool in context.reviewed_tools],
        "tool_definitions": [tool.model_dump(mode="json") for tool in context.reviewed_tools],
        "allowed_draft_outputs": list(binding.allowed_draft_outputs),
        "source_scope": getattr(staff_scope, "source_scope", "selected_sources"),
        "plan_id": context.approved_scope.get("plan_id") if context.approved_scope else binding.plan_id,
        "route_binding_id": binding.id,
        "route_option_id": binding.route_option_id,
        "route": binding.route.model_dump(mode="json"),
        "connection_revision": binding.connection_revision,
        "model_revision": binding.model_revision,
        "root_run_id": context.run_id,
    }
    from .execution_context import identity_from_office_context
    from .staff_notebooks import StaffNotebookService

    reconstructed = StaffNotebookService(context.repo).reconstruct(
        identity_from_office_context(context),
        context.actor_id,
        query=str(work_order.work_order.brief if hasattr(work_order, "work_order") else ""),
        limit=20,
    )
    material = {
        "notes": list(context.notes),
        "history": list(context.history),
    }
    if reconstructed:
        material["notebook"] = [
            {
                "id": item.id,
                "kind": item.kind,
                "text": item.text,
                "refs": list(item.refs),
                "assignment_id": item.assignment_id,
                "stale": item.stale,
            }
            for item in reconstructed
        ]
    return redact_prompt_data(
        {
            "profile": profile.model_dump(mode="json"),
            "work_order": work_order.model_dump(mode="json"),
            "engineer_request": context.repo.get_run(context.run_id)["instruction"],
            "engineer_approved_scope": context.approved_scope,
            "scope": scope,
            "authored_material": material,
        }
    )


__all__ = ["StaffContextService", "build_staff_context", "staff_prompt_context"]
