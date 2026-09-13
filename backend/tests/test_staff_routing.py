"""Bounded delegation authority for generated Tender Office staff."""

import hashlib
import json

import pytest
from test_catalog_authority import configured_office

from quantix.ai_policy import AIPolicyService
from quantix.db import dump, new_id, now
from quantix.manager_profile import ManagerProfileService
from quantix.manager_runtime import ManagerRunProfiles
from quantix.staff_capabilities import capability_catalog, get_capability
from quantix.staff_models import ManagerCreationContext, ManagerProfileEdit, OfficeConflict
from quantix.staff_routing import StaffRoutingService
from quantix.staff_routing_models import (
    ArtifactBasis,
    DelegationEnvelope,
    DelegationRouteOption,
    model_revision_fingerprint,
    route_option_id,
)
from quantix.staff_store import StaffStore


def _personality():
    return {
        "description": "Evidence-led synthetic colleague.",
        "traits": ["careful"],
        "communication_style": "Plain language.",
        "problem_solving_style": "Use bounded checks.",
        "collaboration_style": "Share exact sources.",
        "uncertainty_handling": "Label gaps.",
        "initiative": "Suggest the next safe step.",
        "explanation_style": "Lead with the answer.",
        "language_preferences": ["English"],
        "working_habits": ["Keep citations"],
    }


def _profile(**overrides):
    value = {
        "display_name": "Unfamiliar Colleague",
        "role": "Concrete package synthesiser",
        "title": "Evidence Specialist",
        "specialisms": ["Package review"],
        "persona": "A bounded professional profile.",
        "personality": _personality(),
        "responsibilities": ["Compare source evidence"],
        "objectives": ["List material gaps"],
        "methods": ["Read current sources"],
        "deliverables": ["A source-backed gap list"],
        "success_criteria": ["Every gap has a source"],
        "context_needs": ["Current Tender sources"],
        "requested_tool_ids": [],
        "creation_reason": "Created for this Tender task.",
    }
    value.update(overrides)
    return value


def _work_order(**overrides):
    value = {
        "brief": "Review the current Tender source package.",
        "goal": "Give the Manager a source-backed gap list.",
        "source_ids": [],
        "expected_outputs": ["Gap list"],
        "completion_checks": ["Each gap has a source"],
    }
    value.update(overrides)
    return value


def _source(repo, tender_id):
    body = b"synthetic reviewed source"
    artifact, _ = repo.register_artifact(
        tender_id,
        "Sources/spec.pdf",
        hashlib.sha256(body).hexdigest(),
        len(body),
        {"kind": "pdf", "status": "extracted", "segments": [{"locator": "page:1", "text": body.decode()}]},
    )
    evidence = repo.artifact_evidence(tender_id, artifact["id"])[0]
    return artifact, evidence


def _envelope(route, connection, model, artifacts=(), *, tools=()):
    option = DelegationRouteOption(
        id="0" * 64,
        route=route,
        connection_revision=connection["revision"],
        model_revision=model_revision_fingerprint(model),
        model=model,
        account_name=connection["name"],
        provider=connection["provider_id"],
        data_destination=connection["base_url"],
        billing=connection["billing"],
        provider_managed_extras=False,
    )
    option = option.model_copy(update={"id": route_option_id(option)})
    return DelegationEnvelope(
        version=1,
        purpose="Review current Tender sources.",
        source_scope="selected_sources",
        artifacts=list(artifacts),
        tools=[get_capability(item) for item in tools],
        allowed_draft_outputs=["findings"],
        route_options=[option],
        max_staff=1,
        max_assignments=1,
        max_depth=1,
        max_concurrency=1,
        max_requests=12,
        max_search_calls=0,
        run_budget_usd=10,
        tender_budget_usd=100,
    )


def _receipt(repo, tender, plan, envelope, fingerprint="f" * 64, work_intents=None):
    with repo.db.connect(write=True) as conn:
        conn.execute(
            "CREATE TABLE IF NOT EXISTS plan_review_approvals(id TEXT PRIMARY KEY,tender_id TEXT NOT NULL,plan_id TEXT NOT NULL,fingerprint TEXT NOT NULL,rationale TEXT NOT NULL,review_json TEXT NOT NULL,work_intents_json TEXT NOT NULL,created_at TEXT NOT NULL)"
        )
        conn.execute(
            "INSERT INTO plan_review_approvals VALUES(?,?,?,?,?,?,?,datetime('now'))",
            ("receipt-1", tender["id"], plan["id"], fingerprint, "Synthetic", json.dumps({"delegation": envelope.model_dump(mode="json")}), json.dumps(work_intents or [])),
        )


def _ready_binding_workspace(tmp_path, monkeypatch, *, requested_tool_ids=None, envelope_tools=("read_source",), max_staff=1, max_assignments=1, max_depth=1, max_concurrency=1):
    repo, tender, _connections, connection, model, _policy, route = configured_office(tmp_path, monkeypatch)
    artifact, evidence = _source(repo, tender["id"])
    plan = repo.create_plan(tender["id"], "Review", [{"title": "Review", "role": "Any role", "description": "Review", "source_ids": [evidence["id"]]}])
    repo.approve_plan(tender["id"], plan["id"], "Synthetic plan approval")
    manager = ManagerProfileService(repo)
    planning = repo.create_run(tender["id"], "conversation", "Create colleague")
    context = ManagerCreationContext(tender["id"], planning["id"], manager.get().version, plan["id"])
    staff = StaffStore(repo).create_generated(
        context,
        _profile(requested_tool_ids=requested_tool_ids or []),
        _work_order(source_ids=[evidence["id"]]),
        "create",
    )
    root = repo.create_run(tender["id"], "manager", "Run delegated work")
    ManagerRunProfiles(repo).capture(tender["id"], root["id"])
    envelope = _envelope(
        route,
        connection,
        model,
        [ArtifactBasis(artifact_id=artifact["id"], version=artifact["version"], content_hash=artifact["content_hash"])],
        tools=envelope_tools,
    ).model_copy(update={"max_staff": max_staff, "max_assignments": max_assignments, "max_depth": max_depth, "max_concurrency": max_concurrency})
    _receipt(repo, tender, plan, envelope, work_intents=[{"run_id": root["id"], "tender_id": tender["id"], "plan_id": plan["id"], "kind": "manager", "task_id": None}])
    routing = StaffRoutingService(repo)
    policy_revision = AIPolicyService(repo).get(tender["id"])["revision"]
    routing.save_reviewed_grant(tender["id"], plan["id"], "f" * 64, policy_revision, envelope)
    root_context = ManagerCreationContext(tender["id"], root["id"], manager.get().version, plan["id"])
    return repo, tender, connection, model, route, envelope, routing, staff, root_context, artifact


def test_capability_catalog_uses_actual_tool_metadata():
    catalog = capability_catalog()
    assert {item.id for item in catalog} >= {"read_source", "inspect_estimate", "calculate_drawing_measurement"}
    assert "create_staff" not in {item.id for item in catalog}
    for item in catalog:
        if item.id in {"request_child_assignment", "calculate_engineering", "check_engineering_calculation", "save_work_product", "cite_public_passages", "record_market_observation", "save_working_memory", "search_public_sources", "execute_tool_code", "execute_python_analysis"}:
            assert item.read_only is False
        else:
            assert item.read_only is True


def test_arbitrary_role_binds_from_capabilities_and_current_review(tmp_path, monkeypatch):
    _repo, _tender, _connection, _model, route, envelope, routing, staff, bound_context, _artifact = _ready_binding_workspace(
        tmp_path, monkeypatch, requested_tool_ids=["read_source"], envelope_tools=("read_source", "inspect_estimate")
    )
    bound = routing.bind(bound_context, staff.staff.id, staff.work_order.id, envelope.route_options[0].id, "bind")
    assert bound.route.model_dump(mode="json") == route
    assert [tool.id for tool in bound.tools] == ["read_source"]


def test_familiar_role_cannot_gain_unreviewed_tool(tmp_path, monkeypatch):
    _repo, _tender, _connection, _model, _route, envelope, routing, staff, root_context, _artifact = _ready_binding_workspace(
        tmp_path, monkeypatch, requested_tool_ids=["inspect_estimate"]
    )
    with pytest.raises(ValueError, match="outside the reviewed delegation"):
        routing.bind(root_context, staff.staff.id, staff.work_order.id, envelope.route_options[0].id, "blocked")


def test_visual_tool_requires_actual_model_image_capability(tmp_path, monkeypatch):
    _repo, _tender, _connection, _model, _route, envelope, routing, staff, root_context, _artifact = _ready_binding_workspace(
        tmp_path,
        monkeypatch,
        requested_tool_ids=["view_document_page"],
        envelope_tools=("view_document_page",),
    )
    with pytest.raises(ValueError, match="image capability"):
        routing.bind(root_context, staff.staff.id, staff.work_order.id, envelope.route_options[0].id, "vision")


def test_text_profile_can_bind_text_route_when_envelope_also_contains_visual_tool(tmp_path, monkeypatch):
    _repo, _tender, _connection, _model, _route, envelope, routing, staff, root_context, _artifact = _ready_binding_workspace(
        tmp_path,
        monkeypatch,
        requested_tool_ids=["read_source"],
        envelope_tools=("read_source", "view_document_page"),
    )
    binding = routing.bind(root_context, staff.staff.id, staff.work_order.id, envelope.route_options[0].id, "text")
    assert [tool.id for tool in binding.tools] == ["read_source"]


def test_binding_replay_is_idempotent_and_altered_body_conflicts(tmp_path, monkeypatch):
    _repo, _tender, _connection, _model, _route, envelope, routing, staff, root_context, _artifact = _ready_binding_workspace(
        tmp_path, monkeypatch, requested_tool_ids=[]
    )
    first = routing.bind(root_context, staff.staff.id, staff.work_order.id, envelope.route_options[0].id, "same")
    assert first.tools == []
    replay = routing.bind(root_context, staff.staff.id, staff.work_order.id, envelope.route_options[0].id, "same")
    assert replay.id == first.id
    with pytest.raises(OfficeConflict):
        routing.bind(root_context, staff.staff.id, staff.work_order.id, "different-route", "same")


def test_binding_count_limits_are_root_aggregate(tmp_path, monkeypatch):
    repo, tender, connection, model, route, envelope, routing, first, root_context, _artifact = _ready_binding_workspace(
        tmp_path, monkeypatch, max_staff=1, max_assignments=1
    )
    routing.bind(root_context, first.staff.id, first.work_order.id, envelope.route_options[0].id, "one")
    manager = ManagerProfileService(repo)
    planning = repo.create_run(tender["id"], "conversation", "Create another colleague")
    create_context = ManagerCreationContext(tender["id"], planning["id"], manager.get().version, root_context.scope_id)
    second = StaffStore(repo).create_generated(create_context, _profile(display_name="Second colleague"), _work_order(), "second")
    with pytest.raises(ValueError, match="maximum staff"):
        routing.bind(root_context, second.staff.id, second.work_order.id, envelope.route_options[0].id, "two")


def test_reusing_existing_staff_with_another_work_order_can_use_assignment_limit(tmp_path, monkeypatch):
    repo, tender, _connection, _model, _route, envelope, routing, staff, root_context, _artifact = _ready_binding_workspace(
        tmp_path, monkeypatch, max_staff=1, max_assignments=2
    )
    first = routing.bind(root_context, staff.staff.id, staff.work_order.id, envelope.route_options[0].id, "first")
    second_order_id = new_id()
    second_order = _work_order(goal="Review the same package for a second bounded handoff.")
    with repo.atomic() as conn:
        conn.execute(
            "INSERT INTO office_work_orders(id,tender_id,staff_id,staff_version,creator_run_id,scope_id,work_order_json,created_at) VALUES(?,?,?,?,?,?,?,?)",
            (second_order_id, tender["id"], staff.staff.id, staff.staff.version, staff.work_order.creator_run_id, staff.work_order.scope_id, dump(second_order), now()),
        )
    second = routing.bind(root_context, staff.staff.id, second_order_id, envelope.route_options[0].id, "second")
    assert second.id != first.id


def test_root_requires_matching_approved_intent_or_later_engineer_instruction(tmp_path, monkeypatch):
    repo, tender, _connection, _model, _route, envelope, routing, staff, root_context, _artifact = _ready_binding_workspace(
        tmp_path, monkeypatch
    )
    profiles = ManagerRunProfiles(repo)
    raw = repo.create_run(tender["id"], "task", "Unrelated task")
    profiles.capture(tender["id"], raw["id"])
    raw_context = ManagerCreationContext(tender["id"], raw["id"], root_context.manager_profile_version, root_context.scope_id)
    with pytest.raises(ValueError, match="Manager or conversation"):
        routing.bind(raw_context, staff.staff.id, staff.work_order.id, envelope.route_options[0].id, "raw")

    manager_only = repo.create_run(tender["id"], "conversation", "Manager authored only")
    profiles.capture(tender["id"], manager_only["id"])
    repo.add_message(tender["id"], "manager", "An internal manager note.", run_id=manager_only["id"])
    manager_only_context = ManagerCreationContext(tender["id"], manager_only["id"], root_context.manager_profile_version, root_context.scope_id)
    with pytest.raises(ValueError, match="explicit engineer instruction"):
        routing.bind(manager_only_context, staff.staff.id, staff.work_order.id, envelope.route_options[0].id, "manager-only")

    later = repo.create_run(tender["id"], "conversation", "Follow-up engineer instruction")
    profiles.capture(tender["id"], later["id"])
    repo.add_message(tender["id"], "engineer", "Continue within the approved office scope.", run_id=later["id"])
    later_context = ManagerCreationContext(tender["id"], later["id"], root_context.manager_profile_version, root_context.scope_id)
    bound = routing.bind(later_context, staff.staff.id, staff.work_order.id, envelope.route_options[0].id, "later")
    assert bound.root_run_id == later["id"]


def test_root_intent_must_have_explicit_kind_matching_the_actual_run(tmp_path, monkeypatch):
    repo, tender, _connection, _model, _route, envelope, routing, staff, root_context, _artifact = _ready_binding_workspace(
        tmp_path, monkeypatch
    )
    malformed = [{"run_id": root_context.run_id, "tender_id": tender["id"], "plan_id": root_context.scope_id, "task_id": None}]
    with repo.db.connect(write=True) as conn:
        conn.execute(
            "UPDATE plan_review_approvals SET work_intents_json=? WHERE tender_id=? AND plan_id=? AND fingerprint=?",
            (json.dumps(malformed), tender["id"], root_context.scope_id, "f" * 64),
        )
    with pytest.raises(ValueError, match="explicit engineer instruction"):
        routing.bind(root_context, staff.staff.id, staff.work_order.id, envelope.route_options[0].id, "missing-kind")

    mismatched = [malformed[0] | {"kind": "task"}]
    with repo.db.connect(write=True) as conn:
        conn.execute(
            "UPDATE plan_review_approvals SET work_intents_json=? WHERE tender_id=? AND plan_id=? AND fingerprint=?",
            (json.dumps(mismatched), tender["id"], root_context.scope_id, "f" * 64),
        )
    with pytest.raises(ValueError, match="explicit engineer instruction"):
        routing.bind(root_context, staff.staff.id, staff.work_order.id, envelope.route_options[0].id, "mismatched-kind")


def test_bind_and_validate_require_the_root_manager_pin(tmp_path, monkeypatch):
    repo, tender, _connection, _model, _route, envelope, routing, staff, root_context, _artifact = _ready_binding_workspace(
        tmp_path, monkeypatch
    )
    binding = routing.bind(root_context, staff.staff.id, staff.work_order.id, envelope.route_options[0].id, "initial")
    with repo.db.connect(write=True) as conn:
        conn.execute("DELETE FROM office_manager_run_profiles WHERE run_id=?", (root_context.run_id,))
    with pytest.raises(ValueError, match="immutable Tender Manager profile pin"):
        routing.validate_binding(tender["id"], binding.id)
    with pytest.raises(ValueError, match="immutable Tender Manager profile pin"):
        routing.bind(root_context, staff.staff.id, staff.work_order.id, envelope.route_options[0].id, "missing-pin")

    current = ManagerProfileService(repo).get()
    newer = ManagerProfileService(repo).update(
        ManagerProfileEdit(
            expected_version=current.version,
            display_name=current.display_name,
            title=current.title,
            persona=current.persona,
            personality=current.personality,
            working_preferences=current.working_preferences,
        )
    )
    new_root = repo.create_run(tender["id"], "manager", "Mismatched pin")
    ManagerRunProfiles(repo).capture(tender["id"], new_root["id"])
    mismatched = ManagerCreationContext(tender["id"], new_root["id"], current.version, root_context.scope_id)
    with pytest.raises(ValueError, match="different Tender Manager profile version"):
        routing.bind(mismatched, staff.staff.id, staff.work_order.id, envelope.route_options[0].id, "mismatched-pin")
    assert newer.version == current.version + 1


def test_older_generated_profile_can_reuse_under_new_pinned_manager_root(tmp_path, monkeypatch):
    repo, tender, _connection, _model, _route, envelope, routing, staff, root_context, _artifact = _ready_binding_workspace(
        tmp_path, monkeypatch
    )
    manager = ManagerProfileService(repo)
    current = manager.get()
    newer = manager.update(
        ManagerProfileEdit(
            expected_version=current.version,
            display_name=current.display_name,
            title=current.title,
            persona=current.persona,
            personality=current.personality,
            working_preferences=current.working_preferences,
        )
    )
    root = repo.create_run(tender["id"], "conversation", "Reuse an existing colleague")
    ManagerRunProfiles(repo).capture(tender["id"], root["id"])
    repo.add_message(tender["id"], "engineer", "Reuse the prior colleague within scope.", run_id=root["id"])
    context = ManagerCreationContext(tender["id"], root["id"], newer.version, root_context.scope_id)
    binding = routing.bind(context, staff.staff.id, staff.work_order.id, envelope.route_options[0].id, "reuse-old-profile")
    assert binding.staff_version == staff.staff.version


def test_binding_validation_rejects_stale_source_before_provider_boundary(tmp_path, monkeypatch):
    repo, tender, _connection, _model, _route, envelope, routing, staff, root_context, _artifact = _ready_binding_workspace(
        tmp_path, monkeypatch
    )
    binding = routing.bind(root_context, staff.staff.id, staff.work_order.id, envelope.route_options[0].id, "source")
    repo.register_artifact(tender["id"], "Sources/spec.pdf", hashlib.sha256(b"changed").hexdigest(), 7, {"kind": "pdf", "status": "extracted", "segments": [{"locator": "page:1", "text": "changed"}]})
    with pytest.raises(ValueError, match="sources changed"):
        routing.validate_binding(tender["id"], binding.id)


def test_binding_validation_rejects_widened_artifact_snapshot(tmp_path, monkeypatch):
    repo, tender, _connection, _model, _route, envelope, routing, staff, root_context, artifact = _ready_binding_workspace(
        tmp_path, monkeypatch
    )
    binding = routing.bind(root_context, staff.staff.id, staff.work_order.id, envelope.route_options[0].id, "artifact")
    widened = [basis.model_dump(mode="json") for basis in binding.artifacts]
    widened.append({"artifact_id": artifact["id"], "version": artifact["version"], "content_hash": artifact["content_hash"]})
    with repo.db.connect(write=True) as conn:
        conn.execute("DROP TRIGGER office_route_bindings_immutable")
        conn.execute(
            "UPDATE office_route_bindings SET artifacts_json=? WHERE id=?",
            (json.dumps(widened), binding.id),
        )
    with pytest.raises(ValueError, match="artifact scope changed"):
        routing.validate_binding(tender["id"], binding.id)


def test_bind_rolls_back_binding_and_idempotency_receipt_with_outer_transaction(tmp_path, monkeypatch):
    repo, tender, _connection, _model, _route, envelope, routing, staff, root_context, _artifact = _ready_binding_workspace(
        tmp_path, monkeypatch
    )
    with pytest.raises(RuntimeError):
        with repo.atomic():
            routing.bind(root_context, staff.staff.id, staff.work_order.id, envelope.route_options[0].id, "rollback")
            raise RuntimeError("synthetic event append failed")
    with repo.db.connect() as conn:
        assert conn.execute("SELECT COUNT(*) FROM office_route_bindings").fetchone()[0] == 0
        assert conn.execute("SELECT COUNT(*) FROM office_route_binding_receipts").fetchone()[0] == 0


def test_legacy_plan_approval_without_delegation_grants_nothing(tmp_path, monkeypatch):
    repo, tender, *_ = configured_office(tmp_path, monkeypatch)
    plan = repo.create_plan(tender["id"], "Review", [{"title": "Review", "role": "Estimator", "description": "Review", "source_ids": []}])
    repo.approve_plan(tender["id"], plan["id"], "Synthetic plan approval")
    with repo.db.connect(write=True) as conn:
        conn.execute(
            "CREATE TABLE IF NOT EXISTS plan_review_approvals(id TEXT PRIMARY KEY,tender_id TEXT NOT NULL,plan_id TEXT NOT NULL,fingerprint TEXT NOT NULL,rationale TEXT NOT NULL,review_json TEXT NOT NULL,work_intents_json TEXT NOT NULL,created_at TEXT NOT NULL)"
        )
        conn.execute(
            "INSERT INTO plan_review_approvals VALUES(?,?,?,?,?,?,?,datetime('now'))",
            ("legacy-receipt", tender["id"], plan["id"], "a" * 64, "Legacy", json.dumps({"tasks": []}), "[]"),
        )
    with pytest.raises(ValueError, match="delegation"):
        StaffRoutingService(repo).approved_grant(tender["id"], plan["id"])


@pytest.mark.parametrize("review_json", [
    lambda envelope: {"envelope": envelope.model_dump(mode="json")},
    lambda envelope: {"delegation": None, "envelope": envelope.model_dump(mode="json")},
])
def test_existing_grant_rejects_missing_or_null_canonical_delegation(tmp_path, monkeypatch, review_json):
    repo, tender, _connection, _model, _route, envelope, routing, _staff, root_context, _artifact = _ready_binding_workspace(
        tmp_path, monkeypatch
    )
    with repo.db.connect(write=True) as conn:
        conn.execute(
            "UPDATE plan_review_approvals SET review_json=? WHERE tender_id=? AND plan_id=? AND fingerprint=?",
            (json.dumps(review_json(envelope)), tender["id"], root_context.scope_id, "f" * 64),
        )
    with pytest.raises(ValueError, match="delegation"):
        routing.approved_grant(tender["id"], root_context.scope_id)


def test_changed_same_review_grant_conflicts_without_replacing_snapshot(tmp_path, monkeypatch):
    repo, tender, _connections, connection, model, _policy, route = configured_office(tmp_path, monkeypatch)
    plan = repo.create_plan(tender["id"], "Review", [{"title": "Review", "role": "Estimator", "description": "Review", "source_ids": []}])
    envelope = _envelope(route, connection, model)
    service = StaffRoutingService(repo)
    policy_revision = AIPolicyService(repo).get(tender["id"])["revision"]
    first = service.save_reviewed_grant(tender["id"], plan["id"], "b" * 64, policy_revision, envelope)
    changed = envelope.model_copy(update={"purpose": "Different reviewed purpose"})
    with pytest.raises(OfficeConflict):
        service.save_reviewed_grant(tender["id"], plan["id"], "b" * 64, policy_revision, changed)
    with repo.db.connect() as conn:
        assert conn.execute("SELECT COUNT(*) FROM office_delegation_grants").fetchone()[0] == 1
    assert first.id
