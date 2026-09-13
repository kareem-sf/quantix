"""Safe bounded public retrieval with durable exact-passage citations."""

from __future__ import annotations

import hashlib
import html
import http.client
import ipaddress
import json
import os
import re
import socket
import ssl
from contextlib import nullcontext
from dataclasses import dataclass
from datetime import UTC, date, datetime
from html.parser import HTMLParser
from typing import Callable
from urllib.parse import urljoin, urlsplit

from .db import dump, new_id, now
from .research_models import (
    MarketObservation,
    MarketObservationCommand,
    PublicFetchCommand,
    PublicFetchRequest,
    PublicResearchReceipt,
    ResearchCitation,
    ResearchCitationCommand,
    ResearchPassage,
)
from .staff_models import OfficeConflict

_SCHEMA = (
    """
    CREATE TABLE IF NOT EXISTS public_research_receipts(
        id TEXT PRIMARY KEY,
        tender_id TEXT NOT NULL REFERENCES tenders(id),
        run_id TEXT NOT NULL,
        actor_id TEXT NOT NULL,
        retrieval_mode TEXT NOT NULL DEFAULT 'http',
        renderer_image_id TEXT,
        renderer_fingerprint TEXT,
        requested_url TEXT NOT NULL,
        final_url TEXT NOT NULL,
        redirect_chain_json TEXT NOT NULL,
        status_code INTEGER NOT NULL,
        content_type TEXT NOT NULL,
        content_bytes INTEGER NOT NULL,
        content_sha256 TEXT NOT NULL,
        title TEXT NOT NULL,
        request_hash TEXT NOT NULL,
        idempotency_key TEXT NOT NULL,
        retrieved_at TEXT NOT NULL,
        UNIQUE(tender_id,run_id,idempotency_key)
    )
    """,
    """
    CREATE TABLE IF NOT EXISTS public_research_passages(
        id TEXT PRIMARY KEY,
        receipt_id TEXT NOT NULL REFERENCES public_research_receipts(id),
        passage_index INTEGER NOT NULL,
        text TEXT NOT NULL,
        sha256 TEXT NOT NULL,
        UNIQUE(receipt_id,passage_index)
    )
    """,
    """
    CREATE TABLE IF NOT EXISTS public_research_citations(
        id TEXT PRIMARY KEY,
        tender_id TEXT NOT NULL REFERENCES tenders(id),
        run_id TEXT NOT NULL,
        actor_id TEXT NOT NULL,
        receipt_id TEXT NOT NULL REFERENCES public_research_receipts(id),
        passage_ids_json TEXT NOT NULL,
        purpose TEXT NOT NULL,
        payload_hash TEXT NOT NULL,
        idempotency_key TEXT NOT NULL,
        created_at TEXT NOT NULL,
        UNIQUE(tender_id,run_id,idempotency_key)
    )
    """,
    """
    CREATE TABLE IF NOT EXISTS market_observations(
        id TEXT PRIMARY KEY,
        tender_id TEXT NOT NULL REFERENCES tenders(id),
        run_id TEXT NOT NULL,
        actor_id TEXT NOT NULL,
        payload_json TEXT NOT NULL,
        payload_hash TEXT NOT NULL,
        idempotency_key TEXT NOT NULL,
        created_at TEXT NOT NULL,
        UNIQUE(tender_id,run_id,idempotency_key)
    )
    """,
)

_ALLOWED_CONTENT_TYPES = {
    "text/html",
    "text/plain",
    "text/xml",
    "application/json",
    "application/xml",
    "application/xhtml+xml",
}
_REDIRECTS = {301, 302, 303, 307, 308}
_MAX_REDIRECTS = 5
_PASSAGE_CHARS = 1800


@dataclass(frozen=True)
class TransportResponse:
    status: int
    headers: dict[str, str]
    body: bytes


class _PinnedHTTPSConnection(http.client.HTTPSConnection):
    """Connect to a checked address while retaining hostname TLS validation."""

    def __init__(self, host: str, port: int, address: str):
        super().__init__(host, port, timeout=20, context=ssl.create_default_context())
        self._checked_address = address

    def connect(self):
        sock = socket.create_connection(
            (self._checked_address, self.port), self.timeout, self.source_address
        )
        self.sock = self._context.wrap_socket(sock, server_hostname=self.host)


class _PinnedTransport:
    def get(self, url: str, max_bytes: int, checked_addresses: list[str]) -> TransportResponse:
        parsed = urlsplit(url)
        host = parsed.hostname
        if host is None:
            raise ValueError("The public URL has no host.")
        port = parsed.port or (443 if parsed.scheme.lower() == "https" else 80)
        address = checked_addresses[0]
        connection: http.client.HTTPConnection
        if parsed.scheme.lower() == "https":
            connection = _PinnedHTTPSConnection(host, port, address)
        else:
            connection = http.client.HTTPConnection(address, port, timeout=20)
        target = parsed.path or "/"
        if parsed.query:
            target += f"?{parsed.query}"
        host_header = host if port in {80, 443} else f"{host}:{port}"
        try:
            connection.request(
                "GET",
                target,
                headers={
                    "Host": host_header,
                    "User-Agent": "Quantix-Public-Research/1",
                    "Accept": "text/html,text/plain,application/json,application/xml;q=0.8",
                    "Connection": "close",
                },
            )
            response = connection.getresponse()
            return TransportResponse(
                status=int(response.status),
                headers={key.lower(): value for key, value in response.getheaders()},
                body=response.read(max_bytes + 1),
            )
        finally:
            connection.close()


class _HTMLText(HTMLParser):
    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.hidden = 0
        self.in_title = False
        self.title_parts: list[str] = []
        self.parts: list[str] = []

    def handle_starttag(self, tag, _attrs):
        lowered = tag.casefold()
        if lowered in {"script", "style", "noscript", "svg"}:
            self.hidden += 1
        if lowered == "title":
            self.in_title = True
        if lowered in {"p", "div", "section", "article", "li", "h1", "h2", "h3", "br"}:
            self.parts.append("\n")

    def handle_endtag(self, tag):
        lowered = tag.casefold()
        if lowered in {"script", "style", "noscript", "svg"} and self.hidden:
            self.hidden -= 1
        if lowered == "title":
            self.in_title = False
        if lowered in {"p", "div", "section", "article", "li", "h1", "h2", "h3"}:
            self.parts.append("\n")

    def handle_data(self, data):
        if self.hidden:
            return
        if self.in_title:
            self.title_parts.append(data)
        self.parts.append(data)

    @property
    def title(self) -> str:
        return _clean_text(" ".join(self.title_parts))[:300]

    @property
    def text(self) -> str:
        paragraphs = [_clean_text(item) for item in "".join(self.parts).splitlines()]
        return "\n\n".join(item for item in paragraphs if item)


def _clean_text(value: str) -> str:
    return re.sub(r"\s+", " ", html.unescape(value)).strip()


def _hash(value: bytes | dict) -> str:
    if isinstance(value, dict):
        value = json.dumps(
            value,
            ensure_ascii=False,
            sort_keys=True,
            separators=(",", ":"),
            allow_nan=False,
        ).encode("utf-8")
    return hashlib.sha256(value).hexdigest()


def _resolver(host: str, port: int) -> list[str]:
    return list(
        dict.fromkeys(
            item[4][0] for item in socket.getaddrinfo(host, port, type=socket.SOCK_STREAM)
        )
    )


def _identity(identity) -> tuple[str, str, str]:
    tender_id = getattr(identity, "tender_id", None)
    if not isinstance(tender_id, str) or not tender_id:
        raise ValueError("Public research requires a selected Tender.")
    run_id = getattr(identity, "root_run_id", None) or ""
    actor_id = getattr(identity, "actor_id", None) or "engineer"
    return tender_id, run_id, actor_id


class ResearchService:
    def __init__(
        self,
        repo,
        *,
        test_mode: bool = False,
        resolver=None,
        transport=None,
    ):
        if (resolver is not None or transport is not None) and not test_mode:
            raise ValueError("Research network overrides are available only to tests.")
        self.repo = repo
        self.resolver = resolver or _resolver
        self.transport = transport or _PinnedTransport()
        self.directory = repo.home / "research"
        self.directory.mkdir(parents=True, exist_ok=True)
        with repo.atomic() as conn:
            for statement in _SCHEMA:
                conn.execute(statement)
            columns = {
                row["name"]
                for row in conn.execute("PRAGMA table_info(public_research_receipts)")
            }
            if "retrieval_mode" not in columns:
                conn.execute(
                    "ALTER TABLE public_research_receipts ADD COLUMN retrieval_mode TEXT NOT NULL DEFAULT 'http'"
                )
            if "renderer_image_id" not in columns:
                conn.execute(
                    "ALTER TABLE public_research_receipts ADD COLUMN renderer_image_id TEXT"
                )
            if "renderer_fingerprint" not in columns:
                conn.execute(
                    "ALTER TABLE public_research_receipts ADD COLUMN renderer_fingerprint TEXT"
                )

    def _validate_url(self, url: str) -> list[str]:
        PublicFetchRequest(url=url)
        parsed = urlsplit(url)
        if (
            parsed.scheme.lower() not in {"http", "https"}
            or not parsed.hostname
            or parsed.username is not None
            or parsed.password is not None
            or parsed.fragment
        ):
            raise ValueError("Enter a public HTTP or HTTPS URL without credentials or a fragment.")
        try:
            port = parsed.port or (443 if parsed.scheme.lower() == "https" else 80)
        except ValueError as error:
            raise ValueError("The public URL has an invalid port.") from error
        try:
            literal = ipaddress.ip_address(parsed.hostname)
        except ValueError:
            literal = None
        if literal is not None:
            addresses = [str(literal)]
        else:
            try:
                addresses = self.resolver(parsed.hostname, port)
            except OSError as error:
                raise ValueError("The public URL host could not be resolved.") from error
        if not addresses:
            raise ValueError("The public URL host could not be resolved.")
        for address in addresses:
            try:
                ip = ipaddress.ip_address(address)
            except ValueError as error:
                raise ValueError("The public URL host returned an invalid address.") from error
            if not ip.is_global:
                raise ValueError("Public research URLs must resolve only to a public address.")
        return addresses

    def validate_public_url(self, url: str) -> None:
        """Resolve and reject every non-public destination before admission."""

        self._validate_url(url)

    def _retrieve(self, command: PublicFetchCommand) -> tuple[str, list[str], TransportResponse]:
        current = command.url
        chain: list[str] = []
        for _index in range(_MAX_REDIRECTS + 1):
            checked_addresses = self._validate_url(current)
            chain.append(current)
            response = self.transport.get(current, command.max_bytes, checked_addresses)
            if len(response.body) > command.max_bytes:
                raise ValueError("The public response exceeds the selected content limit.")
            if response.status in _REDIRECTS:
                location = response.headers.get("location")
                if not location:
                    raise ValueError("The public site returned an incomplete redirect.")
                current = urljoin(current, location)
                continue
            if not 200 <= response.status <= 299:
                raise ValueError(
                    f"The public site returned HTTP {response.status}; no source was saved."
                )
            return current, chain, response
        raise ValueError("The public site redirected too many times.")

    @staticmethod
    def _decode(response: TransportResponse) -> tuple[str, str, str]:
        header = response.headers.get("content-type", "")
        content_type = header.split(";", 1)[0].strip().casefold()
        if content_type not in _ALLOWED_CONTENT_TYPES:
            raise ValueError("The public response is not a supported text document.")
        charset = "utf-8"
        match = re.search(r"charset=([A-Za-z0-9._-]+)", header, re.I)
        if match:
            charset = match.group(1)
        try:
            decoded = response.body.decode(charset, errors="replace")
        except LookupError as error:
            raise ValueError(
                "The public response declares an unsupported text encoding."
            ) from error
        if content_type in {"text/html", "application/xhtml+xml"}:
            parser = _HTMLText()
            parser.feed(decoded)
            return content_type, parser.title, parser.text
        return content_type, "", _clean_text(decoded)

    @staticmethod
    def _passages(text: str) -> list[tuple[str, str]]:
        passages: list[tuple[str, str]] = []
        cursor = 0
        while cursor < len(text) and len(passages) < 250:
            end = min(len(text), cursor + _PASSAGE_CHARS)
            if end < len(text):
                boundary = text.rfind("\n\n", cursor, end)
                if boundary > cursor + 300:
                    end = boundary
            value = text[cursor:end].strip()
            if value:
                passages.append((value, _hash(value.encode("utf-8"))))
            cursor = max(cursor + 1, end)
        if text[cursor:].strip():
            raise ValueError("The public text exceeds the supported passage count.")
        return passages

    def _receipt(self, conn, receipt_id: str) -> PublicResearchReceipt:
        row = conn.execute(
            "SELECT * FROM public_research_receipts WHERE id=?", (receipt_id,)
        ).fetchone()
        if row is None:
            raise KeyError("This public research receipt could not be found.")
        passages = [
            ResearchPassage(
                id=item["id"],
                index=item["passage_index"],
                text=item["text"],
                sha256=item["sha256"],
            )
            for item in conn.execute(
                """
                SELECT * FROM public_research_passages
                WHERE receipt_id=? ORDER BY passage_index
                """,
                (receipt_id,),
            )
        ]
        cited = conn.execute(
            "SELECT 1 FROM public_research_citations WHERE receipt_id=? LIMIT 1",
            (receipt_id,),
        ).fetchone()
        return PublicResearchReceipt(
            id=row["id"],
            tender_id=row["tender_id"],
            run_id=row["run_id"] or None,
            actor_id=row["actor_id"],
            retrieval_mode=row["retrieval_mode"],
            requested_url=row["requested_url"],
            final_url=row["final_url"],
            redirect_chain=json.loads(row["redirect_chain_json"]),
            status_code=row["status_code"],
            content_type=row["content_type"],
            content_bytes=row["content_bytes"],
            content_sha256=row["content_sha256"],
            title=row["title"],
            retrieved_at=row["retrieved_at"],
            passages=passages,
            cited=bool(cited),
            renderer_image_id=row["renderer_image_id"],
            renderer_fingerprint=row["renderer_fingerprint"],
        )

    def get(self, tender_id: str, receipt_id: str) -> PublicResearchReceipt:
        with self.repo.db.connect() as conn:
            receipt = self._receipt(conn, receipt_id)
            if receipt.tender_id != tender_id:
                raise KeyError("This public research receipt could not be found.")
            return receipt

    def list(self, tender_id: str, *, offset: int = 0, limit: int = 50):
        self.repo.get_tender(tender_id)
        if offset < 0 or not 1 <= limit <= 100:
            raise ValueError("Read 1 to 100 research receipts from a nonnegative offset.")
        with self.repo.db.connect() as conn:
            rows = conn.execute(
                """
                SELECT id FROM public_research_receipts
                WHERE tender_id=? ORDER BY retrieved_at DESC,id DESC LIMIT ? OFFSET ?
                """,
                (tender_id, limit, offset),
            ).fetchall()
            return [self._receipt(conn, row["id"]) for row in rows]

    def list_for_identity(
        self, identity, *, offset: int = 0, limit: int = 50
    ) -> tuple[list[PublicResearchReceipt], int]:
        """Return only public receipts fetched by this exact actor root."""

        tender_id, run_id, actor_id = _identity(identity)
        self.repo.get_tender(tender_id)
        if offset < 0 or not 1 <= limit <= 100:
            raise ValueError("Read 1 to 100 research receipts from a nonnegative offset.")
        with self.repo.db.connect() as conn:
            all_count = int(
                conn.execute(
                    "SELECT COUNT(*) FROM public_research_receipts WHERE tender_id=?",
                    (tender_id,),
                ).fetchone()[0]
            )
            allowed_count = int(
                conn.execute(
                    """
                    SELECT COUNT(*) FROM public_research_receipts
                    WHERE tender_id=? AND run_id=? AND actor_id=?
                    """,
                    (tender_id, run_id, actor_id),
                ).fetchone()[0]
            )
            rows = conn.execute(
                """
                SELECT id FROM public_research_receipts
                WHERE tender_id=? AND run_id=? AND actor_id=?
                ORDER BY retrieved_at DESC,id DESC LIMIT ? OFFSET ?
                """,
                (tender_id, run_id, actor_id, limit, offset),
            ).fetchall()
            return (
                [self._receipt(conn, row["id"]) for row in rows],
                all_count - allowed_count,
            )

    def _existing_fetch(
        self,
        tender_id: str,
        run_id: str,
        actor_id: str,
        command: PublicFetchCommand,
        request_hash: str,
    ) -> PublicResearchReceipt | None:
        with self.repo.db.connect() as conn:
            existing = conn.execute(
                """
                SELECT id,request_hash,actor_id FROM public_research_receipts
                WHERE tender_id=? AND run_id=? AND idempotency_key=?
                """,
                (tender_id, run_id, command.idempotency_key),
            ).fetchone()
            if existing is None:
                return None
            if existing["request_hash"] != request_hash:
                raise OfficeConflict(
                    "This public-fetch key was already used for different research."
                )
            if existing["actor_id"] != actor_id:
                raise OfficeConflict(
                    "This public-fetch key belongs to another Tender actor root."
                )
            return self._receipt(conn, existing["id"])

    def _save_fetch(
        self,
        *,
        tender_id: str,
        run_id: str,
        actor_id: str,
        command: PublicFetchCommand,
        request_hash: str,
        final_url: str,
        chain: list[str],
        response: TransportResponse,
        content_type: str,
        title: str,
        text: str,
        retrieval_mode: str = "http",
        renderer_image_id: str | None = None,
        renderer_fingerprint: str | None = None,
        write_guard: Callable[[], object] | None = None,
        validate_commit: Callable[[], None] | None = None,
    ) -> PublicResearchReceipt:
        passages = self._passages(text)
        if not passages:
            raise ValueError("The public response contains no readable text.")
        receipt_id, stamp = new_id(), now()
        temporary = self.directory / f"{receipt_id}.body.tmp"
        destination = self.directory / f"{receipt_id}.body"
        temporary.write_bytes(response.body)
        try:
            with (write_guard() if write_guard else nullcontext()), self.repo.atomic() as conn:
                if validate_commit:
                    validate_commit()
                conn.execute(
                    """
                    INSERT INTO public_research_receipts(
                        id,tender_id,run_id,actor_id,retrieval_mode,
                        renderer_image_id,renderer_fingerprint,requested_url,final_url,
                        redirect_chain_json,status_code,content_type,content_bytes,
                        content_sha256,title,request_hash,idempotency_key,retrieved_at
                    ) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)
                    """,
                    (
                        receipt_id,
                        tender_id,
                        run_id,
                        actor_id,
                        retrieval_mode,
                        renderer_image_id,
                        renderer_fingerprint,
                        command.url,
                        final_url,
                        dump(chain),
                        response.status,
                        content_type,
                        len(response.body),
                        _hash(response.body),
                        title,
                        request_hash,
                        command.idempotency_key,
                        stamp,
                    ),
                )
                conn.executemany(
                    """
                    INSERT INTO public_research_passages(
                        id,receipt_id,passage_index,text,sha256
                    ) VALUES(?,?,?,?,?)
                    """,
                    [
                        (new_id(), receipt_id, index, value, digest)
                        for index, (value, digest) in enumerate(passages)
                    ],
                )
                os.replace(temporary, destination)
                return self._receipt(conn, receipt_id)
        except BaseException:
            temporary.unlink(missing_ok=True)
            destination.unlink(missing_ok=True)
            raise

    def fetch(
        self,
        identity,
        command: PublicFetchCommand | dict,
        *,
        write_guard: Callable[[], object] | None = None,
        validate_commit: Callable[[], None] | None = None,
    ) -> PublicResearchReceipt:
        command = PublicFetchCommand.model_validate(command)
        tender_id, run_id, actor_id = _identity(identity)
        self.repo.get_tender(tender_id)
        request_hash = _hash(command.model_dump(mode="json", exclude={"idempotency_key"}))
        existing = self._existing_fetch(
            tender_id, run_id, actor_id, command, request_hash
        )
        if existing is not None:
            return existing

        final_url, chain, response = self._retrieve(command)
        content_type, title, text = self._decode(response)
        return self._save_fetch(
            tender_id=tender_id,
            run_id=run_id,
            actor_id=actor_id,
            command=command,
            request_hash=request_hash,
            final_url=final_url,
            chain=chain,
            response=response,
            content_type=content_type,
            title=title,
            text=text,
            write_guard=write_guard,
            validate_commit=validate_commit,
        )

    def save_rendered(
        self,
        identity,
        command: PublicFetchCommand | dict,
        page,
        *,
        write_guard: Callable[[], object] | None = None,
        validate_commit: Callable[[], None] | None = None,
    ) -> PublicResearchReceipt:
        """Persist bounded HTML/text returned by the isolated browser runtime."""

        command = PublicFetchCommand.model_validate(command)
        tender_id, run_id, actor_id = _identity(identity)
        self.repo.get_tender(tender_id)
        request_hash = _hash(command.model_dump(mode="json", exclude={"idempotency_key"}))
        existing = self._existing_fetch(
            tender_id, run_id, actor_id, command, request_hash
        )
        if existing is not None:
            return existing
        self._validate_url(command.url)
        self._validate_url(page.final_url)
        requested_host = (urlsplit(command.url).hostname or "").encode("idna").decode().lower()
        final_host = (urlsplit(page.final_url).hostname or "").encode("idna").decode().lower()
        if requested_host != final_host:
            raise ValueError("The rendered page left its checked public host.")
        body = str(page.html).encode("utf-8")
        if len(body) > command.max_bytes:
            raise ValueError("The rendered public page exceeds the selected content limit.")
        chain = list(dict.fromkeys([command.url, page.final_url]))
        return self._save_fetch(
            tender_id=tender_id,
            run_id=run_id,
            actor_id=actor_id,
            command=command,
            request_hash=request_hash,
            final_url=page.final_url,
            chain=chain,
            response=TransportResponse(
                status=page.status_code,
                headers={"content-type": "text/html; charset=utf-8"},
                body=body,
            ),
            content_type="text/html",
            title=str(page.title)[:300],
            text=str(page.text),
            retrieval_mode="browser",
            renderer_image_id=page.runtime_image_id,
            renderer_fingerprint=page.runtime_fingerprint,
            write_guard=write_guard,
            validate_commit=validate_commit,
        )

    async def fetch_with_browser(
        self,
        identity,
        command: PublicFetchCommand | dict,
        runtime,
        *,
        write_guard: Callable[[], object] | None = None,
        validate_commit: Callable[[], None] | None = None,
    ):
        """Render and persist a public page without repeating an accepted retry."""

        command = PublicFetchCommand.model_validate(command)
        tender_id, run_id, actor_id = _identity(identity)
        request_hash = _hash(command.model_dump(mode="json", exclude={"idempotency_key"}))
        existing = self._existing_fetch(
            tender_id, run_id, actor_id, command, request_hash
        )
        if existing is not None:
            return existing
        page = await runtime.render(command.url)
        return self.save_rendered(
            identity,
            command,
            page,
            write_guard=write_guard,
            validate_commit=validate_commit,
        )

    def _citation(self, conn, citation_id: str) -> ResearchCitation:
        row = conn.execute(
            """
            SELECT c.*,r.final_url,r.title,r.retrieved_at,r.content_sha256
            FROM public_research_citations c
            JOIN public_research_receipts r ON r.id=c.receipt_id
            WHERE c.id=?
            """,
            (citation_id,),
        ).fetchone()
        if row is None:
            raise KeyError("This public research citation could not be found.")
        return ResearchCitation(
            id=row["id"],
            tender_id=row["tender_id"],
            run_id=row["run_id"] or None,
            actor_id=row["actor_id"],
            receipt_id=row["receipt_id"],
            passage_ids=json.loads(row["passage_ids_json"]),
            purpose=row["purpose"],
            url=row["final_url"],
            title=row["title"],
            retrieved_at=row["retrieved_at"],
            content_sha256=row["content_sha256"],
            work_product_reference=f"public_citation:{row['id']}",
            created_at=row["created_at"],
        )

    def get_citation(self, tender_id: str, citation_id: str) -> ResearchCitation:
        self.repo.get_tender(tender_id)
        with self.repo.db.connect() as conn:
            citation = self._citation(conn, citation_id)
        if citation.tender_id != tender_id:
            raise KeyError("This public research citation could not be found.")
        return citation

    def validate_work_product_refs(
        self, identity, source_refs: list[str]
    ) -> list[ResearchCitation]:
        """Validate canonical cited-public references for one exact actor root."""

        tender_id, run_id, actor_id = _identity(identity)
        citations: list[ResearchCitation] = []
        with self.repo.db.connect() as conn:
            for reference in source_refs:
                if reference.startswith(("http://", "https://")):
                    raise ValueError(
                        "Use a saved public_citation reference instead of an unverified URL."
                    )
                if not reference.startswith("public_citation:"):
                    continue
                citation_id = reference.removeprefix("public_citation:")
                citation = self._citation(conn, citation_id)
                if (
                    citation.tender_id != tender_id
                    or (citation.run_id or "") != run_id
                    or citation.actor_id != actor_id
                ):
                    raise ValueError(
                        "The public citation was not retrieved and cited by this Tender actor root."
                    )
                receipt = self._receipt(conn, citation.receipt_id)
                passage_ids = {item.id for item in receipt.passages}
                if any(item not in passage_ids for item in citation.passage_ids):
                    raise ValueError("A cited public passage is no longer available.")
                citations.append(citation)
        return citations

    def cite(
        self,
        identity,
        command: ResearchCitationCommand | dict,
        *,
        write_guard: Callable[[], object] | None = None,
        validate_commit: Callable[[], None] | None = None,
    ) -> ResearchCitation:
        command = ResearchCitationCommand.model_validate(command)
        tender_id, run_id, actor_id = _identity(identity)
        payload_hash = _hash(command.model_dump(mode="json", exclude={"idempotency_key"}))
        with (write_guard() if write_guard else nullcontext()), self.repo.atomic() as conn:
            if validate_commit:
                validate_commit()
            existing = conn.execute(
                """
                SELECT id,payload_hash FROM public_research_citations
                WHERE tender_id=? AND run_id=? AND idempotency_key=?
                """,
                (tender_id, run_id, command.idempotency_key),
            ).fetchone()
            if existing is not None:
                if existing["payload_hash"] != payload_hash:
                    raise OfficeConflict(
                        "This citation key was already used for different passages."
                    )
                return self._citation(conn, existing["id"])
            receipt = self._receipt(conn, command.receipt_id)
            if receipt.tender_id != tender_id:
                raise KeyError("This public research receipt could not be found.")
            if (receipt.run_id or "") != run_id or receipt.actor_id != actor_id:
                raise ValueError(
                    "A public research receipt belongs to another Tender actor root."
                )
            available = {item.id for item in receipt.passages}
            if any(passage_id not in available for passage_id in command.passage_ids):
                raise KeyError("A selected public research passage could not be found.")
            identifier, stamp = new_id(), now()
            conn.execute(
                """
                INSERT INTO public_research_citations(
                    id,tender_id,run_id,actor_id,receipt_id,passage_ids_json,
                    purpose,payload_hash,idempotency_key,created_at
                ) VALUES(?,?,?,?,?,?,?,?,?,?)
                """,
                (
                    identifier,
                    tender_id,
                    run_id,
                    actor_id,
                    receipt.id,
                    dump(command.passage_ids),
                    command.purpose,
                    payload_hash,
                    command.idempotency_key,
                    stamp,
                ),
            )
            return self._citation(conn, identifier)

    def _observation(self, conn, observation_id: str) -> MarketObservation:
        row = conn.execute(
            "SELECT * FROM market_observations WHERE id=?", (observation_id,)
        ).fetchone()
        if row is None:
            raise KeyError("This market observation could not be found.")
        payload = json.loads(row["payload_json"])
        citations = [self._citation(conn, item) for item in payload["citation_ids"]]
        reasons = []
        if (
            payload.get("valid_until")
            and date.fromisoformat(payload["valid_until"]) < datetime.now(UTC).date()
        ):
            reasons.append("validity_date_reached")
        return MarketObservation(
            id=row["id"],
            tender_id=row["tender_id"],
            run_id=row["run_id"] or None,
            actor_id=row["actor_id"],
            citations=citations,
            created_at=row["created_at"],
            needs_recheck=bool(reasons),
            review_reasons=reasons,
            **payload,
        )

    def record_market_observation(
        self,
        identity,
        command: MarketObservationCommand | dict,
        *,
        write_guard: Callable[[], object] | None = None,
        validate_commit: Callable[[], None] | None = None,
    ) -> MarketObservation:
        command = MarketObservationCommand.model_validate(command)
        tender_id, run_id, actor_id = _identity(identity)
        payload = command.model_dump(mode="json", exclude={"idempotency_key"})
        payload_hash = _hash(payload)
        with (write_guard() if write_guard else nullcontext()), self.repo.atomic() as conn:
            if validate_commit:
                validate_commit()
            existing = conn.execute(
                """
                SELECT id,payload_hash FROM market_observations
                WHERE tender_id=? AND run_id=? AND idempotency_key=?
                """,
                (tender_id, run_id, command.idempotency_key),
            ).fetchone()
            if existing is not None:
                if existing["payload_hash"] != payload_hash:
                    raise OfficeConflict(
                        "This market-observation key was already used for different content."
                    )
                return self._observation(conn, existing["id"])
            for citation_id in command.citation_ids:
                citation = self._citation(conn, citation_id)
                if citation.tender_id != tender_id:
                    raise ValueError("A market observation citation belongs to another Tender.")
                if (citation.run_id or "") != run_id or citation.actor_id != actor_id:
                    raise ValueError("A market observation citation belongs to another actor root.")
            identifier, stamp = new_id(), now()
            conn.execute(
                """
                INSERT INTO market_observations(
                    id,tender_id,run_id,actor_id,payload_json,payload_hash,
                    idempotency_key,created_at
                ) VALUES(?,?,?,?,?,?,?,?)
                """,
                (
                    identifier,
                    tender_id,
                    run_id,
                    actor_id,
                    dump(payload),
                    payload_hash,
                    command.idempotency_key,
                    stamp,
                ),
            )
            return self._observation(conn, identifier)

    def list_market_observations(
        self, tender_id: str, *, offset: int = 0, limit: int = 100
    ) -> list[MarketObservation]:
        self.repo.get_tender(tender_id)
        if offset < 0 or not 1 <= limit <= 100:
            raise ValueError("Read 1 to 100 market observations from a nonnegative offset.")
        with self.repo.db.connect() as conn:
            rows = conn.execute(
                """
                SELECT id FROM market_observations WHERE tender_id=?
                ORDER BY created_at DESC,id DESC LIMIT ? OFFSET ?
                """,
                (tender_id, limit, offset),
            ).fetchall()
            return [self._observation(conn, row["id"]) for row in rows]


__all__ = ["ResearchService", "TransportResponse"]
