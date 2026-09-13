"""Immutable deferred-search identity verification shared with budget admission."""

from __future__ import annotations

import hashlib

from .staff_routing_models import canonical_json


def request_fingerprint(value: dict) -> str:
    return hashlib.sha256(canonical_json(value).encode("utf-8")).hexdigest()


def source_scope_fingerprint(grant) -> str:
    return request_fingerprint(
        {
            "source_scope": grant.envelope.source_scope,
            "artifacts": [
                item.model_dump(mode="json") for item in grant.envelope.artifacts
            ],
        }
    )


def request_identity(value) -> dict:
    return {
        key: value[key]
        for key in (
            "tender_id",
            "root_run_id",
            "actor_id",
            "requested_by_actor_id",
            "requested_by_assignment_id",
            "requested_by_route_binding_id",
            "requested_by_staff_version",
            "plan_id",
            "route_option_id",
            "connection_id",
            "query",
            "result_limit",
            "review_fingerprint",
            "source_scope_fingerprint",
        )
    }


def verify_search_request(
    conn,
    request_id: str,
    *,
    tender_id: str,
    root_run_id: str,
    plan_id: str,
    route_option_id: str,
    connection_id: str,
    manager_id: str,
    review_fingerprint: str,
    source_scope_fingerprint: str,
):
    """Return one exact deferred request after current grant/scope comparison."""

    row = conn.execute(
        "SELECT * FROM public_search_requests WHERE id=?", (request_id,)
    ).fetchone()
    if row is None or row["status"] != "deferred":
        raise ValueError("The deferred public search request is unavailable.")
    expected = {
        "tender_id": tender_id,
        "root_run_id": root_run_id,
        "plan_id": plan_id,
        "route_option_id": route_option_id,
        "connection_id": connection_id,
        "actor_id": manager_id,
        "review_fingerprint": review_fingerprint,
        "source_scope_fingerprint": source_scope_fingerprint,
    }
    if any(row[key] != value for key, value in expected.items()):
        raise ValueError(
            "The deferred public search no longer matches its reviewed root and source scope."
        )
    if row["request_hash"] != request_fingerprint(request_identity(row)):
        raise ValueError("The deferred public search request identity changed.")
    return row


__all__ = [
    "request_fingerprint",
    "request_identity",
    "source_scope_fingerprint",
    "verify_search_request",
]
