"""Unconfirmed BOQ rows from exact passages in preserved source documents."""

import hashlib
import json
import re
import unicodedata
from decimal import Decimal

from .db import dump, new_id
from .estimate_models import SourceBoqProposal

BOQ_TABLE = """CREATE TABLE IF NOT EXISTS boq_items(
    id TEXT PRIMARY KEY,tender_id TEXT NOT NULL REFERENCES tenders(id),
    source_id TEXT NOT NULL REFERENCES evidence(id),artifact_id TEXT NOT NULL REFERENCES artifacts(id),
    active INTEGER NOT NULL DEFAULT 1,data_json TEXT NOT NULL,row_key TEXT NOT NULL DEFAULT '',
    UNIQUE(tender_id,source_id,row_key))"""

EXCLUSION_SCHEMA = (
    """CREATE TABLE IF NOT EXISTS boq_source_exclusions(
        decision_id TEXT PRIMARY KEY REFERENCES decisions(id),
        item_id TEXT NOT NULL REFERENCES boq_items(id),
        current_artifact_id TEXT REFERENCES artifacts(id))""",
    "CREATE TRIGGER IF NOT EXISTS boq_exclusion_no_update BEFORE UPDATE ON boq_source_exclusions BEGIN SELECT RAISE(ABORT,'Source exclusion basis is immutable'); END",
    "CREATE TRIGGER IF NOT EXISTS boq_exclusion_no_delete BEFORE DELETE ON boq_source_exclusions BEGIN SELECT RAISE(ABORT,'Source exclusion basis is immutable'); END",
)


def migrate_source_rows(conn):
    """Called once by the versioned database migrator, with foreign keys disabled.

    Rebuild only the obsolete unique key. IDs and referencing rows are retained;
    the migrator checks every foreign key before committing the transaction.
    """
    columns = {row[1] for row in conn.execute("PRAGMA table_info(boq_items)")}
    if not columns or "row_key" in columns:
        return
    conn.execute(BOQ_TABLE.replace("boq_items(", "boq_items_source_migration("))
    conn.execute("""INSERT INTO boq_items_source_migration
        (id,tender_id,source_id,artifact_id,active,data_json)
        SELECT id,tender_id,source_id,artifact_id,active,data_json FROM boq_items""")
    conn.execute("DROP TABLE boq_items")
    conn.execute("ALTER TABLE boq_items_source_migration RENAME TO boq_items")


# Characters OCR and PDF text layers insert that a copied excerpt cannot show:
# direction marks, joiners and byte-order marks.
_INVISIBLE = dict.fromkeys(
    map(
        ord,
        "\u200b\u200c\u200d\u200e\u200f\u202a\u202b\u202c\u202d\u202e\u2066\u2067\u2068\u2069\u061c\ufeff",
    ),
    None,
)

# The same unit written in Arabic and in English.
_UNIT_GROUPS = (
    {"m3", "m³", "cum", "cu.m", "م3", "م٣", "متر مكعب"},
    {"m2", "m²", "sqm", "sq.m", "م2", "م٢", "متر مربع"},
    {"m", "lm", "rm", "m.l", "ml", "م.ط", "م ط", "مط", "متر طولي"},
    {"no", "nos", "nr", "no.", "each", "ea", "pcs", "عدد", "قطعة"},
    {"ls", "l.s", "lump sum", "item", "مقطوعية", "مقطوع", "مقطوعيه"},
    {"kg", "كجم", "كغ", "كيلو"},
    {"t", "ton", "tonne", "طن"},
    {"set", "طقم", "مجموعة"},
)


def _plain(text: str) -> str:
    """Text as a reader sees it: compatibility forms folded, invisible marks dropped,
    runs of spaces and line breaks made single spaces."""

    return " ".join(unicodedata.normalize("NFKC", text).translate(_INVISIBLE).split())


# Arabic short vowels, tanween, shadda, sukun, dagger alef and tatweel: a copied
# excerpt often drops them or a PDF reading places them apart.
_ARABIC_MARKS = dict.fromkeys([*range(0x064B, 0x0653), 0x0670, 0x0640], None)


def matchable(text: str) -> str:
    """Text for checking that an excerpt is in its passage: as a reader sees it,
    and without Arabic vowel marks."""

    return " ".join(_plain(text).translate(_ARABIC_MARKS).split())


def _unit_variants(unit: str) -> set[str]:
    written = _plain(unit).casefold()
    for group in _UNIT_GROUPS:
        folded = {_plain(item).casefold() for item in group}
        if written in folded:
            return folded
    return {written}


# The longest row text a shortened excerpt may stand for.
MAX_FILLED_EXCERPT = 1500
_GAP = re.compile(r"\s*(?:\.\.\.|…)\s*")


def _filled_excerpt(excerpt: str, text: str) -> str | None:
    """The passage text an excerpt shortened with "..." stands for.

    Every piece must be copied exactly and appear in order in one passage; the
    words between them are taken from the passage. None when the excerpt is not
    shortened, a piece is missing, or the span is longer than one row.
    """

    pieces = [_plain(piece) for piece in _GAP.split(excerpt)]
    pieces = [piece for piece in pieces if piece]
    if len(pieces) < 2 or not _GAP.search(excerpt):
        return None
    plain = _plain(text)
    start = plain.find(pieces[0])
    while start >= 0:
        position = start + len(pieces[0])
        for piece in pieces[1:]:
            found = plain.find(piece, position)
            if found < 0:
                break
            position = found + len(piece)
        else:
            span = plain[start:position]
            return span if len(span) <= MAX_FILLED_EXCERPT else None
        start = plain.find(pieces[0], start + 1)
    return None


def _first_difference(excerpt: str, text: str) -> str:
    """Where an excerpt stops matching its passage, so one retry can fix it."""

    low, high = 0, len(excerpt)
    while low < high:
        middle = (low + high + 1) // 2
        if excerpt[:middle] in text:
            low = middle
        else:
            high = middle - 1
    if low < 3:
        return " Its opening words are not in the passage."
    start = text.find(excerpt[:low])
    return (
        f' It matches up to "{excerpt[max(0, low - 30) : low]}", then has '
        f'"{excerpt[low : low + 30]}" where the passage has "{text[start + low : start + low + 30]}".'
    )


def validate_source_row(repo, tender_id, values):
    proposal = SourceBoqProposal.model_validate(values)
    evidence = repo.get_evidence(tender_id, proposal.source_id)
    filled = _filled_excerpt(proposal.source_excerpt, evidence["text"])
    if filled:
        # Keep the full row text, and a description taken from it when none was given.
        written = {**proposal.model_dump(), "source_excerpt": filled}
        if not str(values.get("description") or "").strip():
            written["description"] = ""
        proposal = SourceBoqProposal.model_validate(written)
    artifact = repo.get_artifact(tender_id, evidence["artifact_id"])
    if not artifact["is_current"]:
        raise ValueError("The source changed. Propose the BOQ row from its current version.")
    if proposal.replaces_item_id:
        with repo.db.connect() as conn:
            old = conn.execute(
                """SELECT a.relative_path,a.is_current FROM boq_items b
                JOIN artifacts a ON a.id=b.artifact_id WHERE b.id=? AND b.tender_id=? AND b.row_key<>''""",
                (proposal.replaces_item_id, tender_id),
            ).fetchone()
        if old is None:
            raise KeyError("The replaced source row does not belong to this Tender.")
        if old["is_current"] or old["relative_path"] != artifact["relative_path"]:
            raise ValueError(
                "A replacement must cite the current revision of the replaced row's source file."
            )
    if matchable(proposal.source_excerpt) not in matchable(evidence["text"]):
        raise ValueError(
            "Copy the exact BOQ source excerpt from the selected passage."
            + _first_difference(matchable(proposal.source_excerpt), matchable(evidence["text"]))
        )
    normal = (
        _plain(proposal.source_excerpt)
        .translate(str.maketrans("٠١٢٣٤٥٦٧٨٩٫٬", "0123456789.\x00"))
        .replace("\x00", "")
    )
    numbers = re.findall(r"(?<![\w.,])\d+(?:,\d{3})*(?:\.\d+)?(?![\w.,])", normal)
    if Decimal(proposal.quantity) not in {Decimal(number.replace(",", "")) for number in numbers}:
        raise ValueError(
            f'The quantity "{proposal.quantity}" is not in the exact source excerpt. Record a calculated quantity separately.'
        )
    if not any(
        re.search(r"(?<!\w)" + re.escape(unit) + r"(?!\w)", normal.casefold())
        for unit in _unit_variants(proposal.unit)
    ):
        raise ValueError(
            f'The unit "{proposal.unit}" is not written in the source excerpt. Copy the unit as '
            "written there; conversion is a separate calculation."
        )
    # The excerpt is evidence; the proposed interpretation remains unconfirmed.
    source_path = repo.object_path(tender_id, artifact["id"])
    with source_path.open("rb") as stream:
        if hashlib.file_digest(stream, "sha256").hexdigest() != artifact["content_hash"]:
            raise ValueError("The preserved source bytes changed. Restore the source before use.")
    return proposal, evidence, artifact


def row_key(reference: str) -> str:
    return hashlib.sha256(reference.casefold().encode()).hexdigest()


def save_conflict(repo, conn, tender_id, proposal, artifact) -> str | None:
    """Why a checked row cannot be saved beside the rows already saved, or None.

    A job checks this when it stages a row, so the whole job's save never fails
    at the end on one row.
    """

    key = row_key(proposal.row_reference)
    existing = conn.execute(
        "SELECT data_json FROM boq_items WHERE tender_id=? AND source_id=? AND row_key=?",
        (tender_id, proposal.source_id, key),
    ).fetchone()
    if existing:
        saved = SourceBoqProposal.model_validate(
            json.loads(existing["data_json"])["source_proposal"]
        ).model_dump(mode="json")
        if saved != proposal.model_dump(mode="json"):
            return (
                f"A saved row from this passage already uses item number {proposal.row_reference} "
                "with different text. Give a different row its own item number, or leave this one out."
            )
        return None
    earlier = conn.execute(
        """SELECT b.data_json FROM boq_items b JOIN evidence e ON e.id=b.source_id
        WHERE b.tender_id=? AND b.artifact_id=? AND b.row_key=? AND b.active=1
          AND b.source_id<>? AND COALESCE(e.is_current,1)=0""",
        (tender_id, artifact["id"], key, proposal.source_id),
    ).fetchall()
    if any(json.loads(row["data_json"]).get("confirmed") for row in earlier):
        return (
            f"The engineer already confirmed row {proposal.row_reference} from an earlier "
            "reading of this file. Ask the engineer before changing it."
        )
    if proposal.replaces_item_id and not any(
        item["id"] == proposal.replaces_item_id for item in retired_source_rows(repo, tender_id)
    ):
        return "Choose an unresolved earlier source row to replace. Its scope may already have a checked replacement."
    return None


def propose_source_row(service, tender_id, values, *, run_id=None):
    repo = service.repo
    with repo.atomic() as conn:
        proposal, evidence, artifact = validate_source_row(repo, tender_id, values)
        if run_id:
            run = repo.get_run(run_id)
            if run["tender_id"] != tender_id:
                raise KeyError("The BOQ proposal run is outside this Tender.")
            if run["status"] not in {"queued", "running"}:
                raise ValueError("This BOQ proposal needs an active work run.")
        key = row_key(proposal.row_reference)
        original = proposal.model_dump(mode="json")
        problem = save_conflict(repo, conn, tender_id, proposal, artifact)
        if problem:
            raise ValueError(problem)
        existing = conn.execute(
            "SELECT id FROM boq_items WHERE tender_id=? AND source_id=? AND row_key=?",
            (tender_id, proposal.source_id, key),
        ).fetchone()
        if existing:
            return next(
                item for item in service.view(tender_id)["items"] if item["id"] == existing["id"]
            )
        # The same row proposed from a newer reading of the same file replaces the
        # earlier unreviewed proposal instead of sitting beside it.
        earlier = conn.execute(
            """SELECT b.id FROM boq_items b JOIN evidence e ON e.id=b.source_id
            WHERE b.tender_id=? AND b.artifact_id=? AND b.row_key=? AND b.active=1
              AND b.source_id<>? AND COALESCE(e.is_current,1)=0""",
            (tender_id, artifact["id"], key, proposal.source_id),
        ).fetchall()
        identifier = new_id()
        data = {
            "document": artifact["relative_path"],
            "sheet": evidence.get("sheet") or "",
            "locator": evidence["locator"],
            "description": proposal.description,
            "unit": proposal.unit,
            "unit_cell": "",
            "quantity_cell": "source",
            "quantity_candidates": {"source": proposal.quantity},
            "supplied_quantity": proposal.quantity,
            "confirmed": False,
            "issues": [
                "Check the proposed description, unit and quantity against the exact source excerpt before confirming this row."
            ],
            "unit_rate": None,
            "components": [],
            "currency": None,
            "tax_basis": "unknown",
            "vat_percent": None,
            "provenance": None,
            "source_proposal": original,
            "source_excerpt": proposal.source_excerpt,
            "row_reference": proposal.row_reference,
            "origin": "agent" if run_id else "engineer",
            "run_id": run_id,
        }
        conn.execute(
            "INSERT INTO boq_items(id,tender_id,source_id,artifact_id,active,data_json,row_key) VALUES(?,?,?,?,1,?,?)",
            (identifier, tender_id, proposal.source_id, artifact["id"], dump(data), key),
        )
        for previous in earlier:
            conn.execute(
                "UPDATE boq_items SET active=0,data_json=json_set(data_json,'$.superseded_by',?) WHERE id=?",
                (identifier, previous["id"]),
            )
        # A proposal does not certify that spreadsheet rows have been refreshed.
        if not any(a["kind"] == "spreadsheet" for a in repo.list_artifacts(tender_id)):
            conn.execute(
                "INSERT INTO estimate_state VALUES(?,?) ON CONFLICT(tender_id) DO UPDATE SET revision=excluded.revision",
                (tender_id, repo.get_tender(tender_id)["revision"]),
            )
        return next(item for item in service.view(tender_id)["items"] if item["id"] == identifier)


def retired_source_rows(repo, tender_id):
    """Keep removed source scope visible until a checked replacement or decision."""
    with repo.db.connect() as conn:
        rows = conn.execute(
            """SELECT b.id,b.source_id,b.artifact_id,b.data_json,
                current.id AS current_artifact_id
            FROM boq_items b JOIN artifacts old ON old.id=b.artifact_id
            LEFT JOIN artifacts current ON current.tender_id=old.tender_id
                AND current.relative_path=old.relative_path AND current.is_current=1
            WHERE b.tender_id=? AND b.row_key<>'' AND old.is_current=0
              AND NOT EXISTS (SELECT 1 FROM decisions d JOIN boq_source_exclusions x ON x.decision_id=d.id
                WHERE d.tender_id=b.tender_id AND x.item_id=b.id AND x.current_artifact_id IS current.id
                  AND d.target_type='boq_item' AND d.target_id=b.id AND d.decision='exclude_revised_boq_row')
              AND NOT EXISTS (SELECT 1 FROM boq_items replacement JOIN evidence e ON e.id=replacement.source_id
                WHERE replacement.tender_id=b.tender_id
                  AND json_extract(replacement.data_json,'$.source_proposal.replaces_item_id')=b.id
                  AND json_extract(replacement.data_json,'$.confirmed')=1
                  AND instr(e.text,json_extract(replacement.data_json,'$.source_excerpt'))>0)
            ORDER BY old.relative_path,b.rowid""",
            (tender_id,),
        )
        result = []
        for row in rows:
            data = json.loads(row["data_json"])
            result.append(
                {key: row[key] for key in ("id", "source_id", "artifact_id", "current_artifact_id")}
                | {
                    key: data.get(key)
                    for key in ("description", "row_reference", "unit", "supplied_quantity")
                }
            )
        return result


def exclude_source_row(service, tender_id, item_id, values):
    from .estimate_models import SourceRowExclusion

    decision = SourceRowExclusion.model_validate(values)
    with service.repo.atomic() as conn:
        selected = next(
            (
                item
                for item in retired_source_rows(service.repo, tender_id)
                if item["id"] == item_id
            ),
            None,
        )
        if selected is None:
            raise ValueError("Select an unresolved BOQ row from an earlier source revision.")
        if selected["current_artifact_id"] != decision.current_artifact_id:
            raise ValueError(
                "The source revision changed since this review. Review the current source before excluding the old row."
            )
        decision_id = service._decision(
            conn, tender_id, "boq_item", item_id, "exclude_revised_boq_row", decision.rationale
        )
        conn.execute(
            "INSERT INTO boq_source_exclusions VALUES(?,?,?)",
            (decision_id, item_id, decision.current_artifact_id),
        )
    return service.view(tender_id)
