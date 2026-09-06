"""Quotation routes under the application's authenticated /api surface."""

from fastapi import APIRouter, Response

from .correspondence import QuoteService
from .correspondence_models import (
    DraftInput,
    MailSettings,
    MailSettingsPatch,
    ManualReply,
    QuotePreview,
    QuoteRecord,
    ReplyRecord,
    SendDecision,
    SyncRequest,
    SyncResult,
)


def create_router(repo):
    service = QuoteService(repo)
    service.recover_interrupted_sends()
    router = APIRouter(prefix="/api", tags=["Supplier quotations"])

    @router.get("/mail/settings", response_model=MailSettings)
    def mail_settings():
        return service.settings()

    @router.patch("/mail/settings", response_model=MailSettings)
    def update_settings(request: MailSettingsPatch):
        return service.update_settings(request.model_dump(exclude_unset=True))

    @router.post("/mail/sync", response_model=SyncResult)
    def sync(request: SyncRequest):
        return service.sync_replies(request.max_messages)

    @router.get("/tenders/{tender_id}/quotes", response_model=list[QuoteRecord])
    def quotes(tender_id: str):
        return service.list_drafts(tender_id)

    @router.post("/tenders/{tender_id}/quotes", response_model=QuoteRecord)
    def create(tender_id: str, request: DraftInput):
        return service.create_draft(tender_id, request.model_dump())

    @router.patch("/tenders/{tender_id}/quotes/{quote_id}", response_model=QuoteRecord)
    def edit(tender_id: str, quote_id: str, request: DraftInput):
        return service.edit_draft(tender_id, quote_id, request.model_dump())

    @router.get("/tenders/{tender_id}/quotes/{quote_id}/preview", response_model=QuotePreview)
    def preview(tender_id: str, quote_id: str):
        return service.preview(tender_id, quote_id)

    @router.get("/tenders/{tender_id}/quotes/{quote_id}/eml", response_class=Response)
    def eml(tender_id: str, quote_id: str):
        return Response(
            service.eml(tender_id, quote_id),
            media_type="message/rfc822",
            headers={"Content-Disposition": 'attachment; filename="quotation-request.eml"'},
        )

    @router.post("/tenders/{tender_id}/quotes/{quote_id}/approve", response_model=QuoteRecord)
    def approve(tender_id: str, quote_id: str, decision: SendDecision):
        return service.approve(tender_id, quote_id, decision.model_dump())

    @router.post("/tenders/{tender_id}/quotes/{quote_id}/send", response_model=QuoteRecord)
    def send(tender_id: str, quote_id: str, decision: SendDecision):
        return service.send(tender_id, quote_id, decision.model_dump())

    @router.get("/tenders/{tender_id}/quotes/{quote_id}/replies", response_model=list[ReplyRecord])
    def replies(tender_id: str, quote_id: str):
        return service.replies(tender_id, quote_id)

    @router.post("/tenders/{tender_id}/quotes/{quote_id}/replies", response_model=ReplyRecord)
    def register(tender_id: str, quote_id: str, request: ManualReply):
        return service.register_reply(tender_id, quote_id, request.model_dump(mode="json"))

    return router
