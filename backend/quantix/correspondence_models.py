"""Supplier quotation request drafts and engineer-recorded supplier replies."""

from datetime import datetime
from email.headerregistry import Address

from pydantic import BaseModel, ConfigDict, Field, field_validator


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


class QuoteRecord(DraftInput):
    id: str
    tender_id: str
    message_id: str
    status: str
    attachments: list[Attachment]
    created_at: str
    updated_at: str


class QuotePreview(MailModel):
    quote: QuoteRecord
    attachments: list[Attachment]
    warnings: list[str]


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
    sender: str
    received_at: str
    subject: str
    text: str
    headers: str
    source_ids: list[str]
    supporting_source_ids: list[str]
    created_at: str
