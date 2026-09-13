"""Session reuse never changes a saved Tender/profile/account/source binding."""
from types import SimpleNamespace

import pytest

from quantix.ai_connections import AIConnectionService
from quantix.manager_runtime import ManagerRunProfiles
from quantix.native_execution import NativeExecutionService, project_client_event
from quantix.repository import Repository


def workspace(tmp_path):
    repo = Repository(tmp_path)
    tender = repo.create_tender("Synthetic native session")
    connection = AIConnectionService(repo).create({"name": "Synthetic subscription", "provider_id": "codex", "protocol": "codex", "auth_type": "client_login", "billing": "subscription"})
    connection["_checked_component_version"] = "verified-test-runtime"
    return repo, tender, connection, NativeExecutionService(repo)


def context(repo, tender):
    run = repo.create_run(tender["id"], "conversation")
    ManagerRunProfiles(repo).capture(tender["id"], run["id"])
    return SimpleNamespace(repo=repo, tender_id=tender["id"], run_id=run["id"], approved_scope=None)


def test_compatible_native_session_preserves_scope_and_turn_history(tmp_path):
    repo, tender, connection, service = workspace(tmp_path)
    first = context(repo, tender)
    route = {"model_id": "exact", "reasoning": "medium"}
    session = service.prepare(first, connection, route, operation="conversation", tools=["submit"])
    service.finish(first, session.id, provider_session_id="native-exact-id", state="completed")
    second = context(repo, tender)
    resumed = service.prepare(second, connection, route, operation="conversation", tools=["submit"], resume_session_id=session.id)
    assert resumed.provider_session_id == "native-exact-id" and resumed.scope_fingerprint == session.scope_fingerprint
    with repo.db.connect() as conn:
        assert conn.execute("SELECT count(*) FROM native_client_session_turns WHERE session_id=?", (session.id,)).fetchone()[0] == 2
    with pytest.raises(ValueError, match="no longer"):
        service.finish(first, session.id, state="running")


@pytest.mark.parametrize("change", ["model", "tools", "runtime"])
def test_incompatible_native_continuation_is_rejected(tmp_path, change):
    repo, tender, connection, service = workspace(tmp_path)
    first = context(repo, tender)
    route = {"model_id": "exact"}
    session = service.prepare(first, connection, route, operation="conversation", tools=["submit"])
    service.finish(first, session.id, provider_session_id="native-id", state="completed")
    second = context(repo, tender)
    if change == "model":
        route = {"model_id": "different"}
    if change == "runtime":
        connection = {**connection, "_checked_component_version": "different-runtime"}
    with pytest.raises(ValueError, match="incompatible"):
        service.prepare(second, connection, route, operation="conversation", tools=["different"] if change == "tools" else ["submit"], resume_session_id=session.id)


def test_worker_event_cannot_forge_staff_identity_or_control_fields(tmp_path):
    repo, tender, _, _ = workspace(tmp_path)
    current = context(repo, tender)
    current.actor_id, current.assignment_id = "actual-staff", "actual-assignment"
    project_client_event(current, "assistant_text_delta", {"text": "Actual supplied text", "actor_id": "forged", "api_key": "hidden"})
    saved = repo.run_events(current.run_id)[0]["data"]
    assert saved == {"text": "Actual supplied text", "origin": "client", "actor_id": "actual-staff", "assignment_id": "actual-assignment"}


def test_native_metadata_api_is_scoped_and_excludes_account_internals(tmp_path):
    from fastapi import FastAPI
    from fastapi.testclient import TestClient

    from quantix.native_execution_routes import create_router
    repo, tender, account, service = workspace(tmp_path)
    current = context(repo, tender)
    binding = service.prepare(current, account, {"model_id": "exact"}, operation="conversation")
    service.finish(current, binding.id, provider_session_id="native-id", state="completed")
    app = FastAPI()
    app.include_router(create_router(repo))
    with TestClient(app) as client:
        response = client.get(f'/api/tenders/{tender["id"]}/native-sessions')
        assert response.status_code == 200 and response.json()[0]["provider_session_id"] == "native-id"
        assert "credentials" not in response.text and "account_home" not in response.text
        capabilities = client.get(f'/api/ai/connections/{account["id"]}/native-capabilities').json()
        assert next(item for item in capabilities if item["id"] == "native_files_shell")["supported"] is False
        assert next(item for item in capabilities if item["id"] == "output_token_limit")["supported"] is False
