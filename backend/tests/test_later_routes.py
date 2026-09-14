"""HTTP fences for Tender-scoped extraction reprocessing."""

from __future__ import annotations

import hashlib
import hmac

from fastapi import FastAPI, Request
from fastapi.responses import JSONResponse
from fastapi.testclient import TestClient

from quantix.later_routes import create_router
from quantix.repository import Repository

TOKEN = "synthetic-later-session"


def _app(repo):
    app = FastAPI()

    @app.middleware("http")
    async def authenticate(request: Request, call_next):
        if not hmac.compare_digest(request.headers.get("authorization", ""), f"Bearer {TOKEN}"):
            return JSONResponse({"detail": "Not authorised."}, status_code=401)
        return await call_next(request)

    app.include_router(create_router(repo))
    return app


def _pdf_bytes() -> bytes:
    return (
        b"%PDF-1.4\n1 0 obj<< /Type /Catalog /Pages 2 0 R >>endobj\n"
        b"2 0 obj<< /Type /Pages /Kids [3 0 R] /Count 1 >>endobj\n"
        b"3 0 obj<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 100] "
        b"/Contents 4 0 R >>endobj\n4 0 obj<< /Length 0 >>stream\nendstream\n"
        b"endobj\ntrailer<< /Root 1 0 R >>\n%%EOF\n"
    )


def test_reprocess_requires_selected_tender_membership_hash_and_bounded_pages(tmp_path):
    repo = Repository(tmp_path / "home")
    selected = repo.create_tender("Selected Tender")
    other = repo.create_tender("Other Tender")
    body = _pdf_bytes()
    digest = hashlib.sha256(body).hexdigest()
    (repo.objects / digest).write_bytes(body)
    repo.register_artifact(
        selected["id"],
        "Sources/scan.pdf",
        digest,
        len(body),
        {"kind": "pdf", "status": "extracted", "segments": []},
    )
    headers = {"Authorization": f"Bearer {TOKEN}"}

    with TestClient(_app(repo)) as client:
        unknown_tender = client.post(
            "/api/tenders/missing-tender/extractions/reprocess",
            headers=headers,
            json={"original_hash": digest},
        )
        assert unknown_tender.status_code == 404

        cross_tender = client.post(
            f"/api/tenders/{other['id']}/extractions/reprocess",
            headers=headers,
            json={"original_hash": digest},
        )
        assert cross_tender.status_code in {404, 409}

        malformed_hash = client.post(
            f"/api/tenders/{selected['id']}/extractions/reprocess",
            headers=headers,
            json={"original_hash": "not-a-sha256"},
        )
        assert malformed_hash.status_code == 409

        for page_limit in (0, 2001):
            bounded = client.post(
                f"/api/tenders/{selected['id']}/extractions/reprocess",
                headers=headers,
                json={"original_hash": digest, "page_limit": page_limit},
            )
            assert bounded.status_code == 409

        accepted = client.post(
            f"/api/tenders/{selected['id']}/extractions/reprocess",
            headers=headers,
            json={"original_hash": digest, "page_limit": 1},
        )
        assert accepted.status_code == 200, accepted.text
        assert accepted.json()["original_hash_unchanged"] is True

        (repo.objects / digest).write_bytes(b"tampered source bytes")
        tampered = client.post(
            f"/api/tenders/{selected['id']}/extractions/reprocess",
            headers=headers,
            json={"original_hash": digest},
        )
        assert tampered.status_code == 409
