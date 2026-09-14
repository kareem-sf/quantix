"""Publish a completed extraction into canonical current evidence.

Original file bytes and earlier evidence rows stay intact. Only the active
pointer and current-evidence flags move. A page that this run did not extract
keeps its previous readable text.
"""

from __future__ import annotations

from .db import dump, new_id


def segment_locator(segment: dict) -> str:
    locator = segment.get("locator")
    if locator:
        return str(locator)
    page = segment.get("page")
    if page is not None:
        return f"page:{int(page)}"
    raise ValueError("An extraction segment is missing a page locator.")


def publishable(segment: dict) -> bool:
    return segment.get("state") == "extracted" and bool(str(segment.get("text") or "").strip())


def publish_extraction(
    conn, *, artifact: dict, extraction_id: str, segments: list[dict], stamp: str
):
    current = {
        row["locator"]: dict(row)
        for row in conn.execute(
            "SELECT id,locator FROM evidence WHERE artifact_id=? AND COALESCE(is_current,1)=1",
            (artifact["id"],),
        )
    }
    published: list[dict] = []
    retained: list[str] = []
    for segment in segments:
        locator = segment_locator(segment)
        if not publishable(segment):
            if locator in current:
                retained.append(locator)
            continue
        previous = current.get(locator)
        if previous:
            conn.execute("UPDATE evidence SET is_current=0 WHERE id=?", (previous["id"],))
        evidence_id = new_id()
        conn.execute(
            """INSERT INTO evidence(
                id,artifact_id,locator,text,page,sheet,cell_range,kind,metadata_json,extraction_id,is_current
            ) VALUES(?,?,?,?,?,?,?,?,?,?,1)""",
            (
                evidence_id,
                artifact["id"],
                locator,
                segment.get("text") or "",
                segment.get("page"),
                segment.get("sheet"),
                segment.get("cell_range"),
                segment.get("kind") or "text",
                dump(
                    {
                        key: value
                        for key, value in {
                            "method": segment.get("method"),
                            "state": segment.get("state"),
                            "heading": segment.get("heading"),
                        }.items()
                        if value
                    }
                ),
                extraction_id,
            ),
        )
        published.append(
            {
                "id": evidence_id,
                "locator": locator,
                "replaced_id": previous["id"] if previous else None,
            }
        )
        current[locator] = {"id": evidence_id, "locator": locator}
    conn.execute(
        "UPDATE artifacts SET active_extraction_id=? WHERE id=?",
        (extraction_id, artifact["id"]),
    )
    return {"published": published, "retained_locators": retained, "stamp": stamp}
