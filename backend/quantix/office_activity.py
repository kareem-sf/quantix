"""Mirror committed office receipts into the owning run, without new authority."""

import json
from types import SimpleNamespace

from .run_activity import ActivityRecorder, current_operation

_LABELS = {
    "assignment_queued": "Specialist assignment queued.",
    "assignment_started": "Specialist work started.",
    "assignment_waiting": "Specialist work is waiting for a response.",
    "assignment_completed": "Specialist draft saved.",
    "assignment_failed": "Specialist work failed.",
    "assignment_cancelled": "Specialist work cancelled.",
    "assignment_interrupted": "Specialist work interrupted.",
    "staff_created": "Specialist profile created.",
    "profile_updated": "Specialist profile updated.",
    "message_posted": "Office message saved.",
    "artifact_shared": "Work shared with a colleague.",
    "source_inspected": "Source inspection recorded.",
    "scope_changed": "Reviewed work scope changed.",
}


def mirror_office_event(
    repo, conn, tender_id, event_type, event_id, actor_id, assignment_id, record_ref, payload
):
    tables = {
        row[0]
        for row in conn.execute(
            "SELECT name FROM sqlite_master WHERE type='table' AND name IN ('office_assignments','office_staff_versions')"
        )
    }
    assignment = None
    if assignment_id and "office_assignments" in tables:
        assignment = conn.execute(
            "SELECT * FROM office_assignments WHERE id=? AND tender_id=?",
            (assignment_id, tender_id),
        ).fetchone()
    parent = current_operation()
    owning = (
        conn.execute(
            "SELECT o.run_id FROM run_activity_operations o JOIN runs r ON r.id=o.run_id WHERE o.id=? AND r.tender_id=?",
            (parent, tender_id),
        ).fetchone()
        if parent
        else None
    )
    run_id = assignment["root_run_id"] if assignment else owning[0] if owning else None
    if not run_id:
        return  # An engineer action outside a run is not invented run activity.
    profile = None
    if actor_id and "office_staff_versions" in tables:
        version = assignment["staff_version"] if assignment else None
        row = conn.execute(
            "SELECT profile_json FROM office_staff_versions WHERE staff_id=? AND tender_id=? AND (? IS NULL OR version=?) ORDER BY version DESC LIMIT 1",
            (actor_id, tender_id, version, version),
        ).fetchone()
        if row:
            profile = SimpleNamespace(name=json.loads(row[0]).get("name", "Specialist"))
    context = SimpleNamespace(
        repo=repo,
        tender_id=tender_id,
        run_id=run_id,
        actor_id=actor_id,
        assignment_id=assignment_id,
        staff_profile=profile,
    )
    recorder = ActivityRecorder(context)
    category = (
        "assignment"
        if event_type.startswith("assignment_")
        else "source"
        if event_type == "source_inspected"
        else "message"
        if event_type in {"message_posted", "artifact_shared"}
        else "office"
    )
    phase = event_type.removeprefix("assignment_") if category == "assignment" else "observed"
    values = {"office_event_id": event_id, "record_ref": record_ref, "receipt": payload}
    previous = (
        conn.execute(
            "SELECT id FROM run_activity_operations WHERE run_id=? AND json_extract(metadata_json,'$.assignment_id')=? AND json_extract(metadata_json,'$.category')='assignment' ORDER BY created_at LIMIT 1",
            (run_id, assignment_id),
        ).fetchone()
        if category == "assignment"
        else None
    )
    if previous:
        recorder.record(previous[0], category, phase, _LABELS[event_type], values)
    else:
        recorder.start(category, _LABELS[event_type], values, phase=phase)
