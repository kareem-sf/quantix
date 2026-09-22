from pathlib import Path
from typing import Annotated

from fastapi import APIRouter, Depends, HTTPException, Request, UploadFile
from fastapi.responses import FileResponse, Response
from pydantic import BaseModel, ConfigDict
from sqlalchemy.orm import Session

from quantix import tenders
from quantix.api.tenders import DB
from quantix.documents import library, readers
from quantix.documents.models import Document

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


@router.get("/tenders/{tender_id}/documents")
def list_documents(tender_id: str, session: DB) -> list[DocumentOut]:
    _tender(session, tender_id)
    return [DocumentOut.model_validate(d) for d in library.documents(session, tender_id)]


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
    if document.kind == "image":
        return FileResponse(path)
    if document.kind != "pdf" or not document.page_count or not 1 <= number <= document.page_count:
        raise HTTPException(status_code=404, detail="This page has no image.")
    return Response(readers.render_page(path, number), media_type="image/png")


@router.get("/documents/{document_id}/file", response_class=FileResponse)
def original_file(document_id: str, session: DB, home: Home) -> FileResponse:
    document = _document(session, document_id)
    return FileResponse(library.stored_file(home, document), filename=document.name)
