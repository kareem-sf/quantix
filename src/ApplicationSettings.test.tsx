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
  inspectAiProviders: vi.fn(),
  probeAiProvider: vi.fn(),
  inspectDiagnosticsStatus: vi.fn(),
  inspectQuantixDoctor: vi.fn(),
  inspectRuntimeReadiness: vi.fn(),
  openChatGptDeviceLoginPage: vi.fn(),
  openDiagnosticLogs: vi.fn(),
  refreshApplicationSettings: vi.fn(),
  removeAiProvider: vi.fn(),
  repairQuantixDoctor: vi.fn(),
  saveAiProvider: vi.fn(),
  setActiveAiProvider: vi.fn(),
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
import { exactApplicationAiSelectionIsReady } from "./applicationAiSelectionReadiness";

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
    installation_schema_version: 25n,
    tender_schema_version: 36n,
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
  adapter_version: "0.151.0",
  status_summary: "Connect ChatGPT.",
};

const readyConnection = {
  ...disconnectedConnection,
  status: "ready" as const,
  account_label: "engineer@example.com",
  account_plan: "plus",
  catalogue_fetched_at: "2026-08-30T00:00:00Z",
  models: [
    {
      model_id: "gpt-construction",
      display_name: "Recommended",
      description: "Balanced for everyday Tender work.",
      is_default: true,
      input_modalities: ["text"],
      reasoning_options: [
        {
          selection: { kind: "effort", value: "medium" } as const,
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
          selection: { kind: "effort", value: "high" } as const,
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
  reasoning: { kind: "effort", value: "medium" } as const,
  catalogue_fetched_at: "2026-08-30T00:00:00Z",
  adapter_version: "0.151.0",
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
  const selection = {
    ...preparedSelection,
    reasoning: { ...preparedSelection.reasoning },
  };
  return {
    ...applicationFacts,
    ai_execution_selection: selection,
    ai_execution_approval: approved
      ? {
          ...selection,
          reasoning: { ...selection.reasoning },
          account_fingerprint:
            "117d68e191e9e848c1172767d9ca54204ef5e4b20d1ead8855ef0f17f906f695",
          data_destination: "ChatGPT subscription",
          approved_at: "2026-08-22T08:01:00Z",
        }
      : null,
    provider_connections: [readyConnection],
    chatgpt: {
      state: "connected" as const,
      account_id: "engineer@example.com",
      plan_type: "plus",
      expires_at_ms: 1_800_000_000_000n,
      login_phase: "completed" as const,
    },
  };
}

function renderSettings(onAiAvailabilityChange = vi.fn()) {
  const rendered = render(
    <ApplicationSettings
      onAiAvailabilityChange={onAiAvailabilityChange}
      onClose={vi.fn()}
    />,
  );
  fireEvent.click(screen.getByRole("button", { name: "AI Providers" }));
  return rendered;
}

function ClosableSettings() {
  const [open, setOpen] = useState(true);
  return open ? (
    <ApplicationSettings
      onAiAvailabilityChange={vi.fn()}
      onClose={() => setOpen(false)}
    />
  ) : (
    <p>Settings closed</p>
  );
}

const noProviders = () => ({
  connections: [],
  active_id: null,
  file_path: "C:\\Users\\engineer\\.quantix\\.env",
});

beforeEach(() => {
  host.refreshApplicationSettings.mockResolvedValue(disconnectedView());
  host.inspectAiProviders.mockResolvedValue(noProviders());
  host.cancelChatGptLogin.mockResolvedValue(undefined);
  host.openChatGptDeviceLoginPage.mockResolvedValue(undefined);
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
  it("recognizes the exact production approval through the shared contract", async () => {
    expect(await exactApplicationAiSelectionIsReady(connectedView(true))).toBe(
      true,
    );
  });

  it("does not let an older account digest authorize a newer Settings snapshot", async () => {
    let finishDigest: ((value: ArrayBuffer) => void) | undefined;
    const digest = vi
      .spyOn(globalThis.crypto.subtle, "digest")
      .mockReturnValueOnce(
        new Promise<ArrayBuffer>((resolve) => {
          finishDigest = resolve;
        }),
      );
    const onAiAvailabilityChange = vi.fn();
    host.refreshApplicationSettings
      .mockResolvedValueOnce(connectedView(true))
      .mockResolvedValue(disconnectedView());
    renderSettings(onAiAvailabilityChange);
    await waitFor(() => expect(digest).toHaveBeenCalledTimes(1));

    fireEvent.click(screen.getByRole("button", { name: "Refresh" }));
    expect(
      await screen.findByRole("button", { name: "Connect ChatGPT" }),
    ).toBeTruthy();
    const fingerprint =
      "117d68e191e9e848c1172767d9ca54204ef5e4b20d1ead8855ef0f17f906f695";
    await act(async () => {
      finishDigest?.(
        Uint8Array.from(
          fingerprint.match(/.{2}/g)!.map((byte) => Number.parseInt(byte, 16)),
        ).buffer as ArrayBuffer,
      );
      await Promise.resolve();
    });

    expect(onAiAvailabilityChange).toHaveBeenLastCalledWith(false);
  });

  it("keeps approval unavailable when account fingerprinting fails", async () => {
    vi.spyOn(globalThis.crypto.subtle, "digest").mockRejectedValueOnce(
      new Error("digest unavailable"),
    );
    const onAiAvailabilityChange = vi.fn();
    host.refreshApplicationSettings.mockResolvedValue(connectedView(true));
    renderSettings(onAiAvailabilityChange);

    expect(
      await screen.findByRole("button", { name: "Use ChatGPT" }),
    ).toBeTruthy();
    expect(onAiAvailabilityChange).toHaveBeenLastCalledWith(false);
  });

  it("shows one beginner ChatGPT card with browser sign-in as the primary action", async () => {
    renderSettings();

    expect(
      await screen.findByRole("heading", { name: "AI Providers" }),
    ).toBeTruthy();
    expect(
      screen.getByText(
        "Connect the AI accounts Quantix may use for Tender work. Sign-in always completes in your browser, with the provider.",
      ),
    ).toBeTruthy();
    expect(
      screen.getByRole("button", { name: "Connect ChatGPT" }),
    ).toBeTruthy();
    const troubleshooting = screen
      .getByText("Having trouble signing in?")
      .closest("details");
    expect(troubleshooting?.open).toBe(false);
    expect(troubleshooting?.querySelector("button")?.textContent).toBe(
      "Sign in on another device",
    );
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
    fireEvent.click(screen.getByRole("button", { name: "AI Providers" }));
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

  it("preserves device initiation guidance across a failed Host refresh until a recovery action", async () => {
    host.startChatGptDeviceLogin.mockRejectedValue({
      code: "store_unavailable",
    });
    host.refreshApplicationSettings
      .mockResolvedValueOnce(disconnectedView())
      .mockResolvedValue(disconnectedView("failed"));
    renderSettings();

    fireEvent.click(await screen.findByText("Having trouble signing in?"));
    fireEvent.click(
      screen.getByRole("button", { name: "Sign in on another device" }),
    );

    expect(
      await screen.findByText(
        "Sign in on another device is unavailable right now. You can still connect ChatGPT in your browser.",
      ),
    ).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Refresh" }));
    expect(
      await screen.findByText(
        "Sign in on another device is unavailable right now. You can still connect ChatGPT in your browser.",
      ),
    ).toBeTruthy();
    expect(
      screen.getByRole("button", { name: "Connect ChatGPT" }),
    ).toBeTruthy();
    expect(
      screen.getByRole("button", { name: "Try device sign-in again" }),
    ).toBeTruthy();
    expect(screen.queryByText(/ChatGPT sign-in did not finish/i)).toBeNull();
  });

  it("shows, automatically opens, copies, opens again, waits on, and cancels the one-time-code fallback", async () => {
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
    await waitFor(() =>
      expect(host.openChatGptDeviceLoginPage).toHaveBeenCalledTimes(1),
    );
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
    await waitFor(() =>
      expect(host.openChatGptDeviceLoginPage).toHaveBeenCalledTimes(2),
    );

    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(await screen.findByText(/sign-in was cancelled/i)).toBeTruthy();
    expect(screen.queryByText("ABCD-EFGH")).toBeNull();
  });

  it("preserves an OpenAI page error through background polling and clears it after opening succeeds", async () => {
    vi.useFakeTimers();
    host.startChatGptDeviceLogin.mockResolvedValue({
      verification_url: "https://auth.openai.com/codex/device",
      user_code: "KEEP-CODE",
    });
    host.openChatGptDeviceLoginPage.mockRejectedValueOnce({
      code: "store_unavailable",
    });
    host.refreshApplicationSettings
      .mockResolvedValueOnce(disconnectedView())
      .mockResolvedValue(disconnectedView("awaiting_device"));
    renderSettings();

    await act(async () => {
      await Promise.resolve();
    });
    fireEvent.click(screen.getByText("Having trouble signing in?"));
    await act(async () => {
      fireEvent.click(
        screen.getByRole("button", { name: "Sign in on another device" }),
      );
      await Promise.resolve();
      await Promise.resolve();
      await Promise.resolve();
    });

    expect(screen.getByText("KEEP-CODE")).toBeTruthy();
    expect(
      screen.getByText(/Quantix could not open the OpenAI sign-in page\./),
    ).toBeTruthy();
    expect(
      screen.getByRole("alert", { name: "OpenAI page problem" }),
    ).toBeTruthy();

    await act(async () => {
      vi.advanceTimersByTime(1_200);
      await Promise.resolve();
    });
    expect(host.refreshApplicationSettings).toHaveBeenCalledTimes(3);
    expect(
      screen.getByText(/Quantix could not open the OpenAI sign-in page\./),
    ).toBeTruthy();

    await act(async () => {
      fireEvent.click(
        screen.getByRole("button", { name: "Open OpenAI sign-in page" }),
      );
      await Promise.resolve();
    });
    expect(host.openChatGptDeviceLoginPage).toHaveBeenCalledTimes(2);
    expect(screen.queryByRole("alert")).toBeNull();
    expect(screen.getByText("KEEP-CODE")).toBeTruthy();
  });

  it("preserves a copy error through background polling and clears it after copying succeeds", async () => {
    vi.useFakeTimers();
    host.startChatGptDeviceLogin.mockResolvedValue({
      verification_url: "https://auth.openai.com/codex/device",
      user_code: "COPY-CODE",
    });
    host.refreshApplicationSettings
      .mockResolvedValueOnce(disconnectedView())
      .mockResolvedValue(disconnectedView("awaiting_device"));
    vi.mocked(navigator.clipboard.writeText)
      .mockRejectedValueOnce(new Error("clipboard unavailable"))
      .mockResolvedValue(undefined);
    renderSettings();

    await act(async () => {
      await Promise.resolve();
    });
    fireEvent.click(screen.getByText("Having trouble signing in?"));
    await act(async () => {
      fireEvent.click(
        screen.getByRole("button", { name: "Sign in on another device" }),
      );
      await Promise.resolve();
      await Promise.resolve();
      await Promise.resolve();
    });
    expect(screen.getByText("COPY-CODE")).toBeTruthy();

    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Copy code" }));
      await Promise.resolve();
    });
    expect(screen.getByText(/Quantix could not copy the code\./)).toBeTruthy();

    await act(async () => {
      vi.advanceTimersByTime(1_200);
      await Promise.resolve();
    });
    expect(host.refreshApplicationSettings).toHaveBeenCalledTimes(3);
    expect(screen.getByText(/Quantix could not copy the code\./)).toBeTruthy();

    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Copy code" }));
      await Promise.resolve();
    });
    expect(screen.getByRole("button", { name: "Copied" })).toBeTruthy();
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("clears opener and clipboard guidance only when its matching action succeeds", async () => {
    host.startChatGptDeviceLogin.mockResolvedValue({
      verification_url: "https://auth.openai.com/codex/device",
      user_code: "DISTINCT-ERRORS",
    });
    host.openChatGptDeviceLoginPage.mockRejectedValueOnce(
      new Error("automatic opener failure"),
    );
    vi.mocked(navigator.clipboard.writeText).mockRejectedValueOnce(
      new Error("clipboard failure"),
    );
    host.refreshApplicationSettings
      .mockResolvedValueOnce(disconnectedView())
      .mockResolvedValue(disconnectedView("awaiting_device"));
    renderSettings();

    fireEvent.click(await screen.findByText("Having trouble signing in?"));
    fireEvent.click(
      screen.getByRole("button", { name: "Sign in on another device" }),
    );
    expect(await screen.findByText("DISTINCT-ERRORS")).toBeTruthy();
    expect(
      screen.getByText(/Quantix could not open the OpenAI sign-in page\./),
    ).toBeTruthy();
    expect(
      screen.getByRole("alert", { name: "OpenAI page problem" }),
    ).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "Copy code" }));
    await waitFor(() =>
      expect(
        screen.getByText(/Quantix could not copy the code\./),
      ).toBeTruthy(),
    );
    expect(
      screen.getByRole("alert", { name: "Copy code problem" }),
    ).toBeTruthy();
    expect(screen.getAllByRole("alert")).toHaveLength(2);
    fireEvent.click(
      screen.getByRole("button", { name: "Open OpenAI sign-in page" }),
    );
    await waitFor(() =>
      expect(host.openChatGptDeviceLoginPage).toHaveBeenCalledTimes(2),
    );
    expect(screen.getByText(/Quantix could not copy the code\./)).toBeTruthy();
    expect(
      screen.queryByText(/Quantix could not open the OpenAI sign-in page\./),
    ).toBeNull();
    expect(
      screen.queryByRole("alert", { name: "OpenAI page problem" }),
    ).toBeNull();

    fireEvent.click(screen.getByRole("button", { name: "Copy code" }));
    expect(await screen.findByRole("button", { name: "Copied" })).toBeTruthy();
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("ignores a superseded opener success that finishes after the current attempt fails", async () => {
    let finishFirstOpen: (() => void) | undefined;
    host.startChatGptDeviceLogin.mockResolvedValue({
      verification_url: "https://auth.openai.com/codex/device",
      user_code: "OPEN-ORDER",
    });
    host.refreshApplicationSettings
      .mockResolvedValueOnce(disconnectedView())
      .mockResolvedValue(disconnectedView("awaiting_device"));
    renderSettings();

    fireEvent.click(await screen.findByText("Having trouble signing in?"));
    fireEvent.click(
      screen.getByRole("button", { name: "Sign in on another device" }),
    );
    expect(await screen.findByText("OPEN-ORDER")).toBeTruthy();
    host.openChatGptDeviceLoginPage
      .mockReturnValueOnce(
        new Promise<void>((resolve) => {
          finishFirstOpen = resolve;
        }),
      )
      .mockRejectedValueOnce(new Error("latest opener failed"));

    fireEvent.click(
      screen.getByRole("button", { name: "Open OpenAI sign-in page" }),
    );
    fireEvent.click(
      screen.getByRole("button", { name: "Open OpenAI sign-in page" }),
    );
    expect(
      await screen.findByText(
        /Quantix could not open the OpenAI sign-in page\./,
      ),
    ).toBeTruthy();

    await act(async () => {
      finishFirstOpen?.();
      await Promise.resolve();
    });
    expect(
      screen.getByText(/Quantix could not open the OpenAI sign-in page\./),
    ).toBeTruthy();
  });

  it("ignores a superseded copy failure that finishes after the current attempt succeeds", async () => {
    let failFirstCopy: ((reason: Error) => void) | undefined;
    host.startChatGptDeviceLogin.mockResolvedValue({
      verification_url: "https://auth.openai.com/codex/device",
      user_code: "COPY-ORDER",
    });
    host.refreshApplicationSettings
      .mockResolvedValueOnce(disconnectedView())
      .mockResolvedValue(disconnectedView("awaiting_device"));
    renderSettings();

    fireEvent.click(await screen.findByText("Having trouble signing in?"));
    fireEvent.click(
      screen.getByRole("button", { name: "Sign in on another device" }),
    );
    expect(await screen.findByText("COPY-ORDER")).toBeTruthy();
    vi.mocked(navigator.clipboard.writeText)
      .mockReturnValueOnce(
        new Promise<void>((_resolve, reject) => {
          failFirstCopy = reject;
        }),
      )
      .mockResolvedValueOnce(undefined);

    fireEvent.click(screen.getByRole("button", { name: "Copy code" }));
    fireEvent.click(screen.getByRole("button", { name: "Copy code" }));
    expect(await screen.findByRole("button", { name: "Copied" })).toBeTruthy();

    await act(async () => {
      failFirstCopy?.(new Error("superseded clipboard failure"));
      await Promise.resolve();
    });
    expect(screen.getByRole("button", { name: "Copied" })).toBeTruthy();
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("invalidates pending device actions and clears their errors on a terminal login state", async () => {
    let failPendingOpen: ((reason: Error) => void) | undefined;
    let failPendingCopy: ((reason: Error) => void) | undefined;
    host.startChatGptDeviceLogin.mockResolvedValue({
      verification_url: "https://auth.openai.com/codex/device",
      user_code: "TERMINAL-CODE",
    });
    host.openChatGptDeviceLoginPage
      .mockRejectedValueOnce(new Error("automatic opener failure"))
      .mockReturnValueOnce(
        new Promise<void>((_resolve, reject) => {
          failPendingOpen = reject;
        }),
      );
    vi.mocked(navigator.clipboard.writeText)
      .mockRejectedValueOnce(new Error("initial clipboard failure"))
      .mockReturnValueOnce(
        new Promise<void>((_resolve, reject) => {
          failPendingCopy = reject;
        }),
      );
    host.refreshApplicationSettings
      .mockResolvedValueOnce(disconnectedView())
      .mockResolvedValueOnce(disconnectedView("awaiting_device"))
      .mockResolvedValueOnce(disconnectedView("failed"));
    renderSettings();

    fireEvent.click(await screen.findByText("Having trouble signing in?"));
    fireEvent.click(
      screen.getByRole("button", { name: "Sign in on another device" }),
    );
    expect(await screen.findByText("TERMINAL-CODE")).toBeTruthy();
    expect(
      screen.getByText(/Quantix could not open the OpenAI sign-in page\./),
    ).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "Copy code" }));
    await waitFor(() =>
      expect(
        screen.getByText(/Quantix could not copy the code\./),
      ).toBeTruthy(),
    );
    fireEvent.click(screen.getByRole("button", { name: "Copy code" }));
    fireEvent.click(
      screen.getByRole("button", { name: "Open OpenAI sign-in page" }),
    );
    fireEvent.click(screen.getByRole("button", { name: "Refresh" }));

    const terminalAlert = await screen.findByRole("alert");
    expect(terminalAlert.textContent).toContain(
      "ChatGPT sign-in did not finish",
    );
    expect(screen.queryByText("TERMINAL-CODE")).toBeNull();
    expect(screen.queryByText(/Quantix could not copy the code\./)).toBeNull();
    expect(
      screen.queryByText(/Quantix could not open the OpenAI sign-in page\./),
    ).toBeNull();

    await act(async () => {
      failPendingOpen?.(new Error("late opener failure"));
      failPendingCopy?.(new Error("late clipboard failure"));
      await Promise.resolve();
    });
    expect(screen.getByRole("alert").textContent).toContain(
      "ChatGPT sign-in did not finish",
    );
    expect(screen.queryByText(/Quantix could not copy the code\./)).toBeNull();
    expect(
      screen.queryByText(/Quantix could not open the OpenAI sign-in page\./),
    ).toBeNull();
    expect(document.body.textContent).not.toContain("code is still ready");
  });

  it("preserves a device opener error after an unrelated update succeeds", async () => {
    host.startChatGptDeviceLogin.mockResolvedValue({
      verification_url: "https://auth.openai.com/codex/device",
      user_code: "KEEP-DEVICE-ERROR",
    });
    host.openChatGptDeviceLoginPage.mockRejectedValueOnce(
      new Error("automatic opener failure"),
    );
    host.refreshApplicationSettings
      .mockResolvedValueOnce(disconnectedView())
      .mockResolvedValue(disconnectedView("awaiting_device"));
    host.checkQuantixUpdate.mockResolvedValue({ state: "up_to_date" });
    renderSettings();

    fireEvent.click(await screen.findByText("Having trouble signing in?"));
    fireEvent.click(
      screen.getByRole("button", { name: "Sign in on another device" }),
    );
    expect(await screen.findByText("KEEP-DEVICE-ERROR")).toBeTruthy();
    expect(
      screen.getByText(/Quantix could not open the OpenAI sign-in page\./),
    ).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "Updates" }));
    fireEvent.click(screen.getByRole("button", { name: "Check for update" }));
    await waitFor(() => expect(host.checkQuantixUpdate).toHaveBeenCalledWith());
    expect(screen.queryByRole("alert")).toBeNull();

    fireEvent.click(screen.getByRole("button", { name: "AI Providers" }));
    expect(
      screen.getByText(/Quantix could not open the OpenAI sign-in page\./),
    ).toBeTruthy();
  });

  it("shows an unrelated general failure independently from device guidance", async () => {
    host.startChatGptDeviceLogin.mockResolvedValue({
      verification_url: "https://auth.openai.com/codex/device",
      user_code: "TWO-SOURCES",
    });
    host.openChatGptDeviceLoginPage.mockRejectedValueOnce(
      new Error("automatic opener failure"),
    );
    host.refreshApplicationSettings
      .mockResolvedValueOnce(disconnectedView())
      .mockResolvedValue(disconnectedView("awaiting_device"));
    host.checkQuantixUpdate.mockRejectedValue(new Error("update unavailable"));
    renderSettings();

    fireEvent.click(await screen.findByText("Having trouble signing in?"));
    fireEvent.click(
      screen.getByRole("button", { name: "Sign in on another device" }),
    );
    expect(await screen.findByText("TWO-SOURCES")).toBeTruthy();
    expect(
      screen.getByText(/Quantix could not open the OpenAI sign-in page\./),
    ).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "Updates" }));
    fireEvent.click(screen.getByRole("button", { name: "Check for update" }));
    expect(
      await screen.findByText(
        "Quantix could not reach the signed update source. Try again later.",
      ),
    ).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "AI Providers" }));
    expect(screen.getAllByRole("alert")).toHaveLength(2);
    expect(
      screen.getByText(/Quantix could not open the OpenAI sign-in page\./),
    ).toBeTruthy();
    expect(
      screen.getByText(
        "Quantix could not reach the signed update source. Try again later.",
      ),
    ).toBeTruthy();
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
        reasoning: { kind: "effort", value: "medium" },
      });
    });
    expect(
      await screen.findByText("ChatGPT is ready for new Tenders."),
    ).toBeTruthy();
  });

  it("does not accept approval for a different provider or data destination", async () => {
    const wrongProvider = connectedView(true);
    wrongProvider.ai_execution_approval!.provider =
      "different-provider" as typeof preparedSelection.provider;
    const onAiAvailabilityChange = vi.fn();
    host.refreshApplicationSettings.mockResolvedValue(wrongProvider);
    const { unmount } = renderSettings(onAiAvailabilityChange);

    expect(
      await screen.findByRole("button", { name: "Use ChatGPT" }),
    ).toBeTruthy();
    expect(onAiAvailabilityChange).toHaveBeenLastCalledWith(false);

    unmount();
    const wrongDestination = connectedView(true);
    wrongDestination.ai_execution_approval!.data_destination =
      "A different destination";
    host.refreshApplicationSettings.mockResolvedValue(wrongDestination);
    renderSettings(onAiAvailabilityChange);

    expect(
      await screen.findByRole("button", { name: "Use ChatGPT" }),
    ).toBeTruthy();
    expect(onAiAvailabilityChange).toHaveBeenLastCalledWith(false);
  });

  it("does not accept selection provenance from a different catalogue or adapter", async () => {
    const staleCatalogue = connectedView(true);
    staleCatalogue.ai_execution_selection.catalogue_fetched_at =
      "chatgpt-direct-v0";
    const onAiAvailabilityChange = vi.fn();
    host.refreshApplicationSettings.mockResolvedValue(staleCatalogue);
    const { unmount } = renderSettings(onAiAvailabilityChange);

    expect(
      await screen.findByRole("button", { name: "Use ChatGPT" }),
    ).toBeTruthy();
    expect(onAiAvailabilityChange).toHaveBeenLastCalledWith(false);

    unmount();
    const staleAdapter = connectedView(true);
    staleAdapter.ai_execution_selection.adapter_version = "chatgpt-direct-v0";
    host.refreshApplicationSettings.mockResolvedValue(staleAdapter);
    renderSettings(onAiAvailabilityChange);

    expect(
      await screen.findByRole("button", { name: "Use ChatGPT" }),
    ).toBeTruthy();
    expect(onAiAvailabilityChange).toHaveBeenLastCalledWith(false);
  });

  it("does not accept malformed account fingerprint data", async () => {
    const malformedFingerprint = connectedView(true);
    malformedFingerprint.ai_execution_approval!.account_fingerprint =
      "stale-account-fingerprint";
    const onAiAvailabilityChange = vi.fn();
    host.refreshApplicationSettings.mockResolvedValue(malformedFingerprint);
    renderSettings(onAiAvailabilityChange);

    expect(
      await screen.findByRole("button", { name: "Use ChatGPT" }),
    ).toBeTruthy();
    expect(onAiAvailabilityChange).toHaveBeenLastCalledWith(false);
  });

  it("does not accept inconsistent or stale account approval data", async () => {
    const changedAccount = connectedView(true);
    changedAccount.provider_connections = [
      {
        ...readyConnection,
        account_label: "replacement@example.com",
      },
    ];
    const onAiAvailabilityChange = vi.fn();
    host.refreshApplicationSettings.mockResolvedValue(changedAccount);
    renderSettings(onAiAvailabilityChange);

    expect(
      await screen.findByRole("button", { name: "Use ChatGPT" }),
    ).toBeTruthy();
    expect(onAiAvailabilityChange).toHaveBeenLastCalledWith(false);
  });

  it("keeps model, reasoning, provenance, and future-Tender behavior in advanced settings", async () => {
    const initial = connectedView(true);
    host.refreshApplicationSettings.mockResolvedValue(initial);
    host.updateAiExecutionSelection.mockResolvedValue({
      ...initial,
      ai_execution_selection: {
        ...preparedSelection,
        model_id: "gpt-deep",
        reasoning: { kind: "effort", value: "high" },
      },
      ai_execution_approval: null,
    });
    renderSettings();

    const advanced = await screen.findByText("Advanced model settings");
    fireEvent.click(advanced);
    expect(screen.getByText("Catalogue provenance")).toBeTruthy();
    expect(
      screen.getByText("Built-in catalogue version: 2026-08-30T00:00:00Z"),
    ).toBeTruthy();
    expect(document.body.textContent).not.toContain("Invalid Date");
    expect(
      screen.getByText(/Model changes become the default for future Tenders/),
    ).toBeTruthy();

    fireEvent.click(screen.getByLabelText("Model"));
    fireEvent.click(screen.getByRole("option", { name: /^Deep review/ }));
    await waitFor(() => {
      expect(host.updateAiExecutionSelection).toHaveBeenCalledWith({
        connection_id: "codex_chatgpt",
        model_id: "gpt-deep",
        reasoning: { kind: "effort", value: "high" },
      });
    });
  });

  it("lists model providers and marks the default", async () => {
    host.inspectAiProviders.mockResolvedValue({
      connections: [
        {
          id: "OPENROUTER",
          display_name: "OpenRouter",
          route: "openai_compatible",
          base_url: "https://openrouter.ai/api/v1",
          model_id: "anthropic/claude-sonnet-4.5",
          has_api_key: true,
          is_active: true,
        },
        {
          id: "GROQ",
          display_name: "Groq",
          route: "openai_compatible",
          base_url: "https://api.groq.com/openai/v1",
          model_id: "llama-3.3-70b-versatile",
          has_api_key: false,
          is_active: false,
        },
      ],
      active_id: "OPENROUTER",
      file_path: "C:\\Users\\engineer\\.quantix\\.env",
    });
    renderSettings();
    fireEvent.click(screen.getByRole("button", { name: "AI Providers" }));

    expect(await screen.findByText("OpenRouter")).toBeTruthy();
    expect(screen.getByText("Default")).toBeTruthy();
    // A provider with no key cannot run, so the interface has to say so.
    expect(
      screen.getByText("No API key stored. Edit this provider to add one."),
    ).toBeTruthy();
    // The storage trade-off is stated wherever keys are managed.
    expect(screen.getByText(/stored as plain text/)).toBeTruthy();
  });

  it("saves a new provider from the add form", async () => {
    host.saveAiProvider.mockResolvedValue({
      connections: [],
      active_id: null,
      file_path: "C:\\Users\\engineer\\.quantix\\.env",
    });
    renderSettings();
    fireEvent.click(screen.getByRole("button", { name: "AI Providers" }));
    fireEvent.click(
      await screen.findByRole("button", { name: /Add provider/ }),
    );

    fireEvent.change(screen.getByLabelText("Name"), {
      target: { value: "OpenRouter" },
    });
    fireEvent.change(screen.getByLabelText("Id"), {
      target: { value: "openrouter" },
    });
    fireEvent.change(screen.getByLabelText("Base URL"), {
      target: { value: "https://openrouter.ai/api/v1" },
    });
    fireEvent.change(screen.getByLabelText("Model id"), {
      target: { value: "anthropic/claude-sonnet-4.5" },
    });
    fireEvent.change(screen.getByLabelText("API key"), {
      target: { value: "sk-or-secret" },
    });
    fireEvent.click(screen.getByRole("button", { name: /Save provider/ }));

    await waitFor(() => {
      expect(host.saveAiProvider).toHaveBeenCalledWith({
        id: "openrouter",
        display_name: "OpenRouter",
        route: "openai_compatible",
        base_url: "https://openrouter.ai/api/v1",
        model_id: "anthropic/claude-sonnet-4.5",
        api_key: "sk-or-secret",
      });
    });
  });

  it("keeps the stored key when an edit leaves the key field blank", async () => {
    const stored = {
      connections: [
        {
          id: "OPENROUTER",
          display_name: "OpenRouter",
          route: "openai_compatible" as const,
          base_url: "https://openrouter.ai/api/v1",
          model_id: "anthropic/claude-sonnet-4.5",
          has_api_key: true,
          is_active: true,
        },
      ],
      active_id: "OPENROUTER",
      file_path: "C:\\Users\\engineer\\.quantix\\.env",
    };
    host.inspectAiProviders.mockResolvedValue(stored);
    host.saveAiProvider.mockResolvedValue(stored);
    renderSettings();
    fireEvent.click(screen.getByRole("button", { name: "AI Providers" }));
    fireEvent.click(
      await screen.findByRole("button", { name: "Edit OpenRouter" }),
    );

    fireEvent.change(screen.getByLabelText("Model id"), {
      target: { value: "openai/gpt-5.3" },
    });
    fireEvent.click(screen.getByRole("button", { name: /Save provider/ }));

    await waitFor(() => {
      expect(host.saveAiProvider).toHaveBeenCalledWith(
        expect.objectContaining({ id: "OPENROUTER", api_key: null }),
      );
    });
  });

  it("reports what a provider test found", async () => {
    host.inspectAiProviders.mockResolvedValue({
      connections: [
        {
          id: "OPENROUTER",
          display_name: "OpenRouter",
          route: "openai_compatible",
          base_url: "https://openrouter.ai/api/v1",
          model_id: "anthropic/claude-sonnet-4.5",
          has_api_key: true,
          is_active: true,
        },
      ],
      active_id: "OPENROUTER",
      file_path: "C:\Users\engineer\.quantix\.env",
    });
    host.probeAiProvider.mockResolvedValue({
      reached: false,
      summary:
        "The provider rejected the API key. Check the key and try again.",
    });
    renderSettings();
    fireEvent.click(screen.getByRole("button", { name: "AI Providers" }));
    fireEvent.click(await screen.findByRole("button", { name: /Test/ }));

    await waitFor(() => {
      expect(host.probeAiProvider).toHaveBeenCalledWith("OPENROUTER");
    });
    expect(
      await screen.findByText(
        "The provider rejected the API key. Check the key and try again.",
      ),
    ).toBeTruthy();
  });
});
