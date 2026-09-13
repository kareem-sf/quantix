"""Approved OpenAI hosted Python: exact uploads, private artifacts and cleanup."""
from __future__ import annotations

import asyncio
import dataclasses
import hashlib
import inspect
import json
import re
import time
from pathlib import Path

from pydantic import BaseModel, ConfigDict, Field

from .ai_generation_models import NativeToolGrant, NativeUploadArtifact
from .db import dump, new_id, now

MAX_FILE_BYTES = 20 * 1024 * 1024
MAX_TOTAL_BYTES = 50 * 1024 * 1024
_ID = re.compile(r"[A-Za-z0-9_-]{1,200}")


class NativeArtifact(BaseModel):
    model_config = ConfigDict(extra="forbid")
    id: str
    tender_id: str
    run_id: str
    filename: str
    size_bytes: int
    content_hash: str
    origin: str = "provider_code"
    status: str = "unreviewed"
    input_artifacts: list[NativeUploadArtifact] = Field(default_factory=list)
    actor_id: str | None = None
    assignment_id: str | None = None
    connection_id: str | None = None
    model_id: str | None = None


class NativeArtifactService:
    def __init__(self, repo):
        self.repo = repo
        with repo.atomic() as conn:
            conn.execute("""CREATE TABLE IF NOT EXISTS native_code_artifacts(
                id TEXT PRIMARY KEY,tender_id TEXT NOT NULL,run_id TEXT NOT NULL,
                provider_file_id TEXT NOT NULL,container_id TEXT NOT NULL,data_json TEXT NOT NULL,
                relative_path TEXT NOT NULL,created_at TEXT NOT NULL,
                UNIQUE(tender_id,run_id,container_id,provider_file_id))""")
            conn.execute("""CREATE TABLE IF NOT EXISTS native_code_cleanup(
                id TEXT PRIMARY KEY,tender_id TEXT NOT NULL,run_id TEXT NOT NULL,connection_id TEXT NOT NULL,
                connection_revision INTEGER NOT NULL,kind TEXT NOT NULL,provider_id TEXT,
                state TEXT NOT NULL,rationale TEXT,created_at TEXT NOT NULL,updated_at TEXT NOT NULL)""")

    def cleanup_record(self, context, connection, kind, provider_id=None, *, state="pending"):
        identifier = new_id()
        with self.repo.atomic() as conn:
            conn.execute("INSERT INTO native_code_cleanup VALUES(?,?,?,?,?,?,?,?,?,?,?)", (identifier, context.tender_id,
                context.run_id, connection["id"], connection["revision"], kind, provider_id, state, None, now(), now()))
        return identifier

    def cleanup_status(self, identifier, state, *, pending_only=False):
        with self.repo.atomic() as conn:
            query = "UPDATE native_code_cleanup SET state=?,updated_at=? WHERE id=?" + (" AND state='pending'" if pending_only else "")
            conn.execute(query, (state, now(), identifier))

    def cleanup_identify(self, identifier, provider_id):
        with self.repo.atomic() as conn:
            conn.execute("UPDATE native_code_cleanup SET provider_id=?,updated_at=? WHERE id=? AND state='pending'", (provider_id, now(), identifier))

    def reconcile_cleanup(self, tender_id, receipt_id, *, engineer_confirmed, rationale):
        if engineer_confirmed is not True or not isinstance(rationale, str) or not rationale.strip() or len(rationale) > 4000:
            raise ValueError("Confirm the provider cleanup review and record its outcome.")
        with self.repo.atomic() as conn:
            row = conn.execute("SELECT id,state,run_id FROM native_code_cleanup WHERE id=? AND tender_id=? AND state IN ('uncertain','pending')", (receipt_id, tender_id)).fetchone()
            if row is None:
                raise ValueError("This uncertain cleanup receipt was not found.")
            if row["state"] == "pending" and self.repo.get_run(row["run_id"])["status"] in {"queued", "running"}:
                raise ValueError("Stop or finish the provider work before reviewing its pending cleanup.")
            conn.execute("UPDATE native_code_cleanup SET state='reviewed',rationale=?,updated_at=? WHERE id=?", (rationale.strip(), now(), receipt_id))
            conn.execute("INSERT INTO decisions VALUES(?,?,?,?,?,?,?)", (new_id(), tender_id, "native_cleanup", receipt_id,
                "review_provider_cleanup", rationale.strip(), now()))
        return {"ok": True}

    def pending_cleanup(self, tender_id):
        self.repo.get_tender(tender_id)
        with self.repo.db.connect() as conn:
            return [dict(row) for row in conn.execute("""SELECT c.id,c.run_id,c.connection_id,c.connection_revision,c.kind,c.state,c.created_at
                FROM native_code_cleanup c LEFT JOIN runs r ON r.id=c.run_id
                WHERE c.tender_id=? AND (c.state='uncertain' OR (c.state='pending' AND (r.status IS NULL OR r.status NOT IN ('queued','running'))))
                ORDER BY c.created_at LIMIT 100""", (tender_id,))]

    def save(self, context, container_id, file_id, filename, content, *, input_artifacts=(), connection=None, model_id=None):
        if len(content) > MAX_FILE_BYTES:
            raise ValueError("A provider-generated file exceeds the download limit.")
        identifier = new_id()
        filename = Path(filename.replace("\\", "/")).name[:150] if isinstance(filename, str) and filename else "provider-output.bin"
        filename = re.sub(r"[\x00-\x1f\x7f]", "", filename) or "provider-output.bin"
        suffix = Path(filename).suffix if re.fullmatch(r"\.[A-Za-z0-9]{1,10}", Path(filename).suffix) else ".bin"
        relative = Path("outputs") / "native-code" / context.tender_id / context.run_id / f"{identifier}{suffix}"
        target = (self.repo.home / relative).resolve()
        if not target.is_relative_to(self.repo.home.resolve()):
            raise ValueError("The generated draft directory must remain inside the Quantix home.")
        target.parent.mkdir(parents=True, exist_ok=True)
        artifact = NativeArtifact(id=identifier, tender_id=context.tender_id, run_id=context.run_id,
            filename=filename, size_bytes=len(content), content_hash=hashlib.sha256(content).hexdigest(), input_artifacts=list(input_artifacts),
            actor_id=getattr(context, "actor_id", None), assignment_id=getattr(context, "assignment_id", None),
            connection_id=connection.get("id") if connection else None, model_id=model_id)
        target.write_bytes(content)
        try:
            with self.repo.atomic() as conn:
                run = self.repo.get_run(context.run_id)
                if run["tender_id"] != context.tender_id or run["status"] not in {"queued", "running"}:
                    raise InterruptedError("The provider-generated draft arrived after its work stopped.")
                existing = conn.execute("SELECT data_json FROM native_code_artifacts WHERE tender_id=? AND run_id=? AND container_id=? AND provider_file_id=?",
                    (context.tender_id, context.run_id, container_id, file_id)).fetchone()
                if existing:
                    target.unlink(missing_ok=True)
                    return NativeArtifact.model_validate_json(existing[0])
                conn.execute("INSERT INTO native_code_artifacts VALUES(?,?,?,?,?,?,?,?)",
                    (identifier, context.tender_id, context.run_id, file_id, container_id, dump(artifact.model_dump()), relative.as_posix(), now()))
                self.repo.event(context.run_id, "native_artifact_saved", "A provider-generated draft file is ready for review.",
                                {"artifact_id": identifier, "filename": filename, "origin": "provider_code",
                                 "actor_id": artifact.actor_id, "assignment_id": artifact.assignment_id})
        except BaseException:
            target.unlink(missing_ok=True)
            raise
        return artifact

    def list(self, tender_id, run_id):
        run = self.repo.get_run(run_id)
        if run["tender_id"] != tender_id:
            raise KeyError("This work does not belong to the Tender.")
        with self.repo.db.connect() as conn:
            return [NativeArtifact.model_validate_json(row[0]) for row in conn.execute(
                "SELECT data_json FROM native_code_artifacts WHERE tender_id=? AND run_id=? ORDER BY created_at,id LIMIT 100",
                (tender_id, run_id))]

    def path(self, tender_id, artifact_id):
        with self.repo.db.connect() as conn:
            row = conn.execute("SELECT relative_path,data_json FROM native_code_artifacts WHERE tender_id=? AND id=?", (tender_id, artifact_id)).fetchone()
        if row is None:
            raise KeyError("This generated draft was not found.")
        path = (self.repo.home / row[0]).resolve()
        if not path.is_relative_to((self.repo.home / "outputs" / "native-code").resolve()):
            raise ValueError("The generated file location is invalid.")
        if not path.is_file() or path.stat().st_size > MAX_FILE_BYTES or hashlib.sha256(path.read_bytes()).hexdigest() != json.loads(row[1])["content_hash"]:
            raise ValueError("The generated file changed or is unavailable.")
        return path


class HostedCodeExecution:
    def __init__(self, context, connection, route, client):
        self.context, self.connection, self.route, self.client = context, connection, route, client
        self.grant = NativeToolGrant.model_validate(connection.get("_native_tool_grant"))
        self.uploaded = []
        self.artifacts = []
        self.container_ids = set()
        self.call_containers = {}
        self.downloaded = set()
        self.received_bytes = 0
        self.started = time.monotonic()
        self.cleanup = {}
        self.pending_usage = []
        self.request_cleanup = {}
        self.awaiting_response = set()
        self.storage = NativeArtifactService(context.repo)
        if connection.get("provider_id") != "openai" or connection.get("protocol") != "openai_responses":
            raise ValueError("Hosted code is connected only to the official OpenAI Responses adapter.")
        if connection.get("base_url") and connection["base_url"].rstrip("/") != "https://api.openai.com/v1":
            raise ValueError("Hosted code requires the official OpenAI endpoint. Custom gateway container behavior is not established.")
        if not self.grant.hosted_code_spend_usd or not self.grant.code_execution_price:
            raise ValueError("Review hosted-code spending before execution. Only explicitly selected files may be uploaded.")
        if any(item["connection_id"] == connection["id"] and item["connection_revision"] == connection["revision"] for item in self.storage.pending_cleanup(context.tender_id)):
            raise ValueError("Review the previous uncertain provider cleanup before starting another hosted-code session.")

    def _active(self):
        if time.monotonic() - self.started >= 900:
            raise InterruptedError("The hosted-code session reached its 15-minute application lifetime. Review saved work before another session.")
        run = self.context.repo.get_run(self.context.run_id)
        if run["tender_id"] != self.context.tender_id:
            raise ValueError("The hosted-code work does not belong to this Tender.")
        if run["status"] not in {"queued", "running"}:
            raise InterruptedError("The hosted-code work was stopped.")
        if any((self.grant.tender_id != self.context.tender_id, self.grant.run_id != self.context.run_id,
                self.grant.connection_id != self.connection.get("id"),
                self.grant.connection_revision != self.connection.get("revision"), self.grant.model_id != self.route.get("model_id"))):
            raise ValueError("The hosted-code grant does not match this execution.")

    async def prepare(self, parameters, reservation_id):
        from pydantic_ai.messages import UploadedFile
        from pydantic_ai.native_tools import CodeExecutionTool
        self._active()
        if len(self.container_ids) >= 20:
            raise ValueError("This hosted-code execution reached its container boundary. Review the saved work before a new session.")
        if not isinstance(reservation_id, str) or not reservation_id:
            raise ValueError("Reserve hosted-code and inference spending before uploading source files.")
        if not self.uploaded:
            total = 0
            sources = []
            for basis in self.grant.uploaded_artifacts:
                artifact = self.context.repo.get_artifact(self.context.tender_id, basis.artifact_id)
                if not artifact.get("is_current") or artifact["version"] != basis.version or artifact["content_hash"] != basis.content_hash:
                    raise ValueError("A reviewed hosted-code input changed. Review the current original file again.")
                path = self.context.repo.object_path(self.context.tender_id, basis.artifact_id)
                if path.stat().st_size > MAX_FILE_BYTES:
                    raise ValueError("A hosted-code input exceeds the upload limit.")
                content = path.read_bytes()
                total += len(content)
                if total > MAX_TOTAL_BYTES or hashlib.sha256(content).hexdigest() != basis.content_hash:
                    raise ValueError("The hosted-code inputs exceed the limit or differ from the reviewed files.")
                sources.append((Path(artifact["relative_path"]).name, content))
            for filename, content in sources:
                self._active()
                receipt_id = self.storage.cleanup_record(self.context, self.connection, "upload")
                try:
                    created = await self.client.files.create(file=(filename, content), purpose="assistants")
                except BaseException:
                    self.storage.cleanup_status(receipt_id, "uncertain")
                    raise
                if not isinstance(created.id, str) or not _ID.fullmatch(created.id):
                    self.storage.cleanup_status(receipt_id, "uncertain")
                    raise ValueError("The provider returned an invalid uploaded-file identifier.")
                self.uploaded.append(created.id)
                self.storage.cleanup_identify(receipt_id, created.id)
                self.cleanup[("upload", created.id)] = receipt_id
        self.request_cleanup[reservation_id] = self.storage.cleanup_record(self.context, self.connection, "container")
        self.awaiting_response.add(reservation_id)
        return dataclasses.replace(parameters, native_tools=[
            CodeExecutionTool(files=[UploadedFile(file_id=identifier, provider_name="openai") for identifier in self.uploaded])
            if isinstance(tool, CodeExecutionTool) else tool for tool in parameters.native_tools])

    def observe(self, response, reservation_id=None):
        complete_identity = True
        for part in response.parts:
            if (getattr(part, "tool_name", None) == "code_execution" and getattr(part, "part_kind", None) == "builtin-tool-call"
                and getattr(part, "provider_name", None) == "openai"):
                args = getattr(part, "args", None)
                if isinstance(args, dict) and isinstance(args.get("container_id"), str) and _ID.fullmatch(args["container_id"]):
                    identifier = args["container_id"]
                    if isinstance(getattr(part, "tool_call_id", None), str):
                        self.call_containers[part.tool_call_id] = identifier
                    if identifier not in self.container_ids:
                        self.container_ids.add(identifier)
                        self.cleanup[("container", identifier)] = self.storage.cleanup_record(self.context, self.connection, "container", identifier)
                else:
                    complete_identity = False
        if reservation_id in self.request_cleanup and response.state == "complete" and complete_identity:
            self.storage.cleanup_status(self.request_cleanup[reservation_id], "removed")
            self.awaiting_response.discard(reservation_id)

    def request_rejected(self, reservation_id):
        if reservation_id in self.request_cleanup:
            self.storage.cleanup_status(self.request_cleanup[reservation_id], "removed")
            self.awaiting_response.discard(reservation_id)

    def defer_usage(self, usage, reservation_id, callback):
        # Money remains reserved for the entire bounded hosted environment.
        # Only a confirmed close can release its container exposure. Each
        # inference response is still reported exactly once to the root meter.
        self.pending_usage.append((usage, reservation_id, callback))

    async def collect(self, response):
        from pydantic_ai.messages import BinaryContent, FilePart, TextPart
        self._active()
        self.observe(response)
        for part in response.parts:
            if isinstance(part, FilePart) and isinstance(part.content, BinaryContent) and part.id in self.call_containers:
                content = part.content.data
                reference = f"inline-{part.id}-{hashlib.sha256(content).hexdigest()[:16]}"
                container = self.call_containers[part.id]
                if (container, reference) not in self.downloaded:
                    self.received_bytes += len(content)
                    if len(content) > MAX_FILE_BYTES or self.received_bytes > MAX_TOTAL_BYTES or len(self.artifacts) >= 100:
                        raise ValueError("Provider-generated artifacts exceed the private output limits.")
                    suffix = {"image/png": ".png", "image/jpeg": ".jpg", "image/webp": ".webp"}.get(part.content.media_type, ".bin")
                    artifact = self.storage.save(self.context, container, reference, "code-output" + suffix, content,
                        input_artifacts=self.grant.uploaded_artifacts, connection=self.connection, model_id=self.route["model_id"])
                    self.artifacts.append(artifact.model_dump())
                    self.downloaded.add((container, reference))
                continue
            if not isinstance(part, TextPart):
                continue
            for citation in (part.provider_details or {}).get("annotations", []):
                if not isinstance(citation, dict) or citation.get("type") != "container_file_citation":
                    continue
                container, file_id = citation.get("container_id"), citation.get("file_id")
                if container not in self.container_ids or not isinstance(file_id, str) or not _ID.fullmatch(file_id):
                    raise ValueError("A generated file citation does not belong to the observed hosted-code execution.")
                if (container, file_id) in self.downloaded:
                    continue
                if len(self.artifacts) >= 100:
                    raise ValueError("The provider-generated artifact count exceeds this execution's limit.")
                self._active()
                chunks = []
                size = 0
                async with self.client.containers.files.content.with_streaming_response.retrieve(file_id, container_id=container) as downloaded:
                    async for chunk in downloaded.iter_bytes():
                        size += len(chunk)
                        self.received_bytes += len(chunk)
                        if size > MAX_FILE_BYTES or self.received_bytes > MAX_TOTAL_BYTES:
                            raise ValueError("Provider-generated files exceed the private download limit.")
                        chunks.append(chunk)
                self._active()
                artifact = self.storage.save(self.context, container, file_id, citation.get("filename"), b"".join(chunks), input_artifacts=self.grant.uploaded_artifacts,
                    connection=self.connection, model_id=self.route["model_id"])
                self.artifacts.append(artifact.model_dump())
                self.downloaded.add((container, file_id))

    async def close(self):
        uncertain = bool(self.awaiting_response)
        for reservation_id in self.awaiting_response:
            self.storage.cleanup_status(self.request_cleanup[reservation_id], "uncertain")
        cancellation = None
        for (kind, identifier), receipt_id in self.cleanup.items():
            try:
                operation = self.client.files.delete(identifier) if kind == "upload" else self.client.containers.delete(identifier)
                deleted = await asyncio.wait_for(operation, timeout=5)
                if kind == "upload" and getattr(deleted, "deleted", True) is not True:
                    raise ValueError("The provider did not confirm removal of its temporary upload.")
                self.storage.cleanup_status(receipt_id, "removed")
            except BaseException as error:
                uncertain = True
                self.storage.cleanup_status(receipt_id, "uncertain")
                self.context.repo.event(self.context.run_id, "native_upload_cleanup_uncertain",
                    "A temporary provider resource could not be removed. Review provider cleanup and spending before another hosted-code session.", {"cleanup_receipt_id": receipt_id})
                if isinstance(error, asyncio.CancelledError):
                    cancellation = error
                    for pending_receipt in self.cleanup.values():
                        self.storage.cleanup_status(pending_receipt, "uncertain", pending_only=True)
                    break
        for usage, reservation_id, callback in self.pending_usage:
            if uncertain:
                usage["usage_complete"] = False
                usage["provider_usage_is_incomplete"] = True
            if callback is not None:
                result = callback(usage, reservation_id)
                if inspect.isawaitable(result):
                    await result
        self.pending_usage.clear()
        if cancellation is not None:
            raise cancellation
