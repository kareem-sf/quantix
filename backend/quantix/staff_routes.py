"""Authenticated read-only routes for the live Tender Office."""

from __future__ import annotations

from fastapi import APIRouter, HTTPException, Query

from .assignment_graph import AssignmentGraphService
from .assignment_graph_models import (
    GraphDraft,
    GraphRevisionRequest,
    WorkGraphStatus,
    WorkGraphVersion,
)
from .execution_context import OfficeExecutionIdentity
from .instruction_models import InstructionAdmission, InstructionRevisionRequest
from .office_coordination import OfficeCoordinationService
from .office_delivery_models import CoordinationRequest, DeliveryReceipt, ReceiptRequest
from .office_handoff_models import Handoff, HandoffPage, HandoffPayload, TransferRequest
from .office_handoffs import OfficeHandoffService
from .office_instructions import OfficeInstructionService, WorkStatusSnapshot
from .office_message_models import OfficeMessagePage
from .office_read import OfficeReadService
from .office_read_models import (
    AssignmentPage,
    OfficeEventPageWithInstance,
    OfficeSnapshot,
    StaffDesk,
    StaffPage,
    StaffReceiptPage,
    StaffResultPage,
    StaffVersionPage,
    StaffWorkOrderPage,
)
from .staff_assignment_models import StaffAssignment, StaffResult
from .staff_lifecycle import StaffLifecycleService
from .staff_lifecycle_models import (
    StaffLifecycleReceipt,
    StaffLifecycleRequest,
    StaffPreferenceRequest,
)
from .staff_models import OfficeConflict
from .staff_notebook_models import (
    NotebookEntry,
    NotebookEntryDraft,
    NotebookKind,
    NotebookPage,
    NotebookQuery,
)
from .staff_notebooks import StaffNotebookService
from .staff_store_models import StaffProfileRecord


def _call(read):
    """Map domain read failures to the same public handlers as the app."""

    try:
        return read()
    except KeyError as error:
        raise HTTPException(
            status_code=404,
            detail="This item could not be found in the selected Tender.",
        ) from error
    except ValueError as error:
        raise HTTPException(status_code=409, detail=str(error)) from error


def _engineer_ctx(tender_id: str) -> OfficeExecutionIdentity:
    return OfficeExecutionIdentity(
        tender_id=tender_id,
        actor_kind="engineer",
        actor_id="engineer",
        root_run_id=None,
        budget_scope_id=None,
        assignment_id=None,
        profile_version=None,
        route_binding_id=None,
        instruction_revision_id=None,
        grant_fingerprint=None,
        ownership_epoch=None,
        trusted_invocation_id=None,
    )


def create_router(repo, reader=None) -> APIRouter:
    """Build the bounded office read API around one cached reader."""

    service = reader or OfficeReadService(repo)
    lifecycle = StaffLifecycleService(repo)
    notebooks = StaffNotebookService(repo)
    handoffs = OfficeHandoffService(repo)
    coordination = OfficeCoordinationService(repo)
    graphs = AssignmentGraphService(repo)
    instructions = OfficeInstructionService(repo)
    router = APIRouter(prefix="/api", tags=["Tender Office"])

    @router.get("/tenders/{tender_id}/office", response_model=OfficeSnapshot)
    def get_office(tender_id: str):
        return _call(lambda: service.snapshot(tender_id))

    @router.get("/tenders/{tender_id}/staff", response_model=StaffPage)
    def get_staff(
        tender_id: str,
        cursor: str | None = Query(None, max_length=1000),
        limit: int = Query(50, ge=1, le=50),
        lifecycle: str | None = Query(None, max_length=40),
    ):
        return _call(
            lambda: service.staff_page(
                tender_id, cursor=cursor, limit=limit, lifecycle=lifecycle
            )
        )

    @router.post(
        "/tenders/{tender_id}/staff/{staff_id}/lifecycle",
        response_model=StaffLifecycleReceipt,
    )
    def change_staff_lifecycle(tender_id: str, staff_id: str, command: StaffLifecycleRequest):
        if command.staff_id != staff_id:
            raise HTTPException(status_code=409, detail="The staff identity does not match this desk.")
        try:
            return lifecycle.transition(_engineer_ctx(tender_id), command)
        except OfficeConflict as error:
            raise HTTPException(status_code=409, detail=str(error)) from error
        except KeyError as error:
            raise HTTPException(
                status_code=404,
                detail="This item could not be found in the selected Tender.",
            ) from error
        except ValueError as error:
            raise HTTPException(status_code=409, detail=str(error)) from error

    @router.patch(
        "/tenders/{tender_id}/staff/{staff_id}/preferences",
        response_model=StaffProfileRecord,
    )
    def change_staff_preferences(tender_id: str, staff_id: str, command: StaffPreferenceRequest):
        if command.staff_id != staff_id:
            raise HTTPException(status_code=409, detail="The staff identity does not match this desk.")
        try:
            return lifecycle.revise_preferences(_engineer_ctx(tender_id), command)
        except OfficeConflict as error:
            raise HTTPException(status_code=409, detail=str(error)) from error
        except KeyError as error:
            raise HTTPException(
                status_code=404,
                detail="This item could not be found in the selected Tender.",
            ) from error
        except ValueError as error:
            raise HTTPException(status_code=409, detail=str(error)) from error

    @router.post(
        "/tenders/{tender_id}/staff/{staff_id}/notebook",
        response_model=NotebookEntry,
    )
    def append_notebook(tender_id: str, staff_id: str, command: NotebookEntryDraft):
        try:
            return notebooks.append(_engineer_ctx(tender_id), staff_id, command)
        except KeyError as error:
            raise HTTPException(
                status_code=404,
                detail="This item could not be found in the selected Tender.",
            ) from error
        except ValueError as error:
            raise HTTPException(status_code=409, detail=str(error)) from error

    @router.get(
        "/tenders/{tender_id}/staff/{staff_id}/notebook",
        response_model=NotebookPage,
    )
    def get_notebook(
        tender_id: str,
        staff_id: str,
        cursor: str | None = Query(None, max_length=1000),
        limit: int = Query(50, ge=1, le=200),
        query: str = Query("", max_length=1000),
        kind: NotebookKind | None = Query(None),
        current: bool | None = Query(None),
    ):
        return _call(
            lambda: notebooks.retrieve(
                _engineer_ctx(tender_id),
                NotebookQuery(
                    staff_id=staff_id,
                    query=query,
                    kind=kind,
                    current=current,
                    limit=limit,
                    cursor=cursor,
                ),
            )
        )

    @router.post("/tenders/{tender_id}/handoffs", response_model=Handoff)
    def transfer_handoff(tender_id: str, command: TransferRequest):
        try:
            return handoffs.transfer_saved_result(_engineer_ctx(tender_id), command)
        except OfficeConflict as error:
            raise HTTPException(status_code=409, detail=str(error)) from error
        except KeyError as error:
            raise HTTPException(
                status_code=404,
                detail="This item could not be found in the selected Tender.",
            ) from error
        except ValueError as error:
            raise HTTPException(status_code=409, detail=str(error)) from error

    @router.get("/tenders/{tender_id}/handoffs", response_model=HandoffPage)
    def list_handoffs(
        tender_id: str,
        staff_id: str = Query(..., min_length=1, max_length=64),
        direction: str = Query("received", max_length=20),
        cursor: str | None = Query(None, max_length=1000),
        limit: int = Query(50, ge=1, le=50),
    ):
        if direction not in {"received", "sent"}:
            raise HTTPException(
                status_code=422, detail="Choose received or sent handoffs."
            )
        return _call(
            lambda: handoffs.list_handoffs(
                tender_id,
                staff_id=staff_id,
                direction=direction,
                limit=limit,
                cursor=cursor,
            )
        )

    @router.get("/tenders/{tender_id}/handoffs/{handoff_id}", response_model=HandoffPayload)
    def read_handoff(
        tender_id: str,
        handoff_id: str,
        offset: int = Query(0, ge=0),
        limit: int = Query(50, ge=1, le=200),
    ):
        from .office_handoff_models import HandoffSelection

        return _call(
            lambda: handoffs.read_handoff(
                _engineer_ctx(tender_id),
                HandoffSelection(handoff_id=handoff_id, offset=offset, limit=limit),
            )
        )

    @router.post("/tenders/{tender_id}/office/deliveries", response_model=DeliveryReceipt)
    def deliver_office_message(tender_id: str, command: CoordinationRequest):
        try:
            return coordination.deliver(_engineer_ctx(tender_id), command)
        except OfficeConflict as error:
            raise HTTPException(status_code=409, detail=str(error)) from error
        except KeyError as error:
            raise HTTPException(
                status_code=404,
                detail="This item could not be found in the selected Tender.",
            ) from error
        except ValueError as error:
            raise HTTPException(status_code=409, detail=str(error)) from error

    @router.post(
        "/tenders/{tender_id}/office/messages/{message_id}/acknowledge",
        response_model=DeliveryReceipt,
    )
    def acknowledge_office_message(tender_id: str, message_id: str, command: ReceiptRequest):
        if command.message_id != message_id:
            raise HTTPException(status_code=409, detail="The message identity does not match.")
        try:
            return coordination.acknowledge(_engineer_ctx(tender_id), command)
        except OfficeConflict as error:
            raise HTTPException(status_code=409, detail=str(error)) from error
        except KeyError as error:
            raise HTTPException(
                status_code=404,
                detail="This item could not be found in the selected Tender.",
            ) from error
        except ValueError as error:
            raise HTTPException(status_code=409, detail=str(error)) from error

    @router.get(
        "/tenders/{tender_id}/office/messages/{message_id}/delivery",
        response_model=DeliveryReceipt,
    )
    def message_delivery_state(tender_id: str, message_id: str):
        try:
            return coordination.delivery_for(tender_id, message_id)
        except KeyError as error:
            raise HTTPException(
                status_code=404,
                detail="This item could not be found in the selected Tender.",
            ) from error
        except ValueError as error:
            raise HTTPException(status_code=409, detail=str(error)) from error

    @router.post("/tenders/{tender_id}/office/graphs", response_model=WorkGraphVersion)
    def propose_work_graph(tender_id: str, command: GraphDraft):
        try:
            return graphs.propose_graph(_engineer_ctx(tender_id), command)
        except OfficeConflict as error:
            raise HTTPException(status_code=409, detail=str(error)) from error
        except ValueError as error:
            raise HTTPException(status_code=409, detail=str(error)) from error

    @router.post("/tenders/{tender_id}/office/graphs/revisions", response_model=WorkGraphVersion)
    def revise_work_graph(tender_id: str, command: GraphRevisionRequest):
        try:
            ident = _engineer_ctx(tender_id)
            from .execution_context import OfficeExecutionIdentity

            ident = OfficeExecutionIdentity(
                tender_id=tender_id,
                actor_kind=ident.actor_kind,
                actor_id=ident.actor_id,
                root_run_id=command.root_run_id,
                budget_scope_id=ident.budget_scope_id,
                assignment_id=ident.assignment_id,
                profile_version=ident.profile_version,
                route_binding_id=ident.route_binding_id,
                instruction_revision_id=ident.instruction_revision_id,
                grant_fingerprint=ident.grant_fingerprint,
                ownership_epoch=ident.ownership_epoch,
                trusted_invocation_id=ident.trusted_invocation_id,
            )
            return graphs.revise_graph(ident, command)
        except OfficeConflict as error:
            raise HTTPException(status_code=409, detail=str(error)) from error
        except ValueError as error:
            raise HTTPException(status_code=409, detail=str(error)) from error

    @router.get("/tenders/{tender_id}/office/graphs/latest", response_model=WorkGraphStatus)
    def latest_work_graph(
        tender_id: str, root_run_id: str = Query(..., min_length=1, max_length=64)
    ):
        try:
            status = graphs.status(tender_id, root_run_id)
        except ValueError as error:
            raise HTTPException(status_code=409, detail=str(error)) from error
        if status is None:
            raise HTTPException(
                status_code=404,
                detail="This item could not be found in the selected Tender.",
            )
        return status

    @router.get("/tenders/{tender_id}/office/status", response_model=WorkStatusSnapshot)
    def office_status(tender_id: str, root_run_id: str | None = Query(None, max_length=160)):
        ident = _engineer_ctx(tender_id)
        from .execution_context import OfficeExecutionIdentity

        ident = OfficeExecutionIdentity(
            tender_id=tender_id,
            actor_kind=ident.actor_kind,
            actor_id=ident.actor_id,
            root_run_id=root_run_id,
            budget_scope_id=root_run_id,
            assignment_id=None,
            profile_version=None,
            route_binding_id=None,
            instruction_revision_id=None,
            grant_fingerprint=None,
            ownership_epoch=None,
            trusted_invocation_id=None,
        )
        return _call(lambda: instructions.status(ident))
    @router.post("/tenders/{tender_id}/office/instructions", response_model=InstructionAdmission)
    def admit_instruction(tender_id: str, command: InstructionRevisionRequest):
        from .execution_context import OfficeExecutionIdentity

        active = [
            run for run in repo.list_runs(tender_id)
            if run["status"] in {"queued", "running"} and run["kind"] == "manager"
        ]
        if not active:
            raise HTTPException(status_code=409, detail="There is no running Manager work to steer.")
        active.sort(key=lambda run: (run.get("created_at") or "", run["id"]), reverse=True)
        root_run_id = active[0]["id"]
        ident = OfficeExecutionIdentity(
            tender_id=tender_id,
            actor_kind="engineer",
            actor_id="engineer",
            root_run_id=root_run_id,
            budget_scope_id=root_run_id,
            assignment_id=None,
            profile_version=None,
            route_binding_id=None,
            instruction_revision_id=None,
            grant_fingerprint=None,
            ownership_epoch=None,
            trusted_invocation_id=None,
        )
        try:
            return instructions.admit(ident, command)
        except OfficeConflict as error:
            raise HTTPException(status_code=409, detail=str(error)) from error
        except KeyError as error:
            raise HTTPException(
                status_code=404,
                detail="This item could not be found in the selected Tender.",
            ) from error
        except ValueError as error:
            raise HTTPException(status_code=409, detail=str(error)) from error

    @router.get(
        "/tenders/{tender_id}/office/instructions", response_model=list[InstructionAdmission]
    )
    def list_instructions(tender_id: str, root_run_id: str = Query(..., min_length=1, max_length=64)):
        try:
            return instructions.list(tender_id, root_run_id)
        except ValueError as error:
            raise HTTPException(status_code=409, detail=str(error)) from error

    @router.get("/tenders/{tender_id}/staff/{staff_id}", response_model=StaffDesk)
    def get_staff_desk(
        tender_id: str,
        staff_id: str,
        version: int | None = Query(None, ge=1),
    ):
        return _call(lambda: service.staff_desk(tender_id, staff_id, version=version))

    @router.get("/tenders/{tender_id}/staff/{staff_id}/versions", response_model=StaffVersionPage)
    def get_staff_versions(
        tender_id: str,
        staff_id: str,
        offset: int = Query(0, ge=0),
        limit: int = Query(50, ge=1, le=50),
    ):
        return _call(lambda: service.versions_page(tender_id, staff_id, offset=offset, limit=limit))

    @router.get(
        "/tenders/{tender_id}/staff/{staff_id}/work-orders",
        response_model=StaffWorkOrderPage,
    )
    def get_staff_work_orders(
        tender_id: str,
        staff_id: str,
        offset: int = Query(0, ge=0),
        limit: int = Query(100, ge=1, le=200),
    ):
        return _call(
            lambda: service.work_orders_page(tender_id, staff_id, offset=offset, limit=limit)
        )

    @router.get("/tenders/{tender_id}/staff/{staff_id}/results", response_model=StaffResultPage)
    def get_staff_results(
        tender_id: str,
        staff_id: str,
        offset: int = Query(0, ge=0),
        limit: int = Query(50, ge=1, le=50),
    ):
        return _call(lambda: service.results_page(tender_id, staff_id, offset=offset, limit=limit))

    @router.get("/tenders/{tender_id}/office/events", response_model=OfficeEventPageWithInstance)
    def get_office_events(
        tender_id: str,
        after: str | None = Query(None, max_length=500),
        limit: int = Query(100, ge=1, le=100),
    ):
        return _call(lambda: service.events_page(tender_id, after=after, limit=limit))

    @router.get("/tenders/{tender_id}/office/messages", response_model=OfficeMessagePage)
    def get_office_messages(
        tender_id: str,
        cursor: str | None = Query(None, max_length=1000),
        limit: int = Query(50, ge=1, le=50),
        staff_id: str | None = Query(None, max_length=160),
        assignment_id: str | None = Query(None, max_length=160),
    ):
        return _call(
            lambda: service.messages_page(
                tender_id,
                cursor=cursor,
                limit=limit,
                staff_id=staff_id,
                assignment_id=assignment_id,
            )
        )

    @router.get("/tenders/{tender_id}/office/assignments", response_model=AssignmentPage)
    def get_office_assignments(
        tender_id: str,
        staff_id: str | None = Query(None, max_length=160),
        root_run_id: str | None = Query(None, max_length=160),
        offset: int = Query(0, ge=0),
        limit: int = Query(100, ge=1, le=200),
    ):
        return _call(
            lambda: service.assignments_page(
                tender_id,
                staff_id=staff_id,
                root_run_id=root_run_id,
                offset=offset,
                limit=limit,
            )
        )

    @router.get(
        "/tenders/{tender_id}/office/assignments/{assignment_id}",
        response_model=StaffAssignment,
    )
    def get_office_assignment(tender_id: str, assignment_id: str):
        return _call(lambda: service.assignment(tender_id, assignment_id))

    @router.get("/tenders/{tender_id}/office/results/{result_id}", response_model=StaffResult)
    def get_office_result(tender_id: str, result_id: str):
        return _call(lambda: service.result(tender_id, result_id))

    @router.get(
        "/tenders/{tender_id}/office/assignments/{assignment_id}/receipts",
        response_model=StaffReceiptPage,
    )
    def get_office_receipts(
        tender_id: str,
        assignment_id: str,
        offset: int = Query(0, ge=0),
        limit: int = Query(200, ge=1, le=200),
    ):
        return _call(
            lambda: service.receipts_page(tender_id, assignment_id, offset=offset, limit=limit)
        )

    return router


__all__ = ["create_router"]
