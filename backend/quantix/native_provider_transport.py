"""Enforce documented container isolation fields missing from Pydantic's tool type.

Uses the public HTTPX transport API. The official OpenAI SDK still owns the
request and complete tool schema; Quantix changes only the container's limits.
"""
import json

import httpx2

from .ai_api_errors import DirectAPIError


class NativeCodeTransport(httpx2.AsyncBaseTransport):
    def __init__(self, wrapped):
        self.wrapped = wrapped

    async def handle_async_request(self, request):
        if request.method == "POST" and request.url.host == "api.openai.com" and request.url.path == "/v1/responses":
            raw = await request.aread()
            try:
                body = json.loads(raw)
            except (ValueError, UnicodeDecodeError):
                raise DirectAPIError("The SDK produced an invalid hosted-code request.") from None
            changed = False
            for tool in body.get("tools", []):
                if not isinstance(tool, dict) or tool.get("type") != "code_interpreter":
                    continue
                container = tool.get("container")
                if not isinstance(container, dict) or container.get("type") != "auto":
                    raise DirectAPIError("Hosted code must use an owned temporary container.")
                container["memory_limit"] = "1g"
                container["network_policy"] = {"type": "disabled"}
                changed = True
            if changed:
                content = json.dumps(body, ensure_ascii=False, separators=(",", ":")).encode("utf-8")
                headers = request.headers.copy()
                headers["content-length"] = str(len(content))
                headers.pop("transfer-encoding", None)
                request = httpx2.Request(request.method, request.url, headers=headers, content=content, extensions=request.extensions)
        return await self.wrapped.handle_async_request(request)

    async def aclose(self):
        await self.wrapped.aclose()
