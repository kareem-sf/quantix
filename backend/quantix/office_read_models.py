"""Public read models for the authenticated live Tender Office."""

from __future__ import annotations

from pydantic import Field

from .office_event_models import OfficeEventPage
from .office_message_models import OfficeMessagePage
from .staff_assignment_models import StaffAssignment, StaffResult
from .staff_context_models import StaffSourceReceipt
from .staff_models import ManagerProfile, OfficeModel
from .staff_store_models import StaffProfileRecord, StaffWorkOrderRecord


class OfficePartialFlags(OfficeModel):
    """Tell the client which bounded collections require a follow-up read."""

    staff: bool = False
    assignments: bool = False
    messages: bool = False
    work_orders: bool = False
    results: bool = False
    receipts: bool = False
    versions: bool = False


class OfficeSnapshot(OfficeModel):
    """One consistent, bounded view of a Tender's live office."""

    manager: ManagerProfile
    staff: list[StaffProfileRecord] = Field(default_factory=list, max_length=300)
    assignments: list[StaffAssignment] = Field(default_factory=list, max_length=200)
    messages: OfficeMessagePage
    cursor: str | None = Field(default=None, max_length=500)
    sequence: int = Field(default=0, ge=0)
    instance_id: str = Field(min_length=1, max_length=500)
    interrupted_assignment_count: int = Field(default=0, ge=0)
    staff_total: int = Field(default=0, ge=0)
    assignments_total: int = Field(default=0, ge=0)
    partial_flags: OfficePartialFlags = Field(default_factory=OfficePartialFlags)


class StaffPage(OfficeModel):
    """Stable creation-order pagination over all retained staff identities."""

    items: list[StaffProfileRecord] = Field(default_factory=list, max_length=50)
    next_cursor: str | None = Field(default=None, max_length=1000)


class StaffDesk(OfficeModel):
    """A bounded, attributable read model for one generated colleague."""

    profile: StaffProfileRecord
    version_numbers: list[int] = Field(default_factory=list, max_length=200)
    work_orders: list[StaffWorkOrderRecord] = Field(default_factory=list, max_length=50)
    assignments: list[StaffAssignment] = Field(default_factory=list, max_length=200)
    results: list[StaffResult] = Field(default_factory=list, max_length=50)
    receipts: list[StaffSourceReceipt] = Field(default_factory=list, max_length=200)
    messages: OfficeMessagePage
    partial_flags: OfficePartialFlags = Field(default_factory=OfficePartialFlags)


class StaffVersionPage(OfficeModel):
    """Paged immutable profile versions for a generated colleague."""

    items: list[StaffProfileRecord] = Field(default_factory=list, max_length=50)
    next_offset: int | None = Field(default=None, ge=0)
    has_more: bool = False


class StaffWorkOrderPage(OfficeModel):
    """Paged immutable work orders for a generated colleague."""

    items: list[StaffWorkOrderRecord] = Field(default_factory=list, max_length=200)
    next_offset: int | None = Field(default=None, ge=0)
    has_more: bool = False


class StaffResultPage(OfficeModel):
    """Paged immutable draft results for a generated colleague."""

    items: list[StaffResult] = Field(default_factory=list, max_length=50)
    next_offset: int | None = Field(default=None, ge=0)
    has_more: bool = False


class OfficeEventPageWithInstance(OfficeEventPage):
    """Event polling response carrying the server history incarnation."""

    instance_id: str = Field(min_length=1, max_length=500)


class AssignmentPage(OfficeModel):
    """Bounded Tender-scoped assignment history."""

    items: list[StaffAssignment] = Field(default_factory=list, max_length=200)
    next_offset: int | None = Field(default=None, ge=0)
    has_more: bool = False


class StaffReceiptPage(OfficeModel):
    """Bounded canonical source-read receipt history for one assignment."""

    items: list[StaffSourceReceipt] = Field(default_factory=list, max_length=200)
    next_offset: int | None = Field(default=None, ge=0)
    has_more: bool = False


__all__ = [
    "AssignmentPage",
    "OfficeEventPageWithInstance",
    "OfficePartialFlags",
    "OfficeSnapshot",
    "StaffDesk",
    "StaffPage",
    "StaffReceiptPage",
    "StaffResultPage",
    "StaffVersionPage",
    "StaffWorkOrderPage",
]
