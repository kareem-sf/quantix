"""Preserve provider-returned web citations and request usage, never model-invented URLs."""

import re
from datetime import UTC, datetime
from urllib.parse import urlsplit

from agents import RunHooks

from .office_tools import safe_text


def usage_record(usage) -> dict:
    return {
        "requests": getattr(usage, "requests", 1),
        "input_tokens": usage.input_tokens,
        "output_tokens": usage.output_tokens,
        "total_tokens": usage.total_tokens,
        "cached_input_tokens": getattr(usage.input_tokens_details, "cached_tokens", 0) or 0,
        "cache_write_tokens": getattr(usage.input_tokens_details, "cache_write_tokens", 0) or 0,
        "reasoning_tokens": getattr(usage.output_tokens_details, "reasoning_tokens", 0) or 0,
    }


class ResearchRecord(RunHooks):
    def __init__(self, context):
        self.context = context
        self.sources: dict[str, dict] = {}
        self.responses: set[str | int] = set()
        self.requests: list[dict] = []
        self.web_search_calls = 0

    async def on_llm_end(self, context, agent, response) -> None:
        self.collect(response)

    def collect(self, response) -> None:
        identity = response.response_id or id(response)
        if identity in self.responses:
            return
        self.responses.add(identity)
        usage = usage_record(response.usage)
        usage["response_id"] = response.response_id
        if response.raw_usage:
            usage["provider_usage"] = response.raw_usage
        self.requests.append(usage)
        self.context.repo.event(
            self.context.run_id, "model_usage", "Model response received.", usage
        )
        for output in response.output:
            item = output.model_dump(mode="json") if hasattr(output, "model_dump") else output
            if item.get("type") == "web_search_call":
                self.web_search_calls += 1
                for source in (item.get("action") or {}).get("sources", []) or []:
                    self._source(source, "consulted")
            elif item.get("type") == "message":
                for content in item.get("content", []):
                    for annotation in content.get("annotations", []):
                        if annotation.get("type") == "url_citation":
                            self._source(annotation, "cited")

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
        record = self.sources.setdefault(
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

    def usage(self, result) -> dict:
        for response in result.raw_responses:
            self.collect(response)
        return {
            **usage_record(result.context_wrapper.usage),
            "request_details": self.requests,
            "web_search_calls": self.web_search_calls,
            "estimated_cost_usd": None,
            "cost_basis": "Token usage recorded; provider billing and tool charges are separate.",
        }
