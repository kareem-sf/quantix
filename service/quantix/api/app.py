import secrets
from contextlib import asynccontextmanager
from pathlib import Path

from fastapi import Depends, FastAPI, Header, HTTPException

from quantix.api import tenders
from quantix.core.db import open_database


def create_app(home: Path, token: str) -> FastAPI:
    """Build the service for one data home. Every route except /health needs the launch token."""

    def require_token(authorization: str = Header(default="")) -> None:
        if not secrets.compare_digest(authorization, f"Bearer {token}"):
            raise HTTPException(status_code=401, detail="Missing or wrong access token.")

    @asynccontextmanager
    async def lifespan(app: FastAPI):
        app.state.sessions = open_database(home)
        yield
        app.state.sessions.kw["bind"].dispose()

    app = FastAPI(title="Quantix", version="0.1.0", lifespan=lifespan)

    @app.get("/health", tags=["service"])
    def health() -> dict[str, str]:
        return {"status": "ok"}

    app.include_router(tenders.router, dependencies=[Depends(require_token)])
    return app
