"""Small adapters for the current provider-neutral office test boundary."""

import json

from quantix.office_tools import source_tools


def api_result_for(output, web_sources=None, usage=None):
    """Return the result shape produced by ``quantix.ai_execution``."""

    return {
        "output": output,
        "web_sources": web_sources or [],
        "usage": usage
        or {
            "requests": 1,
            "input_tokens": 100,
            "output_tokens": 50,
            "total_tokens": 150,
            "cached_input_tokens": 10,
            "reasoning_tokens": 25,
            "usage_complete": True,
            "request_details": [{"input_tokens": 100, "output_tokens": 50}],
        },
    }


def _definitions(options):
    definitions = source_tools()
    consult = options.get("consult") if options else None
    if consult is not None:
        definitions.append(consult)
    return definitions


async def invoke_tool(context, name, arguments, *, options=None):
    """Invoke a registered office tool with validated current arguments."""

    definition = next(item for item in _definitions(options or {}) if item.name == name)
    value = await definition.invoke(context, arguments)
    return value


async def invoke_json_tool(context, name, arguments, *, options=None):
    value = await invoke_tool(context, name, arguments, options=options)
    return json.loads(value) if isinstance(value, str) else value


def approve_plan_and_team(repo, tender_id, plan, rationale):
    """Create the current approved plan/team state required by job execution."""

    from quantix.ai_policy import AIPolicyService

    policy = AIPolicyService(repo)
    proposed = policy.propose_team(tender_id, plan["id"])
    approved = repo.approve_plan(tender_id, plan["id"], rationale)
    policy.approve_team(
        tender_id,
        plan["id"],
        proposed["fingerprint"],
        rationale=rationale,
        require_approved_plan=True,
    )
    return approved
