"""Concurrent ready-branch execution under one root allowance.

Serial execution remains the default: an envelope with ``max_concurrency=1``
runs one assignment per wave in creation order, exactly as before. Higher
reviewed caps run independent ready branches as concurrent coroutines; the
database layer holds one connection per thread and asyncio task, so branches
never share a transaction. Original-client accounts serialize whole turns
behind a per-account asyncio lock while direct API work overlaps. Every
dispatch holds an aggregate reservation from the same root allowance. A
failing branch never cancels its siblings; dependants of a failed branch fail
fast with the blocking reason instead of waiting forever. Outer cancellation
(Stop) reaches every branch because no worker thread can outlive the drain.
"""

from __future__ import annotations

import asyncio

from .execution_context import OfficeExecutionIdentity
from .staff_runtime import StaffAssignmentOutcome, run_staff_assignment

_TERMINAL_PREREQUISITE_STATES = {"failed", "cancelled", "interrupted"}


def _scope_identity(tender_id: str, root_run_id: str) -> OfficeExecutionIdentity:
    return OfficeExecutionIdentity(
        tender_id=tender_id,
        actor_kind="engineer",
        actor_id="engineer",
        root_run_id=root_run_id,
        budget_scope_id=root_run_id,
        assignment_id=None,
        profile_version=None,
        route_binding_id=None,
        instruction_revision_id=None,
        grant_fingerprint=None,
        ownership_epoch=None,
        trusted_invocation_id=None,
    )


def _locked_connection_id(repo, tender_id: str, assignment) -> str | None:
    """Return the account ID that must serialize this assignment, if any.

    Original-client accounts serialize whole assignment turns behind one
    per-account lock; direct API accounts overlap freely and need no lock.
    """

    from .ai_connections import AIConnectionService
    from .staff_routing import StaffRoutingService

    try:
        binding = StaffRoutingService(repo).validate_binding(tender_id, assignment.route_binding_id)
        connection = AIConnectionService(repo).get(binding.route.connection_id)
    except (KeyError, ValueError):
        return None
    if connection.get("auth_type") != "client_login":
        return None
    return connection["id"]


async def _run_branch(repo, tender_id: str, assignment_id: str, lock: asyncio.Lock | None):
    if lock is not None:
        from types import SimpleNamespace

        from .run_activity import ActivityRecorder
        from .staff_assignments import StaffAssignmentService

        assignment = StaffAssignmentService(repo).get(tender_id, assignment_id)
        recorder = ActivityRecorder(SimpleNamespace(repo=repo, tender_id=tender_id,
            run_id=assignment.root_run_id, assignment_id=assignment_id, actor_id=assignment.staff_id))
        waiting = recorder.start("waiting", "Waiting for this AI account's current assignment to finish.",
                                 {"reason": "original_client_account_serialization"}, phase="queued") if lock.locked() else None
        try:
            async with lock:
                if waiting:
                    recorder.record(waiting, "waiting", "completed", "The AI account is available for this assignment.")
                    waiting = None
                return await run_staff_assignment(repo, tender_id, assignment_id)
        except asyncio.CancelledError:
            if waiting:
                recorder.record(waiting, "waiting", "interrupted", "Stopped while waiting for the AI account.")
            raise
    return await run_staff_assignment(repo, tender_id, assignment_id)


async def drain_queued_assignments(
    repo,
    tender_id: str,
    root_run_id: str,
    *,
    max_concurrency: int,
    max_requests: int,
    sink: list[StaffAssignmentOutcome] | None = None,
) -> list[StaffAssignmentOutcome]:
    """Run queued assignments for one root until none are ready.

    Outcomes are appended to ``sink`` as waves complete. Sibling failures are
    recorded on their assignments without cancelling the wave; the first
    failure is re-raised after the drain so the run keeps its existing
    failure semantics, without losing already completed siblings.
    """

    from .resource_leases import ResourceLeaseService
    from .staff_assignments import StaffAssignmentService
    from .staff_routing import StaffRoutingService

    if not 1 <= max_concurrency <= 8:
        raise ValueError("The reviewed concurrency cap is outside the supported range.")
    assignments = StaffAssignmentService(repo)
    routing = StaffRoutingService(repo)
    leases = ResourceLeaseService(repo)
    scope = _scope_identity(tender_id, root_run_id)
    leases.ensure_scope(scope, max_requests=max_requests)
    account_locks: dict[str, asyncio.Lock] = {}
    outcomes: list[StaffAssignmentOutcome] = sink if sink is not None else []
    first_error: BaseException | None = None

    def prereq_status(assignment) -> dict[str, str]:
        states: dict[str, str] = {}
        for prerequisite_id in assignment.prerequisites:
            try:
                states[prerequisite_id] = assignments.get(tender_id, prerequisite_id).status
            except KeyError:
                states[prerequisite_id] = "missing"
        return states

    def account_lock(assignment):
        locked = _locked_connection_id(repo, tender_id, assignment)
        if locked is None:
            return None
        return account_locks.setdefault(locked, asyncio.Lock())

    def current_assignments():
        # Completed rows retain their place in history. Read all pages so a
        # long root does not strand queued work beyond its first 200 rows.
        rows = []
        offset = 0
        while True:
            page = assignments.list(tender_id, root_run_id=root_run_id, offset=offset, limit=200)
            rows.extend(page)
            if len(page) < 200:
                return rows
            offset += len(page)

    while True:
        try:
            if repo.get_run(root_run_id)["status"] not in {"queued", "running"}:
                break
        except KeyError:
            break
        queued = [
            item
            for item in current_assignments()
            if item.status == "queued"
        ]
        if not queued:
            break
        binding = routing.validate_binding(tender_id, queued[0].route_binding_id)
        grant = routing.validate_root(tender_id, root_run_id, binding.plan_id)
        if max_concurrency > grant.envelope.max_concurrency:
            raise ValueError("Execution exceeds the reviewed concurrency allowance.")
        states = {item.id: prereq_status(item) for item in queued}
        for item in queued:
            bad = sorted(
                prerequisite_id
                for prerequisite_id, state in states[item.id].items()
                if state in _TERMINAL_PREREQUISITE_STATES or state == "missing"
            )
            if not bad:
                continue
            try:
                assignments.fail(
                    tender_id,
                    item.id,
                    f"Prerequisite {bad[0]} can no longer complete.",
                )
            except (KeyError, ValueError):
                pass
        queued = [
            item
            for item in current_assignments()
            if item.status == "queued"
        ]
        states = {item.id: prereq_status(item) for item in queued}
        ready = sorted(
            (
                item
                for item in queued
                if all(state == "completed" for state in states[item.id].values())
            ),
            key=lambda item: (item.depth, item.created_at, item.id),
        )
        if not ready:
            remaining = [item for item in queued]
            if remaining:
                # Nothing can proceed: every remaining branch waits on another
                # remaining branch, so the prerequisites hold a cycle. Fail
                # fast with the blocking reason instead of looping forever.
                for item in remaining:
                    try:
                        assignments.fail(
                            tender_id,
                            item.id,
                            "Prerequisite assignments cannot complete; the dependency chain is blocked.",
                        )
                    except (KeyError, ValueError):
                        pass
            break
        try:
            if repo.get_run(root_run_id)["status"] not in {"queued", "running"}:
                break
        except KeyError:
            break
        wave = ready[:max_concurrency]
        admitted = []
        for item in wave:
            try:
                if not leases.reserve(scope):
                    break
            except ValueError:
                break
            admitted.append(item)
        if not admitted:
            break
        results = await asyncio.gather(
            *(_run_branch(repo, tender_id, item.id, account_lock(item)) for item in admitted),
            return_exceptions=True,
        )
        for item, result in zip(admitted, results):
            if isinstance(result, BaseException):
                if first_error is None:
                    first_error = result
                continue
            outcomes.append(result)
    if first_error is not None:
        raise first_error
    return outcomes
