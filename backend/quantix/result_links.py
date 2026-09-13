"""Additional conversation links resolved from actual same-Tender saved records."""

import json


def saved_work_links(conn, tender_id, run_id):
    tables = {row[0] for row in conn.execute("SELECT name FROM sqlite_master WHERE type='table'")}
    links = []

    def add(kind, identifier, title, section, view):
        links.append(
            {
                "kind": kind,
                "id": identifier,
                "title": title,
                "target": f"/tenders/{tender_id}/{section}?view={view}&record={identifier}",
            }
        )

    if "submission_requirements" in tables:
        for row in conn.execute(
            "SELECT id,payload_json FROM submission_requirements WHERE tender_id=? AND json_extract(payload_json,'$.run_id')=? ORDER BY rowid",
            (tender_id, run_id),
        ):
            data = json.loads(row["payload_json"])
            add("requirement", row["id"], data["title"], "submission", "requirements")
    if "boq_items" in tables:
        for row in conn.execute(
            "SELECT id,data_json FROM boq_items WHERE tender_id=? AND json_extract(data_json,'$.run_id')=? ORDER BY rowid",
            (tender_id, run_id),
        ):
            add(
                "boq_item",
                row["id"],
                json.loads(row["data_json"])["description"],
                "estimate",
                "boq",
            )
    # Tool events identify the exact saved version/calculation; join against the
    # real record so an unresolvable or cross-Tender ID never becomes a link.
    if "work_product_versions" in tables:
        for row in conn.execute(
            """SELECT DISTINCT v.product_id,v.title FROM run_events e
            JOIN work_product_versions v ON v.id=json_extract(e.data_json,'$.version_id')
            WHERE e.run_id=? AND e.kind='saved_work_product' AND v.tender_id=? ORDER BY e.id""",
            (run_id, tender_id),
        ):
            add("work_product", row["product_id"], row["title"], "manager", "work-product")
    if "office_calculations" in tables:
        for row in conn.execute(
            """SELECT DISTINCT c.id,c.method_id FROM run_events e
            JOIN office_calculations c ON c.id=json_extract(e.data_json,'$.calculation_id')
            WHERE e.run_id=? AND e.kind='calculation_completed' AND c.tender_id=? ORDER BY e.id""",
            (run_id, tender_id),
        ):
            add(
                "calculation",
                row["id"],
                "Calculation: " + row["method_id"],
                "manager",
                "calculation",
            )
    return list({(link["kind"], link["id"]): link for link in links}.values())
