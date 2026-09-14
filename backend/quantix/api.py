"""Authenticated loopback API for the Quantix desktop interface."""

import asyncio
import hashlib
import hmac
import secrets
import time
from contextlib import asynccontextmanager
from pathlib import Path
from typing import Literal

from fastapi import FastAPI, Query, Request
from fastapi.middleware.cors import CORSMiddleware
from fastapi.responses import FileResponse, JSONResponse, Response
from starlette.middleware.trustedhost import TrustedHostMiddleware

from . import __version__
from . import models as m
from .ai_routes import create_router as create_ai_router
from .ai_runtimes import RuntimeService
from .ai_setup import AISetupService
from .ai_setup_routes import create_router as create_setup_router
from .backup_routes import create_router as create_backup_router
from .correspondence_routes import create_router as create_correspondence_router
from .diagnostic_routes import create_router as create_diagnostic_router
from .diagnostics import diagnostic_context, initialize, record, record_exception, route_template
from .documents import render_pdf_page
from .estimate_routes import create_router as create_estimate_router
from .factory_reset import FactoryResetService, load_journal
from .jobs import JobManager
from .knowledge_routes import create_router as create_knowledge_router
from .manager_routes import create_router as create_manager_router
from .map_routes import create_router as create_map_router
from .measurement_routes import create_router as create_measurement_router
from .pending import (
    PendingInstruction,
    PendingInstructionCancel,
    PendingInstructionConfirm,
    PendingInstructionEdit,
)
from .repository import Repository
from .requirement_routes import create_router as create_requirement_router
from .reset_routes import create_router as create_reset_router
from .retrieval_models import RetrievalResponse
from .retrieval_service import retrieve
from .semantic import SemanticService
from .semantic_models import SemanticStatus
from .settings import SettingsService
from .storage import logs_dir, prepare_process_environment
from .submission_models import ConstructionProgramme, ProgrammeProposalRecord
from .submission_routes import create_router as create_submission_router
from .team_routes import create_router as create_team_router

ALLOWED_ORIGINS = (
    "http://127.0.0.1:1420",
    "http://localhost:1420",
    "tauri://localhost",
    "http://tauri.localhost",
    "https://tauri.localhost",
)


def create_app(home: Path, token: str) -> FastAPI:
    if not token or len(token) < 16:
        raise ValueError("A private local session token is required.")
    # Direct API construction and the normal launcher use the same workspace
    # root. The launcher calls this before importing dependencies; keeping the
    # call here also covers isolated repository/API probes.
    # A confirmed reset reopens only recovery endpoints: constructors otherwise
    # migrate databases and recover interrupted AI setup before middleware runs.
    recovery = load_journal(home) is not None
    home = prepare_process_environment(home)
    repo = None if recovery else Repository(home)
    diagnostics = initialize(logs_dir(home))
    settings = None if recovery else SettingsService(repo)
    jobs = None if recovery else JobManager(repo, settings)
    runtimes = None if recovery else RuntimeService(repo)
    ai_setup = None if recovery else AISetupService(repo)
    semantic = None if recovery else SemanticService(repo)

    def reset_busy():
        blockers = []
        if jobs and any(not task.done() for task in jobs.tasks.values()):
            blockers.append("Finish or stop the current Tender work before resetting Quantix.")
        if jobs and jobs.indexer.busy():
            blockers.append("Finish or stop meaning-search preparation before resetting Quantix.")
        if ai_setup and any(not task.done() for task in ai_setup.tasks.values()):
            blockers.append("Finish or stop AI account setup before resetting Quantix.")
        from .ai_connections import AIConnectionService

        with AIConnectionService._states_lock:
            state = AIConnectionService._states.get(str(home))
            if state:
                with state.lock:
                    if state.leases or state.exclusive:
                        blockers.append(
                            "An AI account is still in use. Finish or stop its work before resetting Quantix."
                        )
        worker = getattr(repo, "_ai_worker_client", None)
        if worker and worker.executions:
            blockers.append("Finish or stop current AI work before resetting Quantix.")
        return blockers

    async def close_clients():
        if jobs:
            await jobs.close()
        from .embedding_runtime import release_runtimes

        release_runtimes(home)
        if ai_setup:
            await ai_setup.close()
        if runtimes:
            await runtimes.close()

    reset = FactoryResetService(home, repo=repo, busy=reset_busy, close_clients=close_clients)

    @asynccontextmanager
    async def lifespan(app):
        if repo and not reset.pending:
            repo.recover_interrupted_runs()
            from .team import TeamService

            TeamService(repo).recover_interrupted()
            try:
                from .ai_reservations import release_rejected_reservations

                release_rejected_reservations(repo)
            except Exception as error:  # A repair must never block startup.
                diagnostics.record_exception("ai_reservation_repair_failed", error)
            try:
                from .ai_policy_repair import repair_orphaned_accounts

                repair_orphaned_accounts(repo)
            except Exception as error:  # A repair must never block startup.
                diagnostics.record_exception("ai_policy_repair_failed", error)
            jobs.indexer.bind_loop(asyncio.get_running_loop())
            jobs.indexer.recover()
        yield
        await close_clients()
        diagnostics.close()

    app = FastAPI(title="Quantix local workspace", version=__version__, lifespan=lifespan)
    app.state.repo, app.state.jobs, app.state.diagnostics = repo, jobs, diagnostics
    app.state.reset = reset

    @app.middleware("http")
    async def authenticate(request: Request, call_next):
        if request.url.path.startswith("/api") and request.method != "OPTIONS":
            supplied = request.headers.get("authorization", "")
            if not hmac.compare_digest(supplied, f"Bearer {token}"):
                return JSONResponse(
                    {"detail": "This local session is not authorised. Reopen Quantix."},
                    status_code=401,
                )
        try:
            tracked = reset.enter_request(request.method, request.url.path)
        except ValueError as error:
            return JSONResponse({"detail": str(error)}, status_code=409)
        try:
            response = await call_next(request)
        finally:
            reset.leave_request(tracked)
        response.headers["X-Content-Type-Options"] = "nosniff"
        response.headers["Referrer-Policy"] = "no-referrer"
        return response

    @app.exception_handler(KeyError)
    async def not_found(request, error):
        return JSONResponse(
            {"detail": "This item could not be found in the selected Tender."}, status_code=404
        )

    @app.exception_handler(ValueError)
    async def invalid_action(request, error):
        return JSONResponse({"detail": str(error)}, status_code=409)

    from fastapi.exceptions import RequestValidationError

    @app.exception_handler(RequestValidationError)
    async def invalid_request(request, error):
        # Validation errors can otherwise echo write-only credentials in their
        # raw input/context, including a whole rejected connection body.
        return JSONResponse(
            {
                "detail": [
                    {"loc": item["loc"], "msg": item["msg"], "type": item["type"]}
                    for item in error.errors()
                ]
            },
            status_code=422,
        )

    @app.get("/healthz")
    def healthz():
        return {"ready": True, "version": __version__}

    def engine_capabilities():
        from .engines import verify
        from .tesseract_runtime import tesseract_executable, tesseract_languages

        bundled = verify()
        ocr = tesseract_executable() is not None and {"eng", "ara"} <= tesseract_languages()
        return (["ocr_ready"] if ocr else []) + (["meaning_ready"] if bundled["meaning"] else [])

    @app.get("/api/health", response_model=m.Health)
    def health():
        if reset.pending:
            return m.Health(
                version=__version__,
                provider_ready=False,
                model="",
                home=str(home),
                reset_pending=True,
                capabilities=["factory_reset"] if reset.supported else [],
            )
        public = settings.public()
        return m.Health(
            version=__version__,
            provider_ready=public.provider_ready,
            model=public.model,
            home=public.home,
            workspace_revision=2,
            reset_pending=False,
            capabilities=(["factory_reset"] if reset.supported else [])
            + [
                "run_activity",
                "manager_profile",
                "estimates",
                "outputs",
                "meaning_search",
                "backups",
                "quotations",
                "knowledge",
                "takeoff",
                "submissions",
                "project_map",
                "submission_requirements",
                "ai_connections",
                "guided_ai_setup",
                "generation_controls",
                "agent_chat",
                "team",
                "work_products",
                "tender_profile",
                "company_library",
            ]
            + engine_capabilities(),
        )

    @app.get("/api/settings", response_model=m.Settings)
    def get_settings():
        return settings.public()

    @app.patch("/api/settings", response_model=m.Settings)
    def update_settings(command: m.SettingsPatch):
        return settings.update(command)

    @app.get("/api/tenders", response_model=list[m.Tender])
    def list_tenders():
        return repo.list_tenders()

    @app.post("/api/tenders", response_model=m.Tender)
    def create_tender(command: m.CreateTender):
        return repo.create_tender(command.name)

    @app.patch("/api/tenders/{tender_id}", response_model=m.Tender)
    def rename_tender(tender_id: str, command: m.RenameTender):
        return repo.rename_tender(tender_id, command.name, source="engineer")

    @app.get("/api/tenders/{tender_id}", response_model=m.Overview)
    def overview(tender_id: str):
        return repo.overview(tender_id)

    @app.post("/api/tenders/{tender_id}/imports", response_model=m.Run)
    async def start_import(tender_id: str, command: m.ImportRequest):
        return jobs.start_import(tender_id, command.source_path)

    @app.get("/api/tenders/{tender_id}/artifacts", response_model=list[m.Artifact])
    def artifacts(tender_id: str, include_history: bool = False):
        return repo.list_artifacts(tender_id, current_only=not include_history)

    @app.get("/api/tenders/{tender_id}/artifacts/{artifact_id}", response_model=m.Artifact)
    def artifact(tender_id: str, artifact_id: str):
        return repo.get_artifact(tender_id, artifact_id)

    @app.get("/api/tenders/{tender_id}/artifacts/{artifact_id}/original")
    def original(tender_id: str, artifact_id: str):
        saved = repo.get_artifact(tender_id, artifact_id)
        path = repo.object_path(tender_id, artifact_id)
        with path.open("rb") as stream:
            if hashlib.file_digest(stream, "sha256").hexdigest() != saved["content_hash"]:
                raise ValueError(
                    "The preserved original changed. Restore a checked backup before using this file."
                )
        return FileResponse(path, filename=saved["name"], media_type="application/octet-stream")

    @app.get(
        "/api/tenders/{tender_id}/artifacts/{artifact_id}/evidence", response_model=list[m.Evidence]
    )
    def artifact_evidence(
        tender_id: str,
        artifact_id: str,
        offset: int = Query(0, ge=0),
        limit: int = Query(50, ge=1, le=200),
        sheet: str | None = Query(None, max_length=300),
        cell_range: str | None = Query(None, max_length=80),
    ):
        return repo.artifact_evidence(
            tender_id, artifact_id, offset, limit, sheet=sheet, cell_range=cell_range
        )

    @app.get("/api/tenders/{tender_id}/artifacts/{artifact_id}/preview")
    async def preview(tender_id: str, artifact_id: str, page: int = Query(1, ge=1)):
        artifact = repo.get_artifact(tender_id, artifact_id)
        if artifact["kind"] != "pdf":
            raise ValueError(
                "A page preview is available for PDF sources. Open the source rows for this document."
            )
        source = repo.object_path(tender_id, artifact_id)
        png = await asyncio.to_thread(render_pdf_page, source, page)
        return Response(
            png, media_type="image/png", headers={"Cache-Control": "private,max-age=60"}
        )

    @app.get("/api/tenders/{tender_id}/evidence/{evidence_id}", response_model=m.Evidence)
    def evidence(tender_id: str, evidence_id: str):
        return repo.get_evidence(tender_id, evidence_id)

    @app.get("/api/tenders/{tender_id}/search", response_model=RetrievalResponse)
    def search(
        tender_id: str,
        q: str = Query("", max_length=1000),
        mode: Literal["auto", "words", "meaning", "combined"] = "auto",
        area: str | None = Query(None, max_length=300),
        status: str | None = Query(None, max_length=50),
        document_kind: str | None = Query(None, max_length=50),
        limit: int = Query(20, ge=1, le=50),
        cursor: str | None = Query(None, max_length=2000),
    ):
        return retrieve(
            repo,
            tender_id,
            q,
            mode=mode,
            limit=limit,
            cursor=cursor,
            semantic=semantic,
            area=area,
            status=status,
            document_kind=document_kind,
        )

    @app.get("/api/tenders/{tender_id}/search-status", response_model=SemanticStatus)
    def search_status(tender_id: str):
        return jobs.indexer.status(tender_id)

    @app.post("/api/tenders/{tender_id}/search-index", response_model=SemanticStatus)
    async def prepare_search(tender_id: str):
        return jobs.start_index(tender_id)

    @app.post("/api/tenders/{tender_id}/search-index/cancel", response_model=SemanticStatus)
    async def cancel_search_index(tender_id: str):
        return jobs.cancel_index(tender_id)

    @app.get(
        "/api/tenders/{tender_id}/messages",
        response_model=list[m.Message] | m.MessagePage,
    )
    def messages(
        tender_id: str,
        limit: int | None = Query(None, ge=1, le=100),
        cursor: str | None = Query(None, max_length=100),
    ):
        if limit is None and cursor is None:
            return repo.messages(tender_id)
        return repo.message_page(tender_id, limit=limit or 50, cursor=cursor)

    @app.post("/api/tenders/{tender_id}/messages", response_model=m.MessageSubmission)
    async def send_message(tender_id: str, command: m.MessageRequest):
        return jobs.submit_message(
            tender_id,
            command.content,
            idempotency_key=command.idempotency_key,
            action=command.action,
        )

    @app.get(
        "/api/tenders/{tender_id}/pending-message",
        response_model=PendingInstruction | None,
    )
    def pending_message(tender_id: str):
        return jobs.pending.get(tender_id)

    @app.patch(
        "/api/tenders/{tender_id}/pending-message",
        response_model=PendingInstruction,
    )
    def edit_pending_message(
        tender_id: str,
        command: PendingInstructionEdit,
    ):
        return jobs.edit_pending(
            tender_id,
            command.content,
            pending_id=command.pending_id,
            expected_revision=command.expected_revision,
            action=command.action,
        )

    @app.delete("/api/tenders/{tender_id}/pending-message", response_model=m.MutationReceipt)
    def cancel_pending_message(
        tender_id: str,
        command: PendingInstructionCancel,
    ):
        return jobs.cancel_pending(
            tender_id,
            pending_id=command.pending_id,
            expected_revision=command.expected_revision,
        )

    @app.post(
        "/api/tenders/{tender_id}/pending-message/confirm",
        response_model=m.MessageSubmission,
    )
    async def confirm_pending_message(tender_id: str, command: PendingInstructionConfirm):
        return jobs.consume_pending(
            tender_id,
            pending_id=command.pending_id,
            expected_revision=command.expected_revision,
            explicit=True,
        )

    @app.get("/api/tenders/{tender_id}/findings", response_model=list[m.Finding])
    def findings(tender_id: str):
        return repo.list_findings(tender_id)

    @app.post("/api/tenders/{tender_id}/findings/{finding_id}/decision", response_model=m.Finding)
    def decide_finding(tender_id: str, finding_id: str, command: m.DecisionRequest):
        return repo.decide_finding(tender_id, finding_id, command.decision, command.rationale)

    @app.get("/api/tenders/{tender_id}/plans", response_model=list[m.WorkPlan])
    def plans(tender_id: str):
        return repo.list_plans(tender_id)

    @app.post("/api/tenders/{tender_id}/plans/{plan_id}/approve", response_model=m.WorkPlan)
    async def approve_plan(tender_id: str, plan_id: str, command: m.ApprovalRequest):
        if jobs.active(tender_id):
            raise ValueError("Finish or stop the current Tender work before approving a new plan.")
        plan = repo.approve_plan(tender_id, plan_id, command.rationale)
        jobs.carry_out_plan(tender_id, plan_id)
        return plan

    @app.get("/api/tenders/{tender_id}/tasks", response_model=list[m.Task])
    def tasks(tender_id: str):
        return repo.list_tasks(tender_id)

    @app.post("/api/tenders/{tender_id}/work/stop", response_model=m.MutationReceipt)
    def stop_tender_work(tender_id: str):
        return jobs.stop_tender_work(tender_id)

    @app.get(
        "/api/tenders/{tender_id}/programme-proposals", response_model=list[ProgrammeProposalRecord]
    )
    def programme_proposals(tender_id: str):
        proposals = []
        for run in repo.list_runs(tender_id):
            if run["status"] != "completed" or not run["result"].get("programme_proposal"):
                continue
            programme = ConstructionProgramme.model_validate(run["result"]["programme_proposal"])
            source_ids = sorted(
                {sid for activity in programme.activities for sid in activity.source_ids}
            )
            current = True
            with repo.db.connect() as conn:
                try:
                    repo._check_sources(conn, tender_id, source_ids)
                except (ValueError, KeyError):
                    current = False
            proposals.append(
                {
                    "run_id": run["id"],
                    "created_at": run["updated_at"],
                    "source_ids": source_ids,
                    "is_current": current,
                    "programme": programme.model_dump(mode="json"),
                }
            )
        return proposals

    @app.post("/api/tenders/{tender_id}/tasks/{task_id}/run", response_model=m.Run)
    async def start_task(tender_id: str, task_id: str):
        return jobs.start_task(tender_id, task_id)

    @app.get("/api/tenders/{tender_id}/runs", response_model=list[m.Run])
    def runs(tender_id: str):
        return repo.list_runs(tender_id)

    @app.get("/api/runs/{run_id}", response_model=m.Run)
    def run(run_id: str):
        return repo.get_run(run_id)

    @app.get("/api/runs/{run_id}/events", response_model=list[m.RunEvent])
    def events(run_id: str):
        return repo.run_events(run_id)

    @app.post("/api/runs/{run_id}/cancel", response_model=m.Run)
    async def cancel(run_id: str):
        return jobs.cancel(run_id)

    @app.post("/api/runs/{run_id}/resume", response_model=m.Run)
    async def resume(run_id: str):
        return jobs.resume(run_id)

    @app.post("/api/shutdown", response_model=m.MutationReceipt)
    async def shutdown():
        callback = getattr(app.state, "request_shutdown", None)
        if callback is None:
            raise ValueError("This service is managed by its test host.")
        callback()
        return {"ok": True}

    app.add_middleware(
        CORSMiddleware,
        allow_origins=ALLOWED_ORIGINS,
        allow_methods=["GET", "POST", "PATCH", "PUT", "DELETE"],
        allow_headers=["Authorization", "Content-Type"],
        expose_headers=["X-Quantix-Request-Id"],
    )
    app.add_middleware(
        TrustedHostMiddleware, allowed_hosts=["127.0.0.1", "localhost", "testserver"]
    )

    @app.middleware("http")
    async def enforce_origin(request: Request, call_next):
        # Outside CORS so disallowed preflights also stop before any route executes.
        origin = request.headers.get("origin")
        if (
            request.url.path.startswith("/api")
            and origin is not None
            and origin not in ALLOWED_ORIGINS
        ):
            return JSONResponse(
                {"detail": "This browser origin is not authorised to use the local workspace."},
                status_code=403,
            )
        return await call_next(request)

    @app.middleware("http")
    async def diagnostic_requests(request: Request, call_next):
        request_id = secrets.token_hex(16)
        started = time.monotonic()
        with diagnostic_context(request_id=request_id):
            try:
                response = await call_next(request)
            except Exception as error:
                # Keep unexpected errors useful to support while avoiding raw
                # provider messages, URLs, paths, prompts or request bodies.
                route = route_template(request.scope)
                record_exception(
                    "api_unexpected_error",
                    error,
                    route=route,
                    method=request.method,
                    phase="request",
                    outcome="error",
                    error_reference=request_id,
                )
                response = JSONResponse(
                    {
                        "detail": "Quantix could not complete that request. Use the request reference when asking for support.",
                        "error_reference": request_id,
                    },
                    status_code=500,
                )
                origin = request.headers.get("origin")
                if origin in ALLOWED_ORIGINS:
                    response.headers["Access-Control-Allow-Origin"] = origin
                    response.headers["Access-Control-Expose-Headers"] = "X-Quantix-Request-Id"
                    response.headers["Vary"] = "Origin"
            response.headers["X-Quantix-Request-Id"] = request_id
            response.headers["X-Content-Type-Options"] = "nosniff"
            response.headers["Referrer-Policy"] = "no-referrer"
            if request.method != "OPTIONS":
                response.headers.setdefault("Cache-Control", "no-store")
            record(
                "api_request",
                route=route_template(request.scope),
                method=request.method,
                http_status=response.status_code,
                duration_ms=int((time.monotonic() - started) * 1000),
                phase="request",
                outcome="success" if response.status_code < 400 else "handled_error",
                error_reference=request_id if response.status_code >= 400 else None,
            )
            return response

    app.include_router(create_diagnostic_router(diagnostics))
    app.include_router(create_reset_router(reset))
    if recovery:
        return app

    app.include_router(create_manager_router(repo))
    app.include_router(create_team_router(repo))
    app.include_router(create_estimate_router(repo))
    app.include_router(create_backup_router(repo))
    app.include_router(create_correspondence_router(repo))
    app.include_router(create_knowledge_router(repo))
    app.include_router(create_measurement_router(repo))
    from .takeoff_routes import create_router as create_takeoff_router

    app.include_router(create_takeoff_router(repo))
    app.include_router(create_map_router(repo))
    app.include_router(create_requirement_router(repo))
    app.include_router(create_ai_router(repo, runtimes, ai_setup))
    app.include_router(create_setup_router(repo, ai_setup, on_tender_ai=jobs.maybe_analyze))
    from .ai_generation_routes import create_router as create_generation_router

    app.include_router(create_generation_router(repo))
    from .chat_stream_routes import create_router as create_chat_stream_router

    app.include_router(create_chat_stream_router(repo, should_stop=lambda: reset.pending))
    from .run_activity_routes import create_router as create_activity_router

    app.include_router(create_activity_router(repo, should_stop=lambda: reset.pending))
    app.include_router(create_submission_router(repo))
    from .later_routes import create_router as create_later_router

    app.include_router(create_later_router(repo))

    # The event body is parsed manually after its size cap, so FastAPI cannot
    # infer the request model from the handler signature. Keep the generated
    # OpenAPI component sourced from the same strict Pydantic contract.
    from .diagnostic_models import DiagnosticEventInput

    _openapi = app.openapi

    def openapi_with_diagnostics():
        schema = _openapi()
        schema.setdefault("components", {}).setdefault("schemas", {})["DiagnosticEventInput"] = (
            DiagnosticEventInput.model_json_schema(ref_template="#/components/schemas/{model}")
        )
        return schema

    app.openapi = openapi_with_diagnostics
    return app
