"""Reusable professional definitions stay separate from Tender authority."""

from __future__ import annotations

import json
from types import SimpleNamespace

import pytest
from fastapi import FastAPI
from fastapi.testclient import TestClient

from quantix.manager_profile import ManagerProfileService
from quantix.manager_runtime import ManagerRunProfiles
from quantix.repository import Repository
from quantix.staff_models import ManagerCreationContext
from quantix.staff_store import StaffStore


def _personality() -> dict:
    return {
        "description": "A careful quantity surveying professional.",
        "traits": ["careful", "commercially aware"],
        "communication_style": "Use plain construction language.",
        "problem_solving_style": "Check quantities and rates separately.",
        "collaboration_style": "Share traceable working notes.",
        "uncertainty_handling": "Label every allowance and gap.",
        "initiative": "Raise material commercial risks early.",
        "explanation_style": "Lead with the decision needed.",
        "language_preferences": ["English"],
        "working_habits": ["Keep units with quantities"],
    }


def _profile(name: str = "Amina Hassan") -> dict:
    return {
        "display_name": name,
        "role": "Quantity surveying",
        "title": "Senior Quantity Surveyor",
        "specialisms": ["Bills of quantities", "Rate build-ups"],
        "persona": "A practical quantity surveyor who keeps commercial work auditable.",
        "personality": _personality(),
        "responsibilities": ["Check quantities and commercial allowances"],
        "objectives": ["Give the Tender Manager a traceable commercial position"],
        "methods": ["Reconcile each item against current evidence"],
        "deliverables": ["A checked quantity and rate schedule"],
        "success_criteria": ["Every quantity and rate has a stated basis"],
        "context_needs": ["Current Tender documents and reviewed company methods"],
        "requested_tool_ids": ["read_source", "calculate"],
        "creation_reason": "Reusable quantity-surveying support for Tender work.",
    }


def _settings() -> dict:
    return {
        "temperature": 0.2,
        "top_p": 0.9,
        "reasoning": "medium",
        "max_output_tokens": 4096,
        "output_mode": "auto",
        "native_tools": [],
        "max_search_calls": 3,
        "max_native_tool_calls": 3,
    }


def _create_command(name: str = "Amina Hassan", key: str = "create-qs"):
    from quantix.agent_definition_models import AgentDefinitionCreate

    return AgentDefinitionCreate(
        profile=_profile(name),
        generation_settings=_settings(),
        idempotency_key=key,
    )


def _order() -> dict:
    return {
        "brief": "Check the priced bill and report material gaps.",
        "goal": "Give the Tender Manager a traceable commercial check.",
        "source_ids": [],
        "expected_outputs": ["Checked quantity and rate schedule"],
        "completion_checks": ["Every quantity and rate has a stated basis"],
    }


def test_definition_library_starts_empty_and_preserves_immutable_versions(tmp_path):
    from quantix.agent_definition_models import AgentDefinitionEdit
    from quantix.agent_definitions import AgentDefinitionService

    repo = Repository(tmp_path / "quantix")
    service = AgentDefinitionService(repo)
    assert service.list() == []

    first = service.create(_create_command())
    replay = service.create(_create_command())
    assert replay == first
    assert first.version == 1
    assert first.lifecycle == "active"
    assert first.profile.display_name == "Amina Hassan"
    assert first.generation_settings.max_output_tokens == 4096

    second = service.revise(
        first.id,
        AgentDefinitionEdit(
            expected_version=1,
            profile=_profile("Amina Hassan MRICS"),
            generation_settings={**_settings(), "max_output_tokens": 6144},
            idempotency_key="revise-qs",
        ),
    )
    assert second.version == 2
    assert second.profile.display_name == "Amina Hassan MRICS"
    assert service.get(first.id, version=1) == first
    assert [item.version for item in service.versions(first.id)] == [2, 1]
    assert service.get(first.id).version == 2


def test_definition_mutations_are_conflict_safe_and_retired_definitions_stay_exportable(tmp_path):
    from quantix.agent_definition_models import (
        AgentDefinitionDuplicate,
        AgentDefinitionEdit,
        AgentDefinitionRetire,
    )
    from quantix.agent_definitions import AgentDefinitionService
    from quantix.staff_models import OfficeConflict

    repo = Repository(tmp_path / "quantix")
    service = AgentDefinitionService(repo)
    source = service.create(_create_command())

    with pytest.raises(OfficeConflict, match="different"):
        service.create(_create_command(name="Changed body"))
    with pytest.raises(OfficeConflict, match="changed"):
        service.revise(
            source.id,
            AgentDefinitionEdit(
                expected_version=2,
                profile=_profile(),
                generation_settings=_settings(),
                idempotency_key="stale-edit",
            ),
        )

    copy = service.duplicate(
        source.id,
        AgentDefinitionDuplicate(
            source_version=1,
            display_name="Amina Hassan - Commercial Review",
            idempotency_key="copy-qs",
        ),
    )
    assert copy.id != source.id
    assert copy.version == 1
    assert copy.profile.display_name == "Amina Hassan - Commercial Review"
    assert copy.source_definition_id == source.id
    assert copy.source_definition_version == 1

    retired = service.retire(
        source.id,
        AgentDefinitionRetire(expected_version=1, idempotency_key="retire-qs"),
    )
    assert retired.lifecycle == "retired"
    assert service.list() == [copy]
    assert {item.id for item in service.list(include_retired=True)} == {copy.id, source.id}
    with pytest.raises(OfficeConflict, match="retired"):
        service.revise(
            source.id,
            AgentDefinitionEdit(
                expected_version=1,
                profile=_profile("Should not save"),
                generation_settings=_settings(),
                idempotency_key="edit-retired",
            ),
        )

    exported = service.export(source.id, version=1).model_dump(mode="json")
    assert exported["profile"] == _profile()
    assert exported["generation_settings"] == _settings()
    assert set(exported) == {
        "schema_version",
        "definition_id",
        "version",
        "fingerprint",
        "profile",
        "generation_settings",
    }
    serialized = json.dumps(exported).lower()
    assert "credential" not in serialized
    assert "source_ids" not in serialized
    assert "grant" not in serialized
    assert "connection_id" not in serialized


def test_exact_definition_version_instantiates_normal_staff_without_execution_authority(tmp_path):
    from quantix.agent_definition_models import AgentDefinitionEdit
    from quantix.agent_definitions import AgentDefinitionService

    repo = Repository(tmp_path / "quantix")
    tender = repo.create_tender("Synthetic reuse Tender")
    run = repo.create_run(tender["id"], "manager", "Use an exact professional definition")
    manager = ManagerProfileService(repo)
    ManagerRunProfiles(repo).capture(tender["id"], run["id"])
    context = ManagerCreationContext(
        tender["id"], run["id"], manager.get().version, "scope-definition-reuse"
    )
    definitions = AgentDefinitionService(repo)
    definition = definitions.create(_create_command())

    receipt = StaffStore(repo).create_from_definition(
        context,
        definition.id,
        definition.version,
        _order(),
        "reuse-definition",
    )
    assert receipt.staff.definition_id == definition.id
    assert receipt.staff.definition_version == 1
    assert receipt.work_order.definition_id == definition.id
    assert receipt.work_order.definition_version == 1
    assert receipt.staff.version == 1
    with repo.db.connect() as conn:
        assignment_table = conn.execute(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name='office_assignments'"
        ).fetchone()
        if assignment_table:
            assert conn.execute("SELECT COUNT(*) FROM office_assignments").fetchone()[0] == 0
        binding_table = conn.execute(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name='office_route_bindings'"
        ).fetchone()
        if binding_table:
            assert conn.execute("SELECT COUNT(*) FROM office_route_bindings").fetchone()[0] == 0

    definitions.revise(
        definition.id,
        AgentDefinitionEdit(
            expected_version=1,
            profile=_profile("Later library version"),
            generation_settings={**_settings(), "temperature": 0.4},
            idempotency_key="later-definition",
        ),
    )
    saved = StaffStore(repo).get_staff(tender["id"], receipt.staff.id)
    assert saved.display_name == "Amina Hassan"
    assert saved.definition_id == definition.id
    assert saved.definition_version == 1


def test_definition_reference_survives_later_staff_lifecycle_versions(tmp_path):
    from quantix.agent_definitions import AgentDefinitionService
    from quantix.execution_context import engineer_identity
    from quantix.staff_lifecycle import StaffLifecycleService
    from quantix.staff_lifecycle_models import StaffLifecycleRequest

    repo = Repository(tmp_path / "quantix")
    tender = repo.create_tender("Synthetic staff lifecycle Tender")
    run = repo.create_run(tender["id"], "manager", "Reuse a professional definition")
    manager = ManagerProfileService(repo)
    ManagerRunProfiles(repo).capture(tender["id"], run["id"])
    context = ManagerCreationContext(
        tender["id"], run["id"], manager.get().version, "scope-definition-lifecycle"
    )
    definition = AgentDefinitionService(repo).create(_create_command())
    created = StaffStore(repo).create_from_definition(
        context, definition.id, 1, _order(), "definition-lifecycle"
    )

    retired = StaffLifecycleService(repo).transition(
        engineer_identity(tender["id"]),
        StaffLifecycleRequest(
            staff_id=created.staff.id,
            expected_version=1,
            target="retired",
            reason="The synthetic Tender no longer needs this colleague.",
            idempotency_key="definition-staff-retire",
        ),
    )
    assert retired.staff.version == 2
    assert retired.staff.definition_id == definition.id
    assert retired.staff.definition_version == 1
    first = StaffStore(repo).get_staff(tender["id"], created.staff.id, version=1)
    assert first.definition_id == definition.id
    assert first.definition_version == 1


@pytest.mark.asyncio
async def test_manager_tools_save_discover_and_reuse_exact_definitions(tmp_path):
    from quantix.staff_generation import staff_generation_tools

    repo = Repository(tmp_path / "quantix")
    tender = repo.create_tender("Synthetic Manager definition Tender")
    run = repo.create_run(tender["id"], "manager", "Build a reusable QS definition")
    ManagerRunProfiles(repo).capture(tender["id"], run["id"])
    context = SimpleNamespace(repo=repo, tender_id=tender["id"], run_id=run["id"])
    tools = {
        item.name: item
        for item in staff_generation_tools(repo, tender["id"], run["id"], "scope-library")
    }

    assert {
        "save_agent_definition",
        "list_agent_definitions",
        "read_agent_definition",
        "create_staff_from_definition",
    }.issubset(tools)
    assert "tender_id" not in tools["save_agent_definition"].parameters["properties"]
    assert "run_id" not in tools["save_agent_definition"].parameters["properties"]
    assert "grant" not in json.dumps(tools["save_agent_definition"].parameters).lower()

    saved = json.loads(
        await tools["save_agent_definition"].invoke(
            context,
            {"profile": _profile(), "generation_settings": _settings()},
            invocation_id="manager-save-definition",
        )
    )
    listed = json.loads(await tools["list_agent_definitions"].invoke(context, {}))
    assert listed["items"][0]["id"] == saved["definition"]["id"]
    exact = json.loads(
        await tools["read_agent_definition"].invoke(
            context,
            {"definition_id": saved["definition"]["id"], "version": 1},
        )
    )
    assert exact["version"] == 1

    created = json.loads(
        await tools["create_staff_from_definition"].invoke(
            context,
            {
                "definition_id": saved["definition"]["id"],
                "definition_version": 1,
                "work_order": _order(),
            },
            invocation_id="manager-reuse-definition",
        )
    )
    assert created["staff"]["definition_id"] == saved["definition"]["id"]
    assert created["staff"]["definition_version"] == 1
    assert created["work_order"]["staff_version"] == 1


def test_definition_brief_action_queues_the_manager_tool_enabled_branch(tmp_path):
    from quantix.conversation import request_kind
    from quantix.jobs import JobManager
    from quantix.staff_generation import staff_generation_tools

    repo = Repository(tmp_path / "quantix")
    tender = repo.create_tender("Synthetic definition generation Tender")
    instruction = (
        "Create and save one reusable professional definition from this brief. "
        "Use save_agent_definition with a complete provider-independent profile."
    )
    kind = request_kind(instruction, action="review_documents")
    jobs = JobManager(repo, object())
    run = jobs._queue(tender["id"], kind, instruction)

    assert run["kind"] == "manager"
    assert ManagerRunProfiles(repo).get(tender["id"], run["id"]).id
    tool_names = {
        item.name
        for item in staff_generation_tools(repo, tender["id"], run["id"], "scope-definition-brief")
    }
    assert "save_agent_definition" in tool_names
    assert "create_staff_from_definition" in tool_names


def test_binding_uses_only_compatible_reviewed_definition_preferences(tmp_path, monkeypatch):
    from test_staff_routing import _ready_binding_workspace

    from quantix.agent_definition_models import AgentDefinitionCreate
    from quantix.agent_definitions import AgentDefinitionService
    from quantix.ai_generation_models import GenerationSettings
    from quantix.ai_models import AIRoute

    (
        repo,
        tender,
        _connection,
        _model,
        route_data,
        envelope,
        routing,
        existing_staff,
        root_context,
        _artifact,
    ) = _ready_binding_workspace(tmp_path, monkeypatch, envelope_tools=())
    route = AIRoute.model_validate(route_data)
    manager = ManagerProfileService(repo)
    definitions = AgentDefinitionService(repo)
    neutral = GenerationSettings().model_dump(mode="json")
    mismatched_reasoning = "high" if route.reasoning != "high" else "low"
    mismatch = definitions.create(
        AgentDefinitionCreate(
            profile={
                **_profile("Definition mismatch"),
                "requested_tool_ids": [],
            },
            generation_settings={**neutral, "reasoning": mismatched_reasoning},
            idempotency_key="definition-mismatch",
        )
    )
    mismatch_run = repo.create_run(tender["id"], "manager", "Create mismatched staff")
    mismatch_context = ManagerCreationContext(
        tender["id"], mismatch_run["id"], manager.get().version, root_context.scope_id
    )
    mismatch_staff = StaffStore(repo).create_from_definition(
        mismatch_context,
        mismatch.id,
        mismatch.version,
        existing_staff.work_order.work_order,
        "mismatch-staff",
    )

    with pytest.raises(ValueError, match="definition preferences.*review"):
        routing.bind(
            root_context,
            mismatch_staff.staff.id,
            mismatch_staff.work_order.id,
            envelope.route_options[0].id,
            "mismatch-binding",
        )

    compatible = definitions.create(
        AgentDefinitionCreate(
            profile={
                **_profile("Definition match"),
                "requested_tool_ids": [],
            },
            generation_settings={**neutral, "reasoning": route.reasoning},
            idempotency_key="definition-match",
        )
    )
    compatible_run = repo.create_run(tender["id"], "manager", "Create compatible staff")
    compatible_context = ManagerCreationContext(
        tender["id"], compatible_run["id"], manager.get().version, root_context.scope_id
    )
    compatible_staff = StaffStore(repo).create_from_definition(
        compatible_context,
        compatible.id,
        compatible.version,
        existing_staff.work_order.work_order,
        "compatible-staff",
    )
    binding = routing.bind(
        root_context,
        compatible_staff.staff.id,
        compatible_staff.work_order.id,
        envelope.route_options[0].id,
        "compatible-binding",
    )
    assert binding.definition_id == compatible.id
    assert binding.definition_version == compatible.version
    assert binding.route.model_dump(mode="json") == route.model_dump(mode="json")


def test_definition_routes_cover_library_versions_duplicate_retire_and_export(tmp_path):
    from quantix.agent_definition_models import AgentDefinitionCreate
    from quantix.agent_definition_routes import create_router

    repo = Repository(tmp_path / "quantix")
    app = FastAPI()
    app.include_router(create_router(repo))
    client = TestClient(app)

    assert client.get("/api/agent-definitions").json() == []
    created = client.post(
        "/api/agent-definitions",
        json=AgentDefinitionCreate(
            profile=_profile(),
            generation_settings=_settings(),
            idempotency_key="route-create",
        ).model_dump(mode="json"),
    )
    assert created.status_code == 200
    value = created.json()

    edited = client.patch(
        f"/api/agent-definitions/{value['id']}",
        json={
            "expected_version": 1,
            "profile": _profile("Route edit"),
            "generation_settings": _settings(),
            "idempotency_key": "route-edit",
        },
    )
    assert edited.status_code == 200
    assert edited.json()["version"] == 2
    versions = client.get(f"/api/agent-definitions/{value['id']}/versions")
    assert [item["version"] for item in versions.json()] == [2, 1]

    duplicate = client.post(
        f"/api/agent-definitions/{value['id']}/duplicate",
        json={
            "source_version": 1,
            "display_name": "Route copy",
            "idempotency_key": "route-copy",
        },
    )
    assert duplicate.status_code == 200
    assert duplicate.json()["source_definition_version"] == 1

    exported = client.get(f"/api/agent-definitions/{value['id']}/export?version=1")
    assert exported.status_code == 200
    assert exported.json()["profile"]["display_name"] == "Amina Hassan"
    retired = client.post(
        f"/api/agent-definitions/{value['id']}/retire",
        json={"expected_version": 2, "idempotency_key": "route-retire"},
    )
    assert retired.status_code == 200
    assert retired.json()["lifecycle"] == "retired"
    assert [item["id"] for item in client.get("/api/agent-definitions").json()] == [
        duplicate.json()["id"]
    ]
