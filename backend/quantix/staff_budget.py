"""Aggregate request, spending and search accounting for one office root.

The Manager root is the budget owner.  Staff meters are attribution views over
that same root and never create a grant or reset its counters.  The provider
call remains outside the synchronous authority/SQLite transaction: admission
validates the reviewed scope and records a reservation, then the async runtime
uses the returned reservation ID.
"""

from __future__ import annotations

import hashlib
import json
from decimal import Decimal, InvalidOperation
from typing import Any

from .ai_models import AIRoute
from .ai_policy import (
    _metered_billing,
    route_request_cost,
    route_search_reservation,
    route_usage_cost,
    search_accounting,
)
from .ai_subscription import reported_usage_fields
from .db import dump, new_id, now
from .staff_routing import StaffRoutingService


def _integer(value: Any, label: str, *, minimum: int = 0) -> int:
    if type(value) is not int or value < minimum:
        raise ValueError(f"{label} must be an integer of at least {minimum}.")
    return value


def _decimal(value: Any, label: str) -> Decimal:
    try:
        parsed = Decimal(str(value))
    except (InvalidOperation, TypeError, ValueError) as error:
        raise ValueError(f"{label} is invalid.") from error
    if not parsed.is_finite():
        raise ValueError(f"{label} is invalid.")
    return parsed


class OfficeBudgetMeter:
    """Reserve and report usage against an explicitly reviewed office root.

    ``plan_id`` identifies the approved work scope.  A Manager meter leaves
    staff identity fields unset; a staff meter supplies all three of
    ``assignment_id``, ``staff_id`` and ``binding_id`` so attribution cannot be
    model-controlled or inferred from a role name.
    """

    def __init__(
        self,
        policy,
        tender_id: str,
        root_run_id: str,
        route: dict | AIRoute,
        *,
        plan_id: str,
        assignment_id: str | None = None,
        staff_id: str | None = None,
        binding_id: str | None = None,
        research_route_option_id: str | None = None,
        research_request_id: str | None = None,
    ):
        if policy is None or not hasattr(policy, "repo"):
            raise TypeError("An AIPolicyService is required for office budget metering.")
        if not isinstance(plan_id, str) or not plan_id:
            raise ValueError("An approved plan is required for office budget metering.")
        self.service = policy
        self.repo = policy.repo
        self.tender_id = tender_id
        self.root_run_id = root_run_id
        self.plan_id = plan_id
        self.route = AIRoute.model_validate(route).model_dump(mode="json")
        self.assignment_id = assignment_id
        self.staff_id = staff_id
        self.binding_id = binding_id
        self.research_route_option_id = research_route_option_id
        self.research_request_id = research_request_id
        if bool(research_route_option_id) != bool(research_request_id):
            raise ValueError("A reviewed research route and its durable request are both required.")
        identities = (assignment_id, staff_id, binding_id)
        if any(value is not None for value in identities) and not all(value is not None for value in identities):
            raise ValueError("A staff budget meter requires its assignment, staff and route binding identities.")
        self.is_staff = all(value is not None for value in identities)
        if self.research_request_id and self.is_staff:
            raise ValueError("Deferred provider research is coordinated by the pinned Manager; its requesting colleague remains on the research receipt.")
        self.routing = StaffRoutingService(self.repo)
        self.connection: dict | None = policy.connections.get(self.route["connection_id"])
        self.model: dict | None = next(
            (item for item in policy.connections.models(self.route["connection_id"])
             if item["model_id"] == self.route["model_id"]),
            None,
        )
        self.pending: set[str] = set()

    @staticmethod
    def _same_route(left: dict, right: dict) -> bool:
        return AIRoute.model_validate(left).model_dump(mode="json") == AIRoute.model_validate(right).model_dump(mode="json")

    @staticmethod
    def _route_within(approved: dict, requested: dict) -> bool:
        """Allow only lower per-request output and search ceilings."""

        approved_route = AIRoute.model_validate(approved).model_dump(mode="json")
        requested_route = AIRoute.model_validate(requested).model_dump(mode="json")
        ceilings = {"max_output_tokens", "max_search_calls", "max_native_tool_calls"}
        for field in set(approved_route) | set(requested_route):
            if field not in ceilings and requested_route.get(field) != approved_route.get(field):
                return False
        return (
            requested_route["max_output_tokens"] <= approved_route["max_output_tokens"]
            and requested_route["max_search_calls"] <= approved_route["max_search_calls"]
            and requested_route.get("max_native_tool_calls", 3) <= approved_route.get("max_native_tool_calls", 3)
        )

    @staticmethod
    def _json(value: Any) -> str:
        try:
            return json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":"), allow_nan=False)
        except (TypeError, ValueError) as error:
            raise ValueError("The provider usage report is not valid JSON.") from error

    def _metadata(self, grant_id: str) -> dict[str, Any]:
        data: dict[str, Any] = {
            "root_run_id": self.root_run_id,
            "plan_id": self.plan_id,
            "grant_id": grant_id,
            "meter_scope": "staff" if self.is_staff else "manager",
            "route_fingerprint": hashlib.sha256(self._json(self.route).encode("utf-8")).hexdigest(),
        }
        if self.is_staff:
            data.update(
                actor_id=self.staff_id,
                staff_id=self.staff_id,
                assignment_id=self.assignment_id,
                binding_id=self.binding_id,
            )
        else:
            data["actor_id"] = "manager"
        if self.research_request_id:
            manager = self.routing.manager_runs.get(self.tender_id, self.root_run_id)
            data.update(meter_scope="research", actor_id=manager.id, research_request_id=self.research_request_id,
                        research_route_option_id=self.research_route_option_id, origin="reviewed_api_research")
        return data

    def _root_totals(self, conn) -> dict[str, Any]:
        from .research_budget import research_search_usage
        spent = Decimal(0)
        reserved = Decimal(0)
        requests = 0
        search_used = 0
        overrun = False
        search_unknown = False
        for row in conn.execute(
            "SELECT data_json FROM ai_usage WHERE tender_id=? AND run_id=?",
            (self.tender_id, self.root_run_id),
        ):
            data = json.loads(row[0])
            spent += _decimal(data.get("estimated_cost_usd") or 0, "saved AI cost")
            reserved += _decimal(data.get("reserved_usd") or 0, "saved AI reservation")
            requests += int(data.get("requests") or 0)
            search = search_accounting(data)
            search_used += int(search["calls"])
            overrun = overrun or bool(search["overrun"])
            search_unknown = search_unknown or bool(search["unknown"])
        return {
            "spent": spent,
            "reserved": reserved,
            "requests": requests,
            "search_used": search_used + research_search_usage(conn, self.tender_id, self.root_run_id),
            "search_overrun": overrun,
            "search_unknown": search_unknown,
            "native_overrun": any(json.loads(row[0]).get("code_allowance_overrun", False)
                or json.loads(row[0]).get("native_call_limit_overrun", False) for row in conn.execute(
                "SELECT data_json FROM ai_usage WHERE tender_id=? AND run_id=?", (self.tender_id, self.root_run_id))),
        }

    def _validate_assignment(self, conn, binding) -> None:
        tables = conn.execute(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name='office_assignments'"
        ).fetchone()
        if tables is None:
            raise ValueError("The staff assignment record is unavailable. Queue the reviewed assignment again.")
        row = conn.execute(
            "SELECT * FROM office_assignments WHERE tender_id=? AND id=?",
            (self.tender_id, self.assignment_id),
        ).fetchone()
        if row is None:
            raise ValueError("The staff assignment does not belong to this Tender.")
        expected = {
            "root_run_id": self.root_run_id,
            "staff_id": self.staff_id,
            "staff_version": binding.staff_version,
            "work_order_id": binding.work_order_id,
            "route_binding_id": self.binding_id,
        }
        for field, value in expected.items():
            if row[field] != value:
                raise ValueError(f"The saved staff assignment has a mismatched {field.replace('_', ' ')} identity.")
        if row["status"] != "running":
            raise ValueError("Only a running staff assignment can reserve AI work.")
        if binding.root_run_id != self.root_run_id or binding.staff_id != self.staff_id or binding.id != self.binding_id:
            raise ValueError("The saved staff route binding does not match this assignment.")

    def _validate_scope(self, conn):
        grant, policy, _run = self.routing._validate_root_in_conn(
            conn, self.tender_id, self.root_run_id, self.plan_id
        )
        if self.research_request_id:
            from .ai_direct import supports_direct
            from .research_search_receipts import source_scope_fingerprint, verify_search_request
            option = next((candidate for candidate in grant.envelope.route_options if candidate.id == self.research_route_option_id), None)
            connection = self.service.connections.get(self.route["connection_id"])
            if (option is None or not self._route_within(option.route.model_dump(), self.route)
                or not supports_direct(connection) or not self.route["web_search"]
                or set(self.route.get("native_tools", [])).difference({"web_search"})):
                raise ValueError("This research request needs its exact reviewed direct-API search route, without unrelated native tools.")
            manager = self.routing.manager_runs.get(self.tender_id, self.root_run_id)
            verify_search_request(conn, self.research_request_id, tender_id=self.tender_id, root_run_id=self.root_run_id,
                plan_id=self.plan_id, route_option_id=self.research_route_option_id, connection_id=connection["id"],
                manager_id=manager.id, review_fingerprint=grant.review_fingerprint,
                source_scope_fingerprint=source_scope_fingerprint(grant))
        elif self.is_staff:
            binding = self.routing.validate_binding(self.tender_id, self.binding_id)
            option = next((candidate for candidate in grant.envelope.route_options if candidate.id == binding.route_option_id), None)
            if (
                option is None
                or binding.root_run_id != self.root_run_id
                or binding.plan_id != self.plan_id
                or binding.route_option_id != option.id
                or not self._same_route(binding.route, option.route)
                or not self._route_within(binding.route.model_dump(mode="json"), self.route)
            ):
                raise ValueError("The staff route binding does not match the approved route for this root.")
            self._validate_assignment(conn, binding)
        else:
            if any(value is not None for value in (self.assignment_id, self.staff_id, self.binding_id)):
                raise ValueError("Manager budget metering cannot carry a staff assignment identity.")
            team_row = conn.execute(
                "SELECT data_json FROM plan_ai_team WHERE tender_id=? AND plan_id=?",
                (self.tender_id, self.plan_id),
            ).fetchone()
            if team_row is not None:
                team = json.loads(team_row[0])
                manager_route = team.get("manager") if team.get("status") == "approved" else None
                if not manager_route or not self._route_within(manager_route, self.route):
                    raise ValueError("The Manager route does not match the approved Manager route for this root.")
            else:
                manager_route = policy.get("manager")
                if not manager_route or not self._route_within(manager_route, self.route):
                    raise ValueError("The approved Manager route is unavailable for this root.")
            option = next((candidate for candidate in grant.envelope.route_options if self._same_route(candidate.route, manager_route)), None)
            if option is None:
                raise ValueError("The exact approved Manager route is not a reviewed route option.")

        connection = self.service.connections.get(self.route["connection_id"])
        model = next(
            (item for item in self.service.connections.models(connection["id"]) if item["model_id"] == self.route["model_id"]),
            None,
        )
        if model is None:
            raise ValueError("The selected AI model is no longer available. Review the route before continuing.")
        self.connection, self.model = connection, model
        return grant, policy, option

    @staticmethod
    def _limits(grant, policy) -> dict[str, Any]:
        envelope = grant.envelope
        policy_requests = int(policy.get("max_requests") or 0)
        limits = {"max_requests": min(envelope.max_requests, policy_requests), "max_search_calls": envelope.max_search_calls}
        for field in ("run_budget_usd", "tender_budget_usd"):
            envelope_value = getattr(envelope, field)
            policy_value = policy.get(field)
            limits[field] = (
                min(Decimal(str(envelope_value)), Decimal(str(policy_value)))
                if envelope_value is not None and policy_value is not None
                else None
            )
        return limits

    def _reserve(self, estimated_input: int, max_output: int, requests: int) -> str:
        _integer(estimated_input, "The input allowance", minimum=0)
        _integer(max_output, "The output allowance", minimum=0)
        _integer(requests, "The request count", minimum=1)
        if max_output > self.route["max_output_tokens"]:
            raise ValueError("The requested output ceiling exceeds the approved route limit.")
        with self.service.connections.authority_guard(), self.repo.atomic() as conn:
            grant, policy, _option = self._validate_scope(conn)
            limits = self._limits(grant, policy)
            totals = self._root_totals(conn)
            if totals["native_overrun"]:
                raise ValueError("The provider exceeded the reviewed native-tool allowance. Further root work is paused.")
            if totals["search_overrun"]:
                raise ValueError("The hosted search allowance was exceeded by a provider response. Later root work is stopped for review.")
            if totals["requests"] + requests > limits["max_requests"]:
                raise ValueError("This approved Tender root reached its request allowance before finishing work.")

            metered = _metered_billing(self.connection)
            amount = route_request_cost(
                self.connection, self.model, self.route, estimated_input, max_output, requests
            )
            from .ai_native_tools import (
                native_charge_used,
                native_request_fields,
                selection_for_route,
            )
            native_fields = native_request_fields(self.route, self.model, requests)
            if native_fields:
                selected = selection_for_route(grant.envelope, _option, self.route, self.model)
                prior = [json.loads(row[0]) for row in conn.execute(
                    "SELECT data_json FROM ai_usage WHERE tender_id=? AND run_id=? AND connection_id=? AND model_id=?",
                    (self.tender_id, self.root_run_id, self.route["connection_id"], self.route["model_id"]))]
                prior = [value for value in prior if value.get("native_route_option_id") == _option.id]
                if selected.max_charge_usd is None or native_charge_used(prior) + Decimal(str(native_fields["reserved_code_cost_usd"])) > Decimal(str(selected.max_charge_usd)):
                    raise ValueError("The reviewed hosted-code allowance cannot cover this request.")
                native_fields["native_route_option_id"] = _option.id
            if metered and (limits["run_budget_usd"] is None or limits["tender_budget_usd"] is None):
                raise ValueError("Metered office work needs explicit reviewed run and Tender spending limits.")
            if metered and (
                totals["spent"] + totals["reserved"] + amount > limits["run_budget_usd"]
                or self.service._totals(conn, self.tender_id)[0]
                + self.service._totals(conn, self.tender_id)[1]
                + amount > limits["tender_budget_usd"]
            ):
                raise ValueError("The approved office budget cannot cover this request. Later model work is paused for review.")

            requested_search = route_search_reservation(self.route, requests)
            if requested_search and totals["search_unknown"]:
                raise ValueError("The root has incomplete historical online-search usage. Review its usage before continuing search work.")
            if totals["search_used"] + requested_search > limits["max_search_calls"]:
                raise ValueError("The approved online-search allowance cannot cover this request. The provider was not called.")

            identifier = new_id()
            data = {
                **self._metadata(grant.id),
                **native_fields,
                "actual_model": None,
                "billing": self.connection["billing"],
                "status": "reserved",
                "requests": requests,
                "input_tokens": 0,
                "output_tokens": 0,
                "estimated_cost_usd": None,
                "reserved_usd": float(amount),
                "reserved_search_calls": requested_search,
                "web_search_calls": 0,
                "search_accounting_version": 1,
                "search_usage_complete": requested_search == 0,
                "search_usage_uncertain": requested_search > 0,
                "search_allowance_overrun": False,
                "detail": (
                    "Conservative request allowance; hosted providers may control internal online-search calls."
                    if self.route["web_search"]
                    else "Conservative request allowance; provider billing may differ."
                ),
            }
            conn.execute(
                "INSERT INTO ai_usage VALUES(?,?,?,?,?,?,?)",
                (
                    identifier,
                    self.root_run_id,
                    self.tender_id,
                    self.route["connection_id"],
                    self.route["model_id"],
                    dump(data),
                    now(),
                ),
            )
            self.pending.add(identifier)
            return identifier

    async def before_request(self, estimated_input: int, max_output: int, requests: int = 1) -> str:
        """Synchronously admit one request before yielding to provider code."""

        return self._reserve(estimated_input, max_output, requests)

    def _report(self, usage: dict[str, Any], reservation_id: str) -> dict[str, Any] | None:
        if not isinstance(usage, dict) or not isinstance(reservation_id, str) or not reservation_id:
            raise ValueError("The model response has no valid budget reservation.")
        response_fingerprint = self._json(usage)
        with self.service.connections.authority_guard(), self.repo.atomic() as conn:
            row = conn.execute(
                "SELECT * FROM ai_usage WHERE id=? AND tender_id=? AND run_id=?",
                (reservation_id, self.tender_id, self.root_run_id),
            ).fetchone()
            if row is None:
                raise ValueError("The model response has no matching budget reservation for this root.")
            data = json.loads(row["data_json"])
            if self.model is None:
                self.model = next(
                    (item for item in self.service.connections.models(self.route["connection_id"])
                     if item["model_id"] == self.route["model_id"]),
                    None,
                )
            if self.model is None or self.connection is None:
                raise ValueError("The selected AI model is no longer available for this response.")
            if row["connection_id"] != self.route["connection_id"] or row["model_id"] != self.route["model_id"]:
                raise ValueError("The model response does not match the reserved AI route.")
            expected = self._metadata(data["grant_id"])
            if any(data.get(key) != value for key, value in expected.items()):
                raise ValueError("The model response does not match the reserved office identity.")
            if data.get("response_fingerprint") is not None:
                if data["response_fingerprint"] == response_fingerprint:
                    self.pending.discard(reservation_id)
                    return None
                raise ValueError("A different model response was already recorded for this reservation.")

            incoming = usage.get("input_tokens")
            outgoing = usage.get("output_tokens")
            tokens_complete = (
                usage.get("usage_complete", True) is not False
                and type(incoming) is int
                and incoming >= 0
                and type(outgoing) is int
                and outgoing >= 0
            )
            reported_search = usage.get("web_search_calls")
            search_observed = type(reported_search) is int and reported_search >= 0
            usage_complete = usage.get("usage_complete", True) is not False
            search_complete = (
                not self.route["web_search"]
                or (
                    usage_complete
                    and search_observed
                )
            )
            from .ai_native_tools import native_response_fields
            previous_ids = set()
            for previous in conn.execute(
                "SELECT data_json FROM ai_usage WHERE tender_id=? AND run_id=? AND connection_id=? AND model_id=? AND id<>?",
                (self.tender_id, self.root_run_id, self.route["connection_id"], self.route["model_id"], reservation_id)):
                previous_data = json.loads(previous[0])
                if previous_data.get("code_usage_complete") is True:
                    previous_ids.update(previous_data.get("code_execution_container_ids", []))
            native_fields, native_complete, code_sessions = native_response_fields(self.route, self.model, usage, data, previous_container_ids=previous_ids)
            complete = tokens_complete and search_complete and native_complete
            incoming = incoming if tokens_complete else 0
            outgoing = outgoing if tokens_complete else 0
            actual_requests = usage.get("requests")
            if type(actual_requests) is not int or actual_requests < 1:
                actual_requests = int(data.get("requests") or 1)
                complete = False if usage.get("requests") is not None else complete
            actual_search = int(reported_search) if search_observed and self.route["web_search"] else 0
            reserved_search = int(data.get("reserved_search_calls") or 0)
            previous_search = int(data.get("web_search_calls") or 0)
            observed_search = max(previous_search, actual_search)
            search_overrun = (
                data.get("search_allowance_overrun") is True
                or (search_observed and observed_search > reserved_search)
            )
            cost = None
            if complete and _metered_billing(self.connection):
                cost = route_usage_cost(
                    self.connection, self.model, self.route, incoming, outgoing, actual_search,
                    cached_input_tokens=usage.get("cached_input_tokens"),
                    code_sessions=code_sessions,
                )
            if complete:
                data["reserved_usd"] = 0
                data["reserved_search_calls"] = 0
            data.update(
                actual_model=usage.get("actual_model"),
                native_call_limit_overrun=(data.get("native_call_limit_overrun") is True or usage.get("native_call_limit_overrun") is True),
                input_tokens=incoming,
                output_tokens=outgoing,
                requests=max(1, int(actual_requests)),
                web_search_calls=observed_search,
                status="reported" if complete else "uncertain",
                estimated_cost_usd=cost,
                response_fingerprint=response_fingerprint,
                reserved_search_calls=0 if complete else reserved_search,
                search_accounting_version=1,
                search_usage_complete=not self.route["web_search"] or complete,
                search_usage_uncertain=self.route["web_search"] and not complete,
                search_allowance_overrun=bool(search_overrun),
                detail=(
                    "Provider token and online-search usage recorded; cost is an estimate, not an invoice."
                    if complete and _metered_billing(self.connection)
                    else "Original AI client usage is recorded for its account allowance; Quantix cannot establish an exact Tender charge."
                    if complete
                    else "Provider usage was incomplete. The conservative request and online-search allowance remains reserved."
                ),
            )
            if self.connection["protocol"] == "grok_build":
                data["detail"] = (
                    "Grok usage belongs to the account's shared allowance and any explicitly permitted extras. A reported model cost is not a confirmed Tender charge."
                    if complete
                    else "Grok usage details were incomplete. Its conservative allowance remains reserved; a reported model cost is not a confirmed Tender charge."
                )
            data.update(reported_usage_fields(usage))
            data.update(native_fields)
            conn.execute("UPDATE ai_usage SET data_json=? WHERE id=?", (dump(data), reservation_id))
            self.pending.discard(reservation_id)
        self.repo.event(
            self.root_run_id,
            "ai_usage",
            "AI usage recorded.",
            data
            | {
                "connection_id": self.route["connection_id"],
                "model_id": self.route["model_id"],
            },
        )
        return data

    async def on_response(self, usage: dict[str, Any], reservation_id: str) -> None:
        """Record one matching provider response exactly once."""

        self._report(usage, reservation_id)

    def interrupted(self, error: BaseException | None = None) -> None:
        """Settle outstanding reservations after a stopped call.

        A provider rejection before processing bills nothing, so its hold is
        released; any other stop keeps the hold uncertain until reconciled.
        """

        if not self.pending:
            return
        from .ai_api_errors import rejected_before_processing
        from .ai_reservations import released_usage

        rejected = rejected_before_processing(error)
        with self.service.connections.authority_guard(), self.repo.atomic() as conn:
            for identifier in tuple(self.pending):
                row = conn.execute(
                    "SELECT data_json FROM ai_usage WHERE id=? AND tender_id=? AND run_id=?",
                    (identifier, self.tender_id, self.root_run_id),
                ).fetchone()
                if row is None:
                    continue
                data = json.loads(row["data_json"])
                if data.get("status") == "reserved" and rejected:
                    conn.execute("UPDATE ai_usage SET data_json=? WHERE id=?", (dump(released_usage(data)), identifier))
                elif data.get("status") == "reserved":
                    data.update(
                        status="uncertain",
                        interrupted=True,
                        detail="The request ended without complete usage. Reserved cost and search allowance remain until reconciled.",
                    )
                    conn.execute("UPDATE ai_usage SET data_json=? WHERE id=?", (dump(data), identifier))
            self.pending.clear()

    def remaining_search_calls(self) -> int:
        """Return the root's currently available aggregate search allowance."""

        with self.service.connections.authority_guard(), self.repo.atomic() as conn:
            grant, policy, _option = self._validate_scope(conn)
            totals = self._root_totals(conn)
            if totals["search_unknown"]:
                return 0
            return max(0, self._limits(grant, policy)["max_search_calls"] - totals["search_used"])


__all__ = ["OfficeBudgetMeter"]
