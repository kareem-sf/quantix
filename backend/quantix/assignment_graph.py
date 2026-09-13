"""Persist task-specific work graphs without a closed workflow catalogue."""

from __future__ import annotations

import json

from .assignment_graph_models import (
    GraphDraft,
    GraphRevisionRequest,
    NodeStatus,
    WorkGraphStatus,
    WorkGraphVersion,
    WorkNode,
)
from .db import dump, new_id, now
from .execution_context import OfficeExecutionIdentity
from .staff_models import OfficeConflict
from .staff_store import _canonical_hash, _key

_SCHEMA = (
    """
    CREATE TABLE IF NOT EXISTS office_instruction_revisions (
        id TEXT PRIMARY KEY,
        root_id TEXT NOT NULL,
        original_engineer_message_id TEXT,
        revision INTEGER NOT NULL,
        content TEXT NOT NULL,
        basis_fingerprint TEXT NOT NULL,
        created_at TEXT NOT NULL,
        UNIQUE (root_id, revision)
    )
    """,
    """
    CREATE TABLE IF NOT EXISTS office_work_graphs (
        id TEXT PRIMARY KEY,
        tender_id TEXT NOT NULL,
        root_id TEXT NOT NULL,
        revision INTEGER NOT NULL,
        instruction_revision_id TEXT NOT NULL,
        nodes_json TEXT NOT NULL,
        edges_json TEXT NOT NULL,
        basis_fingerprint TEXT NOT NULL,
        created_at TEXT NOT NULL,
        UNIQUE (tender_id, root_id, revision)
    )
    """,
    """
    CREATE TABLE IF NOT EXISTS office_work_graph_receipts (
        id TEXT PRIMARY KEY,
        tender_id TEXT NOT NULL,
        root_id TEXT NOT NULL,
        operation TEXT NOT NULL,
        idempotency_key TEXT NOT NULL,
        payload_hash TEXT NOT NULL,
        graph_id TEXT NOT NULL,
        created_at TEXT NOT NULL,
        UNIQUE (tender_id, root_id, operation, idempotency_key)
    )
    """,
)


def _has_cycle(keys: list[str], prerequisites: dict[str, list[str]]) -> bool:
    visiting: set[str] = set()
    seen: set[str] = set()

    def walk(node: str) -> bool:
        if node in visiting:
            return True
        if node in seen:
            return False
        visiting.add(node)
        for parent in prerequisites.get(node, []):
            if parent not in prerequisites and parent not in keys:
                continue
            if walk(parent):
                return True
        visiting.remove(node)
        seen.add(node)
        return False

    return any(walk(key) for key in keys)


def node_statuses(graph: WorkGraphVersion) -> list[NodeStatus]:
    """Derive one display status and concrete remedy per node.

    A blocked prerequisite, a failed owner, a missing owner and missing
    engineer input stay distinct: only completed prerequisites unblock a node.
    """

    keys = {item.id: item.key for item in graph.nodes}
    states = {item.key: item.state for item in graph.nodes}
    statuses = []
    for item in graph.nodes:
        if item.state == "completed":
            statuses.append(
                NodeStatus(
                    key=item.key,
                    derived="completed",
                    waiting_on=[],
                    remedy="Kept as completed history.",
                )
            )
        elif item.state in {"failed", "cancelled"}:
            statuses.append(
                NodeStatus(
                    key=item.key,
                    derived="needs_attempt",
                    waiting_on=[],
                    remedy="Start a new attempt for this node before continuing.",
                )
            )
        elif item.state == "running":
            statuses.append(
                NodeStatus(
                    key=item.key, derived="working", waiting_on=[], remedy="Work is running."
                )
            )
        else:
            waiting = sorted(
                {
                    keys[parent]
                    for parent in item.prerequisite_ids
                    if states.get(keys.get(parent, ""), "completed") != "completed"
                }
            )
            if waiting:
                statuses.append(
                    NodeStatus(
                        key=item.key,
                        derived="waiting",
                        waiting_on=waiting,
                        remedy=f"Waiting on: {', '.join(waiting)}.",
                    )
                )
            elif item.owner_staff_id is None:
                statuses.append(
                    NodeStatus(
                        key=item.key,
                        derived="needs_owner",
                        waiting_on=[],
                        remedy="Assign an owner before starting this node.",
                    )
                )
            else:
                statuses.append(
                    NodeStatus(
                        key=item.key, derived="ready", waiting_on=[], remedy="Ready to start."
                    )
                )
    return statuses


class AssignmentGraphService:
    def __init__(self, repo):
        self.repo = repo
        with repo.atomic() as conn:
            for statement in _SCHEMA:
                conn.execute(statement)

    def _root(self, ctx: OfficeExecutionIdentity) -> tuple[str, str]:
        if not ctx.tender_id or not ctx.root_run_id:
            raise ValueError("A work graph requires a Tender work root.")
        return ctx.tender_id, ctx.root_run_id

    def _nodes_from_draft(self, nodes) -> tuple[list[WorkNode], list[list[str]]]:
        keys = [item.key for item in nodes]
        if len(set(keys)) != len(keys):
            raise ValueError("Work graph node keys must be unique.")
        prerequisites = {item.key: list(item.prerequisite_keys) for item in nodes}
        unknown = [
            parent
            for item in nodes
            for parent in item.prerequisite_keys
            if parent not in prerequisites
        ]
        if unknown:
            raise ValueError(f"A prerequisite '{unknown[0]}' is not a node in this graph.")
        if _has_cycle(keys, prerequisites):
            raise ValueError("A work graph cannot contain a cycle.")
        ids = {item.key: new_id() for item in nodes}
        records = [
            WorkNode(
                id=ids[item.key],
                key=item.key,
                owner_staff_id=item.owner_staff_id,
                expected_output=item.expected_output,
                completion_criteria=item.completion_criteria,
                prerequisite_ids=[ids[parent] for parent in item.prerequisite_keys],
                state="blocked" if item.prerequisite_keys else "ready",
                brief=item.brief,
            )
            for item in nodes
        ]
        edges = [
            [ids[parent], ids[item.key]] for item in nodes for parent in item.prerequisite_keys
        ]
        return records, edges

    def _record(self, row) -> WorkGraphVersion:
        return WorkGraphVersion(
            id=row["id"],
            root_id=row["root_id"],
            revision=row["revision"],
            instruction_revision_id=row["instruction_revision_id"],
            nodes=[WorkNode.model_validate(item) for item in json.loads(row["nodes_json"])],
            edges=json.loads(row["edges_json"]),
            basis_fingerprint=row["basis_fingerprint"],
            created_at=row["created_at"],
        )

    def propose_graph(self, ctx: OfficeExecutionIdentity, draft: GraphDraft) -> WorkGraphVersion:
        if not ctx.root_run_id:
            ctx = OfficeExecutionIdentity(
                tender_id=ctx.tender_id,
                actor_kind=ctx.actor_kind,
                actor_id=ctx.actor_id,
                root_run_id=draft.root_run_id,
                budget_scope_id=ctx.budget_scope_id,
                assignment_id=ctx.assignment_id,
                profile_version=ctx.profile_version,
                route_binding_id=ctx.route_binding_id,
                instruction_revision_id=ctx.instruction_revision_id,
                grant_fingerprint=ctx.grant_fingerprint,
                ownership_epoch=ctx.ownership_epoch,
                trusted_invocation_id=ctx.trusted_invocation_id,
            )
        tender_id, root_id = self._root(ctx)
        key = _key(draft.idempotency_key)
        payload_hash = _canonical_hash(draft.model_dump(mode="json"))
        records, edges = self._nodes_from_draft(draft.nodes)
        with self.repo.atomic() as conn:
            existing = conn.execute(
                """
                SELECT payload_hash,graph_id FROM office_work_graph_receipts
                WHERE tender_id=? AND root_id=? AND operation='propose' AND idempotency_key=?
                """,
                (tender_id, root_id, key),
            ).fetchone()
            if existing is not None:
                if existing["payload_hash"] != payload_hash:
                    raise OfficeConflict("This graph key was already used for different content.")
                row = conn.execute(
                    "SELECT * FROM office_work_graphs WHERE id=?", (existing["graph_id"],)
                ).fetchone()
                return self._record(row)
            stamp = now()
            instruction_id = new_id()
            conn.execute(
                """
                INSERT INTO office_instruction_revisions(
                    id,root_id,original_engineer_message_id,revision,content,basis_fingerprint,created_at
                ) VALUES(?,?,?,?,?,?,?)
                """,
                (
                    instruction_id,
                    root_id,
                    draft.engineer_message_id,
                    1,
                    draft.outcome,
                    payload_hash,
                    stamp,
                ),
            )
            graph_id = new_id()
            conn.execute(
                """
                INSERT INTO office_work_graphs(
                    id,tender_id,root_id,revision,instruction_revision_id,nodes_json,edges_json,
                    basis_fingerprint,created_at
                ) VALUES(?,?,?,?,?,?,?,?,?)
                """,
                (
                    graph_id,
                    tender_id,
                    root_id,
                    1,
                    instruction_id,
                    dump([item.model_dump(mode="json") for item in records]),
                    dump(edges),
                    payload_hash,
                    stamp,
                ),
            )
            conn.execute(
                """
                INSERT INTO office_work_graph_receipts(
                    id,tender_id,root_id,operation,idempotency_key,payload_hash,graph_id,created_at
                ) VALUES(?,?,?,?,?,?,?,?)
                """,
                (new_id(), tender_id, root_id, "propose", key, payload_hash, graph_id, stamp),
            )
            row = conn.execute(
                "SELECT * FROM office_work_graphs WHERE id=?", (graph_id,)
            ).fetchone()
            return self._record(row)

    def revise_graph(
        self, ctx: OfficeExecutionIdentity, request: GraphRevisionRequest
    ) -> WorkGraphVersion:
        tender_id, root_id = self._root(ctx)
        key = _key(request.idempotency_key)
        payload_hash = _canonical_hash(request.model_dump(mode="json"))
        records, edges = self._nodes_from_draft(request.nodes)
        with self.repo.atomic() as conn:
            current = conn.execute(
                """
                SELECT * FROM office_work_graphs
                WHERE tender_id=? AND root_id=? ORDER BY revision DESC LIMIT 1
                """,
                (tender_id, root_id),
            ).fetchone()
            if current is None:
                raise ValueError("Propose a work graph before revising it.")
            if current["revision"] != request.expected_revision:
                raise OfficeConflict("The work graph changed. Refresh before revising it.")
            previous = [WorkNode.model_validate(item) for item in json.loads(current["nodes_json"])]
            completed = {item.key: item for item in previous if item.state == "completed"}
            merged = []
            for item in records:
                if item.key in completed:
                    kept = completed[item.key]
                    merged.append(kept)
                else:
                    merged.append(item)
            stamp = now()
            graph_id = new_id()
            conn.execute(
                """
                INSERT INTO office_work_graphs(
                    id,tender_id,root_id,revision,instruction_revision_id,nodes_json,edges_json,
                    basis_fingerprint,created_at
                ) VALUES(?,?,?,?,?,?,?,?,?)
                """,
                (
                    graph_id,
                    tender_id,
                    root_id,
                    current["revision"] + 1,
                    current["instruction_revision_id"],
                    dump([item.model_dump(mode="json") for item in merged]),
                    dump(edges),
                    payload_hash,
                    stamp,
                ),
            )
            conn.execute(
                """
                INSERT INTO office_work_graph_receipts(
                    id,tender_id,root_id,operation,idempotency_key,payload_hash,graph_id,created_at
                ) VALUES(?,?,?,?,?,?,?,?)
                """,
                (new_id(), tender_id, root_id, "revise", key, payload_hash, graph_id, stamp),
            )
            row = conn.execute(
                "SELECT * FROM office_work_graphs WHERE id=?", (graph_id,)
            ).fetchone()
            return self._record(row)

    def mark_completed(self, tender_id: str, root_id: str, node_key: str) -> None:
        with self.repo.atomic() as conn:
            current = conn.execute(
                """
                SELECT * FROM office_work_graphs
                WHERE tender_id=? AND root_id=? ORDER BY revision DESC LIMIT 1
                """,
                (tender_id, root_id),
            ).fetchone()
            if current is None:
                raise ValueError("Propose a work graph before completing a node.")
            nodes = [WorkNode.model_validate(item) for item in json.loads(current["nodes_json"])]
            updated = [
                item.model_copy(update={"state": "completed"}) if item.key == node_key else item
                for item in nodes
            ]
            conn.execute(
                "UPDATE office_work_graphs SET nodes_json=? WHERE id=?",
                (dump([item.model_dump(mode="json") for item in updated]), current["id"]),
            )

    def latest(self, tender_id: str, root_id: str) -> WorkGraphVersion | None:
        with self.repo.db.connect() as conn:
            row = conn.execute(
                """
                SELECT * FROM office_work_graphs
                WHERE tender_id=? AND root_id=? ORDER BY revision DESC LIMIT 1
                """,
                (tender_id, root_id),
            ).fetchone()
            return None if row is None else self._record(row)

    def status(self, tender_id: str, root_id: str) -> WorkGraphStatus | None:
        """Return the latest graph with derived per-node statuses and remedies."""
        if not tender_id or not root_id:
            raise ValueError("A work graph status requires the selected Tender and work root.")
        graph = self.latest(tender_id, root_id)
        if graph is None:
            return None
        return WorkGraphStatus(graph=graph, nodes=node_statuses(graph))

    def count(self, tender_id: str, root_id: str) -> int:
        with self.repo.db.connect() as conn:
            row = conn.execute(
                "SELECT COUNT(*) FROM office_work_graphs WHERE tender_id=? AND root_id=?",
                (tender_id, root_id),
            ).fetchone()
            return int(row[0])
