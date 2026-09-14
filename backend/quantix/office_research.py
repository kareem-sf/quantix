"""Preserve provider-returned web citations and request usage, never model-invented URLs."""

import re
from datetime import UTC, datetime
from urllib.parse import urlsplit

from .office_tools import safe_text


class ResearchRecord:
    def __init__(self, context):
        self.context = context
        self._sources: dict[str, dict] = {}

    @property
    def sources(self) -> dict[str, dict]:
        return self._sources

    @sources.setter
    def sources(self, value: dict[str, dict]) -> None:
        self._sources = value

    def add_sources(self, sources):
        for source in sources:
            self._source(source, "cited" if source.get("cited") else "consulted")

    def _source(self, source: dict, kind: str) -> None:
        url = source.get("url", "")
        try:
            parsed = urlsplit(url)
        except ValueError:
            return
        if parsed.scheme not in {"http", "https"} or not parsed.hostname or parsed.username:
            return
        if len(url) > 3000:
            return
        record = self._sources.setdefault(
            url,
            {
                "url": url,
                "title": safe_text(source.get("title"), 300),
                "retrieved_at": datetime.now(UTC).isoformat(),
                "cited": False,
            },
        )
        if kind == "cited":
            record["cited"] = True

    def validate(self, output) -> None:
        for url in re.findall(r"https?://[^\s<>\]\)\"]+", output.summary):
            if url.rstrip(".,;:") not in self.sources:
                raise ValueError("The summary cites a URL not returned by web search in this run.")
        for proposal in [*output.web_findings, *output.price_proposals]:
            for url in proposal.urls:
                if url not in self.sources:
                    raise ValueError(
                        "The response cites a URL not returned by web search in this run."
                    )
        for price in output.price_proposals:
            if price.basis == "observed" and price.observed_on is None:
                raise ValueError("An observed price needs its source observation date.")
            if price.observed_on and price.observed_on > datetime.now(UTC).date():
                raise ValueError("A price observation cannot be dated in the future.")
