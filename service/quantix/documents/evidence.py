"""Checking what the office cites. A quote must really be on the page it names; numbers must be in the quote."""

import re
from decimal import Decimal, InvalidOperation

from sqlalchemy.orm import Session

from quantix.documents import library
from quantix.documents.arabic import searchable
from quantix.documents.models import Document, WebPage

_DIGITS = str.maketrans("٠١٢٣٤٥٦٧٨٩۰۱۲۳۴۵۶۷۸۹٫٬", "01234567890123456789.,")
_NUMBER = re.compile(r"\d[\d,]*(?:\.\d+)?")


def _plain(text: str) -> str:
    return " ".join(searchable(text.translate(_DIGITS)).split())


def missing_piece(page_text: str, quote: str) -> str | None:
    """The first piece of `quote` that isn't in the page's text, in order; None when all of it is there. "..." in
    the quote may skip words between exact pieces."""
    text = _plain(page_text)
    position = 0
    for piece in (p for p in re.split(r"\.{3}|…", quote) if p.strip()):
        at = text.find(_plain(piece), position)
        if at < 0:
            return piece.strip()
        position = at + len(_plain(piece))
    return None


def check_quote(session: Session, tender_id: str, document_id: str, page: int, quote: str) -> Document:
    """The document, if `quote` appears on that page."""
    document = session.get(Document, document_id)
    if document is None or document.tender_id != tender_id or document.status == "replaced":
        raise ValueError(f"No current document has the id {document_id}.")
    found = library.page(session, document_id, page)
    if found is None:
        raise ValueError(f"{document.name} has no page {page}.")
    missing = missing_piece(found.text, quote)
    if missing is not None:
        raise ValueError(
            f"“{missing[:80]}” is not on {document.name}, page {page}. Quote the page exactly as read_page shows it."
        )
    return document


def check_web_quote(session: Session, web_page_id: str, quote: str) -> WebPage:
    """The saved web page, if `quote` appears on it."""
    page = session.get(WebPage, web_page_id)
    if page is None:
        raise ValueError(f"No saved web page has the id {web_page_id}. Read the page with read_web_page first.")
    missing = missing_piece(page.text, quote)
    if missing is not None:
        raise ValueError(
            f"“{missing[:80]}” is not on the saved page {page.url}. Quote it exactly as read_web_page shows it."
        )
    return page


def numbers_in(text: str) -> set[Decimal]:
    values = set()
    for match in _NUMBER.findall(text.translate(_DIGITS)):
        try:
            values.add(Decimal(match.replace(",", "")))
        except InvalidOperation:
            continue
    return values
