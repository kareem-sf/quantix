"""Document groups come from the package analysis, and the engineer's moves win."""

import hashlib

from quantix.db import dump, now
from quantix.document_groups import UNGROUPED, DocumentGroupService
from quantix.package_analysis import ensure_schema, source_fingerprint
from quantix.repository import Repository


def _file(repo, tender_id, path, status="extracted"):
    artifact, _ = repo.register_artifact(
        tender_id,
        path,
        hashlib.sha256(path.encode()).hexdigest(),
        10,
        {"kind": "pdf", "status": status, "segments": [{"locator": "page:1", "text": path}]},
    )
    return artifact


def test_groups_follow_the_package_map_and_engineer_moves(tmp_path):
    repo = Repository(tmp_path / "quantix")
    tender = repo.create_tender("Fire station")
    elec = _file(repo, tender["id"], "FIRE STATION-ELEC.pdf")
    boq = _file(repo, tender["id"], "1-BOQ.pdf")
    form = _file(repo, tender["id"], "13-site visit form.pdf", status="needs_attention")

    service = DocumentGroupService(repo)
    before = service.list(tender["id"])
    assert before.grouped is False
    assert [group.name for group in before.groups] == [UNGROUPED]

    ensure_schema(repo)
    data = {
        "documents": [
            {"document_id": elec["id"], "label": "Electrical layouts and schedules"},
            {"document_id": boq["id"], "title": "Bill of quantities"},
        ],
        "groups": [
            {"name": "Drawings", "document_ids": [elec["id"]]},
            {"name": "Bill of quantities", "document_ids": [boq["id"]]},
        ],
    }
    with repo.db.connect(write=True) as conn:
        conn.execute(
            "INSERT OR REPLACE INTO package_maps VALUES(?,?,?,?)",
            (
                tender["id"],
                dump(data),
                source_fingerprint(repo.list_artifacts(tender["id"])),
                now(),
            ),
        )

    state = service.list(tender["id"])
    assert state.grouped is True
    assert [(group.name, [doc.name for doc in group.documents]) for group in state.groups] == [
        ("Drawings", ["FIRE STATION-ELEC.pdf"]),
        ("Bill of quantities", ["1-BOQ.pdf"]),
        (UNGROUPED, ["13-site visit form.pdf"]),
    ]
    assert state.groups[0].documents[0].label == "Electrical layouts and schedules"
    assert state.problems == 1 and state.groups[2].documents[0].problem

    moved = service.move(tender["id"], form["id"], "Site visit")
    site = next(group for group in moved.groups if group.name == "Site visit")
    assert site.documents[0].moved_by_engineer is True
    assert UNGROUPED not in [group.name for group in moved.groups]
