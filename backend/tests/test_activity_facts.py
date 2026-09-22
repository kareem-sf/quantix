import json

from quantix.activity_facts import fact_for


def names(kind, identifier):
    return {
        ("artifact", "doc-1"): "Systems.pdf",
        ("source", "ev-1"): "14.الرد على الاستفسارات 02 1.pdf",
        ("staff", "staff-1"): "Layla Haddad",
    }.get((kind, identifier))


def test_document_read_names_the_file_pages_and_what_was_found():
    running = fact_for("read_whole_document", "prepared", {"artifact_id": "doc-1"}, lookup=names)
    assert running == {
        "kind": "read",
        "line": "Reading",
        "subject": "Systems.pdf",
        "state": "running",
    }

    outputs = {
        "passages": [
            {
                "id": "p1",
                "artifact_name": "Systems.pdf",
                "page": 1,
                "text": "0\x14 0\x14\nFire Alarm Edwards EST4\nBMS: Building Management System Johnson Controls",
            },
            {
                "id": "p2",
                "artifact_name": "Systems.pdf",
                "page": 2,
                "text": "Card Reader HID access control",
            },
        ],
        "next_offset": 2,
    }
    done = fact_for(
        "read_whole_document", "completed", {"artifact_id": "doc-1"}, outputs, lookup=names
    )
    assert done["line"] == "Read"
    assert done["subject"] == "Systems.pdf"
    assert done["result"] == "pages 1–2"
    # Unreadable extraction noise is skipped; real lines are kept.
    assert done["found"][0] == "Fire Alarm Edwards EST4"
    assert done["details"] == ["More pages remain to be read."]
    assert done["open"] == {"artifact_id": "doc-1", "page": 1}
    assert done["state"] == "done"


def test_search_reports_matches_per_file_and_keeps_terms_behind_details():
    hits = [
        {"id": "a", "artifact_name": "3-معايير.pdf", "page": 1, "text": "x"},
        {"id": "b", "artifact_name": "3-معايير.pdf", "page": 4, "text": "x"},
        {"id": "c", "artifact_name": "Specs.pdf", "page": 44, "text": "x"},
    ]
    fact = fact_for("search_sources", "completed", {"query": "site visit", "exact": True}, hits)
    assert fact["result"] == "3 matches in 2 files"
    assert fact["found"] == ["3-معايير.pdf · pages 1–4", "Specs.pdf · page 44"]
    assert fact["details"] == ["Search terms: “site visit” (exact words)"]

    nothing = fact_for("search_sources", "completed", {"query": "postponed"}, [])
    assert nothing["result"] == "nothing found"


def test_housekeeping_checks_say_what_they_actually_found():
    notes = fact_for("list_reusable_notes", "completed", {}, {"notes": [], "next_offset": None})
    assert (notes["line"], notes["result"]) == ("Checked company guidance", "none saved yet")

    drafts = fact_for(
        "list_work_products",
        "completed",
        {},
        json.dumps({"items": [{"title": "BOQ review note", "version": 1}]}),
    )
    assert (drafts["result"], drafts["found"]) == ("1 draft", ["BOQ review note"])

    coverage = fact_for(
        "inspect_extraction_coverage",
        "completed",
        {},
        {
            "totals": {"documents": 20, "extracted": 19, "needs_attention": 1},
            "documents": [
                {
                    "name": "13.نموذج الزيارة.pdf",
                    "status": "needs_attention",
                    "other_warnings": [{"message": "Arabic text recognition is not installed."}],
                },
                {"name": "Systems.pdf", "status": "extracted"},
            ],
        },
    )
    assert coverage["result"] == "19 of 20 documents read, 1 need a second look"
    assert coverage["found"] == ["13.نموذج الزيارة.pdf: Arabic text recognition is not installed."]

    team = fact_for("list_team", "completed", {}, {"staff": [], "recent_assignments": []})
    assert team["result"] == "no staff hired yet"


def test_staff_handoff_uses_the_staff_members_name():
    fact = fact_for(
        "assign_work",
        "completed",
        {
            "staff_id": "staff-1",
            "title": "List the specified systems",
            "brief": "Read Systems.pdf and the specs.",
        },
        {"assignment_id": "x", "status": "queued"},
        lookup=names,
    )
    assert fact["assignment_id"] == "x"
    assert (fact["line"], fact["subject"], fact["result"]) == (
        "Handed work to",
        "Layla Haddad",
        "List the specified systems",
    )


def test_failures_read_plainly_and_internal_tools_are_hidden():
    failed = fact_for(
        "inspect_submission_requirements",
        "blocked",
        {"offset": 0, "limit": 30},
        error="Read 1 to 20 submission requirements from a nonnegative offset.",
        recoverable=True,
    )
    assert failed["state"] == "failed"
    assert failed["line"] == "Couldn't finish: checking submission requirements"
    assert failed["recoverable"] is True
    assert fact_for("proposal_format", "completed", {"kind": "plan"}, {}) is None
    assert fact_for("quantix_submit_result", "completed", {}, None) is None


def test_unexpected_shapes_fall_back_to_a_generic_line():
    fact = fact_for("read_whole_document", "completed", {"artifact_id": 7}, object())
    assert fact["state"] == "done"
    assert fact["line"]
    unknown = fact_for("compare_source_versions", "completed", {}, {})
    assert unknown["line"] == "Compared source versions"


def test_a_repeated_passage_read_names_the_file_and_a_corrected_call_hides_the_raw_error():
    repeat = fact_for(
        "read_source",
        "completed",
        {"source_id": "s1"},
        {"id": "s1", "text_offset": 0, "already_returned": True},
        lookup=lambda kind, identifier: "BOQ.pdf",
    )
    assert (repeat["line"], repeat["subject"]) == ("Already had a passage from", "BOQ.pdf")
    corrected = fact_for(
        "propose",
        "blocked",
        {"kind": "boq_item_proposals"},
        error="Correct these tool arguments and call again: boq_item_proposals.0.quantity: Input should be a valid string",
        recoverable=True,
    )
    assert corrected["result"] == "needed a correction"
    assert corrected["details"][0].startswith("Correct these tool arguments")
