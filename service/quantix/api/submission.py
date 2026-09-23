import os
import subprocess
import sys
from datetime import datetime
from decimal import Decimal

from fastapi import APIRouter, HTTPException, Request, UploadFile
from pydantic import BaseModel, Field
from sqlalchemy.orm import Session

from quantix import tenders
from quantix.api.tenders import DB
from quantix.documents.models import Document
from quantix.submission import export, records
from quantix.submission.models import Draft, Requirement

router = APIRouter(tags=["submission"])


class DraftOut(BaseModel):
    id: str
    title: str
    body: str
    status: str
    proposed_by: str


class RequirementOut(BaseModel):
    id: str
    section: str
    title: str
    document_id: str | None
    document_name: str | None
    page: int | None
    quote: str | None
    added_by: str
    state: str  # ready | review | missing
    draft: DraftOut | None
    ready_note: str | None
    file_name: str | None


class ColumnsOut(BaseModel):
    document_id: str
    document_name: str
    sheet: int
    rate_column: str
    amount_column: str
    proposed_by: str


class SubmissionOut(BaseModel):
    requirements: list[RequirementOut]
    columns: list[ColumnsOut]


class RequirementIn(BaseModel):
    section: str = Field(min_length=1)
    title: str = Field(min_length=1)


class DecisionIn(BaseModel):
    approve: bool
    reason: str | None = None


class ReadyIn(BaseModel):
    ready: bool
    note: str = ""


class ExportIn(BaseModel):
    spread_markups: bool = True


class ExportOut(BaseModel):
    folder: str
    files: list[str]
    priced_total: Decimal
    summary_total: Decimal
    factor: Decimal
    not_ready: list[str]


def _tender(session: Session, tender_id: str) -> None:
    if tenders.get_tender(session, tender_id) is None:
        raise HTTPException(status_code=404, detail="Tender not found.")


def _requirement(session: Session, requirement_id: str) -> Requirement:
    requirement = session.get(Requirement, requirement_id)
    if requirement is None:
        raise HTTPException(status_code=404, detail="Not found.")
    return requirement


@router.get("/tenders/{tender_id}/submission")
def get_submission(tender_id: str, session: DB) -> SubmissionOut:
    _tender(session, tender_id)
    rows = []
    for r in records.requirements(session, tender_id):
        current = records.current_draft(session, r.id)
        document = session.get(Document, r.document_id) if r.document_id else None
        rows.append(
            RequirementOut(
                id=r.id,
                section=r.section,
                title=r.title,
                document_id=r.document_id,
                document_name=document.name if document else None,
                page=r.page,
                quote=r.quote,
                added_by=r.added_by,
                state=records.state(session, r),
                draft=DraftOut.model_validate(current, from_attributes=True) if current else None,
                ready_note=r.ready_note,
                file_name=r.file_name,
            )
        )
    columns = [
        ColumnsOut(
            document_id=c.document_id,
            document_name=session.get(Document, c.document_id).name,
            sheet=c.sheet,
            rate_column=c.rate_column,
            amount_column=c.amount_column,
            proposed_by=c.proposed_by,
        )
        for c in records.pricing_columns(session, tender_id)
    ]
    return SubmissionOut(requirements=rows, columns=columns)


@router.post("/tenders/{tender_id}/requirements", status_code=201)
def add_requirement(tender_id: str, body: RequirementIn, session: DB) -> None:
    _tender(session, tender_id)
    if any(r.title.lower() == body.title.strip().lower() for r in records.requirements(session, tender_id)):
        raise HTTPException(status_code=400, detail="The checklist already has that requirement.")
    session.add(
        Requirement(tender_id=tender_id, section=body.section.strip(), title=body.title.strip(), added_by="engineer")
    )
    session.commit()


@router.post("/drafts/{draft_id}/decision")
def decide_draft(draft_id: str, body: DecisionIn, session: DB, request: Request) -> None:
    draft = session.get(Draft, draft_id)
    if draft is None:
        raise HTTPException(status_code=404, detail="Not found.")
    if draft.status != "proposed":
        raise HTTPException(status_code=400, detail="This has already been decided.")
    records.decide(session, draft, body.approve, body.reason)
    session.commit()
    request.app.state.office.engineer_spoke(draft.tender_id)


@router.post("/requirements/{requirement_id}/ready")
def mark_ready(requirement_id: str, body: ReadyIn, session: DB) -> None:
    requirement = _requirement(session, requirement_id)
    requirement.ready_note = body.note.strip() if body.ready else None
    session.commit()


@router.post("/requirements/{requirement_id}/file")
async def attach_file(requirement_id: str, file: UploadFile, session: DB, request: Request) -> None:
    requirement = _requirement(session, requirement_id)
    try:
        records.attach(request.app.state.home, requirement, file.filename or "", await file.read())
    except ValueError as error:
        raise HTTPException(status_code=400, detail=str(error)) from error
    session.commit()


@router.post("/tenders/{tender_id}/export")
def build_package(tender_id: str, body: ExportIn, session: DB, request: Request) -> ExportOut:
    """The engineer's release: the package is built in the Quantix exports folder on this computer."""
    _tender(session, tender_id)
    built = export.build(request.app.state.home, session, tender_id, body.spread_markups, datetime.now())
    return ExportOut(**vars(built))


@router.post("/exports/{folder}/open", status_code=204)
def open_folder(folder: str, request: Request) -> None:
    exports = export.exports_dir(request.app.state.home)
    path = exports / folder
    if path.parent != exports or not path.is_dir():
        raise HTTPException(status_code=404, detail="That package is not in the exports folder.")
    if sys.platform == "win32":
        os.startfile(path)  # noqa: S606  (opens Explorer on the engineer's own folder)
    else:
        subprocess.Popen(["open" if sys.platform == "darwin" else "xdg-open", str(path)])
