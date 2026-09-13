"""Provider-neutral tool definitions using ordinary Pydantic JSON schemas."""

import hashlib
import inspect
from dataclasses import dataclass
from typing import Any, Callable, Generic, TypeVar, get_type_hints

from pydantic import ConfigDict, ValidationError, create_model

T = TypeVar("T")


class ToolArgumentError(ValueError):
    """The caller asked for a window the tool cannot serve, and may ask again.

    Only complaints about the supplied arguments belong here. A refused
    capability or an out-of-scope source is never one of these.
    """


# Identities the server resolves for every call. A tool argument never carries them.
SERVER_IDENTITY_FIELDS = frozenset({
    "tender_id", "run_id", "root_run_id", "actor_id", "actor_kind", "route_binding_id",
    "budget_scope_id", "grant_fingerprint", "invocation_id", "trusted_invocation_id",
})


def argument_problem(error: ValidationError, limit: int = 5) -> str:
    """Name the argument fields a call got wrong without echoing their values."""

    problems = []
    for item in error.errors(include_input=False, include_url=False)[:limit]:
        location = ".".join(str(part) for part in item.get("loc", ())) or "arguments"
        problems.append(f"{location}: {item.get('msg', 'is invalid')}")
    more = len(error.errors()) - len(problems)
    suffix = f" ({more} more)" if more > 0 else ""
    return "Correct these tool arguments and call again: " + "; ".join(problems) + suffix


@dataclass
class ToolContext(Generic[T]):
    context: T
    invocation_id: str | None = None


@dataclass(frozen=True)
class ToolDefinition:
    name: str
    description: str
    parameters: dict
    function: Any
    argument_model: Any
    read_only: bool = True
    idempotent: bool = False
    requires_invocation_id: bool = False

    async def invoke(self, context, arguments, *, invocation_id: str | None = None):
        if invocation_id is not None and (
            not isinstance(invocation_id, str) or not 0 < len(invocation_id) <= 160
        ):
            raise ValueError("The trusted invocation identity is invalid.")
        if self.requires_invocation_id and not invocation_id:
            raise ValueError("A trusted invocation identity is required for this tool.")
        try:
            parsed = self.argument_model.model_validate(arguments)
        except ValidationError as error:
            if any(
                item.get("type") == "extra_forbidden"
                and item.get("loc", ())[:1]
                and item["loc"][0] in SERVER_IDENTITY_FIELDS
                for item in error.errors(include_input=False, include_url=False)
            ):
                # Naming a Tender, run or actor is an attempt to change scope, not a typo.
                raise ValueError(
                    "Tender tool calls cannot name a Tender, run or actor. The server supplies them."
                ) from None
            # Any other malformed call is the model's to correct.
            raise ToolArgumentError(argument_problem(error)) from None
        result = self.function(ToolContext(context, invocation_id), **parsed.model_dump())
        return await result if inspect.isawaitable(result) else result


def _definition(
    function: Callable[..., Any],
    *,
    read_only: bool,
    idempotent: bool,
    requires_invocation_id: bool,
) -> ToolDefinition:
    hints = get_type_hints(function)
    fields = {}
    for index, (name, parameter) in enumerate(inspect.signature(function).parameters.items()):
        if index == 0:
            continue
        fields[name] = (
            hints.get(name, Any),
            ... if parameter.default is inspect.Parameter.empty else parameter.default,
        )
    arguments = create_model(
        function.__name__ + "Arguments", __config__=ConfigDict(extra="forbid"), **fields
    )
    return ToolDefinition(
        function.__name__,
        inspect.getdoc(function) or function.__name__,
        arguments.model_json_schema(),
        function,
        arguments,
        read_only=read_only,
        idempotent=idempotent,
        requires_invocation_id=requires_invocation_id,
    )


def tool(
    function: Callable[..., Any] | None = None,
    *,
    read_only: bool = True,
    idempotent: bool = False,
    requires_invocation_id: bool = False,
):
    """Build a provider-neutral tool definition from a context-first function.

    ``@tool`` remains the shorthand for the read-only defaults.  Mutation
    wrappers opt in to truthful effect metadata and trusted invocation IDs via
    ``@tool(read_only=False, ...)``.  The metadata is kept out of the argument
    schema sent to providers.
    """

    if (
        type(read_only) is not bool
        or type(idempotent) is not bool
        or type(requires_invocation_id) is not bool
    ):
        raise TypeError("Tool effect metadata must be boolean.")
    if read_only and requires_invocation_id:
        raise ValueError("A read-only tool cannot require an invocation identity.")

    def decorate(fn: Callable[..., Any]) -> ToolDefinition:
        return _definition(
            fn,
            read_only=read_only,
            idempotent=idempotent,
            requires_invocation_id=requires_invocation_id,
        )

    return decorate(function) if function is not None else decorate


def trusted_invocation_id(
    namespace: str, tool_name: str, external_id: str | int | None
) -> str | None:
    """Return a bounded opaque identity for one bridge namespace and tool call.

    Provider or client supplied IDs are never exposed in the returned value.
    The bridge namespace is server-generated and the digest binds the external
    ID to the specific tool, so replaying the same call in one bridge remains
    stable while other sessions and tools remain distinct.
    """

    if not isinstance(namespace, str) or not namespace or len(namespace) > 64:
        raise ValueError("The bridge invocation namespace is invalid.")
    if not isinstance(tool_name, str) or not tool_name or len(tool_name) > 128:
        raise ValueError("The bridge tool name is invalid.")
    if external_id is None or isinstance(external_id, bool):
        return None
    if isinstance(external_id, str):
        type_tag = "s"
    elif isinstance(external_id, int):
        type_tag = "i"
    else:
        return None
    value = str(external_id)
    if not value or len(value) > 256:
        return None
    digest = hashlib.sha256(f"{tool_name}\x00{type_tag}\x00{value}".encode("utf-8")).hexdigest()
    return f"qti-{namespace}-{digest}"


def tool_activity_message(definition: ToolDefinition) -> str:
    """Describe a local tool event without copying model-controlled text."""

    return (
        "Inspecting Tender context."
        if definition.read_only
        else "Working on the Tender request."
    )
