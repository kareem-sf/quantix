"""Strict contracts for grounded public research and market observations."""

from __future__ import annotations

from typing import Literal
from urllib.parse import parse_qsl, urlsplit

from pydantic import Field, field_validator, model_validator

from .staff_models import IdentifierText, OfficeModel, TimestampText


class BrowserResearchStatusModel(OfficeModel):
    ready: bool
    podman_available: bool
    state: Literal[
        "not_installed",
        "prerequisite_required",
        "setup_required",
        "stopped",
        "ready",
        "needs_repair",
        "busy",
    ]
    image: str
    image_id: str | None = Field(default=None, max_length=200)
    detail: str


class PublicFetchRequest(OfficeModel):
    url: str = Field(min_length=1, max_length=3000)
    max_bytes: int = Field(default=1_000_000, ge=1024, le=2_000_000)

    @field_validator("url")
    @classmethod
    def public_http_shape(cls, value: str) -> str:
        try:
            parsed = urlsplit(value)
        except ValueError as error:
            raise ValueError("Enter a complete public HTTP or HTTPS URL.") from error
        if (
            parsed.scheme.lower() not in {"http", "https"}
            or not parsed.hostname
            or parsed.username is not None
            or parsed.password is not None
            or parsed.fragment
        ):
            raise ValueError("Enter a public HTTP or HTTPS URL without credentials or a fragment.")
        sensitive = {
            "token",
            "access_token",
            "api_key",
            "apikey",
            "key",
            "signature",
            "credential",
            "password",
        }
        if any(
            key.casefold() in sensitive
            or "credential" in key.casefold()
            or "signature" in key.casefold()
            for key, _value in parse_qsl(parsed.query, keep_blank_values=True)
        ):
            raise ValueError("Public research URLs cannot contain credentials.")
        return value


class PublicFetchCommand(PublicFetchRequest):
    idempotency_key: IdentifierText


class PublicSearchRequest(OfficeModel):
    query: str = Field(min_length=1, max_length=2000)
    limit: int = Field(default=5, ge=1, le=10)


class PublicSearchHit(OfficeModel):
    url: str = Field(min_length=1, max_length=3000)
    title: str = Field(default="", max_length=300)
    summary: str = Field(default="", max_length=2000)
    retrieved_at: TimestampText
    provider_metadata: Literal[True] = True
    citable: Literal[False] = False


class PublicSearchReceipt(OfficeModel):
    id: IdentifierText
    tender_id: IdentifierText
    root_run_id: IdentifierText
    actor_id: IdentifierText
    requested_by_actor_id: IdentifierText
    requested_by_assignment_id: IdentifierText | None = None
    requested_by_route_binding_id: IdentifierText | None = None
    requested_by_staff_version: int | None = Field(default=None, ge=1)
    query: str
    limit: int = Field(ge=1, le=10)
    status: Literal["deferred", "completed", "failed"]
    route_option_id: IdentifierText
    connection_id: IdentifierText
    review_fingerprint: str = Field(pattern=r"^[a-f0-9]{64}$")
    source_scope_fingerprint: str = Field(pattern=r"^[a-f0-9]{64}$")
    request_hash: str = Field(pattern=r"^[a-f0-9]{64}$")
    results: list[PublicSearchHit] = Field(max_length=10)
    usage: dict = Field(default_factory=dict)
    created_at: TimestampText
    completed_at: TimestampText | None = None
    detail: str


class ResearchSearchCandidate(OfficeModel):
    url: str = Field(min_length=1, max_length=3000)
    title: str = Field(default="", max_length=300)
    summary: str = Field(default="", max_length=2000)


class ResearchSearchOutput(OfficeModel):
    results: list[ResearchSearchCandidate] = Field(default_factory=list, max_length=10)


class ResearchPassage(OfficeModel):
    id: IdentifierText
    index: int = Field(ge=0)
    text: str = Field(min_length=1, max_length=4000)
    sha256: str = Field(pattern=r"^[a-f0-9]{64}$")


class PublicResearchReceipt(OfficeModel):
    id: IdentifierText
    tender_id: IdentifierText
    run_id: IdentifierText | None = None
    actor_id: IdentifierText
    retrieval_mode: Literal["http", "browser"]
    requested_url: str
    final_url: str
    redirect_chain: list[str] = Field(min_length=1, max_length=6)
    status_code: int = Field(ge=200, le=299)
    content_type: str = Field(min_length=1, max_length=200)
    content_bytes: int = Field(ge=0, le=2_000_000)
    content_sha256: str = Field(pattern=r"^[a-f0-9]{64}$")
    title: str = Field(default="", max_length=300)
    retrieved_at: TimestampText
    passages: list[ResearchPassage] = Field(max_length=250)
    cited: bool
    renderer_image_id: str | None = Field(default=None, max_length=200)
    renderer_fingerprint: str | None = Field(
        default=None, pattern=r"^[a-f0-9]{64}$"
    )


class ResearchCitationDraft(OfficeModel):
    receipt_id: IdentifierText
    passage_ids: list[IdentifierText] = Field(min_length=1, max_length=50)
    purpose: str = Field(min_length=1, max_length=1000)

    @field_validator("passage_ids")
    @classmethod
    def unique_passages(cls, values: list[str]) -> list[str]:
        return list(dict.fromkeys(values))


class ResearchCitationCommand(ResearchCitationDraft):
    idempotency_key: IdentifierText


class ResearchCitation(ResearchCitationDraft):
    id: IdentifierText
    tender_id: IdentifierText
    run_id: IdentifierText | None = None
    actor_id: IdentifierText
    url: str
    title: str = Field(default="", max_length=300)
    retrieved_at: TimestampText
    content_sha256: str = Field(pattern=r"^[a-f0-9]{64}$")
    work_product_reference: str = Field(pattern=r"^public_citation:[A-Za-z0-9_.:/-]{1,160}$")
    created_at: TimestampText


MarketBasis = Literal["observed_quotation", "published_price", "estimate"]
TaxBasis = Literal["excluding_vat", "including_vat", "unknown"]


class MarketObservationDraft(OfficeModel):
    basis: MarketBasis
    product: str = Field(min_length=1, max_length=300)
    specification: str = Field(min_length=1, max_length=2000)
    value: str = Field(pattern=r"^-?\d{1,12}(?:\.\d{1,6})?$")
    unit: str = Field(min_length=1, max_length=100)
    geography: str = Field(min_length=1, max_length=300)
    currency: str = Field(pattern=r"^[A-Z]{3}$")
    tax_basis: TaxBasis
    delivery_terms: str = Field(min_length=1, max_length=1000)
    observed_on: str = Field(pattern=r"^\d{4}-\d{2}-\d{2}$")
    valid_until: str | None = Field(default=None, pattern=r"^\d{4}-\d{2}-\d{2}$")
    citation_ids: list[IdentifierText] = Field(min_length=1, max_length=20)

    @field_validator("citation_ids")
    @classmethod
    def unique_citations(cls, values: list[str]) -> list[str]:
        return list(dict.fromkeys(values))

    @model_validator(mode="after")
    def valid_dates(self):
        from datetime import UTC, date, datetime

        observed = date.fromisoformat(self.observed_on)
        if observed > datetime.now(UTC).date():
            raise ValueError("A market observation cannot be dated in the future.")
        if self.valid_until and date.fromisoformat(self.valid_until) < observed:
            raise ValueError("The validity date cannot precede the observation date.")
        return self


class MarketObservationCommand(MarketObservationDraft):
    idempotency_key: IdentifierText


class MarketObservation(MarketObservationDraft):
    id: IdentifierText
    tender_id: IdentifierText
    run_id: IdentifierText | None = None
    actor_id: IdentifierText
    citations: list[ResearchCitation]
    created_at: TimestampText
    needs_recheck: bool
    review_reasons: list[str]


__all__ = [
    "BrowserResearchStatusModel",
    "MarketObservation",
    "MarketObservationCommand",
    "MarketObservationDraft",
    "PublicFetchCommand",
    "PublicFetchRequest",
    "PublicSearchHit",
    "PublicSearchReceipt",
    "PublicSearchRequest",
    "PublicResearchReceipt",
    "ResearchCitation",
    "ResearchCitationCommand",
    "ResearchCitationDraft",
    "ResearchPassage",
    "ResearchSearchCandidate",
    "ResearchSearchOutput",
]
