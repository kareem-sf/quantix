"""Isolated staff contexts, scope enforcement and attributable source receipts."""

from __future__ import annotations

import asyncio
import hashlib
import json

import pytest
from test_staff_routing import (
    _envelope,
    _profile,
    _ready_binding_workspace,
    _receipt,
    _source,
    _work_order,
)

from quantix.ai_policy import AIPolicyService
from quantix.db import new_id, now
from quantix.manager_profile import ManagerProfileService
from quantix.manager_runtime import ManagerRunProfiles
from quantix.office_tools import source_tools
from quantix.staff_models import ManagerCreationContext
from quantix.staff_routing import StaffRoutingService
from quantix.staff_store import StaffStore


def _build_staff_context(*args, **kwargs):
    from quantix.staff_context import build_staff_context

    return build_staff_context(*args, **kwargs)


def _staff_prompt_context(*args, **kwargs):
    from quantix.staff_context import staff_prompt_context

    return staff_prompt_context(*args, **kwargs)


def _assignment(repo, binding, root_context, *, status="running"):
    from quantix.staff_assignments import StaffAssignmentService

    service = StaffAssignmentService(repo)
    assignment = service.queue(root_context, binding.id, "assignment")
    if status == "running":
        assignment = service.start(binding.tender_id, assignment.id, assignment.revision)
    elif status != "queued":
        raise ValueError("The synthetic assignment helper supports queued/running only.")
    return assignment.id


def _context_workspace(tmp_path, monkeypatch, *, tools=("read_source",)):
    repo, tender, _connection, _model, _route, envelope, routing, staff, root, artifact = _ready_binding_workspace(
        tmp_path,
        monkeypatch,
        requested_tool_ids=list(tools),
        envelope_tools=tools,
    )
    binding = routing.bind(root, staff.staff.id, staff.work_order.id, envelope.route_options[0].id, "context")
    assignment_id = _assignment(repo, binding, root)
    return repo, tender, staff, binding, assignment_id, artifact


def _context_workspace_with_unreviewed_source(tmp_path, monkeypatch, *, tools, requested_tools=None):
    """Create both artifacts before plan review so the extra stays unreviewed."""

    repo, tender, _connections, connection, model, _policy, route = __import__(
        "test_catalog_authority", fromlist=["configured_office"]
    ).configured_office(tmp_path, monkeypatch)
    allowed_artifact, allowed_evidence = _source(repo, tender["id"])
    excluded_body = b"EXCLUDED-UNREVIEWED-CANARY"
    excluded_artifact, _ = repo.register_artifact(
        tender["id"],
        "Private/excluded.pdf",
        hashlib.sha256(excluded_body).hexdigest(),
        len(excluded_body),
        {"kind": "pdf", "status": "extracted", "segments": [{"locator": "page:1", "text": excluded_body.decode()}]},
    )
    excluded_evidence = repo.artifact_evidence(tender["id"], excluded_artifact["id"])[0]
    plan = repo.create_plan(
        tender["id"],
        "Review source package",
        [{"title": "Review", "role": "Any role", "description": "Review", "source_ids": [allowed_evidence["id"]]}],
    )
    repo.approve_plan(tender["id"], plan["id"], "Synthetic plan approval")
    envelope = _envelope(
        route,
        connection,
        model,
        [
            {"artifact_id": allowed_artifact["id"], "version": allowed_artifact["version"], "content_hash": allowed_artifact["content_hash"]},
        ],
        tools=tools,
    )
    # Only the reviewed artifact is in scope; the second current artifact is
    # present in this Tender but was not included in the approval.
    routing = StaffRoutingService(repo)
    policy_revision = AIPolicyService(repo).get(tender["id"])["revision"]
    manager = ManagerProfileService(repo)
    planning = repo.create_run(tender["id"], "conversation", "Create colleague")
    context = ManagerCreationContext(tender["id"], planning["id"], manager.get().version, plan["id"])
    staff = StaffStore(repo).create_generated(
        context,
        _profile(requested_tool_ids=list(tools if requested_tools is None else requested_tools)),
        _work_order(source_ids=[allowed_evidence["id"]]),
        "create",
    )
    root = repo.create_run(tender["id"], "manager", "Run delegated work")
    ManagerRunProfiles(repo).capture(tender["id"], root["id"])
    _receipt(
        repo,
        tender,
        plan,
        envelope,
        work_intents=[
            {"run_id": root["id"], "tender_id": tender["id"], "plan_id": plan["id"], "kind": "manager", "task_id": None}
        ],
    )
    routing.save_reviewed_grant(tender["id"], plan["id"], "f" * 64, policy_revision, envelope)
    root_context = ManagerCreationContext(tender["id"], root["id"], manager.get().version, plan["id"])
    binding = routing.bind(root_context, staff.staff.id, staff.work_order.id, envelope.route_options[0].id, "bind")
    assignment_id = _assignment(repo, binding, root_context)
    return repo, tender, staff, binding, assignment_id, allowed_artifact, excluded_artifact, excluded_evidence


def test_build_context_loads_server_assignment_and_starts_with_fresh_mutable_state(tmp_path, monkeypatch):
    repo, tender, staff, binding, assignment_id, artifact = _context_workspace(tmp_path, monkeypatch)

    context = _build_staff_context(repo, binding.id, assignment_id)

    assert context.tender_id == tender["id"]
    assert context.run_id == binding.root_run_id
    assert context.actor_id == staff.staff.id
    assert context.staff_version == staff.staff.version
    assert context.assignment_id == assignment_id
    assert context.route_binding_id == binding.id
    assert context.approved_scope == repo.approved_scope(tender["id"], binding.plan_id)
    assert context.staff_scope.artifacts
    assert context.reviewed_artifacts[artifact["id"]].version == artifact["version"]
    assert [item.id for item in context.reviewed_tools] == ["read_source"]
    assert context.seen_sources == set()
    assert context.item_bases == {}
    assert context.trusted_recipients == set()
    assert context.source_recipients == {}
    assert context.notes == []
    assert context.history == []


def test_context_prompt_contains_only_exact_profile_order_scope_and_actor_material(tmp_path, monkeypatch):
    repo, _tender, staff, binding, assignment_id, _artifact = _context_workspace(tmp_path, monkeypatch)
    context = _build_staff_context(repo, binding.id, assignment_id)
    context.notes.append({"text": "Only this actor's authored note"})
    context.history.append({"text": "Only this actor's authored history"})
    context.seen_sources.add("parent-source-must-not-leak")

    prompt = _staff_prompt_context(context)

    assert prompt["profile"]["id"] == staff.staff.id
    assert prompt["profile"]["role"] == staff.staff.role
    assert prompt["work_order"]["id"] == staff.work_order.id
    assert prompt["scope"]["artifacts"]
    assert prompt["scope"]["tools"] == ["read_source"]
    assert prompt["authored_material"] == {
        "notes": [{"text": "Only this actor's authored note"}],
        "history": [{"text": "Only this actor's authored history"}],
    }
    assert "parent-source-must-not-leak" not in json.dumps(prompt)


def test_source_reads_require_exact_artifact_scope_and_write_extent_receipt(tmp_path, monkeypatch):
    repo, tender, _staff, binding, assignment_id, artifact, excluded_artifact, excluded = _context_workspace_with_unreviewed_source(
        tmp_path, monkeypatch, tools=("read_source",)
    )
    context = _build_staff_context(repo, binding.id, assignment_id)
    allowed = repo.artifact_evidence(tender["id"], artifact["id"])[0]

    source = context.source(allowed["id"], offset=2, limit=9)

    assert source["text"] == allowed["text"][2:11]
    with repo.db.connect() as conn:
        receipt = conn.execute(
            "SELECT * FROM staff_source_receipts WHERE assignment_id=?",
            (assignment_id,),
        ).fetchone()
    assert receipt["artifact_id"] == artifact["id"]
    assert receipt["artifact_version"] == artifact["version"]
    assert receipt["content_hash"] == artifact["content_hash"]
    assert receipt["source_id"] == allowed["id"]
    assert receipt["text_offset"] == 2
    assert receipt["text_length"] == 9
    assert receipt["method"] == "source"
    from quantix.staff_context import StaffContextService

    listed = StaffContextService(repo).list_receipts(tender["id"], assignment_id)
    assert listed[0].source_id == allowed["id"]
    assert listed[0].content_hash == artifact["content_hash"]

    with pytest.raises(ValueError, match="reviewed source scope"):
        context.source(excluded["id"])

    with repo.db.connect() as conn:
        assert conn.execute("SELECT COUNT(*) FROM staff_source_receipts").fetchone()[0] == 1


@pytest.mark.asyncio
async def test_scoped_tool_selection_and_direct_read_helpers_use_exact_binding_tools(tmp_path, monkeypatch):
    repo, tender, _staff, binding, assignment_id, artifact = _context_workspace(tmp_path, monkeypatch)
    context = _build_staff_context(repo, binding.id, assignment_id)
    names = {definition.name for definition in source_tools(context)}
    assert names == {"read_source"}

    read_document = next(definition for definition in source_tools() if definition.name == "read_document")
    with pytest.raises(ValueError, match="not granted"):
        await read_document.invoke(context, {"artifact_id": artifact["id"], "offset": 0, "limit": 1})


@pytest.mark.asyncio
async def test_committed_read_membership_is_available_to_a_later_calculation_tool(tmp_path, monkeypatch):
    repo, tender, _staff, binding, assignment_id, artifact = _context_workspace(
        tmp_path,
        monkeypatch,
        tools=("read_source", "calculate_drawing_measurement"),
    )
    context = _build_staff_context(repo, binding.id, assignment_id)
    source_id = repo.artifact_evidence(tender["id"], artifact["id"])[0]["id"]
    read_source = next(item for item in source_tools() if item.name == "read_source")
    await read_source.invoke(context, {"source_id": source_id, "offset": 0, "limit": 10})

    import quantix.office_measurement as measurement_module

    async def fake_calculation(calculation_context, values):
        assert calculation_context.has_seen_source(source_id)
        return {"source_id": source_id, "artifact_id": values["artifact_id"]}

    monkeypatch.setattr(measurement_module, "calculate_agent_measurement", fake_calculation)
    calculate = next(
        item for item in source_tools() if item.name == "calculate_drawing_measurement"
    )
    result = json.loads(
        await calculate.invoke(
            context,
            {
                "artifact_id": artifact["id"],
                "page": 1,
                "mode": "count",
                "points": [[0.5, 0.5]],
                "calibration_points": None,
                "calibration_metres": None,
                "scope_label": "Synthetic later calculation",
                "source_ids": [source_id],
            },
        )
    )
    assert result["source_id"] == source_id


@pytest.mark.asyncio
async def test_failed_multi_source_read_does_not_commit_partial_receipts_or_seen_sources(tmp_path, monkeypatch):
    repo, tender, _staff, binding, assignment_id, artifact = _context_workspace(
        tmp_path, monkeypatch, tools=("read_document",)
    )
    context = _build_staff_context(repo, binding.id, assignment_id)
    first = repo.artifact_evidence(tender["id"], artifact["id"])[0]
    second_id = "synthetic-second-evidence"
    with repo.atomic() as conn:
        conn.execute(
            "INSERT INTO evidence(id,artifact_id,locator,text,page,kind,metadata_json) VALUES(?,?,?,?,?,?,?)",
            (second_id, artifact["id"], "page:2", "second excerpt", 2, "text", "{}"),
        )
    original_source = context.source
    calls = 0

    def fail_on_second(evidence_id, *args, **kwargs):
        nonlocal calls
        calls += 1
        if calls == 2:
            raise RuntimeError("synthetic second read failure")
        return original_source(evidence_id, *args, **kwargs)

    monkeypatch.setattr(context, "source", fail_on_second)
    read_document = next(item for item in source_tools() if item.name == "read_document")
    with pytest.raises(RuntimeError, match="second read failure"):
        await read_document.invoke(context, {"artifact_id": artifact["id"], "offset": 0, "limit": 5})
    assert context.seen_sources == set()
    with repo.db.connect() as conn:
        assert conn.execute("SELECT COUNT(*) FROM staff_source_receipts").fetchone()[0] == 0
    assert first["id"] not in context.seen_sources


@pytest.mark.asyncio
async def test_read_event_failure_rolls_back_staged_receipt_and_context_deltas(tmp_path, monkeypatch):
    repo, tender, _staff, binding, assignment_id, artifact = _context_workspace(tmp_path, monkeypatch)
    context = _build_staff_context(repo, binding.id, assignment_id)
    source_id = repo.artifact_evidence(tender["id"], artifact["id"])[0]["id"]

    def fail_event(*_args, **_kwargs):
        raise RuntimeError("synthetic final event failure")

    monkeypatch.setattr(repo, "event", fail_event)
    read_source = next(item for item in source_tools() if item.name == "read_source")
    with pytest.raises(RuntimeError, match="final event failure"):
        await read_source.invoke(context, {"source_id": source_id, "offset": 0, "limit": 10})
    assert context.seen_sources == set()
    assert context.source_recipients == {}
    with repo.db.connect() as conn:
        assert conn.execute("SELECT COUNT(*) FROM staff_source_receipts").fetchone()[0] == 0


@pytest.mark.asyncio
async def test_overlapping_staff_reads_merge_only_successful_invocation_deltas(tmp_path, monkeypatch):
    repo, tender, _staff, binding, assignment_id, artifact, _excluded_artifact, excluded = _context_workspace_with_unreviewed_source(
        tmp_path, monkeypatch, tools=("read_source",)
    )
    context = _build_staff_context(repo, binding.id, assignment_id)
    allowed_id = repo.artifact_evidence(tender["id"], artifact["id"])[0]["id"]
    read_source = next(item for item in source_tools() if item.name == "read_source")
    results = await asyncio.gather(
        read_source.invoke(context, {"source_id": allowed_id, "offset": 0, "limit": 10}),
        read_source.invoke(context, {"source_id": excluded["id"], "offset": 0, "limit": 10}),
        return_exceptions=True,
    )
    assert isinstance(results[0], str)
    assert isinstance(results[1], ValueError)
    assert context.seen_sources == {allowed_id}
    with repo.db.connect() as conn:
        assert conn.execute("SELECT COUNT(*) FROM staff_source_receipts").fetchone()[0] == 1


def test_direct_read_helpers_recheck_live_assignment_identity_and_fail_closed(tmp_path, monkeypatch):
    repo, tender, _staff, binding, assignment_id, artifact = _context_workspace(
        tmp_path, monkeypatch, tools=("read_source",)
    )
    context = _build_staff_context(repo, binding.id, assignment_id)
    source_id = repo.artifact_evidence(tender["id"], artifact["id"])[0]["id"]
    context.staff_version += 1

    with pytest.raises(ValueError, match="assignment identity"):
        context.source(source_id)
    with repo.db.connect() as conn:
        assert conn.execute("SELECT COUNT(*) FROM staff_source_receipts").fetchone()[0] == 0

    with repo.atomic() as conn:
        conn.execute("DROP TABLE office_staff_results")
        conn.execute("DROP TABLE office_assignment_receipts")
        conn.execute("DROP TABLE office_assignments")
    with pytest.raises(ValueError, match="assignment store is unavailable"):
        context.ensure_evidence_allowed(source_id)


def test_redacted_source_receipt_retains_original_excerpt_length(tmp_path, monkeypatch):
    repo, tender, _staff, binding, assignment_id, artifact = _context_workspace(tmp_path, monkeypatch)
    context = _build_staff_context(repo, binding.id, assignment_id)
    source_id = repo.artifact_evidence(tender["id"], artifact["id"])[0]["id"]
    original = r"C:\private\source.pdf and sk-abcdefgh"
    with repo.atomic() as conn:
        conn.execute("UPDATE evidence SET text=? WHERE id=?", (original, source_id))

    result = context.source(source_id, offset=0, limit=12000)

    assert "[local path]" in result["text"]
    assert "[credential]" in result["text"]
    with repo.db.connect() as conn:
        receipt = conn.execute(
            "SELECT text_offset,text_length FROM staff_source_receipts WHERE assignment_id=?",
            (assignment_id,),
        ).fetchone()
    assert receipt["text_offset"] == 0
    assert receipt["text_length"] == len(original)


def test_redaction_expansion_keeps_the_entire_bounded_original_suffix(tmp_path, monkeypatch):
    repo, tender, _staff, binding, assignment_id, artifact = _context_workspace(tmp_path, monkeypatch)
    context = _build_staff_context(repo, binding.id, assignment_id)
    source_id = repo.artifact_evidence(tender["id"], artifact["id"])[0]["id"]
    original = (r"C:\x " * 2395) + "TAIL-ORIGINAL-SUFFIX"
    with repo.atomic() as conn:
        conn.execute("UPDATE evidence SET text=? WHERE id=?", (original, source_id))

    result = context.source(source_id, offset=0, limit=12000)

    assert result["text"].endswith("TAIL-ORIGINAL-SUFFIX")
    assert result["text_length"] == len(original)
    with repo.db.connect() as conn:
        receipt = conn.execute(
            "SELECT text_length FROM staff_source_receipts WHERE assignment_id=?",
            (assignment_id,),
        ).fetchone()
    assert receipt["text_length"] == len(original)


@pytest.mark.asyncio
async def test_business_read_quote_replies_preserves_the_complete_redacted_source(tmp_path, monkeypatch):
    repo, tender, _staff, binding, assignment_id, artifact = _context_workspace(
        tmp_path, monkeypatch, tools=("read_quote_replies",)
    )
    context = _build_staff_context(repo, binding.id, assignment_id)
    source_id = repo.artifact_evidence(tender["id"], artifact["id"])[0]["id"]
    original = (r"C:\x " * 2395) + "TAIL-ORIGINAL-SUFFIX"
    with repo.atomic() as conn:
        conn.execute("UPDATE evidence SET text=? WHERE id=?", (original, source_id))
    monkeypatch.setattr(
        "quantix.correspondence.QuoteService.replies",
        lambda _service, _tender_id, _quote_id: [
            {
                "id": "reply-expansion",
                "origin": "manual",
                "sender": "supplier@example.test",
                "received_at": now(),
                "date_header": None,
                "date_basis": "engineer_entered",
                "subject": "Synthetic reply",
                "warnings": [],
                "source_ids": [source_id],
            }
        ],
    )

    result = json.loads(
        await next(item for item in source_tools() if item.name == "read_quote_replies").invoke(
            context,
            {"quote_id": "quote-expansion", "offset": 0, "limit": 20},
        )
    )
    source = result["replies"][0]["sources"][0]
    assert source["text"].endswith("TAIL-ORIGINAL-SUFFIX")
    assert source["text_length"] == len(original)
    assert source["next_offset"] is None


@pytest.mark.asyncio
async def test_business_inspect_estimate_preserves_the_complete_redacted_source(tmp_path, monkeypatch):
    repo, tender, _staff, binding, assignment_id, artifact = _context_workspace(
        tmp_path, monkeypatch, tools=("inspect_estimate",)
    )
    context = _build_staff_context(repo, binding.id, assignment_id)
    source_id = repo.artifact_evidence(tender["id"], artifact["id"])[0]["id"]
    original = (r"C:\x " * 2395) + "TAIL-ORIGINAL-SUFFIX"
    with repo.atomic() as conn:
        conn.execute("UPDATE evidence SET text=? WHERE id=?", (original, source_id))
    monkeypatch.setattr(
        "quantix.office_business.EstimateService.view",
        lambda _service, _tender_id: {
            "items": [
                {
                    "id": "item-expansion",
                    "source_id": source_id,
                    "artifact_id": artifact["id"],
                    "description": "Synthetic BOQ item",
                }
            ]
        },
    )
    monkeypatch.setattr(
        "quantix.office_business.EstimateService.rate_basis",
        lambda _service, _tender_id, _item_id: {"fingerprint": "synthetic-basis"},
    )

    result = json.loads(
        await next(item for item in source_tools() if item.name == "inspect_estimate").invoke(
            context, {"offset": 0, "limit": 20}
        )
    )
    source = result["items"][0]["source"]
    assert source["text"].endswith("TAIL-ORIGINAL-SUFFIX")
    assert source["text_length"] == len(original)
    assert source["next_offset"] is None


def test_context_consumes_binding_tool_subset_without_widening_to_envelope(tmp_path, monkeypatch):
    repo, tender, _staff, binding, assignment_id, _artifact, _excluded, _excluded_evidence = _context_workspace_with_unreviewed_source(
        tmp_path,
        monkeypatch,
        tools=("read_source", "list_documents"),
        requested_tools=("read_source",),
    )
    context = _build_staff_context(repo, binding.id, assignment_id)

    assert [item.id for item in binding.tools] == ["read_source"]
    assert {definition.name for definition in source_tools(context)} == {"read_source"}


def test_empty_binding_tool_subset_exposes_no_source_tools(tmp_path, monkeypatch):
    repo, _tender, _staff, binding, assignment_id, artifact, _excluded, _excluded_evidence = _context_workspace_with_unreviewed_source(
        tmp_path, monkeypatch, tools=(), requested_tools=()
    )
    context = _build_staff_context(repo, binding.id, assignment_id)

    assert context.reviewed_tools == []
    assert source_tools(context) == []
    source_id = repo.artifact_evidence(context.tender_id, artifact["id"])[0]["id"]
    with pytest.raises(ValueError, match="read_source.*not granted"):
        context.source(source_id)


@pytest.mark.asyncio
async def test_business_record_with_excluded_provenance_is_withheld_as_a_whole(tmp_path, monkeypatch):
    repo, tender, _staff, binding, assignment_id, _artifact, excluded_artifact, excluded = _context_workspace_with_unreviewed_source(
        tmp_path, monkeypatch, tools=("inspect_tender_records", "read_tender_record")
    )
    context = _build_staff_context(repo, binding.id, assignment_id)
    with repo.atomic() as conn:
        conn.execute(
            "INSERT INTO findings(id,tender_id,title,detail,kind,source_ids_json,origin,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?)",
            (
                "finding-excluded",
                tender["id"],
                "Excluded finding name",
                "EXCLUDED-BUSINESS-CANARY detail",
                "finding",
                json.dumps([excluded["id"]]),
                "manager",
                now(),
                now(),
            ),
        )

    listing = json.loads(
        await next(item for item in source_tools() if item.name == "inspect_tender_records").invoke(
            context, {"record_type": "findings", "offset": 0, "limit": 20}
        )
    )
    assert listing["records"] == []
    assert "EXCLUDED-BUSINESS-CANARY" not in json.dumps(listing)
    assert "outside" in listing["withheld_reason"].lower()

    withheld = json.loads(
        await next(item for item in source_tools() if item.name == "read_tender_record").invoke(
            context,
            {"record_type": "findings", "record_id": "finding-excluded", "offset": 0, "limit": 100},
        )
    )
    assert withheld["available"] is False
    assert "EXCLUDED-BUSINESS-CANARY" not in json.dumps(withheld)


@pytest.mark.asyncio
async def test_staff_cannot_read_manager_messages_or_run_instructions_from_permitted_sources(tmp_path, monkeypatch):
    repo, tender, _staff, binding, assignment_id, artifact = _context_workspace(
        tmp_path, monkeypatch, tools=("inspect_tender_records", "read_tender_record")
    )
    context = _build_staff_context(repo, binding.id, assignment_id)
    source_id = repo.artifact_evidence(tender["id"], artifact["id"])[0]["id"]
    private_run = repo.create_run(tender["id"], "conversation", "MANAGER-RUN-CANARY")["id"]
    repo.update_run(private_run, result={"source_ids": [source_id]})
    with repo.atomic() as conn:
        conn.execute(
            "INSERT INTO messages(id,tender_id,role,content,source_ids_json,run_id,created_at) VALUES(?,?,?,?,?,?,?)",
            (
                "manager-message-private",
                tender["id"],
                "manager",
                "MANAGER-MESSAGE-CANARY",
                json.dumps([source_id]),
                private_run,
                now(),
            ),
        )

    for record_type, record_id, canary in (
        ("messages", "manager-message-private", "MANAGER-MESSAGE-CANARY"),
        ("runs", private_run, "MANAGER-RUN-CANARY"),
    ):
        listing = json.loads(
            await next(item for item in source_tools() if item.name == "inspect_tender_records").invoke(
                context, {"record_type": record_type, "offset": 0, "limit": 20}
            )
        )
        assert listing["records"] == []
        assert canary not in json.dumps(listing)
        detail = json.loads(
            await next(item for item in source_tools() if item.name == "read_tender_record").invoke(
                context, {"record_type": record_type, "record_id": record_id, "offset": 0, "limit": 100}
            )
        )
        assert detail["available"] is False
        assert canary not in json.dumps(detail)


@pytest.mark.asyncio
async def test_staff_record_pagination_filters_before_offset_and_next_cursor(tmp_path, monkeypatch):
    repo, tender, _staff, binding, assignment_id, _artifact, excluded_artifact, excluded = _context_workspace_with_unreviewed_source(
        tmp_path,
        monkeypatch,
        tools=("inspect_tender_records",),
    )
    context = _build_staff_context(repo, binding.id, assignment_id)
    allowed_source = repo.artifact_evidence(tender["id"], _artifact["id"])[0]["id"]
    with repo.atomic() as conn:
        for identifier, title, source_id in (
            ("excluded-page", "EXCLUDED-PAGE-CANARY", excluded["id"]),
            ("allowed-page", "Allowed finding", allowed_source),
        ):
            conn.execute(
                "INSERT INTO findings(id,tender_id,title,detail,kind,source_ids_json,origin,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?)",
                (
                    identifier,
                    tender["id"],
                    title,
                    "synthetic detail",
                    "finding",
                    json.dumps([source_id]),
                    "manager",
                    now(),
                    now(),
                ),
            )

    result = json.loads(
        await next(item for item in source_tools() if item.name == "inspect_tender_records").invoke(
            context, {"record_type": "findings", "offset": 0, "limit": 1}
        )
    )
    assert [row["id"] for row in result["records"]] == ["allowed-page"]
    assert result["next_offset"] is None
    assert "EXCLUDED-PAGE-CANARY" not in json.dumps(result)


@pytest.mark.asyncio
async def test_staff_record_scan_reaches_permitted_history_after_a_long_excluded_prefix(tmp_path, monkeypatch):
    repo, tender, _staff, binding, assignment_id, artifact, _excluded_artifact, excluded = _context_workspace_with_unreviewed_source(
        tmp_path, monkeypatch, tools=("inspect_tender_records",)
    )
    context = _build_staff_context(repo, binding.id, assignment_id)
    allowed_source = repo.artifact_evidence(tender["id"], artifact["id"])[0]["id"]
    with repo.atomic() as conn:
        for index in range(205):
            conn.execute(
                "INSERT INTO findings(id,tender_id,title,detail,kind,source_ids_json,origin,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?)",
                (
                    f"excluded-prefix-{index}", tender["id"], "Excluded prefix", "EXCLUDED-LONG-PREFIX-CANARY",
                    "finding", json.dumps([excluded["id"]]), "manager", now(), now(),
                ),
            )
        conn.execute(
            "INSERT INTO findings(id,tender_id,title,detail,kind,source_ids_json,origin,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?)",
            (
                "allowed-after-prefix", tender["id"], "Allowed after prefix", "Allowed detail",
                "finding", json.dumps([allowed_source]), "manager", now(), now(),
            ),
        )

    inspect_records = next(item for item in source_tools() if item.name == "inspect_tender_records")
    result = json.loads(
        await inspect_records.invoke(context, {"record_type": "findings", "offset": 0, "limit": 1})
    )
    assert [row["id"] for row in result["records"]] == ["allowed-after-prefix"]
    assert result["next_offset"] is None
    assert "EXCLUDED-LONG-PREFIX-CANARY" not in json.dumps(result)


@pytest.mark.asyncio
async def test_staff_requirement_scan_reaches_permitted_record_after_a_long_excluded_prefix(tmp_path, monkeypatch):
    repo, tender, _staff, binding, assignment_id, artifact, _excluded_artifact, excluded = _context_workspace_with_unreviewed_source(
        tmp_path, monkeypatch, tools=("inspect_submission_requirements",)
    )
    context = _build_staff_context(repo, binding.id, assignment_id)
    allowed_source = repo.artifact_evidence(tender["id"], artifact["id"])[0]["id"]
    from quantix.tender_requirements import RequirementService

    RequirementService(repo)
    def requirement_payload(source_id, title, detail):
        evidence = repo.get_evidence(tender["id"], source_id)
        source_artifact = repo.get_artifact(tender["id"], evidence["artifact_id"])
        return {
            "title": title,
            "detail": detail,
            "source_ids": [source_id],
            "deliverable_kind": "analysis_docx",
            "due_date": None,
            "sources": [
                {
                    "source_id": source_id,
                    "artifact_id": source_artifact["id"],
                    "artifact_name": source_artifact["name"],
                    "relative_path": source_artifact["relative_path"],
                    "locator": evidence["locator"],
                    "version": source_artifact["version"],
                    "content_hash": source_artifact["content_hash"],
                    "evidence_hash": "0" * 64,
                    "kind": evidence["kind"],
                }
            ],
            "origin": "manager",
            "run_id": None,
        }
    with repo.atomic() as conn:
        for index in range(105):
            conn.execute(
                "INSERT INTO submission_requirements(id,tender_id,payload_json,created_at) VALUES(?,?,?,?)",
                (
                    f"excluded-requirement-{index}",
                    tender["id"],
                    json.dumps(requirement_payload(excluded["id"], "Excluded requirement", "EXCLUDED-REQUIREMENT-CANARY")),
                    "2026-01-02T00:00:00.000+00:00",
                ),
            )
        conn.execute(
            "INSERT INTO submission_requirements(id,tender_id,payload_json,created_at) VALUES(?,?,?,?)",
            (
                "allowed-after-requirements",
                tender["id"],
                json.dumps(requirement_payload(allowed_source, "Allowed requirement", "Allowed requirement detail")),
                "2026-01-01T00:00:00.000+00:00",
            ),
        )

    inspect_requirements = next(
        item for item in source_tools() if item.name == "inspect_submission_requirements"
    )
    result = json.loads(
        await inspect_requirements.invoke(context, {"offset": 0, "limit": 1})
    )
    assert [row["id"] for row in result["requirements"]] == ["allowed-after-requirements"]
    assert result["next_offset"] is None
    assert "EXCLUDED-REQUIREMENT-CANARY" not in json.dumps(result)


@pytest.mark.asyncio
async def test_scoped_quote_and_reply_reads_keep_permitted_rows_and_withhold_mixed_rows(tmp_path, monkeypatch):
    repo, tender, _staff, binding, assignment_id, artifact, _excluded_artifact, excluded = _context_workspace_with_unreviewed_source(
        tmp_path,
        monkeypatch,
        tools=("inspect_quote_requests", "read_quote_replies"),
    )
    context = _build_staff_context(repo, binding.id, assignment_id)
    allowed_source = repo.artifact_evidence(tender["id"], artifact["id"])[0]["id"]
    allowed_quote = {
        "id": "quote-allowed",
        "to": ["allowed@supplier.example"],
        "cc": [],
        "subject": "Allowed quote",
        "body": "ALLOWED-QUOTE-BODY",
        "status": "draft",
        "delivery_history_status": None,
        "delivery_detail": "",
        "message_id": "message-allowed",
        "source_ids": [allowed_source],
    }
    mixed_quote = allowed_quote | {
        "id": "quote-mixed",
        "to": ["mixed@supplier.example"],
        "subject": "MIXED-QUOTE-CANARY",
        "body": "MIXED-QUOTE-CANARY",
        "source_ids": [allowed_source, excluded["id"]],
    }
    allowed_reply = {
        "id": "reply-allowed",
        "origin": "manual",
        "sender": "allowed@supplier.example",
        "received_at": now(),
        "date_header": None,
        "date_basis": "engineer_entered",
        "subject": "Allowed reply",
        "warnings": [],
        "source_ids": [allowed_source],
    }
    mixed_reply = allowed_reply | {
        "id": "reply-mixed",
        "subject": "MIXED-REPLY-CANARY",
        "source_ids": [allowed_source, excluded["id"]],
    }
    monkeypatch.setattr(
        "quantix.correspondence.QuoteService.list_drafts",
        lambda _service, _tender_id: [allowed_quote, mixed_quote],
    )
    monkeypatch.setattr(
        "quantix.correspondence.QuoteService.replies",
        lambda _service, _tender_id, _quote_id: [allowed_reply, mixed_reply],
    )

    inspect_quotes = next(item for item in source_tools() if item.name == "inspect_quote_requests")
    quotes = json.loads(await inspect_quotes.invoke(context, {"offset": 0, "limit": 20}))
    assert [row["id"] for row in quotes["quotes"]] == ["quote-allowed"]
    assert "MIXED-QUOTE-CANARY" not in json.dumps(quotes)
    read_replies = next(item for item in source_tools() if item.name == "read_quote_replies")
    replies = json.loads(
        await read_replies.invoke(context, {"quote_id": "quote-allowed", "offset": 0, "limit": 20})
    )
    assert [row["id"] for row in replies["replies"]] == ["reply-allowed"]
    assert "MIXED-REPLY-CANARY" not in json.dumps(replies)


@pytest.mark.asyncio
async def test_manager_quote_and_reply_offsets_page_full_lists_without_repeating_first_page(tmp_path, monkeypatch):
    from quantix.office_tools import OfficeContext

    repo, tender, _connections, _connection, _model, _policy, _route = __import__(
        "test_catalog_authority", fromlist=["configured_office"]
    ).configured_office(tmp_path, monkeypatch)
    run = repo.create_run(tender["id"], "manager", "Synthetic manager read")
    context = OfficeContext(repo, tender["id"], run["id"])
    quotes = [
        {
            "id": f"quote-{index}",
            "to": [f"supplier{index}@example.test"],
            "cc": [],
            "subject": f"Quote {index}",
            "body": f"Quote body {index}",
            "status": "draft",
            "delivery_history_status": None,
            "delivery_detail": "",
            "message_id": f"message-{index}",
            "source_ids": [],
        }
        for index in range(3)
    ]
    replies = [
        {
            "id": f"reply-{index}",
            "origin": "manual",
            "sender": f"supplier{index}@example.test",
            "received_at": now(),
            "date_header": None,
            "date_basis": "engineer_entered",
            "subject": f"Reply {index}",
            "warnings": [],
            "source_ids": [],
        }
        for index in range(3)
    ]
    monkeypatch.setattr(
        "quantix.correspondence.QuoteService.list_drafts",
        lambda _service, _tender_id: quotes,
    )
    monkeypatch.setattr(
        "quantix.correspondence.QuoteService.replies",
        lambda _service, _tender_id, _quote_id: replies,
    )
    inspect_quotes = next(item for item in source_tools() if item.name == "inspect_quote_requests")
    quote_page = json.loads(
        await inspect_quotes.invoke(context, {"offset": 2, "limit": 1})
    )
    assert [row["id"] for row in quote_page["quotes"]] == ["quote-2"]
    assert quote_page["next_offset"] is None
    read_replies = next(item for item in source_tools() if item.name == "read_quote_replies")
    reply_page = json.loads(
        await read_replies.invoke(context, {"quote_id": "quote-0", "offset": 2, "limit": 1})
    )
    assert [row["id"] for row in reply_page["replies"]] == ["reply-2"]
    assert reply_page["next_offset"] is None


def test_staff_measurement_requires_own_visual_receipt_identity(tmp_path, monkeypatch):
    repo, tender, staff, binding, assignment_id, artifact = _context_workspace(
        tmp_path, monkeypatch, tools=("calculate_drawing_measurement",)
    )
    context = _build_staff_context(repo, binding.id, assignment_id)
    source_id = repo.artifact_evidence(tender["id"], artifact["id"])[0]["id"]
    context.add_seen_source(source_id)
    repo.event(
        binding.root_run_id,
        "visual_source_viewed",
        "Other actor viewed a drawing region.",
        {"source_id": source_id, "artifact_id": artifact["id"], "page": 1, "region": [0, 0, 1, 1]},
    )
    monkeypatch.setattr(
        "quantix.measurements.MeasurementService.supporting_sources",
        lambda *_args, **_kwargs: [],
    )
    values = {
        "artifact_id": artifact["id"],
        "page": 1,
        "mode": "count",
        "points": [[0.5, 0.5]],
        "calibration_points": None,
        "calibration_metres": None,
        "scope_label": "Synthetic measured mark",
        "source_ids": [source_id],
    }
    from quantix.office_measurement import validate_agent_measurement

    with pytest.raises(ValueError, match="Inspect the measured drawing page"):
        validate_agent_measurement(context, values)

    with repo.atomic() as conn:
        for _index in range(205):
            conn.execute(
                """
                INSERT INTO staff_source_receipts(
                    id,tender_id,actor_id,profile_id,profile_version,assignment_id,
                    route_binding_id,root_run_id,source_id,artifact_id,artifact_version,
                    content_hash,locator,text_offset,text_length,page,region_json,
                    visible_cells_json,method,created_at
                ) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)
                """,
                (
                    new_id(), tender["id"], staff.staff.id, staff.staff.id, staff.staff.version,
                    assignment_id, binding.id, binding.root_run_id, source_id, artifact["id"],
                    artifact["version"], artifact["content_hash"], "Page 1 text", 0, 1, 1,
                    None, "[]", "source", now(),
                ),
            )
        conn.execute(
            """
            INSERT INTO staff_source_receipts(
                id,tender_id,actor_id,profile_id,profile_version,assignment_id,
                route_binding_id,root_run_id,source_id,artifact_id,artifact_version,
                content_hash,locator,text_offset,text_length,page,region_json,
                visible_cells_json,method,created_at
            ) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)
            """,
            (
                new_id(), tender["id"], staff.staff.id, staff.staff.id, staff.staff.version,
                assignment_id, binding.id, binding.root_run_id, source_id, artifact["id"],
                artifact["version"], artifact["content_hash"], "Page 1", None, None, 1,
                json.dumps([0, 0, 1, 1]), "[]", "visual", now(),
            ),
        )
    proposal = validate_agent_measurement(context, values)
    assert proposal.artifact_id == artifact["id"]


def test_staff_context_round_trips_through_prepare_and_assignment_save(tmp_path, monkeypatch):
    repo, tender, staff, binding, assignment_id, artifact = _context_workspace(
        tmp_path, monkeypatch, tools=("read_source",)
    )
    context = _build_staff_context(repo, binding.id, assignment_id)
    source_id = repo.artifact_evidence(tender["id"], artifact["id"])[0]["id"]
    context.source(source_id)

    from quantix.office import prepare_result
    from quantix.office_research import ResearchRecord
    from quantix.office_types import OfficeOutput
    from quantix.staff_assignment_models import PreparedStaffDraft
    from quantix.staff_assignments import StaffAssignmentService

    prepared = prepare_result(
        OfficeOutput(summary="A source-backed staff draft.", source_ids=[source_id]),
        context,
        {"requests": 1, "usage_complete": True},
        ResearchRecord(context),
    )
    assert prepared.approved_plan_id == binding.plan_id
    assert prepared.actor_id == staff.staff.id
    assert prepared.assignment_id == assignment_id
    saved = StaffAssignmentService(repo).save_result(
        tender["id"],
        assignment_id,
        PreparedStaffDraft(
            assignment_id=assignment_id,
            staff_id=staff.staff.id,
            staff_version=staff.staff.version,
            route_binding_id=binding.id,
            prepared=prepared,
        ),
    )
    assert saved.assignment_id == assignment_id
    assert saved.approved_plan_id == binding.plan_id


@pytest.mark.asyncio
async def test_scoped_document_listing_does_not_reveal_unreviewed_names(tmp_path, monkeypatch):
    repo, tender, _staff, binding, assignment_id, artifact, excluded, _excluded_evidence = _context_workspace_with_unreviewed_source(
        tmp_path, monkeypatch, tools=("list_documents",)
    )
    context = _build_staff_context(repo, binding.id, assignment_id)
    listing = json.loads(
        await next(item for item in source_tools() if item.name == "list_documents").invoke(context, {})
    )
    names = {row["name"] for row in listing["documents"]}
    assert artifact["name"] in names
    assert excluded["name"] not in names
    assert excluded["id"] not in json.dumps(listing)


@pytest.mark.asyncio
async def test_project_map_with_excluded_provenance_is_withheld(tmp_path, monkeypatch):
    repo, tender, _staff, binding, assignment_id, _artifact, excluded, _excluded_evidence = _context_workspace_with_unreviewed_source(
        tmp_path, monkeypatch, tools=("inspect_project_map",)
    )
    context = _build_staff_context(repo, binding.id, assignment_id)
    with repo.atomic() as conn:
        conn.execute(
            "CREATE TABLE IF NOT EXISTS project_nodes(id TEXT PRIMARY KEY,tender_id TEXT NOT NULL,payload_json TEXT NOT NULL,created_at TEXT NOT NULL)"
        )
        conn.execute(
            "INSERT INTO project_nodes VALUES(?,?,?,?)",
            (
                "node-excluded",
                tender["id"],
                json.dumps({"title": "EXCLUDED-PROJECT-CANARY", "parent_id": None, "source_ids": [repo.artifact_evidence(tender["id"], excluded["id"])[0]["id"]], "source_manifest": []}),
                now(),
            ),
        )

    result = json.loads(
        await next(item for item in source_tools() if item.name == "inspect_project_map").invoke(
            context, {"offset": 0, "limit": 20}
        )
    )
    assert result["nodes"] == []
    assert "EXCLUDED-PROJECT-CANARY" not in json.dumps(result)


def test_receipt_write_is_rejected_after_terminal_root(tmp_path, monkeypatch):
    repo, tender, _staff, binding, assignment_id, artifact = _context_workspace(tmp_path, monkeypatch)
    context = _build_staff_context(repo, binding.id, assignment_id)
    source_id = repo.artifact_evidence(tender["id"], artifact["id"])[0]["id"]
    repo.update_run(binding.root_run_id, status="completed")

    with pytest.raises(ValueError, match="no longer active"):
        context.source(source_id)
    with repo.db.connect() as conn:
        assert conn.execute("SELECT COUNT(*) FROM staff_source_receipts").fetchone()[0] == 0


def test_receipt_write_is_rejected_after_assignment_cancellation(tmp_path, monkeypatch):
    repo, tender, _staff, binding, assignment_id, artifact = _context_workspace(tmp_path, monkeypatch)
    context = _build_staff_context(repo, binding.id, assignment_id)
    source_id = repo.artifact_evidence(tender["id"], artifact["id"])[0]["id"]
    with repo.atomic() as conn:
        conn.execute(
            "UPDATE office_assignments SET status='cancelled',revision=revision+1 WHERE id=?",
            (assignment_id,),
        )

    with pytest.raises(ValueError, match="assignment .*no longer active"):
        context.source(source_id)
    with repo.db.connect() as conn:
        assert conn.execute("SELECT COUNT(*) FROM staff_source_receipts").fetchone()[0] == 0
