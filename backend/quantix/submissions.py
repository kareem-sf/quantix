"""Freeze existing reviewed drafts into a scoped local export, without transmission."""

import hashlib
import json
import re
from zipfile import ZIP_DEFLATED, ZipFile

from .db import dump, new_id, now, record
from .outputs import OutputService, fingerprint
from .submission_models import SubmissionApproval, SubmissionSelection
from .tender_requirements import RequirementService


class SubmissionService:
    def __init__(self, repo):
        self.repo = repo
        self.outputs = OutputService(repo)
        self.requirements = RequirementService(repo)
        # Frozen files and their manifests live alongside draft exports.
        self.directory = self.outputs.directory
        with repo.db.connect(write=True) as conn:
            conn.execute(
                "CREATE TABLE IF NOT EXISTS submissions(id TEXT PRIMARY KEY,tender_id TEXT NOT NULL REFERENCES tenders(id),record_json TEXT NOT NULL)"
            )
            conn.execute(
                "CREATE TRIGGER IF NOT EXISTS submissions_immutable_update BEFORE UPDATE ON submissions BEGIN SELECT RAISE(ABORT,'Approved export records are immutable'); END"
            )
            conn.execute(
                "CREATE TRIGGER IF NOT EXISTS submissions_immutable_delete BEFORE DELETE ON submissions BEGIN SELECT RAISE(ABORT,'Approved export records are immutable'); END"
            )

    def list(self, tender_id):
        self.repo.get_tender(tender_id)
        with self.repo.db.connect() as conn:
            return [
                record(row)["record"]
                for row in conn.execute(
                    "SELECT record_json FROM submissions WHERE tender_id=? ORDER BY rowid DESC",
                    (tender_id,),
                )
            ]

    def get(self, tender_id, submission_id):
        with self.repo.db.connect() as conn:
            return record(
                conn.execute(
                    "SELECT record_json FROM submissions WHERE tender_id=? AND id=?",
                    (tender_id, submission_id),
                ).fetchone()
            )["record"]

    def preview(self, tender_id, values):
        selection = SubmissionSelection.model_validate(values)
        with self.repo.atomic():
            self.repo.get_tender(tender_id)
            outputs, blockers, repairs = [], [], []
            warnings = [
                "Approval covers only the selected files and the engineer stated scope. Full Tender compliance, complete source coverage and external transmission are not established by this export."
            ]
            active_requirements, offset = [], 0
            while rows := self.requirements.list(tender_id, offset=offset, limit=100):
                active_requirements.extend(rows)
                offset += len(rows)
            requirement_ids = selection.requirement_ids
            if requirement_ids is None:
                requirement_ids = [row["id"] for row in active_requirements]
            requirement_basis = self.requirements.submission_basis(
                tender_id, requirement_ids, selection.output_ids
            )
            blockers.extend(requirement_basis["blocking_reasons"])
            repairs.extend(requirement_basis["blockers"])
            warnings.extend(requirement_basis["warnings"])
            omitted = [
                row["title"] for row in active_requirements if row["id"] not in requirement_ids
            ]
            if omitted:
                warnings.append("Requirements outside this scoped export: " + "; ".join(omitted))
            if not active_requirements:
                warnings.append(
                    "No submission requirements are registered. Required Tender contents have not been established."
                )
            for identifier in sorted(selection.output_ids):
                output = self.outputs.get(tender_id, identifier)
                outputs.append(output)
                try:
                    self.outputs.path(tender_id, identifier)
                    captured = self.outputs.check_current(tender_id, output)
                    output_reasons = [
                        output["filename"] + ": " + reason
                        for reason in captured["blocking_reasons"]
                    ]
                except (ValueError, KeyError) as exc:
                    output_reasons = [output["filename"] + ": " + str(exc)]
                blockers.extend(output_reasons)
                repairs.extend(
                    {
                        "code": "output_changed",
                        "message": reason,
                        "target": {"kind": "output", "record_id": identifier, "output_ids": []},
                    }
                    for reason in output_reasons
                )
                warnings.extend(
                    output["filename"] + ": " + warning
                    for warning in output["metadata"].get("warnings", [])
                )
            result = {
                "outputs": outputs,
                "blocking_reasons": sorted(set(blockers)),
                "blockers": repairs,
                "warnings": sorted(set(warnings)),
                "requirements": requirement_basis["requirements"],
                "requirement_ids": sorted(requirement_ids),
            }
            return result | {"fingerprint": fingerprint(result)}

    def approve(self, tender_id, values):
        decision = SubmissionApproval.model_validate(values)
        identifier = new_id()
        path = self.directory / ("submission-" + identifier + ".zip")
        try:
            with self.repo.atomic():
                selection = {
                    "output_ids": decision.output_ids,
                    "requirement_ids": decision.requirement_ids,
                }
                preview = self.preview(tender_id, selection)
                if decision.fingerprint != preview["fingerprint"]:
                    raise ValueError(
                        "The selected files or their current basis changed. Review a fresh submission preview."
                    )
                if preview["blocking_reasons"]:
                    raise ValueError(
                        "Final export is blocked: " + " ".join(preview["blocking_reasons"])
                    )
                if sorted(set(decision.acknowledged_gaps)) != preview["warnings"]:
                    raise ValueError(
                        "Acknowledge all current scope limits and gaps shown in the submission preview."
                    )
                manifest = {
                    "id": identifier,
                    "tender_id": tender_id,
                    "status": "approved_export",
                    "filename": path.name,
                    "created_at": now(),
                    "fingerprint": preview["fingerprint"],
                    "output_ids": sorted(decision.output_ids),
                    "outputs": preview["outputs"],
                    "requirements": preview["requirements"],
                    "requirement_ids": preview["requirement_ids"],
                    "acknowledged_scope": decision.acknowledged_scope,
                    "acknowledged_gaps": preview["warnings"],
                    "rationale": decision.rationale,
                    "final_review_confirmed": True,
                    "external_transmission": False,
                }
                with ZipFile(path, "x", compression=ZIP_DEFLATED) as archive:
                    archive.writestr("submission-manifest.json", dump(manifest))
                    for output in preview["outputs"]:
                        payload = self.outputs.path(tender_id, output["id"]).read_bytes()
                        if hashlib.sha256(payload).hexdigest() != output["sha256"]:
                            raise ValueError("A selected file changed during final export.")
                        archive.writestr(output["filename"], payload)
                        archive.writestr(output["filename"] + ".manifest.json", dump(output))
                with ZipFile(path) as archive:
                    if archive.testzip() is not None:
                        raise ValueError("The frozen package failed its integrity check.")
                if self.preview(tender_id, selection)["fingerprint"] != preview["fingerprint"]:
                    raise ValueError("The source or output files changed during final export.")
                result = manifest | {
                    "size": path.stat().st_size,
                    "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
                }
                path.with_suffix(".json").write_text(dump(result), encoding="utf-8")
                with self.repo.db.connect(write=True) as conn:
                    conn.execute(
                        "INSERT INTO submissions VALUES(?,?,?)",
                        (identifier, tender_id, dump(result)),
                    )
                    conn.execute(
                        "INSERT INTO decisions VALUES(?,?,?,?,?,?,?)",
                        (
                            new_id(),
                            tender_id,
                            "submission",
                            identifier,
                            "approve_export",
                            decision.rationale + "\nApproved scope: " + decision.acknowledged_scope,
                            now(),
                        ),
                    )
                return result
        except BaseException:
            path.unlink(missing_ok=True)
            path.with_suffix(".json").unlink(missing_ok=True)
            raise

    def path(self, tender_id, submission_id):
        saved = self.get(tender_id, submission_id)
        if not re.fullmatch(r"submission-[a-f0-9]{32}\.zip", saved["filename"]):
            raise ValueError("The submission record contains an invalid filename.")
        path = self.directory / saved["filename"]
        if not path.is_file() or hashlib.sha256(path.read_bytes()).hexdigest() != saved["sha256"]:
            raise ValueError(
                "The frozen submission is missing or changed; its integrity could not be verified."
            )
        try:
            manifest = json.loads(path.with_suffix(".json").read_text(encoding="utf-8"))
        except (OSError, ValueError) as exc:
            raise ValueError("The submission manifest is missing or changed.") from exc
        if manifest != saved:
            raise ValueError(
                "The submission manifest changed; its integrity could not be verified."
            )
        return path
