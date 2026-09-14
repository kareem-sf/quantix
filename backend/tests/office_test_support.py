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
    """The tools a real turn offered, or the source and proposal tools outside a turn."""
    from quantix.proposal_tools import proposal_tools

    return options.get("definitions") or [*source_tools(), *proposal_tools()]


async def invoke_tool(context, name, arguments, *, options=None):
    """Invoke a registered office tool with validated current arguments."""

    definition = next(item for item in _definitions(options or {}) if item.name == name)
    value = await definition.invoke(context, arguments)
    return value


async def invoke_json_tool(context, name, arguments, *, options=None):
    value = await invoke_tool(context, name, arguments, options=options)
    return json.loads(value) if isinstance(value, str) else value


def approve_plan_and_team(repo, tender_id, plan, rationale):
    """Approve a plan as the engineer does; the Manager assigns its team at run time."""

    return repo.approve_plan(tender_id, plan["id"], rationale)
