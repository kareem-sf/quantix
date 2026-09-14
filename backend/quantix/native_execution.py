"""Immutable session scope and safe event projection for official clients."""
from __future__ import annotations

import hashlib
import json
import re

from .db import dump, new_id, now
from .native_execution_models import NativeClientCapability, NativeSessionBinding
from .run_activity import ActivityRecorder

_SAFE_ID = re.compile(r"[A-Za-z0-9_-]{1,200}")


def _fingerprint(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(",", ":"), default=str).encode()).hexdigest()


def client_capabilities(protocol):
    if protocol not in {"codex", "grok_build"}:
        raise ValueError("Choose a supported original client.")
    docs = "https://learn.chatgpt.com/docs/codex-sdk" if protocol == "codex" else "https://docs.x.ai/build/cli/headless-scripting"
    return [NativeClientCapability(id=identifier, origin=origin, supported=supported, detail=detail, source=docs)
        for identifier, origin, supported, detail in (
            ("managed_session", "client", True, "The original client's exact session can continue only under the same profile, account, model, runtime and source scope."),
            ("output_token_limit", "client", protocol == "grok_build", "Grok applies the configured output limit." if protocol == "grok_build" else "The pinned Codex client exposes no hard output-token limit. Its route allowance is not a provider-enforced ceiling; use a direct API for that requirement."),
            ("native_files_shell", "client", False, "Host file and shell tools are disabled; platform isolation has not been qualified for arbitrary native actions."),
            ("native_search", "client", False, "A verifiable native search allowance and source receipt are not connected. Use the reviewed Quantix public-reading tools."),
            ("native_subagents", "client", False, "Native child agents lack root spending admission. Use reviewed Quantix assignments."),
            ("scoped_tools", "quantix", True, "The shared Quantix MCP bridge enforces exact tools, source permissions and publication validation."),
            ("sandboxed_code", "quantix", True, "Reviewed code work uses the separately prepared Quantix isolated code runtime; runtime readiness remains required."),
        )]


class NativeExecutionService:
    def __init__(self, repo):
        self.repo = repo
        with repo.atomic() as conn:
            conn.execute("""CREATE TABLE IF NOT EXISTS native_client_sessions(
                id TEXT PRIMARY KEY,tender_id TEXT NOT NULL,run_id TEXT NOT NULL,
                connection_id TEXT NOT NULL,state TEXT NOT NULL,data_json TEXT NOT NULL,
                created_at TEXT NOT NULL,updated_at TEXT NOT NULL)""")
            conn.execute("""CREATE TABLE IF NOT EXISTS native_client_session_turns(
                id TEXT PRIMARY KEY,session_id TEXT NOT NULL,run_id TEXT NOT NULL,
                state TEXT NOT NULL,binding_json TEXT NOT NULL,created_at TEXT NOT NULL,updated_at TEXT NOT NULL)""")

    def prepare(self, context, connection, route, *, operation="execute", tools=(), resume_session_id=None):
        if context is None or operation == "check":
            return None
        from .ai_connections import AIConnectionService
        connections = AIConnectionService(self.repo)
        with connections.authority_guard(), self.repo.atomic() as conn:
            run = self.repo.get_run(context.run_id)
            if run["tender_id"] != context.tender_id or run["status"] not in {"queued", "running"}:
                raise ValueError("Only current active Tender work can start an original-client session.")
            current = connections.get(connection["id"])
            if current["revision"] != connection["revision"]:
                raise ValueError("The original-client account changed before session admission.")
            from .manager_runtime import ManagerRunProfiles
            try:
                profile = ManagerRunProfiles(self.repo).get(context.tender_id, context.run_id)
            except KeyError:
                raise ValueError("This work needs its immutable Tender Manager profile pin before starting an original-client session.") from None
            profile_id, version = (context.actor_id, profile.version) if getattr(context, "is_staff", False) else (profile.id, profile.version)
            runtime = connection.get("_checked_component_version")
            if not runtime:
                raise ValueError("Check this exact original-client software and model before creating a managed session.")
            artifacts = []
            if operation != "conversation":
                artifacts = [{key: artifact[key] for key in ("id", "version", "content_hash")} for artifact in self.repo.list_artifacts(context.tender_id)]
            scope = _fingerprint({"artifacts": artifacts, "approved_scope": getattr(context, "approved_scope", None),
                                  "tools": list(tools), "operation": operation})
            observed_model = connection.get("_model") or {}
            settings = _fingerprint({"route": route, "model": {key: observed_model.get(key)
                for key in ("model_id", "capabilities", "pricing", "source")}})
            binding = NativeSessionBinding(id=new_id(), tender_id=context.tender_id, run_id=context.run_id,
                protocol=connection["protocol"], connection_id=connection["id"], connection_revision=connection["revision"],
                model_id=route["model_id"], runtime_revision=runtime, profile_id=profile_id,
                profile_version=version, scope_fingerprint=scope, settings_fingerprint=settings)
            if resume_session_id is None and conn.execute("SELECT 1 FROM sqlite_master WHERE type='table' AND name='office_resume_origins'").fetchone():
                origin = conn.execute("SELECT previous_run_id FROM office_resume_origins WHERE tender_id=? AND root_run_id=?", (context.tender_id, context.run_id)).fetchone()
                if origin is not None:
                    candidates = conn.execute("SELECT data_json FROM native_client_sessions WHERE tender_id=? AND run_id=? AND connection_id=? ORDER BY updated_at DESC",
                        (context.tender_id, origin[0], connection["id"])).fetchall()
                    fields = ("protocol", "connection_revision", "model_id", "runtime_revision", "profile_id", "profile_version", "scope_fingerprint", "settings_fingerprint")
                    for candidate_row in candidates:
                        candidate = NativeSessionBinding.model_validate_json(candidate_row[0])
                        if (candidate.provider_session_id and candidate.state in {"completed", "interrupted", "running"}
                            and all(getattr(candidate, name) == getattr(binding, name) for name in fields)):
                            resume_session_id = candidate.id
                            break
            if resume_session_id:
                row = conn.execute("SELECT data_json FROM native_client_sessions WHERE tender_id=? AND id=?", (context.tender_id, resume_session_id)).fetchone()
                if row is None:
                    raise ValueError("The original-client session does not belong to this Tender.")
                previous = NativeSessionBinding.model_validate_json(row[0])
                if previous.state == "running" and self.repo.get_run(previous.run_id)["status"] in {"failed", "cancelled", "interrupted"}:
                    previous = previous.model_copy(update={"state": "interrupted"})
                fields = ("protocol", "connection_id", "connection_revision", "model_id", "runtime_revision", "profile_id", "profile_version", "scope_fingerprint", "settings_fingerprint")
                if previous.state not in {"completed", "interrupted"} or not previous.provider_session_id or any(getattr(previous, name) != getattr(binding, name) for name in fields):
                    raise ValueError("The original-client session is incompatible with the current profile, account, model, runtime or reviewed scope. Start a new session.")
                if previous.protocol == "grok_build":
                    from uuid import UUID
                    try:
                        UUID(previous.provider_session_id)
                    except ValueError:
                        raise ValueError("Grok continuation needs its exact native UUID. Start a new session instead of matching a title.") from None
                binding = binding.model_copy(update={"id": previous.id, "provider_session_id": previous.provider_session_id})
                conn.execute("UPDATE native_client_sessions SET run_id=?,state=?,data_json=?,updated_at=? WHERE id=?", (context.run_id, "prepared", dump(binding.model_dump()), now(), previous.id))
            else:
                conn.execute("INSERT INTO native_client_sessions VALUES(?,?,?,?,?,?,?,?)",
                    (binding.id, binding.tender_id, binding.run_id, binding.connection_id, binding.state, dump(binding.model_dump()), now(), now()))
            conn.execute("INSERT INTO native_client_session_turns VALUES(?,?,?,?,?,?,?)",
                (new_id(), binding.id, binding.run_id, "prepared", dump(binding.model_dump()), now(), now()))
            return binding

    def finish(self, context, binding_id, *, provider_session_id=None, state="completed"):
        if state not in {"running", "completed", "interrupted", "failed"}:
            raise ValueError("Unsupported original-client session state.")
        if provider_session_id is not None and (not isinstance(provider_session_id, str) or not _SAFE_ID.fullmatch(provider_session_id)):
            raise ValueError("The original client returned an invalid session identifier.")
        with self.repo.atomic() as conn:
            row = conn.execute("SELECT data_json FROM native_client_sessions WHERE tender_id=? AND id=? AND run_id=?", (context.tender_id, binding_id, context.run_id)).fetchone()
            if row is None:
                raise ValueError("The session update no longer belongs to this execution.")
            saved = NativeSessionBinding.model_validate_json(row[0])
            if saved.state in {"completed", "interrupted", "failed", "incompatible"} and state == "running":
                raise ValueError("A late native session event cannot restart settled work.")
            if saved.provider_session_id and provider_session_id and saved.provider_session_id != provider_session_id:
                raise ValueError("The original client changed its session identity unexpectedly.")
            updated = saved.model_copy(update={"state": state, "provider_session_id": provider_session_id or saved.provider_session_id})
            conn.execute("UPDATE native_client_sessions SET state=?,data_json=?,updated_at=? WHERE id=?", (state, dump(updated.model_dump()), now(), binding_id))
            conn.execute("UPDATE native_client_session_turns SET state=?,binding_json=?,updated_at=? WHERE session_id=? AND run_id=? AND state IN ('prepared','running')",
                (state, dump(updated.model_dump()), now(), binding_id, context.run_id))
            return updated

    def list(self, tender_id):
        self.repo.get_tender(tender_id)
        with self.repo.db.connect() as conn:
            return [NativeSessionBinding.model_validate_json(row[0]) for row in conn.execute(
                "SELECT data_json FROM native_client_sessions WHERE tender_id=? ORDER BY updated_at DESC LIMIT 100", (tender_id,))]


def project_client_event(context, kind, data, *, request_operation=None):
    if context is None:
        return
    if kind not in {"assistant_text_delta", "assistant_reasoning_summary", "runtime_public_section"} or not isinstance(data, dict):
        raise ValueError("Unsupported original-client response event.")
    text = data.get("text")
    if not isinstance(text, str) or not text:
        raise ValueError("The original-client response event has no text.")
    values = {"text": text, "origin": "client"}
    if request_operation:
        values["activity_operation_id"] = request_operation
    if data.get("reset") is True:
        values["reset"] = True
    for name in ("request_id", "item_id", "summary_index", "delta", "complete"):
        if name in data:
            values[name] = data[name]
    for name in ("assignment_id", "actor_id"):
        value = getattr(context, name, None)
        if isinstance(value, str) and value:
            values[name] = value
    from .activity_privacy import sanitize
    values, _, _ = sanitize(values, getattr(context, "_activity_secrets", ()))
    with context.repo.atomic():
        active = context.repo.get_run(context.run_id)["status"] in {"queued", "running"}
        settled_section = request_operation and data.get("delta") is False
        if not active and not settled_section:
            raise InterruptedError("This original-client work has stopped.")
        if active and kind != "runtime_public_section":
            context.repo.event(context.run_id, kind, "Original client response draft.", values)
        if request_operation:
            category = "reasoning_summary" if kind == "assistant_reasoning_summary" else "draft"
            key = (request_operation, category, data.get("item_id"), data.get("summary_index"))
            sections = getattr(context, "_activity_client_sections", None)
            if sections is None:
                context._activity_client_sections = sections = {}
            recorder = ActivityRecorder(context)
            phase = "completed" if data.get("complete") else "observed" if settled_section else "delta"
            capture_status = "partial" if settled_section and not data.get("complete") else "complete"
            operation = sections.get(key)
            if operation is None:
                operation = recorder.start(category, "Original client supplied a reasoning summary." if category == "reasoning_summary" else "Original client response draft.",
                    values, phase=phase if settled_section else "started", parent_operation_id=request_operation, capture_status=capture_status)
                sections[key] = operation
            else:
                recorder.record(operation, category, phase, "Original client supplied a reasoning summary." if category == "reasoning_summary" else "Original client response draft.", values, capture_status=capture_status)
