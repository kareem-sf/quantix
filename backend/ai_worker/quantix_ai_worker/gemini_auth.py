"""Gemini's documented ACP account and metadata operations, without prompts.

The original CLI opens the Google browser and owns its OAuth callback/cache.
Quantix neither implements Google OAuth nor extracts the CLI's tokens.
"""

import asyncio
import json
import os
import re
import subprocess
from contextlib import asynccontextmanager
from uuid import uuid4

from .common import RuntimeUnavailable, child_environment, stop_process
from .gemini import GEMINI_VERSION, _require_local_policy_authority, gemini_command


class GeminiSignInRequired(RuntimeUnavailable):
    """The original client's managed Google sign-in cache is absent."""


@asynccontextmanager
async def acp_session(home, connection, *, allow_browser=False):
    _require_local_policy_authority()
    # Pinned CLI Storage.getOAuthCredsPath() uses GEMINI_CLI_HOME/.gemini.
    # Only the original CLI reads or refreshes these credentials. Presence is a
    # prerequisite for an unattended attempt, never proof of a signed-in account.
    if not allow_browser and not (home / ".gemini" / "oauth_creds.json").is_file():
        raise GeminiSignInRequired("Sign in to this Gemini connection before checking its account or discovering models.")
    process = None
    stderr = None
    work = home / "account" / uuid4().hex
    work.mkdir(parents=True, exist_ok=True)
    settings = work / "account-settings.json"
    settings.write_text(json.dumps({
        "general": {"enableAutoUpdate": False, "enableAutoUpdateNotification": False},
        "tools": {"core": []}, "mcp": {"allowed": []}, "mcpServers": {},
        "telemetry": {"enabled": False, "logPrompts": False},
        "admin": {"secureModeEnabled": True, "extensions": {"enabled": False}, "skills": {"enabled": False}},
        "hooksConfig": {"enabled": False}, "experimental": {"autoMemory": False},
        "context": {"fileName": [], "includeDirectories": []},
        "security": {"auth": {"selectedType": "oauth-personal"}},
    }), encoding="utf-8")
    try:
        env = child_environment(home)
        env["GEMINI_CLI_HOME"] = str(home)
        env["GEMINI_CLI_SURFACE"] = "quantix"
        env["GEMINI_CLI_SYSTEM_SETTINGS_PATH"] = str(settings)
        if allow_browser:
            from .browser_environment import sign_in_browser_environment
            env = sign_in_browser_environment(env, account_key="GEMINI_CLI_HOME", account_home=home)
            for key in ("DISPLAY", "WAYLAND_DISPLAY", "DBUS_SESSION_BUS_ADDRESS", "XDG_RUNTIME_DIR", "XAUTHORITY", "USER", "LOGNAME"):
                if os.environ.get(key):
                    env[key] = os.environ[key]
        else:
            # Saved-account checks and discovery must not open a browser. ACP
            # can fall back to manual auth when cached credentials fail; such
            # non-protocol output is rejected below and the process is stopped.
            env["NO_BROWSER"] = "true"
        process = await asyncio.create_subprocess_exec(
            *gemini_command(home, connection), "--acp", cwd=work,
            env={key: value for key, value in env.items() if value},
            stdin=asyncio.subprocess.PIPE, stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE, limit=1024 * 1024,
            **({"creationflags": subprocess.CREATE_NO_WINDOW} if os.name == "nt" else {}),
        )

        async def discard_diagnostics():
            while await process.stderr.read(4096):
                pass

        stderr = asyncio.create_task(discard_diagnostics())
        total = 0

        async def request(identifier, method, params):
            nonlocal total
            data = {"jsonrpc": "2.0", "id": identifier, "method": method, "params": params}
            process.stdin.write((json.dumps(data) + "\n").encode("utf-8"))
            await process.stdin.drain()
            while line := await process.stdout.readline():
                total += len(line)
                if total > 4 * 1024 * 1024:
                    raise RuntimeUnavailable("The original Gemini account stream exceeded its supported size.")
                try:
                    message = json.loads(line)
                except (ValueError, UnicodeDecodeError):
                    raise RuntimeUnavailable("The original Gemini client could not complete the saved-account operation. Refresh account access or sign in again.") from None
                if not isinstance(message, dict) or message.get("jsonrpc") != "2.0":
                    raise RuntimeUnavailable("The original Gemini client returned an invalid account response.")
                if "method" in message and "id" in message:
                    denial = {"jsonrpc": "2.0", "id": message["id"],
                              "error": {"code": -32601, "message": "Only account and model metadata operations are available."}}
                    process.stdin.write((json.dumps(denial) + "\n").encode("utf-8"))
                    await process.stdin.drain()
                    continue
                if message.get("id") != identifier:
                    continue
                if "error" in message or "result" not in message:
                    raise RuntimeUnavailable("The original Gemini client could not confirm account access. Check the connection and refresh; sign in again if access has expired.")
                return message["result"]
            raise RuntimeUnavailable("The original Gemini account connection closed before completion.")

        async with asyncio.timeout(45):
            initialized = await request(1, "initialize", {
                "protocolVersion": 1,
                "clientInfo": {"name": "quantix", "title": "Quantix Tender Office", "version": "1.0.0"},
                "clientCapabilities": {"fs": {"readTextFile": False, "writeTextFile": False}, "terminal": False},
            })
        if (not isinstance(initialized, dict) or initialized.get("protocolVersion") != 1
                or initialized.get("agentInfo", {}).get("version") != GEMINI_VERSION
                or not any(method.get("id") == "oauth-personal" for method in initialized.get("authMethods", []))):
            raise RuntimeUnavailable("This Gemini component does not expose the supported Google account interface.")
        yield request, initialized, work
    finally:
        if process and process.stdin:
            process.stdin.close()
            try:
                await asyncio.wait_for(process.wait(), 3)
            except (TimeoutError, ProcessLookupError):
                pass
        await stop_process(process)
        if stderr:
            stderr.cancel()
            await asyncio.gather(stderr, return_exceptions=True)
        settings.unlink(missing_ok=True)


async def authenticate(account, ready):
    try:
        async with asyncio.timeout(360):
            async with acp_session(account.home, account.connection, allow_browser=True) as (request, _initialized, _work):
                pending = account.record("login_pending", "Gemini is opening its Google sign-in in your browser. Complete that account step to continue.")
                if not ready.done():
                    ready.set_result(pending)
                # The pinned dispatcher accepts methodId. Its response follows
                # the original CLI's successful refreshAuth and cache update.
                await request(2, "authenticate", {"methodId": "oauth-personal"})
            account.record("signed_in", "The original Gemini client confirmed Google sign-in. Model access remains to be checked.")
    except asyncio.CancelledError:
        account.record("sign_in_required", "The pending Google sign-in was cancelled.")
        if not ready.done():
            ready.cancel()
        raise
    except Exception:
        value = account.record("sign_in_required", "The original Google sign-in expired or could not finish. Start sign-in again.")
        if not ready.done():
            ready.set_result(value)


async def cached_session_metadata(home, connection):
    if connection["auth_type"] != "client_login":
        raise RuntimeUnavailable("Gemini subscription model discovery requires this original client's managed Google sign-in.")
    async with asyncio.timeout(60):
        async with acp_session(home, connection) as (request, _initialized, work):
            # session/new returns the original client's model metadata. There is
            # no session/prompt; auto memory, external tools and hooks are off.
            session = await request(2, "session/new", {"cwd": str(work), "mcpServers": []})
            if (not isinstance(session, dict) or not isinstance(session.get("sessionId"), str)
                    or not session["sessionId"] or not isinstance(session.get("models"), dict)):
                raise RuntimeUnavailable("The original Gemini client returned invalid account metadata.")
            entries = session["models"].get("availableModels")
            if not isinstance(entries, list) or len(entries) > 2000:
                raise RuntimeUnavailable("The original Gemini client returned an invalid model catalog.")
            return session


async def cached_account_status(home, connection):
    try:
        # The pinned ACP session manager returns metadata only after refreshAuth
        # succeeds. OAuth validates saved credentials with Google's getTokenInfo;
        # no inference prompt is sent to establish this account status.
        await cached_session_metadata(home, connection)
    except GeminiSignInRequired:
        return False
    return True


async def discover_models(home, connection):
    session = await cached_session_metadata(home, connection)
    result = {}
    for entry in session["models"]["availableModels"]:
        identifier = entry.get("modelId") if isinstance(entry, dict) else None
        # Automatic routing aliases cannot establish an exact approved
        # model identity for a Tender. Keep only published model IDs.
        if not isinstance(identifier, str) or not re.fullmatch(r"gemini-[0-9][A-Za-z0-9._-]*", identifier):
            continue
        result[identifier] = {"model_id": identifier, "display_name": entry.get("name") or identifier,
                              "capabilities": {"web_search": False, "reasoning": []}}
    if not result:
        raise RuntimeUnavailable("Gemini did not list an exact model for this account. Its automatic routing options cannot be used for Tender work.")
    return list(result.values())
