"""Durable job state with cooperative cancellation and per-Tender execution lanes."""

import asyncio
import re
import threading
from decimal import Decimal
from pathlib import Path

from .ai_connections import AIConnectionService
from .diagnostics import record, record_exception
from .intake import import_package
from .manager_runtime import ManagerRunProfiles
from .office_types import PreparedOfficeResult
from .pending import PendingInstructionService
from .repository import Repository
from .retrieval_indexing import RetrievalIndexer
from .settings import SettingsService
from .team import TeamService


def safe_error(error: BaseException, secret: str | None = None) -> str:
    message = str(error) or type(error).__name__
    if secret:
        message = message.replace(secret, "[private key]")
    message = re.sub(r"sk-[A-Za-z0-9_-]+", "[private key]", message)
    return message[:2000]


# A Manager run includes its colleagues' turns, so it is allowed longer than one agent loop.
MANAGER_RUN_SECONDS = 45 * 60


class JobManager:
    def __init__(self, repo: Repository, settings: SettingsService):
        self.repo = repo
        self.settings = settings
        self.tasks: dict[str, asyncio.Task] = {}
        self.cancelled: dict[str, threading.Event] = {}
        self.lanes: dict[str, asyncio.Lock] = {}
        self.closing = False
        # Analyzing tender package runs automatically after every import. Tests that
        # measure a single job switch it off.
        self.auto_analyze = True
        self.pending = PendingInstructionService(repo)
        self.manager_profiles = ManagerRunProfiles(repo)
        self.team = TeamService(repo)
        self.indexer = RetrievalIndexer(repo)
        self.repo.on_retrieval_generation = self.indexer.request_refresh

    def active(self, tender_id):
        return [
            r
            for r in self.repo.list_runs(tender_id)
            if r["status"] in {"queued", "running"} and r["kind"] != "index"
        ]

    def _start(self, tender_id, kind, instruction, *, task_id=None):
        with self.repo.atomic():
            run = self._queue(tender_id, kind, instruction, task_id=task_id)
        return self._schedule(run, task_id)

    def _queue(self, tender_id, kind, instruction, *, task_id=None):
        """Save queued work inside the caller's synchronous transaction."""
        if self.closing:
            raise ValueError("The Tender Office is closing. Reopen it before starting more work.")
        active = self.active(tender_id)
        if active:
            raise ValueError(
                "This Tender already has work in progress. Stop that work before changing its instructions."
            )
        run = self.repo.create_run(tender_id, kind, instruction)
        if kind in {"manager", "identify", "analysis"}:
            self.manager_profiles.capture(tender_id, run["id"])
        if task_id:
            self.repo.event(
                run["id"], "assigned_task", "An approved task is queued.", {"task_id": task_id}
            )
        return run

    def _schedule(self, run, task_id=None):
        """Start execution only after its queue transaction has committed."""
        self.cancelled[run["id"]] = threading.Event()
        self.tasks[run["id"]] = asyncio.create_task(
            self._execute(run, task_id), name=f"quantix-{run['id']}"
        )
        self.tasks[run["id"]].add_done_callback(lambda _: self._cleanup(run["id"]))
        return run

    def _cleanup(self, run_id):
        self.tasks.pop(run_id, None)
        self.cancelled.pop(run_id, None)

    def start_import(self, tender_id, source_path):
        source = Path(source_path).expanduser()
        if not source.exists():
            raise ValueError("The selected package cannot be found on this computer.")
        with self.repo.atomic():
            run = self._queue(tender_id, "import", str(source.resolve()))
        return self._schedule(run)

    def _provisional_name(self, tender_id):
        """Name a still-unnamed Tender after its package folder, marked provisional."""

        from .project_identity import package_name

        tender = self.repo.get_tender(tender_id)
        if tender["name_source"] != "pending":
            return
        source = next(
            (
                run["instruction"]
                for run in self.repo.list_runs(tender_id)
                if run["kind"] == "import" and run["status"] == "completed"
            ),
            None,
        )
        if source:
            self.repo.rename_tender(tender_id, package_name(source), source="package")

    def start_identify(self, tender_id):
        """Identify the project from its imported package on the Tender's approved AI."""

        from .ai_policy import AIPolicyService

        policy = AIPolicyService(self.repo)
        with policy.connections.authority_guard(), self.repo.atomic():
            self._preflight_route(tender_id, policy)
            if not self.repo.list_artifacts(tender_id):
                raise ValueError("Add the Tender package before identifying the project.")
            run = self._queue(tender_id, "identify", "Identify the project from its package.")
        return self._schedule(run)

    def start_analysis(self, tender_id):
        """Analyzing tender package: OCR coverage, evidence index, BOQ and the AI package map."""

        with self.repo.atomic():
            if not self.repo.list_artifacts(tender_id):
                raise ValueError("Add the tender package before analysing it.")
            run = self._queue(tender_id, "analysis", "Analyzing tender package.")
        return self._schedule(run)

    def _ai_ready(self, tender_id):
        """True when the Tender's AI can be used now, otherwise the plain reason."""

        from .ai_policy import AIPolicyService

        policy = AIPolicyService(self.repo)
        try:
            with policy.connections.authority_guard():
                self._preflight_route(tender_id, policy)
            return True
        except (KeyError, ValueError) as error:
            return str(error)

    def needs_analysis(self, tender_id):
        from .package_analysis import package_map

        if not self.repo.list_artifacts(tender_id):
            return False
        saved = package_map(self.repo, tender_id)
        return (
            saved is None
            or not saved["current"]
            or (saved.get("stages") or {}).get("map", {}).get("state") != "completed"
        )

    def maybe_analyze(self, tender_id):
        """Start package analysis when the package changed or its map is not complete yet."""

        try:
            if self.active(tender_id) or not self.needs_analysis(tender_id):
                return None
            return self.start_analysis(tender_id)
        except (KeyError, ValueError) as error:
            record(
                "package_analysis",
                phase="admission",
                outcome="skipped",
                tender_id=tender_id,
                detail=type(error).__name__,
            )
            return None

    def maybe_identify(self, tender_id):
        """Start identification when a Tender still carries its package name.

        Without usable AI access the package name stays; choosing the Tender's
        AI later calls this again.
        """

        try:
            tender = self.repo.get_tender(tender_id)
            if tender["name_source"] not in {"pending", "package"} or self.active(tender_id):
                return None
            return self.start_identify(tender_id)
        except (KeyError, ValueError) as error:
            record(
                "project_identify",
                phase="admission",
                outcome="skipped",
                tender_id=tender_id,
                detail=type(error).__name__,
            )
            # Without usable AI the Tender keeps a provisional folder name and
            # tells the engineer what finishes the analysis.
            try:
                if self.repo.get_tender(tender_id)["name_source"] == "pending":
                    self._provisional_name(tender_id)
                    self.repo.add_message(
                        tender_id,
                        "system",
                        "The documents are registered. Choose the AI for this Tender in the message box "
                        "to finish analyzing the tender package and identify the project.",
                    )
            except (KeyError, ValueError):
                pass
            return None

    def start_index(self, tender_id):
        return self.indexer.request_refresh(tender_id, force=True)

    def cancel_index(self, tender_id):
        return self.indexer.cancel(tender_id)

    def start_manager(self, tender_id, instruction, *, engineer_message=True):
        from .ai_policy import AIPolicyService

        policy = AIPolicyService(self.repo)
        if not instruction.strip():
            raise ValueError("Enter an instruction for the Tender Manager.")
        with policy.connections.authority_guard(), self.repo.atomic():
            if self.active(tender_id):
                raise ValueError(
                    "Stop the current work before sending a new instruction. Your message has not been sent."
                )
            policy.routes_for(tender_id)
            run = self._queue(tender_id, "manager", instruction)
            if engineer_message:
                self.repo.add_message(tender_id, "engineer", instruction, run_id=run["id"])
        return self._schedule(run)

    @staticmethod
    def _hold_reason(error: BaseException) -> str:
        message = str(error).lower()
        if any(value in message for value in ("budget", "spending", "allowance", "price")):
            return "budget"
        if any(
            value in message for value in ("model", "account", "sign in", "connection", "setup")
        ):
            return "model"
        if "source" in message or "evidence" in message:
            return "source"
        if "work" in message or "current tender" in message:
            return "work"
        return "permission"

    def _preflight_route(self, tender_id, policy):
        """Validate the currently selected route without reserving or billing work."""

        from .ai_connections import is_supported_profile
        from .ai_policy import BudgetMeter
        from .ai_readiness import require_ready
        from .ai_subscription import allows_extras

        routes = policy.routes_for(tender_id)
        route = routes[0]
        connections = policy.connections
        connection = connections.get(route["connection_id"])
        if not is_supported_profile(connection):
            raise ValueError(
                "This saved AI account is retired. Choose one of the five supported provider routes."
            )
        if not connection["enabled"]:
            raise ValueError(
                "The selected AI account is disabled. Enable it in Settings before continuing."
            )
        # Reading credentials here confirms the selected account is usable;
        # values never leave this process or enter a request/result record.
        credentials = connections.credentials(connection["id"])
        if connection["auth_type"] in {"api_key", "environment"} and not credentials.get("api_key"):
            raise ValueError(
                "Enter the selected AI account credential in Settings before continuing."
            )
        try:
            require_ready(self.repo, connection, route["model_id"])
            meter = BudgetMeter(policy, tender_id, tender_id, route)
        except (KeyError, StopIteration) as error:
            raise ValueError(
                "The selected AI model is no longer available. Refresh its setup before continuing."
            ) from error
        current = policy.get(tender_id)
        if (
            allows_extras(connection)
            and current.get("provider_managed_extras", {}).get(connection["id"])
            != connection["revision"]
        ):
            raise ValueError("Review this AI account's spending settings before continuing.")
        if (
            meter.connection["billing"] in {"metered", "unknown"} or allows_extras(connection)
        ) and current["restore_reconciliation_required"]:
            raise ValueError(
                "Review spending after restoring this workspace before continuing paid AI work."
            )
        prices = meter.model.get("pricing")
        if meter.connection["billing"] in {"metered", "unknown"} and (
            not prices or not current["run_budget_usd"] or not current["tender_budget_usd"]
        ):
            raise ValueError(
                "Confirm the AI prices and Tender spending allowance in Settings before continuing."
            )
        with self.repo.db.connect() as conn:
            spent, reserved, _request_count = policy._totals(conn, tender_id)
        if current["max_requests"] < 1:
            raise ValueError(
                "The Tender AI request allowance is exhausted. Review its limits before continuing."
            )
        if current["tender_budget_usd"] is not None and spent + reserved >= Decimal(
            str(current["tender_budget_usd"])
        ):
            raise ValueError(
                "The Tender spending allowance is exhausted. Review its limits before continuing."
            )
        return route

    def _immediate_result(self, run):
        return {"outcome": "immediate", "run": run}

    def _pending_result(self, pending):
        public = self.pending._public(pending)
        return {"outcome": "pending", "pending": public}

    def submit_message(self, tender_id, instruction, *, idempotency_key=None, action=None):
        """Route freeform text, queue a single pending draft when busy, or start now."""

        if not isinstance(instruction, str) or not instruction.strip():
            raise ValueError("Enter an instruction for the Tender Manager.")
        if action not in {None, "review_documents"}:
            raise ValueError("Unknown message action.")
        instruction = instruction.strip()
        # A generated key still gives every pending row an explicit identity;
        # callers that retry should send their own stable key.
        from .pending import _clean_key

        key = _clean_key(idempotency_key)
        from .ai_policy import AIPolicyService

        policy = AIPolicyService(self.repo)
        with policy.connections.authority_guard(), self.repo.atomic():
            existing = self.pending.lookup_idempotency(tender_id, key, instruction, action)
            if existing:
                if existing["outcome_kind"] == "immediate" and existing.get("run_id"):
                    return self._immediate_result(self.repo.get_run(existing["run_id"]))
                current = self.pending.get(tender_id)
                if current is not None and current["id"] == existing.get("pending_id"):
                    return self._pending_result(current)
                if existing["outcome_kind"] == "pending":
                    raise ValueError(
                        "This pending message was cancelled or replaced. Use a new idempotency key."
                    )
            active = self.active(tender_id)
            if active:
                pending = self.pending.upsert(
                    tender_id,
                    instruction,
                    key,
                    [run["id"] for run in active],
                    action=action,
                )
                return self._pending_result(pending)
            if policy.get(tender_id)["manager"] is None:
                raise ValueError(
                    "Open Settings and choose a Tender Manager connection before sending this instruction."
                )
            self._preflight_route(tender_id, policy)
            run = self._queue(tender_id, "manager", instruction)
            self.repo.add_message(tender_id, "engineer", instruction, run_id=run["id"])
            self.pending.remember_idempotency(
                tender_id,
                key,
                instruction,
                action=action,
                outcome_kind="immediate",
                run_id=run["id"],
                outcome={"run_id": run["id"]},
            )
        return self._immediate_result(self._schedule(run))

    def consume_pending(self, tender_id, *, pending_id, expected_revision, explicit=True):
        """Consume the latest draft and queue exactly one run in one transaction."""

        from .ai_policy import AIPolicyService

        policy = AIPolicyService(self.repo)
        with policy.connections.authority_guard(), self.repo.atomic():
            if self.active(tender_id):
                raise ValueError(
                    "Finish or stop the current Tender work before sending this instruction."
                )
            current = self.pending.get(tender_id)
            if current is None:
                raise ValueError("There is no pending instruction to send.")
            if not explicit and current["status"] != "pending":
                return None
            if current["id"] != pending_id or current["revision"] != expected_revision:
                raise ValueError("The pending instruction changed. Refresh it before sending.")
            self._preflight_route(tender_id, policy)
            consumed = self.pending.consume(
                tender_id, pending_id=pending_id, expected_revision=expected_revision
            )
            instruction = consumed["content"]
            run = self._queue(tender_id, "manager", instruction)
            self.repo.add_message(tender_id, "engineer", instruction, run_id=run["id"])
            self.pending.remember_idempotency(
                tender_id,
                consumed["idempotency_key"],
                instruction,
                action=consumed.get("action"),
                outcome_kind="immediate",
                pending_id=consumed["id"],
                run_id=run["id"],
                outcome={"run_id": run["id"]},
            )
        return self._immediate_result(self._schedule(run))

    def edit_pending(self, tender_id, content, *, pending_id, expected_revision, action=None):
        return self.pending._public(
            self.pending.edit(
                tender_id,
                content,
                pending_id=pending_id,
                expected_revision=expected_revision,
                action=action,
            )
        )

    def cancel_pending(self, tender_id, *, pending_id, expected_revision):
        return self.pending.cancel(
            tender_id, pending_id=pending_id, expected_revision=expected_revision
        )

    def stop_tender_work(self, tender_id):
        self.repo.get_tender(tender_id)
        # Set the hold before cancellation starts.  A cancelled task therefore
        # cannot race a successful completion callback into auto-dispatch.
        self.pending.hold_for_tender(tender_id, "stopped")
        for active in self.active(tender_id):
            self.cancel(active["id"])
        return {"ok": True}

    def _maybe_dispatch_pending(self, tender_id):
        pending = self.pending.get(tender_id)
        if pending is None or pending["status"] != "pending" or self.active(tender_id):
            return None
        try:
            return self.consume_pending(
                tender_id,
                pending_id=pending["id"],
                expected_revision=pending["revision"],
                explicit=False,
            )
        except ValueError as error:
            self.pending.hold_for_tender(tender_id, self._hold_reason(error))
            return None

    def start_task(self, tender_id, task_id):
        from .ai_policy import AIPolicyService

        policy = AIPolicyService(self.repo)
        with policy.connections.authority_guard(), self.repo.atomic() as conn:
            task = self.repo.get_task(tender_id, task_id)
            plan = self.repo.get_plan(tender_id, task["plan_id"])
            if plan["status"] != "approved" or task["status"] not in {
                "ready",
                "failed",
                "cancelled",
                "interrupted",
            }:
                raise ValueError(
                    "This task requires an approved current plan before work can start."
                )
            self.repo.approved_scope(tender_id, task["plan_id"])
            self._preflight_route(tender_id, policy)
            self.repo._check_sources(conn, tender_id, task["source_ids"])
            instruction = f"Carry out the approved task '{task['title']}': {task['description']}"
            run = self._queue(tender_id, "manager", instruction)
            self.repo.add_message(tender_id, "engineer", instruction, run_id=run["id"])
            self.repo.event(
                run["id"],
                "requested_task",
                "The engineer asked the Tender Manager to carry out this approved task.",
                {"task_id": task_id, "plan_id": task["plan_id"]},
            )
        return self._schedule(run)

    def carry_out_plan(self, tender_id, plan_id):
        """Queue one Manager run for an approved plan; the Manager assigns its team."""

        from .ai_policy import AIPolicyService

        policy = AIPolicyService(self.repo)
        with policy.connections.authority_guard(), self.repo.atomic():
            plan = self.repo.get_plan(tender_id, plan_id)
            if plan["status"] != "approved":
                raise ValueError("Approve the plan before starting its work.")
            self._preflight_route(tender_id, policy)
            instruction = f"Carry out the approved work plan: {plan['title']}. Assign your team as the work needs."
            run = self._queue(tender_id, "manager", instruction)
            self.repo.event(
                run["id"],
                "approved_plan",
                "The Tender Manager is carrying out the approved plan.",
                {"plan_id": plan_id},
            )
        return self._schedule(run)

    async def _execute(self, run, task_id):
        from types import SimpleNamespace

        from .run_activity import ActivityRecorder, activity_scope

        identifier, tender_id = run["id"], run["tender_id"]
        publication_task_id = task_id or next(
            (
                event["data"].get("task_id")
                for event in self.repo.run_events(identifier)
                if event["kind"] == "requested_task"
            ),
            None,
        )
        started = asyncio.get_running_loop().time()
        record("job_lifecycle", phase="queued", outcome="started", run_id=identifier)
        cancelled = self.cancelled[identifier]
        prepared = None
        created_outputs = []
        publication_committed = False
        monitor = ActivityRecorder(
            SimpleNamespace(repo=self.repo, run_id=identifier, tender_id=tender_id)
        )
        execution_operation = None
        execution_scope = None
        try:
            execution_operation = monitor.start(
                "run",
                "Instruction accepted; waiting for this Tender's current work.",
                {"instruction": run["instruction"], "kind": run["kind"]},
                phase="queued",
            )
            execution_scope = activity_scope(execution_operation)
            execution_scope.__enter__()
            lane = f"{tender_id}:index" if run["kind"] == "index" else tender_id
            async with self.lanes.setdefault(lane, asyncio.Lock()):
                if cancelled.is_set():
                    raise InterruptedError("Work stopped before it started.")
                initial_detail = {
                    "manager": "The Tender Manager is reviewing the package.",
                    "identify": "Identifying the project from its documents.",
                    "analysis": "Analyzing tender package.",
                }.get(run["kind"], "Work is starting.")
                self.repo.update_run(identifier, status="running", detail=initial_detail)
                monitor.record(execution_operation, "run", "started", initial_detail)
                record("job_lifecycle", phase="running", outcome="started", run_id=identifier)
                if publication_task_id:
                    self.repo.update_task(
                        tender_id, publication_task_id, status="running", run_id=identifier
                    )
                if run["kind"] == "import":
                    result = await asyncio.to_thread(
                        import_package,
                        self.repo,
                        tender_id,
                        Path(run["instruction"]),
                        identifier,
                        cancelled,
                    )
                    from .estimates import EstimateService

                    estimate = await asyncio.to_thread(
                        EstimateService(self.repo).refresh, tender_id
                    )
                    result["boq_count"] = len(estimate["items"])
                elif run["kind"] == "index":
                    from .semantic import SemanticService

                    def progress(percent, detail):
                        self.repo.update_run(
                            identifier, progress=min(99, max(0, percent)), detail=detail
                        )

                    result = await asyncio.to_thread(
                        SemanticService(self.repo).index, tender_id, cancelled.is_set, progress
                    )
                elif run["kind"] == "analysis":
                    from .package_analysis import run_analysis

                    prepared = await asyncio.wait_for(
                        run_analysis(
                            self.repo,
                            tender_id,
                            identifier,
                            cancelled,
                            ai_ready=lambda: self._ai_ready(tender_id),
                        ),
                        timeout=3 * 60 * 60,
                    )
                elif run["kind"] == "identify":
                    from .project_identity import run_identification

                    prepared = await asyncio.wait_for(
                        run_identification(self.repo, tender_id, identifier), timeout=5 * 60
                    )
                else:
                    from .office import run_manager

                    prepared = await asyncio.wait_for(
                        run_manager(self.repo, tender_id, identifier, run["instruction"]),
                        timeout=MANAGER_RUN_SECONDS,
                    )
                if cancelled.is_set():
                    raise InterruptedError("Work stopped at your request.")
                validation_operation = monitor.start(
                    "validation", "Checking the result before saving Tender records."
                )
                with AIConnectionService(self.repo).authority_guard(), self.repo.atomic():
                    if cancelled.is_set():
                        raise InterruptedError("Work stopped before publication.")
                    from .package_analysis import PreparedAnalysisResult, apply_analysis
                    from .project_identity import PreparedIdentityResult, apply_identity

                    if isinstance(prepared, PreparedAnalysisResult):
                        if prepared.run_id != identifier or prepared.tender_id != tender_id:
                            raise ValueError("The worker returned an invalid package analysis.")
                        result = apply_analysis(self.repo, prepared)
                        if prepared.overview is None:
                            self._provisional_name(tender_id)
                    elif isinstance(prepared, PreparedIdentityResult):
                        if prepared.run_id != identifier or prepared.tender_id != tender_id:
                            raise ValueError(
                                "The worker returned an invalid project identification."
                            )
                        result = apply_identity(self.repo, prepared)
                    elif isinstance(prepared, PreparedOfficeResult):
                        from .office import publish_prepared

                        if not isinstance(prepared, PreparedOfficeResult) or (
                            prepared.run_id != identifier or prepared.tender_id != tender_id
                        ):
                            raise ValueError(
                                "The worker returned an invalid prepared Tender result."
                            )
                        result = publish_prepared(self.repo, prepared)
                    self.repo.update_run(
                        identifier,
                        status="completed",
                        progress=100,
                        detail="Work is saved.",
                        result=result,
                        usage=result.get("usage", {}),
                    )
                    if publication_task_id:
                        waiting = any(
                            assignment.status == "waiting"
                            for assignment in self.team.list(tender_id, run_id=identifier)
                        )
                        self.repo.update_task(
                            tender_id,
                            publication_task_id,
                            status="interrupted" if waiting else "completed",
                            result=result,
                            run_id=identifier,
                        )
                    if (
                        isinstance(prepared, PreparedOfficeResult)
                        and prepared.output.draft_documents
                    ):
                        from .outputs import OutputService

                        if not prepared.approved_plan_id:
                            raise ValueError("The draft requests have no approved work scope.")
                        outputs = OutputService(self.repo)
                        for document in prepared.output.draft_documents:
                            created_outputs.append(
                                outputs.generate_from_office(
                                    tender_id,
                                    document.model_dump(mode="json"),
                                    identifier,
                                    prepared.approved_plan_id,
                                )
                            )
                        result = result | {"generated_outputs": created_outputs}
                        self.repo.update_run(identifier, result=result)
                    self.repo.event(
                        identifier, "completed", "Work is saved and available for review."
                    )
                    monitor.record(
                        validation_operation,
                        "validation",
                        "completed",
                        "Result checks passed and Tender records were saved.",
                    )
                    monitor.record(
                        execution_operation,
                        "run",
                        "completed",
                        "Work is saved and available for review.",
                        {
                            "usage": result.get("usage", {}),
                            "generated_outputs": result.get("generated_outputs", []),
                        },
                    )
                publication_committed = True
                record(
                    "job_lifecycle",
                    phase="completed",
                    outcome="saved",
                    run_id=identifier,
                    duration_ms=int((asyncio.get_running_loop().time() - started) * 1000),
                )
            self.pending.on_run_finished(identifier, "completed")
            if run["kind"] == "import" and self.auto_analyze:
                self.maybe_analyze(tender_id)
            if run["kind"] == "import":
                self.indexer.request_refresh(tender_id, force=True)
            self._maybe_dispatch_pending(tender_id)
        except (asyncio.CancelledError, InterruptedError):
            self._stop(run, publication_task_id)
        except Exception as error:
            reference = record_exception("job_failed", error, phase=run["kind"], run_id=identifier)
            from .run_activity import persist_recording_failure

            persist_recording_failure(self.repo, identifier)
            message = (
                "The work exceeded its time limit. Review the saved progress, narrow the request and resume."
                if isinstance(error, TimeoutError)
                else safe_error(error)
            )
            with self.repo.atomic():
                current = self.repo.get_run(identifier)
                if current["status"] == "completed":
                    # A post-completion Manager handoff can fail after the
                    # durable work is already saved. Keep that parent run
                    # complete and leave one visible, restart-safe action.
                    followup = "The completed work is saved. Review AI access, then ask the Tender Manager to continue."
                    self.repo.event(
                        identifier,
                        "manager_followup_needed",
                        followup,
                        {"error_reference": reference},
                    )
                    self.repo.add_message(tender_id, "system", followup)
                    return
                if current["status"] in {"queued", "running"}:
                    self.repo.update_run(
                        identifier, status="failed", error=message, detail="Work needs attention."
                    )
                    if run["kind"] in {"identify", "analysis"}:
                        # Never leave the failure silent or the Tender unnamed.
                        self._provisional_name(tender_id)
                        self.repo.add_message(
                            tender_id,
                            "system",
                            "Quantix could not finish analyzing the tender package: "
                            f"{message} Use Resume on the analysis to try again.",
                        )
                    self.repo.event(identifier, "failed", message, {"error_reference": reference})
                    from .run_activity import ActivityRecordingError, settle_open_operations

                    try:
                        settle_open_operations(
                            self.repo,
                            identifier,
                            "failed",
                            "Run ended before this step had a confirmed completion.",
                        )
                    except ActivityRecordingError:
                        pass  # Saving failure must not roll back revoked authority.
                    self.team.cancel_run(
                        tender_id, identifier, "The Manager run stopped before this work finished."
                    )
                    if publication_task_id:
                        self.repo.update_task(
                            tender_id, publication_task_id, status="failed", run_id=identifier
                        )
            self.pending.on_run_finished(identifier, "failed")
        finally:
            if execution_scope is not None:
                execution_scope.__exit__(None, None, None)
            if not publication_committed and created_outputs:
                output_root = (self.repo.home / "outputs").resolve()
                for output in created_outputs:
                    path = (output_root / output["filename"]).resolve()
                    if path.is_relative_to(output_root):
                        try:
                            path.unlink(missing_ok=True)
                            path.with_suffix(".json").unlink(missing_ok=True)
                        except OSError:
                            self.repo.event(
                                identifier,
                                "cleanup_needed",
                                "An unfinished draft file could not be removed. Its generation was not published.",
                            )
            self._cleanup(identifier)

    def _stop(self, run, task_id):
        status = "interrupted" if self.closing else "cancelled"
        with AIConnectionService(self.repo).authority_guard(), self.repo.atomic():
            self.pending.hold_for_tender(run["tender_id"], status)
            if self.repo.get_run(run["id"])["status"] not in {"queued", "running"}:
                return
            self.repo.update_run(
                run["id"],
                status=status,
                detail="Work stopped. Saved source records remain available.",
            )
            self.repo.event(run["id"], status, "Work stopped before completion.")
            from .run_activity import ActivityRecordingError, settle_open_operations

            try:
                settle_open_operations(
                    self.repo,
                    run["id"],
                    "interrupted",
                    "Work stopped before this step had a confirmed completion.",
                )
            except ActivityRecordingError:
                pass
            self.team.cancel_run(run["tender_id"], run["id"], "This work was stopped.")
            record("job_lifecycle", phase=status, outcome="stopped", run_id=run["id"])
            if task_id:
                self.repo.update_task(run["tender_id"], task_id, status=status, run_id=run["id"])
        self.pending.on_run_finished(run["id"], status)

    def cancel(self, run_id):
        run = self.repo.get_run(run_id)
        if run["status"] not in {"queued", "running"}:
            return run
        if event := self.cancelled.get(run_id):
            event.set()
        task_id = next(
            (
                e["data"]["task_id"]
                for e in self.repo.run_events(run_id)
                if e["kind"] in {"assigned_task", "requested_task"}
            ),
            None,
        )
        # Revoke durable authority before returning, while the tracked task
        # continues its provider/lease cleanup. Late usage remains recordable.
        self._stop(run, task_id)
        if (run["status"] == "queued" or run["kind"] not in {"import", "index"}) and (
            task := self.tasks.get(run_id)
        ):
            task.cancel()
        return self.repo.get_run(run_id)

    def resume(self, run_id):
        run = self.repo.get_run(run_id)
        if run["status"] not in {"failed", "cancelled", "interrupted"}:
            raise ValueError("Only stopped or failed work can be resumed.")
        if run["kind"] == "import":
            resumed = self.start_import(run["tender_id"], run["instruction"])
        elif run["kind"] == "index":
            self.indexer.request_refresh(run["tender_id"], force=True)
            raise ValueError(
                "Meaning search is preparing in the background. Exact-word search stays available."
            )
        elif run["kind"] in {"identify", "analysis"}:
            resumed = self.start_analysis(run["tender_id"])
        elif run["kind"] in {"manager", "conversation"}:
            from .ai_policy import AIPolicyService

            policy = AIPolicyService(self.repo)
            with policy.connections.authority_guard(), self.repo.atomic():
                self._preflight_route(run["tender_id"], policy)
                retry = self._queue(run["tender_id"], "manager", run["instruction"])
                selected_task = next(
                    (
                        event["data"]
                        for event in self.repo.run_events(run_id)
                        if event["kind"] == "requested_task"
                    ),
                    None,
                )
                if selected_task:
                    self.repo.event(
                        retry["id"],
                        "requested_task",
                        "Continuing the engineer's selected task after Resume.",
                        selected_task,
                    )
            # Retrying the original accepted instruction creates only a new
            # run. Its engineer message and original submission receipt stay
            # unchanged, and any held pending instruction stays held.
            resumed = self._schedule(retry)
        else:
            raise ValueError("Start a new research request through the Tender Manager.")
        self.repo.event(
            resumed["id"],
            "resumed_from",
            "Resuming from an earlier stopped run.",
            {"previous_run_id": run_id},
        )
        return resumed

    async def close(self):
        self.closing = True
        await self.indexer.close()
        pending = list(self.tasks.values())
        for run_id in list(self.tasks):
            self.cancel(run_id)
        if pending:
            try:
                await asyncio.wait_for(asyncio.gather(*pending, return_exceptions=True), timeout=15)
            except TimeoutError:
                pass
