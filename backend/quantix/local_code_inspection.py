"""Scoped, bounded views over the existing Monty/Python receipt files."""

import base64
import hashlib
import json
import re

from .local_code_models import LocalCodeDetail, LocalCodeFile, LocalCodePage, LocalCodeRun
from .local_code_records import record_hash
from .sandbox_protocol import safe_name, sha256


class LocalCodeInspection:
    def __init__(self, repo):
        self.repo = repo

    def _path(self, engine, identifier):
        if engine not in {"monty", "python"} or not re.fullmatch(r"[a-f0-9]{32}", identifier):
            raise KeyError("Local code record not found.")
        root = self.repo.home / "code"
        return (
            root / "receipts" / (identifier + ".json")
            if engine == "monty"
            else root / "runs" / identifier / "receipt.json"
        )

    def _safe_file(self, path, maximum):
        if (
            not path.is_file()
            or path.is_symlink()
            or not path.resolve().is_relative_to(self.repo.home.resolve() / "code")
            or path.stat().st_size > maximum
        ):
            raise ValueError("The saved local-code record is missing, linked or oversized.")
        current = path.parent
        while current != self.repo.home:
            if current.is_symlink() or (hasattr(current, "is_junction") and current.is_junction()):
                raise ValueError("Linked local-code records cannot be inspected.")
            current = current.parent
        return path

    def _read(self, path):
        value = json.loads(self._safe_file(path, 8 * 1024 * 1024).read_text(encoding="utf-8"))
        if not isinstance(value, dict):
            raise ValueError("The saved local-code record is invalid.")
        if value.get("record_version"):
            body = {key: data for key, data in value.items() if key != "record_sha256"}
            if value.get("record_sha256") != record_hash(body):
                raise ValueError(
                    "The local-code receipt changed. It cannot be presented as the executed record."
                )
        return value

    def _scope(self, tender_id, root_run_id, actor_id=None, assignment_id=None):
        root = self.repo.get_run(root_run_id)
        if root["tender_id"] != tender_id:
            raise KeyError("Local code record not found in this work request.")
        if assignment_id:
            with self.repo.db.connect() as conn:
                row = conn.execute(
                    "SELECT root_run_id,staff_id FROM office_assignments WHERE tender_id=? AND id=?",
                    (tender_id, assignment_id),
                ).fetchone()
            if (
                not row
                or row["root_run_id"] != root_run_id
                or (actor_id and row["staff_id"] != actor_id)
            ):
                raise KeyError("Local code record not found in this assignment.")
        if actor_id and actor_id not in {"manager", "engineer"}:
            with self.repo.db.connect() as conn:
                if not conn.execute(
                    "SELECT 1 FROM sqlite_master WHERE type='table' AND name='office_staff'"
                ).fetchone():
                    raise KeyError("Local code record not found for this colleague.")
                row = conn.execute(
                    "SELECT 1 FROM office_staff WHERE tender_id=? AND id=?", (tender_id, actor_id)
                ).fetchone()
            if not row:
                raise KeyError("Local code record not found for this colleague.")
        return root

    def _matches(self, value, tender_id, root_run_id, actor_id, assignment_id):
        identity = value.get("execution_identity") or {}
        return (
            value.get("tender_id") == tender_id
            and (value.get("root_run_id") or value.get("run_id")) == root_run_id
            and (not actor_id or identity.get("actor_id") == actor_id)
            and (not assignment_id or identity.get("assignment_id") == assignment_id)
        )

    def _summary(self, value, engine, root):
        identity = value.get("execution_identity") or {}
        status = value.get("status", "interrupted")
        active = root["status"] in {"queued", "running"}
        phase = value.get("phase") or status
        if status == "running" and not active:
            status, phase = "interrupted", "stopped_or_cleanup_pending"
        return LocalCodeRun(
            id=value["id"],
            engine=engine,
            root_run_id=root["id"],
            actor_id=identity.get("actor_id"),
            assignment_id=identity.get("assignment_id"),
            legacy_attribution=not bool(identity),
            status=status,
            phase=phase,
            created_at=value.get("created_at", ""),
            duration_seconds=value.get("duration_seconds", 0),
            detail=value.get("detail", ""),
            root_active=active,
        )

    def page(
        self, tender_id, root_run_id, *, actor_id=None, assignment_id=None, cursor=None, limit=25
    ):
        root = self._scope(tender_id, root_run_id, actor_id, assignment_id)
        if not 1 <= limit <= 50:
            raise ValueError("Choose between 1 and 50 local code records.")
        scope = [tender_id, root_run_id, actor_id, assignment_id]
        after = ""
        if cursor:
            try:
                decoded = json.loads(base64.urlsafe_b64decode(cursor))
                if decoded["scope"] != scope:
                    raise ValueError()
                after = decoded["after"]
            except Exception as error:
                raise ValueError("This local code page belongs to different work.") from error
        paths = [
            ("monty/" + path.stem, "monty", path)
            for path in (self.repo.home / "code" / "receipts").glob("*.json")
        ]
        paths += [
            ("python/" + path.parent.name, "python", path)
            for path in (self.repo.home / "code" / "runs").glob("*/receipt.json")
        ]
        items, next_cursor, scanned = [], None, 0
        for key, engine, path in sorted(paths):
            if key <= after:
                continue
            scanned += 1
            value = self._read(path)
            if self._matches(value, *scope):
                items.append(self._summary(value, engine, root))
            if len(items) >= limit or scanned >= 200:
                next_cursor = base64.urlsafe_b64encode(
                    json.dumps({"scope": scope, "after": key}).encode()
                ).decode()
                break
        return LocalCodePage(items=items, next_cursor=next_cursor)

    def _record(
        self, tender_id, root_run_id, engine, identifier, actor_id=None, assignment_id=None
    ):
        root = self._scope(tender_id, root_run_id, actor_id, assignment_id)
        path = self._path(engine, identifier)
        try:
            value = self._read(path)
        except OSError as error:
            raise KeyError("Local code record not found.") from error
        if not self._matches(value, tender_id, root_run_id, actor_id, assignment_id):
            raise KeyError("Local code record not found in this scope.")
        if value.get("record_version"):
            body = {key: data for key, data in value.items() if key != "record_sha256"}
            if value.get("record_sha256") != record_hash(body):
                raise ValueError(
                    "The local-code receipt changed. It cannot be presented as the executed record."
                )
        return root, value, path

    def _file(self, folder, item, kind):
        safe_name(item["name"])
        path = (
            folder
            / ("inputs" if kind == "input" else "outputs" if kind == "output" else "")
            / item["name"]
        )
        self._safe_file(path, 100 * 1024 * 1024)
        with path.open("rb") as stream:
            actual = hashlib.file_digest(stream, "sha256").hexdigest()
        if actual != item["sha256"] or (
            "size_bytes" in item and path.stat().st_size != item["size_bytes"]
        ):
            raise ValueError(
                "A saved code input or output changed. Restore its original receipt-linked bytes."
            )
        return path

    def detail(
        self, tender_id, root_run_id, engine, identifier, *, actor_id=None, assignment_id=None
    ):
        root, value, path = self._record(
            tender_id, root_run_id, engine, identifier, actor_id, assignment_id
        )
        inputs = []
        if engine == "monty":
            code, data = value["code"], value["inputs"]
            exact = value.get("inputs_encoded_json")
            encoded = (
                exact.encode()
                if isinstance(exact, str)
                else json.dumps(data, ensure_ascii=False, allow_nan=False).encode()
            )
            if sha256(encoded) != value["inputs_sha256"] or json.loads(encoded) != data:
                raise ValueError("The saved composition inputs changed.")
            data = json.loads(encoded)
        else:
            code = self._safe_file(path.parent / "code.py", 131072).read_text(encoding="utf-8")
            data = None
            for item in value.get("inputs", []):
                target = self._file(path.parent, item, "input")
                inputs.append(LocalCodeFile(**{**item, "size_bytes": target.stat().st_size}))
        if sha256(code.encode()) != value["code_sha256"]:
            raise ValueError("The saved code changed and no longer matches its execution receipt.")
        for kind, entries in (("output", value.get("outputs", [])), ("log", value.get("logs", []))):
            for item in entries:
                self._file(path.parent, item, kind)
        return LocalCodeDetail(
            run=self._summary(value, engine, root),
            code=code,
            code_sha256=value["code_sha256"],
            record_integrity="verified" if value.get("record_version") else "legacy_hashes_only",
            limits=value.get("limits", {}),
            runtime={
                key: value.get(key)
                for key in (
                    "engine",
                    "engine_version",
                    "runtime_sha256",
                    "runtime_fingerprint",
                    "image_id",
                    "library_versions",
                )
            },
            inputs=inputs,
            input_values=data,
            inputs_sha256=value.get("inputs_sha256"),
            outputs=value.get("outputs", []),
            logs=value.get("logs", []),
            stdout=value.get("stdout", "")[:12000],
            stderr=value.get("stderr", "")[:12000],
            printed=value.get("printed", "")[:12000],
            output=value.get("output"),
            calls=value.get("calls", []),
        )

    def file(
        self,
        tender_id,
        root_run_id,
        engine,
        identifier,
        kind,
        name,
        *,
        actor_id=None,
        assignment_id=None,
    ):
        _, value, path = self._record(
            tender_id, root_run_id, engine, identifier, actor_id, assignment_id
        )
        key = {"input": "inputs", "output": "outputs", "log": "logs"}.get(kind)
        if not key or engine != "python":
            raise KeyError("Generated code file not found.")
        item = next((item for item in value.get(key, []) if item.get("name") == name), None)
        if not item:
            raise KeyError("Generated code file not found.")
        return self._file(path.parent, item, kind)
