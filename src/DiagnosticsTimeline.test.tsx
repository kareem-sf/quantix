import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
  DiagnosticsTimeline,
  type DiagnosticsTimelineHost,
  type DiagnosticsTimelinePage,
  type DiagnosticsTimelineStatus,
} from "./DiagnosticsTimeline";

function status(
  overrides: Partial<DiagnosticsTimelineStatus> = {},
): DiagnosticsTimelineStatus {
  return {
    scope: "application",
    tender_id: null,
    state: "healthy",
    retention_days: 14,
    retained_bytes: 1024n,
    retention_limit_bytes: 5n * 1024n * 1024n * 1024n,
    retained_event_count: 2,
    dropped_event_count: 0,
    degraded_reason: null,
    checked_at: "2026-08-21T10:00:00Z",
    deep: {
      state: "idle",
      session_id: null,
      started_at: null,
      ends_at: null,
      remaining_seconds: null,
      detail: null,
    },
    component: null,
    ...overrides,
  };
}

function page(): DiagnosticsTimelinePage {
  return {
    events: [
      {
        event_id: "event-1",
        occurred_at: "2026-08-21T09:59:00Z",
        severity: "warning",
        component: "Runtime",
        title: "Runtime check completed",
        detail: "The managed document tools are ready.",
        scope: "application",
        operation_id: "operation-1",
        parent_operation_id: "operation-parent",
        outcome: "passed",
        error_code: "none",
      },
    ],
    next_cursor: null,
    has_more: false,
    snapshot: "2026-08-21T10:00:00Z",
  };
}

function makeHost(): DiagnosticsTimelineHost {
  return {
    inspectDiagnosticsStatus: vi.fn().mockResolvedValue(status()),
    inspectDiagnosticTimeline: vi.fn().mockResolvedValue(page()),
    startTenderDeepDiagnostics: vi.fn().mockResolvedValue({}),
    stopTenderDeepDiagnostics: vi.fn().mockResolvedValue({}),
    openDiagnosticLogs: vi.fn().mockResolvedValue({}),
    exportDiagnosticsSupportBundle: vi.fn().mockResolvedValue("support.zip"),
  };
}

describe("DiagnosticsTimeline", () => {
  beforeEach(() => vi.useRealTimers());
  afterEach(() => {
    cleanup();
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  it("loads application diagnostics and filters a newest-first timeline", async () => {
    const host = makeHost();
    render(
      <DiagnosticsTimeline open host={host} selectedTenderId="tender-1" />,
    );

    expect(await screen.findByText("Runtime check completed")).toBeTruthy();
    expect(screen.getByText("operation-1")).toBeTruthy();
    expect(screen.getByText("operation-parent")).toBeTruthy();
    expect(host.inspectDiagnosticsStatus).toHaveBeenCalledWith({
      scope: "tender",
      tender_id: "tender-1",
    });
    expect(host.inspectDiagnosticTimeline).toHaveBeenCalledWith({
      scope: "tender",
      tender_id: "tender-1",
      cursor: null,
      limit: 25,
      severity: null,
      component: null,
    });

    fireEvent.change(
      screen.getByRole("combobox", { name: "Filter diagnostics by severity" }),
      {
        target: { value: "debug" },
      },
    );
    await waitFor(() => {
      expect(host.inspectDiagnosticTimeline).toHaveBeenLastCalledWith({
        scope: "tender",
        tender_id: "tender-1",
        cursor: null,
        limit: 25,
        severity: "debug",
        component: null,
      });
    });
  });

  it("supports Tender deep diagnostics, stop, logs, and confirmed export", async () => {
    const host = makeHost();
    let statusCalls = 0;
    vi.mocked(host.inspectDiagnosticsStatus).mockImplementation(async () => {
      statusCalls += 1;
      return status({
        scope: "tender",
        deep:
          statusCalls >= 2
            ? {
                state: "running",
                session_id: "deep-session-1",
                started_at: "2026-08-21T10:00:00Z",
                ends_at: "2026-08-21T11:00:00Z",
                remaining_seconds: 3599n,
                detail: "Redacted protocol diagnostics are active.",
              }
            : {
                state: "idle",
                session_id: null,
                started_at: null,
                ends_at: null,
                remaining_seconds: null,
                detail: null,
              },
      });
    });
    render(
      <DiagnosticsTimeline
        open
        host={host}
        selectedTenderId="tender-1"
        selectedTenderName="Juhayna"
      />,
    );

    fireEvent.click(
      await screen.findByRole("button", { name: "Tender · Juhayna" }),
    );
    await waitFor(() => {
      expect(host.inspectDiagnosticsStatus).toHaveBeenLastCalledWith({
        scope: "tender",
        tender_id: "tender-1",
      });
    });
    fireEvent.click(
      await screen.findByRole("button", { name: "Run deep diagnostics" }),
    );
    expect(
      screen.getByRole("dialog", { name: "Run deep diagnostics?" }),
    ).toBeTruthy();
    fireEvent.click(
      screen.getByRole("button", { name: "Start deep diagnostics" }),
    );
    await waitFor(() =>
      expect(host.startTenderDeepDiagnostics).toHaveBeenCalledWith({
        tender_id: "tender-1",
        policy_revision: 1,
      }),
    );
    expect(
      await screen.findByRole("button", { name: "Stop deep diagnostics" }),
    ).toBeTruthy();
    fireEvent.click(
      screen.getByRole("button", { name: "Stop deep diagnostics" }),
    );
    await waitFor(() =>
      expect(host.stopTenderDeepDiagnostics).toHaveBeenCalledWith({
        tender_id: "tender-1",
        session_id: expect.any(String),
      }),
    );

    fireEvent.click(
      screen.getByRole("button", { name: "Export support bundle" }),
    );
    expect(
      screen.getByRole("dialog", { name: "Export diagnostic support bundle?" }),
    ).toBeTruthy();
    fireEvent.click(
      screen.getByLabelText(/Include deep-diagnostics data \(sensitive\)/),
    );
    fireEvent.click(
      screen.getByRole("button", { name: "Continue to sensitive export" }),
    );
    expect(
      screen.getByRole("dialog", {
        name: "Confirm sensitive deep-event export",
      }),
    ).toBeTruthy();
    const sensitiveAcknowledgement = screen.getByLabelText(
      "I understand this bundle contains sensitive diagnostic evidence.",
    );
    expect(sensitiveAcknowledgement).toBeTruthy();
    fireEvent.click(sensitiveAcknowledgement);
    fireEvent.click(screen.getByRole("button", { name: "Confirm and export" }));
    await waitFor(() =>
      expect(host.exportDiagnosticsSupportBundle).toHaveBeenCalledWith({
        scope: "tender",
        tender_id: "tender-1",
        include_deep: true,
        policy_revision: 1,
      }),
    );

    fireEvent.click(screen.getByRole("button", { name: "Open logs" }));
    await waitFor(() =>
      expect(host.openDiagnosticLogs).toHaveBeenCalledWith({
        scope: "tender",
        tender_id: "tender-1",
      }),
    );
  });

  it("polls only while the diagnostics surface is open", async () => {
    vi.useFakeTimers();
    const host = makeHost();
    const { rerender } = render(<DiagnosticsTimeline open host={host} />);
    await act(async () => {
      await Promise.resolve();
    });
    const initialTimelineCalls = vi.mocked(host.inspectDiagnosticTimeline).mock
      .calls.length;
    expect(vi.getTimerCount()).toBe(1);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(10_000);
    });
    expect(
      vi.mocked(host.inspectDiagnosticsStatus).mock.calls.length,
    ).toBeGreaterThan(1);
    expect(vi.mocked(host.inspectDiagnosticTimeline).mock.calls.length).toBe(
      initialTimelineCalls,
    );
    rerender(<DiagnosticsTimeline open={false} host={host} />);
    expect(vi.getTimerCount()).toBe(0);
  });
});
