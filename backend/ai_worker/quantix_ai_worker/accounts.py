"""Original-client account flows inside the managed connection worker."""

import asyncio
import re
from urllib.parse import urlsplit

from .common import RuntimeConnectionFailure, RuntimeUnavailable, owned_client

try:
    from .diagnostics import record_exception
except ImportError:  # Source-only account probes run before worker preparation.
    try:
        from quantix.diagnostics import record_exception
    except ImportError:  # pragma: no cover - diagnostics are best effort.

        def record_exception(*_args, **_kwargs):
            return False


DOCS = {
    "codex": "https://learn.chatgpt.com/docs/codex-sdk",
    "grok_build": "https://docs.x.ai/build/overview",
}


class AccountService:
    """Original client auth in this connection's owned worker process."""

    def __init__(self, connection, home):
        self.connection = connection
        self.home = home
        self.pending = None
        self.state = None

    def record(self, state, detail, **values):
        self.state = {
            "connection_id": self.connection["id"],
            "installed": True,
            "state": state,
            "detail": detail,
            "login_url": None,
            "user_code": None,
            "docs_url": DOCS.get(self.connection["protocol"]),
            **values,
        }
        return self.state

    def _account_failure(self, operation, error, fallback):
        """Record only bounded exception structure and return safe UI copy."""
        try:
            record_exception(
                "ai_account_operation_failed",
                error,
                phase="account",
                operation=operation,
                protocol=self.connection.get("protocol"),
                connection_id=self.connection.get("id"),
            )
        except Exception:
            # Diagnostics are best effort and must never block account recovery.
            pass
        if isinstance(error, (RuntimeUnavailable, RuntimeConnectionFailure)):
            # These exception classes are created by our provider adapters and
            # already carry sanitized, actionable recovery text.
            return str(error)[:1200]
        return fallback

    async def status(self):
        if self.state:
            return self.state
        if self.connection["auth_type"] == "client_login":
            return await self.saved_account_status()
        return self.record(
            "installed",
            "The original client component is prepared. Sign-in and model access have not been confirmed.",
        )

    async def saved_account_status(self):
        """Ask the original client about its saved account without starting login.

        Settled account workers close after each operation. A fresh worker must
        recover status through the client, rather than requiring another login
        or treating the presence of private cache files as confirmed access.
        """
        protocol = self.connection["protocol"]
        try:
            async with asyncio.timeout(75):
                if protocol == "codex":
                    from openai_codex import AsyncCodex

                    from .codex import codex_config

                    async with owned_client(AsyncCodex(codex_config(self.home))) as client:
                        account = await client.account()
                        metadata = account.model_dump(mode="json", by_alias=True).get("account")
                    signed_in = isinstance(metadata, dict) and metadata.get("type") == "chatgpt"
                elif protocol == "grok_build":
                    from .grok_auth import cached_account_status

                    signed_in = await cached_account_status(self.home, self.connection)
                else:
                    return self.record(
                        "installed",
                        "The original client component is prepared. Sign-in and model access have not been confirmed.",
                    )
        except (RuntimeUnavailable, RuntimeConnectionFailure) as error:
            return self.record(
                "attention",
                self._account_failure(
                    "status",
                    error,
                    "The original client could not refresh account access. Check the connection and try again.",
                ),
            )
        except Exception as error:
            return self.record(
                "attention",
                self._account_failure(
                    "status",
                    error,
                    "The original client could not refresh saved account access. Check the connection and sign in again.",
                ),
            )
        if signed_in:
            return self.record(
                "signed_in",
                "The original client confirmed this connection's saved sign-in. Model access remains to be checked.",
            )
        return self.record(
            "sign_in_required",
            "The original client has no confirmed saved sign-in for this connection. Start sign-in to continue.",
        )

    async def login(self):
        if self.pending and not self.pending.done():
            return await self.status()
        protocol = self.connection["protocol"]
        if protocol == "grok_build":
            from .grok_auth import authenticate

            ready = asyncio.get_running_loop().create_future()
            self.pending = asyncio.create_task(authenticate(self, ready))
            return await asyncio.shield(ready)
        if protocol != "codex":
            raise RuntimeUnavailable("This connection does not offer a sign-in action.")
        from openai_codex import AsyncCodex

        from .codex import codex_config

        ready = asyncio.get_running_loop().create_future()

        async def complete():
            handle = None
            try:
                async with owned_client(AsyncCodex(codex_config(self.home))) as client:
                    async with asyncio.timeout(45):
                        handle = await client.login_chatgpt()
                    value = self.record(
                        "login_pending",
                        "Continue the original OpenAI sign-in in your browser.",
                        login_url=handle.auth_url,
                    )
                    if not ready.done():
                        ready.set_result(value)
                    try:
                        async with asyncio.timeout(900):
                            result = await handle.wait()
                    except BaseException:
                        try:
                            await asyncio.wait_for(handle.cancel(), 3)
                        except Exception:
                            pass
                        raise
                self.record(
                    "signed_in" if result.success else "sign_in_required",
                    "OpenAI confirmed sign-in. Model access remains to be checked."
                    if result.success
                    else "OpenAI did not complete sign-in. Start sign-in again.",
                )
            except asyncio.CancelledError:
                self.record("sign_in_required", "The pending sign-in was cancelled.")
                if not ready.done():
                    ready.cancel()
                raise
            except (RuntimeUnavailable, RuntimeConnectionFailure) as error:
                value = self.record(
                    "attention",
                    self._account_failure(
                        "login",
                        error,
                        "The original OpenAI sign-in could not start. Check the connection and try again.",
                    ),
                )
                if not ready.done():
                    ready.set_result(value)
            except Exception as error:
                value = self.record(
                    "attention",
                    self._account_failure(
                        "login",
                        error,
                        "The original OpenAI sign-in could not start or finish. Check the connection and start sign-in again.",
                    ),
                )
                if not ready.done():
                    ready.set_result(value)

        self.pending = asyncio.create_task(complete())
        return await asyncio.shield(ready)

    async def login_device(self):
        if self.pending and not self.pending.done():
            return await self.status()
        if self.connection["protocol"] == "codex":
            return await self.codex_device_login()
        if self.connection["protocol"] != "grok_build":
            raise RuntimeUnavailable("This connection does not offer a device sign-in action.")
        from .grok_auth import authenticate_device

        ready = asyncio.get_running_loop().create_future()
        self.pending = asyncio.create_task(authenticate_device(self, ready))
        return await asyncio.shield(ready)

    async def codex_device_login(self):
        """Run the official SDK's documented ChatGPT device-code flow."""
        from openai_codex import AsyncCodex

        from .codex import codex_config

        ready = asyncio.get_running_loop().create_future()

        async def complete():
            handle = None
            try:
                async with owned_client(AsyncCodex(codex_config(self.home))) as client:
                    async with asyncio.timeout(45):
                        handle = await client.login_chatgpt_device_code()
                    url = handle.verification_url
                    parsed = urlsplit(url) if isinstance(url, str) else None
                    code = handle.user_code
                    if (
                        not parsed
                        or parsed.scheme != "https"
                        or parsed.hostname
                        not in {"auth.openai.com", "chatgpt.com", "auth.chatgpt.com"}
                        or parsed.port not in {None, 443}
                        or parsed.username
                        or parsed.password
                        or len(url) > 5000
                        or any(ord(char) < 33 for char in url)
                        or not isinstance(code, str)
                        or not re.fullmatch(r"[A-Za-z0-9-]{1,128}", code)
                    ):
                        raise RuntimeUnavailable(
                            "The original OpenAI client returned an unsupported device sign-in address or code."
                        )
                    value = self.record(
                        "login_pending",
                        "Open the official OpenAI address and confirm this one-time code.",
                        login_url=url,
                        user_code=code,
                    )
                    if not ready.done():
                        ready.set_result(value)
                    try:
                        async with asyncio.timeout(900):
                            result = await handle.wait()
                    except BaseException:
                        try:
                            await asyncio.wait_for(handle.cancel(), 3)
                        except Exception:
                            pass
                        raise
                self.record(
                    "signed_in" if result.success else "sign_in_required",
                    "OpenAI confirmed sign-in. Model access remains to be checked."
                    if result.success
                    else "OpenAI did not complete device sign-in. Start sign-in again.",
                )
            except asyncio.CancelledError:
                self.record("sign_in_required", "The pending device sign-in was cancelled.")
                if not ready.done():
                    ready.cancel()
                raise
            except (RuntimeUnavailable, RuntimeConnectionFailure) as error:
                value = self.record(
                    "attention",
                    self._account_failure(
                        "device_login",
                        error,
                        "The original OpenAI device sign-in could not start. Check the connection and try again.",
                    ),
                )
                if not ready.done():
                    ready.set_result(value)
            except Exception as error:
                value = self.record(
                    "attention",
                    self._account_failure(
                        "device_login",
                        error,
                        "The original OpenAI device sign-in could not start or finish. Check the connection and start sign-in again.",
                    ),
                )
                if not ready.done():
                    ready.set_result(value)

        self.pending = asyncio.create_task(complete())
        return await asyncio.shield(ready)

    async def logout(self):
        if self.pending and not self.pending.done():
            self.pending.cancel()
            await asyncio.gather(self.pending, return_exceptions=True)
        if self.connection["protocol"] == "grok_build":
            from .grok_auth import logout

            signed_out = await logout(self.home, self.connection)
            return self.record(
                "signed_out" if signed_out else "attention",
                "Grok confirmed this connection is signed out."
                if signed_out
                else "Grok still reports account access. Sign-out has not been confirmed.",
            )
        if self.connection["protocol"] == "codex":
            from openai_codex import AsyncCodex

            from .codex import codex_config

            async with asyncio.timeout(30):
                async with owned_client(AsyncCodex(codex_config(self.home))) as client:
                    await client.logout()
            return self.record(
                "signed_out", "The original Codex client cleared this connection's sign-in."
            )
        raise RuntimeUnavailable("This connection does not offer a sign-out action.")

    async def close(self):
        if self.pending and not self.pending.done():
            self.pending.cancel()
            await asyncio.gather(self.pending, return_exceptions=True)
        # The native component host owns every descendant and terminates any
        # remaining process tree when the worker transport closes.
