"""Freshness checks for saved progress; never invent completed steps."""

import json

from .ai_tools import ToolArgumentError

_MATERIAL_FIELDS = (
    "findings",
    "plan",
    "submission_requirements",
    "boq_item_proposals",
    "quantity_proposals",
    "unit_rate_proposals",
    "quote_drafts",
    "project_map_nodes",
    "takeoff",
    "programme_proposal",
    "draft_documents",
    "requested_drafts",
)


def material_output(value):
    if hasattr(value, "model_dump"):
        value = value.model_dump(mode="json")
    return any(value.get(field) for field in _MATERIAL_FIELDS)


def later_work_run(repo, brief):
    with repo.db.connect() as conn:
        anchor = conn.execute(
            "SELECT rowid FROM runs WHERE id=? AND tender_id=?",
            (brief["run_id"], brief["tender_id"]),
        ).fetchone()
        rows = conn.execute(
            """SELECT id,result_json FROM runs
            WHERE tender_id=? AND status='completed' AND kind IN ('manager','conversation')
              AND id<>? AND ((? IS NOT NULL AND rowid>?) OR (? IS NULL AND updated_at>?))
            ORDER BY rowid DESC""",
            (
                brief["tender_id"],
                brief["run_id"],
                anchor[0] if anchor else None,
                anchor[0] if anchor else None,
                anchor[0] if anchor else None,
                brief["created_at"],
            ),
        )
        for row in rows:
            result = json.loads(row["result_json"])
            if (
                material_output(result)
                or conn.execute(
                    """SELECT 1 FROM run_events WHERE run_id=?
                AND kind IN ('saved_work_product','calculation_completed') LIMIT 1""",
                    (row["id"],),
                ).fetchone()
            ):
                return row["id"]
    return None


def require_current_brief(output, context):
    if context.is_staff:
        return
    with context.repo.db.connect() as conn:
        saved_work = conn.execute(
            "SELECT 1 FROM run_events WHERE run_id=? AND kind IN ('saved_work_product','calculation_completed') LIMIT 1",
            (context.run_id,),
        ).fetchone()
    if not material_output(output) and not saved_work:
        return
    from .work_brief import WorkBriefService

    brief = WorkBriefService(context.repo).current(context.tender_id)
    if brief and brief.status != "complete" and brief.run_id != context.run_id:
        raise ToolArgumentError(
            "Update the working brief with save_work_brief before finishing this work: record the "
            "completed step, remaining questions and next action. If the request replaced the outcome, "
            "save the new outcome. Do not leave the earlier progress note describing this result."
        )
