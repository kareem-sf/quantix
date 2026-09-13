"""Enrich the existing receipt files; this is not another execution store."""

import hashlib
import json
from dataclasses import asdict

from .execution_context import OfficeExecutionIdentity, identity_from_office_context
from .sandbox_protocol import write_record


def record_hash(value):
    return hashlib.sha256(
        json.dumps(
            value, sort_keys=True, ensure_ascii=False, allow_nan=False, separators=(",", ":")
        ).encode()
    ).hexdigest()


def write_local_record(
    path, payload, *, context, engine, phase, runtime_fingerprint=None
):
    identity = (
        context
        if isinstance(context, OfficeExecutionIdentity)
        else identity_from_office_context(context)
    )
    value = {
        **payload,
        "record_version": 1,
        "engine_kind": engine,
        "phase": phase,
        "tender_id": identity.tender_id,
        "root_run_id": identity.root_run_id,
        "execution_identity": asdict(identity),
        "runtime_fingerprint": runtime_fingerprint,
    }
    logs = []
    for stream in ("stdout", "stderr"):
        data = str(value.get(stream, "")).encode()
        if len(data) > 12000:
            path.parent.mkdir(parents=True, exist_ok=True)
            (path.parent / (stream + ".txt")).write_bytes(data)
            logs.append(
                {
                    "name": stream + ".txt",
                    "size_bytes": len(data),
                    "sha256": hashlib.sha256(data).hexdigest(),
                }
            )
            value[stream] = data[:12000].decode("utf-8", "replace")
    value["logs"] = logs
    value["record_sha256"] = record_hash(value)
    write_record(path, value)
    repo = getattr(context, "repo", None)
    if repo is not None and identity.root_run_id:
        repo.event(
            identity.root_run_id,
            "local_code_progress",
            "Local code: " + phase.replace("_", " ") + ".",
            {
                "receipt_id": value["id"],
                "engine": engine,
                "phase": phase,
                "actor_id": identity.actor_id,
                "assignment_id": identity.assignment_id,
            },
        )
