import hashlib
import importlib
from email import policy
from email.parser import BytesParser

import pytest

from quantix.repository import Repository


@pytest.fixture
def setup(tmp_path):
    module = importlib.import_module("quantix.correspondence")
    repo = Repository(tmp_path)
    tid = repo.create_tender("School tender")["id"]
    source = b"Concrete specification attachment"
    digest = hashlib.sha256(source).hexdigest()
    (repo.objects / digest).write_bytes(source)
    artifact, _ = repo.register_artifact(
        tid,
        "Scope/spec.pdf",
        digest,
        len(source),
        {
            "kind": "pdf",
            "status": "extracted",
            "segments": [{"locator": "page:1", "text": "Concrete grade 35"}],
        },
    )
    return repo, tid, artifact, module.QuoteService(repo)


def draft_data(artifact=None, **changes):
    return {
        "to": ["sales@supplier.example"],
        "cc": [],
        "subject": "Concrete quotation request",
        "body": "Please quote the attached concrete scope, including delivery and VAT terms.",
        "attachment_ids": [artifact["id"]] if artifact else [],
        "source_ids": [],
        **changes,
    }


def test_draft_export_preserves_exact_attachment(setup):
    repo, tid, artifact, service = setup
    draft = service.create_draft(tid, draft_data(artifact))
    preview = service.preview(tid, draft["id"])
    parsed = BytesParser(policy=policy.default).parsebytes(service.eml(tid, draft["id"]))
    assert draft["status"] == "draft"
    assert parsed["To"] == "sales@supplier.example"
    assert parsed["Message-ID"] == draft["message_id"]
    assert (
        next(parsed.iter_attachments()).get_payload(decode=True)
        == b"Concrete specification attachment"
    )
    assert preview["attachments"][0]["content_hash"] == artifact["content_hash"]
    assert preview["warnings"] == []


def test_editing_a_draft_replaces_its_content(setup):
    repo, tid, artifact, service = setup
    draft = service.create_draft(tid, draft_data())
    edited = service.edit_draft(tid, draft["id"], draft_data(body="Revised delivery condition."))
    assert edited["body"] == "Revised delivery condition.\n"
    assert edited["message_id"] == draft["message_id"]
    assert [row["id"] for row in service.list_drafts(tid)] == [draft["id"]]


def test_foreign_attachments_and_header_injection_are_rejected(setup):
    repo, tid, artifact, service = setup
    other = repo.create_tender("Other tender")["id"]
    with pytest.raises((KeyError, ValueError)):
        service.create_draft(other, draft_data(artifact))
    for changes in [
        {"subject": "Hello\r\nBcc: stolen@example.com"},
        {"to": ["sales@example.com\nBcc: stolen@example.com"]},
    ]:
        with pytest.raises(ValueError):
            service.create_draft(tid, draft_data(**changes))


def test_changed_attachment_bytes_block_export(setup):
    repo, tid, artifact, service = setup
    draft = service.create_draft(tid, draft_data(artifact))
    repo.object_path(tid, artifact["id"]).write_bytes(b"changed")
    with pytest.raises(ValueError, match="attachment|integrity"):
        service.eml(tid, draft["id"])


def test_manual_reply_keeps_original_text_headers_and_scoped_sources(setup):
    repo, tid, artifact, service = setup
    draft = service.create_draft(tid, draft_data())
    source_id = repo.artifact_evidence(tid, artifact["id"])[0]["id"]
    reply = service.register_reply(
        tid,
        draft["id"],
        {
            "sender": "sales@supplier.example",
            "received_at": "2026-09-06T12:00:00Z",
            "text": "Concrete quotation: 100 EGP/m3, VAT excluded.",
            "subject": "Our quotation",
            "headers": "From: sales@supplier.example\r\nSubject: Our quotation",
            "source_ids": [source_id],
        },
    )
    assert reply["text"].startswith("Concrete quotation")
    assert reply["supporting_source_ids"] == [source_id]
    assert repo.get_evidence(tid, reply["source_ids"][0])["text"].startswith("Concrete quotation")
    assert len(service.replies(tid, draft["id"])) == 1


def test_long_manual_reply_becomes_bounded_citable_tender_segments(setup):
    repo, tid, artifact, service = setup
    draft = service.create_draft(tid, draft_data())
    original = "Quoted concrete rate and delivery conditions. " * 300
    reply = service.register_reply(
        tid,
        draft["id"],
        {
            "sender": "sales@supplier.example",
            "received_at": "2026-09-06T12:00:00Z",
            "text": original,
        },
    )
    sources = [repo.get_evidence(tid, source_id) for source_id in reply["source_ids"]]
    assert len(sources) > 1
    assert all(len(source["text"]) <= 6000 for source in sources)
    assert "".join(source["text"] for source in sources) == original
    assert sources[0]["metadata"]["sender"] == "sales@supplier.example"


def test_routes_export_real_draft_and_offer_no_mail_sending(setup):
    from fastapi import FastAPI
    from fastapi.testclient import TestClient

    repo, tid, artifact, service = setup
    routes = importlib.import_module("quantix.correspondence_routes")
    app = FastAPI()
    app.include_router(routes.create_router(repo))
    client = TestClient(app)
    draft = client.post(f"/api/tenders/{tid}/quotes", json=draft_data(artifact)).json()
    response = client.get(f"/api/tenders/{tid}/quotes/{draft['id']}/eml")
    assert response.status_code == 200
    assert b"Message-ID:" in response.content
    paths = app.openapi()["paths"]
    assert not any(path.startswith("/api/mail") or path.endswith("/send") for path in paths)
    assert "QuotePreview" in app.openapi()["components"]["schemas"]
