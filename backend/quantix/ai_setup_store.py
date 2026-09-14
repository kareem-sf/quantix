"""Durable onboarding and separately recorded connection-check charges."""

import json
from decimal import Decimal

from .db import dump, new_id, now


def _reported_usage_fields(usage):
    result = {}
    value = usage.get("provider_reported_cost_usd")
    if isinstance(value, str) and len(value) <= 80:
        try:
            amount = Decimal(value)
            if amount.is_finite() and amount >= 0:
                result["provider_reported_cost_usd"] = value
        except Exception:
            pass
    for name in ("provider_cost_is_partial", "provider_usage_is_incomplete"):
        if type(usage.get(name)) is bool:
            result[name] = usage[name]
    if type(usage.get("cached_input_tokens")) is int and usage["cached_input_tokens"] >= 0:
        result["cached_input_tokens"] = usage["cached_input_tokens"]
    return result


class SetupStore:
    def __init__(self, repo):
        self.repo = repo
        with repo.db.connect(write=True) as conn:
            conn.execute(
                "CREATE TABLE IF NOT EXISTS ai_setup_accounts(connection_id TEXT PRIMARY KEY REFERENCES ai_connections(id) ON DELETE CASCADE,data_json TEXT NOT NULL)"
            )
            conn.execute(
                "CREATE TABLE IF NOT EXISTS ai_setup_checks(id TEXT PRIMARY KEY,connection_id TEXT NOT NULL,model_id TEXT NOT NULL,created_at TEXT NOT NULL,data_json TEXT NOT NULL)"
            )
            conn.execute(
                "CREATE TABLE IF NOT EXISTS ai_setup_model_checks(connection_id TEXT NOT NULL REFERENCES ai_connections(id) ON DELETE CASCADE,model_id TEXT NOT NULL,data_json TEXT NOT NULL,PRIMARY KEY(connection_id,model_id))"
            )
            for row in conn.execute("SELECT * FROM ai_setup_accounts").fetchall():
                saved = json.loads(row["data_json"])
                check = saved.get("check", {})
                if check.get("status") == "passed" and check.get("model_id"):
                    evidence = {
                        "check": check,
                        "checked_revision": saved.get("checked_revision"),
                        "checked_component_version": saved.get("checked_component_version"),
                    }
                    conn.execute(
                        "INSERT INTO ai_setup_model_checks VALUES(?,?,?) ON CONFLICT(connection_id,model_id) DO NOTHING",
                        (row["connection_id"], check["model_id"], dump(evidence)),
                    )

    def invalidate_model_checks(self, identifier, model_id=None):
        with self.repo.db.connect(write=True) as conn:
            conn.execute(
                "DELETE FROM ai_setup_model_checks WHERE connection_id=? AND (? IS NULL OR model_id=?)",
                (identifier, model_id, model_id),
            )
            saved = self.get(identifier)
            if model_id is None or saved.get("check", {}).get("model_id") == model_id:
                self.update(
                    identifier,
                    check={
                        "status": "not_checked",
                        "detail": "Check this model again before using it.",
                    },
                    checked_revision=None,
                    checked_component_version=None,
                )

    def model_check(self, identifier, model_id):
        with self.repo.db.connect() as conn:
            row = conn.execute(
                "SELECT data_json FROM ai_setup_model_checks WHERE connection_id=? AND model_id=?",
                (identifier, model_id),
            ).fetchone()
        if row:
            return json.loads(row[0])
        saved = self.get(identifier)
        return saved if saved.get("check", {}).get("model_id") == model_id else {}

    def save_model_check(self, identifier, model_id, evidence, *, previous_revision=None):
        with self.repo.db.connect(write=True) as conn:
            # Only the checked-capability update may carry other valid checks
            # forward: the caller owns the account guard and its revision CAS.
            if previous_revision is not None and previous_revision != evidence["checked_revision"]:
                for row in conn.execute(
                    "SELECT model_id,data_json FROM ai_setup_model_checks WHERE connection_id=?",
                    (identifier,),
                ).fetchall():
                    prior = json.loads(row["data_json"])
                    if (
                        prior.get("checked_revision") == previous_revision
                        and prior.get("checked_component_version")
                        == evidence["checked_component_version"]
                    ):
                        prior["checked_revision"] = evidence["checked_revision"]
                        conn.execute(
                            "UPDATE ai_setup_model_checks SET data_json=? WHERE connection_id=? AND model_id=?",
                            (dump(prior), identifier, row["model_id"]),
                        )
            conn.execute(
                "INSERT INTO ai_setup_model_checks VALUES(?,?,?) ON CONFLICT(connection_id,model_id) DO UPDATE SET data_json=excluded.data_json",
                (identifier, model_id, dump(evidence)),
            )

    def get(self, identifier):
        with self.repo.db.connect() as conn:
            row = conn.execute(
                "SELECT data_json FROM ai_setup_accounts WHERE connection_id=?", (identifier,)
            ).fetchone()
        return json.loads(row[0]) if row else {}

    def carry_spending_preference_checks(self, identifier, old_revision, new_revision):
        """Only a spending-preference edit may retain unchanged model evidence.

        Caller holds account authority and the update transaction. Tender route
        authority still refers to the old revision and must be approved again.
        """
        with self.repo.db.connect(write=True) as conn:
            for row in conn.execute(
                "SELECT model_id,data_json FROM ai_setup_model_checks WHERE connection_id=?",
                (identifier,),
            ).fetchall():
                evidence = json.loads(row["data_json"])
                if evidence.get("checked_revision") == old_revision:
                    evidence["checked_revision"] = new_revision
                    conn.execute(
                        "UPDATE ai_setup_model_checks SET data_json=? WHERE connection_id=? AND model_id=?",
                        (dump(evidence), identifier, row["model_id"]),
                    )
            saved = self.get(identifier)
            if saved.get("checked_revision") == old_revision:
                self.update(identifier, checked_revision=new_revision)

    def update(self, identifier, **changes):
        with self.repo.db.connect(write=True) as conn:
            row = conn.execute(
                "SELECT data_json FROM ai_setup_accounts WHERE connection_id=?", (identifier,)
            ).fetchone()
            value = (json.loads(row[0]) if row else {}) | changes
            conn.execute(
                "INSERT INTO ai_setup_accounts VALUES(?,?) ON CONFLICT(connection_id) DO UPDATE SET data_json=excluded.data_json",
                (identifier, dump(value)),
            )
        return value

    def recover(self):
        with self.repo.db.connect(write=True) as conn:
            for row in conn.execute("SELECT * FROM ai_setup_accounts").fetchall():
                value = json.loads(row["data_json"])
                if value.get("stage") in {
                    "preparing",
                    "discovering",
                    "checking",
                    "needs_sign_in",
                } and value.get("active"):
                    value.update(
                        active=False,
                        stage="attention",
                        detail="Setup was interrupted. Choose Retry to continue.",
                        login_url=None,
                        user_code=None,
                    )
                    check = value.get("check", {})
                    if check.get("status") == "checking":
                        check.update(
                            status="interrupted",
                            detail="The check stopped before its result was confirmed. Any uncertain charge remains recorded.",
                        )
                    conn.execute(
                        "UPDATE ai_setup_accounts SET data_json=? WHERE connection_id=?",
                        (dump(value), row["connection_id"]),
                    )
            for row in conn.execute("SELECT id,data_json FROM ai_setup_checks").fetchall():
                value = json.loads(row["data_json"])
                if value.get("status") == "reserved":
                    value["status"] = "uncertain"
                    conn.execute(
                        "UPDATE ai_setup_checks SET data_json=? WHERE id=?",
                        (dump(value), row["id"]),
                    )

    def checks(self, identifier):
        with self.repo.db.connect() as conn:
            allowed = {
                "attempt_id",
                "status",
                "requests",
                "input_tokens",
                "output_tokens",
                "estimated_cost_usd",
                "reserved_usd",
                "actual_model",
                "provider_reported_cost_usd",
                "provider_cost_is_partial",
                "provider_usage_is_incomplete",
                "cached_input_tokens",
                "max_requests",
                "max_input_tokens",
                "max_output_tokens",
                "unknown_cost_consent",
            }
            return [
                {"id": row["id"], "model_id": row["model_id"], "created_at": row["created_at"]}
                | {
                    key: value
                    for key, value in json.loads(row["data_json"]).items()
                    if key in allowed
                }
                for row in conn.execute(
                    "SELECT * FROM ai_setup_checks WHERE connection_id=? ORDER BY created_at DESC",
                    (identifier,),
                )
            ]

    def unresolved_cost(self, identifier):
        return float(
            sum(
                (Decimal(str(row.get("reserved_usd") or 0)) for row in self.checks(identifier)),
                Decimal(0),
            )
        )


class SetupCheckMeter:
    """Never use Tender funds or invent a Tender to pay for an account check."""

    def __init__(
        self,
        repo,
        connection,
        model,
        allowance,
        requests=2,
        input_allowance=16384,
        unknown_cost_accepted=False,
    ):
        self.repo, self.connection, self.model = repo, connection, model
        self.allowance = Decimal(str(allowance or 0))
        self.max_requests, self.requests = requests, 0
        self.input_allowance = input_allowance
        self.unknown_cost_accepted = unknown_cost_accepted
        self.pending = {}
        self.estimated = Decimal(0)
        self.records = []
        self.attempt_id = new_id()
        self.metered = connection["billing"] in {"metered", "unknown"}

    async def before_request(self, estimated_input, max_output, requests=1):
        if self.requests + requests > self.max_requests:
            raise ValueError(
                "The connection check did not finish within its short allowance. No further request was sent."
            )
        if (
            type(estimated_input) is not int
            or estimated_input < 0
            or estimated_input > self.input_allowance
        ):
            raise ValueError(
                "The connection check input exceeds its 16,384-byte allowance. No request was sent."
            )
        if type(max_output) is not int or max_output < 0 or max_output > 1024:
            raise ValueError(
                "The connection check output exceeds its 1,024-token allowance. No request was sent."
            )
        price = self.model.get("pricing")
        if self.metered and not price and not self.unknown_cost_accepted:
            raise ValueError(
                "The cost of this connection check could not be confirmed. No paid request was sent."
            )
        amount = (
            (
                (
                    Decimal(str(estimated_input)) * Decimal(str(price["input_per_million"]))
                    + Decimal(str(max_output)) * Decimal(str(price["output_per_million"]))
                )
                * requests
                / Decimal(1000000)
            )
            if self.metered and price
            else Decimal(0)
        )
        held = sum(self.pending.values(), Decimal(0))
        if self.metered and price and self.estimated + held + amount > self.allowance:
            raise ValueError(
                "The connection check would exceed the displayed allowance. No further request was sent."
            )
        identifier = new_id()
        data = {
            "status": "reserved",
            "attempt_id": self.attempt_id,
            "requests": requests,
            "reserved_usd": float(amount),
            "estimated_cost_usd": None,
            "input_tokens": 0,
            "output_tokens": 0,
            "actual_model": None,
            "max_requests": self.max_requests,
            "max_input_tokens": self.input_allowance,
            "max_output_tokens": 1024,
            "unknown_cost_consent": self.unknown_cost_accepted,
        }
        with self.repo.db.connect(write=True) as conn:
            conn.execute(
                "INSERT INTO ai_setup_checks VALUES(?,?,?,?,?)",
                (identifier, self.connection["id"], self.model["model_id"], now(), dump(data)),
            )
        self.requests += requests
        self.pending[identifier] = amount
        self.records.append(identifier)
        return identifier

    async def on_response(self, usage, reservation_id):
        if reservation_id not in self.pending:
            raise ValueError("The connection check returned an unmatched usage record.")
        incoming, outgoing = usage.get("input_tokens"), usage.get("output_tokens")
        complete = (
            usage.get("usage_complete", True)
            and type(incoming) is int
            and incoming >= 0
            and type(outgoing) is int
            and outgoing >= 0
        )
        cost = None
        if complete and self.metered and self.model.get("pricing"):
            price = self.model["pricing"]
            cost = (
                Decimal(incoming) * Decimal(str(price["input_per_million"]))
                + Decimal(outgoing) * Decimal(str(price["output_per_million"]))
            ) / Decimal(1000000)
            self.estimated += cost
        held = self.pending[reservation_id]
        if complete or not self.metered:
            self.pending.pop(reservation_id)
            held = Decimal(0)
        data = {
            "status": "reported" if complete else "uncertain",
            "attempt_id": self.attempt_id,
            "requests": usage.get("requests") or 0,
            "reserved_usd": float(held),
            "estimated_cost_usd": float(cost) if cost is not None else None,
            "input_tokens": incoming if type(incoming) is int else 0,
            "output_tokens": outgoing if type(outgoing) is int else 0,
            "actual_model": usage.get("actual_model"),
            "max_requests": self.max_requests,
            "max_input_tokens": self.input_allowance,
            "max_output_tokens": 1024,
            "unknown_cost_consent": self.unknown_cost_accepted,
        }
        data.update(_reported_usage_fields(usage))
        with self.repo.db.connect(write=True) as conn:
            conn.execute(
                "UPDATE ai_setup_checks SET data_json=? WHERE id=?", (dump(data), reservation_id)
            )

    def summary(self, status, detail):
        return {
            "status": status,
            "detail": detail,
            "model_id": self.model["model_id"],
            "checked_at": now(),
            "estimated_cost_usd": float(self.estimated)
            if self.metered and self.model.get("pricing") and not self.pending
            else None,
            "reserved_usd": float(sum(self.pending.values(), Decimal(0))),
        }


class SubscriptionCheckMeter:
    """Record an original-client check without applying API token or cost caps.

    Original clients own their inference accounting. Grok exposes a bounded
    five-round worker check; Codex exposes one client turn whose internal
    inference count and token limits are not available to Quantix.
    """

    def __init__(self, repo, connection, model, *, max_requests=None):
        self.repo, self.connection, self.model = repo, connection, model
        self.max_requests = max_requests
        self.requests = 0
        self.pending = {}
        self.records = []
        self.attempt_id = new_id()

    async def before_request(self, estimated_input, max_output, requests=1):
        if type(requests) is not int or requests < 1:
            raise ValueError("The subscription check returned an invalid request count.")
        if self.max_requests is not None and self.requests + requests > self.max_requests:
            raise ValueError(
                "The subscription connection check reached its bounded worker round limit. Retry the check."
            )
        identifier = new_id()
        data = {
            "status": "reserved",
            "attempt_id": self.attempt_id,
            "requests": requests,
            "reserved_usd": 0,
            "estimated_cost_usd": None,
            "input_tokens": 0,
            "output_tokens": 0,
            "actual_model": None,
            "max_requests": self.max_requests,
            "max_input_tokens": None,
            "max_output_tokens": None,
            "unknown_cost_consent": False,
        }
        with self.repo.db.connect(write=True) as conn:
            conn.execute(
                "INSERT INTO ai_setup_checks VALUES(?,?,?,?,?)",
                (identifier, self.connection["id"], self.model["model_id"], now(), dump(data)),
            )
        self.requests += requests
        self.pending[identifier] = requests
        self.records.append(identifier)
        return identifier

    async def on_response(self, usage, reservation_id):
        if reservation_id not in self.pending:
            raise ValueError("The subscription check returned an unmatched usage record.")
        incoming, outgoing = usage.get("input_tokens"), usage.get("output_tokens")
        complete = (
            usage.get("usage_complete", True)
            and type(incoming) is int
            and incoming >= 0
            and type(outgoing) is int
            and outgoing >= 0
        )
        data = {
            "status": "reported" if complete else "uncertain",
            "attempt_id": self.attempt_id,
            "requests": usage.get("requests")
            if type(usage.get("requests")) is int
            else self.pending[reservation_id],
            "reserved_usd": 0,
            "estimated_cost_usd": None,
            "input_tokens": incoming if type(incoming) is int and incoming >= 0 else 0,
            "output_tokens": outgoing if type(outgoing) is int and outgoing >= 0 else 0,
            "actual_model": usage.get("actual_model"),
            "max_requests": self.max_requests,
            "max_input_tokens": None,
            "max_output_tokens": None,
            "unknown_cost_consent": False,
        }
        data.update(_reported_usage_fields(usage))
        with self.repo.db.connect(write=True) as conn:
            conn.execute(
                "UPDATE ai_setup_checks SET data_json=? WHERE id=?", (dump(data), reservation_id)
            )
        self.pending.pop(reservation_id, None)

    def summary(self, status, detail):
        return {
            "status": status,
            "detail": detail,
            "model_id": self.model["model_id"],
            "checked_at": now(),
            "estimated_cost_usd": None,
            "reserved_usd": 0,
        }
