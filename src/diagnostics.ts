import type { Api, Schema } from "./api";

type DiagnosticEvent = Schema<"DiagnosticEventInput">;
type DiagnosticEventName = DiagnosticEvent["event"];
type DiagnosticErrorType = NonNullable<DiagnosticEvent["error_type"]>;
type DiagnosticSource = NonNullable<DiagnosticEvent["source"]>;

const errorTypes = new Set<DiagnosticErrorType>([
  "Error",
  "TypeError",
  "RangeError",
  "ReferenceError",
  "SyntaxError",
  "URIError",
  "EvalError",
  "AggregateError",
  "Unknown",
]);
const requestIdPattern = /^[0-9a-f]{32}$/;
const maxLocation = 1_000_000;
const maxBufferedEvents = 20;
const maxEventsPerMinute = 60;

let installed = false;
let diagnosticApi: Api | null = null;
let buffered: DiagnosticEvent[] = [];
let sending = false;
let windowStartedAt = 0;
let sentInWindow = 0;

function safeProperty(value: unknown, key: string): unknown {
  try {
    if (
      value === null ||
      (typeof value !== "object" && typeof value !== "function")
    )
      return undefined;
    return Reflect.get(value, key);
  } catch {
    return undefined;
  }
}

function safeErrorType(error: unknown): DiagnosticErrorType {
  const name = safeProperty(error, "name");
  return typeof name === "string" && errorTypes.has(name as DiagnosticErrorType)
    ? (name as DiagnosticErrorType)
    : "Unknown";
}

function safeRequestId(error: unknown) {
  const value = safeProperty(error, "requestId");
  return typeof value === "string" && requestIdPattern.test(value)
    ? value
    : undefined;
}

function safeLocation(error: unknown) {
  const stack = safeProperty(error, "stack");
  if (typeof stack !== "string") return {};
  // Keep only the final numeric frame coordinates. The stack text itself is
  // never sent to the service.
  const match = /:(\d+)(?::(\d+))?(?:\)?\s*)$/.exec(stack.trim());
  if (!match) return {};
  const line = Number(match[1]);
  const column = match[2] === undefined ? undefined : Number(match[2]);
  return {
    ...(Number.isSafeInteger(line) && line >= 0 && line <= maxLocation
      ? { line }
      : {}),
    ...(column !== undefined &&
    Number.isSafeInteger(column) &&
    column >= 0 &&
    column <= maxLocation
      ? { column }
      : {}),
  };
}

function makeEvent(
  event: DiagnosticEventName,
  error: unknown,
  source: DiagnosticSource,
): DiagnosticEvent {
  try {
    const location = safeLocation(error);
    const requestId = safeRequestId(error);
    return {
      event,
      error_type: safeErrorType(error),
      source,
      line: location.line ?? null,
      column: location.column ?? null,
      request_id: requestId ?? null,
    };
  } catch {
    return {
      event,
      error_type: "Unknown",
      source,
      line: null,
      column: null,
      request_id: null,
    };
  }
}

function canSend() {
  const now = Date.now();
  if (now - windowStartedAt >= 60_000) {
    windowStartedAt = now;
    sentInWindow = 0;
  }
  if (sentInWindow >= maxEventsPerMinute) return false;
  sentInWindow += 1;
  return true;
}

function flush() {
  try {
    if (!diagnosticApi || sending || buffered.length === 0) return;
    sending = true;
    const event = buffered.shift()!;
    if (!canSend()) {
      buffered = [];
      sending = false;
      return;
    }
    const api = diagnosticApi;
    const controller = new AbortController();
    const timeout = window.setTimeout(() => {
      try {
        controller.abort();
      } catch {
        /* A timeout cleanup failure must not escape the reporter. */
      }
    }, 4_000);
    // This promise is deliberately isolated. A diagnostics failure must not
    // become another unhandled rejection or trigger recursive reporting.
    void api
      .post<Schema<"DiagnosticEventAck">>(
        "/diagnostics/events",
        event,
        controller.signal,
      )
      .catch(() => undefined)
      .finally(() => {
        try {
          window.clearTimeout(timeout);
        } catch {
          /* A cleanup failure must not escape the reporter. */
        }
        sending = false;
        try {
          flush();
        } catch {
          /* Keep diagnostics best effort if the sink changed during send. */
        }
      });
  } catch {
    // A malformed/revoked sink or unexpected runtime capability must never
    // turn failure reporting into another renderer failure.
    sending = false;
  }
}

function enqueue(event: DiagnosticEvent) {
  try {
    if (buffered.length >= maxBufferedEvents) buffered.shift();
    buffered.push(event);
    flush();
  } catch {
    /* Diagnostics are always non-fatal. */
  }
}

function reportEvent(create: () => DiagnosticEvent) {
  try {
    enqueue(create());
  } catch {
    /* Error normalization and reporting must never throw. */
  }
}

export function configureDiagnosticApi(api: Api | null) {
  diagnosticApi = api;
  flush();
}

export function reportRendererError(error: unknown) {
  reportEvent(() => makeEvent("renderer_error", error, "renderer"));
}

export function reportReactBoundary(error: unknown) {
  reportEvent(() => makeEvent("react_boundary", error, "app"));
}

export function notifyApiError(error: unknown, path?: string) {
  try {
    if (path === "/diagnostics/events") return;
    if (safeProperty(error, "name") === "AbortError") return;
    reportEvent(() => makeEvent("api_network_error", error, "api"));
  } catch {
    /* Error normalization and reporting must never throw. */
  }
}

function handleWindowError(event: ErrorEvent) {
  reportRendererError(event.error);
}

function handleUnhandledRejection(event: PromiseRejectionEvent) {
  reportEvent(() => makeEvent("unhandled_rejection", event.reason, "renderer"));
}

export function installRendererDiagnostics() {
  if (installed || typeof window === "undefined") return;
  installed = true;
  window.addEventListener("error", handleWindowError);
  window.addEventListener("unhandledrejection", handleUnhandledRejection);
}
