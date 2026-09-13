"""Focused authority tests for plan-scoped delegation review."""

from __future__ import annotations

import hashlib
import json

import pytest

from quantix.ai_connections import AIConnectionService
from quantix.ai_policy import AIPolicyService
from quantix.manager_runtime import ManagerRunProfiles
from quantix.plan_review import PlanReviewService, _review_fingerprint
from quantix.repository import Repository
from quantix.staff_capabilities import capability_catalog
from quantix.staff_routing import StaffRoutingService


def _workspace(tmp_path, monkeypatch, *, artifact_count=1):
    repo = Repository(tmp_path)
    tender = repo.create_tender("Synthetic delegation Tender")
    artifact, _ = repo.register_artifact(
        tender["id"],
        "specification.pdf",
        hashlib.sha256(b"synthetic source").hexdigest(),
        16,
        {"kind": "pdf", "status": "extracted", "segments": [{"locator": "page:1", "text": "Scope"}]},
    )
    if artifact_count > 1:
        for index in range(max(0, artifact_count - 2)):
            repo.register_artifact(
                tender["id"],
                f"source-{index:04d}.pdf",
                hashlib.sha256(f"source-{index}".encode()).hexdigest(),
                16,
                {"kind": "pdf", "status": "extracted", "segments": []},
            )
        repo.register_artifact(
            tender["id"],
            "zzz-target.pdf",
            hashlib.sha256(b"target source").hexdigest(),
            16,
            {"kind": "pdf", "status": "extracted", "segments": [{"locator": "page:1", "text": "Target"}]},
        )
    plan = repo.create_plan(
        tender["id"],
        "Synthetic review",
        [{"title": "Review scope", "description": "Review scope", "role": "Any role", "source_ids": []}],
    )
    connections = AIConnectionService(repo)
    connection = connections.create(
        {
            "name": "Synthetic OpenAI",
            "provider_id": "openai",
            "protocol": "openai_responses",
            "base_url": "https://api.example.test/v1",
            "auth_type": "api_key",
            "billing": "metered",
            "credentials": {"api_key": "synthetic"},
            "session_only": True,
        }
    )
    model = connections.save_model(
        connection["id"],
        {
            "model_id": "synthetic-model",
            "display_name": "Synthetic model",
            "capabilities": {"tools": True, "structured_output": True, "web_search": True},
            "pricing": {
                "input_per_million": 1,
                "output_per_million": 2,
                "web_search_per_call": 0.01,
                "source": "synthetic",
                "as_of": "2026-09-09",
            },
        },
    )
    route = {
        "connection_id": connection["id"],
        "model_id": model["model_id"],
        "reasoning": None,
        "max_output_tokens": 2048,
        "web_search": True,
        "max_search_calls": 2,
    }
    policy = AIPolicyService(repo)
    policy.update(
        tender["id"],
        {
            "allowed_connection_ids": [connection["id"]],
            "manager": route,
            "specialist": route,
            "role_routes": {},
            "fallback_routes": [],
            "run_budget_usd": 10,
            "tender_budget_usd": 100,
            "max_requests": 12,
            "engineer_confirmed": True,
            "rationale": "Synthetic authority",
        },
    )
    policy.propose_team(tender["id"], plan["id"])
    # Readiness and the provider execution status are explicit test seams;
    # no provider client or account call is made by this fixture.
    monkeypatch.setattr("quantix.plan_review.ready_evidence", lambda *_args: {"check": {"status": "passed"}})
    monkeypatch.setattr("quantix.staff_routing.require_ready", lambda *_args: "synthetic")
    return repo, tender, plan, artifact, connection, route


def _service(repo, *, queue=None, scheduled=None):
    queue = queue or (lambda *_args: [])
    return PlanReviewService(
        repo,
        save_runs_in_transaction=queue,
        schedule_after_commit=(lambda *args: scheduled.append(args)) if scheduled is not None else None,
    )


def test_get_builds_typed_default_without_writing_proposal_or_grant(tmp_path, monkeypatch):
    repo, tender, plan, artifact, _connection, _route = _workspace(tmp_path, monkeypatch)
    service = _service(repo)
    before = repo.get_tender(tender["id"])["revision"]
    review = service.review(tender["id"], plan["id"])

    assert review["delegation_proposal_version"] == 0
    assert review["delegation"]["max_staff"] == 4
    assert review["delegation"]["max_assignments"] == 12
    assert review["delegation"]["max_depth"] == review["delegation"]["max_concurrency"] == 2
    assert review["delegation"]["artifacts"][0]["artifact_id"] == artifact["id"]
    assert review["delegation"]["route_options"][0]["id"] == review["delegation_options"]["route_options"][0]["id"]
    assert review["delegation"]["max_search_calls"] == 24
    assert repo.get_tender(tender["id"])["revision"] == before
    with repo.db.connect() as conn:
        assert conn.execute("SELECT 1 FROM delegation_proposals").fetchone() is None
        assert conn.execute(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name='office_delegation_grants'"
        ).fetchone() is None


def test_oversized_tender_can_select_and_approve_any_bounded_source(tmp_path, monkeypatch):
    repo, tender, plan, _artifact, _connection, _route = _workspace(
        tmp_path, monkeypatch, artifact_count=501
    )
    service = _service(repo)
    initial = service.review(tender["id"], plan["id"])
    assert initial["delegation"] is None
    assert any(item["code"] == "sources" for item in initial["blockers"])
    assert len(initial["delegation_options"]["artifacts"]) == 50

    page = service.artifact_options(
        tender["id"], plan["id"], offset=0, limit=10, query="zzz-target"
    )
    assert page.total == 1
    target = page.items[0]
    route_id = initial["delegation_options"]["route_options"][0]["id"]
    service.update_delegation(
        tender["id"],
        plan["id"],
        {
            "expected_version": 0,
            "source_scope": "selected_sources",
            "artifact_ids": [target.artifact_id],
            "tool_ids": ["read_source"],
            "allowed_draft_outputs": ["findings"],
            "route_option_ids": [route_id],
            "max_staff": 1,
            "max_assignments": 1,
            "max_requests": 2,
            "max_search_calls": 2,
        },
    )
    selected = service.review(tender["id"], plan["id"])
    assert selected["can_approve"]
    assert [item["artifact_id"] for item in selected["delegation"]["artifacts"]] == [target.artifact_id]

    def queue(tender_id, _plan_id, _plan, _review):
        root = repo.create_run(tender_id, "manager", "Coordinate selected source")
        ManagerRunProfiles(repo).capture(tender_id, root["id"])
        return [{"run": root, "task_id": None}]

    result = _service(repo, queue=queue).approve_and_start(
        tender["id"], plan["id"], {"fingerprint": selected["fingerprint"], "engineer_confirmed": True}
    )
    assert result["review"]["delegation"]["artifacts"][0]["artifact_id"] == target.artifact_id


def test_proposal_cas_and_changed_envelope_are_visible(tmp_path, monkeypatch):
    repo, tender, plan, _artifact, _connection, _route = _workspace(tmp_path, monkeypatch)
    service = _service(repo)
    initial = service.review(tender["id"], plan["id"])
    option_id = initial["delegation_options"]["route_options"][0]["id"]
    saved = service.update_delegation(
        tender["id"],
        plan["id"],
        {
            "expected_version": 0,
            "source_scope": "selected_sources",
            "artifact_ids": [initial["delegation_options"]["artifacts"][0]["artifact_id"]],
            "tool_ids": ["read_source"],
            "allowed_draft_outputs": ["findings"],
            "route_option_ids": [option_id],
            "max_staff": 2,
            "max_assignments": 3,
            "max_requests": 5,
            "max_search_calls": 2,
        },
    )
    assert saved["version"] == 1
    changed = service.review(tender["id"], plan["id"])
    assert changed["delegation_proposal_version"] == 1
    assert changed["delegation"]["tools"][0]["id"] == "read_source"
    assert changed["delegation"]["max_staff"] == 2
    assert changed["fingerprint"] != initial["fingerprint"]
    with pytest.raises(ValueError, match="changed since they were displayed"):
        service.update_delegation(
            tender["id"],
            plan["id"],
            {
                "expected_version": 0,
                "source_scope": "selected_sources",
                "artifact_ids": [changed["delegation_options"]["artifacts"][0]["artifact_id"]],
                "tool_ids": ["read_source"],
                "allowed_draft_outputs": ["findings"],
                "route_option_ids": [option_id],
                "max_staff": 2,
                "max_assignments": 3,
                "max_requests": 5,
                "max_search_calls": 2,
            },
        )


def test_stale_source_and_route_selections_block_without_fallback(tmp_path, monkeypatch):
    repo, tender, plan, artifact, _connection, _route = _workspace(tmp_path, monkeypatch)
    service = _service(repo)
    review = service.review(tender["id"], plan["id"])
    option_id = review["delegation"]["route_options"][0]["id"]
    service.update_delegation(
        tender["id"],
        plan["id"],
        {
            "expected_version": 0,
            "source_scope": "selected_sources",
            "artifact_ids": [artifact["id"]],
            "tool_ids": ["read_source"],
            "allowed_draft_outputs": ["findings"],
            "route_option_ids": [option_id],
            "max_staff": 1,
            "max_assignments": 1,
            "max_requests": 2,
            "max_search_calls": 2,
        },
    )
    repo.register_artifact(
        tender["id"],
        "specification.pdf",
        hashlib.sha256(b"changed source").hexdigest(),
        14,
        {"kind": "pdf", "status": "extracted", "segments": [{"locator": "page:1", "text": "Changed"}]},
    )
    current = service.review(tender["id"], plan["id"])
    assert current["delegation"] is not None
    assert any(item["code"] == "sources" for item in current["blockers"])
    assert current["delegation"]["artifacts"] == []


def test_stale_route_selection_is_blocked_without_selecting_another_route(tmp_path, monkeypatch):
    repo, tender, plan, _artifact, connection, _route = _workspace(tmp_path, monkeypatch)
    service = _service(repo)
    review = service.review(tender["id"], plan["id"])
    option_id = review["delegation"]["route_options"][0]["id"]
    service.update_delegation(
        tender["id"],
        plan["id"],
        {
            "expected_version": 0,
            "source_scope": "reviewed_tender",
            "artifact_ids": [item["artifact_id"] for item in review["delegation_options"]["artifacts"]],
            "tool_ids": ["read_source"],
            "allowed_draft_outputs": ["findings"],
            "route_option_ids": [option_id],
            "max_staff": 1,
            "max_assignments": 1,
            "max_requests": 2,
            "max_search_calls": 2,
        },
    )
    AIConnectionService(repo).rename(connection["id"], "Renamed account")
    current = service.review(tender["id"], plan["id"])
    assert current["delegation"] is None
    assert any("route choices" in item["detail"] for item in current["blockers"])


def test_changed_tool_catalog_blocks_saved_selection_and_timestamp_refresh_keeps_fingerprint(tmp_path, monkeypatch):
    repo, tender, plan, _artifact, connection, _route = _workspace(tmp_path, monkeypatch)
    service = _service(repo)
    review = service.review(tender["id"], plan["id"])
    option_id = review["delegation"]["route_options"][0]["id"]
    service.update_delegation(
        tender["id"],
        plan["id"],
        {
            "expected_version": 0,
            "source_scope": "reviewed_tender",
            "artifact_ids": [item["artifact_id"] for item in review["delegation_options"]["artifacts"]],
            "tool_ids": ["read_source"],
            "allowed_draft_outputs": ["findings"],
            "route_option_ids": [option_id],
            "max_staff": 1,
            "max_assignments": 1,
            "max_requests": 2,
            "max_search_calls": 2,
        },
    )
    before_timestamp_refresh = service.review(tender["id"], plan["id"])
    with repo.db.connect(write=True) as conn:
        row = conn.execute(
            "SELECT data_json FROM ai_connection_models WHERE connection_id=? AND model_id=?",
            (connection["id"], "synthetic-model"),
        ).fetchone()
        model_data = json.loads(row[0])
        model_data["updated_at"] = "2026-09-10T00:00:00+00:00"
        conn.execute(
            "UPDATE ai_connection_models SET data_json=? WHERE connection_id=? AND model_id=?",
            (json.dumps(model_data), connection["id"], "synthetic-model"),
        )
    assert service.review(tender["id"], plan["id"])["fingerprint"] == before_timestamp_refresh["fingerprint"]
    original_catalog = capability_catalog()
    monkeypatch.setattr(
        "quantix.plan_review.capability_catalog",
        lambda: tuple(item for item in original_catalog if item.id != "read_source"),
    )
    changed = service.review(tender["id"], plan["id"])
    assert any(item["code"] == "configuration" for item in changed["blockers"])
    assert changed["delegation"]["tools"] == []


def test_approval_saves_exact_delegation_receipt_and_manager_intent_once(tmp_path, monkeypatch):
    repo, tender, plan, _artifact, _connection, _route = _workspace(tmp_path, monkeypatch)
    scheduled = []

    def queue(tender_id, plan_id, _plan, _review):
        root = repo.create_run(tender_id, "manager", "Coordinate approved work")
        ManagerRunProfiles(repo).capture(tender_id, root["id"])
        return [{"run": root, "task_id": None}]

    service = _service(repo, queue=queue, scheduled=scheduled)
    review = service.review(tender["id"], plan["id"])
    result = service.approve_and_start(
        tender["id"],
        plan["id"],
        {"fingerprint": review["fingerprint"], "engineer_confirmed": True},
    )
    again = service.approve_and_start(
        tender["id"],
        plan["id"],
        {"fingerprint": review["fingerprint"], "engineer_confirmed": True},
    )
    assert result["work_intents"][0]["kind"] == "manager"
    assert result["work_intents"][0]["task_id"] is None
    assert again["work_intents"] == result["work_intents"]
    assert len(scheduled) == 1
    approved_view = service.review(tender["id"], plan["id"])
    assert approved_view["can_approve"] is False
    assert approved_view["delegation"] == result["review"]["delegation"]
    assert "already approved" in approved_view["ai_summary"]
    with repo.db.connect() as conn:
        receipt = conn.execute(
            "SELECT review_json FROM plan_review_approvals WHERE plan_id=?", (plan["id"],)
        ).fetchone()
        assert json.loads(receipt[0])["delegation"] == result["review"]["delegation"]
        assert conn.execute("SELECT COUNT(*) FROM office_delegation_grants WHERE plan_id=?", (plan["id"],)).fetchone()[0] == 1


def test_queue_failure_rolls_back_delegation_grant_and_receipt(tmp_path, monkeypatch):
    repo, tender, plan, _artifact, _connection, _route = _workspace(tmp_path, monkeypatch)

    def queue(*_args):
        root = repo.create_run(tender["id"], "manager", "Will be rolled back")
        ManagerRunProfiles(repo).capture(tender["id"], root["id"])
        raise RuntimeError("synthetic queue failure")

    service = _service(repo, queue=queue)
    review = service.review(tender["id"], plan["id"])
    with pytest.raises(RuntimeError, match="queue failure"):
        service.approve_and_start(
            tender["id"], plan["id"], {"fingerprint": review["fingerprint"], "engineer_confirmed": True}
        )
    assert repo.get_plan(tender["id"], plan["id"])["status"] == "proposed"
    with repo.db.connect() as conn:
        assert conn.execute(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name='office_delegation_grants'"
        ).fetchone() is None
        assert conn.execute("SELECT 1 FROM plan_review_approvals").fetchone() is None
        assert conn.execute("SELECT 1 FROM runs").fetchone() is None


def test_historical_task_receipt_replays_without_delegation_or_reschedule(tmp_path, monkeypatch):
    repo, tender, plan, _artifact, _connection, _route = _workspace(tmp_path, monkeypatch)
    scheduled = []

    def unexpected_queue(*_args):
        raise AssertionError("A historical approval replay must not queue new work.")

    service = _service(repo, queue=unexpected_queue, scheduled=scheduled)
    current = service.review(tender["id"], plan["id"])
    historical = dict(current)
    historical.pop("delegation", None)
    historical.pop("delegation_proposal_version", None)
    historical.pop("delegation_options", None)
    historical.pop("fingerprint", None)
    historical["fingerprint"] = _review_fingerprint(historical)
    repo.approve_plan(tender["id"], plan["id"], "Legacy task approval")
    run = repo.create_run(tender["id"], "task", plan["tasks"][0]["description"])
    intent = {
        "id": "historical-intent",
        "run_id": run["id"],
        "tender_id": tender["id"],
        "plan_id": plan["id"],
        "task_id": plan["tasks"][0]["id"],
        "kind": "task",
        "instruction": run["instruction"],
        "status": "queued",
        "created_at": run["created_at"],
    }
    with repo.db.connect(write=True) as conn:
        conn.execute(
            "INSERT INTO plan_review_approvals VALUES(?,?,?,?,?,?,?,datetime('now'))",
            (
                "legacy",
                tender["id"],
                plan["id"],
                historical["fingerprint"],
                "Legacy",
                json.dumps(historical),
                json.dumps([intent]),
            ),
        )
    approved_view = service.review(tender["id"], plan["id"])
    assert approved_view["delegation"] is None
    assert any(item["code"] == "plan" for item in approved_view["blockers"])
    assert "new proposed work plan" in approved_view["ai_summary"]
    result = service.approve_and_start(
        tender["id"],
        plan["id"],
        {"fingerprint": historical["fingerprint"], "engineer_confirmed": True},
    )
    assert result["review"]["delegation"] is None
    assert result["work_intents"] == [intent]
    assert scheduled == []
    with pytest.raises(ValueError, match="delegation"):
        StaffRoutingService(repo).approved_grant(tender["id"], plan["id"])
    with repo.db.connect() as conn:
        assert conn.execute("SELECT COUNT(*) FROM office_delegation_grants").fetchone()[0] == 0
        assert conn.execute("SELECT COUNT(*) FROM runs").fetchone()[0] == 1


def test_new_approval_requires_one_pinned_manager_root(tmp_path, monkeypatch):
    repo, tender, plan, _artifact, _connection, _route = _workspace(tmp_path, monkeypatch)

    def queue_without_root(*_args):
        return []

    service = _service(repo, queue=queue_without_root)
    review = service.review(tender["id"], plan["id"])
    with pytest.raises(ValueError, match="one pinned Manager root"):
        service.approve_and_start(
            tender["id"],
            plan["id"],
            {"fingerprint": review["fingerprint"], "engineer_confirmed": True},
        )
    assert repo.get_plan(tender["id"], plan["id"])["status"] == "proposed"
    with repo.db.connect() as conn:
        assert conn.execute("SELECT COUNT(*) FROM runs").fetchone()[0] == 0


@pytest.mark.parametrize("queue_shape", ["task", "mixed", "multiple", "cross_tender", "missing_pin"])
def test_new_approval_rejects_non_manager_or_unpinned_root(tmp_path, monkeypatch, queue_shape):
    repo, tender, plan, _artifact, _connection, _route = _workspace(tmp_path, monkeypatch)

    def queue(tender_id, _plan_id, _plan, _review):
        if queue_shape == "cross_tender":
            other = repo.create_tender("Other synthetic Tender")
            root = repo.create_run(other["id"], "manager", "Wrong Tender root")
            ManagerRunProfiles(repo).capture(other["id"], root["id"])
            return [{"run": root, "task_id": None}]
        if queue_shape == "task":
            return [{"run": repo.create_run(tender_id, "task", "Unexpected task root"), "task_id": None}]
        root = repo.create_run(tender_id, "manager", "Synthetic Manager root")
        if queue_shape != "missing_pin":
            ManagerRunProfiles(repo).capture(tender_id, root["id"])
        if queue_shape == "mixed":
            task = repo.create_run(tender_id, "task", "Unexpected task root")
            return [{"run": root, "task_id": None}, {"run": task, "task_id": None}]
        if queue_shape == "multiple":
            second = repo.create_run(tender_id, "manager", "Second Manager root")
            ManagerRunProfiles(repo).capture(tender_id, second["id"])
            return [{"run": root, "task_id": None}, {"run": second, "task_id": None}]
        return [{"run": root, "task_id": None}]

    service = _service(repo, queue=queue)
    review = service.review(tender["id"], plan["id"])
    with pytest.raises(ValueError, match="one pinned Manager root|Manager profile"):
        service.approve_and_start(
            tender["id"],
            plan["id"],
            {"fingerprint": review["fingerprint"], "engineer_confirmed": True},
        )
    assert repo.get_plan(tender["id"], plan["id"])["status"] == "proposed"
    with repo.db.connect() as conn:
        assert conn.execute("SELECT COUNT(*) FROM runs").fetchone()[0] == 0


def test_superseded_unapproved_plan_display_is_historical_and_directs_current_plan(tmp_path, monkeypatch):
    repo, tender, plan, _artifact, _connection, _route = _workspace(tmp_path, monkeypatch)
    replacement = repo.create_plan(
        tender["id"],
        "Replacement review",
        [{"title": "Replacement task", "description": "Use the current package", "role": "Any role", "source_ids": []}],
    )

    review = _service(repo).review(tender["id"], plan["id"])

    assert repo.get_plan(tender["id"], plan["id"])["status"] == "superseded"
    assert replacement["status"] == "proposed"
    assert review["can_approve"] is False
    assert review["delegation"] is None
    assert "superseded" in review["ai_summary"].lower()
    assert "current" in review["ai_summary"].lower()
    assert "already approved" not in review["ai_summary"].lower()
    assert any("superseded" in item["detail"].lower() for item in review["blockers"])


def test_superseded_approved_plan_keeps_historical_scope_without_execution_authority(tmp_path, monkeypatch):
    repo, tender, plan, _artifact, _connection, _route = _workspace(tmp_path, monkeypatch)

    def queue(tender_id, _plan_id, _plan, _review):
        root = repo.create_run(tender_id, "manager", "Coordinate approved work")
        ManagerRunProfiles(repo).capture(tender_id, root["id"])
        return [{"run": root, "task_id": None}]

    service = _service(repo, queue=queue)
    approved = service.approve_and_start(
        tender["id"],
        plan["id"],
        {"fingerprint": service.review(tender["id"], plan["id"])["fingerprint"], "engineer_confirmed": True},
    )
    replacement = repo.create_plan(
        tender["id"],
        "Replacement review",
        [{"title": "Replacement task", "description": "Use the current package", "role": "Any role", "source_ids": []}],
    )
    repo.approve_plan(tender["id"], replacement["id"], "Synthetic replacement approval")

    review = service.review(tender["id"], plan["id"])

    assert repo.get_plan(tender["id"], plan["id"])["status"] == "superseded"
    assert review["can_approve"] is False
    assert review["delegation"] == approved["review"]["delegation"]
    assert "superseded" in review["ai_summary"].lower()
    assert "historical" in review["ai_summary"].lower()
    assert "current" in review["ai_summary"].lower()
    assert "may assign or reuse" not in review["ai_summary"].lower()
    assert any("superseded" in item["detail"].lower() for item in review["blockers"])
