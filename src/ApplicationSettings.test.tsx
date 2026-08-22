import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { useState } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const host = vi.hoisted(() => ({
  cancelChatGptLogin: vi.fn(),
  checkQuantixUpdate: vi.fn(),
  confirmAiExecutionSelection: vi.fn(),
  disconnectChatGpt: vi.fn(),
  exportDiagnosticsSupportBundle: vi.fn(),
  inspectDiagnosticTimeline: vi.fn(),
  inspectDiagnosticsStatus: vi.fn(),
  inspectQuantixDoctor: vi.fn(),
  inspectRuntimeReadiness: vi.fn(),
  openDiagnosticLogs: vi.fn(),
  refreshApplicationSettings: vi.fn(),
  repairQuantixDoctor: vi.fn(),
  startChatGptDeviceLogin: vi.fn(),
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
    installation_schema_version: 25,
    tender_schema_version: 36,
  },
};

const disconnectedConnection = {
  connection_id: "codex_chatgpt",
  provider: "codex" as const,
  display_name: "ChatGPT",
  status: "authentication_required" as const,
  account_label: null,
  account_plan: null,
  models: [],
  catalogue_fetched_at: null,
  adapter_version: "codex-v1",
  status_summary: "Connect ChatGPT.",
};

const readyConnection = {
  ...disconnectedConnection,
  status: "ready" as const,
  account_label: "engineer@example.com",
  account_plan: "plus",
  catalogue_fetched_at: "2026-08-22T08:00:00Z",
  models: [
    {
      model_id: "gpt-construction",
      display_name: "Recommended",
      description: "Balanced for everyday Tender work.",
      is_default: true,
      input_modalities: ["text"],
      reasoning_options: [
        {
          selection: { kind: "codex_effort", value: "medium" } as const,
          label: "Medium",
          description: "Balanced speed and depth.",
          is_default: true,
        },
      ],
    },
    {
      model_id: "gpt-deep",
      display_name: "Deep review",
      description: "More time for difficult Tender reviews.",
      is_default: false,
      input_modalities: ["text"],
      reasoning_options: [
        {
          selection: { kind: "codex_effort", value: "high" } as const,
          label: "High",
          description: "Deeper review.",
          is_default: true,
        },
      ],
    },
  ],
};

const preparedSelection = {
  connection_id: "codex_chatgpt",
  provider: "codex" as const,
  model_id: "gpt-construction",
  reasoning: { kind: "codex_effort", value: "medium" } as const,
  catalogue_fetched_at: "2026-08-22T08:00:00Z",
  adapter_version: "codex-v1",
};

function disconnectedView(
  loginPhase:
    | "idle"
    | "awaiting_browser"
    | "awaiting_device"
    | "failed"
    | "cancelled" = "idle",
) {
  return {
    ...applicationFacts,
    ai_execution_selection: null,
    ai_execution_approval: null,
    provider_connections: [disconnectedConnection],
    chatgpt: {
      state: "absent" as const,
      account_id: null,
      plan_type: null,
      expires_at_ms: null,
      login_phase: loginPhase,
    },
  };
}

function connectedView(approved = false) {
  return {
    ...applicationFacts,
    ai_execution_selection: preparedSelection,
    ai_execution_approval: approved
      ? {
          ...preparedSelection,
          account_fingerprint: "account-fingerprint",
          data_destination: "OpenAI through the connected ChatGPT account",
          approved_at: "2026-08-22T08:01:00Z",
        }
      : null,
    provider_connections: [readyConnection],
    chatgpt: {
      state: "connected" as const,
      account_id: "chatgpt-account",
      plan_type: "plus",
      expires_at_ms: 1_800_000_000_000n,
      login_phase: "completed" as const,
    },
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
  fireEvent.click(screen.getByRole("button", { name: "ChatGPT & Models" }));
  return rendered;
}

function ClosableSettings() {
  const [open, setOpen] = useState(true);
  return open ? (
    <ApplicationSettings
      aiAvailable={false}
      onAiAvailabilityChange={vi.fn()}
      onClose={() => setOpen(false)}
    />
  ) : (
    <p>Settings closed</p>
  );
}

beforeEach(() => {
  host.refreshApplicationSettings.mockResolvedValue(disconnectedView());
  host.cancelChatGptLogin.mockResolvedValue(undefined);
  Object.defineProperty(navigator, "clipboard", {
    configurable: true,
    value: { writeText: vi.fn().mockResolvedValue(undefined) },
  });
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
  vi.restoreAllMocks();
  vi.clearAllMocks();
});

describe("ApplicationSettings ChatGPT connection", () => {
  it("shows one beginner ChatGPT card with browser sign-in as the primary action", async () => {
    renderSettings();

    expect(
      await screen.findByRole("heading", { name: "ChatGPT & Models" }),
    ).toBeTruthy();
    expect(
      screen.getByText(
        "Quantix opens ChatGPT in your browser and reconnects automatically when you finish signing in.",
      ),
    ).toBeTruthy();
    expect(
      screen.getByRole("button", { name: "Connect ChatGPT" }),
    ).toBeTruthy();
    expect(
      screen.queryByRole("button", { name: "Sign in on another device" }),
    ).toBeNull();
    expect(screen.getByText("Having trouble signing in?")).toBeTruthy();
  });

  it("shows the exact automatic browser waiting state and supports cancellation", async () => {
    host.startChatGptLogin.mockResolvedValue({ status: "awaiting_browser" });
    host.refreshApplicationSettings
      .mockResolvedValueOnce(disconnectedView())
      .mockResolvedValueOnce(disconnectedView("awaiting_browser"))
      .mockResolvedValueOnce(disconnectedView("cancelled"));
    renderSettings();

    fireEvent.click(
      await screen.findByRole("button", { name: "Connect ChatGPT" }),
    );

    expect(host.startChatGptLogin).toHaveBeenCalledWith();
    expect(
      await screen.findByText(
        "Finish signing in in your browser. Quantix will connect automatically.",
      ),
    ).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(host.cancelChatGptLogin).toHaveBeenCalledWith());
    expect(
      await screen.findByRole("button", { name: "Connect ChatGPT" }),
    ).toBeTruthy();
  });

  it("turns a blocked browser return into plain guidance and an explicit fallback", async () => {
    host.startChatGptLogin.mockRejectedValue({ code: "oauth_port_blocked" });
    renderSettings();

    fireEvent.click(
      await screen.findByRole("button", { name: "Connect ChatGPT" }),
    );

    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toContain(
      "Quantix could not receive the browser sign-in.",
    );
    expect(alert.textContent).not.toMatch(/port|PID|OAuth/i);
    expect(
      screen.getByRole("button", { name: "Sign in on another device" }),
    ).toBeTruthy();
  });

  it("waits briefly for a cancelled browser attempt before starting the device fallback", async () => {
    host.startChatGptLogin.mockResolvedValue({ status: "awaiting_browser" });
    host.startChatGptDeviceLogin
      .mockRejectedValueOnce({ code: "oauth_already_running" })
      .mockResolvedValue({
        verification_url: "https://auth.openai.com/codex/device",
        user_code: "HAND-OFF",
      });
    host.refreshApplicationSettings
      .mockResolvedValueOnce(disconnectedView())
      .mockResolvedValueOnce(disconnectedView("awaiting_browser"))
      .mockResolvedValueOnce(disconnectedView("cancelled"))
      .mockResolvedValueOnce(disconnectedView("awaiting_device"));
    renderSettings();

    fireEvent.click(
      await screen.findByRole("button", { name: "Connect ChatGPT" }),
    );
    fireEvent.click(await screen.findByText("Having trouble signing in?"));
    fireEvent.click(
      screen.getByRole("button", { name: "Sign in on another device" }),
    );

    expect(await screen.findByText("HAND-OFF")).toBeTruthy();
    expect(host.cancelChatGptLogin).toHaveBeenCalledWith();
    expect(host.startChatGptDeviceLogin).toHaveBeenCalledTimes(2);
  });

  it("cancels a device initiation owned by Settings when the screen closes, including a late result", async () => {
    let finishDeviceStart:
      | ((result: { verification_url: string; user_code: string }) => void)
      | undefined;
    host.startChatGptDeviceLogin.mockReturnValue(
      new Promise<{
        verification_url: string;
        user_code: string;
      }>((resolve) => {
        finishDeviceStart = resolve;
      }),
    );
    render(<ClosableSettings />);
    fireEvent.click(screen.getByRole("button", { name: "ChatGPT & Models" }));
    fireEvent.click(await screen.findByText("Having trouble signing in?"));
    fireEvent.click(
      screen.getByRole("button", { name: "Sign in on another device" }),
    );
    await waitFor(() =>
      expect(host.startChatGptDeviceLogin).toHaveBeenCalledWith(),
    );

    fireEvent.click(screen.getByRole("button", { name: "Back to workspace" }));
    expect(await screen.findByText("Settings closed")).toBeTruthy();
    expect(host.cancelChatGptLogin).toHaveBeenCalledTimes(1);

    await act(async () => {
      finishDeviceStart?.({
        verification_url: "https://auth.openai.com/codex/device",
        user_code: "TOO-LATE",
      });
      await Promise.resolve();
    });
    expect(host.cancelChatGptLogin).toHaveBeenCalledTimes(2);
    expect(screen.queryByText("TOO-LATE")).toBeNull();
  });

  it("recovers an orphaned device attempt with a new one-time code", async () => {
    host.refreshApplicationSettings
      .mockResolvedValueOnce(disconnectedView("awaiting_device"))
      .mockResolvedValueOnce(disconnectedView("cancelled"))
      .mockResolvedValue(disconnectedView("awaiting_device"));
    host.startChatGptDeviceLogin
      .mockRejectedValueOnce({ code: "oauth_already_running" })
      .mockResolvedValue({
        verification_url: "https://auth.openai.com/codex/device",
        user_code: "NEW-CODE",
      });
    renderSettings();

    expect(
      await screen.findByText(
        "The previous one-time code is no longer available. Get a new code to continue.",
      ),
    ).toBeTruthy();
    fireEvent.click(
      screen.getByRole("button", { name: "Get a new one-time code" }),
    );

    expect(await screen.findByText("NEW-CODE")).toBeTruthy();
    expect(host.cancelChatGptLogin).toHaveBeenCalledWith();
    expect(host.startChatGptDeviceLogin).toHaveBeenCalledTimes(2);
  });

  it("shows, copies, opens, waits on, and cancels the one-time-code fallback", async () => {
    const open = vi.spyOn(window, "open").mockImplementation(() => null);
    host.startChatGptDeviceLogin.mockResolvedValue({
      verification_url: "https://auth.openai.com/codex/device",
      user_code: "ABCD-EFGH",
    });
    host.refreshApplicationSettings
      .mockResolvedValueOnce(disconnectedView())
      .mockResolvedValueOnce(disconnectedView("awaiting_device"))
      .mockResolvedValueOnce(disconnectedView("cancelled"));
    renderSettings();

    fireEvent.click(await screen.findByText("Having trouble signing in?"));
    fireEvent.click(
      screen.getByRole("button", { name: "Sign in on another device" }),
    );

    expect(host.startChatGptDeviceLogin).toHaveBeenCalledWith();
    expect(
      await screen.findByText(
        "Enter this one-time code on the OpenAI page, then return to Quantix.",
      ),
    ).toBeTruthy();
    expect(screen.getByLabelText("One-time code").textContent).toBe(
      "ABCD-EFGH",
    );
    expect(
      screen.getByText("Waiting for you to finish signing in"),
    ).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "Copy code" }));
    await waitFor(() =>
      expect(navigator.clipboard.writeText).toHaveBeenCalledWith("ABCD-EFGH"),
    );
    fireEvent.click(
      screen.getByRole("button", { name: "Open OpenAI sign-in page" }),
    );
    expect(open).toHaveBeenCalledWith(
      "https://auth.openai.com/codex/device",
      "_blank",
      "noopener,noreferrer",
    );

    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(await screen.findByText(/sign-in was cancelled/i)).toBeTruthy();
    expect(screen.queryByText("ABCD-EFGH")).toBeNull();
  });

  it("clears the one-time code when the Host reports failure", async () => {
    host.startChatGptDeviceLogin.mockResolvedValue({
      verification_url: "https://auth.openai.com/codex/device",
      user_code: "FAIL-CODE",
    });
    host.refreshApplicationSettings
      .mockResolvedValueOnce(disconnectedView())
      .mockResolvedValueOnce(disconnectedView("awaiting_device"))
      .mockResolvedValueOnce(disconnectedView("failed"));
    renderSettings();

    fireEvent.click(await screen.findByText("Having trouble signing in?"));
    fireEvent.click(
      screen.getByRole("button", { name: "Sign in on another device" }),
    );
    expect(await screen.findByText("FAIL-CODE")).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "Refresh" }));

    expect(
      await screen.findByText(/ChatGPT sign-in did not finish/i),
    ).toBeTruthy();
    await waitFor(() => expect(screen.queryByText("FAIL-CODE")).toBeNull());
    expect(
      screen.getByRole("button", { name: "Sign in on another device" }),
    ).toBeTruthy();
  });

  it("keeps login polling single-flight and clears a transient error after recovery", async () => {
    vi.useFakeTimers();
    let finishRecovery:
      ((view: ReturnType<typeof connectedView>) => void) | undefined;
    const recovering = new Promise<ReturnType<typeof connectedView>>(
      (resolve) => {
        finishRecovery = resolve;
      },
    );
    host.refreshApplicationSettings
      .mockResolvedValueOnce(disconnectedView("awaiting_browser"))
      .mockRejectedValueOnce({ code: "store_unavailable" })
      .mockReturnValueOnce(recovering);
    renderSettings();
    await act(async () => {
      await Promise.resolve();
    });

    await act(async () => {
      vi.advanceTimersByTime(1_200);
      await Promise.resolve();
    });
    expect(screen.getByRole("alert").textContent).toContain(
      "Quantix could not complete this local action",
    );

    await act(async () => {
      vi.advanceTimersByTime(1_200);
      await Promise.resolve();
      vi.advanceTimersByTime(3_600);
    });
    expect(host.refreshApplicationSettings).toHaveBeenCalledTimes(3);

    await act(async () => {
      finishRecovery?.(connectedView(true));
      await Promise.resolve();
    });
    expect(screen.queryByRole("alert")).toBeNull();
    expect(screen.getAllByText("Connected")).toHaveLength(2);
  });

  it("shows the connected account and plan without exposing expiry", async () => {
    host.refreshApplicationSettings.mockResolvedValue(connectedView(true));
    renderSettings();

    expect(await screen.findByText("engineer@example.com")).toBeTruthy();
    expect(screen.getByText("plus plan")).toBeTruthy();
    expect(screen.getAllByText("Connected")).toHaveLength(2);
    expect(screen.getByRole("button", { name: "Disconnect" })).toBeTruthy();
    expect(document.body.textContent).not.toMatch(/expires|expiry/i);
  });

  it("explains how to recover when the account is connected but ChatGPT is unavailable", async () => {
    const unavailable = connectedView(true);
    host.refreshApplicationSettings.mockResolvedValue({
      ...unavailable,
      provider_connections: [
        {
          ...readyConnection,
          status: "temporarily_unavailable",
          status_summary: "ChatGPT could not load the current model catalogue.",
        },
      ],
    });
    renderSettings();

    expect(await screen.findByText("ChatGPT needs attention")).toBeTruthy();
    expect(
      screen.getByText("ChatGPT could not load the current model catalogue."),
    ).toBeTruthy();
    expect(
      screen.getByText(/Select Refresh above.*disconnect it and connect again/),
    ).toBeTruthy();
    expect(screen.getByRole("button", { name: "Disconnect" })).toBeTruthy();
    expect(document.body.textContent).not.toMatch(/expires|expiry/i);
  });

  it("disconnects ChatGPT and clears the connected state", async () => {
    host.refreshApplicationSettings.mockResolvedValue(connectedView(true));
    host.disconnectChatGpt.mockResolvedValue(disconnectedView());
    renderSettings();

    fireEvent.click(await screen.findByRole("button", { name: "Disconnect" }));

    await waitFor(() => expect(host.disconnectChatGpt).toHaveBeenCalledWith());
    expect(
      await screen.findByRole("button", { name: "Connect ChatGPT" }),
    ).toBeTruthy();
  });

  it("presents the prepared recommendation and requires explicit ChatGPT approval", async () => {
    host.refreshApplicationSettings.mockResolvedValue(connectedView(false));
    host.confirmAiExecutionSelection.mockResolvedValue(connectedView(true));
    renderSettings();

    expect(
      await screen.findByText("ChatGPT · Recommended · medium"),
    ).toBeTruthy();
    expect(
      screen.getAllByText(
        /Tender content is sent to OpenAI through your connected ChatGPT account/,
      ).length,
    ).toBeGreaterThan(0);
    fireEvent.click(screen.getByRole("button", { name: "Use ChatGPT" }));

    await waitFor(() => {
      expect(host.confirmAiExecutionSelection).toHaveBeenCalledWith({
        connection_id: "codex_chatgpt",
        model_id: "gpt-construction",
        reasoning: { kind: "codex_effort", value: "medium" },
      });
    });
    expect(
      await screen.findByText("ChatGPT is ready for new Tenders."),
    ).toBeTruthy();
  });

  it("keeps model, reasoning, provenance, and future-Tender behavior in advanced settings", async () => {
    const initial = connectedView(true);
    host.refreshApplicationSettings.mockResolvedValue(initial);
    host.updateAiExecutionSelection.mockResolvedValue({
      ...initial,
      ai_execution_selection: {
        ...preparedSelection,
        model_id: "gpt-deep",
        reasoning: { kind: "codex_effort", value: "high" },
      },
      ai_execution_approval: null,
    });
    renderSettings();

    const advanced = await screen.findByText("Advanced model settings");
    fireEvent.click(advanced);
    expect(screen.getByText("Catalogue provenance")).toBeTruthy();
    expect(
      screen.getByText(/Model changes become the default for future Tenders/),
    ).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: /^Model/ }));
    fireEvent.click(screen.getByRole("option", { name: /^Deep review/ }));
    await waitFor(() => {
      expect(host.updateAiExecutionSelection).toHaveBeenCalledWith({
        connection_id: "codex_chatgpt",
        model_id: "gpt-deep",
        reasoning: { kind: "codex_effort", value: "high" },
      });
    });
  });
});
