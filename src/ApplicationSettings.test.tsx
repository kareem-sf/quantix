import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const host = vi.hoisted(() => ({
  cancelChatGptLogin: vi.fn(),
  connectAnthropic: vi.fn(),
  connectGemini: vi.fn(),
  disconnectAiProvider: vi.fn(),
  disconnectChatGpt: vi.fn(),
  exportDiagnosticsSupportBundle: vi.fn(),
  inspectDiagnosticTimeline: vi.fn(),
  inspectDiagnosticsStatus: vi.fn(),
  inspectRuntimeReadiness: vi.fn(),
  openDiagnosticLogs: vi.fn(),
  refreshApplicationSettings: vi.fn(),
  startChatGptLogin: vi.fn(),
  startTenderDeepDiagnostics: vi.fn(),
  stopTenderDeepDiagnostics: vi.fn(),
  updateAiExecutionSelection: vi.fn(),
  updateGeneralApplicationPreferences: vi.fn(),
  validateQuantixUpdateRestart: vi.fn(),
}));

const notifications = vi.hoisted(() => ({
  enableAttentionNotifications: vi.fn(),
}));

vi.mock("./quantixHost", () => host);
vi.mock("./applicationNotifications", () => notifications);

import { ApplicationSettings } from "./ApplicationSettings";

const applicationFacts = {
  general_preferences: {
    appearance: "system" as const,
    reduced_motion: false,
    high_contrast: false,
    larger_text: false,
    notify_when_attention_needed: false,
  },
  storage: {
    application_home: "A:\\Quantix-test",
    tender_backups_are_preserved: true,
    trash_requires_explicit_purge: true,
  },
  diagnostics: {
    quantix_version: "0.1.0",
    installation_schema_version: 21,
    tender_schema_version: 32,
  },
};

const codexConnection = {
  connection_id: "codex_chatgpt",
  provider: "codex" as const,
  display_name: "OpenAI account via Codex",
  status: "authentication_required" as const,
  account_label: null,
  account_plan: null,
  models: [],
  catalogue_fetched_at: null,
  adapter_version: "0.147.0",
  status_summary: "Connect an OpenAI account.",
};

function settingsView(chatgpt: Record<string, unknown>) {
  return {
    ...applicationFacts,
    ai_execution_selection: null,
    active_provider_login: null,
    provider_connections: [codexConnection],
    chatgpt,
  };
}

function renderSettings() {
  const rendered = render(
    <ApplicationSettings
      aiAvailable={false}
      onAiAvailabilityChange={vi.fn()}
      onClose={vi.fn()}
    />,
  );
  fireEvent.click(screen.getByRole("button", { name: "AI & Models" }));
  return rendered;
}

beforeEach(() => {
  host.refreshApplicationSettings.mockResolvedValue(
    settingsView({
      state: "absent",
      account_id: null,
      plan_type: null,
      expires_at_ms: null,
    }),
  );
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe("ApplicationSettings ChatGPT connection", () => {
  it("offers Connect ChatGPT while no ChatGPT account is connected", async () => {
    renderSettings();

    expect(
      await screen.findByRole("button", { name: "Connect ChatGPT" }),
    ).toBeTruthy();
    expect(host.startChatGptLogin).not.toHaveBeenCalled();
  });

  it("drives the Quantix-owned browser sign-in from the card", async () => {
    host.startChatGptLogin.mockResolvedValue({ status: "awaiting_browser" });
    renderSettings();

    fireEvent.click(
      await screen.findByRole("button", { name: "Connect ChatGPT" }),
    );

    expect(host.startChatGptLogin).toHaveBeenCalledWith();
    expect(
      await screen.findByText("Finish signing in through your browser."),
    ).toBeTruthy();
  });

  it("returns to the connect offer when the browser sign-in is cancelled", async () => {
    host.startChatGptLogin.mockResolvedValue({ status: "awaiting_browser" });
    renderSettings();

    fireEvent.click(
      await screen.findByRole("button", { name: "Connect ChatGPT" }),
    );
    fireEvent.click(await screen.findByRole("button", { name: "Cancel" }));

    expect(host.cancelChatGptLogin).toHaveBeenCalledWith();
    expect(
      await screen.findByRole("button", { name: "Connect ChatGPT" }),
    ).toBeTruthy();
  });

  it("maps sign-in failures to truthful error copy", async () => {
    host.startChatGptLogin.mockRejectedValueOnce({
      code: "oauth_port_blocked",
      port_holders: { port_1455: 4312, port_1457: null },
    });
    renderSettings();

    const connect = async () =>
      fireEvent.click(
        await screen.findByRole("button", { name: "Connect ChatGPT" }),
      );

    await connect();
    let alert = await screen.findByRole("alert");
    expect(alert.textContent).toContain("PID 4312");
    expect(alert.textContent).toContain("1455");
    expect(alert.textContent).not.toContain("1457");

    host.startChatGptLogin.mockRejectedValueOnce({
      code: "oauth_port_blocked",
    });
    fireEvent.click(screen.getByRole("button", { name: "Connect ChatGPT" }));
    alert = await screen.findByRole("alert");
    expect(alert.textContent).toContain("busy");

    host.startChatGptLogin.mockRejectedValueOnce({
      code: "oauth_already_running",
    });
    fireEvent.click(screen.getByRole("button", { name: "Connect ChatGPT" }));
    alert = await screen.findByRole("alert");
    expect(alert.textContent).toContain("already running");
  });

  it("shows the connected account, plan, and expiry from inspect", async () => {
    const expiry = new Date(1_800_000_000_000);
    host.refreshApplicationSettings.mockResolvedValue(
      settingsView({
        state: "connected",
        account_id: "acc-chatgpt-123",
        plan_type: "plus",
        expires_at_ms: 1_800_000_000_000n,
      }),
    );
    renderSettings();

    expect(await screen.findByText(/acc-chatgpt-123/)).toBeTruthy();
    expect(screen.getByText(/plus plan/i)).toBeTruthy();
    expect(screen.getByText(/Expires/).textContent).toBe(
      `plus plan · Expires ${expiry.toLocaleString()}`,
    );
    expect(screen.getByRole("button", { name: "Disconnect" })).toBeTruthy();
    expect(
      screen.queryByRole("button", { name: "Connect ChatGPT" }),
    ).toBeNull();
  });

  it("disconnects ChatGPT through the host command", async () => {
    host.disconnectChatGpt.mockResolvedValue(
      settingsView({
        state: "absent",
        account_id: null,
        plan_type: null,
        expires_at_ms: null,
      }),
    );
    host.refreshApplicationSettings.mockResolvedValue(
      settingsView({
        state: "connected",
        account_id: "acc-chatgpt-123",
        plan_type: "plus",
        expires_at_ms: 1_800_000_000_000n,
      }),
    );
    renderSettings();

    fireEvent.click(await screen.findByRole("button", { name: "Disconnect" }));

    await waitFor(() => {
      expect(host.disconnectChatGpt).toHaveBeenCalledWith();
    });
    expect(
      await screen.findByRole("button", { name: "Connect ChatGPT" }),
    ).toBeTruthy();
  });
});
