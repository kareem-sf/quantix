"""Drop deleted AI accounts from the Tender routes and allowances that named them.

Removing an account used to leave its identifier behind in every Tender it had
been approved for. The next AI change for that Tender then failed while it was
re-validating its own saved allow list, so the account could no longer be
switched from the prompt box at all, and the message named nothing the engineer
could act on. Pruning is a strict reduction of granted authority: an account
that no longer exists can never be routed to.
"""

from __future__ import annotations

import json

from .db import dump, new_id, now

REMOVED_DETAIL = (
    "An AI account named by this Tender no longer exists. It was removed from the "
    "Tender's approved accounts and from every route and spending approval that named it."
)

_ROUTE_KEYS = ("manager", "specialist")


def _route_account(route) -> str | None:
    return route.get("connection_id") if isinstance(route, dict) else None


def prune_policy(policy: dict, missing: set[str]) -> dict | None:
    """Return the policy without any reference to a missing account, or None.

    None means nothing referenced a missing account and the stored row must be
    left exactly as it is, revision included.
    """

    allowed = [item for item in policy.get("allowed_connection_ids") or [] if item not in missing]
    role_routes = {
        role: route
        for role, route in (policy.get("role_routes") or {}).items()
        if _route_account(route) not in missing
    }
    fallbacks = [
        route
        for route in policy.get("fallback_routes") or []
        if _route_account(route) not in missing
    ]
    extras = {
        key: value
        for key, value in (policy.get("provider_managed_extras") or {}).items()
        if key not in missing
    }
    versions = {
        key: value
        for key, value in (policy.get("_connection_versions") or {}).items()
        if key not in missing
    }
    pruned = dict(policy)
    pruned.update(
        allowed_connection_ids=allowed,
        role_routes=role_routes,
        fallback_routes=fallbacks,
        provider_managed_extras=extras,
        _connection_versions=versions,
    )
    for key in _ROUTE_KEYS:
        if _route_account(policy.get(key)) in missing:
            pruned[key] = None
    return None if pruned == policy else pruned


def prune_deleted_accounts(conn, missing: set[str]) -> int:
    """Remove the given accounts from every saved Tender AI policy.

    The caller owns the write transaction so that deleting an account and
    forgetting it everywhere commit together.
    """

    if not missing:
        return 0
    table = conn.execute(
        "SELECT 1 FROM sqlite_master WHERE type='table' AND name='tender_ai_policy'"
    ).fetchone()
    if not table:
        return 0
    changed = 0
    for tender_id, revision, raw in conn.execute(
        "SELECT tender_id, revision, data_json FROM tender_ai_policy"
    ).fetchall():
        pruned = prune_policy(json.loads(raw), missing)
        if pruned is None:
            continue
        stamp = now()
        conn.execute(
            "UPDATE tender_ai_policy SET revision=?, data_json=?, updated_at=? WHERE tender_id=?",
            (int(revision) + 1, dump(pruned), stamp, tender_id),
        )
        conn.execute(
            "INSERT INTO decisions VALUES(?,?,?,?,?,?,?)",
            (
                new_id(),
                tender_id,
                "ai_policy",
                tender_id,
                "remove_deleted_account",
                REMOVED_DETAIL,
                stamp,
            ),
        )
        changed += 1
    return changed


def repair_orphaned_accounts(repo) -> int:
    """Prune accounts that saved Tender policies still name but that are gone."""

    with repo.db.connect(write=True) as conn:
        tables = {
            row[0]
            for row in conn.execute(
                "SELECT name FROM sqlite_master WHERE type='table' AND name IN ('tender_ai_policy','ai_connections')"
            ).fetchall()
        }
        if {"tender_ai_policy", "ai_connections"} - tables:
            return 0
        live = {row[0] for row in conn.execute("SELECT id FROM ai_connections").fetchall()}
        named: set[str] = set()
        for (raw,) in conn.execute("SELECT data_json FROM tender_ai_policy").fetchall():
            policy = json.loads(raw)
            named.update(policy.get("allowed_connection_ids") or [])
            named.update(policy.get("_connection_versions") or {})
            named.update(policy.get("provider_managed_extras") or {})
            routes = [policy.get(key) for key in _ROUTE_KEYS]
            routes += list((policy.get("role_routes") or {}).values())
            routes += list(policy.get("fallback_routes") or [])
            named.update(account for account in map(_route_account, routes) if account)
        return prune_deleted_accounts(conn, named - live)


__all__ = ["prune_policy", "prune_deleted_accounts", "repair_orphaned_accounts", "REMOVED_DETAIL"]
