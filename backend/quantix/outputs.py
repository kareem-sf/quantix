"""Draft engineering exports with checked source and working-record snapshots."""

import hashlib
import json
import re
from pathlib import Path
from zipfile import ZipFile

from .client_boq import prepare_client_boq, write_client_boq
from .db import dump, new_id, now, record
from .estimate_models import DraftDocumentProposal, OutputRequest
from .estimates import EstimateService
from .output_programme import schedule_programme
from .output_word import _technical_word, _word
from .output_workbooks import _excel, _extra_excel


def fingerprint(value):
    return hashlib.sha256(
        json.dumps(
            value, sort_keys=True, ensure_ascii=False, allow_nan=False, separators=(",", ":")
        ).encode()
    ).hexdigest()


class OutputService:
    def __init__(self, repo):
        self.repo = repo
        self.estimates = EstimateService(repo)
        self.directory = repo.home / "outputs"
        self.directory.mkdir(exist_ok=True)
        with repo.db.connect(write=True) as conn:
            conn.execute(
                "CREATE TABLE IF NOT EXISTS generated_outputs(id TEXT PRIMARY KEY,tender_id TEXT NOT NULL REFERENCES tenders(id),record_json TEXT NOT NULL)"
            )

    def list(self, tender_id):
        self.repo.get_tender(tender_id)
        with self.repo.db.connect() as conn:
            return [
                record(row)["record"]
                for row in conn.execute(
                    "SELECT record_json FROM generated_outputs WHERE tender_id=? ORDER BY rowid DESC",
                    (tender_id,),
                )
            ]

    def generate(self, tender_id, values):
        request = OutputRequest.model_validate(values)
        return self._generate(tender_id, request, origin="engineer")

    def generate_from_office(self, tender_id, proposal, run_id, plan_id):
        """Generate a draft within saved plan approval, without an engineer mutation request."""
        request = DraftDocumentProposal.model_validate(proposal)
        with self.repo.atomic():
            scope, assigned = self._office_scope(tender_id, run_id, plan_id)
            if request.kind == "technical_docx" and request.task_id is None:
                if assigned is None:
                    raise ValueError("Select a completed specialist task for the technical draft.")
                request = request.model_copy(update={"task_id": assigned["id"]})
            self._office_technical_scope(tender_id, request, plan_id)
            return self._generate(
                tender_id,
                request,
                origin="agent",
                approval_scope=scope,
                run_id=run_id,
            )

    def _office_scope(self, tender_id, run_id, plan_id):
        scope = self.repo.approved_scope(tender_id, plan_id)
        run = self.repo.get_run(run_id)
        if run["tender_id"] != tender_id:
            raise ValueError("The draft's originating run belongs to another Tender.")
        if run["kind"] not in {"task", "manager", "research"} or run["status"] not in {
            "running", "completed"
        }:
            raise ValueError("Routine draft generation requires an active or completed Tender Office run.")
        assigned = None
        if run["kind"] == "task":
            assigned = next(
                (task for task in self.repo.list_tasks(tender_id) if task["run_id"] == run_id),
                None,
            )
            if assigned is None or assigned["plan_id"] != plan_id:
                raise ValueError("The originating task run is not part of this approved work plan.")
            if assigned["status"] not in {"running", "completed"}:
                raise ValueError("The originating specialist task has not reached a state for draft generation.")
        return scope, assigned

    def _office_technical_scope(self, tender_id, request, plan_id):
        if request.kind != "technical_docx":
            return
        task = self.repo.get_task(tender_id, request.task_id)
        if task["plan_id"] != plan_id or task["status"] != "completed":
            raise ValueError("A routine technical draft requires a completed task from this approved plan.")

    def _generate(self, tender_id, request, *, origin, approval_scope=None, run_id=None):
        with self.repo.atomic():
            captured = self.capture(tender_id, request)
        tender, data, sources = captured["tender"], captured["data"], captured["sources"]
        identifier = new_id()
        suffix = ".xlsx" if request.kind.endswith("xlsx") else ".docx"
        if request.kind == "client_boq":
            suffix = data["client_boq"]["suffix"]
        prefix = {
            "boq_xlsx": "consolidated-boq",
            "analysis_docx": "tender-analysis",
            "technical_docx": "technical-work",
            "registers_xlsx": "tender-registers",
            "comparison_xlsx": "supplier-comparison",
            "programme_xlsx": "construction-programme",
            "client_boq": "client-boq",
        }[request.kind]
        filename = prefix + "-" + identifier + suffix
        path = self.directory / filename
        result = {
            "id": identifier,
            "tender_id": tender_id,
            "kind": request.kind,
            "filename": filename,
            "status": "draft",
            "created_at": now(),
            "source_ids": [source["id"] for source in sources],
            "pricing_complete": data["client_boq"]["pricing_complete"]
            if request.kind == "client_boq"
            else data.get("view", {}).get("complete", False),
            "blocking_reasons": captured["blocking_reasons"],
            "metadata": {
                "origin": origin,
                "run_id": run_id,
                "approval_scope": approval_scope,
                "tender_revision": tender["revision"],
                "source_workbooks_modified": False,
                "client_format": request.kind == "client_boq",
                "formula_recalculation": "Excel recalculates formulas when opened; Summary contains server-calculated Decimal snapshots."
                if request.kind == "boq_xlsx"
                else "Values are a frozen snapshot of the saved records and explicit inputs.",
                "request": request.model_dump(mode="json"),
                "basis_fingerprint": captured["basis_fingerprint"],
                "source_manifest": [
                    {
                        key: source[key]
                        for key in (
                            "id",
                            "artifact_id",
                            "relative_path",
                            "locator",
                            "version",
                            "content_hash",
                            "is_current",
                        )
                    }
                    for source in sources
                ],
                "warnings": captured["warnings"],
                **({"task_id": request.task_id} if request.task_id else {}),
                **(
                    {"scheduled_activities": data["scheduled_activities"]}
                    if request.programme
                    else {}
                ),
                **(
                    {
                        "client_boq": data["client_boq"],
                        "pricing_scope": "Only the explicitly selected confirmed rows",
                        "formula_recalculation": "Mapped values use approved Decimal prices. Supplied amount formulas are preserved where they match the mapped quantity and rate. Other cached totals remain unchanged; Excel is asked to recalculate on opening.",
                    }
                    if request.kind == "client_boq"
                    else {}
                ),
            },
        }
        try:
            if request.kind == "boq_xlsx":
                _excel(path, tender, data["view"], sources)
            elif request.kind == "client_boq":
                write_client_boq(path, self.repo, tender_id, data["client_boq"])
            elif request.kind == "analysis_docx":
                _word(
                    path,
                    tender,
                    data["overview"],
                    data["view"],
                    data["findings"],
                    data["messages"],
                    sources,
                )
            elif request.kind == "technical_docx":
                _technical_word(path, tender, data["task"], sources, captured["warnings"])
            else:
                _extra_excel(path, tender, request, data, sources, captured["warnings"])
            with ZipFile(path) as archive:
                if archive.testzip() is not None:
                    raise ValueError("The generated Office file failed its integrity check.")
            result.update(
                size=path.stat().st_size, sha256=hashlib.sha256(path.read_bytes()).hexdigest()
            )
            with self.repo.atomic():
                if origin == "agent":
                    current_scope, _ = self._office_scope(
                        tender_id, run_id, approval_scope["plan_id"]
                    )
                    if current_scope != approval_scope:
                        raise ValueError("The approved work scope changed while preparing the draft.")
                    self._office_technical_scope(
                        tender_id, request, approval_scope["plan_id"]
                    )
                if (
                    self.capture(tender_id, request)["basis_fingerprint"]
                    != captured["basis_fingerprint"]
                ):
                    raise ValueError(
                        "The source or working records changed while generating the draft. Generate it again."
                    )
                path.with_suffix(".json").write_text(dump(result), encoding="utf-8")
                with self.repo.db.connect(write=True) as conn:
                    conn.execute(
                        "INSERT INTO generated_outputs VALUES(?,?,?)",
                        (identifier, tender_id, dump(result)),
                    )
                    if origin == "engineer":
                        conn.execute(
                            "INSERT INTO decisions VALUES(?,?,?,?,?,?,?)",
                            (
                                new_id(),
                                tender_id,
                                "output",
                                identifier,
                                "generate_draft",
                                request.rationale,
                                now(),
                            ),
                        )
                    else:
                        self.repo.event(
                            run_id,
                            "draft_generated",
                            "A draft document was saved within the approved work plan.",
                            {
                                "output_id": identifier,
                                "kind": request.kind,
                                "plan_id": approval_scope["plan_id"],
                            },
                        )
        except BaseException:
            path.unlink(missing_ok=True)
            path.with_suffix(".json").unlink(missing_ok=True)
            raise
        return result

    def capture(self, tender_id, request):
        tender = self.repo.get_tender(tender_id)
        data, warnings, blockers = {}, [], []
        if request.kind in {"boq_xlsx", "analysis_docx", "comparison_xlsx", "client_boq"}:
            data["view"] = self.estimates.view(tender_id)
        if request.kind == "client_boq":
            data["client_boq"] = prepare_client_boq(
                self.repo, tender_id, request.client_boq, data["view"]
            )
            data.pop("view")
            warnings.extend(data["client_boq"]["warnings"])
        if request.kind in {"analysis_docx", "registers_xlsx"}:
            data["findings"] = self.repo.list_findings(tender_id)
            for finding in data["findings"]:
                if finding["kind"] in {"assumption", "exclusion"} and finding["state"] == "proposed":
                    blockers.append("Unapproved " + finding["kind"] + ": " + finding["title"])
                if finding["state"] == "proposed":
                    warnings.append("Awaiting engineer decision: " + finding["title"])
        if request.kind == "analysis_docx":
            data.update(
                messages=self.repo.messages(tender_id), overview=self.repo.overview(tender_id)
            )
        if request.kind in {"boq_xlsx", "analysis_docx"}:
            blockers.extend(data["view"]["blocking_reasons"])
            warnings.append(data["view"]["coverage_note"])
            warnings.append(
                "Consolidated output. Client-format pricing and complete Tender coverage have not been established."
            )
        if request.kind == "technical_docx":
            task = self.repo.get_task(tender_id, request.task_id)
            if task["status"] != "completed" or not task["result"].get("summary", "").strip():
                raise ValueError("Select a completed specialist task with a saved result.")
            decisions = {finding["id"]: finding for finding in self.repo.list_findings(tender_id)}
            task["result"]["findings"] = [
                finding
                | {
                    "saved_state": finding.get("state", "proposed"),
                    "state": decisions.get(finding.get("id"), finding).get("state", "proposed"),
                }
                for finding in task["result"].get("findings", [])
            ]
            data["task"] = task
            warnings.append(
                "The document covers the selected specialist task. The engineer must check technical adequacy, missing information and applicable Tender requirements."
            )
            result = task["result"]
            for finding in result.get("findings", []):
                if finding.get("state", "proposed") == "proposed":
                    warnings.append("Unresolved " + finding["kind"] + ": " + finding["title"])
                    if finding["kind"] in {"assumption", "exclusion"}:
                        blockers.append(
                            "Unapproved " + finding["kind"] + " in saved specialist result: " + finding["title"]
                        )
            if result.get("price_proposals") or result.get("unit_rate_proposals"):
                blockers.append(
                    "The saved specialist result includes commercial proposals. Confirm the commercial decisions and record a revised specialist result before final export."
                )
        if request.kind == "comparison_xlsx":
            from .correspondence import QuoteService

            mail = QuoteService(self.repo)
            data["quotes"] = mail.list_drafts(tender_id)
            data["replies"] = [
                reply for quote in data["quotes"] for reply in mail.replies(tender_id, quote["id"])
            ]
            data["rates"] = self.estimates.list_rate_proposals(tender_id)
            warnings.append(
                "Supplier replies are quoted as received. Receipt is not commercial acceptance. Rates remain grouped by item, unit, currency, tax basis and conditions; no lowest compliant offer is inferred."
            )
            if not data["replies"]:
                warnings.append("No supplier replies are saved.")
            if not data["rates"]:
                warnings.append(
                    "No structured rate proposals are saved; reply prices have not been normalised."
                )
            if any(
                rate["status"] != "approved" or not rate["is_current"] for rate in data["rates"]
            ):
                blockers.append(
                    "The comparison includes unapproved or stale commercial rate proposals."
                )
        if request.kind == "programme_xlsx":
            data["programme"] = request.programme.model_dump(mode="json")
            data["scheduled_activities"] = schedule_programme(request.programme)
            warnings.append(
                "Construction dates and durations use the explicitly supplied calendar and activities. The engineer must review these inputs. No resource levelling, productivity validation or contractual completion assessment is included."
            )
            warnings.extend(
                "Programme assumption: " + value for value in request.programme.assumptions
            )
            for activity in request.programme.activities:
                warnings.extend(
                    activity.id + " assumption: " + value for value in activity.assumptions
                )
        source_ids = set()

        def collect(value):
            if isinstance(value, dict):
                for key, content in value.items():
                    if key in {
                        "source_ids",
                        "source_ids_read",
                        "supporting_source_ids",
                    } and isinstance(content, list):
                        source_ids.update(content)
                    elif key == "source_id" and isinstance(content, str):
                        source_ids.add(content)
                    else:
                        collect(content)
            elif isinstance(value, list):
                for content in value:
                    collect(content)

        collect(data)
        sources = []
        for identifier in sorted(source_ids):
            source = self.repo.get_evidence(tender_id, identifier)
            artifact = self.repo.get_artifact(tender_id, source["artifact_id"])
            sources.append(
                source | {key: artifact[key] for key in ("content_hash", "version", "is_current")}
            )
            if not artifact["is_current"]:
                blockers.append("A source changed: " + source["relative_path"])
        if not sources:
            warnings.append("No Tender source references are recorded for this output.")
        artifacts = [
            {key: artifact[key] for key in ("id", "content_hash", "version", "status")}
            for artifact in self.repo.list_artifacts(tender_id)
        ]
        return {
            "tender": tender,
            "data": data,
            "sources": sources,
            "warnings": sorted(set(warnings)),
            "blocking_reasons": sorted(set(blockers)),
            "basis_fingerprint": fingerprint(
                {
                    "revision": tender["revision"],
                    "data": data,
                    "sources": sources,
                    "artifacts": artifacts,
                }
            ),
        }

    def get(self, tender_id, output_id):
        with self.repo.db.connect() as conn:
            return record(
                conn.execute(
                    "SELECT record_json FROM generated_outputs WHERE id=? AND tender_id=?",
                    (output_id, tender_id),
                ).fetchone()
            )["record"]

    def path(self, tender_id, output_id) -> Path:
        output = self.get(tender_id, output_id)
        name = output["filename"]
        if not re.fullmatch(
            r"(?:(?:consolidated-boq|tender-analysis|technical-work|tender-registers|supplier-comparison|construction-programme)-[0-9a-f]{32}\.(?:xlsx|docx)|client-boq-[0-9a-f]{32}\.(?:xlsx|xlsm))",
            name,
        ):
            raise ValueError("The output record contains an invalid filename.")
        path = self.directory / name
        if not path.is_file() or hashlib.sha256(path.read_bytes()).hexdigest() != output["sha256"]:
            raise ValueError(
                "The generated file is missing or changed; its integrity could not be verified."
            )
        try:
            manifest = json.loads(path.with_suffix(".json").read_text(encoding="utf-8"))
        except (OSError, ValueError) as exc:
            raise ValueError("The output manifest is missing or changed.") from exc
        if manifest != output:
            raise ValueError("The output manifest changed; its integrity could not be verified.")
        return path

    def check_current(self, tender_id, output):
        metadata = output["metadata"]
        if not metadata.get("request") or not metadata.get("basis_fingerprint"):
            raise ValueError(
                "This older draft has no current source manifest. Generate a new draft before final export."
            )
        request_model = DraftDocumentProposal if metadata.get("origin") == "agent" else OutputRequest
        captured = self.capture(tender_id, request_model.model_validate(metadata["request"]))
        if captured["basis_fingerprint"] != metadata["basis_fingerprint"]:
            raise ValueError(
                "The source or working records changed. This draft is stale; generate and review a new output."
            )
        for source in captured["sources"]:
            if not source["is_current"]:
                raise ValueError("The draft uses a source that is no longer current.")
            path = self.repo.object_path(tender_id, source["artifact_id"])
            if hashlib.sha256(path.read_bytes()).hexdigest() != source["content_hash"]:
                raise ValueError(
                    "The preserved source file changed; its integrity could not be verified."
                )
        return captured
