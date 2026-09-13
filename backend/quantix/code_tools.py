"""Standalone code tools under exact reviewed source/runtime/limit grants."""

import hashlib
import json
from pathlib import Path

from .ai_tools import ToolContext, tool
from .code_composition import CodeCompositionService
from .code_composition_models import CompositionRequest
from .code_runtime_scope import approved_code_definitions, reviewed_code_runtime
from .office_tools import OfficeContext
from .podman_runtime import get_code_runtime
from .python_analysis import PythonAnalysisService, SelectedPythonInput
from .python_analysis_models import PythonAnalysisRequest, PythonLimits


def code_tools():
    @tool(read_only=False, requires_invocation_id=True)
    async def execute_tool_code(ctx: ToolContext[OfficeContext], code: str, inputs: dict) -> str:
        """Compose exact reviewed tools in bounded Monty code. Access supplied data through inputs[...]. Callbacks decode JSON results to values and preserve plain text inside the same source/account/tool fences. No filesystem, network or interpreter recursion is exposed."""
        office = ctx.context
        office.require_tool("execute_tool_code")
        proof = reviewed_code_runtime(office, "monty")

        def authority():
            office.require_tool("execute_tool_code")
            office.ensure_scope_current()
            if reviewed_code_runtime(office, "monty") != proof:
                raise ValueError("The reviewed interpreter scope changed.")

        runtime = get_code_runtime(office.repo)
        async with runtime.operation():
            result = await CodeCompositionService(office.repo.home).execute(
                CompositionRequest(code=code, inputs={"inputs": inputs}),
                context=office,
                tools=approved_code_definitions(office),
                authority_check=authority,
                limits=proof,
            )
        authority()
        office.repo.event(
            office.run_id,
            "code_composed",
            "Reviewed tool code finished.",
            {
                "receipt_id": result.id,
                "status": result.status,
                "runtime_fingerprint": proof.fingerprint,
            },
        )
        return result.model_dump_json()

    @tool(read_only=False, requires_invocation_id=True)
    async def execute_python_analysis(
        ctx: ToolContext[OfficeContext], code: str, artifact_ids: list[str]
    ) -> str:
        """Run Python in the exact reviewed disposable runtime using selected original artifact copies. Limits come from the engineer's review. Input transfer does not mark a source inspected or approve any result."""
        office = ctx.context
        office.require_tool("execute_python_analysis")
        proof = reviewed_code_runtime(office, "python")
        if len(artifact_ids) > 64 or len(artifact_ids) != len(set(artifact_ids)):
            raise ValueError("Choose at most64 distinct reviewed artifacts.")
        selected, total = [], 0
        for index, identifier in enumerate(artifact_ids):
            artifact = office.ensure_artifact_allowed(identifier)
            path = office.repo.object_path(office.tender_id, identifier)
            if path.is_symlink() or path.stat().st_size > 50 * 1024 * 1024 - total:
                raise ValueError(
                    "The selected original files exceed the50MiB input limit or use a link."
                )
            with path.open("rb") as stream:
                content = stream.read(50 * 1024 * 1024 - total + 1)
            total += len(content)
            if (
                total > 50 * 1024 * 1024
                or hashlib.sha256(content).hexdigest() != artifact["content_hash"]
            ):
                raise ValueError("A selected original changed or exceeds the input limit.")
            suffix = Path(artifact["relative_path"]).suffix.lower()
            if not suffix.isascii() or len(suffix) > 10:
                suffix = ".bin"
            selected.append(
                SelectedPythonInput(
                    identifier,
                    f"input-{index:02d}{suffix}",
                    content,
                    artifact["content_hash"],
                    str(artifact["version"]),
                )
            )

        def authority():
            office.require_tool("execute_python_analysis")
            office.ensure_scope_current()
            if reviewed_code_runtime(office, "python") != proof:
                raise ValueError("The reviewed Python runtime changed.")
            for identifier in artifact_ids:
                office.ensure_artifact_allowed(identifier)

        result = await PythonAnalysisService(
            office.repo.home, runtime=get_code_runtime(office.repo)
        ).execute(
            PythonAnalysisRequest(
                code=code,
                input_ids=artifact_ids,
                limits=PythonLimits(
                    cpus=proof.cpus,
                    memory_mib=proof.memory_mib,
                    seconds=proof.seconds,
                    processes=proof.processes,
                    output_mib=proof.output_mib,
                ),
            ),
            inputs=selected,
            context=office,
            authority_check=authority,
            runtime_fingerprint=proof.fingerprint,
        )
        authority()
        office.repo.event(
            office.run_id,
            "python_analyzed",
            "Selected inputs were analyzed in the reviewed runtime.",
            {
                "receipt_id": result.id,
                "status": result.status,
                "artifact_ids": artifact_ids,
                "runtime_fingerprint": proof.fingerprint,
            },
        )
        value = result.model_dump(mode="json")
        for stream in ("stdout", "stderr"):
            value[stream + "_truncated"] = len(value[stream]) > 12000
            value[stream] = value[stream][:12000]
        value["input_names"] = {item.id: item.name for item in selected}
        return json.dumps(value, ensure_ascii=False)

    return [execute_tool_code, execute_python_analysis]
