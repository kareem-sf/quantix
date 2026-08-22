import { act, cleanup, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const host = vi.hoisted(() => ({
  ensureQuantixSetup: vi.fn(),
  inspectApplicationSettings: vi.fn(),
  inspectManagerWorkspace: vi.fn(),
  validateQuantixUpdateRestart: vi.fn(),
}));
const tauri = vi.hoisted(() => ({ invoke: vi.fn() }));

vi.mock("./quantixHost", () => host);
vi.mock("@tauri-apps/api/core", () => tauri);
vi.mock("./ManagerWorkspace", () => ({
  ManagerWorkspace: ({
    initialPreferences,
    setupWarnings,
  }: {
    initialPreferences: { reduced_motion: boolean };
    setupWarnings: string[];
  }) => (
    <main data-testid="manager-workspace">
      <h1>Tender workspace</h1>
      <span data-testid="motion-preference">
        {initialPreferences.reduced_motion ? "reduced" : "full"}
      </span>
      {setupWarnings.length ? <p>{setupWarnings.join(", ")}</p> : null}
    </main>
  ),
}));

import App from "./App";

const projection = {
  catalogue: [],
  selected_tender: null,
  conversation: null,
  current_action: {
    kind: "start_tender" as const,
    title: "Start a Tender",
    summary: "Choose a Tender Package.",
    action_label: "Choose Tender Package",
    requires_engineer: true,
  },
  work: {
    needs_engineer: 0,
    working: 0,
    waiting: 0,
    done: 0,
    cancelled: 0,
    failed: 0,
  },
  files: {
    tender_document_count: 0,
    quantix_output_count: 0,
    tender_documents: [],
  },
  team: { active_agent_runs: 0, waiting_tasks: 0, needs_engineer: 0 },
  intake: null,
};

describe("App bootstrap", () => {
  beforeEach(() => {
    host.ensureQuantixSetup.mockResolvedValue({
      state: "ready",
      issues: [],
    });
    host.validateQuantixUpdateRestart.mockResolvedValue({ state: "idle" });
    host.inspectManagerWorkspace.mockResolvedValue(projection);
    host.inspectApplicationSettings.mockResolvedValue({
      general_preferences: {
        appearance: "system",
        reduced_motion: false,
        larger_text: false,
        notify_when_attention_needed: false,
      },
    });
    tauri.invoke.mockImplementation(async (command: string) =>
      command === "inspect_application_settings"
        ? {
            general_preferences: {
              reduced_motion: false,
            },
          }
        : undefined,
    );
  });

  afterEach(() => {
    Reflect.deleteProperty(window, "__TAURI_INTERNALS__");
    document.documentElement.className = "";
    delete document.documentElement.dataset.quantixAppearance;
    vi.useRealTimers();
    cleanup();
    vi.clearAllMocks();
  });

  it("opens a healthy workspace without an infrastructure checklist", async () => {
    render(<App />);

    expect(await screen.findByTestId("manager-workspace")).toBeTruthy();
    expect(screen.queryByRole("status")).toBeNull();
    expect(host.ensureQuantixSetup).toHaveBeenCalledTimes(1);
    expect(host.inspectManagerWorkspace).toHaveBeenCalledTimes(1);
  });

  it("announces one truthful activity only after the quiet threshold", async () => {
    vi.useFakeTimers();
    let resolveSetup!: (value: { state: "ready"; issues: never[] }) => void;
    host.ensureQuantixSetup.mockReturnValue(
      new Promise((resolve) => {
        resolveSetup = resolve;
      }),
    );
    render(<App />);

    expect(screen.queryByRole("status")).toBeNull();
    await act(async () => {
      vi.advanceTimersByTime(700);
    });
    expect(screen.getByRole("status").textContent).toContain(
      "Opening the local workspace",
    );

    await act(async () => {
      resolveSetup({ state: "ready", issues: [] });
      await Promise.resolve();
    });
    vi.useRealTimers();
    expect(await screen.findByTestId("manager-workspace")).toBeTruthy();
  });

  it("keeps nonblocking setup warnings visible in the workspace", async () => {
    host.ensureQuantixSetup.mockResolvedValue({
      state: "warning",
      issues: ["storage_permissions_unverified"],
    });

    render(<App />);

    expect(await screen.findByTestId("manager-workspace")).toBeTruthy();
    expect(screen.getByText("storage_permissions_unverified")).toBeTruthy();
  });

  it("applies saved motion preferences before publishing the ready workspace", async () => {
    host.inspectApplicationSettings.mockResolvedValue({
      general_preferences: {
        appearance: "light",
        reduced_motion: true,
        larger_text: false,
        notify_when_attention_needed: false,
      },
    });

    render(<App />);

    expect((await screen.findByTestId("motion-preference")).textContent).toBe(
      "reduced",
    );
    expect(document.documentElement.classList).toContain(
      "quantix-reduced-motion",
    );
  });

  it("publishes native display readiness after the opening shell commits", async () => {
    Object.defineProperty(window, "__TAURI_INTERNALS__", {
      configurable: true,
      value: {},
    });
    let resolveSetup!: (value: { state: "ready"; issues: never[] }) => void;
    host.ensureQuantixSetup.mockReturnValue(
      new Promise((resolve) => {
        resolveSetup = resolve;
      }),
    );

    render(<App />);
    await act(async () => Promise.resolve());

    expect(tauri.invoke).toHaveBeenCalledWith("notify_startup_display_ready");

    await act(async () => {
      resolveSetup({ state: "ready", issues: [] });
    });

    expect(await screen.findByTestId("manager-workspace")).toBeTruthy();
    expect(tauri.invoke).toHaveBeenCalledWith("notify_startup_display_ready");
    expect(
      tauri.invoke.mock.calls.filter(
        ([command]) => command === "notify_startup_display_ready",
      ),
    ).toHaveLength(1);
  });

  it("publishes display readiness while the workspace projection is still restoring", async () => {
    Object.defineProperty(window, "__TAURI_INTERNALS__", {
      configurable: true,
      value: {},
    });
    let resolveProjection!: (value: typeof projection) => void;
    host.inspectManagerWorkspace.mockReturnValue(
      new Promise((resolve) => {
        resolveProjection = resolve;
      }),
    );

    render(<App />);

    expect(tauri.invoke).toHaveBeenCalledWith("notify_startup_display_ready");
    expect(screen.queryByTestId("manager-workspace")).toBeNull();

    await act(async () => {
      await new Promise((resolve) => window.setTimeout(resolve, 710));
    });
    expect(screen.getByRole("status").textContent).toContain(
      "Restoring your Tender workspace",
    );

    await act(async () => {
      resolveProjection(projection);
      await Promise.resolve();
    });
    expect(await screen.findByTestId("manager-workspace")).toBeTruthy();
    expect(
      tauri.invoke.mock.calls.filter(
        ([command]) => command === "notify_startup_display_ready",
      ),
    ).toHaveLength(1);
  });
});
