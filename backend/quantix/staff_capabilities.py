"""Closed product-owned capabilities available to generated office staff.

Capability IDs are the actual provider-neutral ``ToolDefinition.name`` values.
The registry is intentionally assembled once from the bundled source/business,
knowledge, project and measurement tools; profile wording never adds entries.
"""

from __future__ import annotations

from functools import lru_cache

from .staff_routing_models import ToolCapability


class CapabilityUnavailable(ValueError):
    """Raised when a profile requests an operation outside the closed catalog."""


# These are local mutation/external-delivery identities even if a future tool
# module exposes one through its general list. They are deliberately excluded
# from specialist authority.
_EXCLUDED_OPERATION_NAMES = {
    "create_staff",
    "revise_staff",
    "execute_staff",
    "send_message",
    "send_email",
    "approve",
    "approve_plan",
    "approve_submission",
}

# Operations specialists may hold under a reviewed root allowance. Each
# service enforces its own effects, source, runtime and spending
# grants; inclusion here alone does not authorize a remote call or code run.
# Child assignments inherit the parent's staff, route, sources and budget.
_GRANTED_MUTATION_NAMES = {
    "request_child_assignment",
    "calculate_engineering",
    "check_engineering_calculation",
    "save_work_product",
    "cite_public_passages",
    "record_market_observation",
    "save_working_memory",
    "search_public_sources",
    "execute_tool_code",
    "execute_python_analysis",
}


@lru_cache(maxsize=1)
def _catalog() -> tuple[ToolCapability, ...]:
    # Import lazily so importing the DTOs never initializes a Repository or
    # provider runtime. The returned definitions are the installed L09a
    # ToolDefinition objects and their truthful effect metadata.
    from .office_tools import MANAGER_ONLY_TOOLS, source_tools

    definitions = [
        item
        for item in source_tools()
        if item.name not in _EXCLUDED_OPERATION_NAMES
        and item.name not in MANAGER_ONLY_TOOLS
        and (item.read_only or item.name in _GRANTED_MUTATION_NAMES)
    ]
    by_name: dict[str, ToolCapability] = {}
    for definition in definitions:
        if definition.name in by_name:
            raise RuntimeError("The bundled Tender tool catalog contains duplicate capability IDs.")
        by_name[definition.name] = ToolCapability(
            id=definition.name,
            version=1,
            description=definition.description,
            read_only=definition.read_only,
        )
    return tuple(by_name[name] for name in sorted(by_name))


def capability_catalog() -> tuple[ToolCapability, ...]:
    """Return independent immutable capability records in stable ID order."""

    return tuple(item.model_copy(deep=True) for item in _catalog())


def get_capability(identifier: str) -> ToolCapability:
    """Resolve one exact capability ID without aliases or role fallback."""

    for capability in _catalog():
        if capability.id == identifier:
            return capability
    raise CapabilityUnavailable(
        f"The requested Tender Office capability '{identifier}' is unavailable."
    )


def resolve_capabilities(identifiers: list[str] | tuple[str, ...]) -> list[ToolCapability]:
    """Resolve an explicit list; an empty list intentionally grants no tools."""

    if not isinstance(identifiers, (list, tuple)):
        raise TypeError("Capability IDs must be provided as a list.")
    if len(identifiers) > 100:
        raise ValueError("A delegation may request at most 100 capabilities.")
    if len(set(identifiers)) != len(identifiers):
        raise ValueError("A delegation cannot repeat a capability ID.")
    return [get_capability(identifier) for identifier in identifiers]


__all__ = [
    "CapabilityUnavailable",
    "ToolCapability",
    "capability_catalog",
    "get_capability",
    "resolve_capabilities",
]
