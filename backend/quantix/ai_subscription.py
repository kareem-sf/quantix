"""Subscription metadata is evidence, not a monetary reservation or an invoice."""

from datetime import UTC, datetime
from decimal import Decimal, InvalidOperation

from .ai_models import SubscriptionUsage
from .db import now


def allows_extras(connection):
    return connection.get("protocol") == "grok_build" and connection.get("settings", {}).get("allow_provider_managed_extras") is True


def _stamp(value):
    parsed = datetime.fromisoformat(value.replace("Z", "+00:00"))
    if parsed.tzinfo is None:
        raise ValueError("The usage period has no timezone.")
    return parsed.astimezone(UTC)


def included_only_current(snapshot, *, max_age_seconds=300):
    """Recheck the typed evidence in the core, including its current period."""
    if not snapshot or snapshot.get("included_only_allowed") is not True:
        return False
    try:
        current = datetime.now(UTC)
        age = (current - _stamp(snapshot["fetched_at"])).total_seconds()
        if not 0 <= age <= max_age_seconds or not _stamp(snapshot["period_start"]) <= current < _stamp(snapshot["period_end"]):
            return False
        percent = snapshot["used_percent"]
        if type(percent) not in {int, float} or not 0 <= percent < 100:
            return False
        credits = [Decimal(snapshot[field]) for field in ("prepaid_balance_usd", "on_demand_cap_usd")]
        return all(value.is_finite() and value == 0 for value in credits) and snapshot["auto_topup_enabled"] is False
    except (KeyError, TypeError, ValueError, InvalidOperation, AttributeError):
        return False


def unknown_subscription(detail="Grok could not confirm its current allowance or extra-spending settings. Open Grok usage settings, then refresh this account."):
    return SubscriptionUsage(fetched_at=now(), included_only_allowed=False, detail=detail).model_dump(mode="json")


def subscription_snapshot(value):
    snapshot = SubscriptionUsage.model_validate(value).model_dump(mode="json")
    for field in ("prepaid_balance_usd", "on_demand_cap_usd", "on_demand_used_usd"):
        amount = snapshot[field]
        if amount is not None:
            try:
                parsed = Decimal(amount)
                valid = len(amount) <= 80 and parsed.is_finite() and parsed >= 0
            except InvalidOperation:
                valid = False
            if not valid:
                snapshot[field] = None
    snapshot["included_only_allowed"] = included_only_current(snapshot)
    return snapshot


def require_subscription_access(connection, snapshot, *, checking=False):
    if included_only_current(snapshot):
        return
    if checking:
        raise ValueError("Grok may use paid extras for this check, and Quantix cannot establish its maximum charge. No request was sent. Open Grok usage settings and refresh when subscription-only access can be confirmed.")
    if not allows_extras(connection):
        raise ValueError((snapshot or {}).get("detail") or "Grok's subscription allowance could not be confirmed. Refresh this account before starting work.")


def reported_usage_fields(usage):
    """Keep provider cost observations separate from billable Tender estimates."""
    result = {}
    value = usage.get("provider_reported_cost_usd")
    if isinstance(value, str) and len(value) <= 80:
        try:
            amount = Decimal(value)
            if amount.is_finite() and amount >= 0:
                result["provider_reported_cost_usd"] = value
        except InvalidOperation:
            pass
    for name in ("provider_cost_is_partial", "provider_usage_is_incomplete"):
        if type(usage.get(name)) is bool:
            result[name] = usage[name]
    for name in ("cached_input_tokens", "reasoning_tokens"):
        if type(usage.get(name)) is int and usage[name] >= 0:
            result[name] = usage[name]
    return result
