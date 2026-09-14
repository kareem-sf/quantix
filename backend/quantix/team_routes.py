"""The engineer's view of a tender's team and the work assigned to it."""

from fastapi import APIRouter

from .execution_context import engineer_identity
from .instruction_models import InstructionAdmission, InstructionRevisionRequest
from .office_instructions import OfficeInstructionService
from .team import TeamService
from .team_models import Assignment, StaffMember, TeamView


def create_router(repo):
    router = APIRouter(prefix="/api", tags=["Tender team"])
    team = TeamService(repo)

    @router.get("/tenders/{tender_id}/team", response_model=TeamView)
    def view(tender_id: str):
        repo.get_tender(tender_id)
        return TeamView(
            staff=team.list_staff(tender_id), assignments=team.list(tender_id, limit=200)
        )

    @router.get("/tenders/{tender_id}/team/assignments/{assignment_id}", response_model=Assignment)
    def assignment(tender_id: str, assignment_id: str):
        return team.get(tender_id, assignment_id)

    @router.post("/tenders/{tender_id}/team/staff/{staff_id}/retire", response_model=StaffMember)
    def retire(tender_id: str, staff_id: str):
        return team.retire(tender_id, staff_id)

    @router.post("/tenders/{tender_id}/runs/{run_id}/steering", response_model=InstructionAdmission)
    def steer(tender_id: str, run_id: str, request: InstructionRevisionRequest):
        """Give the running Manager an instruction it applies at its next turn."""
        return OfficeInstructionService(repo).admit(
            engineer_identity(tender_id, root_run_id=run_id), request
        )

    return router
