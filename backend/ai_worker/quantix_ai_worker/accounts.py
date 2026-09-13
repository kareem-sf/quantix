"""Original-client account flows inside the managed connection worker."""

import asyncio
import json
import os
import platform
import re
import subprocess
import sys
from pathlib import Path
from urllib.parse import urlsplit

from .common import (
    RuntimeConnectionFailure,
    RuntimeUnavailable,
    child_environment,
    explicit_executable,
    owned_client,
    stop_process,
)
from .runtime_setup import component_root

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
    "copilot": "https://docs.github.com/en/copilot/how-tos/copilot-sdk/setup",
    "claude_agent": "https://code.claude.com/docs/en/agent-sdk/quickstart",
    "claude_code": "https://code.claude.com/docs/en/legal-and-compliance",
    "gemini_cli": "https://geminicli.com/docs/get-started/installation/",
    "grok_build": "https://docs.x.ai/build/overview",
}
COPILOT_CLI_VERSION = "1.0.83"


def copilot_runtime(home: Path) -> Path:
    # These helpers read the SDK publisher's pinned release/platform manifest;
    # they do not invoke the CLI, search PATH or download a runtime.
    from copilot._cli_version import CLI_VERSION, RUNTIME_PLATFORMS

    name = "copilot-runtime.exe" if os.name == "nt" else "copilot-runtime"
    runtime_platform = RUNTIME_PLATFORMS.get((sys.platform, platform.machine()))
    if sys.platform == "linux" and any(Path("/lib").glob("ld-musl-*.so.1")):
        runtime_platform = {"linux-x64": "linuxmusl-x64", "linux-arm64": "linuxmusl-arm64"}.get(runtime_platform)
    if not runtime_platform or not CLI_VERSION:
        raise RuntimeUnavailable("The pinned Copilot package does not include this operating system and processor.")
    path = component_root() / "runtime" / CLI_VERSION / "prebuilds" / runtime_platform / name
    if not all(p.is_file() for p in (path, path.parent / "runtime.node", path.parent / ".hostless-runtime-assets-v2")):
        raise RuntimeUnavailable("Install the pinned Copilot runtime for this connection first.")
    return path


def copilot_account_command(home, connection):
    entry = explicit_executable(connection)
    if entry is None:
        package = component_root() / "account-client" / "node_modules" / "@github" / "copilot"
        manifest = package / "package.json"
        if not manifest.is_file():
            raise RuntimeUnavailable("Use Install client to install the original Copilot account client alongside its analysis runtime.")
        metadata = json.loads(manifest.read_text(encoding="utf-8"))
        raw = metadata.get("bin", {}).get("copilot")
        if metadata.get("name") != "@github/copilot" or metadata.get("version") != COPILOT_CLI_VERSION or not isinstance(raw, str):
            raise RuntimeUnavailable("The original Copilot client does not match the supported publisher manifest. Use Install client.")
        entry = (package / raw).resolve()
        if not entry.is_relative_to(package.resolve()) or not entry.is_file():
            raise RuntimeUnavailable("The original Copilot account client is missing its published entry point.")
    if entry.suffix.lower() in {".js", ".mjs", ".cjs"}:
        from .runtime_setup import node_executable
        node = node_executable(home, connection)
        if node is None:
            raise RuntimeUnavailable("Use Install client to install the supported Node runtime for the original account client.")
        return [str(node), str(entry)]
    return [str(entry)]


class AccountService:
    """Original client auth in this connection's owned worker process."""

    def __init__(self, connection, home):
        self.connection = connection
        self.home = home
        self.pending = None
        self.state = None

    def record(self, state, detail, **values):
        self.state = {"connection_id": self.connection["id"], "installed": True,
                      "state": state, "detail": detail, "login_url": None, "user_code": None,
                      "docs_url": DOCS.get(self.connection["protocol"]), **values}
        return self.state

    def _account_failure(self, operation, error, fallback):
        """Record only bounded exception structure and return safe UI copy."""
        try:
            record_exception("ai_account_operation_failed", error, phase="account",
                             operation=operation, protocol=self.connection.get("protocol"),
                             connection_id=self.connection.get("id"))
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
        if self.connection["protocol"] == "claude_code":
            return self.record("attended_only", "The original Claude Code client supports attended account use. Its subscription cannot run Quantix background work.")
        if self.connection["protocol"] == "claude_agent":
            return self.record("api_key_required", "This connection uses an Anthropic API key. Add its credentials before checking access.")
        if self.connection["auth_type"] == "client_login":
            return await self.saved_account_status()
        return self.record("installed", "The original client component is prepared. Sign-in and model access have not been confirmed.")

    async def saved_account_status(self):
        """Ask the original client about its saved account without starting login.

        Settled account workers close after each operation. A fresh worker must
        recover status through the client, rather than requiring another login
        or treating the presence of private cache files as confirmed access.
        """
        protocol = self.connection["protocol"]
        try:
            # Allow the Gemini metadata operation's 60-second deadline to close
            # its process before this outer account deadline cancels cleanup.
            async with asyncio.timeout(75):
                if protocol == "codex":
                    from openai_codex import AsyncCodex

                    from .codex import codex_config

                    async with owned_client(AsyncCodex(codex_config(self.home))) as client:
                        account = await client.account()
                        metadata = account.model_dump(mode="json", by_alias=True).get("account")
                    signed_in = isinstance(metadata, dict) and metadata.get("type") == "chatgpt"
                elif protocol == "copilot":
                    async with owned_client(self.copilot_client(), close_method="stop", force_method="force_stop") as client:
                        account = await client.get_auth_status()
                    signed_in = account.isAuthenticated is True
                elif protocol == "gemini_cli":
                    from .gemini_auth import cached_account_status

                    signed_in = await cached_account_status(self.home, self.connection)
                elif protocol == "grok_build":
                    from .grok_auth import cached_account_status
                    signed_in = await cached_account_status(self.home, self.connection)
                else:
                    return self.record("installed", "The original client component is prepared. Sign-in and model access have not been confirmed.")
        except (RuntimeUnavailable, RuntimeConnectionFailure) as error:
            return self.record("attention", self._account_failure(
                "status", error,
                "The original client could not refresh account access. Check the connection and try again."))
        except Exception as error:
            return self.record("attention", self._account_failure(
                "status", error,
                "The original client could not refresh saved account access. Check the connection and sign in again."))
        if signed_in:
            return self.record("signed_in", "The original client confirmed this connection's saved sign-in. Model access remains to be checked.")
        return self.record("sign_in_required", "The original client has no confirmed saved sign-in for this connection. Start sign-in to continue.")

    async def login(self):
        if self.pending and not self.pending.done():
            return await self.status()
        protocol = self.connection["protocol"]
        if protocol == "claude_agent":
            return self.record("api_key_required", "Add the Anthropic API key in this connection's credentials.")
        if protocol == "copilot":
            return await self.copilot_login()
        if protocol == "gemini_cli":
            from .gemini_auth import authenticate
            ready = asyncio.get_running_loop().create_future()
            self.pending = asyncio.create_task(authenticate(self, ready))
            return await asyncio.shield(ready)
        if protocol == "grok_build":
            from .grok_auth import authenticate
            ready = asyncio.get_running_loop().create_future()
            self.pending = asyncio.create_task(authenticate(self, ready))
            return await asyncio.shield(ready)
        if protocol != "codex":
            return self.record("attended_only", "Claude Code subscription access requires attended use of its original client. Choose Claude Agent with an API key for background work.")
        from openai_codex import AsyncCodex

        from .codex import codex_config

        ready = asyncio.get_running_loop().create_future()

        async def complete():
            handle = None
            try:
                async with owned_client(AsyncCodex(codex_config(self.home))) as client:
                    async with asyncio.timeout(45):
                        handle = await client.login_chatgpt()
                    value = self.record("login_pending", "Continue the original OpenAI sign-in in your browser.",
                                        login_url=handle.auth_url)
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
                self.record("signed_in" if result.success else "sign_in_required",
                            "OpenAI confirmed sign-in. Model access remains to be checked." if result.success
                            else "OpenAI did not complete sign-in. Start sign-in again.")
            except asyncio.CancelledError:
                self.record("sign_in_required", "The pending sign-in was cancelled.")
                if not ready.done():
                    ready.cancel()
                raise
            except (RuntimeUnavailable, RuntimeConnectionFailure) as error:
                value = self.record("attention", self._account_failure(
                    "login", error,
                    "The original OpenAI sign-in could not start. Check the connection and try again."))
                if not ready.done():
                    ready.set_result(value)
            except Exception as error:
                value = self.record("attention", self._account_failure(
                    "login", error,
                    "The original OpenAI sign-in could not start or finish. Check the connection and start sign-in again."))
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
                    if (not parsed or parsed.scheme != "https" or parsed.hostname not in
                            {"auth.openai.com", "chatgpt.com", "auth.chatgpt.com"}
                            or parsed.port not in {None, 443} or parsed.username or parsed.password
                            or len(url) > 5000 or any(ord(char) < 33 for char in url)
                            or not isinstance(code, str) or not re.fullmatch(r"[A-Za-z0-9-]{1,128}", code)):
                        raise RuntimeUnavailable("The original OpenAI client returned an unsupported device sign-in address or code.")
                    value = self.record("login_pending", "Open the official OpenAI address and confirm this one-time code.",
                                        login_url=url, user_code=code)
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
                self.record("signed_in" if result.success else "sign_in_required",
                            "OpenAI confirmed sign-in. Model access remains to be checked." if result.success
                            else "OpenAI did not complete device sign-in. Start sign-in again.")
            except asyncio.CancelledError:
                self.record("sign_in_required", "The pending device sign-in was cancelled.")
                if not ready.done():
                    ready.cancel()
                raise
            except (RuntimeUnavailable, RuntimeConnectionFailure) as error:
                value = self.record("attention", self._account_failure(
                    "device_login", error,
                    "The original OpenAI device sign-in could not start. Check the connection and try again."))
                if not ready.done():
                    ready.set_result(value)
            except Exception as error:
                value = self.record("attention", self._account_failure(
                    "device_login", error,
                    "The original OpenAI device sign-in could not start or finish. Check the connection and start sign-in again."))
                if not ready.done():
                    ready.set_result(value)

        self.pending = asyncio.create_task(complete())
        return await asyncio.shield(ready)

    def copilot_client(self):
        from copilot import CopilotClient, RuntimeConnection
        return CopilotClient(
            connection=RuntimeConnection.for_stdio(path=str(copilot_runtime(self.home))),
            working_directory=str(self.home), base_directory=str(self.home / "copilot"),
            env=child_environment(self.home), use_logged_in_user=True,
            enable_remote_sessions=False, log_level="none",
        )

    async def copilot_login(self):
        if self.connection["auth_type"] != "client_login":
            return self.record("api_key_required", "Add the GitHub token in this connection's credentials.")
        ready = asyncio.get_running_loop().create_future()
        from .browser_environment import sign_in_browser_environment
        login_environment = sign_in_browser_environment(child_environment(self.home),
            account_key="COPILOT_HOME", account_home=self.home / "copilot")
        command = [*copilot_account_command(self.home, self.connection), "login", "--device-code", "--host", "https://github.com"]
        work = self.home / "account"
        work.mkdir(parents=True, exist_ok=True)

        async def complete():
            process = None
            try:
                async with asyncio.timeout(900):
                    process = await asyncio.create_subprocess_exec(
                        *command, cwd=work, env={key: value for key, value in login_environment.items() if value},
                        stdin=asyncio.subprocess.DEVNULL, stdout=asyncio.subprocess.PIPE,
                        stderr=asyncio.subprocess.STDOUT,
                        **({"creationflags": subprocess.CREATE_NO_WINDOW} if os.name == "nt" else {}),
                    )
                    text = ""
                    total = 0
                    while chunk := await process.stdout.read(4096):
                        total += len(chunk)
                        if total > 1024 * 1024:
                            raise RuntimeUnavailable("The original account client exceeded its bounded sign-in output.")
                        text = (text + chunk.decode("utf-8", errors="replace"))[-12000:]
                        text = re.sub(r"\x1b\[[0-?]*[ -/]*[@-~]", "", text)
                        # Only the documented public URL and short user code
                        # cross the worker boundary, never raw account output.
                        code = re.search(r"\b[A-Z0-9]{4}-[A-Z0-9]{4}\b", text, re.IGNORECASE)
                        if "https://github.com/login/device" in text and code and not ready.done():
                            ready.set_result(self.record("login_pending", "Enter this one-time code in GitHub to connect Copilot.",
                                                         login_url="https://github.com/login/device", user_code=code.group().upper()))
                    await process.wait()
                    if process.returncode:
                        raise RuntimeUnavailable("The original Copilot sign-in did not complete.")
                    async with owned_client(self.copilot_client(), close_method="stop", force_method="force_stop") as client:
                        status = await client.get_auth_status()
                    value = self.record("signed_in" if status.isAuthenticated else "sign_in_required",
                                        "Copilot confirmed sign-in. Model access remains to be checked." if status.isAuthenticated
                                        else "Copilot did not confirm this account's sign-in. Start sign-in again.")
                    if not ready.done():
                        ready.set_result(value)
            except asyncio.CancelledError:
                self.record("sign_in_required", "The pending Copilot sign-in was cancelled.")
                if not ready.done():
                    ready.cancel()
                raise
            except Exception:
                value = self.record("sign_in_required", "The original Copilot sign-in expired or could not finish. Start sign-in again.")
                if not ready.done():
                    ready.set_result(value)
            finally:
                await stop_process(process)

        self.pending = asyncio.create_task(complete())
        try:
            async with asyncio.timeout(45):
                return await asyncio.shield(ready)
        except TimeoutError:
            self.pending.cancel()
            await asyncio.gather(self.pending, return_exceptions=True)
            return self.record("sign_in_required", "Copilot did not provide its sign-in code. Start sign-in again.")

    async def logout(self):
        if self.pending and not self.pending.done():
            self.pending.cancel()
            await asyncio.gather(self.pending, return_exceptions=True)
        if self.connection["protocol"] == "grok_build":
            from .grok_auth import logout
            signed_out = await logout(self.home, self.connection)
            return self.record("signed_out" if signed_out else "attention",
                               "Grok confirmed this connection is signed out." if signed_out
                               else "Grok still reports account access. Sign-out has not been confirmed.")
        if self.connection["protocol"] == "codex":
            from openai_codex import AsyncCodex

            from .codex import codex_config
            async with asyncio.timeout(30):
                async with owned_client(AsyncCodex(codex_config(self.home))) as client:
                    await client.logout()
            return self.record("signed_out", "The original Codex client cleared this connection's sign-in.")
        if self.connection["protocol"] == "claude_agent":
            return self.record("api_key_managed", "Remove the saved API key in connection settings to disconnect this account.")
        if self.connection["protocol"] == "copilot":
            if self.connection["auth_type"] != "client_login":
                return self.record("api_key_managed", "Remove this connection's saved GitHub token to disconnect it.")
            from copilot.generated.rpc import AccountLogoutRequest
            async with asyncio.timeout(30):
                async with owned_client(self.copilot_client(), close_method="stop", force_method="force_stop") as client:
                    await client.rpc.account.logout(AccountLogoutRequest())
                    status = await client.get_auth_status()
            return self.record("sign_in_required" if status.isAuthenticated else "signed_out",
                               "Copilot still reports account access; review the original client's account controls." if status.isAuthenticated
                               else "Copilot confirmed this connection is signed out.")
        return self.record("attention", "Manage this subscription account in its original attended client. Quantix has not confirmed sign-out.")

    async def close(self):
        if self.pending and not self.pending.done():
            self.pending.cancel()
            await asyncio.gather(self.pending, return_exceptions=True)
        # The native component host owns every descendant and terminates any
        # remaining process tree when the worker transport closes.
