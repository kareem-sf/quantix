"""Reviewed provider tools and hosted-code accounting shared by root meters."""
from __future__ import annotations

import hashlib
import ipaddress
import json
import re
from datetime import UTC, date, datetime
from decimal import Decimal
from urllib.parse import urlsplit

from pydantic import BaseModel, ConfigDict, Field, field_validator, model_validator

from .ai_generation_models import NativeToolGrant, NativeToolName


class NativeCodePrice(BaseModel):
    model_config = ConfigDict(extra="forbid", strict=True, frozen=True)
    per_session_usd: float = Field(gt=0, allow_inf_nan=False)
    source: str = Field(min_length=1, max_length=2000)
    as_of: str = Field(pattern=r"^\d{4}-\d{2}-\d{2}$")

    @field_validator("per_session_usd")
    @classmethod
    def conservative_openai_floor(cls, value):
        # Default 1 GB / 20-minute session, official pricing checked 2026-09-12.
        # https://developers.openai.com/api/docs/pricing#tools
        if value < .03:
            raise ValueError("Use at least the documented USD 0.03 full-session rate for the supported 1 GB OpenAI container.")
        return value

    @field_validator("as_of")
    @classmethod
    def observed_date(cls, value):
        if date.fromisoformat(value) > datetime.now(UTC).date():
            raise ValueError("The hosted-code price observation cannot be in the future.")
        return value

    @field_validator("source")
    @classmethod
    def documented_source(cls, value):
        parsed = urlsplit(value)
        if parsed.scheme != "https" or not parsed.hostname or parsed.username or parsed.password:
            raise ValueError("Record the documented HTTPS hosted-code pricing page.")
        return value


class ReviewedNativeTools(BaseModel):
    """Per-route selection inside a reviewed envelope; never an AIRoute grant."""
    model_config = ConfigDict(extra="forbid", strict=True, frozen=True)
    native_tools: list[NativeToolName] = Field(default_factory=list, max_length=3)
    web_fetch_domains: list[str] = Field(default_factory=list, max_length=30)
    uploaded_artifact_ids: list[str] = Field(default_factory=list, max_length=10)
    max_calls_per_request: int = Field(default=3, ge=1, le=20)
    max_charge_usd: float | None = Field(default=None, gt=0, allow_inf_nan=False)
    code_execution_price: NativeCodePrice | None = None

    @field_validator("web_fetch_domains")
    @classmethod
    def public_domains(cls, values):
        for value in values:
            try:
                ipaddress.ip_address(value)
            except ValueError:
                pass
            else:
                raise ValueError("Choose public domain names, not IP addresses.")
            if (len(value) > 253 or "." not in value or value.lower().endswith((".local", ".localhost", ".internal"))
                or any(not re.fullmatch(r"[a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?", label) for label in value.split("."))):
                raise ValueError("Choose valid public domain names without paths or wildcards.")
        return list(dict.fromkeys(value.lower() for value in values))

    @model_validator(mode="after")
    def complete_selection(self):
        if len(set(self.native_tools)) != len(self.native_tools) or len(set(self.uploaded_artifact_ids)) != len(self.uploaded_artifact_ids):
            raise ValueError("Select each native tool and source file once.")
        if "web_fetch" in self.native_tools and not self.web_fetch_domains:
            raise ValueError("Review the public domains before enabling provider page reading.")
        if "code_execution" in self.native_tools:
            if self.max_charge_usd is None or self.code_execution_price is None:
                raise ValueError("Hosted code requires documented session pricing and a charge allowance. Only explicitly selected originals may be uploaded.")
            if Decimal(str(self.max_charge_usd)) < Decimal(str(self.code_execution_price.per_session_usd)) * self.max_calls_per_request:
                raise ValueError("The hosted-code allowance cannot cover one request's approved call limit.")
        elif self.uploaded_artifact_ids or self.code_execution_price is not None or self.max_charge_usd is not None:
            raise ValueError("Hosted-code uploads and spending require an explicit code execution selection.")
        return self


def selection_for_route(envelope, option, route, model) -> ReviewedNativeTools:
    raw = getattr(envelope, "native_tools", {}).get(option.id)
    selection = ReviewedNativeTools.model_validate(raw or {})
    requested = set(route.get("native_tools") or [])
    if not requested.issubset(selection.native_tools):
        raise ValueError("Review the exact native tools for this route before using them.")
    requested_calls = max(int(route.get("max_native_tool_calls", 3)) if requested.intersection({"code_execution", "web_fetch"}) else 0,
                          int(route.get("max_search_calls", 3)) if "web_search" in requested else 0)
    if requested and requested_calls > selection.max_calls_per_request:
        raise ValueError("This route exceeds its reviewed native-tool call limit.")
    bases = {item.artifact_id for item in envelope.artifacts}
    if not set(selection.uploaded_artifact_ids).issubset(bases):
        raise ValueError("A hosted-code upload is outside the reviewed source files.")
    if "code_execution" in requested:
        card = model.get("pricing") or {}
        price = selection.code_execution_price
        if price is None or (price.per_session_usd != card.get("code_execution_per_session")
                            or price.source != card.get("code_execution_source")
                            or price.as_of != card.get("code_execution_as_of")):
            raise ValueError("The hosted-code price changed. Review its documented price and allowance again.")
    return selection


def grant_for_execution(context, connection, route) -> NativeToolGrant | None:
    """The single runtime entry point; only a currently validated root can grant."""
    if not route.get("native_tools"):
        return None
    from .staff_budget import OfficeBudgetMeter
    from .staff_routing import StaffRoutingService

    binding = getattr(context, "route_binding", None)
    plan_id = getattr(binding, "plan_id", None) or (getattr(context, "approved_scope", None) or {}).get("plan_id")
    if not plan_id:
        raise ValueError("Native tools require a current reviewed work plan.")
    routing = StaffRoutingService(context.repo)
    with routing.policy.connections.authority_guard(), context.repo.atomic():
        grant = routing.validate_root(context.tender_id, context.run_id, plan_id)
        if binding is not None:
            binding = routing.validate_binding(context.tender_id, binding.id)
            option = next((item for item in grant.envelope.route_options if item.id == binding.route_option_id), None)
        else:
            option = next((item for item in grant.envelope.route_options if OfficeBudgetMeter._route_within(item.route.model_dump(), route)), None)
        if (option is None or option.connection_revision != connection.get("revision")
            or route.get("connection_id") != connection.get("id")
            or not OfficeBudgetMeter._route_within(option.route.model_dump(), route)):
            raise ValueError("The native-tool route no longer matches its reviewed account and model.")
        current = routing.policy.connections.get(connection["id"])
        if any(connection.get(key) != current.get(key) for key in ("provider_id", "protocol", "base_url", "auth_type", "billing", "revision")):
            raise ValueError("The native-tool destination differs from the reviewed connection.")
        selected = selection_for_route(grant.envelope, option, route, option.model)
        artifacts = [item.model_dump() for item in grant.envelope.artifacts if item.artifact_id in selected.uploaded_artifact_ids]
        scope = hashlib.sha256(json.dumps([item.model_dump() for item in grant.envelope.artifacts], sort_keys=True).encode()).hexdigest()
        return NativeToolGrant(approval_id=grant.id, tender_id=context.tender_id, run_id=context.run_id,
            connection_id=connection["id"], connection_revision=connection["revision"], model_id=route["model_id"],
            source_scope_fingerprint=scope, native_tools=selected.native_tools,
            web_fetch_domains=selected.web_fetch_domains, max_calls_per_request=selected.max_calls_per_request,
            hosted_code_spend_usd=selected.max_charge_usd, uploaded_artifacts=artifacts,
            route_option_id=option.id, code_execution_price=selected.code_execution_price.model_dump() if selected.code_execution_price else None)


def native_request_fields(route, model, requests=1):
    if "code_execution" not in route.get("native_tools", []):
        return {}
    card = model.get("pricing") or {}
    price = NativeCodePrice(per_session_usd=card.get("code_execution_per_session"),
                            source=card.get("code_execution_source"), as_of=card.get("code_execution_as_of"))
    sessions = int(route.get("max_native_tool_calls", 3)) * requests
    cost = Decimal(str(price.per_session_usd)) * sessions
    return {"native_accounting_version": 1, "reserved_code_sessions": sessions,
            "reserved_code_cost_usd": float(cost), "code_execution_cost_usd": None,
            "code_execution_container_ids": [], "code_usage_complete": False, "code_allowance_overrun": False}


def native_response_fields(route, model, usage, saved, *, previous_container_ids=()):
    if "code_execution" not in route.get("native_tools", []):
        return {}, True, 0
    ids = usage.get("code_execution_container_ids")
    calls = usage.get("code_execution_calls")
    observed = isinstance(ids, list) and len(ids) <= 100 and all(isinstance(value, str) and re.fullmatch(r"[a-zA-Z0-9_-]{1,200}", value) for value in ids)
    complete = (usage.get("usage_complete") is True and saved.get("native_accounting_version") == 1
                and observed and type(calls) is int and calls >= 0 and (calls == 0 or 0 < len(ids) <= calls))
    ids = list(dict.fromkeys(ids)) if observed else []
    new_sessions = len(set(ids) - set(previous_container_ids))
    overrun = saved.get("code_allowance_overrun", False) or (type(calls) is int and calls > saved.get("reserved_code_sessions", 0))
    cost = float(Decimal(str(model["pricing"]["code_execution_per_session"])) * new_sessions) if complete else None
    return {"native_accounting_version": 1, "code_execution_container_ids": ids, "code_execution_calls": calls if type(calls) is int else None,
            "code_execution_cost_usd": cost, "code_usage_complete": complete, "code_allowance_overrun": overrun,
            "reserved_code_cost_usd": 0 if complete else saved.get("reserved_code_cost_usd", 0),
            "reserved_code_sessions": 0 if complete else saved.get("reserved_code_sessions", 0)}, complete, new_sessions


def native_charge_used(rows):
    values = [json.loads(row[0]) if not isinstance(row, dict) else row for row in rows]
    if any(value.get("code_allowance_overrun") for value in values):
        raise ValueError("The provider exceeded the reviewed hosted-code allowance. Further work is paused.")
    return sum((Decimal(str(value.get("reserved_code_cost_usd") or 0)) + Decimal(str(value.get("code_execution_cost_usd") or 0))
                for value in values), Decimal(0))
