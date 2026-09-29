import os
import shutil
import stat
import subprocess
import sys
from pathlib import Path
from typing import Annotated

from fastapi import APIRouter, Depends, HTTPException, Request, UploadFile
from fastapi.responses import FileResponse, Response
from pydantic import BaseModel, ConfigDict
from sqlalchemy.orm import Session

from quantix import tenders
from quantix.api.tenders import DB
from quantix.documents import cad, library, readers, sheets
from quantix.documents.models import Document
from quantix.review import package
from quantix.takeoff import drawings

router = APIRouter(tags=["documents"])


def home(request: Request) -> Path:
    return request.app.state.home


Home = Annotated[Path, Depends(home)]


class DocumentOut(BaseModel):
    model_config = ConfigDict(from_attributes=True)

    id: str
    path: str
    name: str
    kind: str
    size: int
    status: str
    note: str | None
    page_count: int | None
    group_name: str | None
    description: str | None
    # coverage, kept apart: scans Quantix still has to read by OCR, pages the office opened, pages its work cites
    scans_to_read: int = 0
    opened: int = 0
    cited: int = 0


class PageOut(BaseModel):
    model_config = ConfigDict(from_attributes=True)

    number: int
    text: str
    has_text: bool


class Added(BaseModel):
    added: int
    unchanged: int


class SearchHit(BaseModel):
    document_id: str
    name: str
    page: int
    snippet: str


def _tender(session: Session, tender_id: str) -> None:
    if tenders.get_tender(session, tender_id) is None:
        raise HTTPException(status_code=404, detail="Tender not found.")


def _document(session: Session, document_id: str) -> Document:
    document = session.get(Document, document_id)
    if document is None or tenders.get_tender(session, document.tender_id) is None:
        raise HTTPException(status_code=404, detail="Document not found.")
    return document


@router.post("/tenders/{tender_id}/documents", status_code=201)
def add_documents(tender_id: str, files: list[UploadFile], session: DB, home: Home, request: Request) -> Added:
    _tender(session, tender_id)
    added = unchanged = 0
    for upload in files:
        try:
            stored = library.store(session, home, tender_id, upload.filename or "", upload.file)
        except ValueError as error:
            raise HTTPException(status_code=400, detail=str(error)) from error
        added, unchanged = (added + 1, unchanged) if stored else (added, unchanged + 1)
    request.app.state.reader.wake()
    return Added(added=added, unchanged=unchanged)


class ImportIn(BaseModel):
    """What the engineer chose on this computer: a whole folder, kept with its folders, or single files."""

    folder: str | None = None
    files: list[str] = []


SKIPPED = {"desktop.ini", "thumbs.db"}  # Windows' own files, never part of a package


@router.post("/tenders/{tender_id}/documents/import")
def import_documents(tender_id: str, body: ImportIn, session: DB, home: Home, request: Request) -> Added:
    """Copy the chosen files into the tender from where they are, as the desktop app's own pickers give them. The
    engineer's files are only read, never changed."""
    _tender(session, tender_id)
    chosen: list[tuple[str, Path]] = []
    if body.folder:
        root = Path(body.folder)
        if not root.is_dir():
            raise HTTPException(status_code=400, detail=f"The folder {root.name or body.folder} can't be found.")
        for path in sorted(root.rglob("*")):
            inside = path.relative_to(root).parts
            if path.is_file() and not any(p.startswith(".") for p in inside) and path.name.lower() not in SKIPPED:
                chosen.append((f"{root.name}/{'/'.join(inside)}", path))
    for name in body.files:
        path = Path(name)
        if not path.is_file():
            raise HTTPException(status_code=400, detail=f"{path.name} can't be found.")
        chosen.append((path.name, path))
    added = unchanged = 0
    for name, path in chosen:
        try:
            with path.open("rb") as content:
                stored = library.store(session, home, tender_id, name, content)
        except ValueError as error:
            raise HTTPException(status_code=400, detail=str(error)) from error
        except OSError as error:
            raise HTTPException(status_code=400, detail=f"{path.name} can't be read: {error.strerror}.") from error
        added, unchanged = (added + 1, unchanged) if stored else (added, unchanged + 1)
    request.app.state.reader.wake()
    return Added(added=added, unchanged=unchanged)


@router.get("/tenders/{tender_id}/documents")
def list_documents(tender_id: str, session: DB) -> list[DocumentOut]:
    _tender(session, tender_id)
    counts = {c.document.id: c for c in package.coverage(session, tender_id)}
    return [
        DocumentOut.model_validate(d).model_copy(
            update={"scans_to_read": c.ocr_waiting, "opened": c.opened, "cited": c.cited}
            if (c := counts.get(d.id))
            else {}
        )
        for d in library.documents(session, tender_id)
    ]


@router.get("/tenders/{tender_id}/search")
def search(tender_id: str, q: str, session: DB) -> list[SearchHit]:
    _tender(session, tender_id)
    return [SearchHit(**hit) for hit in library.search(session, tender_id, q)]


@router.get("/documents/{document_id}/pages/{number}")
def get_page(document_id: str, number: int, session: DB) -> PageOut:
    _document(session, document_id)
    page = library.page(session, document_id, number)
    if page is None:
        raise HTTPException(status_code=404, detail="Page not found.")
    return PageOut.model_validate(page)


@router.get("/documents/{document_id}/pages/{number}/image", response_class=Response)
def page_image(document_id: str, number: int, session: DB, home: Home) -> Response:
    document = _document(session, document_id)
    path = library.stored_file(home, document)
    if document.kind == "image" and path.suffix in (".tif", ".tiff"):  # screens can't show TIFF
        try:
            return Response(readers.render_image(path, width=None), media_type="image/png")
        except OSError as error:
            raise HTTPException(status_code=422, detail="This image is damaged and can't be shown.") from error
    if document.kind == "image":
        return FileResponse(path)
    if document.kind == "cad" and document.page_count and 1 <= number <= document.page_count:
        try:
            image, _ = cad.render(drawings.open_drawing(home, document), number, width=1400)
        except (readers.Unreadable, ValueError) as error:
            raise HTTPException(status_code=404, detail=str(error)) from error
        return Response(image, media_type="image/png")
    if document.kind != "pdf" or not document.page_count or not 1 <= number <= document.page_count:
        raise HTTPException(status_code=404, detail="This page has no image.")
    return Response(readers.render_page(path, number), media_type="image/png")


@router.get("/documents/{document_id}/file", response_class=FileResponse)
def original_file(document_id: str, session: DB, home: Home) -> FileResponse:
    document = _document(session, document_id)
    return FileResponse(library.stored_file(home, document), filename=document.name)


@router.get("/documents/{document_id}/sheets/{number}")
def sheet(document_id: str, number: int, session: DB, home: Home, hidden: bool = False) -> sheets.SheetView:
    """A sheet of a workbook as Excel shows it: its cells' text, merges, sizes and formatting. With `hidden`, the rows
    and columns the sheet hides are shown too."""
    document = _document(session, document_id)
    if document.kind != "spreadsheet":
        raise HTTPException(status_code=404, detail="This document isn't a workbook.")
    try:
        return sheets.view(library.stored_file(home, document), number, hidden)
    except readers.Unreadable as error:
        raise HTTPException(status_code=422, detail=str(error)) from error
    except ValueError as error:
        raise HTTPException(status_code=404, detail=str(error)) from error


@router.post("/documents/{document_id}/open", status_code=204)
def open_in_app(document_id: str, session: DB, home: Home) -> None:
    """Opens the file in the app this computer uses for its type. The app gets a read-only copy, so the stored file
    stays exactly as it was supplied."""
    document = _document(session, document_id)
    copy = home / "tenders" / document.tender_id / "opened" / document.id / document.name
    if not copy.exists():
        copy.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(library.stored_file(home, document), copy)
        copy.chmod(stat.S_IREAD)
    try:
        _open_with_app(copy)
    except (OSError, subprocess.CalledProcessError) as error:
        raise HTTPException(
            status_code=409, detail=f"No app on this computer opens {copy.suffix or 'these'} files."
        ) from error


def _open_with_app(path: Path) -> None:
    if sys.platform == "win32":
        os.startfile(path)  # type: ignore[attr-defined]  # Windows only
    else:
        subprocess.run(["open" if sys.platform == "darwin" else "xdg-open", str(path)], check=True)
