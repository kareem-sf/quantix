"""Official Grok ACP account, model and subscription metadata operations.

Only public account status and the documented billing fields cross the worker
boundary. Never call getBearerToken/getApiKey or read the client's auth store.
"""

import asyncio
import json
import math
import re
from contextlib import asynccontextmanager
from datetime import UTC, datetime
from decimal import Decimal
from urllib.parse import urlsplit
from uuid import uuid4

from .common import RuntimeConnectionFailure, RuntimeUnavailable
from .grok_common import (
    GROK_VERSION,
    MODEL_PATTERN,
    account_lock,
    close_process,
    configure,
    discard,
    environment,
    grok_command,
    spawn,
)


class GrokSignInRequired(RuntimeUnavailable):
    pass


_AUTO_TOPUP_RESPONSE_FIELDS = frozenset({"rule"})
_AUTO_TOPUP_RULE_FIELDS = frozenset({
    "enabled", "minBeforeHittingSl", "topupAmount", "maxAmountPerMonth",
})


def _failure(error):
    # Classify bounded protocol errors without returning potentially private
    # provider text, URLs, identifiers or token fragments.
    code = error.get("code") if isinstance(error, dict) else None
    if code == -32601:
        return RuntimeUnavailable("Grok did not recognize an account operation (protocol -32601). Prepare its Quantix component and retry.")
    if code == -32602:
        return RuntimeUnavailable("Grok rejected an account operation's parameters (protocol -32602). Prepare its Quantix component and retry.")
    text = json.dumps(error, ensure_ascii=True)[:16000].lower()
    if re.search(r"\b403\b|permission_denied|entitlement|subscription.*required", text):
        return RuntimeUnavailable("Grok could not grant this account access. Its subscription or selected model may not be eligible; signing in again does not change its entitlement.")
    if re.search(r"\b401\b|auth_required|authentication required|no cached auth|session expired", text):
        return GrokSignInRequired("Grok requires this connection to sign in again.")
    if re.search(r"\b429\b|quota|credit.*exhaust|usage.*limit|rate.limit", text):
        return RuntimeUnavailable("Grok has reached an account usage limit. Review its subscription allowance before retrying.")
    if re.search(r"\b(?:408|500|502|503|504)\b|timed out|timeout", text):
        return RuntimeConnectionFailure("The Grok account service is temporarily unavailable.")
    return RuntimeUnavailable("Grok could not complete this account operation. Review the connection and its model access.")


class ACP:
    """One bounded reader routes concurrent auth and get_url responses by id."""

    def __init__(self, process):
        self.process = process
        self.pending = {}
        self.identifier = 0
        self.write_lock = asyncio.Lock()
        self.reader = asyncio.create_task(self._read())

    async def _send(self, message):
        async with self.write_lock:
            self.process.stdin.write((json.dumps(message, ensure_ascii=False) + "\n").encode())
            await self.process.stdin.drain()

    async def _read(self):
        failure = RuntimeConnectionFailure("The original Grok account connection closed before completion.")
        total = 0
        try:
            while line := await self.process.stdout.readline():
                total += len(line)
                if total > 16 * 1024 * 1024:
                    raise RuntimeUnavailable("Grok exceeded its bounded account metadata stream.")
                value = json.loads(line)
                if not isinstance(value, dict) or value.get("jsonrpc") != "2.0":
                    raise RuntimeUnavailable("Grok returned an invalid account protocol response.")
                if "method" in value:
                    if "id" in value:
                        await self._send({"jsonrpc": "2.0", "id": value["id"], "error": {
                            "code": -32601, "message": "Only account and model metadata operations are available."}})
                    continue
                future = self.pending.get(value.get("id"))
                if future is None or future.done():
                    continue
                if "error" in value:
                    future.set_exception(_failure(value["error"]))
                elif "result" not in value:
                    future.set_exception(RuntimeUnavailable("Grok returned no account result."))
                else:
                    future.set_result(value["result"])
        except asyncio.CancelledError:
            raise
        except (ValueError, UnicodeDecodeError):
            failure = RuntimeUnavailable("Grok returned an unsupported account metadata stream.")
        except Exception as error:
            failure = error if isinstance(error, RuntimeUnavailable) else RuntimeConnectionFailure("The Grok account connection was interrupted.")
        finally:
            for future in self.pending.values():
                if not future.done():
                    future.set_exception(failure)

    async def request(self, method, params=None, *, timeout=45):
        if self.reader.done():
            raise RuntimeConnectionFailure("The Grok account connection is closed.")
        self.identifier += 1
        identifier = self.identifier
        future = asyncio.get_running_loop().create_future()
        self.pending[identifier] = future
        try:
            # Grok uses agent-client-protocol 0.10.4: its ext_method writer
            # prefixes '_' on the wire and the receiver removes that prefix
            # before dispatching the logical x.ai/... extension name.
            wire_method = "_" + method if method.startswith("x.ai/") else method
            await self._send({"jsonrpc": "2.0", "id": identifier, "method": wire_method, "params": params or {}})
            async with asyncio.timeout(timeout):
                return await future
        finally:
            self.pending.pop(identifier, None)
            if not future.done():
                future.cancel()

    async def close(self):
        self.reader.cancel()
        await asyncio.gather(self.reader, return_exceptions=True)


@asynccontextmanager
async def acp_session(home, connection, *, allow_browser=False):
    """Caller owns the connection lock and has installed its private config."""
    work = home / "account" / uuid4().hex
    work.mkdir(parents=True, exist_ok=True)
    process = None
    diagnostics = None
    client = None
    try:
        process = await spawn([*grok_command(connection), "agent", "--no-leader", "stdio"],
                              work, environment(home, allow_browser=allow_browser))
        diagnostics = asyncio.create_task(discard(process.stderr))
        client = ACP(process)
        initialized = await client.request("initialize", {
            "protocolVersion": 1,
            "clientInfo": {"name": "quantix", "title": "Quantix Tender Office", "version": "1.0.0"},
            "clientCapabilities": {"fs": {"readTextFile": False, "writeTextFile": False}, "terminal": False},
            "_meta": {"clientIdentifier": "quantix"},
        })
        meta = initialized.get("_meta") if isinstance(initialized, dict) else None
        meta = meta if isinstance(meta, dict) else {}
        info = initialized.get("agentInfo") if isinstance(initialized, dict) else None
        info = info if isinstance(info, dict) else {}
        version = meta.get("agentVersion") or info.get("version")
        if (not isinstance(initialized, dict) or initialized.get("protocolVersion") != 1
                or version != GROK_VERSION or meta.get("grokShell") is not True):
            raise RuntimeUnavailable("The installed Grok Build does not expose the supported account interface.")
        yield client, initialized
    finally:
        try:
            if client:
                if allow_browser:
                    try:
                        await client.request("x.ai/auth/cancel", timeout=3)
                    except Exception:
                        pass
                await client.close()
            await close_process(process)
        finally:
            if diagnostics:
                diagnostics.cancel()
                await asyncio.gather(diagnostics, return_exceptions=True)
            try:
                work.rmdir()
            except OSError:
                pass


def _require_session(initialized):
    meta = initialized.get("_meta") or {}
    if meta.get("defaultAuthMethodId") != "cached_token":
        raise GrokSignInRequired("Sign in through this Quantix Grok connection before checking its account or model access.")


async def _check_access(client, initialized):
    _require_session(initialized)
    checked = await client.request("x.ai/auth/check_subscription")
    if not isinstance(checked, dict) or checked.get("authenticated") is not True:
        raise GrokSignInRequired("Grok did not confirm this connection's saved sign-in.")
    meta = checked.get("meta")
    if not isinstance(meta, dict) or meta.get("gate"):
        raise RuntimeUnavailable("Grok has restricted this account's Build access. Review the subscription entitlement in the original account.")


def _date(value):
    if not isinstance(value, str) or len(value) > 80:
        return None
    try:
        parsed = datetime.fromisoformat(value.replace("Z", "+00:00"))
        return parsed.astimezone(UTC) if parsed.tzinfo is not None else None
    except ValueError:
        return None


def _money(value):
    # In the official Cent type an existing {} is a zero amount; an absent
    # object remains unknown. int64 proto JSON may encode val as a string.
    if not isinstance(value, dict):
        return None
    raw = value.get("val", 0)
    if type(raw) not in {int, str} or not re.fullmatch(r"[0-9]{1,18}", str(raw)):
        return None
    return format(Decimal(str(raw)) / 100, ".2f")


def _auto_topup_enabled(topup):
    """Normalize the official optional auto-top-up wrapper fail-closed."""
    if not isinstance(topup, dict) or not set(topup).issubset(_AUTO_TOPUP_RESPONSE_FIELDS):
        return None
    rule = topup.get("rule")
    if rule is None:
        # GetAutoTopupRuleResponse.rule is Option<AutoTopupRule>. xAI returns
        # an omitted/null option when no automatic top-up rule is configured.
        return False
    if not isinstance(rule, dict) or not set(rule).issubset(_AUTO_TOPUP_RULE_FIELDS):
        return None
    enabled = rule.get("enabled", False)
    return enabled if type(enabled) is bool else None


def normalize_billing(billing, topup):
    now = datetime.now(UTC)
    config = billing.get("config") if isinstance(billing, dict) else None
    config = config if isinstance(config, dict) else {}
    period = config.get("currentPeriod")
    has_current_period = period is not None
    period = period if isinstance(period, dict) else {}
    # Never combine a weekly numerator/reset with a legacy monthly allowance.
    start = _date(period.get("start") if has_current_period else config.get("billingPeriodStart"))
    end = _date(period.get("end") if has_current_period else config.get("billingPeriodEnd"))
    used = config.get("creditUsagePercent")
    used = float(used) if type(used) in {int, float} and math.isfinite(used) and 0 <= used <= 100 else None
    if used is None and not has_current_period and config.get("creditUsagePercent") is None:
        limit, spent = _money(config.get("monthlyLimit")), _money(config.get("used"))
        if limit is not None and spent is not None and Decimal(limit) > 0 and Decimal(spent) <= Decimal(limit):
            used = float(Decimal(spent) * 100 / Decimal(limit))
    # The official GetAutoTopupRuleResponse is a typed wrapper with an
    # optional `rule`.  xAI's proto JSON omits false scalar fields and can
    # omit the optional rule altogether when no automatic top-up is
    # configured.  A successful wrapper with no rule is therefore an
    # explicit disabled/no-rule observation, while a non-object `rule` is a
    # malformed response and must remain unknown.
    auto = _auto_topup_enabled(topup)
    prepaid, cap, on_demand_used = (_money(config.get(name)) for name in ("prepaidBalance", "onDemandCap", "onDemandUsed"))
    current = start is not None and end is not None and start <= now < end
    allowed = (current and used is not None and used < 100 and prepaid == "0.00" and cap == "0.00" and auto is False)
    detail = ("The current included allowance is available; no prepaid balance, on-demand allowance or enabled automatic top-up was reported. Account-wide usage may change elsewhere."
              if allowed else "Included-only work is paused because the allowance is exhausted or current billing, prepaid, on-demand or automatic top-up information does not establish included-only access.")
    tier = billing.get("subscription_tier") if isinstance(billing, dict) else None
    return {"fetched_at": now.isoformat(), "subscription_tier": tier[:120] if isinstance(tier, str) else None,
            "used_percent": used, "period_type": period["type"][:100] if isinstance(period.get("type"), str) else None,
            "period_start": start.isoformat() if start else None, "period_end": end.isoformat() if end else None,
            "prepaid_balance_usd": prepaid, "on_demand_cap_usd": cap, "on_demand_used_usd": on_demand_used,
            "auto_topup_enabled": auto, "included_only_allowed": bool(allowed), "detail": detail}


async def billing_in_session(client, initialized):
    await _check_access(client, initialized)
    billing = await client.request("x.ai/billing")
    topup = await client.request("x.ai/auto-topup-rule")
    if not isinstance(billing, dict) or not isinstance(topup, dict):
        raise RuntimeUnavailable("Grok returned incomplete subscription metadata. No request was started.")
    return normalize_billing(billing, topup)


async def subscription_usage(home, connection):
    async with account_lock(home, timeout=60):
        configure(home)
        async with acp_session(home, connection) as (client, initialized):
            return await billing_in_session(client, initialized)


async def authenticated_account_status(home, connection):
    """Confirm the cached vendor sign-in without reading billing metadata.

    Account discovery is an authentication operation. Billing and top-up
    endpoints are a separate, optional usage observation and may be
    unavailable even when the saved account can authenticate and use the
    original client.
    """
    async with account_lock(home, timeout=60):
        configure(home)
        async with acp_session(home, connection) as (client, initialized):
            await _check_access(client, initialized)


async def cached_account_status(home, connection):
    try:
        await authenticated_account_status(home, connection)
    except GrokSignInRequired:
        return False
    return True


async def models_in_session(client):
    reply = await client.request("x.ai/models/list")
    if not isinstance(reply, dict) or reply.get("error") or not isinstance(reply.get("result"), dict):
        raise RuntimeUnavailable("Grok returned an invalid model catalog.")
    entries = reply["result"].get("availableModels")
    if not isinstance(entries, list) or len(entries) > 2000:
        raise RuntimeUnavailable("Grok returned an unsupported model catalog size.")
    result = {}
    for entry in entries:
        identifier = entry.get("modelId") if isinstance(entry, dict) else None
        if not isinstance(identifier, str) or not re.fullmatch(MODEL_PATTERN, identifier) or identifier.lower() in {"auto", "default", "automatic"}:
            continue
        meta = entry.get("_meta") or {}
        if not isinstance(meta, dict):
            continue
        context = meta.get("totalContextTokens")
        efforts = meta.get("reasoningEfforts")
        levels = []
        if meta.get("supportsReasoningEffort") is True and isinstance(efforts, list):
            for item in efforts[:30]:
                value = (item.get("id") or item.get("value")) if isinstance(item, dict) else None
                if isinstance(value, str) and re.fullmatch(r"[a-z][a-z0-9_-]{0,39}", value):
                    levels.append(value)
        name = entry.get("name")
        result[identifier] = {"model_id": identifier, "display_name": name[:300] if isinstance(name, str) and name.strip() else identifier,
                              "capabilities": {"context_window": context if type(context) is int and context > 0 else None,
                                               "reasoning": list(dict.fromkeys(levels)), "web_search": False}}
    if not result:
        raise RuntimeUnavailable("Grok did not return an exact available model for this account.")
    return list(result.values())


async def discover_models(home, connection):
    async with account_lock(home, timeout=60):
        configure(home)
        async with acp_session(home, connection) as (client, initialized):
            await _check_access(client, initialized)
            return await models_in_session(client)


def check_billing_permission(snapshot, connection):
    if snapshot["included_only_allowed"]:
        return
    if connection.get("_operation") == "check":
        # A private dollar value alone does not establish an enforceable charge
        # ceiling for this subscription. Root currently supplies no such proof.
        raise RuntimeUnavailable("Grok cannot currently provide a bounded paid connection check. Use included-only account access before checking this model.")
    if connection.get("settings", {}).get("allow_provider_managed_extras") is not True:
        raise RuntimeUnavailable(snapshot["detail"])


async def authenticate(account, ready):
    client = None
    login_task = None
    try:
        async with asyncio.timeout(900):
            async with account_lock(account.home, timeout=120):
                configure(account.home)
                async with acp_session(account.home, account.connection, allow_browser=True) as (client, initialized):
                    if not any(isinstance(method, dict) and method.get("id") == "grok.com" for method in initialized.get("authMethods", [])):
                        raise RuntimeUnavailable("Grok does not offer its supported browser sign-in on this connection.")
                    pending = account.record("login_pending", "Grok is preparing its official account sign-in. Complete the browser step when it opens.")
                    if not ready.done():
                        ready.set_result(pending)
                    login_task = asyncio.create_task(client.request("authenticate", {
                        "methodId": "grok.com", "_meta": {"headless": False, "force_interactive": True, "use_oauth": True},
                    }, timeout=840))
                    # get_url may initially return null until authenticate enters
                    # the original client's interactive flow. It never exposes tokens.
                    for _ in range(40):
                        if login_task.done():
                            break
                        link = await client.request("x.ai/auth/get_url", timeout=45)
                        url = link.get("auth_url") if isinstance(link, dict) else None
                        if isinstance(url, str):
                            parsed = urlsplit(url)
                            if (parsed.scheme != "https" or parsed.hostname not in {"auth.x.ai", "accounts.x.ai"}
                                    or parsed.port not in {None, 443} or parsed.username or parsed.password or len(url) > 5000
                                    or any(ord(char) < 33 for char in url)):
                                raise RuntimeUnavailable("Grok returned an unsupported sign-in address.")
                            account.record("login_pending", "Continue the official Grok sign-in in your browser.", login_url=url)
                            break
                        await asyncio.sleep(0.1)
                    result = await login_task
                    if not isinstance(result, dict) or not isinstance(result.get("_meta"), dict):
                        raise RuntimeUnavailable("Grok did not confirm the completed sign-in.")
                    if result["_meta"].get("gate"):
                        account.record("attention", "Grok confirmed sign-in but this account does not have Build access. Review its subscription entitlement.")
                    else:
                        account.record("signed_in", "Grok confirmed this connection's sign-in. Model access and included usage remain to be checked.")
    except asyncio.CancelledError:
        account.record("attention", "The pending Grok sign-in was cancelled. Existing saved sign-in has not been cleared.")
        if not ready.done():
            ready.cancel()
        raise
    except Exception as error:
        detail = str(error) if isinstance(error, RuntimeUnavailable) else "The original Grok sign-in did not finish. Existing sign-in has not been cleared; retry the account step."
        value = account.record("attention", detail)
        if not ready.done():
            ready.set_result(value)
    finally:
        if login_task:
            if not login_task.done():
                login_task.cancel()
            await asyncio.gather(login_task, return_exceptions=True)


async def logout(home, connection):
    async with account_lock(home, timeout=60):
        configure(home)
        async with acp_session(home, connection) as (client, _initialized):
            await client.request("x.ai/auth/logout")
        # A fresh process reloads the original client's own credential state.
        async with acp_session(home, connection) as (_client, initialized):
            return initialized.get("_meta", {}).get("defaultAuthMethodId") != "cached_token"


async def authenticate_device(account, ready):
    """Parse the original CLI's anchored stderr display, never OAuth responses.

    The pinned device_code::prompt_and_poll formatter prints the verification
    address after its URL marker and an ASCII alphanumeric/hyphen code after
    one of two code markers. It does not promise a four-by-four code shape.
    """
    process = None
    stdout = None

    async def release():
        await close_process(process)

    try:
        async with asyncio.timeout(900):
            async with account_lock(account.home, timeout=120, before_release=release):
                configure(account.home)
                work = account.home / "account" / uuid4().hex
                work.mkdir(parents=True, exist_ok=True)
                process = await spawn([*grok_command(account.connection), "login", "--device-auth"],
                                      work, environment(account.home, allow_browser=True), limit=64 * 1024)
                stdout = asyncio.create_task(discard(process.stdout))
                value = account.record("login_pending", "Grok is preparing its official sign-in code.")
                if not ready.done():
                    ready.set_result(value)
                total = 0
                stage = None
                login_url = None
                user_code = None
                while True:
                    async with asyncio.timeout(840 if user_code else 45):
                        line = await process.stderr.readline()
                    if not line:
                        break
                    total += len(line)
                    if total > 2 * 1024 * 1024:
                        raise RuntimeUnavailable("The original Grok sign-in exceeded its bounded display stream.")
                    text = re.sub(r"\x1b\[[0-?]*[ -/]*[@-~]", "", line.decode("utf-8", errors="replace")).strip()
                    if text == "To sign in, open this URL in your browser:":
                        stage = "url"
                        continue
                    if text in {"Confirm this code in your browser:", "Then enter this code:"}:
                        stage = "code"
                        continue
                    if not text:
                        continue
                    if stage == "url":
                        parsed = urlsplit(text)
                        if (parsed.scheme != "https" or parsed.hostname not in {"auth.x.ai", "accounts.x.ai"}
                                or parsed.port not in {None, 443} or parsed.username or parsed.password
                                or len(text) > 5000 or any(ord(char) < 33 for char in text)):
                            raise RuntimeUnavailable("Grok returned an unsupported device verification address.")
                        login_url = text
                        stage = None
                    elif stage == "code":
                        if not re.fullmatch(r"[A-Za-z0-9-]{1,128}", text):
                            raise RuntimeUnavailable("Grok returned an unsupported user-code display.")
                        user_code = text
                        stage = None
                    if login_url and user_code:
                        account.record("login_pending", "Open the official Grok address and confirm this one-time code.",
                                       login_url=login_url, user_code=user_code)
                await process.wait()
                if process.returncode != 0 or not login_url or not user_code:
                    raise RuntimeUnavailable("The original Grok device sign-in did not finish.")
                configure(account.home)
                async with acp_session(account.home, account.connection) as (client, initialized):
                    await _check_access(client, initialized)
                account.record("signed_in", "Grok confirmed this connection's device sign-in. Model access and included usage remain to be checked.")
    except asyncio.CancelledError:
        account.record("attention", "The pending Grok device sign-in was cancelled. Existing sign-in has not been cleared.")
        if not ready.done():
            ready.cancel()
        raise
    except Exception as error:
        detail = str(error) if isinstance(error, RuntimeUnavailable) else "The original Grok device sign-in did not finish or account access could not be confirmed. Retry the sign-in step."
        value = account.record("attention", detail)
        if not ready.done():
            ready.set_result(value)
    finally:
        try:
            await close_process(process)
        finally:
            if stdout:
                stdout.cancel()
                await asyncio.gather(stdout, return_exceptions=True)
