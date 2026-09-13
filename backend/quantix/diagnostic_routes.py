"""Authenticated local diagnostics endpoints."""

from fastapi import APIRouter, Request
from fastapi.responses import JSONResponse
from pydantic import ValidationError

from .diagnostic_models import DiagnosticEventAck, DiagnosticEventInput, DiagnosticsStatus

MAX_EVENT_BODY_BYTES = 16 * 1024


def create_router(diagnostics):
    router = APIRouter(prefix="/api", tags=["Diagnostics"])

    @router.get("/diagnostics", response_model=DiagnosticsStatus)
    def status():
        return diagnostics.status()

    @router.post(
        "/diagnostics/events",
        response_model=DiagnosticEventAck,
        openapi_extra={
            "requestBody": {
                "required": True,
                "content": {
                    "application/json": {
                        "schema": {
                            "$ref": "#/components/schemas/DiagnosticEventInput",
                        }
                    }
                },
            }
        },
    )
    async def renderer_event(request: Request):
        # Read and bound the body before invoking Pydantic. A rejected payload
        # cannot be reflected through validation details or an echoed body.
        content_length = request.headers.get("content-length")
        try:
            oversized = content_length is not None and int(content_length) > MAX_EVENT_BODY_BYTES
        except ValueError:
            oversized = True
        if oversized:
            return JSONResponse({"detail": "Diagnostic event is too large."}, status_code=413)
        chunks = []
        size = 0
        async for chunk in request.stream():
            size += len(chunk)
            if size > MAX_EVENT_BODY_BYTES:
                return JSONResponse({"detail": "Diagnostic event is too large."}, status_code=413)
            chunks.append(chunk)
        body = b"".join(chunks)
        try:
            value = DiagnosticEventInput.model_validate_json(body)
        except (ValidationError, ValueError, TypeError):
            return JSONResponse({"detail": "Diagnostic event is invalid."}, status_code=422)
        return {"recorded": bool(diagnostics.accept_renderer_event(value))}

    return router
