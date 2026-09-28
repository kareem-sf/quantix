import secrets
from contextlib import asynccontextmanager
from pathlib import Path

from fastapi import Depends, FastAPI, Header, HTTPException

from quantix.api import (
    ai,
    boq,
    company,
    documents,
    drawings,
    estimate,
    office,
    review,
    subcontract,
    submission,
    takeoff,
    tenders,
)
from quantix.core.db import open_database
from quantix.documents.library import Reader
from quantix.documents.meaning import Indexer
from quantix.documents.ocr import Ocr
from quantix.office.runtime import Office
from quantix.review import revisions


def create_app(home: Path, token: str) -> FastAPI:
    """Build the service for one data home. Every route except /health needs the launch token."""

    def require_token(authorization: str = Header(default="")) -> None:
        if not secrets.compare_digest(authorization, f"Bearer {token}"):
            raise HTTPException(status_code=401, detail="Missing or wrong access token.")

    @asynccontextmanager
    async def lifespan(app: FastAPI):
        app.state.sessions = open_database(home)
        app.state.office = Office(home, app.state.sessions)
        app.state.indexer = Indexer(home, app.state.sessions)
        # scanned pages are read by OCR once their document is read, and then join the meaning index
        app.state.ocr = Ocr(home, app.state.sessions, after_page=app.state.indexer.wake)

        def after_read() -> None:
            app.state.office.wake()
            app.state.indexer.wake()
            app.state.ocr.wake()

        # a newer copy of a document takes over the work whose source is unchanged, and wakes the office for the rest
        app.state.reader = Reader(
            home,
            app.state.sessions,
            on_read=lambda session, document: revisions.carry_over(session, home, document),
            after_read=after_read,
        )
        app.state.reader.start()
        app.state.indexer.start()
        app.state.ocr.start()
        app.state.office.start()
        yield
        app.state.office.close()
        app.state.reader.stop()
        app.state.indexer.stop()
        app.state.ocr.stop()
        app.state.sessions.kw["bind"].dispose()

    app = FastAPI(title="Quantix", version="0.1.0", lifespan=lifespan)
    app.state.home = home

    @app.get("/health", tags=["service"])
    def health() -> dict[str, str]:
        return {"status": "ok"}

    app.include_router(tenders.router, dependencies=[Depends(require_token)])
    app.include_router(ai.router, dependencies=[Depends(require_token)])
    app.include_router(documents.router, dependencies=[Depends(require_token)])
    app.include_router(office.router, dependencies=[Depends(require_token)])
    app.include_router(boq.router, dependencies=[Depends(require_token)])
    app.include_router(takeoff.router, dependencies=[Depends(require_token)])
    app.include_router(drawings.router, dependencies=[Depends(require_token)])
    app.include_router(estimate.router, dependencies=[Depends(require_token)])
    app.include_router(subcontract.router, dependencies=[Depends(require_token)])
    app.include_router(submission.router, dependencies=[Depends(require_token)])
    app.include_router(company.router, dependencies=[Depends(require_token)])
    app.include_router(review.router, dependencies=[Depends(require_token)])
    return app
