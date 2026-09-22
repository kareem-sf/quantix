"""Safe error types for the bundled direct API adapter.

Provider SDK exceptions can contain request bodies, response bodies, keys, or
the prompt.  The direct runtime translates those exceptions at its boundary so
only a short, actionable class of failure can reach the application.
"""

from __future__ import annotations

import asyncio


class DirectAPIError(ValueError):
    """A sanitized, actionable direct API failure."""


class DirectDependencyError(DirectAPIError):
    """The bundled direct adapter is not installed at its required version."""


class DirectProviderError(ConnectionError):
    """A sanitized provider transport or API response failure."""


class DirectBudgetError(DirectAPIError):
    """A request was refused before provider I/O by the budget callback."""


_STATUS_DETAILS = {
    400: "The provider rejected the request settings. Check the model and supported options.",
    401: "The provider rejected the API key. Check the selected credentials.",
    402: "The AI account has run out of credit. Add credit with the provider, then continue.",
    403: "The provider denied this request. Check API permissions and model access.",
    404: "The provider could not find the endpoint or model. Check both values exactly.",
    413: "The request was too large for the provider. Continue to pick up with a smaller request.",
    429: "The provider rate or usage limit was reached. Wait or review the provider account limits.",
}

# Client errors a provider returns before running the model. Nothing is
# generated or billed, so a conservative budget reservation can be released.
# Timeouts, dropped connections and server errors stay uncertain because work
# may have started.
REJECTED_BEFORE_PROCESSING = frozenset({400, 401, 403, 404, 409, 413, 415, 422, 429})


def _status_detail(status: int) -> str:
    if status in _STATUS_DETAILS:
        return _STATUS_DETAILS[status]
    if 500 <= status <= 599:
        return "The provider service is unavailable. Retry after checking its status and limits."
    return f"The provider returned HTTP {status}. Check the credentials, endpoint, model and permissions."


# Run error texts that identify a pre-processing rejection in saved records.
REJECTION_DETAILS = tuple(sorted({_status_detail(status) for status in REJECTED_BEFORE_PROCESSING}))


def rejected_before_processing(error: BaseException | None) -> bool:
    """Whether a translated provider failure proves the request was not processed."""

    return (
        isinstance(error, DirectProviderError)
        and getattr(error, "rejected_before_processing", False) is True
    )


def _provider_message(error: BaseException) -> str:
    """The provider's own short explanation, for requests that carry no Tender data.

    Setup work sends a fixed sample and no document, so the provider can only be
    describing its own API. That sentence is usually the one thing that says how
    to fix the connection ("use /v1/responses", "add credit").
    """

    body = getattr(error, "body", None)
    message = body.get("message") if isinstance(body, dict) else None
    if isinstance(message, dict):
        message = message.get("message")
    if not isinstance(message, str):
        return ""
    text = " ".join(message.split())
    if len(text) > 300:
        text = text[:297].rstrip() + "…"
    return text


def provider_failure(error: BaseException, *, reveal: bool = False) -> DirectProviderError:
    """Return a useful provider error without retaining provider data.

    This intentionally does not inspect or interpolate ``str(error)``.  SDK
    exception text commonly includes the raw response body and sometimes the
    submitted request.  ``reveal`` is for setup only, where the request is a
    fixed sample with no Tender content and the provider's own sentence is safe
    to pass on.
    """

    # Streaming wraps the provider's own error in exception groups; classify the
    # error inside, or its status and kind are lost.
    said = _provider_message(error) if reveal else ""
    seen: set[int] = set()
    while isinstance(getattr(error, "exceptions", None), (list, tuple)) and id(error) not in seen:
        seen.add(id(error))
        members = [member for member in error.exceptions if isinstance(member, BaseException)]
        if not members:
            break
        error = members[0]

    status = getattr(error, "status_code", None)
    if not isinstance(status, int):
        response = getattr(error, "response", None)
        status = getattr(response, "status_code", None)
    if not isinstance(status, int):
        status = getattr(error, "code", None)
    safe_status = status if isinstance(status, int) and 100 <= status <= 599 else None
    said = said or (_provider_message(error) if reveal else "")

    def with_message(failure: DirectProviderError) -> DirectProviderError:
        if not said:
            return failure
        detailed = DirectProviderError(f"{failure} The provider said: {said}")
        detailed.__dict__.update(failure.__dict__)
        return detailed

    error_name = type(error).__name__.lower()
    if isinstance(error, TimeoutError) or error_name in {
        "apitimeouterror",
        "timeoutexception",
        "readtimeout",
        "connecttimeout",
    }:
        failure = DirectProviderError(
            "The provider request timed out. Check the endpoint, model and provider limits."
        )
        # A stalled reply is resent like a broken stream before the job stops.
        failure.timed_out = True
        if safe_status is not None:
            failure.status_code = safe_status
        return failure
    if isinstance(error, (ConnectionError, OSError)) or error_name in {
        "apiconnectionerror",
        "connecterror",
        "networkerror",
        "readerror",
        "writeerror",
    }:
        failure = DirectProviderError(
            "The provider connection failed. Check the endpoint, credentials and network access."
        )
        if safe_status is not None:
            failure.status_code = safe_status
        return failure
    if safe_status is not None:
        failure = DirectProviderError(_status_detail(safe_status))
        # A provider-side fault is often gone a moment later, so the step is resent.
        failure.transient = safe_status >= 500
        failure.status_code = safe_status
        failure.rejected_before_processing = safe_status in REJECTED_BEFORE_PROCESSING
        return with_message(failure)
    if error_name == "apierror":
        # The provider sent an error event after the reply had started
        # streaming. Keep only its short error code, never its message body.
        failure = DirectProviderError(
            "The provider stopped its reply with an error part-way through. This is usually "
            "temporary; press Resume to try again."
        )
        failure.provider_code = _provider_code(
            getattr(error, "body", None), getattr(error, "code", None)
        )
        return failure
    return with_message(
        DirectProviderError(
            "The provider rejected the request. Check the credentials, endpoint, model and permissions."
        )
    )


def _provider_code(body, code) -> str | None:
    values = [code]
    if isinstance(body, dict):
        values += [body.get("code"), body.get("type"), body.get("status")]
    for value in values:
        text = str(value) if isinstance(value, (str, int)) else ""
        if text and len(text) <= 64 and all(c.isalnum() or c in "_-." for c in text):
            return text
    return None


def provider_code(error: BaseException | None) -> str | None:
    """The first sanitized provider error code in an exception chain."""

    seen: set[int] = set()
    pending = [error]
    while pending:
        current = pending.pop()
        if current is None or id(current) in seen:
            continue
        seen.add(id(current))
        code = getattr(current, "provider_code", None)
        if code:
            return code
        pending.extend(getattr(current, "exceptions", None) or ())
        pending.extend((current.__cause__, current.__context__))
    return None


def preserve_control_failure(error: BaseException) -> BaseException | None:
    """Return failures whose application meaning must survive SDK translation."""

    if isinstance(error, asyncio.CancelledError):
        return error
    if isinstance(error, (DirectAPIError, DirectProviderError, InterruptedError)):
        return error
    return None


def dependency_detail(missing: list[str]) -> str:
    """Build an actionable status detail without exposing import internals."""

    names = ", ".join(missing)
    return f"The bundled direct API adapter is missing {names}. Repair the Quantix installation and retry."
