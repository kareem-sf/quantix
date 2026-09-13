"""Runtime installation never grants permission; compare exact reviewed proof."""

import hashlib
import json
from contextlib import contextmanager
from pathlib import Path

from .code_runtime_models import ReviewedCodeRuntime


def _hash(value):
    return hashlib.sha256(
        json.dumps(value, sort_keys=True, separators=(",", ":")).encode()
    ).hexdigest()


def available_code_runtimes(repo):
    from .code_composition import MONTY_VERSION, monty_binary
    from .podman_runtime import get_code_runtime

    root = Path(__file__).parent
    implementation = {
        name: hashlib.sha256((root / name).read_bytes()).hexdigest()
        for name in (
            "code_composition.py",
            "python_analysis.py",
            "podman_runtime.py",
            "sandbox_protocol.py",
            "code_tools.py",
            "code_runtime_scope.py",
            "code_runtime_models.py",
            "local_code_records.py",
            "tool_policy.py",
        )
    }
    result = []
    try:
        binary = monty_binary()
        runtime_hash = _hash(
            {
                "implementation": implementation,
                "version": MONTY_VERSION,
                "binary": hashlib.sha256(binary.read_bytes()).hexdigest(),
            }
        )
        result.append(
            ReviewedCodeRuntime(
                engine="monty",
                fingerprint=runtime_hash,
                version=MONTY_VERSION,
                library_versions={"pydantic-monty": MONTY_VERSION},
                seconds=10,
                memory_mib=64,
                cpus=1,
                processes=1,
                output_mib=1,
                tool_calls=64,
            )
        )
    except (ImportError, ValueError, RuntimeError):
        pass
    manifest = get_code_runtime(repo).manifest()
    if manifest and manifest.get("qualified") is True:
        result.append(
            ReviewedCodeRuntime(
                engine="python",
                fingerprint=_hash({"implementation": implementation, "manifest": manifest}),
                version="3.12",
                image_id=manifest["image_id"],
                library_versions=manifest["libraries"],
                seconds=120,
                memory_mib=2048,
                cpus=2,
                processes=64,
                output_mib=50,
                tool_calls=0,
            )
        )
    return result


def validate_code_runtime(repo, reference):
    current = next(
        (item for item in available_code_runtimes(repo) if item.engine == reference.engine), None
    )
    if current is None or any(
        getattr(current, field) != getattr(reference, field)
        for field in ("fingerprint", "version", "image_id", "library_versions")
    ):
        raise ValueError(
            "The reviewed code runtime changed or is unavailable. Review its current proof again."
        )
    for field in ("seconds", "memory_mib", "cpus", "processes", "output_mib", "tool_calls"):
        if getattr(reference, field) > getattr(current, field):
            raise ValueError("The requested code limits exceed the reviewed runtime ceiling.")
    return current


@contextmanager
def _active_work(context):
    """Hold the authority guard while confirming the caller's work is still live."""
    from .manager_runtime import ManagerRunProfiles
    from .office_tools import OfficeContext
    from .staff_routing import StaffRoutingService

    if not isinstance(context, OfficeContext):
        raise ValueError("Code work requires an active Manager or staff execution context.")
    routing = StaffRoutingService(context.repo)
    with routing.policy.connections.authority_guard(), context.repo.atomic() as conn:
        run = context.repo.get_run(context.run_id)
        if run["tender_id"] != context.tender_id or run["status"] not in {"queued", "running"}:
            raise ValueError("This code work has stopped or is outside the active Tender.")
        if context.is_staff:
            assignment = conn.execute(
                "SELECT status FROM office_assignments WHERE tender_id=? AND id=?",
                (context.tender_id, context.assignment_id),
            ).fetchone()
            if assignment is None or assignment["status"] != "running":
                raise ValueError("Code work requires an active running staff assignment.")
        else:
            manager = ManagerRunProfiles(context.repo).get(context.tender_id, context.run_id)
            if context.actor_id != manager.id:
                raise ValueError("This code work is not from the active Tender Manager.")
        yield routing


def reviewed_code_runtime(context, engine):
    with _active_work(context) as routing:
        if context.is_staff:
            references = getattr(context._revalidate_live_identity(), "code_runtimes", [])
        else:
            plan_id = (context.approved_scope or {}).get("plan_id")
            envelope = (
                routing.validate_root(context.tender_id, context.run_id, plan_id).envelope
                if plan_id
                else None
            )
            references = getattr(envelope, "code_runtimes", [])
        reference = next((item for item in references if item.engine == engine), None)
        if reference is None:
            raise ValueError("This code runtime is not selected in the reviewed work scope.")
        validate_code_runtime(context.repo, reference)
        return reference


def approved_code_definitions(context, declared=None):
    from .office_tools import source_tools

    excluded = {"execute_tool_code", "execute_python_analysis"}
    with _active_work(context) as routing:
        if context.is_staff:
            references = context._revalidate_live_identity().tools
        else:
            plan_id = (context.approved_scope or {}).get("plan_id")
            references = (
                routing.validate_root(
                    context.tender_id, context.run_id, plan_id
                ).envelope.tools
                if plan_id
                else []
            )
        identifiers = {item.id for item in references}
    if declared is not None:
        identifiers &= set(declared)
    return [item for item in source_tools() if item.name in identifiers - excluded]
