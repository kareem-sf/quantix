"""Persistent, Tender-scoped staff notebooks with bounded reconstruction."""

from __future__ import annotations

import base64
import json
import sqlite3
from typing import Any

from .db import dump, new_id, now
from .execution_context import OfficeExecutionIdentity
from .staff_notebook_models import (
    NotebookEntry,
    NotebookEntryDraft,
    NotebookPage,
    NotebookQuery,
)
from .staff_routing import StaffRoutingService
from .staff_store import StaffStore

_SCHEMA = (
    """
    CREATE TABLE IF NOT EXISTS office_staff_notebook (
        id TEXT PRIMARY KEY,
        tender_id TEXT NOT NULL REFERENCES tenders(id),
        staff_id TEXT NOT NULL REFERENCES office_staff(id),
        assignment_id TEXT,
        kind TEXT NOT NULL,
        text TEXT NOT NULL,
        refs_json TEXT NOT NULL,
        applicability TEXT NOT NULL,
        supersedes_id TEXT,
        current INTEGER NOT NULL DEFAULT 1,
        created_at TEXT NOT NULL,
        actor_id TEXT,
        profile_version INTEGER,
        route_binding_id TEXT,
        root_run_id TEXT,
        source_scope TEXT NOT NULL DEFAULT 'unbound'
    )
    """,
    """
    CREATE INDEX IF NOT EXISTS office_staff_notebook_staff
    ON office_staff_notebook(tender_id, staff_id, created_at, id)
    """,
)


def _cursor(created_at: str, identifier: str) -> str:
    return base64.urlsafe_b64encode(f"{created_at}\n{identifier}".encode()).decode()


def _decode_cursor(cursor: str) -> tuple[str, str]:
    try:
        created_at, identifier = base64.urlsafe_b64decode(cursor.encode()).decode().split("\n", 1)
    except (ValueError, UnicodeDecodeError) as error:
        raise ValueError("The notebook cursor is invalid.") from error
    if not created_at or not identifier:
        raise ValueError("The notebook cursor is invalid.")
    return created_at, identifier


def _reference_parts(value: Any) -> tuple[str, str]:
    if not isinstance(value, str) or not value.strip():
        raise ValueError("A notebook reference must be a nonblank identifier.")
    value = value.strip()
    if ":" not in value:
        return "source", value
    kind, identifier = value.split(":", 1)
    if kind not in {"source", "output", "staff_result"}:
        raise ValueError(f"The notebook reference type '{kind}' is unsupported.")
    if not identifier:
        raise ValueError(f"The notebook {kind} reference is invalid.")
    return kind, identifier


def _scope_allows(binding, artifact_id: str, version: int, content_hash: str) -> bool:
    return any(
        basis.artifact_id == artifact_id
        and basis.version == version
        and basis.content_hash.lower() == content_hash.lower()
        for basis in binding.artifacts
    )


class StaffNotebookService:
    def __init__(self, repo):
        self.repo = repo
        self.store = StaffStore(repo)
        with repo.atomic() as conn:
            for statement in _SCHEMA:
                conn.execute(statement)
            columns = {
                row["name"] for row in conn.execute("PRAGMA table_info(office_staff_notebook)")
            }
            for name, definition in (
                ("actor_id", "TEXT"),
                ("profile_version", "INTEGER"),
                ("route_binding_id", "TEXT"),
                ("root_run_id", "TEXT"),
                ("source_scope", "TEXT NOT NULL DEFAULT 'unbound'"),
            ):
                if name not in columns:
                    conn.execute(f"ALTER TABLE office_staff_notebook ADD COLUMN {name} {definition}")

    def _tender(self, ctx: OfficeExecutionIdentity) -> str:
        if not ctx.tender_id:
            raise ValueError("Notebook work requires a selected Tender.")
        return ctx.tender_id

    def _assignment_scope(
        self,
        ctx: OfficeExecutionIdentity,
        staff_id: str,
        assignment_id: str | None,
    ) -> tuple[dict[str, Any] | None, Any | None, str]:
        """Resolve server-owned assignment and reviewed binding lineage."""

        if assignment_id is None:
            if ctx.actor_kind == "staff":
                raise ValueError("Staff notebook work requires its originating assignment.")
            return None, None, "unbound"
        with self.repo.db.connect() as conn:
            row = conn.execute(
                "SELECT * FROM office_assignments WHERE tender_id=? AND id=?",
                (ctx.tender_id, assignment_id),
            ).fetchone()
        if row is None:
            raise KeyError("This staff assignment could not be found in the selected Tender.")
        assignment = dict(row)
        if assignment["staff_id"] != staff_id:
            raise ValueError("The notebook assignment belongs to another colleague.")
        if ctx.assignment_id is not None and ctx.assignment_id != assignment_id:
            raise ValueError("The notebook assignment does not match the active staff context.")
        if ctx.actor_kind == "staff":
            if assignment["status"] not in {"queued", "running", "waiting"}:
                raise ValueError("The staff assignment is no longer active.")
            if ctx.root_run_id != assignment["root_run_id"]:
                raise ValueError("The staff assignment identity is no longer active.")
            if ctx.actor_id != staff_id:
                raise ValueError("A colleague can only use their own notebook assignment.")
            if ctx.profile_version != assignment["staff_version"]:
                raise ValueError("The notebook staff profile does not match the assignment.")
            if ctx.route_binding_id != assignment["route_binding_id"]:
                raise ValueError("The notebook route binding does not match the assignment.")
        routing = StaffRoutingService(self.repo)
        binding = routing.validate_binding(ctx.tender_id, assignment["route_binding_id"])
        for label, expected, actual in (
            ("Tender", assignment["tender_id"], binding.tender_id),
            ("root run", assignment["root_run_id"], binding.root_run_id),
            ("staff", assignment["staff_id"], binding.staff_id),
            ("staff version", assignment["staff_version"], binding.staff_version),
            ("work order", assignment["work_order_id"], binding.work_order_id),
            ("route binding", assignment["route_binding_id"], binding.id),
        ):
            if expected != actual:
                raise ValueError(f"The notebook assignment has a mismatched {label} identity.")
        grant = routing.approved_grant(ctx.tender_id, binding.plan_id)
        return assignment, binding, grant.envelope.source_scope

    def _validate_reference(
        self,
        conn,
        tender_id: str,
        value: str,
        *,
        assignment: dict[str, Any] | None,
        binding,
    ) -> None:
        kind, identifier = _reference_parts(value)
        if kind == "source":
            row = conn.execute(
                """
                SELECT e.id AS source_id,a.id AS artifact_id,a.version,a.content_hash,
                       a.is_current,e.locator
                FROM evidence e JOIN artifacts a ON a.id=e.artifact_id
                WHERE a.tender_id=? AND e.id=?
                """,
                (tender_id, identifier),
            ).fetchone()
            if row is None:
                raise ValueError("The notebook source reference does not belong to this Tender.")
            if not row["is_current"]:
                raise ValueError("The notebook source reference is no longer current.")
            if binding is not None and not _scope_allows(
                binding, row["artifact_id"], row["version"], row["content_hash"]
            ):
                raise ValueError("The notebook source reference is outside the reviewed delegation scope.")
            return
        if kind == "staff_result":
            try:
                row = conn.execute(
                    "SELECT assignment_id,payload_json FROM office_staff_results WHERE tender_id=? AND id=?",
                    (tender_id, identifier),
                ).fetchone()
            except sqlite3.OperationalError as error:
                raise ValueError("The notebook staff-result reference is not available in this Tender.") from error
            if row is None:
                raise ValueError("The notebook staff-result reference does not belong to this Tender.")
            if assignment is not None and row["assignment_id"] != assignment["id"]:
                raise ValueError("The notebook staff-result reference is outside the active assignment scope.")
            from .staff_assignments import StaffAssignmentService

            results = StaffAssignmentService(self.repo)
            try:
                result = results.get_result(tender_id, identifier)
                linked_assignment = results.get(tender_id, result.assignment_id)
            except (KeyError, ValueError) as error:
                raise ValueError("The saved notebook staff-result reference is invalid.") from error
            if (
                result.id != identifier
                or result.tender_id != tender_id
                or row["assignment_id"] != result.assignment_id
                or linked_assignment.result_id != result.id
                or linked_assignment.staff_id != result.staff_id
                or linked_assignment.staff_version != result.staff_version
                or linked_assignment.work_order_id != result.work_order_id
                or linked_assignment.route_binding_id != result.route_binding_id
                or result.currentness != "current"
            ):
                raise ValueError("The notebook staff-result source basis is no longer current.")
            return
        try:
            row = conn.execute(
                "SELECT record_json FROM generated_outputs WHERE tender_id=? AND id=?",
                (tender_id, identifier),
            ).fetchone()
        except sqlite3.OperationalError as error:
            raise ValueError("The notebook output reference is not available in this Tender.") from error
        if row is None:
            raise ValueError("The notebook output reference does not belong to this Tender.")
        try:
            payload = json.loads(row["record_json"])
        except (TypeError, ValueError, json.JSONDecodeError) as error:
            raise ValueError("The saved notebook output reference is invalid.") from error
        if not isinstance(payload, dict) or payload.get("id") != identifier:
            raise ValueError("The saved notebook output reference has an invalid identity.")
        if binding is None:
            return
        manifest = (payload.get("metadata") or {}).get("source_manifest")
        if not isinstance(manifest, list) or not manifest:
            raise ValueError("The notebook output reference has no current reviewed source scope.")
        for item in manifest:
            if not isinstance(item, dict):
                raise ValueError("The notebook output source manifest is invalid.")
            artifact_id = item.get("id") or item.get("artifact_id")
            artifact = conn.execute(
                "SELECT id,version,content_hash,is_current FROM artifacts WHERE tender_id=? AND id=?",
                (tender_id, artifact_id),
            ).fetchone()
            if (
                artifact is None
                or not artifact["is_current"]
                or item.get("is_current") is not True
                or artifact["version"] != item.get("version")
                or artifact["content_hash"].lower() != str(item.get("content_hash", "")).lower()
                or not _scope_allows(binding, artifact["id"], artifact["version"], artifact["content_hash"])
            ):
                raise ValueError("The notebook output source basis is outside the reviewed delegation scope.")

    def _validate_references(
        self,
        tender_id: str,
        refs: list[str],
        *,
        assignment: dict[str, Any] | None,
        binding,
    ) -> None:
        with self.repo.db.connect() as conn:
            for value in refs:
                self._validate_reference(
                    conn,
                    tender_id,
                    value,
                    assignment=assignment,
                    binding=binding,
                )

    def append(self, ctx: OfficeExecutionIdentity, staff_id: str, draft: NotebookEntryDraft) -> NotebookEntry:
        tender_id = self._tender(ctx)
        if ctx.actor_kind not in {"engineer", "manager", "staff"}:
            raise ValueError("This actor cannot write a staff notebook.")
        if ctx.actor_kind == "staff" and ctx.actor_id != staff_id:
            raise ValueError("A colleague can only write their own notebook.")
        assignment_id = draft.assignment_id or ctx.assignment_id
        if draft.assignment_id is not None and ctx.assignment_id is not None and draft.assignment_id != ctx.assignment_id:
            raise ValueError("The notebook assignment does not match the active staff context.")
        assignment, binding, source_scope = self._assignment_scope(ctx, staff_id, assignment_id)
        self._validate_references(
            tender_id,
            draft.refs,
            assignment=assignment,
            binding=binding,
        )
        with self.repo.atomic() as conn:
            staff = conn.execute(
                "SELECT 1 FROM office_staff WHERE tender_id=? AND id=?",
                (tender_id, staff_id),
            ).fetchone()
            if staff is None:
                raise KeyError("This item could not be found in the selected Tender.")
            if draft.supersedes_id:
                previous = conn.execute(
                    """
                    SELECT id,staff_id,tender_id,assignment_id,current FROM office_staff_notebook
                    WHERE id=? AND tender_id=? AND staff_id=?
                    """,
                    (draft.supersedes_id, tender_id, staff_id),
                ).fetchone()
                if previous is None:
                    raise KeyError("This item could not be found in the selected Tender.")
                if previous["assignment_id"] != assignment_id:
                    raise ValueError("A notebook correction must stay within its originating assignment.")
                conn.execute(
                    "UPDATE office_staff_notebook SET current=0 WHERE id=?",
                    (draft.supersedes_id,),
                )
            identifier, stamp = new_id(), now()
            conn.execute(
                """
                INSERT INTO office_staff_notebook(
                    id,tender_id,staff_id,assignment_id,kind,text,refs_json,
                    applicability,supersedes_id,current,created_at,actor_id,
                    profile_version,route_binding_id,root_run_id,source_scope
                ) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)
                """,
                (
                    identifier,
                    tender_id,
                    staff_id,
                    assignment_id,
                    draft.kind,
                    draft.text,
                    dump(draft.refs),
                    draft.applicability,
                    draft.supersedes_id,
                    1,
                    stamp,
                    ctx.actor_id,
                    ctx.profile_version,
                    ctx.route_binding_id if binding is not None else None,
                    assignment["root_run_id"] if assignment is not None else ctx.root_run_id,
                    source_scope,
                ),
            )
            return NotebookEntry(
                id=identifier,
                tender_id=tender_id,
                staff_id=staff_id,
                assignment_id=assignment_id,
                kind=draft.kind,
                text=draft.text,
                refs=draft.refs,
                applicability=draft.applicability,
                supersedes_id=draft.supersedes_id,
                created_at=stamp,
                actor_id=ctx.actor_id,
                profile_version=ctx.profile_version,
                route_binding_id=binding.id if binding is not None else None,
                root_run_id=assignment["root_run_id"] if assignment is not None else ctx.root_run_id,
                source_scope=source_scope,
                current=True,
                stale=False,
            )

    def retrieve(
        self,
        ctx: OfficeExecutionIdentity,
        query: NotebookQuery,
        *,
        _assignment_id: str | None = None,
    ) -> NotebookPage:
        tender_id = self._tender(ctx)
        if ctx.actor_kind == "staff" and ctx.actor_id != query.staff_id:
            raise ValueError("A colleague cannot read another colleague's private notebook.")
        query_assignment_id = (
            _assignment_id if _assignment_id is not None else ctx.assignment_id
        ) if ctx.actor_kind == "staff" else _assignment_id
        if ctx.actor_kind == "staff":
            self._assignment_scope(ctx, query.staff_id, query_assignment_id)
            with self.repo.db.connect() as conn:
                assignment_row = conn.execute(
                    "SELECT * FROM office_assignments WHERE tender_id=? AND id=?",
                    (tender_id, query_assignment_id),
                ).fetchone()
            assignment_lineage = dict(assignment_row)
            binding_id = assignment_lineage["route_binding_id"]
        else:
            assignment_lineage = None
            binding_id = None
        after = _decode_cursor(query.cursor) if query.cursor else None
        like = f"%{query.query.strip()}%" if query.query.strip() else None
        with self.repo.db.connect() as conn:
            staff = conn.execute(
                "SELECT 1 FROM office_staff WHERE tender_id=? AND id=?",
                (tender_id, query.staff_id),
            ).fetchone()
            if staff is None:
                raise KeyError("This item could not be found in the selected Tender.")
            filters = ["tender_id=?", "staff_id=?"]
            params: list[Any] = [tender_id, query.staff_id]
            if query_assignment_id is not None:
                filters.append("assignment_id=?")
                params.append(query_assignment_id)
            if ctx.actor_kind == "staff":
                filters.extend(
                    [
                        "actor_id=?",
                        "profile_version=?",
                        "route_binding_id=?",
                        "root_run_id=?",
                    ]
                )
                params.extend(
                    [
                        ctx.actor_id,
                        assignment_lineage["staff_version"],
                        binding_id,
                        assignment_lineage["root_run_id"],
                    ]
                )
            if query.kind is not None:
                filters.append("kind=?")
                params.append(query.kind)
            if query.current is not None:
                filters.append("current=?")
                params.append(1 if query.current else 0)
            if like is not None:
                filters.append("text LIKE ?")
                params.append(like)
            where = " AND ".join(filters)
            total = conn.execute(
                f"SELECT COUNT(*) FROM office_staff_notebook WHERE {where}", params
            ).fetchone()[0]
            page_filters = list(filters)
            page_params = list(params)
            if after:
                page_filters.append("(created_at>? OR (created_at=? AND id>?))")
                page_params.extend([after[0], after[0], after[1]])
            rows = conn.execute(
                f"SELECT * FROM office_staff_notebook WHERE {' AND '.join(page_filters)} ORDER BY created_at,id LIMIT ?",
                (*page_params, query.limit + 1),
            ).fetchall()
        has_more = len(rows) > query.limit
        selected = rows[: query.limit]
        items = [
            NotebookEntry(
                id=row["id"],
                tender_id=row["tender_id"],
                staff_id=row["staff_id"],
                assignment_id=row["assignment_id"],
                kind=row["kind"],
                text=row["text"],
                refs=json.loads(row["refs_json"]),
                applicability=row["applicability"],
                supersedes_id=row["supersedes_id"],
                created_at=row["created_at"],
                actor_id=row["actor_id"],
                profile_version=row["profile_version"],
                route_binding_id=row["route_binding_id"],
                root_run_id=row["root_run_id"],
                source_scope=row["source_scope"] or "unbound",
                current=bool(row["current"]),
                stale=not bool(row["current"]),
            )
            for row in selected
        ]
        return NotebookPage(
            items=items,
            next_cursor=_cursor(selected[-1]["created_at"], selected[-1]["id"]) if has_more and selected else None,
            total=int(total),
        )

    def reconstruct(
        self,
        ctx: OfficeExecutionIdentity,
        staff_id: str,
        *,
        query: str = "",
        limit: int = 20,
    ) -> list[NotebookEntry]:
        """Return a bounded, current-only set of relevant notes. Never the whole notebook."""

        assignment_id = ctx.assignment_id if ctx.actor_kind == "staff" else None
        assignment, binding, _source_scope = self._assignment_scope(ctx, staff_id, assignment_id)
        page = self.retrieve(
            ctx,
            NotebookQuery(staff_id=staff_id, query=query, limit=min(limit, 20)),
            _assignment_id=assignment_id,
        )
        result = []
        for item in page.items:
            if not item.current or item.stale:
                continue
            if ctx.actor_kind == "staff" and (
                assignment is None
                or item.assignment_id != assignment["id"]
                or item.actor_id != ctx.actor_id
                or item.profile_version != assignment["staff_version"]
                or item.route_binding_id != binding.id
                or item.root_run_id != assignment["root_run_id"]
            ):
                continue
            try:
                self._validate_references(
                    self._tender(ctx),
                    item.refs,
                    assignment=assignment,
                    binding=binding,
                )
            except (KeyError, ValueError):
                continue
            result.append(item)
        return result
