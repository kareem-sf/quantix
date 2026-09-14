"""Authenticated Manager profile API contract tests."""

import hmac

import pytest
from fastapi import FastAPI, Request
from fastapi.responses import JSONResponse
from fastapi.testclient import TestClient

from quantix.manager_routes import create_router
from quantix.repository import Repository

TOKEN = "synthetic-manager-session"


def _personality():
    return {
        "description": "A precise construction coordinator.",
        "traits": ["careful", "direct"],
        "communication_style": "State the decision and the next action first.",
        "problem_solving_style": "Check each issue against the available source.",
        "collaboration_style": "Ask focused colleagues for bounded work.",
        "uncertainty_handling": "Label assumptions and name the missing evidence.",
        "initiative": "Suggest one safe follow-up when the evidence supports it.",
        "explanation_style": "Use plain language with concise supporting detail.",
        "language_preferences": ["English", "Arabic"],
        "working_habits": ["Keep source references beside material findings"],
    }


def _edit(version, **changes):
    value = {
        "expected_version": version,
        "display_name": "Rania Haddad",
        "title": "Commercial Tender Coordinator",
        "persona": "Keep the engineer in control of each decision.",
        "personality": _personality(),
        "working_preferences": ["Show assumptions", "State one next action"],
    }
    value.update(changes)
    return value


@pytest.fixture(autouse=True)
def close_temporary_diagnostics():
    """Close only a writer created by a Repository in this test."""

    from quantix import diagnostics

    before = diagnostics._writer
    yield
    after = diagnostics._writer
    if after is not None and after is not before:
        after.close()


def _app(home):
    repo = Repository(home)
    app = FastAPI()

    @app.middleware("http")
    async def authenticate(request: Request, call_next):
        if not hmac.compare_digest(request.headers.get("authorization", ""), f"Bearer {TOKEN}"):
            return JSONResponse({"detail": "Not authorised."}, status_code=401)
        return await call_next(request)

    app.include_router(create_router(repo))
    return app, repo


def test_manager_profile_routes_require_outer_session_auth(tmp_path):
    app, _repo = _app(tmp_path / "home")
    with TestClient(app) as client:
        assert client.get("/api/manager-profile").status_code == 401
        response = client.get("/api/manager-profile", headers={"Authorization": f"Bearer {TOKEN}"})
    assert response.status_code == 200
    assert response.json()["display_name"] == "Tender Manager"


def test_manager_profile_patch_persists_complete_arbitrary_personality(tmp_path):
    app, _repo = _app(tmp_path / "home")
    headers = {"Authorization": f"Bearer {TOKEN}"}
    with TestClient(app) as client:
        current = client.get("/api/manager-profile", headers=headers).json()
        response = client.patch(
            "/api/manager-profile", headers=headers, json=_edit(current["version"])
        )
        assert response.status_code == 200
        assert response.json()["personality"]["language_preferences"] == ["English", "Arabic"]

    reopened_app, _reopened_repo = _app(tmp_path / "home")
    with TestClient(reopened_app) as client:
        saved = client.get("/api/manager-profile", headers=headers).json()
    assert saved["display_name"] == "Rania Haddad"
    assert saved["personality"]["working_habits"] == [
        "Keep source references beside material findings"
    ]


def test_manager_profile_patch_maps_optimistic_conflict_to_fixed_409(tmp_path):
    app, _repo = _app(tmp_path / "home")
    headers = {"Authorization": f"Bearer {TOKEN}"}
    with TestClient(app) as client:
        assert (
            client.patch("/api/manager-profile", headers=headers, json=_edit(1)).status_code == 200
        )
        response = client.patch(
            "/api/manager-profile",
            headers=headers,
            json=_edit(1, display_name="Stale manager"),
        )
    assert response.status_code == 409
    assert response.json()["detail"] == "The Tender Manager changed. Refresh before editing."


@pytest.mark.parametrize(
    "payload",
    [
        {"expected_version": 1},
        {"expected_version": 1, "creator_id": "engineer"},
        {"expected_version": 1, "personality": {}, "grant": "send"},
    ],
)
def test_manager_profile_patch_rejects_malformed_or_server_owned_fields(tmp_path, payload):
    app, _repo = _app(tmp_path / "home")
    headers = {"Authorization": f"Bearer {TOKEN}"}
    with TestClient(app) as client:
        response = client.patch("/api/manager-profile", headers=headers, json=payload)
    assert response.status_code == 422
