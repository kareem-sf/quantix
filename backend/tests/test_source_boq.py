"""Source BOQ rows enter the actual estimate without inventing spreadsheet cells."""

import hashlib
import sqlite3

import pytest
from test_estimates import approval, rate

from quantix.estimates import EstimateService
from quantix.repository import Repository


def document(
    repo, tid, content="A1 Concrete foundations 12.5 m3\nA2 Formwork 30 m2", name="Bill.pdf"
):
    digest = hashlib.sha256(content.encode()).hexdigest()
    (repo.objects / digest).write_bytes(content.encode())
    artifact, _ = repo.register_artifact(
        tid,
        name,
        digest,
        len(content.encode()),
        {
            "kind": "pdf",
            "status": "extracted",
            "metadata": {"page_count": 1},
            "segments": [{"locator": "page:1", "page": 1, "text": content}],
        },
    )
    return artifact, repo.artifact_evidence(tid, artifact["id"], 0, 10)[0]


def row(source, **changes):
    return {
        "source_id": source["id"],
        "row_reference": "A1",
        "source_excerpt": "A1 Concrete foundations 12.5 m3",
        "description": "Concrete foundations",
        "unit": "m3",
        "quantity": "12.5",
        **changes,
    }


def test_two_rows_on_one_pdf_page_can_be_confirmed_priced_and_refreshed(tmp_path):
    repo = Repository(tmp_path)
    tid = repo.create_tender("Synthetic PDF estimate")["id"]
    artifact, source = document(repo, tid)
    service = EstimateService(repo)
    first = service.propose_source_row(tid, row(source))
    second = service.propose_source_row(
        tid,
        row(
            source,
            row_reference="A2",
            source_excerpt="A2 Formwork 30 m2",
            description="Formwork",
            quantity="30",
            unit="m2",
        ),
    )
    assert first["id"] != second["id"]
    assert first["source_id"] == second["source_id"] == source["id"]
    assert first["confirmed"] is False
    assert not service.view(tid)["complete"]
    assert service.propose_source_row(tid, row(source))["id"] == first["id"]
    service.update_item(tid, first["id"], rate(vat_percent="14") | {"unit_rate": "100"})
    service.update_item(tid, second["id"], rate(vat_percent="14") | {"unit_rate": "10"})
    view = service.refresh(tid)
    assert view["complete"] is True
    assert view["totals"][0]["total_ex_vat"] == "1550.00"
    assert view["totals"][0]["total_inc_vat"] == "1767.00"
    assert {item["id"] for item in view["items"]} == {first["id"], second["id"]}
    assert (
        repo.get_artifact(tid, artifact["id"])["content_hash"]
        == hashlib.sha256(b"A1 Concrete foundations 12.5 m3\nA2 Formwork 30 m2").hexdigest()
    )


def test_source_row_rejects_wrong_tender_invented_excerpt_and_changed_retry(tmp_path):
    repo = Repository(tmp_path)
    tid = repo.create_tender("Synthetic boundaries")["id"]
    _, source = document(repo, tid)
    other = repo.create_tender("Other")["id"]
    service = EstimateService(repo)
    with pytest.raises(KeyError):
        service.propose_source_row(other, row(source))
    with pytest.raises(ValueError, match="excerpt"):
        service.propose_source_row(tid, row(source, source_excerpt="Invented concrete row"))
    first = service.propose_source_row(tid, row(source))
    with pytest.raises(ValueError, match="already|different"):
        service.propose_source_row(tid, row(source, description="Different interpretation"))
    assert service.view(tid)["items"][0]["supplied_quantity"] == "12.5"
    document(repo, tid, "A1 Concrete foundations 15 m3")
    with pytest.raises(ValueError, match="current|changed"):
        service.propose_source_row(tid, row(source))
    assert service.refresh(tid)["items"] == []
    with repo.db.connect() as conn:
        assert conn.execute("SELECT id FROM boq_items WHERE id=?", (first["id"],)).fetchone()


def test_agent_proposal_requires_active_same_tender_run_and_never_confirms(tmp_path):
    repo = Repository(tmp_path)
    tid = repo.create_tender("Synthetic agent source rows")["id"]
    _, source = document(repo, tid)
    run = repo.create_run(tid, "manager", "Read PDF BOQ")
    service = EstimateService(repo)
    item = service.propose_source_row(tid, row(source), run_id=run["id"])
    assert item["confirmed"] is False
    with repo.db.connect() as conn:
        assert conn.execute("SELECT COUNT(*) FROM decisions").fetchone()[0] == 0
    repo.update_run(run["id"], status="cancelled")
    with pytest.raises(ValueError, match="active|stopped"):
        service.propose_source_row(tid, row(source), run_id=run["id"])


def test_existing_boq_ids_foreign_keys_and_values_survive_source_row_migration(tmp_path):
    from quantix.source_boq import migrate_source_rows

    path = tmp_path / "legacy.sqlite"
    with sqlite3.connect(path) as conn:
        conn.executescript("""
            CREATE TABLE tenders(id TEXT PRIMARY KEY); INSERT INTO tenders VALUES('tender');
            CREATE TABLE artifacts(id TEXT PRIMARY KEY); INSERT INTO artifacts VALUES('artifact');
            CREATE TABLE evidence(id TEXT PRIMARY KEY); INSERT INTO evidence VALUES('source');
            CREATE TABLE boq_items(id TEXT PRIMARY KEY,tender_id TEXT,source_id TEXT,
                artifact_id TEXT,active INTEGER,data_json TEXT,UNIQUE(tender_id,source_id));
            CREATE TABLE linked(item_id TEXT REFERENCES boq_items(id));
            INSERT INTO boq_items VALUES('item','tender','source','artifact',1,'{"confirmed":true}');
            INSERT INTO linked VALUES('item');
        """)
        conn.execute("BEGIN IMMEDIATE")
        migrate_source_rows(conn)
        assert conn.execute("PRAGMA foreign_key_check").fetchall() == []
        assert conn.execute("SELECT id,row_key,data_json FROM boq_items").fetchone() == (
            "item",
            "",
            '{"confirmed":true}',
        )
        conn.commit()
        migrate_source_rows(conn)
        assert conn.execute("SELECT item_id FROM linked").fetchone()[0] == "item"


def test_quantity_and_unit_must_be_present_in_the_exact_boq_excerpt(tmp_path):
    repo = Repository(tmp_path)
    tid = repo.create_tender("Source numbers")["id"]
    _, source = document(repo, tid)
    service = EstimateService(repo)
    with pytest.raises(ValueError, match="quantity"):
        service.propose_source_row(tid, row(source, quantity="999"))
    with pytest.raises(ValueError, match="unit"):
        service.propose_source_row(tid, row(source, unit="kg"))


def test_real_pdf_import_flows_through_api_to_a_priced_workbook(tmp_path):
    import threading

    from fastapi import FastAPI
    from fastapi.testclient import TestClient
    from openpyxl import load_workbook
    from test_documents import pdf_bytes

    from quantix.estimate_routes import create_router
    from quantix.intake import import_package
    from quantix.outputs import OutputService

    inputs = tmp_path / "originals"
    inputs.mkdir()
    original = pdf_bytes(["A1 Concrete 12 m3; A2 Formwork 30 m2"]).replace(
        b"[0 0 200 100]", b"[0 0 500 200]"
    )
    (inputs / "Bill.pdf").write_bytes(original)
    repo = Repository(tmp_path / "home")
    tid = repo.create_tender("Synthetic imported PDF")["id"]
    run = repo.create_run(tid, "import", str(inputs))
    import_package(repo, tid, inputs, run["id"], threading.Event())
    artifact = repo.list_artifacts(tid)[0]
    source = repo.artifact_evidence(tid, artifact["id"])[0]
    app = FastAPI()
    app.include_router(create_router(repo))
    with TestClient(app) as client:
        created = []
        for reference, excerpt, description, quantity, unit in [
            ("A1", "A1 Concrete 12 m3", "Concrete", "12", "m3"),
            ("A2", "A2 Formwork 30 m2", "Formwork", "30", "m2"),
        ]:
            response = client.post(
                f"/api/tenders/{tid}/estimate/source-rows",
                json=row(
                    source,
                    row_reference=reference,
                    source_excerpt=excerpt,
                    description=description,
                    quantity=quantity,
                    unit=unit,
                ),
            )
            assert response.status_code == 200, response.text
            item = response.json()
            assert item["confirmed"] is False
            created.append(item)
        service = EstimateService(repo)
        for item in created:
            service.update_item(tid, item["id"], rate(vat_percent="0"))
        output = OutputService(repo)
        record = output.generate(tid, approval(kind="boq_xlsx"))
        workbook = load_workbook(output.path(tid, record["id"]), data_only=False)
        assert "Concrete" in str(list(workbook["BOQ"].values))
        assert "Formwork" in str(list(workbook["BOQ"].values))
        assert service.view(tid)["totals"][0]["total_ex_vat"] == "424.20"
    assert (inputs / "Bill.pdf").read_bytes() == original


def test_reprocessed_source_cannot_leave_a_confirmed_row_priced(tmp_path):
    repo = Repository(tmp_path)
    tid = repo.create_tender("Source reprocessing")["id"]
    _, source = document(repo, tid)
    service = EstimateService(repo)
    item = service.propose_source_row(tid, row(source))
    service.update_item(tid, item["id"], rate(vat_percent="0"))
    with repo.db.connect(write=True) as conn:
        conn.execute(
            "UPDATE evidence SET text=? WHERE id=?",
            ("Corrected extraction: quantity unavailable", source["id"]),
        )
    current = service.view(tid)
    assert current["items"][0]["confirmed"] is False
    assert current["items"][0]["line_ex_vat"] is None
    with pytest.raises(ValueError, match="excerpt|source"):
        service.update_item(tid, item["id"], approval(confirm_source=True))


def test_revised_source_scope_cannot_disappear_behind_other_priced_rows(tmp_path):
    repo = Repository(tmp_path)
    tid = repo.create_tender("Two source documents")["id"]
    _, first = document(repo, tid, name="A.pdf")
    _, second = document(repo, tid, name="B.pdf")
    service = EstimateService(repo)
    old = service.propose_source_row(tid, row(first))
    other = service.propose_source_row(tid, row(second))
    for item in (old, other):
        service.update_item(tid, item["id"], rate(vat_percent="0"))
    assert service.view(tid)["complete"] is True
    artifact, replacement = document(repo, tid, "A1 Concrete foundations 15 m3", name="A.pdf")
    changed = service.refresh(tid)
    assert changed["complete"] is False
    assert changed["totals"][0]["total_ex_vat"] is None
    assert changed["retired_source_rows"][0]["id"] == old["id"]
    assert changed["retired_source_rows"][0]["current_artifact_id"] == artifact["id"]
    updated = service.propose_source_row(
        tid,
        row(
            replacement,
            quantity="15",
            source_excerpt="A1 Concrete foundations 15 m3",
            replaces_item_id=old["id"],
        ),
    )
    assert service.view(tid)["retired_source_rows"]
    service.update_item(tid, updated["id"], rate(vat_percent="0"))
    assert service.view(tid)["retired_source_rows"] == []
    assert service.view(tid)["complete"] is True


def test_no_longer_required_revision_needs_an_explicit_recorded_decision(tmp_path):
    repo = Repository(tmp_path)
    tid = repo.create_tender("Source exclusion")["id"]
    _, source = document(repo, tid)
    service = EstimateService(repo)
    old = service.propose_source_row(tid, row(source))
    current, _ = document(repo, tid, "Revised bill removes foundations.")
    assert service.refresh(tid)["retired_source_rows"]
    with pytest.raises(ValueError):
        service.exclude_source_row(tid, old["id"], {"rationale": "No longer in revised scope"})
    with pytest.raises(ValueError, match="revision changed"):
        service.exclude_source_row(
            tid, old["id"], approval(current_artifact_id="outdated-revision")
        )
    result = service.exclude_source_row(tid, old["id"], approval(current_artifact_id=current["id"]))
    assert result["retired_source_rows"] == []
    assert service.refresh(tid)["retired_source_rows"] == []
    with repo.db.connect() as conn:
        assert (
            conn.execute(
                "SELECT target_id FROM decisions WHERE decision='exclude_revised_boq_row'"
            ).fetchone()[0]
            == old["id"]
        )
    document(repo, tid, "Third revision changes the source again.")
    assert service.refresh(tid)["retired_source_rows"][0]["id"] == old["id"]


def test_repeated_references_on_different_pages_need_distinct_replacement_links(tmp_path):
    repo = Repository(tmp_path)
    tid = repo.create_tender("Repeated page references")["id"]
    digest = hashlib.sha256(b"old pages").hexdigest()
    (repo.objects / digest).write_bytes(b"old pages")
    artifact, _ = repo.register_artifact(
        tid,
        "Pages.pdf",
        digest,
        9,
        {
            "kind": "pdf",
            "status": "extracted",
            "segments": [
                {"locator": "page:1", "page": 1, "text": "A1 Concrete foundations 12.5 m3"},
                {"locator": "page:2", "page": 2, "text": "A1 Concrete foundations 12.5 m3"},
            ],
        },
    )
    sources = repo.artifact_evidence(tid, artifact["id"])
    service = EstimateService(repo)
    old = [service.propose_source_row(tid, row(source)) for source in sources]
    _, current = document(repo, tid, "A1 Concrete foundations 15 m3", name="Pages.pdf")
    replacement = service.propose_source_row(
        tid,
        row(
            current,
            quantity="15",
            source_excerpt="A1 Concrete foundations 15 m3",
            replaces_item_id=old[0]["id"],
        ),
    )
    service.update_item(tid, replacement["id"], rate(vat_percent="0"))
    assert [item["id"] for item in service.refresh(tid)["retired_source_rows"]] == [old[1]["id"]]
    assert service.view(tid)["complete"] is False


def test_replacement_chain_keeps_only_the_last_unresolved_source_row(tmp_path):
    repo = Repository(tmp_path)
    tid = repo.create_tender("Source chain")["id"]
    _, first = document(repo, tid)
    service = EstimateService(repo)
    old = service.propose_source_row(tid, row(first))
    _, second = document(repo, tid, "A1 Concrete foundations 15 m3")
    middle = service.propose_source_row(
        tid,
        row(
            second,
            quantity="15",
            source_excerpt="A1 Concrete foundations 15 m3",
            replaces_item_id=old["id"],
        ),
    )
    service.update_item(tid, middle["id"], rate(vat_percent="0"))
    _, third = document(repo, tid, "A1 Concrete foundations 20 m3")
    assert [item["id"] for item in service.refresh(tid)["retired_source_rows"]] == [middle["id"]]
    newest = service.propose_source_row(
        tid,
        row(
            third,
            quantity="20",
            source_excerpt="A1 Concrete foundations 20 m3",
            replaces_item_id=middle["id"],
        ),
    )
    service.update_item(tid, newest["id"], rate(vat_percent="0"))
    assert service.view(tid)["retired_source_rows"] == []


def test_ocr_text_marks_spacing_and_arabic_units_do_not_block_an_exact_excerpt(tmp_path):
    repo = Repository(tmp_path)
    tid = repo.create_tender("Arabic OCR BOQ")["id"]
    ocr = "6.3.1\u200f لوحة تحكم  الإنذار\n\nالرئيسية   عدد 1\n6.3.2 كاشف دخان عدد 58"
    _, source = document(repo, tid, content=ocr, name="BOQ.pdf")
    service = EstimateService(repo)
    item = service.propose_source_row(
        tid,
        row(
            source,
            row_reference="6.3.1",
            source_excerpt="6.3.1 لوحة تحكم الإنذار الرئيسية عدد 1",
            description="Main fire alarm control panel",
            unit="no",
            quantity="1",
        ),
    )
    assert item["unit"] == "no"
    with pytest.raises(ValueError, match="unit"):
        service.propose_source_row(
            tid,
            row(
                source,
                row_reference="6.3.2",
                source_excerpt="6.3.2 كاشف دخان عدد 58",
                description="Smoke detector",
                unit="m2",
                quantity="58",
            ),
        )
    with pytest.raises(ValueError, match="exact BOQ source excerpt"):
        service.propose_source_row(
            tid,
            row(
                source,
                row_reference="6.3.9",
                source_excerpt="6.3.9 كاشف حرارة عدد 3",
                description="Heat detector",
                unit="no",
                quantity="3",
            ),
        )


def _read_again(repo, tid, artifact, text):
    from quantix.extraction_publication import publish_extraction

    with repo.atomic() as conn:
        publish_extraction(
            conn,
            artifact=artifact,
            extraction_id="reread",
            segments=[{"locator": "page:1", "page": 1, "state": "extracted", "text": text}],
            stamp="2026-09-15T00:00:00Z",
        )
    return next(
        item
        for item in repo.artifact_evidence(tid, artifact["id"], 0, 10)
        if item.get("extraction_current", True)
    )


def test_row_proposed_again_after_a_file_is_read_again_replaces_the_unreviewed_row(tmp_path):
    repo = Repository(tmp_path)
    tid = repo.create_tender("Read again")["id"]
    artifact, source = document(repo, tid)
    service = EstimateService(repo)
    first = service.propose_source_row(tid, row(source))
    newer = _read_again(repo, tid, artifact, "A1 Concrete foundations 12.5 m3 (corrected)")
    assert any("read again" in issue for issue in service.view(tid)["items"][0]["issues"])

    second = service.propose_source_row(tid, row(newer, description="Concrete to foundations"))

    items = service.view(tid)["items"]
    assert [item["id"] for item in items] == [second["id"]]
    with repo.db.connect() as conn:
        old = conn.execute(
            "SELECT active,data_json FROM boq_items WHERE id=?", (first["id"],)
        ).fetchone()
    assert old["active"] == 0 and second["id"] in old["data_json"]


def test_confirmed_row_is_not_replaced_after_a_file_is_read_again(tmp_path):
    repo = Repository(tmp_path)
    tid = repo.create_tender("Read again confirmed")["id"]
    artifact, source = document(repo, tid)
    service = EstimateService(repo)
    item = service.propose_source_row(tid, row(source))
    service.update_item(tid, item["id"], approval(confirm_source=True))
    newer = _read_again(repo, tid, artifact, "A1 Concrete foundations 12.5 m3")

    with pytest.raises(ValueError, match="already confirmed"):
        service.propose_source_row(tid, row(newer))


def test_description_defaults_to_the_excerpt_and_errors_name_the_row(tmp_path):
    from quantix.estimate_models import SourceBoqProposal

    repo = Repository(tmp_path)
    tid = repo.create_tender("Short rows")["id"]
    _, source = document(repo, tid)
    values = row(source)
    values.pop("description")
    assert SourceBoqProposal.model_validate(values).description == "Concrete foundations"
    item = EstimateService(repo).propose_source_row(tid, values)
    assert item["description"] == "Concrete foundations"
    with pytest.raises(ValueError, match='The unit "m2" is not written'):
        EstimateService(repo).propose_source_row(tid, row(source, row_reference="A9", unit="m2"))


def test_a_long_row_excerpt_may_be_shortened_with_dots(tmp_path):
    repo = Repository(tmp_path)
    tid = repo.create_tender("Shortened excerpts")["id"]
    content = (
        "A1 Concrete foundations to all pad footings, strip footings and ground beams "
        "including formwork and curing 12.5 m3\nA2 Formwork 30 m2"
    )
    _, source = document(repo, tid, content=content)
    service = EstimateService(repo)
    item = service.propose_source_row(
        tid,
        {
            "source_id": source["id"],
            "row_reference": "A1",
            "source_excerpt": "A1 Concrete foundations ... formwork and curing 12.5 m3",
            "unit": "m3",
            "quantity": "12.5",
        },
    )
    assert item["source_excerpt"] == content.split("\n")[0]
    assert item["description"].startswith("Concrete foundations to all pad footings")
    assert item["description"].endswith("curing")
    # Every piece must be in the passage, in order.
    with pytest.raises(ValueError, match="excerpt"):
        service.propose_source_row(
            tid,
            {
                "source_id": source["id"],
                "row_reference": "A3",
                "source_excerpt": "A1 Concrete foundations ... piling 12.5 m3",
                "unit": "m3",
                "quantity": "12.5",
            },
        )


def test_an_excerpt_that_differs_says_where():
    from quantix.source_boq import _first_difference

    text = "A1 Concrete foundations 12.5 m3"
    message = _first_difference("A1 Concrete footings 12.5 m3", text)
    assert 'up to "A1 Concrete fo"' in message
    assert '"otings 12.5 m3"' in message and '"undations 12.5 m3"' in message
    assert "opening words" in _first_difference("Z9 Steel", text)


def test_refresh_does_not_bring_back_a_replaced_row(tmp_path):
    repo = Repository(tmp_path)
    tid = repo.create_tender("Refresh after read again")["id"]
    artifact, source = document(repo, tid)
    service = EstimateService(repo)
    first = service.propose_source_row(tid, row(source))
    newer = _read_again(repo, tid, artifact, "A1 Concrete foundations 12.5 m3 (corrected)")
    second = service.propose_source_row(tid, row(newer))
    service.refresh(tid)
    assert [item["id"] for item in service.view(tid)["items"]] == [second["id"]]
    assert first["id"] != second["id"]


def test_a_confirmed_row_stays_confirmed_when_its_passage_has_marks_and_line_breaks(tmp_path):
    repo = Repository(tmp_path)
    tid = repo.create_tender("Marks in passage")["id"]
    content = "A1 Concrete‏ foundations\n 12.5 m3"
    _, source = document(repo, tid, content=content)
    service = EstimateService(repo)
    item = service.propose_source_row(
        tid, row(source, source_excerpt="A1 Concrete foundations 12.5 m3")
    )
    service.update_item(tid, item["id"], approval(confirm_source=True))
    assert service.view(tid)["items"][0]["confirmed"] is True


def test_an_excerpt_without_arabic_vowel_marks_still_matches(tmp_path):
    repo = Repository(tmp_path)
    tid = repo.create_tender("Vowel marks")["id"]
    content = "5.3.1.2 مرشات ّ رذاذ منبثقة 180 درجة عدد 40"
    _, source = document(repo, tid, content=content)
    item = EstimateService(repo).propose_source_row(
        tid,
        row(
            source,
            row_reference="5.3.1.2",
            source_excerpt="5.3.1.2 مرشات رذاذ منبثقة 180 درجة عدد 40",
            description="مرشات رذاذ منبثقة",
            unit="عدد",
            quantity="40",
        ),
    )
    assert item["supplied_quantity"] == "40"


@pytest.mark.asyncio
async def test_the_estimate_tool_narrows_rows_and_sends_a_page_passage_once(tmp_path):
    import json as json_module

    from quantix.office_business import business_tools
    from quantix.office_tools import OfficeContext

    repo = Repository(tmp_path)
    tid = repo.create_tender("Estimate tool")["id"]
    _, source = document(repo, tid)
    service = EstimateService(repo)
    service.propose_source_row(tid, row(source))
    service.propose_source_row(
        tid,
        row(
            source,
            row_reference="A2",
            source_excerpt="A2 Formwork 30 m2",
            description="Formwork",
            quantity="30",
            unit="m2",
        ),
    )
    context = OfficeContext(repo, tid, repo.create_run(tid, "manager", "Price")["id"])
    tool = next(item for item in business_tools() if item.name == "inspect_estimate")

    everything = json_module.loads(await tool.invoke(context, {"page_number": 1}))
    assert everything["matching_items"] == 2
    texts = [entry["source"]["text"] for entry in everything["items"]]
    assert (
        "A1 Concrete foundations" in texts[0]
        and texts[1] == "Same passage as an earlier row above."
    )
    assert "source_proposal" not in everything["items"][0]["item"]

    one = json_module.loads(await tool.invoke(context, {"row_references": ["a2"]}))
    assert [entry["item"]["row_reference"] for entry in one["items"]] == ["A2"]
    assert json_module.loads(await tool.invoke(context, {"page_number": 2}))["matching_items"] == 0


def test_a_confirmed_row_no_longer_asks_to_be_checked(tmp_path):
    repo = Repository(tmp_path)
    tid = repo.create_tender("Checked rows")["id"]
    _, source = document(repo, tid)
    service = EstimateService(repo)
    item = service.propose_source_row(tid, row(source))
    assert any(
        issue.startswith("Check the proposed") for issue in service.view(tid)["items"][0]["issues"]
    )
    service.update_item(tid, item["id"], approval(confirm_source=True))
    assert not any(
        issue.startswith("Check the proposed") for issue in service.view(tid)["items"][0]["issues"]
    )
