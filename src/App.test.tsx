import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const host = vi.hoisted(() => ({
  ensureQuantixSetup: vi.fn(),
  inspectRuntimeReadiness: vi.fn(),
  refreshApplicationSettings: vi.fn(),
  repairRuntimeReadiness: vi.fn(),
  resumeManagerIntakes: vi.fn(),
  validateQuantixUpdateRestart: vi.fn(),
}));

vi.mock("./quantixHost", () => host);
vi.mock("./ManagerWorkspace", () => ({
  ManagerWorkspace: ({ aiAvailable }: { aiAvailable: boolean }) => (
    <p>{aiAvailable ? "AI office ready" : "AI office unavailable"}</p>
  ),
}));

import App from "./App";

describe("App runtime startup", () => {
  beforeEach(() => {
    host.ensureQuantixSetup.mockResolvedValue({ state: "ready", issues: [] });
    host.validateQuantixUpdateRestart.mockResolvedValue({ state: "idle" });
    host.resumeManagerIntakes.mockResolvedValue(undefined);
    host.refreshApplicationSettings.mockResolvedValue({
      general_preferences: {
        appearance: "system",
        reduced_motion: false,
        high_contrast: false,
        larger_text: false,
        notify_when_attention_needed: false,
      },
      ai_execution_selection: null,
      provider_connections: [{ status: "ready" }],
      active_provider_login: null,
      storage: {
        application_home: "A:\\Quantix-test",
        tender_backups_are_preserved: true,
        trash_requires_explicit_purge: true,
      },
      diagnostics: {
        quantix_version: "0.1.0",
        installation_schema_version: 18,
        tender_schema_version: 31,
      },
    });
  });

  afterEach(() => {
    cleanup();
    vi.clearAllMocks();
  });

  it("prepares a missing managed runtime before opening the workspace", async () => {
    host.inspectRuntimeReadiness.mockResolvedValue({
      state: "missing_executable",
      issues: ["docling_executable_missing"],
      codex_version: "0.147.0",
      uv_version: "0.12.2",
      docling_version: null,
      repair_available: true,
    });
    host.repairRuntimeReadiness.mockResolvedValue({
      state: "ready",
      issues: [],
      codex_version: "0.147.0",
      uv_version: "0.12.2",
      docling_version: "2.118.0",
      repair_available: false,
    });

    render(<App />);

    expect(await screen.findByText("AI office ready")).toBeTruthy();
    expect(host.repairRuntimeReadiness).toHaveBeenCalledTimes(1);
    await waitFor(() => {
      expect(host.resumeManagerIntakes).toHaveBeenCalledTimes(1);
    });
  });

  it("keeps checking until an active runtime preparation finishes", async () => {
    host.inspectRuntimeReadiness
      .mockResolvedValueOnce({
        state: "preparing",
        issues: ["runtime_preparation_active"],
        codex_version: null,
        uv_version: null,
        docling_version: null,
        repair_available: false,
      })
      .mockResolvedValueOnce({
        state: "preparing",
        issues: ["runtime_preparation_active"],
        codex_version: null,
        uv_version: null,
        docling_version: null,
        repair_available: false,
      })
      .mockResolvedValueOnce({
        state: "ready",
        issues: [],
        codex_version: "0.147.0",
        uv_version: "0.12.2",
        docling_version: "2.118.0",
        repair_available: false,
      });

    render(<App />);

    expect(await screen.findByText("AI office ready")).toBeTruthy();
    expect(host.repairRuntimeReadiness).not.toHaveBeenCalled();
  });
});
