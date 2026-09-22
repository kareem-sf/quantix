"""Find a provider's API address from whatever the engineer pasted.

Providers publish an API address such as ``https://api.example.com/v1``, but
the address people have to hand is usually the website.  A website answers
every path with a page, so the OpenAI client gets HTML where it expects a model
list and fails with a meaningless error.  Quantix asks the few addresses that
belong to the same site, without sending any credential, which one answers like
an OpenAI API, and uses that one.
"""

from __future__ import annotations

import asyncio
from urllib.parse import urlsplit, urlunsplit

PROBE_TIMEOUT_SECONDS = 6.0
PROBE_DEADLINE_SECONDS = 20.0
MAX_CANDIDATES = 5

ADDRESS_HELP = (
    "This address did not answer as an AI API. Open the provider's API documentation and copy "
    "its base address, which usually ends in /v1."
)


def _parts(value: str):
    parts = urlsplit(value)
    return parts.scheme, parts.netloc.lower(), parts.path.rstrip("/")


def _join(scheme: str, netloc: str, path: str) -> str:
    return urlunsplit((scheme, netloc, path, "", ""))


def candidate_base_urls(value: str) -> list[str]:
    """Addresses on the same site that a provider may serve its API from."""

    scheme, netloc, path = _parts(value)
    if not scheme or not netloc:
        return []
    hosts = [netloc]
    host, _, port = netloc.partition(":")
    bare = host.removeprefix("www.")
    if not bare.startswith("api.") and bare.count(".") >= 1:
        hosts.append(f"api.{bare}" + (f":{port}" if port else ""))
    candidates = []
    for host_value in hosts:
        for suffix in ("", "/v1"):
            if suffix and path.endswith(suffix):
                continue
            candidates.append(_join(scheme, host_value, path + suffix))
    return list(dict.fromkeys(candidates))[:MAX_CANDIDATES]


def answers_like_api(status: int, content_type: str, body: str) -> bool:
    """Whether a /models reply came from an OpenAI-compatible API.

    A model list proves it.  So does a refusal to serve one without a key: a
    website has no reason to answer JSON with 401 or 403.
    """

    if "json" not in content_type.lower():
        return False
    if status in {401, 403}:
        return True
    if status != 200:
        return False
    import json

    try:
        payload = json.loads(body)
    except ValueError:
        return False
    if isinstance(payload, list):
        return True
    return isinstance(payload, dict) and isinstance(payload.get("data"), list)


async def _probe(client, base_url: str) -> bool:
    try:
        async with asyncio.timeout(PROBE_TIMEOUT_SECONDS):
            response = await client.get(f"{base_url}/models")
    except Exception:
        return False
    return answers_like_api(
        response.status_code, response.headers.get("content-type", ""), response.text[:4000]
    )


async def resolve_api_base_url(
    value: str | None, *, allow_insecure_http: bool = False
) -> str | None:
    """Return the address on this site that answers as an API, or None.

    No credential is sent: the address is judged by how it answers an
    unauthenticated request for the model list.
    """

    if not value:
        return None
    from .ai_connections import validate_base_url

    try:
        import httpx2 as httpx
    except ImportError:  # pragma: no cover - the bundled client is always present
        return None
    checked = []
    for candidate in candidate_base_urls(value):
        try:
            validated = validate_base_url(candidate, allow_insecure_http=allow_insecure_http)
        except ValueError:
            continue
        if validated and validated not in checked:
            checked.append(validated)
    if not checked:
        return None
    try:
        async with asyncio.timeout(PROBE_DEADLINE_SECONDS):
            async with httpx.AsyncClient(
                timeout=PROBE_TIMEOUT_SECONDS, follow_redirects=False, trust_env=False
            ) as client:
                for base_url in checked:
                    if await _probe(client, base_url):
                        return base_url
    except (TimeoutError, asyncio.TimeoutError):
        return None
    except asyncio.CancelledError:
        raise
    except Exception:
        return None
    return None
