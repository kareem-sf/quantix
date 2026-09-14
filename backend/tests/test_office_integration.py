"""Acceptance checks across the real app and office service boundaries."""

import asyncio
import json

import pytest
from fastapi.testclient import TestClient
from office_test_support import api_result_for
from test_catalog_authority import configured_office

from quantix import diagnostics
from quantix.api import create_app
from quantix.jobs import JobManager
from quantix.manager_profile import ManagerProfileService
from quantix.office_types import OfficeOutput
from quantix.settings import SettingsService
from quantix.team import TeamService


@pytest.fixture(autouse=True)
def close_temporary_diagnostics():
    before = diagnostics._writer
    yield
    after = diagnostics._writer
    if after is not None and after is not before:
        after.close()


def editable(profile, **changes):
    data = profile.model_dump(mode="json")
    for name in ("id", "version", "created_at", "updated_at"):
        data.pop(name)
    return {"expected_version": profile.version, **data, **changes}


def test_prepared_validation_checks_sources_without_publishing(tmp_path, monkeypatch):
    from dataclasses import replace

    from test_repository import source

    from quantix.office import prepare_result, validate_prepared
    from quantix.office_research import ResearchRecord
    from quantix.office_tools import OfficeContext

    repo, tender, *_ = configured_office(tmp_path, monkeypatch)
    _, evidence, _ = source(repo, tender["id"])
    run = repo.create_run(tender["id"], "manager", "Synthetic validation")
    context = OfficeContext(repo, tender["id"], run["id"])
    context.source(evidence["id"])
    prepared = prepare_result(
        OfficeOutput(summary="Synthetic source-backed draft.", source_ids=[evidence["id"]]),
        context, {}, ResearchRecord(context),
    )
    validated = validate_prepared(repo, prepared)
    assert validated.seen_sources == {evidence["id"]}
    assert repo.messages(tender["id"]) == []
    assert repo.list_findings(tender["id"]) == []
    with pytest.raises(ValueError, match="not read"):
        validate_prepared(repo, replace(prepared, source_ids_read=()))


def test_custom_manager_prompt_redacts_private_text_without_losing_profile_structure(tmp_path, monkeypatch):
    from quantix.conversation import _prompt as conversation_prompt
    from quantix.manager_runtime import prompt_profile
    from quantix.office import _prompt as engineering_prompt
    from quantix.office_tools import OfficeContext

    repo, tender, *_ = configured_office(tmp_path, monkeypatch)
    service = ManagerProfileService(repo)
    before = service.get()
    edit = editable(before, persona='Use the note at C:\\private\\manager.txt and explain "why" clearly.')
    edit["personality"]["traits"] = ["Patient", "Arabic: مراجع دقيق", "sk-synthetic-private-token123"]
    profile = service.update(edit)
    run = repo.create_run(tender["id"], "manager", "Synthetic profile")
    context = OfficeContext(repo, tender["id"], run["id"])
    public_profile = prompt_profile(profile)
    encoded = engineering_prompt(context, "Review", public_profile)
    conversation = conversation_prompt(repo, tender["id"], "Review", public_profile)
    for payload in (json.loads(encoded), json.loads(conversation.split("\n\n", 1)[1])):
        professional = payload["manager_profile"]
        assert 'explain "why" clearly.' in professional["persona"]
        assert "[local path]" in professional["persona"]
        assert professional["personality"]["traits"] == ["Patient", "Arabic: مراجع دقيق", "[credential]"]
        assert set(professional["personality"]) == set(public_profile["personality"])
    assert profile.personality.traits[-1] == "sk-synthetic-private-token123"


def test_actual_api_keeps_one_customizable_manager_and_no_staff(tmp_path, monkeypatch):
    monkeypatch.setattr("keyring.get_password", lambda *_: None)
    monkeypatch.setattr(
        "quantix.ai_execution.execute_api",
        lambda *_args, **_kwargs: pytest.fail("Profile inspection must not call AI"),
    )
    app = create_app(tmp_path / "api-home", "synthetic-office-session")
    with TestClient(app) as client:
        denied = client.get("/api/manager-profile")
        assert denied.status_code == 401
        assert denied.headers["cache-control"] == "no-store"
        client.headers["Authorization"] = "Bearer synthetic-office-session"
        response = client.get("/api/manager-profile")
        assert response.status_code == 200
        assert response.headers["cache-control"] == "no-store"
        initial = response.json()
        assert initial["display_name"] == "Tender Manager"
        assert "manager_profile" in client.get("/api/health").json()["capabilities"]
        tenders = [client.post("/api/tenders", json={"name": name}).json()
                   for name in ("Synthetic Office A", "Synthetic Office B")]
        store = TeamService(app.state.repo)
        assert all(store.list_staff(tender["id"]) == [] for tender in tenders)
        profile = ManagerProfileService(app.state.repo).get()
        body = editable(profile, display_name="Custom engineering coordinator")
        body["personality"]["description"] = "Explain assumptions clearly in Arabic or English."
        saved = client.patch("/api/manager-profile", json=body)
        assert saved.status_code == 200
        assert saved.json()["id"] == initial["id"]
        assert saved.json()["version"] == initial["version"] + 1
        assert client.patch("/api/manager-profile", json=body).status_code == 409
        assert client.get("/api/manager-profile").json() == saved.json()
        assert all(app.state.repo.messages(tender["id"]) == [] for tender in tenders)
        assert all(store.list_staff(tender["id"]) == [] for tender in tenders)


@pytest.mark.asyncio
async def test_conversation_and_engineering_use_the_admitted_manager_version(tmp_path, monkeypatch):
    repo, tender, *_ = configured_office(tmp_path, monkeypatch)
    profiles = ManagerProfileService(repo)
    initial = profiles.get()
    before = profiles.update(editable(initial, display_name="Original custom Manager"))
    jobs = JobManager(repo, SettingsService(repo))
    prompts = []

    async def provider(route, connection, credentials, context, prompt, output_type, **options):
        payload = json.loads(prompt.split("\n\n", 1)[-1])
        prompts.append(payload)
        assert payload["manager_profile"]["display_name"] == before.display_name
        if options.get("operation") == "conversation":
            profiles.update(editable(profiles.get(), display_name="Changed during current work"))
            return api_result_for({"kind": "engineering", "reply": "", "next_action": "Review sources."})
        return api_result_for(OfficeOutput(summary="Synthetic review saved."))

    monkeypatch.setattr("quantix.ai_execution.execute_api", provider)
    submitted = jobs.submit_message(tender["id"], "Review this synthetic request")
    await asyncio.gather(*list(jobs.tasks.values()))
    run = repo.get_run(submitted["run"]["id"])
    assert run["status"] == "completed"
    assert len(prompts) == 2
    assert profiles.get().display_name == "Changed during current work"
    from quantix.manager_runtime import ManagerRunProfiles
    assert ManagerRunProfiles(repo).get(tender["id"], run["id"]).version == before.version
    await jobs.close()
