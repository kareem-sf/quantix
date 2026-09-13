"""A deleted AI account must not keep a Tender from changing its AI again."""

import json

import pytest

from quantix.ai_policy_repair import (
    prune_deleted_accounts,
    prune_policy,
    repair_orphaned_accounts,
)


def _policy(**overrides):
    base = {
        "allowed_connection_ids": ["gone", "live"],
        "manager": {"connection_id": "live", "model_id": "m"},
        "specialist": {"connection_id": "gone", "model_id": "m"},
        "role_routes": {"surveyor": {"connection_id": "gone", "model_id": "m"}},
        "fallback_routes": [
            {"connection_id": "gone", "model_id": "m"},
            {"connection_id": "live", "model_id": "m"},
        ],
        "provider_managed_extras": {"gone": 3, "live": 4},
        "_connection_versions": {"gone": 3, "live": 4},
        "max_requests": 32,
        "rationale": "saved",
    }
    return base | overrides


def test_every_reference_to_a_deleted_account_is_dropped():
    pruned = prune_policy(_policy(), {"gone"})
    assert pruned["allowed_connection_ids"] == ["live"]
    assert pruned["specialist"] is None
    assert pruned["role_routes"] == {}
    assert [route["connection_id"] for route in pruned["fallback_routes"]] == ["live"]
    assert pruned["provider_managed_extras"] == {"live": 4}
    assert pruned["_connection_versions"] == {"live": 4}
    # The surviving account and everything unrelated are untouched.
    assert pruned["manager"] == {"connection_id": "live", "model_id": "m"}
    assert pruned["max_requests"] == 32
    assert pruned["rationale"] == "saved"


def test_a_policy_naming_no_deleted_account_is_left_exactly_as_saved():
    assert prune_policy(_policy(), {"absent"}) is None
    assert prune_policy(_policy(), set()) is None


@pytest.fixture
def policy_repo(tmp_path):
    from quantix.ai_connections import AIConnectionService
    from quantix.repository import Repository

    repo = Repository(tmp_path)
    AIConnectionService(repo)  # Owns the ai_connections schema.
    with repo.db.connect(write=True) as conn:
        conn.execute(
            "CREATE TABLE IF NOT EXISTS tender_ai_policy("
            "tender_id TEXT PRIMARY KEY, revision INTEGER NOT NULL, "
            "data_json TEXT NOT NULL, updated_at TEXT NOT NULL)"
        )
    return repo


def test_startup_repair_prunes_only_accounts_that_no_longer_exist(policy_repo):
    tender = policy_repo.create_tender("Repair")["id"]
    with policy_repo.db.connect(write=True) as conn:
        conn.execute(
            "INSERT INTO ai_connections(id,data_json,credential_ref,credential_mode) VALUES(?,?,?,?)",
            ("live", json.dumps({"id": "live"}), None, "missing"),
        )
        conn.execute(
            "INSERT INTO tender_ai_policy VALUES(?,?,?,?)",
            (tender, 8, json.dumps(_policy()), "2026-09-12T00:00:00+00:00"),
        )
    assert repair_orphaned_accounts(policy_repo) == 1
    with policy_repo.db.connect() as conn:
        revision, raw = conn.execute(
            "SELECT revision, data_json FROM tender_ai_policy WHERE tender_id=?", (tender,)
        ).fetchone()
        recorded = conn.execute(
            "SELECT target_type, decision FROM decisions WHERE tender_id=?", (tender,)
        ).fetchall()
    saved = json.loads(raw)
    assert saved["allowed_connection_ids"] == ["live"]
    assert saved["specialist"] is None
    assert int(revision) == 9
    assert ("ai_policy", "remove_deleted_account") in [tuple(row) for row in recorded]
    # Running it again is a no-op: nothing is left to prune.
    assert repair_orphaned_accounts(policy_repo) == 0


def test_pruning_nothing_touches_no_rows(policy_repo):
    tender = policy_repo.create_tender("Untouched")["id"]
    with policy_repo.db.connect(write=True) as conn:
        conn.execute(
            "INSERT INTO tender_ai_policy VALUES(?,?,?,?)",
            (tender, 2, json.dumps(_policy()), "2026-09-12T00:00:00+00:00"),
        )
        assert prune_deleted_accounts(conn, set()) == 0
    with policy_repo.db.connect() as conn:
        revision = conn.execute(
            "SELECT revision FROM tender_ai_policy WHERE tender_id=?", (tender,)
        ).fetchone()[0]
    assert int(revision) == 2
