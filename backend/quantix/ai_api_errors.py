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
    403: "The provider denied this request. Check API permissions and model access.",
    404: "The provider could not find the endpoint or model. Check both values exactly.",
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


def provider_failure(error: BaseException) -> DirectProviderError:
    """Return a useful provider error without retaining provider data.

    This intentionally does not inspect or interpolate ``str(error)``.  SDK
    exception text commonly includes the raw response body and sometimes the
    submitted request.
    """

    status = getattr(error, "status_code", None)
    if not isinstance(status, int):
        response = getattr(error, "response", None)
        status = getattr(response, "status_code", None)
    if not isinstance(status, int):
        status = getattr(error, "code", None)
    safe_status = status if isinstance(status, int) and 100 <= status <= 599 else None
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
        failure.status_code = safe_status
        failure.rejected_before_processing = safe_status in REJECTED_BEFORE_PROCESSING
        return failure
    return DirectProviderError(
        "The provider rejected the request. Check the credentials, endpoint, model and permissions."
    )


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
