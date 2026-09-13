"""Aggregate root request, spending and search accounting for dynamic staff."""

import json

import pytest
from test_catalog_authority import configured_office

from quantix.ai_policy import AIPolicyService, BudgetMeter
from quantix.ai_setup_store import SetupStore
from quantix.db import dump, new_id, now
from quantix.jobs import JobManager
from quantix.manager_profile import ManagerProfileService
from quantix.manager_runtime import ManagerRunProfiles
from quantix.plan_review import PlanReviewService
from quantix.staff_assignments import StaffAssignmentService
from quantix.staff_budget import OfficeBudgetMeter
from quantix.staff_models import ManagerCreationContext
from quantix.staff_routing import StaffRoutingService
from quantix.staff_store import StaffStore


@pytest.mark.asyncio
async def test_narrowed_request_resolves_its_bound_option_when_other_caps_overlap(tmp_path, monkeypatch):
    from test_staff_routing import _profile, _work_order

    repo, tender, _, account, _, policy, route = configured_office(tmp_path, monkeypatch, search=True)
    larger = route | {"max_output_tokens": 4096, "max_search_calls": 5}
    policy.update(tender["id"], {"allowed_connection_ids": [account["id"]], "manager": route,
        "specialist": larger, "run_budget_usd": 10, "tender_budget_usd": 100, "max_requests": 12,
        "engineer_confirmed": True, "rationale": "Synthetic overlapping caps"})
    plan = repo.create_plan(tender["id"], "Overlapping route choices", [{"title": "Review", "description": "Review", "role": "Arbitrary", "source_ids": []}])
    jobs = JobManager(repo, object())
    service = PlanReviewService(repo, save_runs_in_transaction=jobs.queue_approved_plan_runs)
    review = service.review(tender["id"], plan["id"])
    approved = service.approve_and_start(tender["id"], plan["id"], {"fingerprint": review["fingerprint"], "engineer_confirmed": True})
    root_id = approved["work_intents"][0]["run_id"]
    pinned = ManagerRunProfiles(repo).get(tender["id"], root_id)
    context = ManagerCreationContext(tender["id"], root_id, pinned.version, plan["id"])
    staff = StaffStore(repo).create_generated(context, _profile(), _work_order(), "overlap-staff")
    option = next(item for item in review["delegation"]["route_options"] if item["route"]["max_output_tokens"] == 4096)
    routing = StaffRoutingService(repo)
    binding = routing.bind(context, staff.staff.id, staff.work_order.id, option["id"], "overlap-binding")
    assignments = StaffAssignmentService(repo)
    queued = assignments.queue(context, binding.id, "overlap-assignment")
    running = assignments.start(tender["id"], queued.id, queued.revision)
    actual = route | {"max_output_tokens": 1024, "max_search_calls": 1}
    manager_meter = OfficeBudgetMeter(policy, tender["id"], root_id, actual, plan_id=plan["id"])
    staff_meter = OfficeBudgetMeter(policy, tender["id"], root_id, actual, plan_id=plan["id"],
        assignment_id=running.id, staff_id=staff.staff.id, binding_id=binding.id)
    for meter in (manager_meter, staff_meter):
        reservation = await meter.before_request(100, 100)
        await meter.on_response({"requests": 1, "input_tokens": 100, "output_tokens": 10,
                                 "web_search_calls": 0, "usage_complete": True}, reservation)


def _approved_root(tmp_path, monkeypatch, *, max_requests=3, search=True):
    repo, tender, *_rest = configured_office(tmp_path, monkeypatch, max_requests=max_requests, search=search)
    plan = repo.create_plan(tender["id"], "Synthetic delegated plan", [{
        "title": "Review concrete", "role": "Estimator", "description": "Review concrete", "source_ids": [],
    }])
    jobs = JobManager(repo, object())
    service = PlanReviewService(repo, save_runs_in_transaction=jobs.queue_approved_plan_runs)
    review = service.review(tender["id"], plan["id"])
    result = service.approve_and_start(tender["id"], plan["id"], {
        "fingerprint": review["fingerprint"], "engineer_confirmed": True, "rationale": "Synthetic approval",
    })
    for intent in result["work_intents"]:
        repo.update_run(intent["run_id"], status="completed")
    root = repo.create_run(tender["id"], "manager", "Coordinate the reviewed Tender work.")
    ManagerRunProfiles(repo).capture(tender["id"], root["id"])
    repo.add_message(tender["id"], "engineer", "Coordinate the reviewed Tender work.", run_id=root["id"])
    return repo, tender, plan, root, service.policy, result["review"]["delegation"]["route_options"][0]["route"]

@pytest.mark.asyncio
async def test_root_meter_counts_requests_and_search_reservations_across_meters(tmp_path, monkeypatch):
    repo, tender, plan, root, policy, route = _approved_root(tmp_path, monkeypatch, max_requests=2, search=True)
    meter = OfficeBudgetMeter(policy, tender["id"], root["id"], route, plan_id=plan["id"])

    first = await meter.before_request(100, 100)
    await meter.on_response(
        {"requests": 1, "input_tokens": 100, "output_tokens": 20, "web_search_calls": 3, "usage_complete": True},
        first,
    )
    second_meter = OfficeBudgetMeter(policy, tender["id"], root["id"], route, plan_id=plan["id"])
    second = await second_meter.before_request(100, 100)
    with pytest.raises(ValueError, match="request|search|allowance|budget"):
        await second_meter.before_request(100, 100)
    await second_meter.on_response(
        {"requests": 1, "input_tokens": 100, "output_tokens": 20, "web_search_calls": 3, "usage_complete": True},
        second,
    )


@pytest.mark.asyncio
async def test_root_meter_keeps_unknown_usage_reserved_and_duplicate_response_is_idempotent(tmp_path, monkeypatch):
    repo, tender, plan, root, policy, route = _approved_root(tmp_path, monkeypatch, max_requests=3, search=True)
    meter = OfficeBudgetMeter(policy, tender["id"], root["id"], route, plan_id=plan["id"])
    reservation = await meter.before_request(100, 100)
    usage = {"requests": 1, "input_tokens": 100, "output_tokens": 20, "usage_complete": False}
    await meter.on_response(usage, reservation)
    await meter.on_response(usage, reservation)
    with repo.db.connect() as conn:
        data = json.loads(conn.execute("SELECT data_json FROM ai_usage WHERE id=?", (reservation,)).fetchone()[0])
    assert data["status"] == "uncertain"
    assert data["reserved_usd"] > 0
    with pytest.raises(ValueError, match="different|mismatch"):
        await meter.on_response({**usage, "output_tokens": 21}, reservation)


@pytest.mark.asyncio
async def test_partial_search_overrun_is_retained_and_stops_later_root_work(tmp_path, monkeypatch):
    repo, tender, plan, root, policy, route = _approved_root(tmp_path, monkeypatch, max_requests=3, search=True)
    meter = OfficeBudgetMeter(policy, tender["id"], root["id"], route, plan_id=plan["id"])
    reservation = await meter.before_request(100, 100)
    await meter.on_response(
        {
            "requests": 1,
            "input_tokens": 100,
            "output_tokens": 20,
            "web_search_calls": 4,
            "usage_complete": False,
        },
        reservation,
    )
    with repo.db.connect() as conn:
        data = json.loads(conn.execute("SELECT data_json FROM ai_usage WHERE id=?", (reservation,)).fetchone()[0])
    assert data["status"] == "uncertain"
    assert data["reserved_search_calls"] == route["max_search_calls"]
    assert data["web_search_calls"] == 4
    assert data["search_allowance_overrun"] is True
    with pytest.raises(ValueError, match="exceeded|stopped"):
        await OfficeBudgetMeter(policy, tender["id"], root["id"], route, plan_id=plan["id"]).before_request(100, 100)


@pytest.mark.asyncio
async def test_budget_meter_search_accounting_carries_into_office_meter(tmp_path, monkeypatch):
    repo, tender, plan, root, policy, route = _approved_root(tmp_path, monkeypatch, max_requests=3, search=True)
    direct = BudgetMeter(policy, tender["id"], root["id"], route)
    complete = await direct.before_request(100, 100)
    await direct.on_response(
        {"requests": 1, "input_tokens": 100, "output_tokens": 20, "web_search_calls": 3, "usage_complete": True},
        complete,
    )
    uncertain = await direct.before_request(100, 100)
    await direct.on_response(
        {"requests": 1, "input_tokens": 100, "output_tokens": 20, "usage_complete": False},
        uncertain,
    )
    office = OfficeBudgetMeter(policy, tender["id"], root["id"], route, plan_id=plan["id"])
    assert office.remaining_search_calls() == 3
    with repo.db.connect() as conn:
        rows = [json.loads(row[0]) for row in conn.execute(
            "SELECT data_json FROM ai_usage WHERE tender_id=? AND run_id=? ORDER BY created_at",
            (tender["id"], root["id"]),
        )]
    assert rows[0]["search_accounting_version"] == 1
    assert rows[0]["search_usage_complete"] is True
    assert rows[1]["search_usage_uncertain"] is True


@pytest.mark.asyncio
async def test_missing_historical_search_metadata_blocks_new_search_admission(tmp_path, monkeypatch):
    repo, tender, plan, root, policy, route = _approved_root(tmp_path, monkeypatch, max_requests=3, search=True)
    with repo.atomic() as conn:
        conn.execute(
            "INSERT INTO ai_usage VALUES(?,?,?,?,?,?,?)",
            (
                new_id(),
                root["id"],
                tender["id"],
                route["connection_id"],
                route["model_id"],
                dump({"status": "reported", "requests": 1, "estimated_cost_usd": 0, "reserved_usd": 0}),
                now(),
            ),
        )
    office = OfficeBudgetMeter(policy, tender["id"], root["id"], route, plan_id=plan["id"])
    assert office.remaining_search_calls() == 0
    with pytest.raises(ValueError, match="incomplete historical"):
        await office.before_request(100, 100)


@pytest.mark.asyncio
async def test_office_meter_accepts_only_narrower_output_and_search_limits(tmp_path, monkeypatch):
    repo, tender, plan, root, policy, route = _approved_root(tmp_path, monkeypatch, max_requests=3, search=True)
    narrowed = {
        **route,
        "max_output_tokens": route["max_output_tokens"] // 2,
        "max_search_calls": 1,
    }
    meter = OfficeBudgetMeter(policy, tender["id"], root["id"], narrowed, plan_id=plan["id"])
    reservation = await meter.before_request(100, narrowed["max_output_tokens"])
    await meter.on_response(
        {"requests": 1, "input_tokens": 100, "output_tokens": 20, "web_search_calls": 1, "usage_complete": True},
        reservation,
    )
    widened = {**narrowed, "max_search_calls": route["max_search_calls"] + 1}
    with pytest.raises(ValueError, match="route|alternative|approved"):
        await OfficeBudgetMeter(policy, tender["id"], root["id"], widened, plan_id=plan["id"]).before_request(100, 100)


@pytest.mark.asyncio
async def test_staff_late_response_is_recordable_after_assignment_stop(tmp_path, monkeypatch):
    from test_staff_routing import _ready_binding_workspace

    repo, tender, _connection, _model, route, envelope, routing, staff, root_context, _artifact = _ready_binding_workspace(
        tmp_path, monkeypatch, requested_tool_ids=[]
    )
    binding = routing.bind(root_context, staff.staff.id, staff.work_order.id, envelope.route_options[0].id, "late-binding")
    assignments = StaffAssignmentService(repo)
    assignment = assignments.queue(root_context, binding.id, "late-assignment")
    assignment = assignments.start(root_context.tender_id, assignment.id, assignment.revision)
    policy = AIPolicyService(repo)
    meter = OfficeBudgetMeter(
        policy,
        tender["id"],
        root_context.run_id,
        route,
        plan_id=root_context.scope_id,
        assignment_id=assignment.id,
        staff_id=staff.staff.id,
        binding_id=binding.id,
    )
    reservation = await meter.before_request(100, 100)
    assignments.cancel(tender["id"], assignment.id)
    await meter.on_response(
        {"requests": 1, "input_tokens": 100, "output_tokens": 10, "usage_complete": True},
        reservation,
    )


@pytest.mark.asyncio
async def test_two_staff_price_cards_share_one_root_money_and_search_ledger(tmp_path, monkeypatch):
    from test_staff_routing import _profile, _work_order

    repo, tender, connections, account, model, policy, route = configured_office(
        tmp_path, monkeypatch, max_requests=4, search=True
    )
    second_account = connections.create(
        {
            "name": "Synthetic expensive API",
            "provider_id": "openai",
            "protocol": "openai_responses",
            "base_url": "https://expensive.example.test/v1",
            "auth_type": "api_key",
            "billing": "metered",
            "credentials": {"api_key": "synthetic-expensive-key"},
            "session_only": True,
        }
    )
    second_model = connections.save_model(
        second_account["id"],
        {
            "model_id": "expensive-model",
            "display_name": "Synthetic expensive model",
            "capabilities": model["capabilities"],
            "pricing": {
                "input_per_million": 100,
                "output_per_million": 200,
                "web_search_per_call": 0.3,
                "source": "synthetic",
                "as_of": "2026-09-10",
            },
        },
        source="provider",
    )
    SetupStore(repo).update(
        second_account["id"],
        stage="ready",
        selected_model_id=second_model["model_id"],
        check={"status": "passed", "model_id": second_model["model_id"]},
        checked_revision=second_account["revision"],
        checked_component_version="synthetic",
    )
    SetupStore(repo).save_model_check(
        second_account["id"],
        second_model["model_id"],
        {
            "check": {"status": "passed", "model_id": second_model["model_id"]},
            "checked_revision": second_account["revision"],
            "checked_component_version": "synthetic",
        },
    )
    second_route = {
        "connection_id": second_account["id"],
        "model_id": second_model["model_id"],
        "reasoning": route["reasoning"],
        "max_output_tokens": route["max_output_tokens"],
        "web_search": True,
        "max_search_calls": route["max_search_calls"],
    }
    policy.update(
        tender["id"],
        {
            "allowed_connection_ids": [account["id"], second_account["id"]],
            "manager": route,
            "specialist": second_route,
            "role_routes": {},
            "fallback_routes": [],
            "run_budget_usd": 0.93,
            "tender_budget_usd": 100,
            "max_requests": 4,
            "engineer_confirmed": True,
            "rationale": "Synthetic two-price-card budget approval",
        },
    )
    plan = repo.create_plan(tender["id"], "Synthetic two-route plan", [{
        "title": "Review concrete", "role": "Estimator", "description": "Review concrete", "source_ids": [],
    }])
    jobs = JobManager(repo, object())
    service = PlanReviewService(repo, save_runs_in_transaction=jobs.queue_approved_plan_runs)
    review = service.review(tender["id"], plan["id"])
    result = service.approve_and_start(tender["id"], plan["id"], {
        "fingerprint": review["fingerprint"], "engineer_confirmed": True, "rationale": "Synthetic approval",
    })
    for intent in result["work_intents"]:
        repo.update_run(intent["run_id"], status="completed")
    root = repo.create_run(tender["id"], "manager", "Coordinate the reviewed Tender work.")
    manager = ManagerProfileService(repo)
    ManagerRunProfiles(repo).capture(tender["id"], root["id"])
    repo.add_message(tender["id"], "engineer", "Coordinate the reviewed Tender work.", run_id=root["id"])
    planning = repo.create_run(tender["id"], "conversation", "Create synthetic staff")
    context = ManagerCreationContext(tender["id"], planning["id"], manager.get().version, plan["id"])
    staff_store = StaffStore(repo)
    first_staff = staff_store.create_generated(context, _profile(display_name="Low cost staff"), _work_order(), "staff-one")
    second_staff = staff_store.create_generated(context, _profile(display_name="High cost staff"), _work_order(), "staff-two")
    root_context = ManagerCreationContext(tender["id"], root["id"], manager.get().version, plan["id"])
    delegation = result["review"]["delegation"]
    first_option = next(item for item in delegation["route_options"] if item["route"]["connection_id"] == account["id"])
    second_option = next(item for item in delegation["route_options"] if item["route"]["connection_id"] == second_account["id"])
    routing = StaffRoutingService(repo)
    first_binding = routing.bind(root_context, first_staff.staff.id, first_staff.work_order.id, first_option["id"], "binding-one")
    second_binding = routing.bind(root_context, second_staff.staff.id, second_staff.work_order.id, second_option["id"], "binding-two")
    assignments = StaffAssignmentService(repo)
    first_assignment = assignments.queue(root_context, first_binding.id, "assignment-one")
    second_assignment = assignments.queue(root_context, second_binding.id, "assignment-two")
    first_assignment = assignments.start(tender["id"], first_assignment.id, first_assignment.revision)
    second_assignment = assignments.start(tender["id"], second_assignment.id, second_assignment.revision)
    first_meter = OfficeBudgetMeter(
        policy, tender["id"], root["id"], route, plan_id=plan["id"],
        assignment_id=first_assignment.id, staff_id=first_staff.staff.id, binding_id=first_binding.id,
    )
    second_meter = OfficeBudgetMeter(
        policy, tender["id"], root["id"], second_route, plan_id=plan["id"],
        assignment_id=second_assignment.id, staff_id=second_staff.staff.id, binding_id=second_binding.id,
    )
    first_reservation = await first_meter.before_request(100, 100)
    await first_meter.on_response(
        {"requests": 1, "input_tokens": 100, "output_tokens": 100, "web_search_calls": 3, "usage_complete": True},
        first_reservation,
    )
    second_reservation = await second_meter.before_request(0, 128)
    await second_meter.on_response(
        {"requests": 1, "input_tokens": 0, "output_tokens": 128, "web_search_calls": 3, "usage_complete": True},
        second_reservation,
    )
    with repo.db.connect() as conn:
        amounts = [
            json.loads(row[0])["estimated_cost_usd"]
            for row in conn.execute(
                "SELECT data_json FROM ai_usage WHERE tender_id=? AND run_id=? ORDER BY created_at",
                (tender["id"], root["id"]),
            )
        ]
    assert amounts[1] > amounts[0]
    with pytest.raises(ValueError, match="budget|allowance"):
        await OfficeBudgetMeter(policy, tender["id"], root["id"], route, plan_id=plan["id"]).before_request(0, 128)


@pytest.mark.asyncio
async def test_search_overrun_is_recorded_and_stops_later_root_work(tmp_path, monkeypatch):
    repo, tender, plan, root, policy, route = _approved_root(tmp_path, monkeypatch, max_requests=3, search=True)
    meter = OfficeBudgetMeter(policy, tender["id"], root["id"], route, plan_id=plan["id"])
    reservation = await meter.before_request(100, 100)
    await meter.on_response(
        {"requests": 1, "input_tokens": 100, "output_tokens": 20, "web_search_calls": 4, "usage_complete": True},
        reservation,
    )
    with repo.db.connect() as conn:
        data = json.loads(conn.execute("SELECT data_json FROM ai_usage WHERE id=?", (reservation,)).fetchone()[0])
    assert data["search_allowance_overrun"] is True
    with pytest.raises(ValueError, match="exceeded|stopped"):
        await OfficeBudgetMeter(policy, tender["id"], root["id"], route, plan_id=plan["id"]).before_request(100, 100)


@pytest.mark.asyncio
async def test_response_from_a_different_route_cannot_update_a_reservation(tmp_path, monkeypatch):
    repo, tender, plan, root, policy, route = _approved_root(tmp_path, monkeypatch, max_requests=3, search=False)
    meter = OfficeBudgetMeter(policy, tender["id"], root["id"], route, plan_id=plan["id"])
    reservation = await meter.before_request(100, 100)
    changed_route = {**route, "max_output_tokens": route["max_output_tokens"] + 128}
    changed = OfficeBudgetMeter(policy, tender["id"], root["id"], changed_route, plan_id=plan["id"])
    with pytest.raises(ValueError, match="identity|route"):
        await changed.on_response(
            {"requests": 1, "input_tokens": 100, "output_tokens": 10, "usage_complete": True}, reservation
        )


@pytest.mark.asyncio
async def test_staff_meter_requires_exact_assignment_binding_and_shares_root(tmp_path, monkeypatch):
    from test_staff_routing import _ready_binding_workspace

    repo, tender, _connection, _model, route, envelope, routing, staff, root_context, _artifact = _ready_binding_workspace(
        tmp_path, monkeypatch, requested_tool_ids=[]
    )
    binding = routing.bind(root_context, staff.staff.id, staff.work_order.id, envelope.route_options[0].id, "budget-binding")
    assignment = StaffAssignmentService(repo).queue(root_context, binding.id, "budget-assignment")
    policy = AIPolicyService(repo)
    meter = OfficeBudgetMeter(
        policy,
        tender["id"],
        root_context.run_id,
        route,
        plan_id=root_context.scope_id,
        assignment_id=assignment.id,
        staff_id=staff.staff.id,
        binding_id=binding.id,
    )
    with pytest.raises(ValueError, match="running"):
        await meter.before_request(100, 100)
    assignment = StaffAssignmentService(repo).start(root_context.tender_id, assignment.id, assignment.revision)
    reservation = await meter.before_request(100, 100)
    await meter.on_response({"requests": 1, "input_tokens": 100, "output_tokens": 10, "usage_complete": True}, reservation)
    with repo.db.connect() as conn:
        data = json.loads(conn.execute("SELECT data_json FROM ai_usage WHERE id=?", (reservation,)).fetchone()[0])
    assert data["root_run_id"] == root_context.run_id
    assert data["actor_id"] == staff.staff.id
    assert data["assignment_id"] == assignment.id
    assert data["binding_id"] == binding.id


def test_validate_root_requires_the_saved_grant_and_current_policy(tmp_path, monkeypatch):
    from test_staff_routing import _ready_binding_workspace

    repo, tender, _connection, _model, _route, _envelope, routing, _staff, root_context, _artifact = _ready_binding_workspace(
        tmp_path, monkeypatch
    )
    grant = routing.validate_root(tender["id"], root_context.run_id, root_context.scope_id)
    assert grant.plan_id == root_context.scope_id
    with repo.db.connect(write=True) as conn:
        row = conn.execute("SELECT revision FROM tender_ai_policy WHERE tender_id=?", (tender["id"],)).fetchone()
        conn.execute("UPDATE tender_ai_policy SET revision=? WHERE tender_id=?", (row["revision"] + 1, tender["id"]))
    with pytest.raises(ValueError, match="permissions changed"):
        routing.validate_root(tender["id"], root_context.run_id, root_context.scope_id)

@pytest.mark.asyncio
@pytest.mark.parametrize('office', [True, False])
async def test_native_combined_overrun_retains_hold_and_blocks_next_admission(tmp_path, monkeypatch, office):
    from quantix.ai_policy import BudgetMeter
    repo, tender, plan, root, policy, route = _approved_root(tmp_path, monkeypatch, max_requests=3, search=True)
    def meter():
        return (OfficeBudgetMeter(policy, tender['id'], root['id'], route, plan_id=plan['id'])
                if office else BudgetMeter(policy, tender['id'], root['id'], route))
    current = meter()
    reservation = await current.before_request(100, 100)
    await current.on_response({'requests': 1, 'input_tokens': 100, 'output_tokens': 20,
        'web_search_calls': 2, 'usage_complete': False, 'native_call_limit_overrun': True}, reservation)
    with repo.db.connect() as conn:
        data = json.loads(conn.execute('SELECT data_json FROM ai_usage WHERE id=?', (reservation,)).fetchone()[0])
    assert data['native_call_limit_overrun'] is True and data['reserved_usd'] > 0
    with pytest.raises(ValueError, match='exceeded'):
        await meter().before_request(100, 100)
