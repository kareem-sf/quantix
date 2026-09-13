"""Tender-owned routing authority, plan snapshots and conservative AI budgets."""

import hashlib
import json
from decimal import Decimal

from .ai_models import AIRoute, TenderAIInput
from .ai_recovery import restore_basis
from .ai_subscription import allows_extras, reported_usage_fields
from .db import dump, new_id, now, record


def fingerprint(value):
    return hashlib.sha256(dump(value).encode()).hexdigest()


def _metered_billing(connection: dict) -> bool:
    """Return whether a route needs a conservative monetary reservation."""

    return connection.get("billing") in {"metered", "unknown"}


_SEARCH_ACCOUNTING_VERSION = 1


def route_search_reservation(route: dict, requests: int = 1) -> int:
    """Return the finite online-search allowance reserved for a request."""

    if not route.get("web_search"):
        return 0
    return int(route.get("max_search_calls", 0)) * max(1, int(requests))


def search_accounting(data: dict) -> dict[str, bool | int]:
    """Project one usage row onto the canonical root search ledger.

    Rows written before canonical search fields existed are deliberately
    unknown.  Treating those rows as known zero exposure would let a later
    search-capable meter bypass the root allowance.
    """

    required = (
        "search_accounting_version",
        "reserved_search_calls",
        "web_search_calls",
        "search_usage_complete",
        "search_usage_uncertain",
    )
    if data.get("search_accounting_version") != _SEARCH_ACCOUNTING_VERSION or any(key not in data for key in required[1:]):
        return {"calls": 0, "unknown": True, "overrun": data.get("search_allowance_overrun") is True}
    reserved = data.get("reserved_search_calls")
    observed = data.get("web_search_calls")
    complete = data.get("search_usage_complete")
    uncertain_flag = data.get("search_usage_uncertain")
    if (
        type(reserved) is not int
        or reserved < 0
        or type(observed) is not int
        or observed < 0
        or type(complete) is not bool
        or type(uncertain_flag) is not bool
    ):
        return {"calls": 0, "unknown": True, "overrun": data.get("search_allowance_overrun") is True}
    if (not complete or uncertain_flag) and reserved == 0:
        return {"calls": 0, "unknown": True, "overrun": data.get("search_allowance_overrun") is True}
    uncertain = not complete or uncertain_flag or data.get("status") in {"reserved", "uncertain"}
    return {
        "calls": max(reserved, observed) if uncertain else observed,
        "unknown": False,
        "overrun": data.get("search_allowance_overrun") is True,
    }


def route_request_cost(connection: dict, model: dict, route: dict, estimated_input: int,
                       max_output: int, requests: int = 1) -> Decimal:
    """Calculate the conservative request exposure for one selected route.

    This helper is deliberately synchronous.  Callers use it inside their
    authority/SQLite admission transaction so a root meter can validate its
    scope and reserve its allowance atomically before an async provider call.
    """

    metered = _metered_billing(connection)
    if not metered:
        return Decimal(0)
    prices = model.get("pricing")
    if not prices:
        raise ValueError("This paid AI needs confirmed prices and a Tender spending allowance. Open AI setup to finish these steps.")
    if route.get("web_search") and prices.get("web_search_per_call") is None:
        raise ValueError("The price of online research is not confirmed for this AI. Choose another supported AI or review its pricing details.")
    count = max(1, int(requests))
    amount = (
        Decimal(str(estimated_input)) * Decimal(str(prices["input_per_million"]))
        + Decimal(str(max_output)) * Decimal(str(prices["output_per_million"]))
    ) * count / Decimal(1000000)
    if route.get("web_search"):
        amount += Decimal(str(prices["web_search_per_call"])) * int(route.get("max_search_calls", 0)) * count
    from .ai_native_tools import native_request_fields
    amount += Decimal(str(native_request_fields(route, model, count).get("reserved_code_cost_usd", 0)))
    return amount


def route_usage_cost(connection: dict, model: dict, route: dict, input_tokens: int,
                     output_tokens: int, search_calls: int = 0,
                     cached_input_tokens: int | None = None, code_sessions: int | None = None) -> float | None:
    """Return the estimated cost for complete, trustworthy route usage.

    Provider-reported cached input uses the card's cached rate when it has one.
    """

    if not _metered_billing(connection):
        return 0.0
    prices = model.get("pricing")
    if not prices:
        return None
    cached_price = prices.get("cached_input_per_million")
    cached = (
        cached_input_tokens
        if cached_price is not None and type(cached_input_tokens) is int and 0 <= cached_input_tokens <= input_tokens
        else 0
    )
    amount = (
        Decimal(input_tokens - cached) * Decimal(str(prices["input_per_million"]))
        + Decimal(cached) * Decimal(str(cached_price or 0))
        + Decimal(output_tokens) * Decimal(str(prices["output_per_million"]))
    ) / Decimal(1000000)
    if route.get("web_search"):
        price = prices.get("web_search_per_call")
        if price is None:
            return None
        amount += Decimal(str(price)) * int(search_calls)
    if "code_execution" in route.get("native_tools", []):
        if type(code_sessions) is not int or code_sessions < 0 or prices.get("code_execution_per_session") is None:
            return None
        amount += Decimal(str(prices["code_execution_per_session"])) * code_sessions
    return float(amount)


class AIPolicyService:
    def __init__(self, repo):
        self.repo = repo
        from .ai_connections import AIConnectionService
        self.connections = AIConnectionService(repo)
        with repo.db.connect(write=True) as conn:
            for statement in (
                "CREATE TABLE IF NOT EXISTS tender_ai_policy(tender_id TEXT PRIMARY KEY REFERENCES tenders(id),revision INTEGER NOT NULL,data_json TEXT NOT NULL,updated_at TEXT NOT NULL)",
                "CREATE TABLE IF NOT EXISTS plan_ai_team(plan_id TEXT PRIMARY KEY REFERENCES plans(id),tender_id TEXT NOT NULL,data_json TEXT NOT NULL)",
                "CREATE TABLE IF NOT EXISTS ai_team_history(id TEXT PRIMARY KEY,plan_id TEXT NOT NULL,tender_id TEXT NOT NULL,data_json TEXT NOT NULL,rationale TEXT NOT NULL,created_at TEXT NOT NULL)",
                "CREATE TABLE IF NOT EXISTS ai_usage(id TEXT PRIMARY KEY,run_id TEXT NOT NULL REFERENCES runs(id),tender_id TEXT NOT NULL,connection_id TEXT NOT NULL,model_id TEXT NOT NULL,data_json TEXT NOT NULL,created_at TEXT NOT NULL)",
                "CREATE INDEX IF NOT EXISTS ai_usage_tender ON ai_usage(tender_id,run_id)",
            ):
                conn.execute(statement)

    def _totals(self, conn, tender_id, run_id=None):
        spent = Decimal(0)
        reserved = Decimal(0)
        requests = 0
        for row in conn.execute("SELECT data_json FROM ai_usage WHERE tender_id=? AND (? IS NULL OR run_id=?)", (tender_id, run_id, run_id)):
            data = json.loads(row[0])
            spent += Decimal(str(data.get("estimated_cost_usd") or 0))
            reserved += Decimal(str(data.get("reserved_usd") or 0))
            requests += int(data.get("requests") or 0)
        return spent, reserved, requests

    def get(self, tender_id):
        self.repo.get_tender(tender_id)
        restored = restore_basis(self.repo.home)
        with self.repo.db.connect() as conn:
            row = conn.execute("SELECT * FROM tender_ai_policy WHERE tender_id=?", (tender_id,)).fetchone()
            spent, reserved, _ = self._totals(conn, tender_id)
            if row:
                saved = record(row)
                data = {k: v for k, v in saved["data"].items() if not k.startswith("_")} | {"revision": saved["revision"], "updated_at": saved["updated_at"]}
            else:
                data = {"revision": 0, "updated_at": None, "allowed_connection_ids": [], "manager": None, "specialist": None, "role_routes": {}, "fallback_routes": [], "run_budget_usd": None, "tender_budget_usd": None, "max_requests": 12, "rationale": ""}
            needs_review = bool(restored and row and saved["data"].get("_restore_basis") != restored)
            return data | {"tender_id": tender_id, "spent_usd": float(spent), "reserved_usd": float(reserved),
                           "restore_reconciliation_required": needs_review,
                           "spend_history_may_be_incomplete": bool(restored)}

    def validate_route(self, route, allowed):
        route = AIRoute.model_validate(route).model_dump()
        if route["connection_id"] not in allowed:
            raise ValueError("This connection is not approved for the tender's data.")
        connection = self.connections.get(route["connection_id"])
        from .ai_connections import is_supported_profile
        if not is_supported_profile(connection):
            raise ValueError("This saved AI account is retired. Choose one of the five supported provider routes.")
        if not connection["enabled"]:
            raise ValueError("The selected AI connection is disabled.")
        model = next((m for m in self.connections.models(connection["id"]) if m["model_id"] == route["model_id"]), None)
        if model is None:
            raise ValueError("Add or discover the selected model in AI Connections first.")
        capabilities = model["capabilities"]
        from .ai_generation import validate_generation
        validate_generation(route, {**connection, "_model": model})
        if capabilities.get("tools") is not True:
            raise ValueError("The Tender Office requires model tool support. Confirm this model's documented capabilities in AI Connections.")
        if route["web_search"] and capabilities.get("web_search") is not True:
            raise ValueError("Hosted web search is not established for this model and route.")
        from .ai_thinking import allows_level
        if route["reasoning"] and not allows_level(connection, model, route["reasoning"]):
            raise ValueError("Choose a reasoning setting documented for this model.")
        if capabilities.get("max_output_tokens") and route["max_output_tokens"] > capabilities["max_output_tokens"]:
            raise ValueError("The output limit exceeds this model's recorded limit.")
        return route

    def update(self, tender_id, values):
        request = TenderAIInput.model_validate(values)
        with self.connections.authority_guard(), self.repo.atomic() as conn:
            current = self.get(tender_id)
            # Document registration, indexing and package analysis re-read the Tender's
            # AI settings before each request, so choosing the AI while they run is safe.
            if any(r["status"] in {"queued", "running"} and r["kind"] not in {"import", "index", "analysis"}
                   for r in self.repo.list_runs(tender_id)):
                raise ValueError("Stop the current tender work before changing its AI permissions or budget.")
            for identifier in request.allowed_connection_ids:
                self.connections.get(identifier)
            for route in [request.manager, request.specialist, *request.role_routes.values(), *request.fallback_routes]:
                if route:
                    self.validate_route(route, request.allowed_connection_ids)
                    account = self.connections.get(route.connection_id)
                    if allows_extras(account) and request.provider_managed_extras.get(account["id"]) != account["revision"]:
                        raise ValueError("Review and approve Grok's provider-managed extras for this account before using it in the Tender. Quantix cannot guarantee a Tender spending cap for these extras.")
            if any(identifier not in request.allowed_connection_ids or type(revision) is not int or revision < 1
                   for identifier, revision in request.provider_managed_extras.items()):
                raise ValueError("Extra-spending approval must identify an allowed AI account and its displayed revision.")
            data = request.model_dump(exclude={"engineer_confirmed", "restore_budget_reviewed"})
            if current["restore_reconciliation_required"] and request.restore_budget_reviewed:
                has_metered = any(self.connections.get(route.connection_id)["billing"] in {"metered", "unknown"}
                                  for route in [request.manager, request.specialist, *request.role_routes.values(), *request.fallback_routes] if route)
                if has_metered and (not request.run_budget_usd or not request.tender_budget_usd):
                    raise ValueError("Enter new paid-work budgets after reviewing spending since the restored backup.")
                conn.execute("INSERT INTO decisions VALUES(?,?,?,?,?,?,?)", (new_id(), tender_id, "ai_budget_restore", tender_id, "review_remaining_budget", request.rationale, now()))
            if not current["restore_reconciliation_required"] or request.restore_budget_reviewed:
                data["_restore_basis"] = restore_basis(self.repo.home)
            data["_connection_versions"] = {identifier: self.connections.get(identifier)["revision"] for identifier in request.allowed_connection_ids}
            stamp = now()
            conn.execute("INSERT INTO tender_ai_policy VALUES(?,?,?,?) ON CONFLICT(tender_id) DO UPDATE SET revision=excluded.revision,data_json=excluded.data_json,updated_at=excluded.updated_at", (tender_id, current["revision"] + 1, dump(data), stamp))
            conn.execute("INSERT INTO decisions VALUES(?,?,?,?,?,?,?)", (new_id(), tender_id, "ai_policy", tender_id, "approve_routes_and_budget", request.rationale, stamp))
        return self.get(tender_id)

    def _snapshot(self, route, allowed):
        route = self.validate_route(route, allowed)
        connection = self.connections.get(route["connection_id"])
        model = next(m for m in self.connections.models(connection["id"]) if m["model_id"] == route["model_id"])
        return {"route": route, "connection_revision": connection["revision"], "model": model}

    def _assert_idle(self, conn, tender_id):
        if conn.execute("SELECT 1 FROM runs WHERE tender_id=? AND status IN ('queued','running')", (tender_id,)).fetchone():
            raise ValueError("Stop or finish the current work before changing its AI team.")

    @staticmethod
    def _saved_team(conn, tender_id, plan_id):
        row = conn.execute("SELECT data_json FROM plan_ai_team WHERE tender_id=? AND plan_id=?", (tender_id, plan_id)).fetchone()
        return json.loads(row[0]) if row else None

    def propose_team(self, tender_id, plan_id, recommendations=None, *, refresh=False):
        with self.connections.authority_guard(), self.repo.atomic() as conn:
            plan = self.repo.get_plan(tender_id, plan_id)
            if plan["status"] not in {"proposed", "approved"}:
                raise ValueError("This work plan is superseded.")
            saved = self._saved_team(conn, tender_id, plan_id)
            # Publication may create its new plan's first proposal during its
            # own run. Replacing any saved team always requires stopped work.
            if refresh or saved is not None or plan["status"] == "approved":
                self._assert_idle(conn, tender_id)
            return self._propose_team(conn, tender_id, plan, recommendations)

    def _propose_team(self, conn, tender_id, plan, recommendations=None):
        plan_id = plan["id"]
        policy = self.get(tender_id)
        recommendations = recommendations or {}
        saved = self._saved_team(conn, tender_id, plan_id)
        saved_by_task = {
            member.get("task_id"): member.get("route")
            for member in (saved or {}).get("specialists", [])
            if member.get("task_id") and member.get("route")
        }
        members, warnings, snapshots = [], [], []
        if not policy["manager"]:
            warnings.append("Choose the Tender Manager and approve AI routes and budgets before starting this plan.")
        for task in plan["tasks"]:
            # An explicit refresh keeps each previously displayed specialist
            # assignment, including reasoning/output/search settings. New
            # recommendations remain an intentional override for that task.
            route = (recommendations.get(task["id"])
                     or saved_by_task.get(task["id"])
                     or policy["role_routes"].get(task["role"])
                     or policy["specialist"]
                     or policy["manager"])
            if route:
                self.validate_route(route, policy["allowed_connection_ids"])
                members.append({"task_id": task["id"], "role": task["role"], "route": route, "rationale": "Proposed for the specialist role using the tender's approved connection choices."})
        for route in [policy["manager"], *[m["route"] for m in members], *policy["fallback_routes"]]:
            if route:
                snapshots.append(self._snapshot(route, policy["allowed_connection_ids"]))
        data = {"plan_id": plan_id, "policy_revision": policy["revision"], "manager": policy["manager"], "specialists": members, "fallback_routes": policy["fallback_routes"], "status": "proposed", "warnings": warnings}
        data["fingerprint"] = fingerprint({"team": data, "snapshots": snapshots})
        conn.execute("INSERT INTO plan_ai_team VALUES(?,?,?) ON CONFLICT(plan_id) DO UPDATE SET data_json=excluded.data_json", (plan_id, tender_id, dump(data | {"_snapshots": snapshots})))
        return data

    def _public_team(self, data):
        public = {k: v for k, v in data.items() if not k.startswith("_")}
        warnings = list(public["warnings"])
        for snapshot in data.get("_snapshots", []):
            try:
                changed = self.connections.get(snapshot["route"]["connection_id"])["revision"] != snapshot["connection_revision"]
            except KeyError:
                changed = True
            if changed:
                warnings.append("A connection changed or was removed. Refresh and approve the AI team before continuing work.")
        public["warnings"] = list(dict.fromkeys(warnings))
        return public

    def team(self, tender_id, plan_id):
        with self.connections.authority_guard(), self.repo.atomic() as conn:
            plan = self.repo.get_plan(tender_id, plan_id)
            data = self._saved_team(conn, tender_id, plan_id)
            if data and (data["status"] == "approved" or data["policy_revision"] == self.get(tender_id)["revision"]):
                return self._public_team(data)
            if plan["status"] not in {"proposed", "approved"}:
                raise ValueError("This work plan is superseded.")
            self._assert_idle(conn, tender_id)
            return self._propose_team(conn, tender_id, plan)

    def approve_team(self, tender_id, plan_id, expected, rationale="Approved with the work plan", *, require_approved_plan=False):
        with self.connections.authority_guard(), self.repo.atomic() as conn:
            self._assert_idle(conn, tender_id)
            plan = self.repo.get_plan(tender_id, plan_id)
            if require_approved_plan:
                if plan["status"] != "approved":
                    raise ValueError("Approve a proposed AI team together with its engineering work plan.")
            elif plan["status"] != "proposed":
                raise ValueError("Only the current proposed plan can be approved.")
            saved = self._saved_team(conn, tender_id, plan_id)
            if not saved or not expected or expected != saved["fingerprint"]:
                raise ValueError("Review the current AI team before approving the work plan.")
            policy = self.get(tender_id)
            if saved["policy_revision"] != policy["revision"]:
                raise ValueError("The tender's AI permissions changed. Refresh and review its AI team before approval.")
            if saved["warnings"] or not saved["manager"]:
                raise ValueError("Configure this tender's AI routes before approving the plan.")
            for snapshot in saved["_snapshots"]:
                self.validate_route(snapshot["route"], policy["allowed_connection_ids"])
                if self.connections.get(snapshot["route"]["connection_id"])["revision"] != snapshot["connection_revision"]:
                    raise ValueError("An AI connection changed. Refresh the team proposal before approval.")
            saved["status"] = "approved"
            conn.execute("UPDATE plan_ai_team SET data_json=? WHERE tender_id=? AND plan_id=?", (dump(saved), tender_id, plan_id))
            conn.execute("INSERT INTO ai_team_history VALUES(?,?,?,?,?,?)", (new_id(), plan_id, tender_id, dump(saved), rationale, now()))
            conn.execute("INSERT INTO decisions VALUES(?,?,?,?,?,?,?)", (new_id(), tender_id, "ai_team", plan_id, "approve", rationale, now()))
            return self._public_team(saved)

    def routes_for(self, tender_id, *, plan_id=None, task_id=None, role=None):
        with self.connections.authority_guard(), self.repo.db.connect() as conn:
            return self._routes_for(conn, tender_id, plan_id=plan_id, task_id=task_id, role=role)

    def _routes_for(self, conn, tender_id, *, plan_id=None, task_id=None, role=None):
        policy = self.get(tender_id)
        if plan_id:
            if self.repo.get_plan(tender_id, plan_id)["status"] != "approved":
                raise ValueError("This work requires an approved current plan.")
            team = self._saved_team(conn, tender_id, plan_id)
            if not team or team["status"] != "approved":
                raise ValueError("Review and approve the AI team for this work plan.")
            route = next((m["route"] for m in team["specialists"] if (task_id and m["task_id"] == task_id) or (role and m["role"] == role)), None) if task_id or role else team["manager"]
            route = route or team["manager"]
            fallbacks = team["fallback_routes"]
            for snapshot in team["_snapshots"]:
                if self.connections.get(snapshot["route"]["connection_id"])["revision"] != snapshot["connection_revision"]:
                    raise ValueError("An approved AI connection changed. Approve a refreshed team before continuing.")
        else:
            route = (policy["role_routes"].get(role) or policy["specialist"] or policy["manager"]) if role else policy["manager"]
            fallbacks = policy["fallback_routes"]
            row = conn.execute("SELECT data_json FROM tender_ai_policy WHERE tender_id=?", (tender_id,)).fetchone()
            versions = json.loads(row[0]).get("_connection_versions", {}) if row else {}
            for proposed in [route, *fallbacks]:
                if proposed and self.connections.get(proposed["connection_id"])["revision"] != versions.get(proposed["connection_id"]):
                    raise ValueError("An AI connection changed. Review and save this tender's allowed data routes again before starting the Manager.")
        if not route:
            raise ValueError("Open AI setup for this tender and choose a Manager model, allowed connections and budgets.")
        return [self.validate_route(r, policy["allowed_connection_ids"]) for r in [route, *fallbacks]]

    def context_catalog(self, tender_id):
        policy = self.get(tender_id)
        from .ai_readiness import ready_model_ids
        selected = {(route["connection_id"], route["model_id"]) for route in
                    [policy["manager"], policy["specialist"], *policy["role_routes"].values()] if route}
        result = []
        for connection in self.connections.list():
            if connection["id"] not in policy["allowed_connection_ids"]:
                continue
            ready = ready_model_ids(self.repo, connection)
            models = self.connections.models(connection["id"])
            models = [model for model in models if model["model_id"] in ready]
            if not models:
                continue
            models.sort(key=lambda model: ((connection["id"], model["model_id"]) not in selected,
                                           model["capabilities"].get("tools") is not True, model["model_id"]))
            result.append({"connection_id": connection["id"], "name": connection["name"], "provider": connection["provider_id"],
                           "billing": connection["billing"], "models": models[:40]})
        return result

    def usage(self, tender_id):
        self.repo.get_tender(tender_id)
        with self.repo.db.connect() as conn:
            return [record(r)["data"] | {"id": r["id"], "run_id": r["run_id"], "tender_id": tender_id, "connection_id": r["connection_id"], "model_id": r["model_id"], "created_at": r["created_at"]} for r in conn.execute("SELECT * FROM ai_usage WHERE tender_id=? ORDER BY created_at DESC", (tender_id,))]

    def reconcile(self, tender_id, usage_id, values):
        from .ai_models import AIReconcile
        request = AIReconcile.model_validate(values)
        with self.repo.atomic() as conn:
            row = conn.execute("SELECT * FROM ai_usage WHERE id=? AND tender_id=?", (usage_id, tender_id)).fetchone()
            if row is None:
                raise KeyError("This usage record does not belong to the tender.")
            saved = record(row)
            if self.repo.get_run(saved["run_id"])["status"] in {"queued", "running"}:
                raise ValueError("Wait until this run stops before reconciling its usage.")
            if saved["data"]["status"] not in {"uncertain", "reserved"}:
                raise ValueError("Only unresolved usage needs reconciliation.")
            data = saved["data"] | {"status": "reconciled", "reserved_usd": 0, "estimated_cost_usd": request.estimated_cost_usd, "detail": "Engineer reconciliation: " + request.rationale}
            conn.execute("UPDATE ai_usage SET data_json=? WHERE id=?", (dump(data), usage_id))
            conn.execute("INSERT INTO decisions VALUES(?,?,?,?,?,?,?)", (new_id(), tender_id, "ai_usage", usage_id, "reconcile", request.rationale, now()))
        return {"ok": True}


class BudgetMeter:
    def __init__(self, policy, tender_id, run_id, route, *, metadata=None):
        self.service, self.repo = policy, policy.repo
        self.tender_id, self.run_id, self.route = tender_id, run_id, route
        self.connection = policy.connections.get(route["connection_id"])
        self.model = next(m for m in policy.connections.models(route["connection_id"]) if m["model_id"] == route["model_id"])
        self.metadata = dict(metadata or {})
        self.metadata.setdefault("root_run_id", run_id)
        self.pending = set()

    async def before_request(self, estimated_input, max_output, requests=1):
        if "code_execution" in self.route.get("native_tools", []):
            raise ValueError("Hosted code requires a reviewed office root and its shared native-tool spending allowance.")
        with self.service.connections.authority_guard():
            policy = self.service.get(self.tender_id)
            metered = self.connection["billing"] in {"metered", "unknown"}
            if allows_extras(self.connection) and policy.get("provider_managed_extras", {}).get(self.connection["id"]) != self.connection["revision"]:
                raise ValueError("Grok extra spending has not been approved for this Tender and account revision. Review the Tender's AI setup before continuing.")
            if (metered or allows_extras(self.connection)) and policy["restore_reconciliation_required"]:
                raise ValueError("Paid AI work is paused after restoring this workspace. Review spending in your provider accounts, then explicitly review and save the Tender's AI limits, including its recorded spending and reservations.")
            prices = self.model.get("pricing")
            if metered and (not prices or not policy["run_budget_usd"] or not policy["tender_budget_usd"]):
                raise ValueError("This paid AI needs confirmed prices and a Tender spending allowance. Open AI setup to finish these steps.")
            if metered and self.route["web_search"] and prices.get("web_search_per_call") is None:
                raise ValueError("The price of online research is not confirmed for this AI. Choose another supported AI or review its pricing details.")
            amount = route_request_cost(self.connection, self.model, self.route, estimated_input, max_output, requests)
            requested_search = route_search_reservation(self.route, requests)
            with self.repo.atomic() as conn:
                if any(
                    search_accounting(json.loads(row[0]))["overrun"]
                    or json.loads(row[0]).get("native_call_limit_overrun") is True
                    for row in conn.execute(
                        "SELECT data_json FROM ai_usage WHERE tender_id=? AND run_id=?",
                        (self.tender_id, self.run_id),
                    )
                ):
                    raise ValueError("The provider exceeded a reviewed online-research or native-tool allowance. Later root work is stopped for review.")
                total, held, _ = self.service._totals(conn, self.tender_id)
                run_total, run_held, request_count = self.service._totals(conn, self.tender_id, self.run_id)
                if request_count + requests > policy["max_requests"]:
                    raise ValueError("This task reached its work limit before finishing. Review the saved progress and spending allowance before continuing.")
                # Subscription and local routes carry no USD allowance (None);
                # only metered work is compared against money left.
                tender_left = Decimal(str(policy["tender_budget_usd"])) - total - held if metered else None
                run_left = Decimal(str(policy["run_budget_usd"])) - run_total - run_held if metered else None
                if metered and (amount > tender_left or amount > run_left):
                    task = run_left < tender_left
                    limit = Decimal(str(policy["run_budget_usd" if task else "tender_budget_usd"]))
                    used = run_total + run_held if task else total + held
                    raise ValueError(
                        f"AI allowance reached. {'This task' if task else 'This Tender'} has used about USD {used:.2f} "
                        f"of its USD {limit:.2f} allowance, and the next step could need up to USD {amount:.2f}. "
                        "Raise the allowance in Work > AI and spending, or ask for a narrower piece of work."
                    )
                identifier = new_id()
                data = {
                    **self.metadata,
                    "actual_model": None,
                    "billing": self.connection["billing"],
                    "status": "reserved",
                    "requests": requests,
                    "input_tokens": 0,
                    "output_tokens": 0,
                    "estimated_cost_usd": None,
                    "reserved_usd": float(amount),
                    "search_accounting_version": _SEARCH_ACCOUNTING_VERSION,
                    "reserved_search_calls": requested_search,
                    "web_search_calls": 0,
                    "search_usage_complete": requested_search == 0,
                    "search_usage_uncertain": requested_search > 0,
                    "search_allowance_overrun": False,
                    "detail": "Conservative request allowance; provider billing may differ.",
                }
                conn.execute("INSERT INTO ai_usage VALUES(?,?,?,?,?,?,?)", (identifier, self.run_id, self.tender_id, self.route["connection_id"], self.route["model_id"], dump(data), now()))
                self.pending.add(identifier)
            return identifier

    async def on_response(self, usage, reservation_id):
        if not isinstance(usage, dict) or not isinstance(reservation_id, str) or not reservation_id:
            raise ValueError("The model response has no valid budget reservation.")
        incoming, outgoing = usage.get("input_tokens"), usage.get("output_tokens")
        tokens_complete = (
            usage.get("usage_complete", True) is not False
            and type(incoming) is int
            and incoming >= 0
            and type(outgoing) is int
            and outgoing >= 0
        )
        reported_search = usage.get("web_search_calls")
        search_observed = type(reported_search) is int and reported_search >= 0
        search_complete = not self.route["web_search"] or (
            usage.get("usage_complete", True) is not False and search_observed
        )
        complete = tokens_complete and search_complete
        incoming = incoming if tokens_complete else 0
        outgoing = outgoing if tokens_complete else 0
        actual_search = reported_search if self.route["web_search"] and search_observed else 0
        metered = self.connection["billing"] in {"metered", "unknown"}
        cost = None
        with self.service.connections.authority_guard(), self.repo.db.connect(write=True) as conn:
            row = conn.execute("SELECT data_json FROM ai_usage WHERE id=? AND run_id=?", (reservation_id, self.run_id)).fetchone()
            if not row:
                raise ValueError("The model response has no matching budget reservation.")
            data = json.loads(row[0])
            if type(usage.get("requests")) is int and usage["requests"] >= 1:
                data["requests"] = usage["requests"]
            actual_requests = usage.get("requests")
            if type(actual_requests) is not int or actual_requests < 1:
                actual_requests = int(data.get("requests") or 1)
                if usage.get("requests") is not None:
                    complete = False
            if complete and metered:
                cost = route_usage_cost(
                    self.connection,
                    self.model,
                    self.route,
                    incoming,
                    outgoing,
                    actual_search,
                    cached_input_tokens=usage.get("cached_input_tokens"),
                )
            reserved_search = int(data.get("reserved_search_calls") or 0)
            previous_search = int(data.get("web_search_calls") or 0)
            observed_search = max(previous_search, actual_search)
            search_overrun = data.get("search_allowance_overrun") is True or observed_search > reserved_search
            data.update(
                actual_model=usage.get("actual_model"),
                native_call_limit_overrun=(data.get("native_call_limit_overrun") is True or usage.get("native_call_limit_overrun") is True),
                input_tokens=incoming,
                output_tokens=outgoing,
                requests=max(1, int(actual_requests)),
                status="reported" if complete else "uncertain",
                estimated_cost_usd=cost,
                reserved_usd=0 if complete or not metered else data["reserved_usd"],
                reserved_search_calls=0 if complete else reserved_search,
                web_search_calls=observed_search,
                search_usage_complete=not self.route["web_search"] or complete,
                search_usage_uncertain=self.route["web_search"] and not complete,
                search_allowance_overrun=search_overrun,
                detail="Provider token usage recorded; cost is an estimate, not an invoice." if complete else "Provider usage was unavailable. The metered allowance remains reserved.",
            )
            data.update(reported_usage_fields(usage))
            if self.connection["protocol"] == "grok_build":
                data["detail"] = "Grok usage belongs to the account's shared allowance and any explicitly permitted extras. A reported model cost is not a confirmed Tender charge." + (" Some usage details are incomplete." if not complete else "")
            conn.execute("UPDATE ai_usage SET data_json=? WHERE id=?", (dump(data), reservation_id))
            self.pending.discard(reservation_id)
        self.repo.event(self.run_id, "ai_usage", "AI usage recorded.", data | {"connection_id": self.route["connection_id"], "model_id": self.route["model_id"]})

    def interrupted(self, error=None):
        """Settle outstanding reservations after a call stopped.

        A provider rejection before processing bills nothing, so its hold is
        released; any other stop keeps the hold uncertain until reconciled.
        """
        from .ai_api_errors import rejected_before_processing
        from .ai_reservations import released_usage

        rejected = rejected_before_processing(error)
        with self.service.connections.authority_guard(), self.repo.db.connect(write=True) as conn:
            for identifier in self.pending:
                row = conn.execute("SELECT data_json FROM ai_usage WHERE id=?", (identifier,)).fetchone()
                if row:
                    data = json.loads(row[0])
                    if rejected:
                        data = released_usage(data)
                    else:
                        data.update(
                            status="uncertain",
                            search_usage_complete=False if data.get("reserved_search_calls", 0) else data.get("search_usage_complete", True),
                            search_usage_uncertain=bool(data.get("reserved_search_calls", 0)) or data.get("search_usage_uncertain", False),
                            detail="The request ended without complete usage. Reserved cost and search allowance remain until reconciled.",
                        )
                    conn.execute("UPDATE ai_usage SET data_json=? WHERE id=?", (dump(data), identifier))
