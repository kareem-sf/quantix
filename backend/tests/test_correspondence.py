import hashlib
import importlib
import json
import smtplib
import ssl
from email import policy
from email.parser import BytesParser

import pytest

from quantix.repository import Repository


@pytest.fixture
def setup(tmp_path, monkeypatch):
    module = importlib.import_module("quantix.correspondence")
    passwords = {}
    monkeypatch.setattr(
        module.keyring, "get_password", lambda service, user: passwords.get((service, user))
    )
    monkeypatch.setattr(
        module.keyring,
        "set_password",
        lambda service, user, password: passwords.__setitem__((service, user), password),
    )
    monkeypatch.setattr(
        module.keyring,
        "delete_password",
        lambda service, user: passwords.pop((service, user), None),
    )

    def no_live_mail(*args, **kwargs):
        raise AssertionError("Live SMTP and IMAP access is prohibited in development tests.")

    for name in ("SMTP", "SMTP_SSL"):
        monkeypatch.setattr(module.smtplib, name, no_live_mail)
    monkeypatch.setattr(module.imaplib, "IMAP4_SSL", no_live_mail)
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
    return repo, tid, artifact, module.QuoteService(repo), module, passwords


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


def configure(service, **changes):
    return service.update_settings(
        {
            "smtp_host": "smtp.example.com",
            "smtp_port": 465,
            "smtp_security": "ssl",
            "smtp_username": "engineer@example.com",
            "from_address": "engineer@example.com",
            "smtp_password": "smtp-private-password",
            **changes,
        }
    )


def approve(service, tid, draft_id):
    preview = service.preview(tid, draft_id)
    decision = {
        "fingerprint": preview["fingerprint"],
        "engineer_confirmed": True,
        "rationale": "Reviewed recipients, wording and exact source attachments.",
    }
    service.approve(tid, draft_id, decision)
    return decision


def test_draft_export_works_without_account_and_preserves_exact_attachment(setup):
    repo, tid, artifact, service, module, passwords = setup
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
    assert service.preview(tid, draft["id"])["fingerprint"] == preview["fingerprint"]


def test_editing_or_sender_changes_revoke_exact_content_approval(setup, monkeypatch):
    repo, tid, artifact, service, module, passwords = setup
    configure(service)
    draft = service.create_draft(tid, draft_data())
    old = approve(service, tid, draft["id"])
    service.edit_draft(tid, draft["id"], draft_data(body="Revised delivery condition."))
    with pytest.raises(ValueError, match="approv|changed"):
        service.send(tid, draft["id"], old)
    current = approve(service, tid, draft["id"])
    configure(service, from_address="other@example.com")
    with pytest.raises(ValueError, match="approv|changed"):
        service.send(tid, draft["id"], current)


def test_foreign_attachments_and_header_injection_are_rejected(setup):
    repo, tid, artifact, service, module, passwords = setup
    other = repo.create_tender("Other tender")["id"]
    with pytest.raises((KeyError, ValueError)):
        service.create_draft(other, draft_data(artifact))
    for changes in [
        {"subject": "Hello\r\nBcc: stolen@example.com"},
        {"to": ["sales@example.com\nBcc: stolen@example.com"]},
    ]:
        with pytest.raises(ValueError):
            service.create_draft(tid, draft_data(**changes))


def test_passwords_stay_in_keyring_and_insecure_transport_is_rejected(setup):
    repo, tid, artifact, service, module, passwords = setup
    result = configure(service)
    assert result["smtp_ready"] is True
    assert "smtp-private-password" not in json.dumps(result)
    with repo.db.connect() as conn:
        rows = conn.execute("SELECT * FROM mail_settings").fetchall()
        assert "smtp-private-password" not in str([tuple(row) for row in rows])
    with pytest.raises(ValueError):
        configure(service, smtp_security="plain")


@pytest.mark.parametrize("security", ["ssl", "starttls"])
def test_send_persists_before_network_and_requires_verified_tls(setup, monkeypatch, security):
    repo, tid, artifact, service, module, passwords = setup
    configure(service, smtp_security=security, smtp_port=465 if security == "ssl" else 587)
    draft = service.create_draft(tid, draft_data(artifact))
    decision = approve(service, tid, draft["id"])

    class MailServer:
        encrypted = security == "ssl"

        def __init__(self, host, port, **kwargs):
            if security == "ssl":
                assert kwargs["context"].verify_mode == ssl.CERT_REQUIRED
                assert kwargs["context"].check_hostname is True

        def ehlo(self):
            return 250, b"OK"

        def starttls(self, *, context):
            assert context.verify_mode == ssl.CERT_REQUIRED and context.check_hostname
            self.encrypted = True

        def login(self, username, password):
            assert self.encrypted
            assert password == "smtp-private-password"

        def sendmail(self, sender, recipients, raw):
            stored = service.get(tid, draft["id"])
            assert stored["status"] == "sending"
            assert stored["message_id"] in raw.decode()
            assert recipients == ["sales@supplier.example"]
            return {}

        def quit(self):
            pass

    monkeypatch.setattr(module.smtplib, "SMTP_SSL" if security == "ssl" else "SMTP", MailServer)
    sent = service.send(tid, draft["id"], decision)
    assert sent["status"] == "sent"
    assert "accepted" in sent["delivery_detail"].lower()
    with pytest.raises(ValueError):
        service.send(tid, draft["id"], decision)


@pytest.mark.parametrize("outcome", ["uncertain", "partial", "auth_failed"])
def test_ambiguous_send_blocks_retry_and_partial_delivery_is_explicit(setup, monkeypatch, outcome):
    repo, tid, artifact, service, module, passwords = setup
    configure(service)
    draft = service.create_draft(
        tid, draft_data(to=["sales@supplier.example", "second@supplier.example"])
    )
    decision = approve(service, tid, draft["id"])

    class MailServer:
        def __init__(self, *args, **kwargs):
            pass

        def login(self, *args):
            if outcome == "auth_failed":
                raise smtplib.SMTPAuthenticationError(535, b"Refused")

        def sendmail(self, *args):
            if outcome == "uncertain":
                raise TimeoutError("After DATA was submitted")
            return {"second@supplier.example": (550, b"Rejected")}

        def quit(self):
            pass

    monkeypatch.setattr(module.smtplib, "SMTP_SSL", MailServer)
    result = service.send(tid, draft["id"], decision)
    assert (
        result["status"]
        == {"uncertain": "uncertain", "partial": "partially_sent", "auth_failed": "failed"}[outcome]
    )
    if outcome != "auth_failed":
        with pytest.raises(ValueError):
            service.send(tid, draft["id"], decision)
    if outcome == "partial":
        assert result["refused_recipients"][0]["address"] == "second@supplier.example"


def test_manual_reply_keeps_original_text_headers_and_scoped_sources(setup):
    repo, tid, artifact, service, module, passwords = setup
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
    assert reply["origin"] == "manual"
    assert reply["text"].startswith("Concrete quotation")
    assert reply["supporting_source_ids"] == [source_id]
    assert repo.get_evidence(tid, reply["source_ids"][0])["text"].startswith("Concrete quotation")
    assert len(service.replies(tid, draft["id"])) == 1


@pytest.mark.parametrize("oversized", [False, True])
def test_imap_sync_is_read_only_exactly_matches_ids_and_deduplicates_uids(
    setup, monkeypatch, oversized
):
    from email.message import EmailMessage

    repo, tid, artifact, service, module, passwords = setup
    configure(
        service,
        imap_host="imap.example.com",
        imap_username="engineer@example.com",
        imap_password="imap-private-password",
    )
    draft = service.create_draft(tid, draft_data())
    message = EmailMessage(policy=policy.SMTP)
    message["From"] = "sales@supplier.example"
    message["Subject"] = "Quotation returned"
    message["Date"] = "Sun, 06 Sep 2026 12:00:00 +0000"
    message["Message-ID"] = "<supplier-reply@example.com>"
    message["In-Reply-To"] = draft["message_id"]
    message.set_content("Concrete quote: EGP 120 per cubic metre, excluding VAT.")
    message.add_attachment(
        b"supplier PDF bytes", maintype="application", subtype="pdf", filename="quote.pdf"
    )
    raw = message.as_bytes()
    headers = raw.split(b"\r\n\r\n", 1)[0] + b"\r\n\r\n"

    class Mailbox:
        def __init__(self, host, port, *, ssl_context, timeout):
            assert ssl_context.verify_mode == ssl.CERT_REQUIRED and ssl_context.check_hostname

        def login(self, username, password):
            assert password == "imap-private-password"

        def read(self, size):
            return b""

        def select(self, mailbox, readonly=False):
            assert readonly is True
            return "OK", [b"1"]

        def response(self, name):
            assert name == "UIDVALIDITY"
            return "UIDVALIDITY", [b"77"]

        def uid(self, command, *args):
            if command == "search":
                return "OK", [b"1"]
            assert "BODY.PEEK" in args[1]
            if "HEADER.FIELDS" in args[1]:
                size = module.MAX_REPLY_BYTES + 1 if oversized else len(raw)
                return "OK", [(f"1 (RFC822.SIZE {size}".encode(), headers), b")"]
            assert not oversized
            assert "<0." in args[1]
            return "OK", [(b"1 (BODY[]", raw), b")"]

        def logout(self):
            return "BYE", []

    monkeypatch.setattr(module.imaplib, "IMAP4_SSL", Mailbox)
    first = service.sync_replies(30)
    assert first["matched"] == (0 if oversized else 1)
    if oversized:
        assert first["warnings"]
        assert service.replies(tid, draft["id"]) == []
        return
    assert service.sync_replies(30)["matched"] == 0
    replies = service.replies(tid, draft["id"])
    assert len(replies) == 1
    assert "120" in replies[0]["text"]
    assert any("attachment" in warning.lower() for warning in replies[0]["warnings"])
    evidence = repo.get_evidence(tid, replies[0]["source_ids"][0])
    assert "120" in evidence["text"]
    assert repo.object_path(tid, evidence["artifact_id"]).read_bytes() == raw
    assert "In-Reply-To:" in replies[0]["headers"]


def test_restart_marks_inflight_mail_uncertain_without_retrying(setup):
    repo, tid, artifact, service, module, passwords = setup
    draft = service.create_draft(tid, draft_data())
    with repo.db.connect(write=True) as conn:
        conn.execute("UPDATE quote_drafts SET status='sending' WHERE id=?", (draft["id"],))
    service.recover_interrupted_sends()
    assert service.get(tid, draft["id"])["status"] == "uncertain"


def test_reply_matching_never_uses_subject_or_substring_identity(setup):
    from email.message import EmailMessage

    repo, tid, artifact, service, module, passwords = setup
    quote = service.create_draft(tid, draft_data())
    message = EmailMessage()
    message["Subject"] = quote["subject"]
    message["In-Reply-To"] = "<prefix" + quote["message_id"][1:]
    assert module._matched_quote(message, {quote["message_id"]: quote}) is None


def test_routes_keep_password_write_only_and_export_real_draft(setup):
    from fastapi import FastAPI
    from fastapi.testclient import TestClient

    repo, tid, artifact, service, module, passwords = setup
    routes = importlib.import_module("quantix.correspondence_routes")
    app = FastAPI()
    app.include_router(routes.create_router(repo))
    client = TestClient(app)
    response = client.patch("/api/mail/settings", json={"smtp_password": "write-only-password"})
    assert response.status_code == 200
    assert "write-only-password" not in response.text
    draft = client.post(f"/api/tenders/{tid}/quotes", json=draft_data(artifact)).json()
    response = client.get(f"/api/tenders/{tid}/quotes/{draft['id']}/eml")
    assert response.status_code == 200
    assert b"Message-ID:" in response.content
    assert client.post(f"/api/tenders/{tid}/quotes/{draft['id']}/send", json={}).status_code == 422
    assert "QuotePreview" in app.openapi()["components"]["schemas"]


def test_changed_attachment_bytes_cannot_use_existing_approval(setup):
    repo, tid, artifact, service, module, passwords = setup
    configure(service)
    draft = service.create_draft(tid, draft_data(artifact))
    decision = approve(service, tid, draft["id"])
    repo.object_path(tid, artifact["id"]).write_bytes(b"changed")
    with pytest.raises(ValueError, match="attachment|integrity"):
        service.send(tid, draft["id"], decision)


def test_long_manual_reply_becomes_bounded_citable_tender_segments(setup):
    repo, tid, artifact, service, module, passwords = setup
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


def test_sent_preview_keeps_original_sender_after_account_changes(setup, monkeypatch):
    repo, tid, artifact, service, module, passwords = setup
    configure(service)
    draft = service.create_draft(tid, draft_data())
    decision = approve(service, tid, draft["id"])

    class Server:
        def __init__(self, *args, **kwargs):
            pass

        def login(self, *args):
            pass

        def sendmail(self, *args):
            return {}

        def quit(self):
            pass

    monkeypatch.setattr(module.smtplib, "SMTP_SSL", Server)
    service.send(tid, draft["id"], decision)
    configure(service, from_address="changed@example.com")
    assert service.preview(tid, draft["id"])["sender"] == "engineer@example.com"


@pytest.mark.parametrize("outcome", ["sent", "uncertain", "crash"])
def test_same_home_backup_rollback_cannot_blindly_resend_a_submitted_quote(
    setup, monkeypatch, outcome
):
    import sqlite3

    from quantix.backup import BackupService, apply_pending_restore

    repo, tid, artifact, service, module, passwords = setup
    configure(service)
    draft = service.create_draft(tid, draft_data())
    decision = approve(service, tid, draft["id"])
    backup = BackupService(repo).create()
    sent = []

    class Server:
        def __init__(self, *args, **kwargs):
            pass

        def login(self, *args):
            pass

        def sendmail(self, *args):
            sent.append(args)
            if outcome == "uncertain":
                raise TimeoutError("Submission acknowledgement was lost")
            return {}

        def quit(self):
            pass

    monkeypatch.setattr(module.smtplib, "SMTP_SSL", Server)
    if outcome == "crash":

        def crash_after_acceptance(*args):
            raise OSError("Simulated interruption before final receipt")

        monkeypatch.setattr(service, "_finish_delivery", crash_after_acceptance)
        with pytest.raises(OSError):
            service.send(tid, draft["id"], decision)
    else:
        assert service.send(tid, draft["id"], decision)["status"] == outcome
    BackupService(repo).stage_restore(
        repo.home / "backups" / backup["filename"],
        True,
        "Restore earlier workspace",
        expected_sha256=backup["sha256"],
    )
    apply_pending_restore(repo.home)
    restored = module.QuoteService(Repository(repo.home))
    assert restored.get(tid, draft["id"])["delivery_history_status"] == (
        "attempting" if outcome == "crash" else outcome
    )
    with pytest.raises(ValueError, match="receipt|submission|already|delivery"):
        restored.send(tid, draft["id"], decision)
    assert len(sent) == 1
    with sqlite3.connect(repo.home / "mail-delivery.sqlite") as conn:
        with pytest.raises(sqlite3.IntegrityError):
            conn.execute("DELETE FROM delivery_receipts")


def test_cross_home_restore_requires_fresh_delivery_reconciliation(setup, monkeypatch, tmp_path):
    from quantix.backup import BackupService, apply_pending_restore

    repo, tid, artifact, service, module, passwords = setup
    configure(service)
    draft = service.create_draft(tid, draft_data())
    decision = approve(service, tid, draft["id"])
    backup = BackupService(repo).create()
    another = Repository(tmp_path / "another-workspace")
    BackupService(another).stage_restore(
        repo.home / "backups" / backup["filename"],
        True,
        "Recover on this computer",
        expected_sha256=backup["sha256"],
    )
    apply_pending_restore(another.home)
    restored = module.QuoteService(Repository(another.home))
    configure(restored)
    with pytest.raises(ValueError, match="reconcil"):
        restored.send(tid, draft["id"], decision)

    class Server:
        def __init__(self, *args, **kwargs):
            pass

        def login(self, *args):
            pass

        def sendmail(self, *args):
            return {}

        def quit(self):
            pass

    monkeypatch.setattr(module.smtplib, "SMTP_SSL", Server)
    result = restored.send(
        tid,
        draft["id"],
        decision
        | {
            "restore_reconciliation": "Checked the sent mailbox and supplier; this request was not submitted."
        },
    )
    assert result["status"] == "sent"
    # Local creation order, not an imported database's clock, establishes that a
    # new request was created after this restore.
    monkeypatch.setattr(module, "now", lambda: "2000-01-01T00:00:00+00:00")
    fresh = restored.create_draft(tid, draft_data(subject="New request after restore"))
    fresh_decision = approve(restored, tid, fresh["id"])
    assert restored.send(tid, fresh["id"], fresh_decision)["status"] == "sent"
