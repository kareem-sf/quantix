"""Select the saved Tender Manager analysis used by report drafts."""

from .db import record

MISSING_ANALYSIS_NOTE = (
    "No completed Tender Manager engineering analysis with recorded run provenance is available."
)
MISSING_ANALYSIS_REVIEW_LIMITATION = (
    "This draft reports saved Tender records, but it does not contain a completed source-based "
    "Tender Manager analysis. Ask the Tender Manager to review the Tender documents before "
    "relying on this draft."
)


def select_report_analysis(repo, tender_id):
    """Return the latest Manager message published by a completed engineering run."""

    repo.get_tender(tender_id)
    with repo.db.connect() as conn:
        row = conn.execute(
            """
            SELECT m.*, r.kind AS analysis_run_kind, r.status AS analysis_run_status
            FROM messages AS m
            JOIN runs AS r ON r.id=m.run_id AND r.tender_id=m.tender_id
            WHERE m.tender_id=?
              AND m.role='manager'
              AND r.kind IN ('manager','conversation')
              AND r.status='completed'
              AND json_type(r.result_json, '$.summary')='text'
              AND trim(json_extract(r.result_json, '$.summary'))<>''
            ORDER BY m.rowid DESC
            LIMIT 1
            """,
            (tender_id,),
        ).fetchone()
    if row is None:
        return {
            "status": "unavailable",
            "message": None,
            "run": None,
            "note": MISSING_ANALYSIS_NOTE,
            "review_limitation": MISSING_ANALYSIS_REVIEW_LIMITATION,
        }
    selected = record(row)
    message = {
        key: selected[key]
        for key in ("id", "role", "content", "source_ids", "run_id", "created_at")
    }
    return {
        "status": "available",
        "message": message,
        "run": {
            "id": message["run_id"],
            "kind": selected["analysis_run_kind"],
            "status": selected["analysis_run_status"],
        },
        "note": None,
        "review_limitation": None,
    }
