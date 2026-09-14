"""Immutable submission requirements, output links and scoped engineer review."""

import hashlib
from datetime import UTC, date, datetime

from .db import dump, new_id, now, record
from .documents import MAX_FILE_BYTES
from .estimate_models import EngineerDecision
from .outputs import OutputService, fingerprint
from .requirement_models import (
    RequirementDecision,
    RequirementOutputLink,
    RequirementProposal,
)

SCHEMA = (
    "CREATE TABLE IF NOT EXISTS submission_requirements(id TEXT PRIMARY KEY,tender_id TEXT NOT NULL REFERENCES tenders(id),payload_json TEXT NOT NULL,created_at TEXT NOT NULL)",
    "CREATE INDEX IF NOT EXISTS submission_requirements_tender ON submission_requirements(tender_id,created_at)",
    "CREATE TABLE IF NOT EXISTS requirement_events(id TEXT PRIMARY KEY,requirement_id TEXT NOT NULL REFERENCES submission_requirements(id),tender_id TEXT NOT NULL REFERENCES tenders(id),action TEXT NOT NULL,payload_json TEXT NOT NULL,rationale TEXT NOT NULL,created_at TEXT NOT NULL)",
    "CREATE INDEX IF NOT EXISTS requirement_events_requirement ON requirement_events(requirement_id)",
    "CREATE TRIGGER IF NOT EXISTS submission_requirements_no_update BEFORE UPDATE ON submission_requirements BEGIN SELECT RAISE(ABORT,'Submission requirement content is immutable'); END",
    "CREATE TRIGGER IF NOT EXISTS submission_requirements_no_delete BEFORE DELETE ON submission_requirements BEGIN SELECT RAISE(ABORT,'Submission requirement history is immutable'); END",
    "CREATE TRIGGER IF NOT EXISTS requirement_events_no_update BEFORE UPDATE ON requirement_events BEGIN SELECT RAISE(ABORT,'Requirement decisions are immutable'); END",
    "CREATE TRIGGER IF NOT EXISTS requirement_events_no_delete BEFORE DELETE ON requirement_events BEGIN SELECT RAISE(ABORT,'Requirement decisions are immutable'); END",
)


def _evidence_fingerprint(evidence):
    return fingerprint(
        {
            key: evidence.get(key)
            for key in (
                "id",
                "artifact_id",
                "locator",
                "text",
                "page",
                "sheet",
                "cell_range",
                "kind",
                "metadata",
            )
        }
    )


class RequirementService:
    def __init__(self, repo):
        self.repo = repo
        self.outputs = OutputService(repo)
        with repo.db.connect(write=True) as conn:
            for statement in SCHEMA:
                conn.execute(statement)

    def _original_current(self, tender_id, artifact, cache):
        key = (artifact["id"], artifact["content_hash"])
        if key not in cache:
            try:
                path = self.repo.object_path(tender_id, artifact["id"])
                if path.stat().st_size > MAX_FILE_BYTES:
                    raise ValueError("The source exceeds the supported file size.")
                digest, size = hashlib.sha256(), 0
                with path.open("rb") as stream:
                    while chunk := stream.read(1024 * 1024):
                        size += len(chunk)
                        if size > MAX_FILE_BYTES:
                            raise ValueError("The source exceeds the supported file size.")
                        digest.update(chunk)
                cache[key] = (
                    True,
                    digest.hexdigest() == artifact["content_hash"] and size == artifact["size"],
                )
            except (OSError, ValueError):
                cache[key] = (False, False)
        return cache[key]

    def _capture_source(self, tender_id, source_id, cache):
        evidence = self.repo.get_evidence(tender_id, source_id)
        artifact = self.repo.get_artifact(tender_id, evidence["artifact_id"])
        available, original_current = self._original_current(tender_id, artifact, cache)
        if not artifact["is_current"] or not available or not original_current:
            raise ValueError("Propose requirements using current, preserved source files.")
        return {
            "source_id": source_id,
            "artifact_id": artifact["id"],
            "artifact_name": artifact["name"],
            "relative_path": artifact["relative_path"],
            "locator": evidence["locator"],
            "version": artifact["version"],
            "content_hash": artifact["content_hash"],
            "evidence_hash": _evidence_fingerprint(evidence),
            "kind": evidence["kind"],
        }

    def _check_source(self, tender_id, source, cache):
        reasons, available = [], False
        try:
            evidence = self.repo.get_evidence(tender_id, source["source_id"])
            artifact = self.repo.get_artifact(tender_id, source["artifact_id"])
            if (
                evidence["artifact_id"] != source["artifact_id"]
                or not artifact["is_current"]
                or artifact["version"] != source["version"]
                or artifact["content_hash"] != source["content_hash"]
            ):
                reasons.append("source_revision_changed")
            if _evidence_fingerprint(evidence) != source["evidence_hash"]:
                reasons.append("source_evidence_changed")
            available, original_current = self._original_current(tender_id, artifact, cache)
            if not available:
                reasons.append("source_unavailable")
            elif not original_current:
                reasons.append("source_bytes_changed")
        except (KeyError, ValueError):
            reasons.append("source_unavailable")
        return {
            **source,
            "available": available,
            "is_current": not reasons,
            "recheck_reasons": reasons,
        }

    def propose(self, tender_id, values, *, origin="engineer", run_id=None):
        request = RequirementProposal.model_validate(values)
        if origin not in {"engineer", "manager"}:
            raise ValueError("Requirement origin must be engineer or manager.")
        with self.repo.atomic() as conn:
            self.repo.get_tender(tender_id)
            if origin == "manager" and not run_id:
                raise ValueError("A Manager proposal must retain its owning run.")
            if run_id and self.repo.get_run(run_id)["tender_id"] != tender_id:
                raise KeyError("The proposal run is outside the selected Tender.")
            cache = {}
            sources = [
                self._capture_source(tender_id, source_id, cache)
                for source_id in request.source_ids
            ]
            from .requirement_qualifications import validate_qualification

            validate_qualification(self.repo, tender_id, request, manager=origin == "manager")
            identifier, stamp = new_id(), now()
            payload = request.model_dump(mode="json") | {
                "sources": sources,
                "origin": origin,
                "run_id": run_id,
            }
            conn.execute(
                "INSERT INTO submission_requirements VALUES(?,?,?,?)",
                (identifier, tender_id, dump(payload), stamp),
            )
            return self.get(tender_id, identifier)

    def create(self, tender_id, values):
        return self.propose(tender_id, values)

    def _event(self, conn, tender_id, requirement_id, action, rationale, payload=None):
        identifier, stamp = new_id(), now()
        conn.execute(
            "INSERT INTO requirement_events VALUES(?,?,?,?,?,?,?)",
            (
                identifier,
                requirement_id,
                tender_id,
                action,
                dump(payload or {}),
                rationale,
                stamp,
            ),
        )
        conn.execute(
            "INSERT INTO decisions VALUES(?,?,?,?,?,?,?)",
            (
                new_id(),
                tender_id,
                "submission_requirement",
                requirement_id,
                action,
                rationale,
                stamp,
            ),
        )

    def _output_state(self, tender_id, linked, cache):
        key = (linked["output_id"], linked["sha256"], linked["basis_fingerprint"])
        if key not in cache:
            reasons, available = [], False
            try:
                output = self.outputs.get(tender_id, linked["output_id"])
                available = True
                if (
                    output["sha256"] != linked["sha256"]
                    or output["kind"] != linked["kind"]
                    or output["metadata"].get("basis_fingerprint") != linked["basis_fingerprint"]
                ):
                    reasons.append("The linked document record has changed.")
                self.outputs.path(tender_id, linked["output_id"])
                current = self.outputs.check_current(tender_id, output)
                reasons.extend(current["blocking_reasons"])
            except (KeyError, ValueError, OSError) as exc:
                available = False
                reasons.append(str(exc))
            cache[key] = (available, list(dict.fromkeys(reasons)))
        available, reasons = cache[key]
        return {
            **linked,
            "available": available,
            "is_current": not reasons,
            "recheck_reasons": reasons,
        }

    @staticmethod
    def _review_basis(requirement):
        return fingerprint(
            {
                "id": requirement["id"],
                "sources": requirement["sources"],
                "deliverable_kind": requirement["deliverable_kind"],
                "due_date": requirement["due_date"],
                "linked_outputs": requirement["linked_outputs"],
                "source_quote": requirement.get("source_quote", ""),
                "applicability": requirement.get("applicability", "unknown"),
                "condition": requirement.get("condition", ""),
                "exceptions": requirement.get("exceptions", []),
            }
        )

    def get(self, tender_id, requirement_id, *, _source_cache=None, _output_cache=None):
        with self.repo.db.connect() as conn:
            row = record(
                conn.execute(
                    "SELECT * FROM submission_requirements WHERE id=? AND tender_id=?",
                    (requirement_id, tender_id),
                ).fetchone()
            )
            events = [
                record(event)
                for event in conn.execute(
                    "SELECT * FROM requirement_events WHERE requirement_id=? AND tender_id=? ORDER BY rowid",
                    (requirement_id, tender_id),
                )
            ]
            payload = row["payload"]
            source_cache = {} if _source_cache is None else _source_cache
            output_cache = {} if _output_cache is None else _output_cache
            sources = [
                self._check_source(tender_id, source, source_cache) for source in payload["sources"]
            ]
            status, links, reviewed = "proposed", {}, None
            for event in events:
                action = event["action"]
                if action == "approve":
                    status = "approved"
                elif action == "withdraw":
                    status, reviewed = "withdrawn", None
                elif action == "link_output":
                    links[event["payload"]["output_id"]] = event["payload"] | {
                        "linked_at": event["created_at"],
                        "link_rationale": event["rationale"],
                    }
                    reviewed = None
                elif action == "unlink_output":
                    links.pop(event["payload"]["output_id"], None)
                    reviewed = None
                elif action == "reopen":
                    reviewed = None
                elif action in {"satisfied", "exception"}:
                    reviewed = event
            linked_outputs = [
                self._output_state(tender_id, links[key], output_cache) for key in sorted(links)
            ]
            reasons = list(
                dict.fromkeys(reason for source in sources for reason in source["recheck_reasons"])
            )
            overdue = bool(
                payload["due_date"]
                and date.fromisoformat(payload["due_date"]) < datetime.now(UTC).date()
            )
            warnings = []
            qualification = {
                "source_quote": payload.get("source_quote", ""),
                "applicability": payload.get("applicability", "unknown"),
                "condition": payload.get("condition", ""),
                "exceptions": payload.get("exceptions", []),
            }
            applicability_reviewed = any(
                event["payload"].get("applicability_reviewed") is True
                for event in events
                if event["action"] in {"approve", "satisfied", "exception"}
            )
            if qualification["applicability"] == "unknown":
                warnings.append(
                    "Applicability was not recorded. Check the complete source clause, its conditions and exceptions before approving or releasing this requirement."
                )
            elif qualification["applicability"] == "conditional":
                warnings.append(
                    "This requirement applies only under its stated condition. The engineer must establish whether that condition applies."
                )
            if overdue:
                warnings.append(
                    "The recorded due date has passed. Check the current submission instructions."
                )
            result = {
                **payload,
                **qualification,
                "applicability_reviewed": applicability_reviewed,
                "id": requirement_id,
                "tender_id": tender_id,
                "created_at": row["created_at"],
                "sources": sources,
                "status": status,
                "is_current": not reasons,
                "recheck_reasons": reasons,
                "linked_outputs": linked_outputs,
                "review_status": reviewed["action"] if reviewed else "pending",
                "reviewed_at": reviewed["created_at"] if reviewed else None,
                "review_rationale": reviewed["rationale"] if reviewed else None,
                "overdue": overdue,
                "warnings": warnings,
                "audit": [
                    {
                        "id": event["id"],
                        "action": event["action"],
                        "rationale": event["rationale"],
                        "created_at": event["created_at"],
                        "output_id": event["payload"].get("output_id"),
                    }
                    for event in events
                ],
            }
            result["review_is_current"] = bool(
                reviewed
                and status == "approved"
                and not reasons
                and reviewed["payload"].get("basis_fingerprint") == self._review_basis(result)
            )
            return result

    def list(self, tender_id, *, include_withdrawn=False, offset=0, limit=50):
        self.repo.get_tender(tender_id)
        if (
            isinstance(offset, bool)
            or not isinstance(offset, int)
            or offset < 0
            or isinstance(limit, bool)
            or not isinstance(limit, int)
            or not 1 <= limit <= 100
        ):
            raise ValueError("Use a nonnegative offset and read 1 to 100 requirements.")
        with self.repo.db.connect() as conn:
            identifiers = [
                row[0]
                for row in conn.execute(
                    "SELECT r.id FROM submission_requirements r WHERE r.tender_id=? AND (? OR NOT EXISTS(SELECT 1 FROM requirement_events e WHERE e.requirement_id=r.id AND e.action='withdraw')) ORDER BY r.created_at DESC,r.id DESC LIMIT ? OFFSET ?",
                    (tender_id, include_withdrawn, limit, offset),
                )
            ]
            sources, outputs = {}, {}
            return [
                self.get(tender_id, identifier, _source_cache=sources, _output_cache=outputs)
                for identifier in identifiers
            ]

    def decide(self, tender_id, requirement_id, values):
        decision = RequirementDecision.model_validate(values)
        with self.repo.atomic() as conn:
            requirement = self.get(tender_id, requirement_id)
            if requirement["status"] == "withdrawn":
                raise ValueError(
                    "This requirement was withdrawn. Propose a new requirement if needed."
                )
            if decision.decision == "withdraw":
                self._event(conn, tender_id, requirement_id, "withdraw", decision.rationale)
                return self.get(tender_id, requirement_id)
            if not requirement["is_current"]:
                raise ValueError(
                    "The requirement source has changed or is unavailable. Propose it again from current sources."
                )
            needs_applicability = requirement["applicability"] != "unconditional" or bool(
                requirement["exceptions"]
            )
            if (
                decision.decision in {"approve", "satisfied", "exception"}
                and needs_applicability
                and not requirement["applicability_reviewed"]
                and not decision.applicability_reviewed
            ):
                raise ValueError(
                    "Review the requirement's applicability, conditions and exceptions before recording this decision."
                )
            if decision.decision == "approve":
                if requirement["status"] != "proposed":
                    raise ValueError("Only a proposed requirement can be approved.")
                self._event(
                    conn,
                    tender_id,
                    requirement_id,
                    "approve",
                    decision.rationale,
                    {"applicability_reviewed": decision.applicability_reviewed},
                )
            else:
                if requirement["status"] != "approved":
                    raise ValueError(
                        "Approve the requirement before recording its completion review."
                    )
                if decision.decision == "satisfied":
                    if not requirement["linked_outputs"]:
                        raise ValueError(
                            "Link the generated document that satisfies this requirement before marking it satisfied."
                        )
                    if any(not linked["is_current"] for linked in requirement["linked_outputs"]):
                        raise ValueError(
                            "A linked document is missing, changed or blocked. Resolve its basis before marking the requirement satisfied."
                        )
                self._event(
                    conn,
                    tender_id,
                    requirement_id,
                    decision.decision,
                    decision.rationale,
                    {
                        "basis_fingerprint": self._review_basis(requirement),
                        "applicability_reviewed": decision.applicability_reviewed,
                    },
                )
            return self.get(tender_id, requirement_id)

    def link_output(self, tender_id, requirement_id, values):
        decision = RequirementOutputLink.model_validate(values)
        with self.repo.atomic() as conn:
            requirement = self.get(tender_id, requirement_id)
            if requirement["status"] != "approved" or not requirement["is_current"]:
                raise ValueError("Link documents to a current, approved requirement.")
            if any(
                link["output_id"] == decision.output_id for link in requirement["linked_outputs"]
            ):
                raise ValueError("This document is already linked to the requirement.")
            output = self.outputs.get(tender_id, decision.output_id)
            if output["kind"] != requirement["deliverable_kind"]:
                raise ValueError(
                    "The document type does not match this requirement's deliverable type."
                )
            self.outputs.path(tender_id, decision.output_id)
            current = self.outputs.check_current(tender_id, output)
            if current["blocking_reasons"]:
                raise ValueError(
                    "The selected document needs attention: "
                    + " ".join(current["blocking_reasons"])
                )
            linked = {
                "output_id": decision.output_id,
                "filename": output["filename"],
                "kind": output["kind"],
                "sha256": output["sha256"],
                "basis_fingerprint": output["metadata"]["basis_fingerprint"],
            }
            self._event(conn, tender_id, requirement_id, "link_output", decision.rationale, linked)
            return self.get(tender_id, requirement_id)

    def unlink_output(self, tender_id, requirement_id, output_id, values):
        decision = EngineerDecision.model_validate(values)
        with self.repo.atomic() as conn:
            requirement = self.get(tender_id, requirement_id)
            if requirement["status"] != "approved":
                raise ValueError(
                    "Only an approved requirement can have its document links changed."
                )
            if not any(link["output_id"] == output_id for link in requirement["linked_outputs"]):
                raise KeyError("This document is not linked to the selected requirement.")
            self._event(
                conn,
                tender_id,
                requirement_id,
                "unlink_output",
                decision.rationale,
                {"output_id": output_id},
            )
            return self.get(tender_id, requirement_id)

    def submission_basis(self, tender_id, requirement_ids, output_ids):
        self.repo.get_tender(tender_id)
        if (
            not isinstance(requirement_ids, list)
            or len(requirement_ids) > 200
            or any(not isinstance(value, str) or not value for value in requirement_ids)
            or len(set(requirement_ids)) != len(requirement_ids)
        ):
            raise ValueError("Select each submission requirement once, up to 200 requirements.")
        if not isinstance(output_ids, list) or any(
            not isinstance(value, str) or not value for value in output_ids
        ):
            raise ValueError("Choose valid generated output references.")
        selected_outputs = set(output_ids)
        for output_id in selected_outputs:
            self.outputs.get(tender_id, output_id)
        requirements, blockers, warnings, repairs = [], [], [], []
        with self.repo.atomic():
            source_cache, output_cache = {}, {}
            for identifier in sorted(requirement_ids):
                requirement = self.get(
                    tender_id, identifier, _source_cache=source_cache, _output_cache=output_cache
                )
                requirements.append(requirement)
                title = requirement["title"] + ": "

                def block(code, message, output_ids=None):
                    blockers.append(title + message)
                    repairs.append(
                        {
                            "code": code,
                            "message": title + message,
                            "target": {
                                "kind": "package" if output_ids else "requirement",
                                "record_id": identifier,
                                "output_ids": output_ids or [],
                            },
                        }
                    )

                if requirement["status"] != "approved":
                    block("requirement_approval", "the requirement is not currently approved.")
                if (
                    requirement["applicability"] != "unconditional" or requirement["exceptions"]
                ) and not requirement["applicability_reviewed"]:
                    block(
                        "requirement_review", "review the applicability, conditions and exceptions."
                    )
                if not requirement["is_current"]:
                    block("requirement_source", "the source has changed or is unavailable.")
                if not requirement["review_is_current"]:
                    block("requirement_review", "a current engineer completion review is required.")
                elif requirement["review_status"] == "exception":
                    warnings.append(
                        title + "engineer-reviewed exception: " + requirement["review_rationale"]
                    )
                elif requirement["review_status"] == "satisfied":
                    linked = requirement["linked_outputs"]
                    if not linked or any(not output["is_current"] for output in linked):
                        block(
                            "requirement_document",
                            "a linked document is missing, changed or blocked.",
                        )
                    missing = [
                        output["filename"]
                        for output in linked
                        if output["output_id"] not in selected_outputs
                    ]
                    if missing:
                        block(
                            "missing_output",
                            "include the reviewed linked documents: " + ", ".join(missing),
                            [
                                output["output_id"]
                                for output in linked
                                if output["output_id"] not in selected_outputs
                            ],
                        )
                warnings.extend(title + warning for warning in requirement["warnings"])
            result = {
                "requirements": requirements,
                "blocking_reasons": sorted(set(blockers)),
                "blockers": repairs,
                "warnings": sorted(set(warnings)),
            }
            return result | {"fingerprint": fingerprint(result)}
