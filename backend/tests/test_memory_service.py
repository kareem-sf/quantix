"""Working memory keeps exact Tender dependencies and explicit promotion."""

from __future__ import annotations

import hashlib

import pytest

from quantix.execution_context import engineer_identity
from quantix.repository import Repository


def _source(repo, tender_id: str, text: str, digest: str = "a" * 64):
    artifact, _ = repo.register_artifact(
        tender_id,
        "Sources/spec.pdf",
        digest,
        len(text.encode()),
        {
            "kind": "pdf",
            "status": "extracted",
            "segments": [{"locator": "page:1", "text": text}],
        },
    )
    return artifact, repo.artifact_evidence(tender_id, artifact["id"])[0]


def test_memory_dependencies_become_needs_review_after_source_revision(tmp_path):
    from quantix.memory_models import WorkingMemoryCommand
    from quantix.memory_service import MemoryService

    repo = Repository(tmp_path / "quantix")
    tender = repo.create_tender("Synthetic memory Tender")
    artifact, evidence = _source(repo, tender["id"], "Specified concrete is C30/37.")
    service = MemoryService(repo)
    saved = service.save(
        engineer_identity(tender["id"]),
        WorkingMemoryCommand(
            kind="assumption",
            title="Concrete grade",
            content="Use C30/37 pending structural schedule reconciliation.",
            source_ids=[evidence["id"], evidence["id"]],
            idempotency_key="memory-concrete",
        ),
    )
    assert saved.state == "current"
    assert len(saved.dependencies) == 1

    repo.register_artifact(
        tender["id"],
        artifact["relative_path"],
        hashlib.sha256(b"Specified concrete is C35/45.").hexdigest(),
        len(b"Specified concrete is C35/45."),
        {
            "kind": "pdf",
            "status": "extracted",
            "segments": [{"locator": "page:1", "text": "Specified concrete is C35/45."}],
        },
    )
    changed = service.get(tender["id"], saved.id)
    assert changed.state == "needs_review"
    assert changed.review_reasons == ["source_revision_changed"]
    assert service.affected(tender["id"])[0].owner_id == saved.id
    from quantix.memory_models import MemoryPromotionRequest

    with pytest.raises(ValueError, match="current sources"):
        service.promote(
            tender["id"],
            saved.id,
            MemoryPromotionRequest(
                category="reference",
                engineer_confirmed=True,
                rationale="Synthetic stale note must not be promoted.",
            ),
        )


def test_memory_rejects_cross_tender_sources_and_requires_explicit_promotion(tmp_path):
    from pydantic import ValidationError

    from quantix.memory_models import MemoryPromotionRequest, WorkingMemoryCommand
    from quantix.memory_service import MemoryService

    repo = Repository(tmp_path / "quantix")
    tender = repo.create_tender("Origin Tender")
    other = repo.create_tender("Other Tender")
    _artifact, evidence = _source(repo, tender["id"], "Use a 5% waste allowance.")
    service = MemoryService(repo)
    with pytest.raises(KeyError):
        service.save(
            engineer_identity(other["id"]),
            WorkingMemoryCommand(
                kind="scratch",
                title="Foreign note",
                content="This must stay out.",
                source_ids=[evidence["id"]],
                idempotency_key="foreign-memory",
            ),
        )
    saved = service.save(
        engineer_identity(tender["id"]),
        WorkingMemoryCommand(
            kind="assumption",
            title="Waste allowance",
            content="Use a 5% waste allowance only after package review.",
            source_ids=[evidence["id"]],
            idempotency_key="waste-memory",
        ),
    )
    with pytest.raises(ValidationError):
        MemoryPromotionRequest(
            category="method",
            engineer_confirmed=False,
            rationale="Synthetic rejection",
        )
    promoted = service.promote(
        tender["id"],
        saved.id,
        MemoryPromotionRequest(
            category="method",
            engineer_confirmed=True,
            rationale="Approved as reusable guidance for synthetic tests.",
        ),
    )
    assert promoted["source_tender_id"] == tender["id"]
    assert promoted["source_ids"] == [evidence["id"]]
    assert promoted["needs_recheck"] is False
    assert (
        service.promote(
            tender["id"],
            saved.id,
            MemoryPromotionRequest(
                category="method",
                engineer_confirmed=True,
                rationale="Approved as reusable guidance for synthetic tests.",
            ),
        )["id"]
        == promoted["id"]
    )


def test_overview_separates_working_notes_decisions_and_company_knowledge(tmp_path):
    from quantix.knowledge import KnowledgeService
    from quantix.memory_models import WorkingMemoryCommand
    from quantix.memory_service import MemoryService

    repo = Repository(tmp_path / "quantix")
    tender = repo.create_tender("Synthetic memory overview Tender")
    finding = repo.add_finding(
        tender["id"], "Drainage risk", "Invert level is missing.", "risk", []
    )
    repo.decide_finding(tender["id"], finding["id"], "accept", "Track in the bid risk register.")
    KnowledgeService(repo).create(
        {
            "title": "Plain language",
            "content": "Lead with the decision needed.",
            "category": "preference",
            "engineer_confirmed": True,
            "rationale": "Synthetic company guidance.",
        }
    )
    service = MemoryService(repo)
    service.save(
        engineer_identity(tender["id"]),
        WorkingMemoryCommand(
            kind="scratch",
            title="Drainage follow-up",
            content="Ask for the missing invert level.",
            source_ids=[],
            idempotency_key="scratch-drainage",
        ),
    )
    overview = service.overview(tender["id"])
    assert [item.kind for item in overview.working_memory] == ["scratch"]
    assert overview.approved_decisions[0]["target_id"] == finding["id"]
    assert overview.company_knowledge[0]["title"] == "Plain language"


def test_memory_commit_guard_blocks_a_stopped_actor_before_insert(tmp_path):
    from contextlib import nullcontext

    from quantix.memory_models import WorkingMemoryCommand
    from quantix.memory_service import MemoryService

    repo = Repository(tmp_path / "quantix")
    tender = repo.create_tender("Synthetic stopped memory writer")
    service = MemoryService(repo)

    def stopped():
        raise InterruptedError("This Tender run is no longer active.")

    with pytest.raises(InterruptedError, match="no longer active"):
        service.save(
            engineer_identity(tender["id"]),
            WorkingMemoryCommand(
                kind="scratch",
                title="Late note",
                content="Must not be saved after stop.",
                idempotency_key="late-memory",
            ),
            write_guard=nullcontext,
            validate_commit=stopped,
        )
    assert service.list(tender["id"]) == []


def test_working_memory_offset_reaches_the_101st_saved_note(tmp_path):
    from quantix.memory_models import WorkingMemoryCommand
    from quantix.memory_service import MemoryService

    repo = Repository(tmp_path / "quantix")
    tender = repo.create_tender("Synthetic paged memory")
    service = MemoryService(repo)
    identity = engineer_identity(tender["id"])
    for index in range(101):
        service.save(
            identity,
            WorkingMemoryCommand(
                kind="scratch",
                title=f"Working note {index + 1}",
                content="Synthetic paging evidence.",
                idempotency_key=f"memory-page-{index + 1}",
            ),
        )

    page = service.list(tender["id"], offset=100, limit=50)
    assert len(page) == 1
    assert page[0].title == "Working note 1"
