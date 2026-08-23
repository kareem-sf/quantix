import { clearMocks } from "@tauri-apps/api/mocks";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  installBrowserPreviewHost,
  invokeBrowserPreviewHost,
} from "./browserPreviewHost";

const defaultMatchMedia = window.matchMedia;
const defaultPreferences = {
  appearance: "system" as const,
  reduced_motion: false,
  larger_text: false,
  notify_when_attention_needed: false,
};

function setPreviewViewport({ wide }: { wide: boolean }) {
  Object.defineProperty(window, "matchMedia", {
    configurable: true,
    writable: true,
    value: (query: string): MediaQueryList => ({
      matches:
        query === "(min-width: 1280px)" ? wide : query === "(min-width: 820px)",
      media: query,
      onchange: null,
      addListener: () => undefined,
      removeListener: () => undefined,
      addEventListener: () => undefined,
      removeEventListener: () => undefined,
      dispatchEvent: () => false,
    }),
  });
}

describe("browser preview Host", () => {
  beforeEach(() => {
    vi.resetModules();
    invokeBrowserPreviewHost("update_general_application_preferences", {
      command: { preferences: defaultPreferences },
    });
  });

  afterEach(() => {
    cleanup();
    clearMocks();
    vi.resetModules();
    Object.defineProperty(window, "matchMedia", {
      configurable: true,
      writable: true,
      value: defaultMatchMedia,
    });
    window.history.replaceState(null, "", "/");
    delete document.documentElement.dataset.quantixRuntime;
    delete document.documentElement.dataset.quantixAppearance;
    document.documentElement.classList.remove(
      "quantix-reduced-motion",
      "quantix-larger-text",
    );
  });

  it("opens the real workspace shell without a native Tauri Host", async () => {
    await installBrowserPreviewHost();
    const { default: App } = await import("./App");

    render(<App />);

    expect(await screen.findByTestId("manager-workspace")).toBeTruthy();
    expect(
      screen.getByRole("button", { name: "Choose Tender Package" }),
    ).toBeTruthy();
    expect(screen.queryByText("Quantix could not open")).toBeNull();
    expect(document.documentElement.dataset.quantixRuntime).toBe(
      "browser-preview",
    );
  }, 30_000);

  it("treats the browser-only package action like a cancelled native picker", async () => {
    await installBrowserPreviewHost();
    const { default: App } = await import("./App");
    render(<App />);

    const choosePackage = await screen.findByRole("button", {
      name: "Choose Tender Package",
    });
    fireEvent.click(choosePackage);
    fireEvent.click(
      await screen.findByRole("button", { name: "Continue without AI" }),
    );

    await waitFor(() =>
      expect(
        screen.getByRole("button", { name: "Choose Tender Package" }),
      ).toBe(document.activeElement),
    );
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("reports no native package operation in browser preview", () => {
    expect(
      invokeBrowserPreviewHost("inspect_package_intake_progress"),
    ).toBeNull();
    expect(
      invokeBrowserPreviewHost("cancel_package_intake", {
        command: { operation_id: "package-intake-test" },
      }),
    ).toBe(false);
  });

  it("seeds an isolated recovery-required Tender preview with Host recovery calls", () => {
    window.history.replaceState(null, "", "/?tender-recovery-preview=1");

    const workspace = invokeBrowserPreviewHost("inspect_manager_workspace") as {
      catalogue: Array<{ tender_id: string; state: string }>;
    };
    expect(workspace.catalogue).toHaveLength(2);
    expect(
      workspace.catalogue.every(
        (tender) => tender.state === "recovery_required",
      ),
    ).toBe(true);

    const tenderId = workspace.catalogue[0]!.tender_id;
    expect(
      invokeBrowserPreviewHost("inspect_tender_integrity", {
        command: { tender_id: tenderId },
      }),
    ).toMatchObject({
      tender_id: tenderId,
      state: "recovery_required",
      issues: ["database_integrity_invalid", "audit_chain_invalid"],
    });
    expect(
      invokeBrowserPreviewHost("inspect_tender_backups", {
        command: { tender_id: tenderId },
      }),
    ).toHaveLength(1);
    expect(
      invokeBrowserPreviewHost("inspect_tender_recoveries", {
        command: { tender_id: tenderId },
      }),
    ).toEqual([]);

    const prepared = invokeBrowserPreviewHost("prepare_tender_recovery", {
      command: { tender_id: tenderId, backup_id: "b".repeat(32) },
    }) as { state: string; tender_id: string };
    expect(prepared).toMatchObject({
      tender_id: tenderId,
      state: "awaiting_approval",
    });
    expect(
      invokeBrowserPreviewHost("inspect_tender_recoveries", {
        command: { tender_id: workspace.catalogue[1]!.tender_id },
      }),
    ).toEqual([]);
    expect(
      invokeBrowserPreviewHost("resolve_tender_recovery", {
        command: {
          tender_id: tenderId,
          recovery_id: "r".repeat(32),
          decision: "approve_replacement",
          rationale: "Verified browser preview backup",
        },
      }),
    ).toMatchObject({ state: "applied" });

    const selected = invokeBrowserPreviewHost(
      "select_manager_workspace_tender",
      { command: { tender_id: tenderId } },
    ) as {
      selected_tender: { tender_id: string; state: string } | null;
    };
    expect(selected.selected_tender).toMatchObject({
      tender_id: tenderId,
      state: "active",
    });
  });

  it("supports recovery-specific Trash, restore, and permanent purge commands without exposing provider refs", () => {
    window.history.replaceState(null, "", "/?tender-recovery-preview=1");
    const workspace = invokeBrowserPreviewHost("inspect_manager_workspace") as {
      catalogue: Array<{ tender_id: string; name: string; state: string }>;
    };
    const tender = workspace.catalogue[0]!;

    const trashed = invokeBrowserPreviewHost("trash_recovery_required_tender", {
      command: {
        tender_id: tender.tender_id,
        rationale: "Remove the damaged local Store.",
      },
    }) as { tender_id: string; deletion_source: string; state: string };
    expect(trashed).toMatchObject({
      tender_id: tender.tender_id,
      deletion_source: "recovery_required",
      state: "trashed",
    });

    const restored = invokeBrowserPreviewHost("restore_trashed_tender", {
      command: {
        deletion_id: "d".repeat(32),
        rationale: "Return it for recovery inspection.",
      },
    }) as { state: string; deletion_source: string };
    expect(restored).toMatchObject({
      state: "restored",
      deletion_source: "recovery_required",
    });

    const receipt = invokeBrowserPreviewHost("purge_recovery_required_tender", {
      command: {
        tender_id: tender.tender_id,
        rationale: "Erase the corrupted Quantix Store.",
        confirmation_tender_name: tender.name,
      },
    }) as {
      deletion_source: string;
      provider_cleanup_status: string;
      provider_thread_refs?: unknown;
      thread_refs?: unknown;
    };
    expect(receipt).toMatchObject({
      deletion_source: "recovery_required",
      provider_cleanup_status: "incomplete",
    });
    expect(receipt).not.toHaveProperty("provider_thread_refs");
    expect(receipt).not.toHaveProperty("thread_refs");
  });

  it("keeps local preview preferences inside the preview Host", () => {
    const updated = invokeBrowserPreviewHost(
      "update_general_application_preferences",
      {
        command: {
          preferences: {
            appearance: "dark",
            reduced_motion: true,
            larger_text: true,
            notify_when_attention_needed: false,
          },
        },
      },
    );

    expect(updated).toMatchObject({
      general_preferences: {
        appearance: "dark",
        reduced_motion: true,
        larger_text: true,
      },
    });
  });

  it("exposes a seeded Tender workspace for browser acceptance", async () => {
    window.history.replaceState(null, "", "/?workspace-preview=1");
    await installBrowserPreviewHost();
    const { default: App } = await import("./App");
    render(<App />);

    expect(
      await screen.findByRole("heading", {
        name: "North District Civic Centre",
      }),
    ).toBeTruthy();
    const composer = screen.getByRole("textbox", {
      name: "Message your Tendering Manager",
    });
    fireEvent.change(composer, {
      target: { value: "Browser acceptance message" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Send message" }));

    expect(await screen.findByText("Browser acceptance message")).toBeTruthy();

    fireEvent.click(
      screen.getByRole("button", { name: "Show Tender workspace" }),
    );
    const workspace = await screen.findByRole("dialog", {
      name: "Tender workspace",
    });
    expect(
      within(workspace).getByRole("button", { name: "Work" }),
    ).toBeTruthy();
    expect(
      within(workspace).getByRole("button", { name: "Team" }),
    ).toBeTruthy();
    expect(
      within(workspace).getByRole("button", { name: "Files" }),
    ).toBeTruthy();

    fireEvent.change(
      within(workspace).getByRole("searchbox", { name: "Search this Tender" }),
      {
        target: { value: "AI" },
      },
    );
    fireEvent.click(within(workspace).getByRole("button", { name: "Search" }));
    const result = await within(workspace).findByRole("button", {
      name: /Tender AI is waiting/,
    });
    fireEvent.click(result);
    await waitFor(() =>
      expect(screen.queryByText("Tender AI is waiting")).toBeNull(),
    );
  });

  it("keeps Tender AI Settings choices out of the conversation workspace", async () => {
    window.history.replaceState(null, "", "/?workspace-preview=1");
    await installBrowserPreviewHost();
    const { default: App } = await import("./App");
    render(<App />);

    expect(
      await screen.findByRole("textbox", {
        name: "Message your Tendering Manager",
      }),
    ).toBeTruthy();
    expect(
      screen.queryByRole("group", { name: "Tender AI selection" }),
    ).toBeNull();
    expect(screen.queryByText("Provider", { exact: true })).toBeNull();
    expect(screen.queryByText("Model", { exact: true })).toBeNull();
    expect(screen.queryByText("Reasoning", { exact: true })).toBeNull();
  });

  it("keeps the empty Start Tender surface mounted while the sidebar toggles", async () => {
    await installBrowserPreviewHost();
    const { default: App } = await import("./App");
    render(<App />);

    const startTender = await screen.findByRole("heading", {
      name: "Start a Tender",
    });
    const hideTenders = screen.getByRole("button", { name: "Hide Tenders" });
    fireEvent.click(hideTenders);

    expect(screen.getByRole("button", { name: "Show Tenders" })).toBeTruthy();
    expect(screen.getByRole("heading", { name: "Start a Tender" })).toBe(
      startTender,
    );

    fireEvent.click(screen.getByRole("button", { name: "Show Tenders" }));
    expect(screen.getByRole("button", { name: "Hide Tenders" })).toBeTruthy();
    expect(screen.getByRole("heading", { name: "Start a Tender" })).toBe(
      startTender,
    );
  });

  it("keeps the seeded workspace and composer intact while opening the compact workspace dialog", async () => {
    setPreviewViewport({ wide: false });
    window.history.replaceState(null, "", "/?workspace-preview=1");
    await installBrowserPreviewHost();
    const { default: App } = await import("./App");
    render(<App />);

    expect(
      await screen.findByRole("heading", {
        name: "North District Civic Centre",
      }),
    ).toBeTruthy();
    const composer = screen.getByRole("textbox", {
      name: "Message your Tendering Manager",
    });
    fireEvent.change(composer, { target: { value: "Keep this draft" } });

    const contextTrigger = screen.getByRole("button", {
      name: "Show Tender workspace",
    });
    fireEvent.click(contextTrigger);

    const dialog = await screen.findByRole("dialog", {
      name: "Tender workspace",
    });
    expect(within(dialog).queryByText("Current action")).toBeNull();
    expect(within(dialog).getByText("Tender records")).toBeTruthy();
    expect(
      screen.getByRole("heading", { name: "North District Civic Centre" }),
    ).toBeTruthy();
    expect((composer as HTMLTextAreaElement).value).toBe("Keep this draft");

    fireEvent.keyDown(dialog, { key: "Escape" });
    await waitFor(() => {
      expect(
        screen.queryByRole("dialog", { name: "Tender workspace" }),
      ).toBeNull();
      expect(document.activeElement).toBe(contextTrigger);
    });
  });

  it("seeds the complementary Tender workspace landmark on wide browser previews", async () => {
    setPreviewViewport({ wide: true });
    window.history.replaceState(null, "", "/?workspace-preview=1");
    await installBrowserPreviewHost();
    const { default: App } = await import("./App");
    render(<App />);

    const context = await screen.findByRole("complementary", {
      name: "Tender workspace",
    });
    expect(within(context).queryByText("Current action")).toBeNull();
    expect(within(context).getByText("Tender records")).toBeTruthy();

    fireEvent.click(
      within(context).getByRole("button", {
        name: "Close Tender workspace",
      }),
    );
    await waitFor(() =>
      expect(
        screen.getByRole("button", { name: "Show Tender workspace" }),
      ).toBeTruthy(),
    );
    expect(
      screen.getByTestId("manager-workspace").classList.contains("has-context"),
    ).toBe(false);
    expect(
      screen.getByRole("heading", {
        name: "North District Civic Centre",
        level: 1,
      }),
    ).toBeTruthy();
  });
});
