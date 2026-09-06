"""Durable job state with cooperative cancellation and per-Tender execution lanes."""

import asyncio
import re
import threading
from pathlib import Path

from .intake import import_package
from .office_types import PreparedOfficeResult
from .repository import Repository
from .settings import SettingsService


def safe_error(error: BaseException, secret: str | None = None) -> str:
    message = str(error) or type(error).__name__
    if secret:
        message = message.replace(secret, "[private key]")
    message = re.sub(r"sk-[A-Za-z0-9_-]+", "[private key]", message)
    return message[:2000]


class JobManager:
    def __init__(self, repo: Repository, settings: SettingsService):
        self.repo = repo
        self.settings = settings
        self.tasks: dict[str, asyncio.Task] = {}
        self.cancelled: dict[str, threading.Event] = {}
        self.lanes: dict[str, asyncio.Lock] = {}
        self.closing = False

    def active(self, tender_id):
        return [r for r in self.repo.list_runs(tender_id) if r["status"] in {"queued", "running"}]

    def _start(self, tender_id, kind, instruction, *, task_id=None):
        active = self.active(tender_id)
        if active and (kind != "task" or any(r["kind"] != "task" for r in active)):
            raise ValueError(
                "This Tender already has work in progress. Stop that work before changing its instructions."
            )
        run = self.repo.create_run(tender_id, kind, instruction)
        if task_id:
            self.repo.event(
                run["id"], "assigned_task", "An approved task is queued.", {"task_id": task_id}
            )
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
        return self._start(tender_id, "import", str(source.resolve()))

    def start_index(self, tender_id):
        return self._start(
            tender_id, "index", "Prepare search by meaning for the current documents."
        )

    def start_manager(self, tender_id, instruction, *, engineer_message=True):
        if not self.settings.api_key():
            raise ValueError("Open Settings and add an OpenAI API key to use the Tender Manager.")
        if not instruction.strip():
            raise ValueError("Enter an instruction for the Tender Manager.")
        if self.active(tender_id):
            raise ValueError(
                "Stop the current work before sending a new instruction. Your message has not been sent."
            )
        with self.repo.atomic():
            if engineer_message:
                self.repo.add_message(tender_id, "engineer", instruction)
            return self._start(tender_id, "manager", instruction)

    def start_task(self, tender_id, task_id):
        if not self.settings.api_key():
            raise ValueError(
                "Open Settings and connect the Tender Manager before starting specialist work."
            )
        task = self.repo.get_task(tender_id, task_id)
        plan = self.repo.get_plan(tender_id, task["plan_id"])
        if plan["status"] != "approved" or task["status"] not in {
            "ready",
            "failed",
            "cancelled",
            "interrupted",
        }:
            raise ValueError("This task requires an approved current plan before work can start.")
        self.repo.approved_scope(tender_id, task["plan_id"])
        with self.repo.db.connect() as conn:
            self.repo._check_sources(conn, tender_id, task["source_ids"])
        for run in self.active(tender_id):
            if any(
                event["data"].get("task_id") == task_id for event in self.repo.run_events(run["id"])
            ):
                raise ValueError("This task is already queued or running.")
        return self._start(tender_id, "task", task["description"], task_id=task_id)

    def activate_plan(self, tender_id, plan_id):
        if not self.settings.api_key():
            self.repo.add_message(
                tender_id,
                "system",
                "Your work plan is approved. Connect the Tender Manager in Settings to start the ready tasks.",
            )
            return
        for task in self.repo.get_plan(tender_id, plan_id)["tasks"]:
            if task["status"] == "ready":
                self.start_task(tender_id, task["id"])

    async def _execute(self, run, task_id):
        identifier, tender_id = run["id"], run["tender_id"]
        cancelled = self.cancelled[identifier]
        key = None
        prepared = None
        created_outputs = []
        publication_committed = False
        try:
            async with self.lanes.setdefault(tender_id, asyncio.Lock()):
                if cancelled.is_set():
                    raise InterruptedError("Work stopped before it started.")
                self.repo.update_run(identifier, status="running", detail="Work is starting.")
                if task_id:
                    self.repo.update_task(tender_id, task_id, status="running", run_id=identifier)
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
                else:
                    from .office import run_manager, run_specialist

                    key = self.settings.api_key()
                    if not key:
                        raise ValueError(
                            "The AI connection is missing. Add it in Settings and resume this work."
                        )
                    model = self.settings.public().model
                    if task_id:
                        work = run_specialist(
                            self.repo,
                            tender_id,
                            identifier,
                            self.repo.get_task(tender_id, task_id),
                            key,
                            model,
                        )
                    else:
                        instruction = run["instruction"]
                        work = run_manager(
                            self.repo, tender_id, identifier, instruction, key, model
                        )
                    prepared = await asyncio.wait_for(work, timeout=15 * 60)
                if cancelled.is_set():
                    raise InterruptedError("Work stopped at your request.")
                with self.repo.atomic():
                    if cancelled.is_set():
                        raise InterruptedError("Work stopped before publication.")
                    if run["kind"] in {"manager", "task", "research"}:
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
                    if task_id:
                        self.repo.update_task(
                            tender_id, task_id, status="completed", result=result, run_id=identifier
                        )
                    if prepared and prepared.output.draft_documents:
                        from .outputs import OutputService

                        if not prepared.approved_plan_id:
                            raise ValueError("The draft requests have no approved work scope.")
                        outputs = OutputService(self.repo)
                        for document in prepared.output.draft_documents:
                            created_outputs.append(outputs.generate_from_office(
                                tender_id, document.model_dump(mode="json"), identifier, prepared.approved_plan_id,
                            ))
                        result = result | {"generated_outputs": created_outputs}
                        self.repo.update_run(identifier, result=result)
                    self.repo.event(
                        identifier, "completed", "Work is saved and available for review."
                    )
                publication_committed = True
            if run["kind"] == "import" and self.settings.api_key() and not self.closing:
                self.start_manager(
                    tender_id,
                    "Analyse the imported Tender package. Establish the project scope from evidence, identify coverage gaps and questions, and propose a practical work plan. Use supplied BOQ quantities as the default; do not start a full quantity takeoff without an engineer request.",
                    engineer_message=False,
                )
            elif task_id and self.settings.api_key() and not self.closing and not self.active(tender_id):
                task = self.repo.get_task(tender_id, task_id)
                plan = self.repo.get_plan(tender_id, task["plan_id"])
                if plan["status"] == "approved" and all(item["status"] == "completed" for item in plan["tasks"]):
                    self.repo.event(identifier, "manager_review_requested", "The Tender Manager will consolidate the completed specialist work.", {"plan_id": plan["id"]})
                    self.start_manager(
                        tender_id,
                        "Consolidate the completed specialist results for the approved plan: " + plan["title"] + ". Read the saved task results and their evidence. Bring material assumptions, exclusions, quantity/rate proposals, submission requirements, contradictions and remaining gaps back to the engineer. Do not claim complete Tender review from sampled sources or approve commercial decisions. Retain the current approved scope; propose a further plan only where necessary.",
                        engineer_message=False,
                    )
        except (asyncio.CancelledError, InterruptedError):
            self._stop(run, task_id)
        except Exception as error:
            message = (
                "The task exceeded its 15-minute execution limit. Refine the scope and resume."
                if isinstance(error, TimeoutError)
                else safe_error(error, key)
            )
            with self.repo.atomic():
                current = self.repo.get_run(identifier)
                if current["status"] in {"queued", "running"}:
                    self.repo.update_run(
                        identifier, status="failed", error=message, detail="Work needs attention."
                    )
                    self.repo.event(identifier, "failed", message)
                    if task_id:
                        self.repo.update_task(
                            tender_id, task_id, status="failed", run_id=identifier
                        )
        finally:
            if not publication_committed and created_outputs:
                output_root = (self.repo.home / "outputs").resolve()
                for output in created_outputs:
                    path = (output_root / output["filename"]).resolve()
                    if path.is_relative_to(output_root):
                        try:
                            path.unlink(missing_ok=True)
                            path.with_suffix(".json").unlink(missing_ok=True)
                        except OSError:
                            self.repo.event(identifier, "cleanup_needed", "An unfinished draft file could not be removed. Its generation was not published.")
            self._cleanup(identifier)

    def _stop(self, run, task_id):
        status = "interrupted" if self.closing else "cancelled"
        with self.repo.atomic():
            if self.repo.get_run(run["id"])["status"] not in {"queued", "running"}:
                return
            self.repo.update_run(
                run["id"],
                status=status,
                detail="Work stopped. Saved source records remain available.",
            )
            self.repo.event(run["id"], status, "Work stopped before completion.")
            if task_id:
                self.repo.update_task(run["tender_id"], task_id, status=status, run_id=run["id"])

    def cancel(self, run_id):
        run = self.repo.get_run(run_id)
        if run["status"] not in {"queued", "running"}:
            return run
        if event := self.cancelled.get(run_id):
            event.set()
        if run["status"] == "queued":
            task_id = next(
                (
                    e["data"]["task_id"]
                    for e in self.repo.run_events(run_id)
                    if e["kind"] == "assigned_task"
                ),
                None,
            )
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
            resumed = self.start_index(run["tender_id"])
        elif run["kind"] == "manager":
            resumed = self.start_manager(
                run["tender_id"], run["instruction"], engineer_message=False
            )
        elif run["kind"] == "task":
            task_ids = [
                e["data"]["task_id"] for e in self.repo.run_events(run_id) if "task_id" in e["data"]
            ]
            if not task_ids:
                raise ValueError(
                    "The original task cannot be found. Ask the manager for a revised work plan."
                )
            resumed = self.start_task(run["tender_id"], task_ids[0])
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
        pending = list(self.tasks.values())
        for run_id in list(self.tasks):
            self.cancel(run_id)
        if pending:
            try:
                await asyncio.wait_for(asyncio.gather(*pending, return_exceptions=True), timeout=15)
            except TimeoutError:
                pass
