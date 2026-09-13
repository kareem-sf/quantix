"""Grounded public research uses safe retrieval and exact saved passages."""

from __future__ import annotations

import hashlib
import http.client
import json
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from types import SimpleNamespace
from urllib.parse import urlsplit

import pytest

from quantix.execution_context import engineer_identity
from quantix.repository import Repository


class _Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        if self.path == "/start":
            self.send_response(302)
            self.send_header("Location", "/product")
            self.end_headers()
            return
        if self.path == "/private-redirect":
            self.send_response(302)
            self.send_header("Location", "http://127.0.0.1/private")
            self.end_headers()
            return
        body = (
            b"<html><head><title>Pump data</title><script>private()</script></head>"
            b"<body><h1>Concrete pump</h1><p>Output is 70 m3 per hour.</p>"
            b"<p>Delivery to Cairo is quoted separately.</p></body></html>"
        )
        self.send_response(200)
        self.send_header("Content-Type", "text/html; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, *_args):
        return


@pytest.fixture
def public_server():
    server = ThreadingHTTPServer(("127.0.0.1", 0), _Handler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        yield server.server_port
    finally:
        server.shutdown()
        thread.join(timeout=5)


class _LocalFixtureTransport:
    """Exercise real HTTP while the requested host remains a public test name."""

    def __init__(self, port: int):
        self.port = port

    def get(self, url: str, max_bytes: int, _checked_addresses: list[str]):
        from quantix.research_service import TransportResponse

        parsed = urlsplit(url)
        connection = http.client.HTTPConnection("127.0.0.1", self.port, timeout=5)
        connection.request("GET", parsed.path or "/", headers={"Host": parsed.netloc})
        response = connection.getresponse()
        body = response.read(max_bytes + 1)
        result = TransportResponse(
            status=response.status,
            headers={key.lower(): value for key, value in response.getheaders()},
            body=body,
        )
        connection.close()
        return result


def _service(repo, port):
    from quantix.research_service import ResearchService

    return ResearchService(
        repo,
        test_mode=True,
        resolver=lambda _host, _port: ["93.184.216.34"],
        transport=_LocalFixtureTransport(port),
    )


def test_research_tool_schemas_need_no_repository_or_runtime_state():
    from quantix.research_tools import research_tools

    definitions = research_tools()
    assert {item.name for item in definitions} == {
        "search_public_sources",
        "fetch_public_url",
        "fetch_rendered_public_url",
        "cite_public_passages",
        "record_market_observation",
        "list_research_receipts",
        "save_working_memory",
        "list_working_memory",
    }


@pytest.mark.asyncio
async def test_staff_research_and_memory_lists_keep_other_actor_roots_private(
    tmp_path, public_server
):
    from types import SimpleNamespace

    from quantix.execution_context import engineer_identity
    from quantix.memory_models import WorkingMemoryCommand
    from quantix.memory_service import MemoryService
    from quantix.research_models import PublicFetchCommand, ResearchCitationCommand
    from quantix.research_tools import research_tools

    repo = Repository(tmp_path / "quantix")
    tender = repo.create_tender("Synthetic scoped research Tender")
    run = repo.create_run(tender["id"], "manager", "Research")
    other_run = repo.create_run(tender["id"], "manager", "Other research")
    research = _service(repo, public_server)
    current_identity = engineer_identity(
        tender["id"],
        actor_kind="staff",
        actor_id="staff-current",
        root_run_id=run["id"],
    )
    other_identity = engineer_identity(
        tender["id"],
        actor_kind="staff",
        actor_id="staff-other",
        root_run_id=run["id"],
    )
    other_root_identity = engineer_identity(
        tender["id"],
        actor_kind="staff",
        actor_id="staff-current",
        root_run_id=other_run["id"],
    )
    research.fetch(
        current_identity,
        PublicFetchCommand(
            url="https://public.example/current",
            idempotency_key="current-public",
        ),
    )
    other_receipt = research.fetch(
        other_identity,
        PublicFetchCommand(
            url="https://public.example/private-query",
            idempotency_key="other-public",
        ),
    )
    research.fetch(
        other_root_identity,
        PublicFetchCommand(
            url="https://public.example/private-root-query",
            idempotency_key="other-root-public",
        ),
    )
    with pytest.raises(ValueError, match="another Tender actor root"):
        research.cite(
            current_identity,
            ResearchCitationCommand(
                receipt_id=other_receipt.id,
                passage_ids=[other_receipt.passages[0].id],
                purpose="Must not cite another actor's receipt.",
                idempotency_key="cross-actor-citation",
            ),
        )
    memory = MemoryService(repo)
    source_body = b"Synthetic scoped source."
    artifact, _created = repo.register_artifact(
        tender["id"],
        "Sources/scoped.txt",
        hashlib.sha256(source_body).hexdigest(),
        len(source_body),
        {
            "kind": "txt",
            "status": "extracted",
            "segments": [{"locator": "line:1", "text": source_body.decode()}],
        },
    )
    scoped_source = repo.artifact_evidence(tender["id"], artifact["id"])[0]["id"]
    memory.save(
        current_identity,
        WorkingMemoryCommand(
            kind="scratch",
            title="Current scratch",
            content="Visible only to its current actor root.",
            idempotency_key="current-memory",
        ),
    )
    memory.save(
        other_identity,
        WorkingMemoryCommand(
            kind="scratch",
            title="Other private scratch",
            content="Must not cross the actor root.",
            idempotency_key="other-memory",
        ),
    )
    memory.save(
        other_root_identity,
        WorkingMemoryCommand(
            kind="scratch",
            title="Other root scratch",
            content="Must not cross the active root.",
            idempotency_key="other-root-memory",
        ),
    )
    memory.save(
        current_identity,
        WorkingMemoryCommand(
            kind="assumption",
            title="Source outside current scope",
            content="Must be excluded when its exact artifact is not reviewed.",
            source_ids=[scoped_source],
            idempotency_key="outside-scope-memory",
        ),
        inspected_source_ids={scoped_source},
    )
    context = SimpleNamespace(
        repo=repo,
        tender_id=tender["id"],
        run_id=run["id"],
        actor_id="staff-current",
        staff_id="staff-current",
        staff_version=1,
        assignment_id="assignment-current",
        route_binding_id="binding-current",
        route_binding=None,
        is_staff=True,
        require_tool=lambda _tool: None,
        ensure_scope_current=lambda: None,
        evidence_allowed=lambda source_id: source_id != scoped_source,
    )
    tools = {item.name: item for item in research_tools(repo, service=research)}
    receipts = json.loads(
        await tools["list_research_receipts"].invoke(
            context, {"offset": 0, "limit": 20}
        )
    )
    notes = json.loads(
        await tools["list_working_memory"].invoke(
            context, {"offset": 0, "limit": 20}
        )
    )

    assert [item["actor_id"] for item in receipts["items"]] == ["staff-current"]
    assert receipts["excluded_count"] == 2
    assert [item["title"] for item in notes["items"]] == ["Current scratch"]
    assert notes["excluded_count"] == 3


@pytest.mark.asyncio
async def test_cancelled_public_fetch_drains_owned_thread_and_records_actual_outcome(
    tmp_path, monkeypatch
):
    import asyncio
    import threading
    from types import SimpleNamespace

    from quantix.research_service import ResearchService, TransportResponse
    from quantix.research_tools import research_tools

    started = threading.Event()
    release = threading.Event()

    class BlockingTransport:
        def get(self, _url, _max_bytes, _addresses):
            started.set()
            assert release.wait(5)
            return TransportResponse(
                status=200,
                headers={"content-type": "text/plain; charset=utf-8"},
                body=b"Synthetic public result after a blocked fetch.",
            )

    class Budget:
        completions = []

        def reserve(self, _context, *, url, idempotency_key):
            return {"id": f"reservation:{idempotency_key}", "url": url}

        def complete(self, reservation_id, *, receipt_id, failed=False):
            self.completions.append((reservation_id, receipt_id, failed))

    repo = Repository(tmp_path / "quantix")
    tender = repo.create_tender("Synthetic cancelled public fetch")
    run = repo.create_run(tender["id"], "manager", "Fetch public source")
    service = ResearchService(
        repo,
        test_mode=True,
        resolver=lambda _host, _port: ["93.184.216.34"],
        transport=BlockingTransport(),
    )
    budget = Budget()
    monkeypatch.setattr(
        "quantix.research_tools.ResearchBudgetService", lambda _repo: budget
    )
    context = SimpleNamespace(
        repo=repo,
        tender_id=tender["id"],
        run_id=run["id"],
        actor_id="manager",
        staff_id=None,
        assignment_id=None,
        route_binding_id=None,
        route_binding=None,
        staff_version=None,
        is_staff=False,
        require_tool=lambda _tool: None,
        ensure_scope_current=lambda: None,
    )
    tool = next(
        item
        for item in research_tools(repo, service=service)
        if item.name == "fetch_public_url"
    )
    task = asyncio.create_task(
        tool.invoke(
            context,
            {
                "request": {
                    "url": "https://public.example/blocked",
                    "max_bytes": 100_000,
                }
            },
        )
    )
    assert await asyncio.to_thread(started.wait, 2)
    task.cancel()
    await asyncio.sleep(0.05)
    assert not task.done()
    assert budget.completions == []
    release.set()
    with pytest.raises(asyncio.CancelledError):
        await task

    saved = service.list(tender["id"])
    assert len(saved) == 1
    assert budget.completions == [
        (budget.completions[0][0], saved[0].id, False)
    ]


def test_office_research_accepts_only_exact_cited_public_passages():
    from quantix.office_research import ResearchRecord

    url = "https://public.example/cited"
    context = SimpleNamespace(
        research_state={
            "public_citations": {
                url: {
                    "url": url,
                    "title": "Cited source",
                    "retrieved_at": "2026-09-13T00:00:00Z",
                    "cited": True,
                    "citation_id": "citation-1",
                    "passage_ids": ["passage-1"],
                    "content_sha256": "a" * 64,
                }
            }
        }
    )
    record = ResearchRecord(context)
    assert record.sources[url]["citation_id"] == "citation-1"
    record.validate(SimpleNamespace(summary=f"See {url}", web_findings=[], price_proposals=[]))
    with pytest.raises(ValueError, match="not returned"):
        record.validate(
            SimpleNamespace(
                summary="See https://model-invented.example/not-cited",
                web_findings=[],
                price_proposals=[],
            )
        )


@pytest.mark.asyncio
async def test_browser_reader_reports_private_runtime_requirement_without_host_fallback(
    tmp_path, monkeypatch
):
    from quantix.browser_research import BrowserResearchRuntime

    repo = Repository(tmp_path / "quantix")

    class MissingPrivateRuntime:
        def executable(self):
            return None

        async def status(self):
            return SimpleNamespace(
                python_available=False,
                detail="Private Podman is not installed.",
                next_action="Set up local Python.",
                state="not_installed",
            )

    monkeypatch.setattr(
        "quantix.browser_research.get_code_runtime",
        lambda _repo: MissingPrivateRuntime(),
    )
    status = await BrowserResearchRuntime(repo).status()
    assert status.ready is False
    assert status.podman_available is False
    assert status.state == "not_installed"
    assert "Private Podman" in status.detail


@pytest.mark.asyncio
async def test_rendered_page_is_saved_as_the_same_exact_passage_receipt(tmp_path):
    from quantix.browser_research import RenderedPublicPage
    from quantix.research_models import PublicFetchCommand
    from quantix.research_service import ResearchService

    repo = Repository(tmp_path / "quantix")
    tender = repo.create_tender("Synthetic rendered research Tender")
    page = RenderedPublicPage(
        requested_url="https://public.example/rendered",
        final_url="https://public.example/rendered",
        status_code=200,
        title="Rendered pump page",
        text="Rendered output is 75 m3 per hour.",
        html="<html><body>Rendered output is 75 m3 per hour.</body></html>",
        runtime_image_id="sha256:" + "c" * 64,
        runtime_fingerprint="d" * 64,
    )

    class Runtime:
        calls = 0

        async def render(self, _url):
            self.calls += 1
            return page

    runtime = Runtime()
    service = ResearchService(
        repo,
        test_mode=True,
        resolver=lambda _host, _port: ["93.184.216.34"],
    )
    command = PublicFetchCommand(
        url=page.requested_url,
        idempotency_key="rendered-page",
    )
    first = await service.fetch_with_browser(
        engineer_identity(tender["id"]), command, runtime
    )
    replay = await service.fetch_with_browser(
        engineer_identity(tender["id"]), command, runtime
    )
    assert replay == first
    assert runtime.calls == 1
    assert first.retrieval_mode == "browser"
    assert first.renderer_image_id == page.runtime_image_id
    assert first.renderer_fingerprint == page.runtime_fingerprint
    assert first.passages[0].text == page.text


def test_public_fetch_follows_checked_redirect_and_saves_exact_passages(tmp_path, public_server):
    from quantix.research_models import PublicFetchCommand

    repo = Repository(tmp_path / "quantix")
    tender = repo.create_tender("Synthetic public research Tender")
    service = _service(repo, public_server)
    command = PublicFetchCommand(
        url=f"http://public.test:{public_server}/start",
        max_bytes=200_000,
        idempotency_key="fetch-pump",
    )
    first = service.fetch(engineer_identity(tender["id"]), command)
    replay = service.fetch(engineer_identity(tender["id"]), command)

    assert replay == first
    assert first.retrieval_mode == "http"
    assert first.final_url.endswith("/product")
    assert first.redirect_chain == [
        f"http://public.test:{public_server}/start",
        f"http://public.test:{public_server}/product",
    ]
    assert first.content_sha256
    assert any("70 m3 per hour" in passage.text for passage in first.passages)
    assert all("private()" not in passage.text for passage in first.passages)
    assert first.cited is False
    assert service.get(tender["id"], first.id) == first
    body_path = repo.home / "research" / f"{first.id}.body"
    assert body_path.is_file()
    assert body_path.read_bytes().startswith(b"<html>")


def test_private_destination_and_redirect_are_blocked_before_network_use(tmp_path, public_server):
    from quantix.research_models import PublicFetchCommand
    from quantix.research_service import ResearchService

    repo = Repository(tmp_path / "quantix")
    tender = repo.create_tender("Synthetic safe fetch Tender")
    with pytest.raises(ValueError, match="credentials"):
        PublicFetchCommand(
            url="https://public.example/page?access_token=secret",
            idempotency_key="credential-url",
        )
    private = ResearchService(
        repo,
        test_mode=True,
        resolver=lambda _host, _port: ["127.0.0.1"],
        transport=_LocalFixtureTransport(public_server),
    )
    with pytest.raises(ValueError, match="public address"):
        private.fetch(
            engineer_identity(tender["id"]),
            PublicFetchCommand(
                url=f"http://public.test:{public_server}/product",
                idempotency_key="private-dns",
            ),
        )

    service = _service(repo, public_server)
    with pytest.raises(ValueError, match="public address"):
        service.fetch(
            engineer_identity(tender["id"]),
            PublicFetchCommand(
                url=f"http://public.test:{public_server}/private-redirect",
                idempotency_key="private-redirect",
            ),
        )


def test_only_saved_passages_can_be_cited_or_support_market_observations(tmp_path, public_server):
    from quantix.research_models import (
        MarketObservationCommand,
        PublicFetchCommand,
        ResearchCitationCommand,
    )

    repo = Repository(tmp_path / "quantix")
    tender = repo.create_tender("Synthetic cited market Tender")
    identity = engineer_identity(tender["id"])
    service = _service(repo, public_server)
    receipt = service.fetch(
        identity,
        PublicFetchCommand(
            url=f"http://public.test:{public_server}/product",
            idempotency_key="market-fetch",
        ),
    )
    with pytest.raises(KeyError, match="passage"):
        service.cite(
            identity,
            ResearchCitationCommand(
                receipt_id=receipt.id,
                passage_ids=["model-invented-passage"],
                purpose="Support the pump observation.",
                idempotency_key="bad-citation",
            ),
        )
    with pytest.raises(ValueError, match="citation"):
        service.record_market_observation(
            identity,
            MarketObservationCommand(
                basis="observed_quotation",
                product="Concrete pump",
                specification="70 m3/hour nominal output",
                value="12000",
                unit="EGP/day",
                geography="Cairo, Egypt",
                currency="EGP",
                tax_basis="excluding_vat",
                delivery_terms="Delivery quoted separately",
                observed_on="2026-09-12",
                citation_ids=[],
                idempotency_key="uncited-market",
            ),
        )

    citation = service.cite(
        identity,
        ResearchCitationCommand(
            receipt_id=receipt.id,
            passage_ids=[receipt.passages[0].id, receipt.passages[0].id],
            purpose="Support the pump observation.",
            idempotency_key="pump-citation",
        ),
    )
    assert citation.passage_ids == [receipt.passages[0].id]
    assert citation.work_product_reference == f"public_citation:{citation.id}"
    assert service.validate_work_product_refs(identity, [citation.work_product_reference]) == [
        citation
    ]
    with pytest.raises(ValueError, match="saved public_citation"):
        service.validate_work_product_refs(identity, [receipt.final_url])
    observation = service.record_market_observation(
        identity,
        MarketObservationCommand(
            basis="observed_quotation",
            product="Concrete pump",
            specification="70 m3/hour nominal output",
            value="12000",
            unit="EGP/day",
            geography="Cairo, Egypt",
            currency="EGP",
            tax_basis="excluding_vat",
            delivery_terms="Delivery quoted separately",
            observed_on="2026-09-11",
            valid_until="2026-09-11",
            citation_ids=[citation.id],
            idempotency_key="cited-market",
        ),
    )
    assert observation.basis == "observed_quotation"
    assert observation.citations[0].receipt_id == receipt.id
    assert observation.needs_recheck is True
    assert observation.review_reasons == ["validity_date_reached"]


@pytest.mark.asyncio
async def test_citation_tool_updates_only_the_current_office_research_collector(
    tmp_path, public_server
):
    import json

    from quantix.office_research import ResearchRecord
    from quantix.office_tools import OfficeContext
    from quantix.research_models import PublicFetchCommand
    from quantix.research_tools import research_tools

    repo = Repository(tmp_path / "quantix")
    tender = repo.create_tender("Synthetic citation tool Tender")
    run = repo.create_run(tender["id"], "manager", "Cite a fetched public passage")
    service = _service(repo, public_server)
    receipt = service.fetch(
        engineer_identity(
            tender["id"],
            actor_kind="manager",
            actor_id="manager",
            root_run_id=run["id"],
        ),
        PublicFetchCommand(
            url=f"http://public.test:{public_server}/product",
            idempotency_key="tool-source",
        ),
    )
    context = OfficeContext(repo, tender["id"], run["id"])
    context.actor_id = "manager"
    tools = {item.name: item for item in research_tools(repo, service=service)}
    result = await tools["cite_public_passages"].invoke(
        context,
        {
            "citation": {
                "receipt_id": receipt.id,
                "passage_ids": [receipt.passages[0].id],
                "purpose": "Support the synthetic pump observation.",
            }
        },
        invocation_id="cite-tool",
    )
    cited = json.loads(result)
    assert (
        context.research_state["public_citations"][receipt.final_url]["citation_id"] == cited["id"]
    )
    assert ResearchRecord(context).sources[receipt.final_url]["passage_ids"] == [
        receipt.passages[0].id
    ]
