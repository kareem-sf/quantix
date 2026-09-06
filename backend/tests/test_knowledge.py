"""Reusable note decisions use synthetic workspaces and never read customer files."""

import hashlib
import json
import sqlite3
from datetime import UTC, datetime, timedelta

import pytest
from fastapi import FastAPI
from fastapi.responses import JSONResponse
from fastapi.testclient import TestClient

from quantix.knowledge import KnowledgeService
from quantix.knowledge_models import KnowledgeRecord
from quantix.knowledge_routes import create_router
from quantix.repository import Repository


def decision(**changes):
    return {
        "engineer_confirmed": True,
        "rationale": "Approved for reuse with Tender-specific review.",
        **changes,
    }


def note(**changes):
    return {
        "title": "Check delivery scope",
        "content": "Record delivery distance before comparing supply quotations.",
        "category": "method",
        **decision(),
        **changes,
    }


def source(repo, tid, revision=1):
    text = f"Synthetic supporting source revision {revision}."
    digest = hashlib.sha256(text.encode()).hexdigest()
    artifact, _ = repo.register_artifact(
        tid,
        "Example/spec.pdf",
        digest,
        len(text),
        {
            "kind": "pdf",
            "status": "extracted",
            "segments": [{"locator": "page:1", "text": text, "page": 1}],
        },
    )
    return artifact, repo.artifact_evidence(tid, artifact["id"])[0]


@pytest.fixture
def setup(tmp_path):
    repo = Repository(tmp_path)
    tid = repo.create_tender("Synthetic original Tender")["id"]
    artifact, evidence = source(repo, tid)
    return repo, tid, artifact, evidence, KnowledgeService(repo)


def test_explicit_note_approval_is_global_and_does_not_promote_tender_facts(setup):
    repo, tid, _, _, service = setup
    saved = service.create(note())
    assert saved["status"] == "approved"
    assert saved["source_tender_id"] is None
    assert saved["sources_current"] is None
    assert saved["approval_rationale"] == decision()["rationale"]
    assert saved["audit"][0]["action"] == "approve"
    assert saved["audit"][0]["engineer_confirmed"] is True
    assert "not current Tender evidence" in saved["use_limitations"]
    assert repo.list_findings(tid) == []
    with repo.db.connect() as conn:
        assert conn.execute("SELECT COUNT(*) FROM decisions").fetchone()[0] == 0
    assert KnowledgeRecord.model_validate(saved).id == saved["id"]
    assert KnowledgeService(Repository(repo.home)).get(saved["id"])["content"] == saved["content"]


@pytest.mark.parametrize(
    "changes",
    [
        {"engineer_confirmed": False},
        {"engineer_confirmed": None},
        {"engineer_confirmed": 1},
        {"engineer_confirmed": "true"},
        {"rationale": "  "},
        {"title": " "},
        {"content": " "},
        {"category": "automatic_fact"},
        {"status": "approved"},
        {"source_ids": ["fake-source"]},
        {"content": "bad\x00text"},
    ],
)
def test_invalid_or_unapproved_note_is_never_persisted(setup, changes):
    _, _, _, _, service = setup
    with pytest.raises((ValueError, KeyError)):
        service.create(note(**changes))
    assert service.list() == []


def test_scoped_source_provenance_is_captured_without_copying_extracted_text(setup):
    repo, tid, artifact, evidence, service = setup
    saved = service.create(note(source_tender_id=tid, source_ids=[evidence["id"], evidence["id"]]))
    assert saved["source_ids"] == [evidence["id"]]
    assert saved["source_tender_name"] == "Synthetic original Tender"
    linked = saved["sources"][0]
    assert linked["artifact_id"] == artifact["id"]
    assert linked["content_hash"] == artifact["content_hash"]
    assert linked["version"] == 1
    assert linked["locator"] == "page:1"
    assert linked["is_current"] is True
    assert linked["available"] is True
    assert len(linked["evidence_hash"]) == 64
    assert saved["sources_current"] is True
    assert saved["needs_recheck"] is False
    assert evidence["text"] not in json.dumps(saved)
    other = repo.create_tender("Unrelated Tender")["id"]
    with pytest.raises(KeyError):
        service.create(note(source_tender_id=other, source_ids=[evidence["id"]]))
    with pytest.raises(KeyError):
        service.create(note(source_tender_id="missing-tender"))


def test_source_revision_marks_recheck_without_rewriting_approved_content(setup):
    repo, tid, _, evidence, service = setup
    saved = service.create(note(source_tender_id=tid, source_ids=[evidence["id"]]))
    source(repo, tid, 2)
    stale = service.get(saved["id"])
    assert stale["content"] == saved["content"]
    assert stale["approval_rationale"] == saved["approval_rationale"]
    assert stale["status"] == "approved"
    assert stale["sources_current"] is False
    assert stale["needs_recheck"] is True
    assert stale["sources"][0]["content_hash"] == saved["sources"][0]["content_hash"]
    assert stale["sources"][0]["is_current"] is False
    assert "source_revision_changed" in stale["revalidation_reasons"]
    with pytest.raises(ValueError, match="current"):
        service.create(note(source_tender_id=tid, source_ids=[evidence["id"]]))


def test_evidence_change_and_missing_source_are_visible_without_read_failures(setup):
    repo, tid, _, evidence, service = setup
    saved = service.create(note(source_tender_id=tid, source_ids=[evidence["id"]]))
    with repo.db.connect(write=True) as conn:
        conn.execute(
            "UPDATE evidence SET text='Synthetic corrected extraction' WHERE id=?",
            (evidence["id"],),
        )
    changed = service.get(saved["id"])
    assert changed["needs_recheck"] is True
    assert "source_evidence_changed" in changed["revalidation_reasons"]
    with repo.db.connect(write=True) as conn:
        conn.execute("DELETE FROM evidence WHERE id=?", (evidence["id"],))
    missing = service.get(saved["id"])
    assert missing["sources"][0]["available"] is False
    assert missing["sources_current"] is False
    assert "source_unavailable" in missing["revalidation_reasons"]


@pytest.mark.parametrize("category", ["price", "tax"])
def test_price_and_tax_always_require_new_commercial_validation(setup, category):
    _, _, _, _, service = setup
    today = datetime.now(UTC).date()
    saved = service.create(
        note(
            category=category,
            verified_on=today.isoformat(),
            recheck_after=(today + timedelta(days=90)).isoformat(),
        )
    )
    assert saved["commercial_revalidation_required"] is True
    assert saved["needs_recheck"] is True
    assert "commercial_use_requires_fresh_validation" in saved["revalidation_reasons"]
    assert saved["verified_on"] == today.isoformat()


def test_date_checks_do_not_turn_engineer_approval_into_independent_verification(setup):
    _, _, _, _, service = setup
    today = datetime.now(UTC).date()
    due = service.create(
        note(verified_on=(today - timedelta(days=30)).isoformat(), recheck_after=today.isoformat())
    )
    assert due["needs_recheck"] is True
    assert "recheck_date_reached" in due["revalidation_reasons"]
    assert "does not independently verify" in due["use_limitations"]
    with pytest.raises(ValueError):
        service.create(note(verified_on=(today + timedelta(days=1)).isoformat()))
    with pytest.raises(ValueError):
        service.create(
            note(
                verified_on=today.isoformat(), recheck_after=(today - timedelta(days=1)).isoformat()
            )
        )


def test_withdrawal_is_a_separate_immutable_decision_and_default_retrieval_excludes_it(setup):
    _, _, _, _, service = setup
    saved = service.create(note())
    with pytest.raises(ValueError):
        service.withdraw(saved["id"], decision(engineer_confirmed=False))
    withdrawn = service.withdraw(
        saved["id"], decision(rationale="This method is no longer approved for reuse.")
    )
    assert withdrawn["status"] == "withdrawn"
    assert withdrawn["content"] == saved["content"]
    assert withdrawn["withdrawal_rationale"] == "This method is no longer approved for reuse."
    assert [event["action"] for event in withdrawn["audit"]] == ["approve", "withdraw"]
    assert service.list() == []
    assert service.list(include_withdrawn=True)[0]["id"] == saved["id"]
    with pytest.raises(ValueError, match="withdrawn"):
        service.withdraw(saved["id"], decision())
    assert len(service.get(saved["id"])["audit"]) == 2


def test_note_payload_and_approval_audit_cannot_be_modified_or_deleted(setup):
    repo, _, _, _, service = setup
    saved = service.create(note())
    with repo.db.connect(write=True) as conn:
        for statement in [
            "UPDATE reusable_knowledge SET payload_json='{}' WHERE id=?",
            "DELETE FROM reusable_knowledge WHERE id=?",
            "UPDATE knowledge_audit SET rationale='Changed' WHERE knowledge_id=?",
            "DELETE FROM knowledge_audit WHERE knowledge_id=?",
        ]:
            with pytest.raises(sqlite3.IntegrityError, match="immutable"):
                conn.execute(statement, (saved["id"],))
    assert service.get(saved["id"])["approval_rationale"] == decision()["rationale"]


def test_creation_and_withdrawal_participate_in_the_callers_atomic_transaction(setup):
    repo, _, _, _, service = setup
    with pytest.raises(RuntimeError):
        with repo.atomic():
            service.create(note())
            raise RuntimeError("Cancel surrounding operation")
    assert service.list(include_withdrawn=True) == []
    saved = service.create(note())
    with pytest.raises(RuntimeError):
        with repo.atomic():
            service.withdraw(saved["id"], decision())
            raise RuntimeError("Cancel surrounding operation")
    assert service.get(saved["id"])["status"] == "approved"
    assert len(service.get(saved["id"])["audit"]) == 1


def test_filtered_pagination_and_typed_routes_require_engineer_decisions(setup):
    repo, _, _, _, service = setup
    service.create(note(category="preference", title="First preference"))
    service.create(note(category="method", title="Method"))
    service.create(note(category="preference", title="Second preference"))
    assert len(service.list(category="preference", limit=1)) == 1
    assert service.list(category="preference", limit=1, offset=1)[0]["title"] == "First preference"
    app = FastAPI()
    app.include_router(create_router(repo))

    @app.exception_handler(KeyError)
    async def not_found(request, error):
        return JSONResponse({"detail": "Reusable note not found."}, status_code=404)

    @app.exception_handler(ValueError)
    async def invalid(request, error):
        return JSONResponse({"detail": str(error)}, status_code=409)

    with TestClient(app) as client:
        response = client.post("/api/knowledge", json=note())
        assert response.status_code == 200
        saved = response.json()
        assert client.get(f"/api/knowledge/{saved['id']}").json()["audit"][0]["action"] == "approve"
        assert (
            client.get("/api/knowledge?category=preference&limit=1&offset=1").json()[0]["title"]
            == "First preference"
        )
        assert (
            client.post(
                "/api/knowledge",
                json={"title": "Autonomous", "content": "No approval", "category": "method"},
            ).status_code
            == 422
        )
        assert (
            client.post(
                f"/api/knowledge/{saved['id']}/withdraw", json=decision(engineer_confirmed=False)
            ).status_code
            == 422
        )
        assert (
            client.post(f"/api/knowledge/{saved['id']}/withdraw", json=decision()).json()["status"]
            == "withdrawn"
        )
        assert client.get("/api/knowledge/missing").status_code == 404
        assert client.get("/api/knowledge?limit=0").status_code == 422
        assert "KnowledgeRecord" in app.openapi()["components"]["schemas"]


def test_public_provenance_keeps_exact_source_names_and_locations(setup):
    repo, tid, _, _, service = setup
    artifact, _ = repo.register_artifact(
        tid,
        " Leading source.pdf",
        "c" * 64,
        10,
        {
            "kind": "pdf",
            "status": "extracted",
            "segments": [{"locator": " Section 1 ", "text": "Synthetic note support"}],
        },
    )
    evidence = repo.artifact_evidence(tid, artifact["id"])[0]
    saved = service.create(note(source_tender_id=tid, source_ids=[evidence["id"]]))
    public = KnowledgeRecord.model_validate(saved).model_dump(mode="json")
    assert public["sources"][0]["artifact_name"] == " Leading source.pdf"
    assert public["sources"][0]["relative_path"] == " Leading source.pdf"
    assert public["sources"][0]["locator"] == " Section 1 "
