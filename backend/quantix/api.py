"""Authenticated loopback API for the Quantix desktop interface."""

import asyncio
import hashlib
import hmac
from contextlib import asynccontextmanager
from pathlib import Path
from typing import Literal

from fastapi import FastAPI, Query, Request
from fastapi.middleware.cors import CORSMiddleware
from fastapi.responses import FileResponse, JSONResponse, Response
from starlette.middleware.trustedhost import TrustedHostMiddleware

from . import __version__
from . import models as m
from .backup_routes import create_router as create_backup_router
from .correspondence_routes import create_router as create_correspondence_router
from .documents import render_pdf_page
from .estimate_routes import create_router as create_estimate_router
from .jobs import JobManager
from .knowledge_routes import create_router as create_knowledge_router
from .map_routes import create_router as create_map_router
from .measurement_routes import create_router as create_measurement_router
from .repository import Repository
from .requirement_routes import create_router as create_requirement_router
from .semantic import SemanticService
from .semantic_models import SemanticStatus
from .settings import SettingsService
from .submission_routes import create_router as create_submission_router
from .submission_models import ConstructionProgramme, ProgrammeProposalRecord

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
    repo = Repository(home)
    settings = SettingsService(repo)
    jobs = JobManager(repo, settings)
    semantic = SemanticService(repo)

    @asynccontextmanager
    async def lifespan(app):
        repo.recover_interrupted_runs()
        yield
        await jobs.close()

    app = FastAPI(title="Quantix local workspace", version=__version__, lifespan=lifespan)
    app.state.repo, app.state.jobs = repo, jobs

    @app.middleware("http")
    async def authenticate(request: Request, call_next):
        if request.url.path.startswith("/api") and request.method != "OPTIONS":
            supplied = request.headers.get("authorization", "")
            if not hmac.compare_digest(supplied, f"Bearer {token}"):
                return JSONResponse(
                    {"detail": "This local session is not authorised. Reopen Quantix."},
                    status_code=401,
                )
        response = await call_next(request)
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

    @app.get("/healthz")
    def healthz():
        return {"ready": True, "version": __version__}

    @app.get("/api/health", response_model=m.Health)
    def health():
        public = settings.public()
        return m.Health(
            version=__version__,
            provider_ready=public.provider_ready,
            model=public.model,
            home=public.home,
            capabilities=[
                "estimates",
                "outputs",
                "meaning_search",
                "backups",
                "quotations",
                "knowledge",
                "measurements",
                "submissions",
                "project_map",
                "submission_requirements",
            ],
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
    ):
        return repo.artifact_evidence(tender_id, artifact_id, offset, limit)

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

    @app.get("/api/tenders/{tender_id}/search", response_model=list[m.Evidence])
    def search(
        tender_id: str,
        q: str = Query("", max_length=1000),
        mode: Literal["words", "meaning", "combined"] = "words",
        area: str | None = Query(None, max_length=300),
        status: str | None = Query(None, max_length=50),
    ):
        if not q.strip():
            return []
        if mode == "words":
            return repo.search(tender_id, q, area=area, status=status)
        meaning = semantic.search(tender_id, q, area=area, status=status, collapse_duplicates=True)
        if mode == "meaning":
            return meaning
        artifacts_by_id = {a["id"]: a for a in repo.list_artifacts(tender_id)}
        ranked = {}
        for matches in (meaning, repo.search(tender_id, q, area=area, status=status)):
            for rank, hit in enumerate(matches):
                artifact = artifacts_by_id[hit["artifact_id"]]
                identity = (artifact["content_hash"], hit["locator"])
                stored, score = ranked.get(identity, (hit, 0.0))
                ranked[identity] = (stored, score + 1 / (60 + rank + 1))
        return [
            hit | {"score": score}
            for hit, score in sorted(ranked.values(), key=lambda item: -item[1])[:20]
        ]

    @app.get("/api/tenders/{tender_id}/search-status", response_model=SemanticStatus)
    def search_status(tender_id: str):
        return semantic.status(tender_id)

    @app.post("/api/tenders/{tender_id}/search-index", response_model=m.Run)
    async def prepare_search(tender_id: str):
        return jobs.start_index(tender_id)

    @app.get("/api/tenders/{tender_id}/messages", response_model=list[m.Message])
    def messages(tender_id: str):
        return repo.messages(tender_id)

    @app.post("/api/tenders/{tender_id}/messages", response_model=m.Run)
    async def send_message(tender_id: str, command: m.MessageRequest):
        return jobs.start_manager(tender_id, command.content)

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
        jobs.activate_plan(tender_id, plan_id)
        return plan

    @app.get("/api/tenders/{tender_id}/tasks", response_model=list[m.Task])
    def tasks(tender_id: str):
        return repo.list_tasks(tender_id)

    @app.post("/api/tenders/{tender_id}/work/stop", response_model=m.MutationReceipt)
    def stop_tender_work(tender_id: str):
        repo.get_tender(tender_id)
        for active in jobs.active(tender_id):
            jobs.cancel(active["id"])
        return {"ok": True}

    @app.get("/api/tenders/{tender_id}/programme-proposals", response_model=list[ProgrammeProposalRecord])
    def programme_proposals(tender_id: str):
        proposals = []
        for run in repo.list_runs(tender_id):
            if run["status"] != "completed" or not run["result"].get("programme_proposal"):
                continue
            programme = ConstructionProgramme.model_validate(run["result"]["programme_proposal"])
            source_ids = sorted({sid for activity in programme.activities for sid in activity.source_ids})
            current = True
            with repo.db.connect() as conn:
                try:
                    repo._check_sources(conn, tender_id, source_ids)
                except (ValueError, KeyError):
                    current = False
            proposals.append({"run_id": run["id"], "created_at": run["updated_at"], "source_ids": source_ids, "is_current": current, "programme": programme.model_dump(mode="json")})
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

    app.include_router(create_estimate_router(repo))
    app.include_router(create_backup_router(repo))
    app.include_router(create_correspondence_router(repo))
    app.include_router(create_knowledge_router(repo))
    app.include_router(create_measurement_router(repo))
    app.include_router(create_map_router(repo))
    app.include_router(create_requirement_router(repo))
    app.include_router(create_submission_router(repo))
    return app
