"""Real TLS mail clients behind exact-content engineer approval boundaries."""

import hashlib
import imaplib
import mimetypes
import os
import re
import smtplib
import sqlite3
import ssl
import threading
from contextlib import contextmanager, suppress
from datetime import datetime
from email import policy
from email.message import EmailMessage
from email.parser import BytesParser
from email.utils import format_datetime, make_msgid
from html.parser import HTMLParser

import keyring

from .correspondence_models import (
    DraftInput,
    MailAccount,
    MailSettingsPatch,
    ManualReply,
    SendDecision,
    SyncRequest,
)
from .db import dump, new_id, now, record

MAX_ATTACHMENTS = 20 * 1024**2
MAX_REPLY_BYTES = 2 * 1024**2
_SYNC_LOCK = threading.Lock()
SCHEMA = (
    "CREATE TABLE IF NOT EXISTS mail_settings(id INTEGER PRIMARY KEY CHECK(id=1),data_json TEXT NOT NULL)",
    "CREATE TABLE IF NOT EXISTS quote_drafts(id TEXT PRIMARY KEY,tender_id TEXT NOT NULL REFERENCES tenders(id),message_id TEXT NOT NULL UNIQUE,status TEXT NOT NULL,data_json TEXT NOT NULL,approved_fingerprint TEXT,delivery_detail TEXT NOT NULL DEFAULT '',refused_recipients_json TEXT NOT NULL DEFAULT '[]',outbound_eml BLOB,created_at TEXT NOT NULL,updated_at TEXT NOT NULL)",
    "CREATE TABLE IF NOT EXISTS quote_replies(id TEXT PRIMARY KEY,tender_id TEXT NOT NULL REFERENCES tenders(id),quote_id TEXT NOT NULL REFERENCES quote_drafts(id),mailbox_key TEXT,uidvalidity TEXT,uid INTEGER,data_json TEXT NOT NULL,raw_message BLOB,created_at TEXT NOT NULL,UNIQUE(mailbox_key,uidvalidity,uid))",
    "CREATE TABLE IF NOT EXISTS mail_sync_state(mailbox_key TEXT PRIMARY KEY,uidvalidity TEXT NOT NULL,last_uid INTEGER NOT NULL,result_json TEXT NOT NULL)",
)


def _tls():
    context = ssl.create_default_context()
    context.minimum_version = ssl.TLSVersion.TLSv1_2
    return context


class _HTMLText(HTMLParser):
    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.parts, self.hidden = [], 0

    def handle_starttag(self, tag, attrs):
        if tag in {"script", "style"}:
            self.hidden += 1
        elif tag in {"p", "br", "div", "tr"}:
            self.parts.append("\n")

    def handle_endtag(self, tag):
        if tag in {"script", "style"}:
            self.hidden = max(0, self.hidden - 1)

    def handle_data(self, data):
        if not self.hidden:
            self.parts.append(data)


def _literal(data):
    values = [
        part[1]
        for part in data
        if isinstance(part, tuple) and len(part) == 2 and isinstance(part[1], bytes)
    ]
    if len(values) != 1:
        raise ValueError("IMAP did not return one complete message section.")
    return values[0]


def _matched_quote(message, known):
    for header in ("In-Reply-To", "References"):
        identifiers = re.findall(
            r"<[^<>\s]+>", " ".join(str(value) for value in message.get_all(header, []))
        )
        matches = {identifier for identifier in identifiers if identifier in known}
        if matches:
            return known[next(iter(matches))] if len(matches) == 1 else None
    return None


def _reply_data(raw):
    message = BytesParser(policy=policy.default).parsebytes(raw)
    warnings = []
    body = message.get_body(preferencelist=("plain", "html"))
    text = ""
    if body:
        try:
            text = body.get_content()
        except (LookupError, UnicodeError):
            text = (body.get_payload(decode=True) or b"").decode("utf-8", errors="replace")
            warnings.append("Some reply text required fallback character decoding.")
        if body.get_content_type() == "text/html":
            parser = _HTMLText()
            parser.feed(text)
            text = "".join(parser.parts)
            warnings.append(
                "HTML reply text was converted; inspect the original message for formatting."
            )
    for part in message.walk():
        if part.is_attachment() or part.get_filename():
            warnings.append(
                "Reply attachment retained in the original EML but not imported for analysis: "
                + str(part.get_filename() or part.get_content_type())[:300]
            )
    header_bytes = re.split(rb"\r?\n\r?\n", raw, maxsplit=1)[0]
    if not text.strip():
        warnings.append("No readable reply body was found; inspect the original message.")
        text = "Message headers\n" + header_bytes.decode("utf-8", errors="replace")
    return {
        "origin": "imap",
        "sender": str(message.get("From", "")),
        "received_at": now(),
        "date_header": str(message["Date"]) if message["Date"] else None,
        "date_basis": "retrieved_at",
        "subject": str(message.get("Subject", "")),
        "text": text,
        "headers": header_bytes.decode("utf-8", errors="replace"),
        "message_id": str(message["Message-ID"]) if message["Message-ID"] else None,
        "supporting_source_ids": [],
        "warnings": warnings,
    }


class QuoteService:
    def __init__(self, repo):
        self.repo = repo
        self.keyring_service = (
            "Quantix-mail-" + hashlib.sha256(str(repo.home).encode()).hexdigest()[:16]
        )
        with repo.db.connect(write=True) as conn:
            for statement in SCHEMA:
                conn.execute(statement)
        with self._receipts(write=True) as conn:
            conn.execute(
                "CREATE TABLE IF NOT EXISTS delivery_receipts(id TEXT PRIMARY KEY,message_id TEXT NOT NULL,fingerprint TEXT NOT NULL,sender TEXT NOT NULL,phase TEXT NOT NULL,detail TEXT NOT NULL,reconciliation TEXT,created_at TEXT NOT NULL)"
            )
            conn.execute(
                "CREATE INDEX IF NOT EXISTS delivery_receipts_message ON delivery_receipts(message_id)"
            )
            conn.execute(
                "CREATE TRIGGER IF NOT EXISTS delivery_receipts_immutable_update BEFORE UPDATE ON delivery_receipts BEGIN SELECT RAISE(ABORT,'Delivery receipts are immutable'); END"
            )
            conn.execute(
                "CREATE TRIGGER IF NOT EXISTS delivery_receipts_immutable_delete BEFORE DELETE ON delivery_receipts BEGIN SELECT RAISE(ABORT,'Delivery receipts are immutable'); END"
            )

    @contextmanager
    def _receipts(self, write=False):
        path = self.repo.home / "mail-delivery.sqlite"
        for candidate in (
            path,
            path.with_name(path.name + "-wal"),
            path.with_name(path.name + "-shm"),
        ):
            if candidate.is_symlink() or not candidate.resolve().is_relative_to(
                self.repo.home.resolve()
            ):
                raise ValueError("The delivery journal must remain inside this workspace.")
        conn = sqlite3.connect(path, timeout=30)
        conn.row_factory = sqlite3.Row
        try:
            conn.execute("PRAGMA journal_mode=WAL")
            conn.execute("PRAGMA synchronous=FULL")
            if write:
                conn.execute("BEGIN IMMEDIATE")
            yield conn
            conn.commit()
        except BaseException:
            conn.rollback()
            raise
        finally:
            conn.close()

    def _restore_marker(self):
        history = self.repo.home / "restore-history"
        if (
            history.is_symlink()
            or history.is_junction()
            or not history.resolve().is_relative_to(self.repo.home.resolve())
        ):
            raise ValueError("Delivery reconciliation could not verify the local restore history.")
        return dump(
            sorted(
                path.stem
                for path in history.glob("*.json")
                if re.fullmatch(r"[a-f0-9]{32}\.json", path.name)
            )
        )

    def _requires_restore_reconciliation(self, quote):
        marker = self._restore_marker()
        if marker == "[]":
            return False
        with self._receipts() as conn:
            created = conn.execute(
                "SELECT detail FROM delivery_receipts WHERE message_id=? AND phase='draft_created' ORDER BY rowid LIMIT 1",
                (quote["message_id"],),
            ).fetchone()
        return created is None or created[0] != marker

    def _reserve_delivery(self, quote, sender, decision):
        with self._receipts(write=True) as conn:
            previous = conn.execute(
                "SELECT phase FROM delivery_receipts WHERE message_id=? ORDER BY rowid DESC LIMIT 1",
                (quote["message_id"],),
            ).fetchone()
            if previous and previous[0] in {"attempting", "sent", "partially_sent", "uncertain"}:
                raise ValueError(
                    "The local delivery receipt records a previous or uncertain submission. This request cannot be blindly resent after rollback."
                )
            marker = self._restore_marker()
            created = conn.execute(
                "SELECT detail FROM delivery_receipts WHERE message_id=? AND phase='draft_created' ORDER BY rowid LIMIT 1",
                (quote["message_id"],),
            ).fetchone()
            if (
                marker != "[]"
                and (created is None or created[0] != marker)
                and not (decision.restore_reconciliation or "").strip()
            ):
                raise ValueError(
                    "This draft predates a workspace restore. Record a fresh delivery reconciliation after checking the sent mailbox or supplier before sending."
                )
            conn.execute(
                "INSERT INTO delivery_receipts VALUES(?,?,?,?,?,?,?,?)",
                (
                    new_id(),
                    quote["message_id"],
                    decision.fingerprint,
                    sender,
                    "attempting",
                    "Submission reserved before network access.",
                    decision.restore_reconciliation,
                    now(),
                ),
            )

    def _finish_delivery(self, message_id, fingerprint, sender, status, detail):
        with self._receipts(write=True) as conn:
            conn.execute(
                "INSERT INTO delivery_receipts VALUES(?,?,?,?,?,?,NULL,?)",
                (new_id(), message_id, fingerprint, sender, status, detail, now()),
            )

    def _account(self):
        with self.repo.db.connect() as conn:
            row = conn.execute("SELECT data_json FROM mail_settings WHERE id=1").fetchone()
        return MailAccount.model_validate_json(row[0]) if row else MailAccount()

    def _password(self, protocol):
        try:
            return keyring.get_password(self.keyring_service, protocol)
        except keyring.errors.KeyringError as exc:
            raise ValueError("Windows could not read the mail account credential.") from exc

    def settings(self):
        account = self._account()
        return {
            **account.model_dump(),
            "smtp_ready": bool(
                account.smtp_host
                and account.smtp_username
                and account.from_address
                and self._password("smtp")
            ),
            "imap_ready": bool(
                account.imap_host and account.imap_username and self._password("imap")
            ),
            "detail": "Saved account details are not a verified connection. Only SSL or STARTTLS is supported.",
        }

    def update_settings(self, values):
        patch = MailSettingsPatch.model_validate(values)
        account = MailAccount.model_validate(
            self._account().model_dump()
            | patch.model_dump(exclude_unset=True, exclude={"smtp_password", "imap_password"})
        )
        for protocol in ("smtp", "imap"):
            secret = getattr(patch, protocol + "_password")
            if secret is not None:
                password = secret.get_secret_value()
                if len(password) > 4000:
                    raise ValueError("The mail account password is too long.")
                try:
                    if password:
                        keyring.set_password(self.keyring_service, protocol, password)
                    else:
                        with suppress(keyring.errors.PasswordDeleteError):
                            keyring.delete_password(self.keyring_service, protocol)
                except keyring.errors.KeyringError as exc:
                    raise ValueError(
                        "Windows could not securely save the mail account password."
                    ) from exc
        with self.repo.db.connect(write=True) as conn:
            conn.execute(
                "INSERT INTO mail_settings VALUES(1,?) ON CONFLICT(id) DO UPDATE SET data_json=excluded.data_json",
                (dump(account.model_dump()),),
            )
        return self.settings()

    def _attachments(self, tender_id, identifiers):
        attachments, total = [], 0
        for identifier in dict.fromkeys(identifiers):
            artifact = self.repo.get_artifact(tender_id, identifier)
            path = self.repo.object_path(tender_id, identifier)
            if not path.resolve().is_relative_to(self.repo.home.resolve()) or path.is_symlink():
                raise ValueError("An attachment must be a saved original inside this workspace.")
            total += path.stat().st_size
            if total > MAX_ATTACHMENTS:
                raise ValueError("Attachments exceed the 20 MiB message limit.")
            data = path.read_bytes()
            if (
                len(data) != artifact["size"]
                or hashlib.sha256(data).hexdigest() != artifact["content_hash"]
            ):
                raise ValueError("An attachment changed or failed its integrity check.")
            info = {
                "artifact_id": identifier,
                "filename": re.sub(r"[\r\n\x00]", "_", artifact["name"]),
                "content_hash": artifact["content_hash"],
                "size": len(data),
                "version": artifact["version"],
                "is_current": artifact["is_current"],
            }
            attachments.append((info, data))
        return attachments

    def create_draft(self, tender_id, values):
        request = DraftInput.model_validate(values)
        self.repo.get_tender(tender_id)
        for source_id in request.source_ids:
            self.repo.get_evidence(tender_id, source_id)
        attachments = self._attachments(tender_id, request.attachment_ids)
        identifier, stamp = new_id(), now()
        message_id = make_msgid(domain="quantix.local")
        data = request.model_dump() | {"attachments": [item[0] for item in attachments]}
        with self.repo.db.connect(write=True) as conn:
            conn.execute(
                "INSERT INTO quote_drafts(id,tender_id,message_id,status,data_json,created_at,updated_at) VALUES(?,?,?,'draft',?,?,?)",
                (
                    identifier,
                    tender_id,
                    message_id,
                    dump(data),
                    stamp,
                    stamp,
                ),
            )
        with self._receipts(write=True) as conn:
            conn.execute(
                "INSERT INTO delivery_receipts VALUES(?,?,?,'','draft_created',?,NULL,?)",
                (new_id(), message_id, "", self._restore_marker(), stamp),
            )
        return self.get(tender_id, identifier)

    def get(self, tender_id, quote_id):
        with self.repo.db.connect() as conn:
            row = record(
                conn.execute(
                    "SELECT * FROM quote_drafts WHERE id=? AND tender_id=?", (quote_id, tender_id)
                ).fetchone()
            )
        row.pop("outbound_eml")
        with self._receipts() as conn:
            receipt = conn.execute(
                "SELECT phase,fingerprint FROM delivery_receipts WHERE message_id=? AND phase<>'draft_created' ORDER BY rowid DESC LIMIT 1",
                (row["message_id"],),
            ).fetchone()
        return (
            {key: value for key, value in row.items() if key != "data"}
            | row["data"]
            | {
                "delivery_history_status": receipt["phase"] if receipt else None,
                "delivery_history_fingerprint": receipt["fingerprint"] if receipt else None,
            }
        )

    def list_drafts(self, tender_id):
        self.repo.get_tender(tender_id)
        with self.repo.db.connect() as conn:
            identifiers = [
                row[0]
                for row in conn.execute(
                    "SELECT id FROM quote_drafts WHERE tender_id=? ORDER BY created_at DESC",
                    (tender_id,),
                )
            ]
        return [self.get(tender_id, identifier) for identifier in identifiers]

    def edit_draft(self, tender_id, quote_id, values):
        request = DraftInput.model_validate(values)
        for source_id in request.source_ids:
            self.repo.get_evidence(tender_id, source_id)
        attachments = self._attachments(tender_id, request.attachment_ids)
        data = request.model_dump() | {"attachments": [item[0] for item in attachments]}
        with self.repo.db.connect(write=True) as conn:
            quote = self.get(tender_id, quote_id)
            if quote["status"] not in {"draft", "approved", "failed"}:
                raise ValueError(
                    "A sent or uncertain message cannot be edited or blindly resent. Create a new draft if needed."
                )
            conn.execute(
                "UPDATE quote_drafts SET data_json=?,status='draft',approved_fingerprint=NULL,outbound_eml=NULL,delivery_detail='',refused_recipients_json='[]',updated_at=? WHERE id=?",
                (dump(data), now(), quote_id),
            )
        return self.get(tender_id, quote_id)

    def _prepare(self, tender_id, quote_id):
        quote, account = self.get(tender_id, quote_id), self._account()
        attachments = self._attachments(tender_id, quote["attachment_ids"])
        canonical = {
            key: quote[key]
            for key in ("to", "cc", "subject", "body", "source_ids", "message_id", "created_at")
        }
        canonical["attachments"] = [item[0] for item in attachments]
        canonical["account"] = {
            key: value
            for key, value in account.model_dump().items()
            if key.startswith("smtp_") or key == "from_address"
        }
        fingerprint = hashlib.sha256(dump(canonical).encode()).hexdigest()
        message = EmailMessage(policy=policy.SMTP)
        if account.from_address:
            message["From"] = account.from_address
        message["To"] = ", ".join(quote["to"])
        if quote["cc"]:
            message["Cc"] = ", ".join(quote["cc"])
        message["Subject"], message["Message-ID"] = quote["subject"], quote["message_id"]
        message["Date"] = format_datetime(datetime.fromisoformat(quote["created_at"]))
        message.set_content(quote["body"])
        for info, data in attachments:
            mime = mimetypes.guess_type(info["filename"])[0] or "application/octet-stream"
            major, minor = mime.split("/", 1)
            message.add_attachment(data, maintype=major, subtype=minor, filename=info["filename"])
        if attachments:
            message.set_boundary("quantix-" + fingerprint)
        preview = {
            "quote": quote,
            "sender": account.from_address or None,
            "attachments": canonical["attachments"],
            "fingerprint": fingerprint,
            "smtp_host": account.smtp_host,
            "smtp_ready": self.settings()["smtp_ready"],
            "warnings": ["An attachment is an older source revision."]
            if any(not item[0]["is_current"] for item in attachments)
            else [],
        }
        return preview, message.as_bytes(), account

    def preview(self, tender_id, quote_id):
        quote = self.get(tender_id, quote_id)
        if quote["status"] in {"sending", "sent", "partially_sent", "uncertain"}:
            original = BytesParser(policy=policy.default).parsebytes(self.eml(tender_id, quote_id))
            return {
                "quote": quote,
                "sender": str(original["From"]) if original["From"] else None,
                "attachments": quote["attachments"],
                "fingerprint": quote["approved_fingerprint"] or "",
                "smtp_host": quote.get("submission_host") or "",
                "smtp_ready": False,
                "warnings": ["Saved submission content. This message cannot be blindly resent."],
            }
        preview = self._prepare(tender_id, quote_id)[0]
        preview["restore_reconciliation_required"] = self._requires_restore_reconciliation(quote)
        return preview

    def eml(self, tender_id, quote_id):
        with self.repo.db.connect() as conn:
            row = conn.execute(
                "SELECT outbound_eml FROM quote_drafts WHERE id=? AND tender_id=?",
                (quote_id, tender_id),
            ).fetchone()
            if row and row[0]:
                return bytes(row[0])
        return self._prepare(tender_id, quote_id)[1]

    def approve(self, tender_id, quote_id, values):
        decision = SendDecision.model_validate(values)
        with self.repo.atomic():
            preview = self.preview(tender_id, quote_id)
            if (
                preview["quote"]["status"] not in {"draft", "approved", "failed"}
                or decision.fingerprint != preview["fingerprint"]
            ):
                raise ValueError("The message changed or cannot be approved in its current state.")
            with self.repo.db.connect(write=True) as conn:
                conn.execute(
                    "UPDATE quote_drafts SET status='approved',approved_fingerprint=?,updated_at=? WHERE id=?",
                    (decision.fingerprint, now(), quote_id),
                )
                conn.execute(
                    "INSERT INTO decisions VALUES(?,?,?,?,?,?,?)",
                    (
                        new_id(),
                        tender_id,
                        "quote",
                        quote_id,
                        "approve_send",
                        decision.rationale,
                        now(),
                    ),
                )
        return self.get(tender_id, quote_id)

    def send(self, tender_id, quote_id, values):
        decision = SendDecision.model_validate(values)
        with self.repo.atomic():
            preview, raw, account = self._prepare(tender_id, quote_id)
            quote = preview["quote"]
            if (
                quote["status"] not in {"approved", "failed"}
                or quote["approved_fingerprint"] != preview["fingerprint"]
                or decision.fingerprint != preview["fingerprint"]
            ):
                raise ValueError(
                    "The current recipients, message and attachments need engineer approval. Sent or uncertain messages cannot be retried."
                )
            password = self._password("smtp")
            if not preview["smtp_ready"] or not password:
                raise ValueError("Configure the SMTP account before sending this approved request.")
            self._reserve_delivery(quote, account.from_address, decision)
            with self.repo.db.connect(write=True) as conn:
                content = {
                    key: quote[key]
                    for key in ("to", "cc", "subject", "body", "attachment_ids", "source_ids")
                }
                content.update(
                    attachments=preview["attachments"], submission_host=account.smtp_host
                )
                conn.execute(
                    "UPDATE quote_drafts SET data_json=? WHERE id=?", (dump(content), quote_id)
                )
                conn.execute(
                    "UPDATE quote_drafts SET status='sending',outbound_eml=?,delivery_detail='Submission started; acceptance is not yet known.',updated_at=? WHERE id=?",
                    (raw, now(), quote_id),
                )
                conn.execute(
                    "INSERT INTO decisions VALUES(?,?,?,?,?,?,?)",
                    (new_id(), tender_id, "quote", quote_id, "send", decision.rationale, now()),
                )
        client, started, refused = None, False, {}
        try:
            context = _tls()
            if account.smtp_security == "ssl":
                client = smtplib.SMTP_SSL(
                    account.smtp_host, account.smtp_port, context=context, timeout=30
                )
            else:
                client = smtplib.SMTP(account.smtp_host, account.smtp_port, timeout=30)
                client.ehlo()
                client.starttls(context=context)
                client.ehlo()
            client.login(account.smtp_username, password)
            started = True
            refused = client.sendmail(
                account.from_address, list(dict.fromkeys(quote["to"] + quote["cc"])), raw
            )
            status = "partially_sent" if refused else "sent"
            detail = (
                "SMTP accepted the message for some recipients; others were refused."
                if refused
                else "SMTP accepted the message for all recipients. Inbox delivery is not confirmed."
            )
        except smtplib.SMTPRecipientsRefused as exc:
            refused, status, detail = (
                exc.recipients,
                "failed",
                "SMTP refused all recipients; no message was accepted.",
            )
        except (smtplib.SMTPSenderRefused, smtplib.SMTPDataError):
            status, detail = (
                "failed",
                "SMTP rejected the sender or message data; no acceptance was reported.",
            )
        except (OSError, smtplib.SMTPException):
            status = "uncertain" if started else "failed"
            detail = (
                "SMTP acceptance is uncertain. Check the sent mailbox or contact the supplier before creating another request."
                if started
                else "SMTP connection, TLS or authentication failed before message submission."
            )
        finally:
            if client:
                with suppress(OSError, smtplib.SMTPException):
                    client.quit()
        refusal_records = [
            {"address": recipient, "code": int(value[0])} for recipient, value in refused.items()
        ]
        self._finish_delivery(
            quote["message_id"], decision.fingerprint, account.from_address, status, detail
        )
        with self.repo.db.connect(write=True) as conn:
            conn.execute(
                "UPDATE quote_drafts SET status=?,delivery_detail=?,refused_recipients_json=?,updated_at=? WHERE id=?",
                (status, detail, dump(refusal_records), now(), quote_id),
            )
        return self.get(tender_id, quote_id)

    def recover_interrupted_sends(self):
        with self.repo.db.connect(write=True) as conn:
            conn.execute(
                "UPDATE quote_drafts SET status='uncertain',delivery_detail='Quantix closed during submission. Check acceptance before creating another request.',updated_at=? WHERE status='sending'",
                (now(),),
            )

    def _save_reply(
        self, tender_id, quote_id, data, raw, mailbox_key=None, uidvalidity=None, uid=None
    ):
        self.get(tender_id, quote_id)
        for source_id in data["supporting_source_ids"]:
            self.repo.get_evidence(tender_id, source_id)
        digest = hashlib.sha256(raw).hexdigest()
        target = self.repo.objects / digest
        if (
            not target.resolve().is_relative_to(self.repo.home.resolve())
            or self.repo.objects.is_symlink()
            or self.repo.objects.is_junction()
        ):
            raise ValueError("Reply originals must stay inside this workspace.")
        if not target.exists():
            temporary = self.repo.objects / (new_id() + ".partial")
            with temporary.open("xb") as handle:
                handle.write(raw)
                handle.flush()
                os.fsync(handle.fileno())
            os.replace(temporary, target)
        elif hashlib.sha256(target.read_bytes()).hexdigest() != digest:
            raise ValueError("A saved reply original failed its integrity check.")
        identifier, stamp = new_id(), now()
        with self.repo.atomic():
            with self.repo.db.connect() as conn:
                if (
                    mailbox_key
                    and conn.execute(
                        "SELECT 1 FROM quote_replies WHERE mailbox_key=? AND uidvalidity=? AND uid=?",
                        (mailbox_key, uidvalidity, uid),
                    ).fetchone()
                ):
                    return None
            suffix = "eml" if data["origin"] == "imap" else "json"
            text = data["text"]
            extraction = {
                "kind": "other",
                "status": "needs_attention" if data["warnings"] else "extracted",
                "warnings": [
                    {"code": "reply_attachment_review", "message": warning}
                    for warning in data["warnings"]
                ],
                "metadata": {
                    "origin": data["origin"],
                    "sender": data["sender"],
                    "received_at": data["received_at"],
                    "date_basis": data["date_basis"],
                    "date_header": data["date_header"],
                    "message_id": data["message_id"],
                },
                "segments": [
                    {
                        "locator": f"reply:{identifier}/characters:{offset + 1}-{min(offset + 6000, len(text))}",
                        "text": text[offset : offset + 6000],
                        "kind": "correspondence",
                        "metadata": {
                            "sender": data["sender"],
                            "received_at": data["received_at"],
                            "date_basis": data["date_basis"],
                            "date_header": data["date_header"],
                            "quote_id": quote_id,
                            "message_id": data["message_id"],
                        },
                    }
                    for offset in range(0, len(text), 6000)
                ],
            }
            artifact, _ = self.repo.register_artifact(
                tender_id,
                f"Correspondence/{quote_id}/reply-{identifier}.{suffix}",
                digest,
                len(raw),
                extraction,
            )
            data["source_ids"] = []
            offset = 0
            while sources := self.repo.artifact_evidence(tender_id, artifact["id"], offset, 200):
                data["source_ids"].extend(source["id"] for source in sources)
                offset += len(sources)
            with self.repo.db.connect(write=True) as conn:
                conn.execute(
                    "INSERT INTO quote_replies VALUES(?,?,?,?,?,?,?,?,?)",
                    (
                        identifier,
                        tender_id,
                        quote_id,
                        mailbox_key,
                        uidvalidity,
                        uid,
                        dump(data),
                        raw,
                        stamp,
                    ),
                )
        return {
            "id": identifier,
            "tender_id": tender_id,
            "quote_id": quote_id,
            "created_at": stamp,
            **data,
        }

    def register_reply(self, tender_id, quote_id, values):
        request = ManualReply.model_validate(values)
        data = request.model_dump(mode="json")
        raw = dump(data).encode()
        data["supporting_source_ids"] = data.pop("source_ids")
        data.update(
            origin="manual",
            message_id=None,
            date_header=None,
            date_basis="engineer_entered",
            warnings=[],
        )
        return self._save_reply(tender_id, quote_id, data, raw)

    def replies(self, tender_id, quote_id):
        self.get(tender_id, quote_id)
        with self.repo.db.connect() as conn:
            rows = [
                record(row)
                for row in conn.execute(
                    "SELECT id,tender_id,quote_id,data_json,created_at FROM quote_replies WHERE quote_id=? AND tender_id=? ORDER BY created_at",
                    (quote_id, tender_id),
                )
            ]
        return [
            {key: value for key, value in row.items() if key != "data"} | row["data"]
            for row in rows
        ]

    def sync_replies(self, max_messages=30):
        limit = SyncRequest(max_messages=max_messages).max_messages
        with _SYNC_LOCK:
            account = self._account()
            password = self._password("imap")
            if not account.imap_host or not account.imap_username or not password:
                raise ValueError("Configure the IMAP SSL account before checking replies.")
            mailbox_key = hashlib.sha256(
                dump(
                    [
                        account.imap_host,
                        account.imap_port,
                        account.imap_username,
                        account.imap_mailbox,
                    ]
                ).encode()
            ).hexdigest()
            result = {
                "checked": 0,
                "matched": 0,
                "skipped": 0,
                "more_available": False,
                "warnings": [],
            }
            client = None
            try:
                client = imaplib.IMAP4_SSL(
                    account.imap_host, account.imap_port, ssl_context=_tls(), timeout=30
                )
                original_read = client.read

                def bounded_read(size):
                    if size < 0 or size > MAX_REPLY_BYTES + 65536:
                        raise ValueError("The mail server returned an oversized message section.")
                    return original_read(size)

                client.read = bounded_read
                client.login(account.imap_username, password)
                mailbox = '"' + account.imap_mailbox.replace("\\", "\\\\").replace('"', '\\"') + '"'
                status, _ = client.select(mailbox, readonly=True)
                if status != "OK":
                    raise ValueError("IMAP could not open the mailbox read-only.")
                _, validity_data = client.response("UIDVALIDITY")
                validity = validity_data[0].decode() if validity_data and validity_data[0] else ""
                if not validity.isdigit():
                    raise ValueError("IMAP did not provide a valid UIDVALIDITY value.")
                with self.repo.db.connect() as conn:
                    state = conn.execute(
                        "SELECT uidvalidity,last_uid FROM mail_sync_state WHERE mailbox_key=?",
                        (mailbox_key,),
                    ).fetchone()
                    last_uid = state[1] if state and state[0] == validity else 0
                    known = {
                        row["message_id"]: {"id": row["id"], "tender_id": row["tender_id"]}
                        for row in conn.execute("SELECT id,tender_id,message_id FROM quote_drafts")
                    }
                status, ids = client.uid("search", None, "UID", f"{last_uid + 1}:*")
                if status != "OK":
                    raise ValueError("IMAP could not search the mailbox.")
                identifiers = sorted(
                    {
                        int(value)
                        for value in (ids[0] or b"").split()
                        if value.isdigit() and int(value) > last_uid
                    }
                )
                result["more_available"] = len(identifiers) > limit
                for uid in identifiers[:limit]:
                    with self.repo.db.connect() as conn:
                        existing = conn.execute(
                            "SELECT 1 FROM quote_replies WHERE mailbox_key=? AND uidvalidity=? AND uid=?",
                            (mailbox_key, validity, uid),
                        ).fetchone()
                    if existing:
                        last_uid = uid
                        continue
                    status, data = client.uid(
                        "fetch",
                        str(uid),
                        "(RFC822.SIZE BODY.PEEK[HEADER.FIELDS (MESSAGE-ID IN-REPLY-TO REFERENCES FROM DATE SUBJECT CONTENT-TYPE)])",
                    )
                    if status != "OK":
                        raise ValueError(
                            "IMAP could not read a message header. Retry the reply check."
                        )
                    headers = _literal(data)
                    if len(headers) > 65536:
                        raise ValueError("A reply header exceeds the supported size.")
                    size_match = re.search(
                        rb"RFC822.SIZE\s+(\d+)",
                        b" ".join(part[0] for part in data if isinstance(part, tuple)),
                        re.I,
                    )
                    result["checked"] += 1
                    quote = _matched_quote(
                        BytesParser(policy=policy.default).parsebytes(headers), known
                    )
                    if quote is None:
                        result["skipped"] += 1
                    elif not size_match or int(size_match[1]) > MAX_REPLY_BYTES:
                        result["skipped"] += 1
                        result["warnings"].append(
                            f"Message UID {uid} is too large or has no reported size; import its quotation manually."
                        )
                    else:
                        status, data = client.uid(
                            "fetch", str(uid), f"(BODY.PEEK[]<0.{MAX_REPLY_BYTES + 1}>)"
                        )
                        if status != "OK":
                            raise ValueError("IMAP could not read a reply. Retry the reply check.")
                        raw = _literal(data)
                        if len(raw) > MAX_REPLY_BYTES or len(raw) != int(size_match[1]):
                            raise ValueError(
                                "The reply was oversized or incomplete; no partial quotation was imported."
                            )
                        if (
                            _matched_quote(
                                BytesParser(policy=policy.default).parsebytes(raw), known
                            )
                            != quote
                        ):
                            raise ValueError(
                                "Reply references changed during fetch; no quotation was imported."
                            )
                        saved = self._save_reply(
                            quote["tender_id"],
                            quote["id"],
                            _reply_data(raw),
                            raw,
                            mailbox_key,
                            validity,
                            uid,
                        )
                        result["matched"] += bool(saved)
                    last_uid = uid
                    with self.repo.db.connect(write=True) as conn:
                        conn.execute(
                            "INSERT INTO mail_sync_state VALUES(?,?,?,?) ON CONFLICT(mailbox_key) DO UPDATE SET uidvalidity=excluded.uidvalidity,last_uid=excluded.last_uid,result_json=excluded.result_json",
                            (mailbox_key, validity, last_uid, dump(result)),
                        )
            except (imaplib.IMAP4.error, OSError) as exc:
                raise ValueError(
                    "IMAP connection, TLS or authentication failed. No mailbox flags were intentionally changed."
                ) from exc
            finally:
                if client:
                    with suppress(imaplib.IMAP4.error, OSError):
                        client.logout()
            return result
