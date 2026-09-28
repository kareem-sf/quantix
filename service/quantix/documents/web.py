"""Web research: searching the web and reading its pages, for the market facts a tender's documents don't give,
such as prices, suppliers, subcontractors, datasheets and outputs. Firecrawl answers first and needs no key;
TinyFish takes over when Firecrawl can't, once the engineer has added its free key. Only the search words and page
addresses leave the computer. Every page read is saved as it was, so what the office cites from it can be checked."""

import logging
from dataclasses import dataclass
from datetime import UTC, datetime, timedelta
from pathlib import Path
from typing import Any

import httpx
from sqlalchemy import select
from sqlalchemy.orm import Session

from quantix.ai import connections
from quantix.documents.models import WebPage

log = logging.getLogger("quantix.web")

SERVICES = ("firecrawl", "tinyfish")  # in the order they are tried
FIRECRAWL = "https://api.firecrawl.dev/v2"
TINYFISH_SEARCH = "https://api.search.tinyfish.ai"
TINYFISH_FETCH = "https://api.fetch.tinyfish.ai"
RESULTS = 8
FRESH = timedelta(days=7)  # a page read more recently than this is reused rather than read again
KEEP = 200_000  # characters of a page kept

client = httpx.Client(timeout=90.0)


class Unavailable(Exception):
    """No web service could answer."""


@dataclass
class Result:
    title: str
    url: str
    snippet: str


def search(home: Path, query: str) -> list[Result]:
    """What the web has for the query, best first."""
    return _first(home, "search", query)


def read(session: Session, home: Path, url: str) -> WebPage:
    """The page's text, saved: the copy read in the past week, or a new reading."""
    if not url.startswith(("https://", "http://")):
        raise ValueError("Give the page's full address, starting with https://")
    since = datetime.now(UTC) - FRESH
    query = select(WebPage).where(WebPage.url == url, WebPage.read_at >= since).order_by(WebPage.read_at.desc())
    saved = session.scalars(query).first()
    if saved is not None:
        return saved
    title, text = _first(home, "read", url)
    page = WebPage(url=url, title=(title or url)[:300], text=text[:KEEP])
    session.add(page)
    session.flush()
    return page


def check_key(service: str, api_key: str) -> None:
    """Raises if the service refuses the key, by searching once with it."""
    _SEARCH[service]("construction materials", api_key)


def _first(home: Path, action: str, argument: str) -> Any:
    keys = connections.web_keys(home)
    for service in SERVICES:
        if service == "tinyfish" and "tinyfish" not in keys:
            continue  # it has no keyless use
        try:
            return (_SEARCH if action == "search" else _READ)[service](argument, keys.get(service))
        except (httpx.HTTPError, KeyError, TypeError, ValueError) as error:
            log.warning("Web %s through %s failed: %s", action, service, error)
    raise Unavailable()


def _firecrawl_search(query: str, api_key: str | None) -> list[Result]:
    response = client.post(f"{FIRECRAWL}/search", json={"query": query, "limit": RESULTS}, headers=_bearer(api_key))
    response.raise_for_status()
    found = response.json()["data"]["web"]
    return [Result(r.get("title") or r["url"], r["url"], r.get("description") or "") for r in found]


def _firecrawl_read(url: str, api_key: str | None) -> tuple[str, str]:
    body = {"url": url, "formats": ["markdown"], "onlyMainContent": True}
    response = client.post(f"{FIRECRAWL}/scrape", json=body, headers=_bearer(api_key))
    response.raise_for_status()
    data = response.json()["data"]
    if (data["metadata"].get("statusCode") or 200) >= 400 or not data.get("markdown"):
        raise ValueError(f"the page answered {data['metadata'].get('statusCode')} with no text")
    title = data["metadata"].get("title")
    return (title[0] if isinstance(title, list) else title) or "", data["markdown"]


def _tinyfish_search(query: str, api_key: str | None) -> list[Result]:
    response = client.get(TINYFISH_SEARCH, params={"query": query}, headers={"X-API-Key": api_key or ""})
    response.raise_for_status()
    found = response.json()["results"][:RESULTS]
    return [Result(r.get("title") or r["url"], r["url"], r.get("snippet") or "") for r in found]


def _tinyfish_read(url: str, api_key: str | None) -> tuple[str, str]:
    body = {"urls": [url], "format": "markdown"}
    response = client.post(TINYFISH_FETCH, json=body, headers={"X-API-Key": api_key or ""})
    response.raise_for_status()
    data = response.json()
    if not data.get("results") or not data["results"][0].get("text"):
        raise ValueError(next((e["error"] for e in data.get("errors") or []), "no text"))
    return data["results"][0].get("title") or "", data["results"][0]["text"]


def _bearer(api_key: str | None) -> dict[str, str]:
    return {"Authorization": f"Bearer {api_key}"} if api_key else {}  # without a key, Firecrawl's free daily use


_SEARCH = {"firecrawl": _firecrawl_search, "tinyfish": _tinyfish_search}
_READ = {"firecrawl": _firecrawl_read, "tinyfish": _tinyfish_read}
