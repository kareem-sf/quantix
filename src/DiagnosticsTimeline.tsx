import {
  AlertTriangle,
  CheckCircle2,
  ChevronDown,
  Clock3,
  Download,
  ExternalLink,
  FileClock,
  LoaderCircle,
  Logs,
  RefreshCw,
  ShieldAlert,
  Square,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";

import type { DiagnosticTimelineEvent } from "./bindings/DiagnosticTimelineEvent";
import type { DiagnosticTimelinePage } from "./bindings/DiagnosticTimelinePage";
import type { DiagnosticComponent } from "./bindings/DiagnosticComponent";
import type { DiagnosticsStatus } from "./bindings/DiagnosticsStatus";
import type { ExportDiagnosticsSupportBundleCommand } from "./bindings/ExportDiagnosticsSupportBundleCommand";
import type { InspectDiagnosticTimelineCommand } from "./bindings/InspectDiagnosticTimelineCommand";
import type { InspectDiagnosticsStatusCommand } from "./bindings/InspectDiagnosticsStatusCommand";
import type { OpenDiagnosticLogsCommand } from "./bindings/OpenDiagnosticLogsCommand";
import type { StartTenderDeepDiagnosticsCommand } from "./bindings/StartTenderDeepDiagnosticsCommand";
import type { StopTenderDeepDiagnosticsCommand } from "./bindings/StopTenderDeepDiagnosticsCommand";
import {
  exportDiagnosticsSupportBundle,
  inspectDiagnosticTimeline,
  inspectDiagnosticsStatus,
  openDiagnosticLogs,
  startTenderDeepDiagnostics,
  stopTenderDeepDiagnostics,
} from "./quantixHost";
import "./DiagnosticsTimeline.css";

export type DiagnosticsTimelineStatus = DiagnosticsStatus;
export type DiagnosticsTimelineEvent = DiagnosticTimelineEvent;
export type DiagnosticsTimelinePage = DiagnosticTimelinePage;
export type DiagnosticsScope = InspectDiagnosticsStatusCommand["scope"];
export type DiagnosticSeverity = NonNullable<
  InspectDiagnosticTimelineCommand["severity"]
>;

export interface DiagnosticsTimelineHost {
  inspectDiagnosticsStatus: typeof inspectDiagnosticsStatus;
  inspectDiagnosticTimeline: typeof inspectDiagnosticTimeline;
  startTenderDeepDiagnostics: typeof startTenderDeepDiagnostics;
  stopTenderDeepDiagnostics: typeof stopTenderDeepDiagnostics;
  openDiagnosticLogs: typeof openDiagnosticLogs;
  exportDiagnosticsSupportBundle: typeof exportDiagnosticsSupportBundle;
}

const defaultHost: DiagnosticsTimelineHost = {
  inspectDiagnosticsStatus,
  inspectDiagnosticTimeline,
  startTenderDeepDiagnostics,
  stopTenderDeepDiagnostics,
  openDiagnosticLogs,
  exportDiagnosticsSupportBundle,
};

function errorMessage(reason: unknown): string {
  return reason instanceof Error
    ? reason.message
    : "Diagnostics could not be loaded. Try again.";
}

function formatTime(valueToFormat: string | null | undefined): string {
  if (!valueToFormat) return "Unknown time";
  const date = new Date(valueToFormat);
  return Number.isNaN(date.valueOf()) ? valueToFormat : date.toLocaleString();
}

function formatBytes(value: bigint | number): string {
  const bytes = Number(value);
  if (!Number.isFinite(bytes) || bytes <= 0) return "0 B";
  const units = ["B", "KiB", "MiB", "GiB"];
  const index = Math.min(
    Math.floor(Math.log(bytes) / Math.log(1024)),
    units.length - 1,
  );
  return `${(bytes / 1024 ** index).toFixed(index === 0 ? 0 : 1)} ${units[index]}`;
}

function deepRemaining(
  status: DiagnosticsTimelineStatus | null,
  now = Date.now(),
): number {
  if (!status || status.deep.state !== "running") return 0;
  if (
    status.deep.remaining_seconds !== null &&
    status.deep.remaining_seconds !== undefined
  ) {
    return Math.max(0, Math.ceil(Number(status.deep.remaining_seconds)));
  }
  if (!status.deep.ends_at) return 0;
  return Math.max(
    0,
    Math.ceil((new Date(status.deep.ends_at).valueOf() - now) / 1000),
  );
}

export interface DiagnosticsTimelineProps {
  open: boolean;
  selectedTenderId?: string | null;
  selectedTenderName?: string | null;
  host?: DiagnosticsTimelineHost;
}

export function DiagnosticsTimeline({
  open,
  selectedTenderId = null,
  selectedTenderName = null,
  host = defaultHost,
}: DiagnosticsTimelineProps) {
  const [scope, setScope] = useState<DiagnosticsScope>(
    selectedTenderId ? "tender" : "application",
  );
  const [severity, setSeverity] = useState<DiagnosticSeverity | "all">("all");
  const [component, setComponent] = useState<DiagnosticComponent | "all">(
    "all",
  );
  const [status, setStatus] = useState<DiagnosticsTimelineStatus | null>(null);
  const [events, setEvents] = useState<DiagnosticTimelineEvent[]>([]);
  const [nextCursor, setNextCursor] = useState<string | null>(null);
  const [hasMore, setHasMore] = useState(false);
  const [busy, setBusy] = useState(false);
  const [timelineBusy, setTimelineBusy] = useState(false);
  const [deepConfirmOpen, setDeepConfirmOpen] = useState(false);
  const [exportConfirmOpen, setExportConfirmOpen] = useState(false);
  const [sensitiveExportConfirmOpen, setSensitiveExportConfirmOpen] =
    useState(false);
  const [sensitiveExportAcknowledged, setSensitiveExportAcknowledged] =
    useState(false);
  const [includeDeep, setIncludeDeep] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [clock, setClock] = useState(() => Date.now());
  const currentTenderId = scope === "tender" ? selectedTenderId : null;
  const currentTenderName = scope === "tender" ? selectedTenderName : null;

  useEffect(() => {
    if (!selectedTenderId && scope === "tender") setScope("application");
  }, [scope, selectedTenderId]);

  const loadStatus = useCallback(async () => {
    try {
      const command: InspectDiagnosticsStatusCommand = {
        scope,
        tender_id: currentTenderId,
      };
      setStatus(await host.inspectDiagnosticsStatus(command));
      setError(null);
    } catch (reason) {
      setError(errorMessage(reason));
    }
  }, [currentTenderId, host, scope]);

  const loadTimeline = useCallback(
    async (cursor: string | null, append: boolean) => {
      setTimelineBusy(true);
      try {
        const command: InspectDiagnosticTimelineCommand = {
          scope,
          tender_id: currentTenderId,
          cursor,
          limit: 25,
          severity: severity === "all" ? null : severity,
          component: component === "all" ? null : component,
        };
        const page = await host.inspectDiagnosticTimeline(command);
        setEvents((previous) =>
          append ? [...previous, ...page.events] : page.events,
        );
        setNextCursor(page.next_cursor);
        setHasMore(page.has_more);
        setError(null);
      } catch (reason) {
        setError(errorMessage(reason));
      } finally {
        setTimelineBusy(false);
      }
    },
    [component, currentTenderId, host, scope, severity],
  );

  useEffect(() => {
    if (!open) return;
    setEvents([]);
    setNextCursor(null);
    setHasMore(false);
    void Promise.all([loadStatus(), loadTimeline(null, false)]);
    const timer = window.setInterval(() => void loadStatus(), 10_000);
    return () => window.clearInterval(timer);
  }, [loadStatus, loadTimeline, open]);

  useEffect(() => {
    if (!open || status?.deep.state !== "running") return;
    const timer = window.setInterval(() => setClock(Date.now()), 1000);
    return () => window.clearInterval(timer);
  }, [open, status?.deep.state]);

  const components = useMemo(() => {
    const available = new Set(
      events.map((event) => event.component).filter(Boolean),
    );
    if (status?.component) available.add(status.component);
    return [...available].sort((left, right) => left.localeCompare(right));
  }, [events, status?.component]);

  const runDeep = async () => {
    if (!currentTenderId) return;
    setBusy(true);
    setError(null);
    try {
      const command: StartTenderDeepDiagnosticsCommand = {
        tender_id: currentTenderId,
        policy_revision: 1,
      };
      await host.startTenderDeepDiagnostics(command);
      setDeepConfirmOpen(false);
      setMessage("Deep diagnostics started. It will run for up to 60 minutes.");
      await loadStatus();
    } catch (reason) {
      setError(errorMessage(reason));
    } finally {
      setBusy(false);
    }
  };

  const stopDeep = async () => {
    if (!currentTenderId || !status?.deep.session_id) return;
    setBusy(true);
    try {
      const command: StopTenderDeepDiagnosticsCommand = {
        tender_id: currentTenderId,
        session_id: status.deep.session_id,
      };
      await host.stopTenderDeepDiagnostics(command);
      setMessage("Deep diagnostics stopped.");
      await loadStatus();
    } catch (reason) {
      setError(errorMessage(reason));
    } finally {
      setBusy(false);
    }
  };

  const openLogs = async () => {
    setBusy(true);
    try {
      const command: OpenDiagnosticLogsCommand = {
        scope,
        tender_id: currentTenderId,
      };
      await host.openDiagnosticLogs(command);
      setMessage("Diagnostic logs opened.");
    } catch (reason) {
      setError(errorMessage(reason));
    } finally {
      setBusy(false);
    }
  };

  const exportBundle = async () => {
    setBusy(true);
    try {
      const command: ExportDiagnosticsSupportBundleCommand = {
        scope,
        tender_id: currentTenderId,
        include_deep: includeDeep,
        policy_revision: 1,
      };
      await host.exportDiagnosticsSupportBundle(command);
      setExportConfirmOpen(false);
      setSensitiveExportConfirmOpen(false);
      setSensitiveExportAcknowledged(false);
      setIncludeDeep(false);
      setMessage("Support bundle exported.");
    } catch (reason) {
      setError(errorMessage(reason));
    } finally {
      setBusy(false);
    }
  };

  const confirmExportSelection = () => {
    if (includeDeep) {
      setExportConfirmOpen(false);
      setSensitiveExportAcknowledged(false);
      setSensitiveExportConfirmOpen(true);
      return;
    }
    void exportBundle();
  };

  if (!open) return null;

  const remaining = deepRemaining(status, clock);
  const deepRunning = status?.deep.state === "running";
  const scopeLabel =
    scope === "tender"
      ? currentTenderName || currentTenderId || "Selected Tender"
      : "Application";

  return (
    <section
      className="diagnostics-timeline"
      aria-labelledby="diagnostics-timeline-title"
    >
      <div className="diagnostics-timeline__heading">
        <div>
          <h3 id="diagnostics-timeline-title">Diagnostics timeline</h3>
          <p>
            Review retained local health events for the application or selected
            Tender. Deep runs collect redacted protocol and process evidence;
            they do not scan Tender content.
          </p>
        </div>
        <span
          className={`diagnostics-timeline__state diagnostics-timeline__state--${status?.state ?? "inspecting"}`}
          role="status"
        >
          {status?.state === "healthy" ? (
            <CheckCircle2 size={15} aria-hidden="true" />
          ) : (
            <ShieldAlert size={15} aria-hidden="true" />
          )}
          {status?.state === "healthy"
            ? "Healthy"
            : status?.state === "degraded"
              ? "Degraded"
              : status?.state === "disabled"
                ? "Unavailable"
                : "Inspecting…"}
        </span>
      </div>

      <div className="diagnostics-timeline__toolbar">
        <div
          className="diagnostics-timeline__scope"
          aria-label="Diagnostics scope"
        >
          <button
            type="button"
            aria-pressed={scope === "application"}
            onClick={() => setScope("application")}
          >
            Application
          </button>
          <button
            type="button"
            aria-pressed={scope === "tender"}
            disabled={!selectedTenderId}
            onClick={() => setScope("tender")}
          >
            Tender{currentTenderName ? ` · ${currentTenderName}` : ""}
          </button>
        </div>
        <label>
          <span>Severity</span>
          <select
            aria-label="Filter diagnostics by severity"
            value={severity}
            onChange={(event) =>
              setSeverity(event.target.value as DiagnosticSeverity | "all")
            }
          >
            <option value="all">All severities</option>
            <option value="critical">Critical</option>
            <option value="error">Error</option>
            <option value="warning">Warning</option>
            <option value="info">Info</option>
            <option value="debug">Debug</option>
          </select>
        </label>
        <label>
          <span>Component</span>
          <select
            aria-label="Filter diagnostics by component"
            value={component}
            onChange={(event) =>
              setComponent(event.target.value as DiagnosticComponent | "all")
            }
          >
            <option value="all">All components</option>
            {components.map((item) => (
              <option key={item} value={item}>
                {item}
              </option>
            ))}
          </select>
        </label>
        <button
          type="button"
          className="diagnostics-timeline__icon-button"
          disabled={busy || timelineBusy}
          onClick={() =>
            void Promise.all([loadStatus(), loadTimeline(null, false)])
          }
          aria-label="Refresh diagnostics"
        >
          <RefreshCw
            size={15}
            className={busy || timelineBusy ? "is-spinning" : undefined}
            aria-hidden="true"
          />
          Refresh
        </button>
      </div>

      {status ? (
        <div className="diagnostics-timeline__summary">
          <div>
            <span>Scope</span>
            <strong>{scopeLabel}</strong>
          </div>
          <div>
            <span>Retention</span>
            <strong>
              {status.retention_days === null ||
              status.retention_days === undefined
                ? "Not reported"
                : `${status.retention_days} days`}
            </strong>
          </div>
          <div>
            <span>Events retained</span>
            <strong>{status.retained_event_count ?? "Not reported"}</strong>
          </div>
          <div>
            <span>Storage used</span>
            <strong>
              {formatBytes(status.retained_bytes)} /{" "}
              {formatBytes(status.retention_limit_bytes)}
            </strong>
          </div>
          <div>
            <span>Dropped</span>
            <strong>{status.dropped_event_count ?? 0}</strong>
          </div>
        </div>
      ) : null}

      {status?.state === "degraded" ||
      status?.state === "disabled" ||
      status?.degraded_reason ? (
        <div
          className="diagnostics-timeline__notice diagnostics-timeline__notice--warning"
          role="alert"
        >
          <AlertTriangle size={17} aria-hidden="true" />
          <span>
            {status.degraded_reason ||
              "Diagnostics are degraded. Some events may be missing."}
          </span>
        </div>
      ) : null}

      {message ? (
        <p className="diagnostics-timeline__message" role="status">
          {message}
        </p>
      ) : null}
      {error ? (
        <p className="diagnostics-timeline__error" role="alert">
          {error}
        </p>
      ) : null}

      <div className="diagnostics-timeline__actions">
        {scope === "tender" ? (
          deepRunning ? (
            <button
              type="button"
              className="diagnostics-timeline__danger"
              disabled={busy}
              onClick={() => void stopDeep()}
            >
              <Square size={15} aria-hidden="true" /> Stop deep diagnostics
            </button>
          ) : (
            <button
              type="button"
              disabled={busy}
              onClick={() => setDeepConfirmOpen(true)}
            >
              <FileClock size={15} aria-hidden="true" /> Run deep diagnostics
            </button>
          )
        ) : null}
        <button type="button" disabled={busy} onClick={() => void openLogs()}>
          <ExternalLink size={15} aria-hidden="true" /> Open logs
        </button>
        <button
          type="button"
          disabled={busy}
          onClick={() => setExportConfirmOpen(true)}
        >
          <Download size={15} aria-hidden="true" /> Export support bundle
        </button>
      </div>

      {deepRunning ? (
        <div className="diagnostics-timeline__deep" aria-live="polite">
          <Clock3 size={17} aria-hidden="true" />
          <div>
            <strong>Deep diagnostics running</strong>
            <span>
              {remaining
                ? `${Math.floor(remaining / 60)}m ${remaining % 60}s remaining`
                : "Up to 60 minutes remaining"}
            </span>
          </div>
        </div>
      ) : null}

      <div className="diagnostics-timeline__events" aria-live="polite">
        {timelineBusy && !events.length ? (
          <p className="diagnostics-timeline__empty">
            <LoaderCircle className="is-spinning" size={17} /> Loading timeline…
          </p>
        ) : events.length ? (
          events.map((event) => (
            <article
              key={event.event_id}
              className="diagnostics-timeline__event"
              data-severity={event.severity}
            >
              <span
                className="diagnostics-timeline__event-marker"
                aria-hidden="true"
              />
              <div>
                <div className="diagnostics-timeline__event-meta">
                  <strong>{event.title}</strong>
                  <time dateTime={event.occurred_at}>
                    {formatTime(event.occurred_at)}
                  </time>
                </div>
                <div className="diagnostics-timeline__event-tags">
                  <span>{event.component}</span>
                  <span>{event.severity}</span>
                </div>
                {event.operation_id ||
                event.parent_operation_id ||
                event.outcome ||
                event.error_code ? (
                  <dl className="diagnostics-timeline__event-correlation">
                    {event.operation_id ? (
                      <div>
                        <dt>Operation</dt>
                        <dd>{event.operation_id}</dd>
                      </div>
                    ) : null}
                    {event.parent_operation_id ? (
                      <div>
                        <dt>Parent</dt>
                        <dd>{event.parent_operation_id}</dd>
                      </div>
                    ) : null}
                    {event.outcome ? (
                      <div>
                        <dt>Outcome</dt>
                        <dd>{event.outcome}</dd>
                      </div>
                    ) : null}
                    {event.error_code ? (
                      <div>
                        <dt>Error</dt>
                        <dd>{event.error_code}</dd>
                      </div>
                    ) : null}
                  </dl>
                ) : null}
                {event.detail ? <p>{event.detail}</p> : null}
              </div>
            </article>
          ))
        ) : (
          <p className="diagnostics-timeline__empty">
            <Logs size={17} /> No retained diagnostic events for this scope.
          </p>
        )}
      </div>

      {hasMore && nextCursor ? (
        <button
          type="button"
          className="diagnostics-timeline__load-more"
          disabled={timelineBusy}
          onClick={() => void loadTimeline(nextCursor, true)}
        >
          {timelineBusy ? "Loading…" : "Load older events"}
          <ChevronDown size={15} aria-hidden="true" />
        </button>
      ) : null}

      <small className="diagnostics-timeline__footnote">
        Newest events appear first.{" "}
        {status?.checked_at
          ? `Last checked ${formatTime(status.checked_at)}.`
          : "Status has not been checked yet."}
      </small>

      {deepConfirmOpen ? (
        <div className="diagnostics-timeline__dialog-backdrop">
          <section
            className="diagnostics-timeline__dialog"
            role="dialog"
            aria-modal="true"
            aria-labelledby="deep-dialog-title"
          >
            <h4 id="deep-dialog-title">Run deep diagnostics?</h4>
            <p>
              This collects redacted protocol and process evidence for up to 60
              minutes and may use additional CPU. It does not scan Tender
              content or change Tender records.
            </p>
            <div>
              <button type="button" onClick={() => setDeepConfirmOpen(false)}>
                Cancel
              </button>
              <button
                type="button"
                disabled={busy}
                onClick={() => void runDeep()}
              >
                Start deep diagnostics
              </button>
            </div>
          </section>
        </div>
      ) : null}
      {exportConfirmOpen ? (
        <div className="diagnostics-timeline__dialog-backdrop">
          <section
            className="diagnostics-timeline__dialog"
            role="dialog"
            aria-modal="true"
            aria-labelledby="export-dialog-title"
          >
            <h4 id="export-dialog-title">Export diagnostic support bundle?</h4>
            <p>
              The bundle contains redacted local diagnostics and timeline events
              for {scopeLabel}.
            </p>
            <label>
              <input
                type="checkbox"
                checked={includeDeep}
                onChange={(event) => setIncludeDeep(event.target.checked)}
              />{" "}
              Include deep-diagnostics data <strong>(sensitive)</strong>
            </label>
            {includeDeep ? (
              <p className="diagnostics-timeline__dialog-warning">
                This adds redacted protocol/process evidence and may be large.
              </p>
            ) : null}
            <div>
              <button
                type="button"
                onClick={() => {
                  setExportConfirmOpen(false);
                  setIncludeDeep(false);
                }}
              >
                Cancel
              </button>
              <button
                type="button"
                disabled={busy}
                onClick={confirmExportSelection}
              >
                {includeDeep ? "Continue to sensitive export" : "Export bundle"}
              </button>
            </div>
          </section>
        </div>
      ) : null}
      {sensitiveExportConfirmOpen ? (
        <div className="diagnostics-timeline__dialog-backdrop">
          <section
            className="diagnostics-timeline__dialog"
            role="dialog"
            aria-modal="true"
            aria-labelledby="sensitive-export-dialog-title"
          >
            <h4 id="sensitive-export-dialog-title">
              Confirm sensitive deep-event export
            </h4>
            <p>
              The bundle will include retained deep-diagnostics events with
              redacted protocol and process evidence. Review the bundle before
              sharing it outside your organization.
            </p>
            <label>
              <input
                type="checkbox"
                checked={sensitiveExportAcknowledged}
                onChange={(event) =>
                  setSensitiveExportAcknowledged(event.target.checked)
                }
              />{" "}
              I understand this bundle contains sensitive diagnostic evidence.
            </label>
            <div>
              <button
                type="button"
                onClick={() => {
                  setSensitiveExportConfirmOpen(false);
                  setSensitiveExportAcknowledged(false);
                  setIncludeDeep(false);
                }}
              >
                Cancel
              </button>
              <button
                type="button"
                disabled={busy || !sensitiveExportAcknowledged}
                onClick={() => void exportBundle()}
              >
                Confirm and export
              </button>
            </div>
          </section>
        </div>
      ) : null}
    </section>
  );
}
