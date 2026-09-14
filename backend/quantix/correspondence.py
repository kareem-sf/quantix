"""Supplier quotation request drafts. The engineer sends them from their own mail client."""

import hashlib
import mimetypes
import os
import re
from datetime import datetime
from email import policy
from email.message import EmailMessage
from email.utils import format_datetime, make_msgid

from .correspondence_models import DraftInput, ManualReply
from .db import dump, new_id, now, record

MAX_ATTACHMENTS = 20 * 1024**2
SCHEMA = (
    "CREATE TABLE IF NOT EXISTS quote_drafts(id TEXT PRIMARY KEY,tender_id TEXT NOT NULL REFERENCES tenders(id),message_id TEXT NOT NULL UNIQUE,status TEXT NOT NULL,data_json TEXT NOT NULL,created_at TEXT NOT NULL,updated_at TEXT NOT NULL)",
    "CREATE TABLE IF NOT EXISTS quote_replies(id TEXT PRIMARY KEY,tender_id TEXT NOT NULL REFERENCES tenders(id),quote_id TEXT NOT NULL REFERENCES quote_drafts(id),data_json TEXT NOT NULL,raw_message BLOB,created_at TEXT NOT NULL)",
)


class QuoteService:
    def __init__(self, repo):
        self.repo = repo
        with repo.db.connect(write=True) as conn:
            for statement in SCHEMA:
                conn.execute(statement)

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

    def _data(self, tender_id, values):
        request = DraftInput.model_validate(values)
        for source_id in request.source_ids:
            self.repo.get_evidence(tender_id, source_id)
        attachments = self._attachments(tender_id, request.attachment_ids)
        return request.model_dump() | {"attachments": [item[0] for item in attachments]}

    def create_draft(self, tender_id, values):
        self.repo.get_tender(tender_id)
        data = self._data(tender_id, values)
        identifier, stamp = new_id(), now()
        with self.repo.db.connect(write=True) as conn:
            conn.execute(
                "INSERT INTO quote_drafts(id,tender_id,message_id,status,data_json,created_at,updated_at) VALUES(?,?,?,'draft',?,?,?)",
                (
                    identifier,
                    tender_id,
                    make_msgid(domain="quantix.local"),
                    dump(data),
                    stamp,
                    stamp,
                ),
            )
        return self.get(tender_id, identifier)

    def get(self, tender_id, quote_id):
        with self.repo.db.connect() as conn:
            row = record(
                conn.execute(
                    "SELECT id,tender_id,message_id,status,data_json,created_at,updated_at FROM quote_drafts WHERE id=? AND tender_id=?",
                    (quote_id, tender_id),
                ).fetchone()
            )
        return {key: value for key, value in row.items() if key != "data"} | row["data"]

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
        self.get(tender_id, quote_id)
        data = self._data(tender_id, values)
        with self.repo.db.connect(write=True) as conn:
            conn.execute(
                "UPDATE quote_drafts SET data_json=?,updated_at=? WHERE id=? AND tender_id=?",
                (dump(data), now(), quote_id, tender_id),
            )
        return self.get(tender_id, quote_id)

    def _message(self, tender_id, quote_id):
        quote = self.get(tender_id, quote_id)
        attachments = self._attachments(tender_id, quote["attachment_ids"])
        message = EmailMessage(policy=policy.SMTP)
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
            digest = hashlib.sha256(dump([item[0] for item in attachments]).encode()).hexdigest()
            message.set_boundary("quantix-" + digest)
        return quote, [item[0] for item in attachments], message.as_bytes()

    def preview(self, tender_id, quote_id):
        quote, attachments, _ = self._message(tender_id, quote_id)
        warnings = (
            ["An attachment is an older source revision."]
            if any(not item["is_current"] for item in attachments)
            else []
        )
        return {"quote": quote, "attachments": attachments, "warnings": warnings}

    def eml(self, tender_id, quote_id):
        return self._message(tender_id, quote_id)[2]

    def register_reply(self, tender_id, quote_id, values):
        request = ManualReply.model_validate(values)
        data = request.model_dump(mode="json")
        raw = dump(data).encode()
        data["supporting_source_ids"] = data.pop("source_ids")
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
        text = data["text"]
        segment_metadata = {
            "sender": data["sender"],
            "received_at": data["received_at"],
            "quote_id": quote_id,
        }
        extraction = {
            "kind": "other",
            "status": "extracted",
            "warnings": [],
            "metadata": {
                "origin": "manual",
                "sender": data["sender"],
                "received_at": data["received_at"],
            },
            "segments": [
                {
                    "locator": f"reply:{identifier}/characters:{offset + 1}-{min(offset + 6000, len(text))}",
                    "text": text[offset : offset + 6000],
                    "kind": "correspondence",
                    "metadata": segment_metadata,
                }
                for offset in range(0, len(text), 6000)
            ],
        }
        with self.repo.atomic():
            artifact, _ = self.repo.register_artifact(
                tender_id,
                f"Correspondence/{quote_id}/reply-{identifier}.json",
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
                    "INSERT INTO quote_replies(id,tender_id,quote_id,data_json,raw_message,created_at) VALUES(?,?,?,?,?,?)",
                    (identifier, tender_id, quote_id, dump(data), raw, stamp),
                )
        return {
            "id": identifier,
            "tender_id": tender_id,
            "quote_id": quote_id,
            "created_at": stamp,
            **data,
        }

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
