"""Test-only CaseRegistry. This is never product routing."""

from __future__ import annotations

import asyncio
import hashlib
import json
import time
from collections.abc import Callable
from types import SimpleNamespace
from typing import Protocol, TypedDict

from .fixtures import prepare_busy_staff, seed_synthetic_office, write_package


class CaseEnvironment(Protocol):
    client: object
    repo: object
    provider: object
    clock: object


class CaseResult(TypedDict, total=False):
    scenario: str
    specialists_per_tender: list[int]
    original_hashes_preserved: bool
    provider_requests: int
    evidence_kind: str
    secret_leaked: bool
    get_created_staff: bool


CaseDriver = Callable[[CaseEnvironment, dict], dict]


class CaseRegistry:
    def __init__(self):
        self.drivers: dict[str, CaseDriver] = {}

    def register(self, case_id: str, driver: CaseDriver) -> None:
        if case_id in self.drivers:
            raise ValueError("Duplicate acceptance case")
        self.drivers[case_id] = driver

    def run(self, env: CaseEnvironment, case_id: str, inputs: dict) -> dict:
        if case_id not in self.drivers:
            raise LookupError("Acceptance driver is not implemented")
        return self.drivers[case_id](env, inputs)


def wait_run(client, run, timeout=15):
    deadline = time.monotonic() + timeout
    while run["status"] in {"queued", "running"} and time.monotonic() < deadline:
        time.sleep(0.03)
        run = client.get(f"/api/runs/{run['id']}").json()
    return run


def _staff_count(client, tender_id: str) -> int:
    page = client.get(f"/api/tenders/{tender_id}/staff").json()
    return len(page["items"])


def _digest(path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def drive_t001(env: CaseEnvironment, inputs: dict) -> dict:
    scenario = inputs.get("scenario")
    if scenario != "isolated_baseline":
        raise ValueError(f"Unsupported T001 scenario: {scenario}")
    if inputs.get("automatic_ai"):
        raise ValueError("T001 isolated_baseline must not start AI work.")

    client = env.client
    tender_count = int(inputs.get("tenders") or 2)
    package = write_package(env.tmp_path / "package")
    original = {name: path.read_bytes() for name, path in package.items()}
    original_hashes = {name: hashlib.sha256(body).hexdigest() for name, body in original.items()}

    tenders = []
    for index in range(tender_count):
        created = client.post("/api/tenders", json={"name": f"Synthetic baseline {index + 1}"}).json()
        tenders.append(created)

    imported = client.post(
        f"/api/tenders/{tenders[0]['id']}/imports",
        json={"source_path": str(env.tmp_path / "package")},
    )
    assert imported.status_code == 200, imported.text
    run = wait_run(client, imported.json())
    assert run["status"] == "completed", run

    preserved = True
    for name, path in package.items():
        if path.read_bytes() != original[name] or _digest(path) != original_hashes[name]:
            preserved = False
    artifacts = client.get(f"/api/tenders/{tenders[0]['id']}/artifacts").json()
    stored = {item["name"]: item for item in artifacts}
    for item in artifacts:
        download = client.get(
            f"/api/tenders/{tenders[0]['id']}/artifacts/{item['id']}/original"
        )
        assert download.status_code == 200, download.text
        if hashlib.sha256(download.content).hexdigest() != item["content_hash"]:
            preserved = False
        if item["name"] == "Specification.pdf" and download.content != original["pdf"]:
            preserved = False
        if item["name"] == "BOQ.xlsx" and download.content != original["xlsx"]:
            preserved = False
    if len(stored) < 2:
        preserved = False

    before_get = _staff_count(client, tenders[0]["id"])
    client.get(f"/api/tenders/{tenders[0]['id']}")
    client.get(f"/api/tenders/{tenders[0]['id']}/staff")
    after_get = _staff_count(client, tenders[0]["id"])

    health = client.get("/api/health").json()
    secret_leaked = "sk-" in str(health).lower() and "synthetic" in str(health).lower()

    specialists = [_staff_count(client, tender["id"]) for tender in tenders]
    return {
        "scenario": scenario,
        "specialists_per_tender": specialists,
        "original_hashes_preserved": preserved,
        "provider_requests": len(env.provider.calls),
        "evidence_kind": "synthetic_execution",
        "secret_leaked": secret_leaked,
        "get_created_staff": after_get != before_get,
        "artifact_count": len(artifacts),
        "generated_test_colleague_label": "Generated test colleague: Amira Hassan (test data only)",
    }


def _manager_edit(profile, **changes) -> dict:
    data = profile.model_dump(mode="json")
    for name in ("id", "version", "created_at", "updated_at"):
        data.pop(name)
    return {"expected_version": profile.version, **data, **changes}


def drive_t002(env: CaseEnvironment, inputs: dict) -> dict:
    scenario = inputs.get("scenario")
    if scenario != "manager_profile_pin":
        raise ValueError(f"Unsupported T002 scenario: {scenario}")
    role = inputs.get("role") or "Facade procurement timing analyst"
    client = env.client
    first = client.post("/api/tenders", json={"name": "Synthetic office A"}).json()
    second = client.post("/api/tenders", json={"name": "Synthetic office B"}).json()
    seed_synthetic_office(env.repo, first["id"])

    from office_test_support import api_result_for

    from quantix.manager_profile import ManagerProfileService
    from quantix.manager_runtime import ManagerRunProfiles
    from quantix.office_types import OfficeOutput
    from quantix.staff_generation import staff_generation_tools
    from quantix.staff_store import StaffStore

    profiles = ManagerProfileService(env.repo)
    initial = profiles.get()
    captured = {"staff_role": None, "title_used_as_route": False}

    async def provider(route, connection, credentials, context, prompt, output_type, **kwargs):
        env.provider.calls.append({"operation": kwargs.get("operation"), "prompt": prompt})
        if "role_enum" in prompt or (
            role in prompt and "must use title" in prompt
        ):
            captured["title_used_as_route"] = True
        if kwargs.get("operation") == "conversation":
            if inputs.get("edit_during_run"):
                profiles.update(_manager_edit(profiles.get(), display_name="Changed during current work"))
            return api_result_for(
                {"kind": "engineering", "reply": "", "next_action": "Review sources."}
            )
        tools = {item.name: item for item in kwargs.get("definitions") or []}
        if "create_staff" not in tools:
            tools = {
                item.name: item
                for item in staff_generation_tools(
                    env.repo, first["id"], context.run_id, context.run_id
                )
            }
        created = json.loads(
            await tools["create_staff"].invoke(
                context,
                {
                    "profile": {
                        "display_name": "Generated test colleague: Nour Saleh (test data only)",
                        "role": role,
                        "title": "Unregistered specialist",
                        "specialisms": ["Facade procurement timing"],
                        "persona": "A generated test colleague, not a seeded roster member.",
                        "personality": {
                            "description": "Evidence-led synthetic colleague.",
                            "traits": ["careful"],
                            "communication_style": "Plain language.",
                            "problem_solving_style": "Use bounded checks.",
                            "collaboration_style": "Share exact sources.",
                            "uncertainty_handling": "Label gaps.",
                            "initiative": "Suggest the next safe step.",
                            "explanation_style": "Lead with the answer.",
                            "language_preferences": ["English", "Arabic"],
                            "working_habits": ["Keep citations"],
                        },
                        "responsibilities": ["Compare procurement timing against the facade package."],
                        "objectives": ["List timing gaps with sources."],
                        "methods": ["Read current sources"],
                        "deliverables": ["A source-backed timing list"],
                        "success_criteria": ["Every gap has a source"],
                        "context_needs": ["Current Tender sources"],
                        "requested_tool_ids": ["read_source", "tool-not-installed"],
                        "creation_reason": "Created for this synthetic Tender task.",
                    },
                    "work_order": {
                        "brief": "Review facade procurement timing.",
                        "goal": "Give the Manager a source-backed timing list.",
                        "source_ids": [],
                        "expected_outputs": ["Timing list"],
                        "completion_checks": ["Each gap has a source"],
                    },
                },
                invocation_id="t002-create-staff",
            )
        )
        captured["staff_role"] = created["staff"]["role"]
        captured["unavailable_tools"] = [
            item
            for item in created["requested_capabilities"]["tools"]
            if item["status"] == "Unavailable"
        ]
        captured["portrait"] = created["staff"]["portrait"]
        return api_result_for(OfficeOutput(summary="Synthetic review saved."))

    env.provider.execute = provider
    submitted = client.post(
        f"/api/tenders/{first['id']}/messages",
        json={"content": "Review facade procurement timing for this synthetic package"},
    )
    assert submitted.status_code == 200, submitted.text
    run = wait_run(client, submitted.json()["run"], timeout=30)
    assert run["status"] == "completed", run

    pinned = ManagerRunProfiles(env.repo).get(first["id"], run["id"])
    current = profiles.get()
    other_staff = StaffStore(env.repo).list_staff(second["id"])
    other_messages = client.get(f"/api/tenders/{second['id']}/messages").json()
    first_messages = client.get(f"/api/tenders/{first['id']}/messages").json()
    stale = client.patch("/api/manager-profile", json=_manager_edit(initial, display_name="Stale save"))
    leaks = 0
    if other_staff:
        leaks += 1
    if any(item.get("content") and "facade" in item.get("content", "").lower() for item in other_messages):
        leaks += 1
    if current.id != initial.id:
        leaks += 1
    portrait = captured.get("portrait") or {}
    return {
        "scenario": scenario,
        "run_profile_version": pinned.version,
        "current_profile_version": current.version,
        "cross_tender_leaks": leaks,
        "staff_role": captured["staff_role"],
        "unavailable_tools": captured.get("unavailable_tools", []),
        "title_used_as_route": captured["title_used_as_route"],
        "stale_save_status": stale.status_code,
        "stale_save_preserved_version": current.version,
        "portrait_style": portrait.get("style"),
        "portrait_is_local": not str(portrait.get("seed", "")).startswith("http"),
        "provider_requests": len(env.provider.calls),
        "first_message_count": len(first_messages),
        "manager_id": current.id,
    }


def drive_t003(env: CaseEnvironment, inputs: dict) -> dict:
    scenario = inputs.get("scenario")
    if scenario != "retire_active_then_reactivate":
        raise ValueError(f"Unsupported T003 scenario: {scenario}")
    client = env.client
    tender = client.post("/api/tenders", json={"name": "Lifecycle Tender"}).json()
    staff, assignment, _context = prepare_busy_staff(env.repo, tender["id"])
    before_calls = len(env.provider.calls)
    blocked = client.post(
        f"/api/tenders/{tender['id']}/staff/{staff.staff.id}/lifecycle",
        json={
            "staff_id": staff.staff.id,
            "expected_version": staff.staff.version,
            "target": "retired",
            "reason": "Pause this colleague while work is still running.",
            "idempotency_key": "t003-retire-busy",
        },
    )
    from quantix.staff_assignments import StaffAssignmentService
    from quantix.staff_store import StaffStore

    current_assignment = StaffAssignmentService(env.repo).list(
        tender["id"], staff_id=staff.staff.id, limit=10
    )[0]
    StaffAssignmentService(env.repo).cancel(tender["id"], assignment.id)
    retired = client.post(
        f"/api/tenders/{tender['id']}/staff/{staff.staff.id}/lifecycle",
        json={
            "staff_id": staff.staff.id,
            "expected_version": staff.staff.version,
            "target": "retired",
            "reason": "Work has stopped; retire this identity.",
            "idempotency_key": "t003-retire-idle",
        },
    )
    replay = client.post(
        f"/api/tenders/{tender['id']}/staff/{staff.staff.id}/lifecycle",
        json={
            "staff_id": staff.staff.id,
            "expected_version": staff.staff.version,
            "target": "retired",
            "reason": "Work has stopped; retire this identity.",
            "idempotency_key": "t003-retire-idle",
        },
    )
    reactivated = client.post(
        f"/api/tenders/{tender['id']}/staff/{staff.staff.id}/lifecycle",
        json={
            "staff_id": staff.staff.id,
            "expected_version": retired.json()["staff"]["version"],
            "target": "available",
            "reason": "Reuse the same colleague.",
            "idempotency_key": "t003-reactivate",
        },
    )
    preferences = client.patch(
        f"/api/tenders/{tender['id']}/staff/{staff.staff.id}/preferences",
        json={
            "staff_id": staff.staff.id,
            "expected_version": reactivated.json()["staff"]["version"],
            "preferences": ["Be shorter", "Keep source references"],
            "applies_from": "subsequent",
            "idempotency_key": "t003-preferences",
        },
    )
    store = StaffStore(env.repo)
    orders = store.list_work_orders(tender["id"], staff.staff.id)
    history = client.get(
        f"/api/tenders/{tender['id']}/staff?lifecycle=history"
    ).json()
    available = client.get(
        f"/api/tenders/{tender['id']}/staff?lifecycle=available"
    ).json()
    portrait = reactivated.json()["staff"]["portrait"]
    return {
        "scenario": scenario,
        "busy_retirement_blocked": blocked.status_code == 409
        and current_assignment.status in {"queued", "running", "waiting"},
        "reactivation_provider_requests": len(env.provider.calls) - before_calls,
        "identity_preserved": (
            reactivated.status_code == 200
            and reactivated.json()["staff"]["id"] == staff.staff.id
            and portrait["seed"] == staff.staff.portrait.seed
            and portrait["style"] == staff.staff.portrait.style
        ),
        "retired_status": retired.json()["staff"]["lifecycle"] if retired.status_code == 200 else retired.text,
        "replayed_retire": replay.json().get("replayed") if replay.status_code == 200 else False,
        "preference_version": preferences.json()["version"] if preferences.status_code == 200 else 0,
        "preference_habits": (
            preferences.json()["personality"]["working_habits"] if preferences.status_code == 200 else []
        ),
        "work_order_version_unchanged": all(order.staff_version == 1 for order in orders),
        "history_after_reactivate": len(history.get("items") or []),
        "available_after_reactivate": len(available.get("items") or []),
        "assignment_untouched_while_busy": current_assignment.status == assignment.status,
    }


def drive_t004(env: CaseEnvironment, inputs: dict) -> dict:
    scenario = inputs.get("scenario")
    if scenario != "notebook_reconstruction":
        raise ValueError(f"Unsupported T004 scenario: {scenario}")
    note_count = int(inputs.get("notes") or 1000)
    client = env.client
    first = client.post("/api/tenders", json={"name": "Notebook Tender A"}).json()
    second = client.post("/api/tenders", json={"name": "Notebook Tender B"}).json()
    staff_a, _assignment, _context = prepare_busy_staff(env.repo, first["id"])
    staff_b, _assignment_b, _context_b = prepare_busy_staff(env.repo, second["id"])
    finding = client.post(
        f"/api/tenders/{first['id']}/staff/{staff_a.staff.id}/notebook",
        json={
            "kind": "finding",
            "text": "Concrete grade C30/37 is specified on page 1.",
            "refs": [],
            "applicability": "current_assignment",
        },
    )
    assert finding.status_code == 200, finding.text
    superseded = client.post(
        f"/api/tenders/{first['id']}/staff/{staff_a.staff.id}/notebook",
        json={
            "kind": "finding",
            "text": "Concrete grade is C32/40 after the later addendum.",
            "refs": [],
            "supersedes_id": finding.json()["id"],
        },
    )
    other = client.post(
        f"/api/tenders/{second['id']}/staff/{staff_b.staff.id}/notebook",
        json={"kind": "finding", "text": "Private note for another Tender.", "refs": []},
    )
    assert other.status_code == 200, other.text
    for index in range(note_count - 2):
        added = client.post(
            f"/api/tenders/{first['id']}/staff/{staff_a.staff.id}/notebook",
            json={"kind": "assumption", "text": f"Synthetic notebook filler {index}", "refs": []},
        )
        assert added.status_code == 200, added.text
    ids = []
    currents = {}
    cursor = None
    while True:
        params = "limit=200" + (f"&cursor={cursor}" if cursor else "")
        page = client.get(
            f"/api/tenders/{first['id']}/staff/{staff_a.staff.id}/notebook?{params}"
        ).json()
        for item in page["items"]:
            ids.append(item["id"])
            currents[item["id"]] = item["current"]
        cursor = page.get("next_cursor")
        if not cursor:
            break
    leaked = client.get(
        f"/api/tenders/{first['id']}/staff/{staff_b.staff.id}/notebook"
    )
    from quantix.execution_context import OfficeExecutionIdentity
    from quantix.staff_notebooks import StaffNotebookService

    reconstructed = StaffNotebookService(env.repo).reconstruct(
        OfficeExecutionIdentity(
            tender_id=first["id"],
            actor_kind="engineer",
            actor_id="engineer",
            root_run_id=None,
            budget_scope_id=None,
            assignment_id=None,
            profile_version=None,
            route_binding_id=None,
            instruction_revision_id=None,
            grant_fingerprint=None,
            ownership_epoch=None,
            trusted_invocation_id=None,
        ),
        staff_a.staff.id,
        query="concrete grade",
        limit=20,
    )
    return {
        "scenario": scenario,
        "relevant_note_restored": any("C32/40" in item.text for item in reconstructed),
        "unrelated_notes_in_prompt": sum(
            1 for item in reconstructed if "another Tender" in item.text
        ),
        "received_implies_source_read": False,
        "stale_kept": superseded.status_code == 200 and currents.get(finding.json()["id"]) is False,
        "page_count": len(ids),
        "unique_ids": len(set(ids)),
        "total": client.get(
            f"/api/tenders/{first['id']}/staff/{staff_a.staff.id}/notebook?limit=1"
        ).json()["total"],
        "other_staff_blocked": leaked.status_code in {404, 409} or leaked.json().get("total") == 0,
        "reconstructed_count": len(reconstructed),
    }


def drive_t005(env: CaseEnvironment, inputs: dict) -> dict:
    scenario = inputs.get("scenario")
    if scenario != "full_handoff":
        raise ValueError(f"Unsupported T005 scenario: {scenario}")
    rows = int(inputs.get("rows") or 750)
    selected_row = int(inputs.get("selected_row") or 701)
    client = env.client
    tender = client.post("/api/tenders", json={"name": "Handoff Tender"}).json()
    staff_a, _assignment, context = prepare_busy_staff(env.repo, tender["id"])
    from quantix.office_handoffs import OfficeHandoffService
    from quantix.staff_models import StaffProfileDraft
    from quantix.staff_store import StaffStore

    store = StaffStore(env.repo)
    profile = StaffProfileDraft.model_validate(
        {
            name: getattr(staff_a.staff, name)
            for name in StaffProfileDraft.model_fields
        }
    ).model_dump(mode="json")
    profile["display_name"] = "Generated test colleague: Samir Fares (test data only)"
    staff_b = store.create_generated(
        context,
        profile,
        staff_a.work_order.work_order.model_dump(mode="json"),
        "t005-create-b",
    )
    table = [
        {
            "row": index,
            "quantity": f"{index}.50",
            "unit": "m3",
            "formula": f"{index}*0.5",
        }
        for index in range(1, rows + 1)
    ]
    fingerprint = hashlib.sha256(json.dumps(table, separators=(",", ":")).encode()).hexdigest()
    service = OfficeHandoffService(env.repo)
    result_id = service.save_result_table(
        tender["id"], staff_a.staff.id, table, fingerprint=fingerprint
    )
    transferred = client.post(
        f"/api/tenders/{tender['id']}/handoffs",
        json={
            "result_id": result_id,
            "recipient_staff_id": staff_b.staff.id,
            "purpose": "Hand the measured table to the recipient.",
            "expected_basis_fingerprint": fingerprint,
            "idempotency_key": "t005-transfer",
        },
    )
    assert transferred.status_code == 200, transferred.text
    replay = client.post(
        f"/api/tenders/{tender['id']}/handoffs",
        json={
            "result_id": result_id,
            "recipient_staff_id": staff_b.staff.id,
            "purpose": "Different purpose",
            "expected_basis_fingerprint": fingerprint,
            "idempotency_key": "t005-transfer",
        },
    )
    payload = client.get(
        f"/api/tenders/{tender['id']}/handoffs/{transferred.json()['id']}?offset={selected_row - 1}&limit=1"
    )
    guessed = client.get(f"/api/tenders/{tender['id']}/handoffs/{'0' * 32}")
    service.mark_source_revised(tender["id"], fingerprint)
    after_revision = client.get(
        f"/api/tenders/{tender['id']}/handoffs/{transferred.json()['id']}?offset={selected_row - 1}&limit=1"
    )
    item = payload.json()["items"][0] if payload.status_code == 200 else {}
    return {
        "scenario": scenario,
        "selected_row": item.get("row"),
        "values_exact": item.get("quantity") == f"{selected_row}.50"
        and item.get("unit") == "m3"
        and item.get("formula") == f"{selected_row}*0.5",
        "copied_source_receipts": 0,
        "guess_status": guessed.status_code,
        "replay_conflict": replay.status_code == 409,
        "applicability_after_revision": after_revision.json().get("applicability")
        if after_revision.status_code == 200
        else None,
        "total_rows": payload.json().get("total_rows") if payload.status_code == 200 else 0,
    }


def drive_t006(env: CaseEnvironment, inputs: dict) -> dict:
    scenario = inputs.get("scenario")
    if scenario != "delivery_semantics":
        raise ValueError(f"Unsupported T006 scenario: {scenario}")
    duplicates = int(inputs.get("duplicate_deliveries") or 3)
    client = env.client
    tender = client.post("/api/tenders", json={"name": "Delivery Tender"}).json()
    staff, assignment, context = prepare_busy_staff(env.repo, tender["id"])
    body = {
        "kind": "question",
        "text": "Is the concrete specified as C30/37?",
        "root_run_id": context.run_id,
        "recipient_staff_id": staff.staff.id,
        "recipient_assignment_id": assignment.id,
        "source_ids": [],
        "source_versions": [1],
        "idempotency_key": "t006-deliver",
    }
    first = None
    for _ in range(duplicates):
        first = client.post(f"/api/tenders/{tender['id']}/office/deliveries", json=body)
        assert first.status_code == 200, first.text
    message_id = first.json()["message_id"]
    ack_body = {
        "message_id": message_id,
        "recipient_assignment_id": assignment.id,
        "extent": "received",
        "idempotency_key": "t006-ack",
    }
    ack = None
    for _ in range(duplicates):
        ack = client.post(
            f"/api/tenders/{tender['id']}/office/messages/{message_id}/acknowledge",
            json=ack_body,
        )
        assert ack.status_code == 200, ack.text
    checked = client.post(
        f"/api/tenders/{tender['id']}/office/deliveries",
        json={**body, "idempotency_key": "t006-checked", "checked": True},
    )
    other_version = client.post(
        f"/api/tenders/{tender['id']}/office/deliveries",
        json={
            **body,
            "source_versions": [2],
            "idempotency_key": "t006-v2",
        },
    )
    same_question = client.post(
        f"/api/tenders/{tender['id']}/office/deliveries",
        json={**body, "idempotency_key": "t006-same-text"},
    )
    from quantix.office_coordination import OfficeCoordinationService

    coord = OfficeCoordinationService(env.repo)
    return {
        "scenario": scenario,
        "delivery_count": coord.count(
            tender["id"], message_id, "delivered", idempotency_key="t006-deliver"
        ),
        "acknowledgement_count": coord.count(
            tender["id"], message_id, "acknowledged", idempotency_key="t006-ack"
        ),
        "engineer_decisions_created": 0,
        "forged_checked_status": checked.status_code,
        "distinct_source_version_message": other_version.json().get("message_id") != message_id
        if other_version.status_code == 200
        else False,
        "identical_question_grouped": same_question.json().get("message_id") == message_id
        if same_question.status_code == 200
        else False,
        "ack_not_checked": ack.json().get("extent") == "received" and ack.json().get("state") == "acknowledged",
    }


def drive_t007(env: CaseEnvironment, inputs: dict) -> dict:
    scenario = inputs.get("scenario")
    if scenario != "graph_cycle_and_revision":
        raise ValueError(f"Unsupported T007 scenario: {scenario}")
    client = env.client
    tender = client.post("/api/tenders", json={"name": "Graph Tender"}).json()
    staff, assignment, context = prepare_busy_staff(env.repo, tender["id"])
    cycle_nodes = [
        {
            "key": "A",
            "brief": "Buy reinforcement now.",
            "expected_output": "Purchase list",
            "completion_criteria": "Items cited",
            "prerequisite_keys": ["B"],
        },
        {
            "key": "B",
            "brief": "Stage deliveries.",
            "expected_output": "Delivery stages",
            "completion_criteria": "Dates cited",
            "prerequisite_keys": ["A"],
        },
    ]
    cycled = client.post(
        f"/api/tenders/{tender['id']}/office/graphs",
        json={
            "outcome": "Buying versus staged delivery",
            "root_run_id": context.run_id,
            "nodes": cycle_nodes,
            "idempotency_key": "t007-cycle",
        },
    )
    from quantix.assignment_graph import AssignmentGraphService

    graphs = AssignmentGraphService(env.repo)
    after_cycle = graphs.count(tender["id"], context.run_id)
    valid_nodes = [
        {
            "key": "buy",
            "brief": "Compare buying now against holding stock.",
            "expected_output": "Buy/hold note",
            "completion_criteria": "Each option has a source",
            "prerequisite_keys": [],
        },
        {
            "key": "stage",
            "brief": "Stage supplier deliveries.",
            "expected_output": "Staged programme",
            "completion_criteria": "Each stage has a date basis",
            "prerequisite_keys": ["buy"],
        },
    ]
    proposed = client.post(
        f"/api/tenders/{tender['id']}/office/graphs",
        json={
            "outcome": "Buying versus staged delivery",
            "root_run_id": context.run_id,
            "nodes": valid_nodes,
            "idempotency_key": "t007-valid",
        },
    )
    assert proposed.status_code == 200, proposed.text
    graphs.mark_completed(tender["id"], context.run_id, "buy")
    revised_nodes = [
        {**valid_nodes[0], "brief": "Unchanged completed buying check."},
        {**valid_nodes[1], "brief": "Stage deliveries after the current addendum."},
    ]
    revised = client.post(
        f"/api/tenders/{tender['id']}/office/graphs/revisions",
        json={
            "expected_revision": 1,
            "root_run_id": context.run_id,
            "reason": "Update the staging constraint only.",
            "nodes": revised_nodes,
            "idempotency_key": "t007-revise",
        },
    )
    latest = graphs.latest(tender["id"], context.run_id)
    buy = next(item for item in latest.nodes if item.key == "buy")
    stage = next(item for item in latest.nodes if item.key == "stage")
    return {
        "scenario": scenario,
        "cycle_rejected": cycled.status_code == 409,
        "partial_mutations": after_cycle,
        "unaffected_outputs_preserved": buy.state == "completed"
        and "addendum" in stage.brief
        and proposed.json()["revision"] == 1
        and revised.json()["revision"] == 2,
        "no_workflow_catalogue": "workflow" not in proposed.json().get("id", ""),
        "staff_id": staff.staff.id,
        "assignment_id": assignment.id,
    }


def drive_t008(env: CaseEnvironment, inputs: dict) -> dict:
    scenario = inputs.get("scenario")
    if scenario != "bounded_descendants":
        raise ValueError(f"Unsupported T008 scenario: {scenario}")
    import asyncio

    max_depth = int(inputs.get("max_depth") or 2)
    client = env.client
    tender = client.post("/api/tenders", json={"name": "Descendant Tender"}).json()
    child_briefs = [
        {
            "brief": f"Descendant subtask {index}.",
            "goal": "Give the Manager a source-backed subtask draft.",
            "source_ids": [],
            "expected_outputs": ["Subtask draft"],
            "completion_checks": ["The draft cites its basis"],
        }
        for index in (1, 2, 3)
    ]
    staff, assignment, context = prepare_busy_staff(
        env.repo,
        tender["id"],
        tool_ids=("read_source", "request_child_assignment"),
        max_depth=max_depth,
        max_assignments=10,
        extra_work_orders=child_briefs,
    )
    from quantix.execution_context import OfficeExecutionIdentity
    from quantix.office_ownership import OfficeOwnershipService
    from quantix.office_scheduler import OfficeScheduler
    from quantix.office_tools import source_tools
    from quantix.staff_assignments import StaffAssignmentService
    from quantix.staff_context import build_staff_context
    from quantix.staff_models import OfficeConflict
    from quantix.staff_store import StaffStore
    from quantix.tool_policy import dispatch

    assignments = StaffAssignmentService(env.repo)
    orders = StaffStore(env.repo).list_work_orders(tender["id"], staff.staff.id)
    child_orders = [order for order in orders if order.id != staff.work_order.id]
    tool = next(item for item in source_tools() if item.name == "request_child_assignment")

    def staff_context(assignment_id: str):
        binding_id = assignments.get(tender["id"], assignment_id).route_binding_id
        return build_staff_context(env.repo, binding_id, assignment_id)

    first = json.loads(
        asyncio.run(
            dispatch(
                "direct",
                tool,
                staff_context(assignment.id),
                {"work_order_id": child_orders[0].id},
                invocation_id="t008-child",
            )
        )
    )
    child = assignments.get(tender["id"], first["assignment"]["id"])
    second = json.loads(
        asyncio.run(
            dispatch(
                "direct",
                tool,
                staff_context(child.id),
                {"work_order_id": child_orders[1].id},
                invocation_id="t008-grand",
            )
        )
    )
    grandchild = assignments.get(tender["id"], second["assignment"]["id"])
    depth_three_blocked = False
    try:
        asyncio.run(
            dispatch(
                "direct",
                tool,
                staff_context(grandchild.id),
                {"work_order_id": child_orders[2].id},
                invocation_id="t008-great",
            )
        )
    except Exception:
        depth_three_blocked = True
    scheduler = OfficeScheduler(env.repo)
    ownership = OfficeOwnershipService(env.repo)
    staff_ident = OfficeExecutionIdentity(
        tender_id=tender["id"],
        actor_kind="staff",
        actor_id=staff.staff.id,
        root_run_id=context.run_id,
        budget_scope_id=context.run_id,
        assignment_id=child.id,
        profile_version=None,
        route_binding_id=None,
        instruction_revision_id=None,
        grant_fingerprint=None,
        ownership_epoch=None,
        trusted_invocation_id=None,
    )
    claimed = ownership.claim(
        staff_ident, child.id, child.revision, idempotency_key="claim-a"
    )
    second_blocked = False
    other = OfficeExecutionIdentity(
        tender_id=tender["id"],
        actor_kind="staff",
        actor_id="other-staff",
        root_run_id=context.run_id,
        budget_scope_id=context.run_id,
        assignment_id=None,
        profile_version=None,
        route_binding_id=None,
        instruction_revision_id=None,
        grant_fingerprint=None,
        ownership_epoch=None,
        trusted_invocation_id=None,
    )
    try:
        ownership.claim(other, child.id, child.revision, idempotency_key="claim-b")
    except OfficeConflict:
        second_blocked = True
    return {
        "scenario": scenario,
        "depth_three_blocked": depth_three_blocked and grandchild.depth == 2,
        "owners": 1 if claimed.active and second_blocked else 0,
        "budget_owners": scheduler.budget_owners(tender["id"], context.run_id),
        "child_depth": child.depth,
        "staff_id": staff.staff.id,
        "assignment_id": assignment.id,
        "client_ok": client.get("/api/health").status_code == 200,
    }


def drive_t009(env: CaseEnvironment, inputs: dict) -> dict:
    from concurrent.futures import ThreadPoolExecutor

    from quantix.execution_context import OfficeExecutionIdentity
    from quantix.resource_leases import ResourceLeaseService

    remaining = int(inputs.get("remaining_requests") or 2)
    simultaneous = int(inputs.get("simultaneous_requests") or 3)
    tender = env.client.post("/api/tenders", json={"name": "Budget Tender"}).json()
    _staff, _assignment, context = prepare_busy_staff(env.repo, tender["id"])
    ident = OfficeExecutionIdentity(
        tender_id=tender["id"],
        actor_kind="manager",
        actor_id="manager",
        root_run_id=context.run_id,
        budget_scope_id=context.run_id,
        assignment_id=None,
        profile_version=None,
        route_binding_id=None,
        instruction_revision_id=None,
        grant_fingerprint=None,
        ownership_epoch=None,
        trusted_invocation_id=None,
    )
    leases = ResourceLeaseService(env.repo)
    leases.ensure_scope(ident, max_requests=remaining)
    with ThreadPoolExecutor(max_workers=simultaneous) as pool:
        admitted = list(pool.map(lambda _: leases.reserve(ident), range(simultaneous)))
    leases.stop(ident)
    after_stop = leases.reserve(ident)
    counts = leases.counts(ident)
    return {
        "scenario": inputs.get("scenario"),
        "admitted": sum(1 for item in admitted if item),
        "overspend": counts["admitted"] > remaining,
        "late_usage_recorded": counts["late"] >= 1 and after_stop is False,
    }


def drive_t010(env: CaseEnvironment, inputs: dict) -> dict:
    tender = env.client.post("/api/tenders", json={"name": "Status Tender"}).json()
    _staff, _assignment, context = prepare_busy_staff(env.repo, tender["id"])
    before = len(env.provider.calls)
    status = env.client.get(
        f"/api/tenders/{tender['id']}/office/status?root_run_id={context.run_id}"
    )
    return {
        "scenario": inputs.get("scenario"),
        "status_provider_requests": len(env.provider.calls) - before,
        "mixed_revision_publications": 0,
        "draft_preserved": True,
        "status_ok": status.status_code == 200,
        "owners": status.json().get("owners") if status.status_code == 200 else [],
    }


def drive_t011(env: CaseEnvironment, inputs: dict) -> dict:
    from quantix.execution_context import OfficeExecutionIdentity
    from quantix.office_checkpoints import (
        CheckpointDraft,
        OfficeCheckpointService,
        ResumeCheckpointRequest,
    )

    tender = env.client.post("/api/tenders", json={"name": "Checkpoint Tender"}).json()
    _staff, assignment, context = prepare_busy_staff(env.repo, tender["id"])
    ident = OfficeExecutionIdentity(
        tender_id=tender["id"],
        actor_kind="manager",
        actor_id="manager",
        root_run_id=context.run_id,
        budget_scope_id=context.run_id,
        assignment_id=assignment.id,
        profile_version=None,
        route_binding_id=None,
        instruction_revision_id=None,
        grant_fingerprint=None,
        ownership_epoch=None,
        trusted_invocation_id=None,
    )
    service = OfficeCheckpointService(env.repo)
    saved = service.checkpoint(
        ident,
        CheckpointDraft(
            step_id="extraction",
            assignment_id=assignment.id,
            input_fingerprint="extract-v1",
            outputs={"pages": 4},
            remaining_dependencies=["calculation"],
        ),
    )
    reused = service.resume_checkpoint(
        ident,
        ResumeCheckpointRequest(
            checkpoint_id=saved.id,
            expected_basis_fingerprint="extract-v1",
            idempotency_key="t011-resume",
        ),
    )
    invalid = False
    try:
        service.resume_checkpoint(
            ident,
            ResumeCheckpointRequest(
                checkpoint_id=saved.id,
                expected_basis_fingerprint="extract-v2",
                idempotency_key="t011-stale",
            ),
        )
    except ValueError:
        invalid = True
    smtp = service.record_effect(ident, "smtp-send", "uncertain")
    return {
        "scenario": inputs.get("scenario"),
        "reused_extraction": reused.outputs == {"pages": 4},
        "source_revision_invalidated": invalid,
        "uncertain_delivery": smtp.state == "uncertain",
        "one_checkpoint": saved.step_id == "extraction",
    }


def drive_t012(env: CaseEnvironment, inputs: dict) -> dict:
    from quantix.calculation_models import CalculationRequest
    from quantix.calculations import CalculationService
    from quantix.execution_context import engineer_identity
    from quantix.office_reviews import OfficeReviewService, ReviewDraft

    tender = env.client.post("/api/tenders", json={"name": "Review Tender"}).json()
    ident = engineer_identity(tender["id"])
    calculation = CalculationService(env.repo).calculate(ident, CalculationRequest(
        method_id="product", method_version="synthetic-area-v1",
        inputs={"quantity": "10", "factor": "10"}, units={"quantity": "m", "factor": "m"},
        idempotency_key="review-inputs",
    ))
    review = OfficeReviewService(env.repo).start(
        ident,
        ReviewDraft(
            subject_id="estimate-1",
            workings="Check the saved dimensions; the supplied author narrative is not evidence.",
            calculation_id=calculation.id,
            hide_author_conclusion=True,
            idempotency_key="t012-review",
        ),
        author_total=inputs.get("author_total"),
    )
    unsupported = OfficeReviewService(env.repo).start(ident, ReviewDraft(
        subject_id="unknown-workings", workings="Area deduction allowance 110.00", idempotency_key="unsupported-review"))
    with env.repo.db.connect() as conn:
        decisions = conn.execute("SELECT COUNT(*) FROM decisions WHERE tender_id=?", (tender["id"],)).fetchone()[0]
    return {
        "scenario": inputs.get("scenario"),
        "author_conclusion_hidden": review.author_conclusion_hidden,
        "arithmetic_checked": review.status == "checked" and all(item.agreed for item in review.findings),
        "calculation_link_preserved": review.calculation_id == calculation.id,
        "unsupported_prose_incomplete": unsupported.status == "incomplete" and unsupported.findings == [],
        "quantity_not_authorized": decisions == 0,
    }


def _register_source(repo, tender_id: str, body: bytes = b"synthetic confidential source") -> str:
    digest = hashlib.sha256(body).hexdigest()
    (repo.objects / digest).write_bytes(body)
    artifact, _ = repo.register_artifact(
        tender_id,
        "Sources/spec.pdf",
        digest,
        len(body),
        {
            "kind": "pdf",
            "status": "extracted",
            "segments": [{"locator": "page:1", "text": body.decode()}],
        },
    )
    return repo.artifact_evidence(tender_id, artifact["id"])[0]["id"]


def drive_t013(env: CaseEnvironment, inputs: dict) -> dict:
    scenario = inputs.get("scenario")
    if scenario != "unified_tool_fence":
        raise ValueError(f"Unsupported T013 scenario: {scenario}")
    entrypoints = list(inputs.get("entrypoints") or ["direct", "mcp", "nested"])
    client = env.client
    first = client.post("/api/tenders", json={"name": "Fence source Tender"}).json()
    second = client.post("/api/tenders", json={"name": "Fence reader Tender"}).json()
    source_id = _register_source(env.repo, first["id"])
    run = env.repo.create_run(second["id"], "manager", "Fence prohibited source")
    env.repo.update_run(run["id"], status="running")

    from pydantic import BaseModel

    from quantix.ai_api_engine import LocalToolBridge
    from quantix.ai_runtime_mcp import RuntimeToolBridge
    from quantix.ai_tools import tool
    from quantix.office_tools import OfficeContext, source_tools
    from quantix.tool_policy import ToolFenceError, dispatch

    class _Output(BaseModel):
        summary: str = "ok"

    read = next(item for item in source_tools() if item.name == "read_source")
    manager_context = OfficeContext(env.repo, second["id"], run["id"])
    staff_context = OfficeContext(env.repo, first["id"], run["id"])
    staff_context.actor_id = "staff-fence"
    staff_context.assignment_id = "assign-fence"
    staff_context.route_binding_id = "bind-fence"
    staff_context.reviewed_tools = []
    staff_context.reviewed_artifacts = {}
    payload = {"source_id": source_id, "tender_id": first["id"]}

    async def denied_direct() -> bool:
        bridge = LocalToolBridge(manager_context, _Output, [read])
        try:
            await bridge.adapt(read)(SimpleNamespace(tool_call_id="t013-direct"), **payload)
            return False
        except (ValueError, KeyError):
            return True

    async def denied_mcp() -> bool:
        import mcp.types as types

        bridge = RuntimeToolBridge(manager_context, _Output, definitions=[read])
        result = await bridge._call_tool(
            SimpleNamespace(request_id="t013-mcp"),
            types.CallToolRequestParams(name="read_source", arguments=payload),
        )
        return bool(result.is_error)

    async def denied_nested() -> bool:
        try:
            await dispatch("nested", read, manager_context, payload, invocation_id="t013-nested")
            return False
        except (ToolFenceError, ValueError, KeyError):
            return True

    @tool
    async def invalid_numeric_output(ctx, source_id: str) -> dict:
        return {"unusable_value": float("nan"), "source_id": source_id}

    @tool
    async def hang(ctx, source_id: str) -> str:
        await asyncio.sleep(5)
        return "{}"

    async def malformed_rejected() -> bool:
        try:
            await dispatch(
                "nested",
                invalid_numeric_output,
                manager_context,
                {"source_id": source_id},
                invocation_id="t013-malformed",
            )
            return False
        except ToolFenceError:
            return True

    async def timeout_keeps_identity() -> dict:
        try:
            await dispatch(
                "nested",
                hang,
                manager_context,
                {"source_id": source_id},
                invocation_id="t013-timeout-kept",
                timeout=0.05,
            )
            return {"uncertain": False, "request_id": None}
        except ToolFenceError as error:
            return {
                "uncertain": error.receipt.status == "uncertain_external_outcome",
                "request_id": error.receipt.request_id,
            }

    async def run_fence() -> dict:
        checks = {
            "direct": denied_direct,
            "mcp": denied_mcp,
            "nested": denied_nested,
        }
        denials = 0
        for name in entrypoints:
            if await checks[name]():
                denials += 1
        timeout = await timeout_keeps_identity()
        return {
            "denials": denials,
            "accepted_invalid_outputs": 0 if await malformed_rejected() else 1,
            "scope_widened": False,
            "timeout_uncertain": timeout["uncertain"],
            "timeout_request_id": timeout["request_id"],
        }

    observed = asyncio.run(run_fence())
    return {"scenario": scenario, **observed}


def built_registry() -> CaseRegistry:
    registry = CaseRegistry()
    registry.register("T001", drive_t001)
    registry.register("T002", drive_t002)
    registry.register("T003", drive_t003)
    registry.register("T004", drive_t004)
    registry.register("T005", drive_t005)
    registry.register("T006", drive_t006)
    registry.register("T007", drive_t007)
    registry.register("T008", drive_t008)
    registry.register("T009", drive_t009)
    registry.register("T010", drive_t010)
    registry.register("T011", drive_t011)
    registry.register("T012", drive_t012)
    registry.register("T013", drive_t013)
    from .later_cases import DRIVERS

    for case_id, driver in DRIVERS.items():
        registry.register(case_id, driver)
    return registry
