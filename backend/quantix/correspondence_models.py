"""Supplier quotation drafts, explicit send decisions and public mail settings."""

from datetime import datetime
from email.headerregistry import Address
from typing import Literal

from pydantic import BaseModel, ConfigDict, Field, SecretStr, field_validator


def address(value):
    if (
        not isinstance(value, str)
        or any(char in value for char in "\r\n\x00")
        or not value.isascii()
    ):
        raise ValueError("Enter one plain email address without control characters.")
    value = value.strip()
    try:
        parsed = Address(addr_spec=value)
        if not parsed.username or not parsed.domain or parsed.addr_spec != value:
            raise ValueError
    except (ValueError, IndexError) as exc:
        raise ValueError("Enter one plain email address.") from exc
    return value


class MailModel(BaseModel):
    model_config = ConfigDict(extra="forbid")


class DraftInput(MailModel):
    to: list[str] = Field(min_length=1, max_length=30)
    cc: list[str] = Field(default_factory=list, max_length=20)
    subject: str = Field(min_length=1, max_length=300)
    body: str = Field(min_length=1, max_length=100000)
    attachment_ids: list[str] = Field(default_factory=list, max_length=20)
    source_ids: list[str] = Field(default_factory=list, max_length=100)

    @field_validator("to", "cc")
    @classmethod
    def addresses(cls, values):
        return list(dict.fromkeys(address(value) for value in values))

    @field_validator("subject")
    @classmethod
    def subject_header(cls, value):
        if any(char in value for char in "\r\n\x00") or not value.strip():
            raise ValueError("The subject must be a single nonblank header line.")
        return value.strip()

    @field_validator("body")
    @classmethod
    def message_body(cls, value):
        if not value.strip() or "\x00" in value:
            raise ValueError("Enter the quotation request text.")
        return value.replace("\r\n", "\n").replace("\r", "\n").rstrip("\n") + "\n"


class Attachment(MailModel):
    artifact_id: str
    filename: str
    content_hash: str
    size: int
    version: int
    is_current: bool


class RecipientRefusal(MailModel):
    address: str
    code: int


class QuoteRecord(DraftInput):
    id: str
    tender_id: str
    message_id: str
    status: str
    attachments: list[Attachment]
    approved_fingerprint: str | None
    delivery_detail: str
    refused_recipients: list[RecipientRefusal]
    created_at: str
    updated_at: str
    submission_host: str | None = None
    delivery_history_status: str | None = None
    delivery_history_fingerprint: str | None = None


class QuotePreview(MailModel):
    quote: QuoteRecord
    sender: str | None
    attachments: list[Attachment]
    fingerprint: str
    smtp_host: str
    smtp_ready: bool
    warnings: list[str]
    restore_reconciliation_required: bool = False


class SendDecision(MailModel):
    fingerprint: str = Field(pattern=r"^[a-f0-9]{64}$")
    engineer_confirmed: Literal[True]
    rationale: str = Field(min_length=1, max_length=4000)
    restore_reconciliation: str | None = Field(default=None, min_length=1, max_length=4000)

    @field_validator("rationale")
    @classmethod
    def nonblank(cls, value):
        if not value.strip():
            raise ValueError("Record why the current message is approved.")
        return value.strip()


class MailAccount(MailModel):
    smtp_host: str = Field(default="", max_length=253, pattern=r"^[A-Za-z0-9.-]*$")
    smtp_port: int = Field(default=465, ge=1, le=65535)
    smtp_security: Literal["ssl", "starttls"] = "ssl"
    smtp_username: str = Field(default="", max_length=300)
    from_address: str = ""
    imap_host: str = Field(default="", max_length=253, pattern=r"^[A-Za-z0-9.-]*$")
    imap_port: int = Field(default=993, ge=1, le=65535)
    imap_username: str = Field(default="", max_length=300)
    imap_mailbox: str = Field(default="INBOX", min_length=1, max_length=200)

    @field_validator("from_address")
    @classmethod
    def sender(cls, value):
        return address(value) if value else ""

    @field_validator("smtp_username", "imap_username", "imap_mailbox")
    @classmethod
    def control_free(cls, value):
        if any(char in value for char in "\r\n\x00") or not value.isascii():
            raise ValueError("Mail account fields must use ASCII without control characters.")
        return value


class MailSettingsPatch(MailAccount):
    smtp_password: SecretStr | None = None
    imap_password: SecretStr | None = None


class MailSettings(MailAccount):
    smtp_ready: bool
    imap_ready: bool
    detail: str


class ManualReply(MailModel):
    sender: str
    received_at: datetime
    subject: str = Field(default="", max_length=500)
    text: str = Field(min_length=1, max_length=200000)
    headers: str = Field(default="", max_length=50000)
    source_ids: list[str] = Field(default_factory=list, max_length=100)

    @field_validator("sender")
    @classmethod
    def sender_address(cls, value):
        return address(value)

    @field_validator("received_at")
    @classmethod
    def dated(cls, value):
        if value.tzinfo is None:
            raise ValueError("The reply date needs its time zone.")
        return value


class ReplyRecord(MailModel):
    id: str
    tender_id: str
    quote_id: str
    origin: Literal["manual", "imap"]
    sender: str
    received_at: str
    date_header: str | None = None
    date_basis: Literal["engineer_entered", "retrieved_at"]
    subject: str
    text: str
    headers: str
    message_id: str | None
    source_ids: list[str]
    supporting_source_ids: list[str]
    warnings: list[str]
    created_at: str


class SyncRequest(MailModel):
    max_messages: int = Field(default=30, ge=1, le=50)


class SyncResult(MailModel):
    checked: int
    matched: int
    skipped: int
    more_available: bool
    warnings: list[str]
