import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { afterEach, beforeEach, vi } from "vitest";
import App from "./App";

// The side pane now starts collapsed. These tests are about what it does once
// it is open, so they restore the saved "open" preference the app honours.
beforeEach(() => {
  localStorage.setItem("quantix.right-workspace.v2", "split");
});

const native = vi.hoisted(() => ({ desktop: false, invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({
  isTauri: () => native.desktop,
  invoke: native.invoke,
}));

beforeEach(() => {
  window.location.hash = "";
  native.desktop = false;
  native.invoke.mockReset();
});

afterEach(() => {
  vi.unstubAllEnvs();
  vi.unstubAllGlobals();
});

it("gates a pending reset before any ordinary workspace queries mount", async () => {
  vi.stubEnv("VITE_QUANTIX_API_BASE", "http://localhost/api");
  vi.stubEnv("VITE_QUANTIX_API_TOKEN", "test");
  window.location.hash = "#/settings?section=preferences";
  const reads: string[] = [];
  vi.stubGlobal("fetch", async (url: string) => {
    reads.push(new URL(url).pathname);
    if (url.endsWith("/health")) return Response.json({ reset_pending: true });
    if (url.endsWith("/reset/status"))
      return Response.json({
        reset_id: "a".repeat(32),
        state: "credential_error",
        credentials_cleared: false,
        fingerprint: "accepted-fingerprint",
        detail: "Close the AI sign-in window and retry.",
      });
    throw new Error(`Ordinary workspace query mounted: ${url}`);
  });
  render(
    <QueryClientProvider
      client={
        new QueryClient({ defaultOptions: { queries: { retry: false } } })
      }
    >
      <App />
    </QueryClientProvider>,
  );
  expect(
    await screen.findByRole("heading", { name: "Finish resetting Quantix" }),
  ).toBeVisible();
  expect(
    await screen.findByText("Close the AI sign-in window and retry."),
  ).toBeVisible();
  expect(
    reads.every(
      (path) => path === "/api/health" || path === "/api/reset/status",
    ),
  ).toBe(true);
  expect(
    screen.queryByRole("button", { name: "New tender" }),
  ).not.toBeInTheDocument();
  expect(
    screen.queryByRole("heading", { name: "Settings" }),
  ).not.toBeInTheDocument();
});

it("keeps recovery mounted after an accepted reset clears the connection cache", async () => {
  native.desktop = true;
  native.invoke.mockImplementation(async (command: string) => {
    if (command === "connection_info")
      return { base_url: "http://localhost/api", token: "synthetic" };
    if (command === "reset_support") return true;
    if (command === "finish_reset")
      throw "Close the other Quantix window and retry.";
    throw new Error(`Unexpected native command: ${command}`);
  });
  window.location.hash = "#/settings?section=reset";
  let accepted = false;
  const afterAcceptance: string[] = [];
  const status = {
    reset_id: "b".repeat(32),
    state: "ready",
    credentials_cleared: true,
    fingerprint: "confirmed",
    detail: "Ready to remove Quantix data.",
  };
  vi.stubGlobal("fetch", async (url: string, init: RequestInit) => {
    const path = new URL(url).pathname.replace(/^\/api/, "");
    if (accepted) afterAcceptance.push(path);
    if (path === "/reset" && init.method === "POST") {
      accepted = true;
      return Response.json(status);
    }
    if (path === "/reset/status") return Response.json(status);
    if (path === "/reset/preview")
      return Response.json({
        supported: true,
        home: "C:/synthetic/.quantix",
        tender_count: 0,
        artifact_count: 0,
        account_count: 0,
        backup_count: 0,
        blockers: [],
        fingerprint: "confirmed",
      });
    if (path === "/health")
      return Response.json({
        workspace_revision: 2,
        ai_setup_revision: 7,
        capabilities: ["factory_reset"],
        reset_pending: accepted,
      });
    if (path === "/settings")
      return Response.json({
        home: "C:/synthetic/.quantix",
        default_currency: "EGP",
      });
    if (path === "/tenders") return Response.json([]);
    throw new Error(`Unexpected read: ${path}`);
  });
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  render(
    <QueryClientProvider client={client}>
      <App />
    </QueryClientProvider>,
  );
  const user = userEvent.setup({ delay: null });
  await user.click(
    await screen.findByRole("button", { name: "Reset Quantix…" }),
  );
  await user.type(screen.getByLabelText("Type RESET to confirm"), "RESET");
  await user.click(screen.getByRole("button", { name: "Reset and close" }));
  await screen.findByText("Close the other Quantix window and retry.");
  expect(
    screen.getByRole("heading", { name: "Finish resetting Quantix" }),
  ).toBeVisible();
  expect(
    screen.queryByRole("heading", { name: "Settings" }),
  ).not.toBeInTheDocument();
  expect(
    screen.queryByRole("button", { name: "New tender" }),
  ).not.toBeInTheDocument();
  expect(client.getQueryCache().getAll()).toHaveLength(0);
  window.location.hash = "#/settings?section=accounts";
  await waitFor(() =>
    expect(
      screen.getByRole("heading", { name: "Finish resetting Quantix" }),
    ).toBeVisible(),
  );
  expect(afterAcceptance.every((path) => path === "/reset/status")).toBe(true);
});

it("preserves an engineer draft while inspecting Work and returning to Manager", async () => {
  vi.stubEnv("VITE_QUANTIX_API_BASE", "http://localhost/api");
  vi.stubEnv("VITE_QUANTIX_API_TOKEN", "test");
  const tender = {
    id: "one",
    name: "Drainage works",
    status: "active",
    revision: 1,
    created_at: "",
    updated_at: "",
  };
  vi.stubGlobal("fetch", async (url: string) => {
    let data: unknown = [];
    if (url.endsWith("/tenders")) data = [tender];
    else if (url.endsWith("/tenders/one"))
      data = {
        tender,
        artifact_count: 0,
        evidence_count: 0,
        coverage: {
          registered: 0,
          extracted: 0,
          needs_attention: 0,
          unsupported: 0,
          failed: 0,
        },
        areas: [],
        findings: [],
        plan: null,
        active_runs: [],
      };
    else if (url.endsWith("/settings"))
      data = {
        provider_ready: true,
        model: "gpt-6-astra",
        home: "C:/local",
        default_currency: "EGP",
      };
    else if (url.endsWith("/health"))
      data = {
        version: "test",
        workspace_revision: 2,
        ai_setup_revision: 7,
        provider_ready: true,
        model: "gpt-6-astra",
        home: "C:/local",
        capabilities: [],
      };
    else if (url.endsWith("/tenders/one/ai-policy"))
      data = {
        tender_id: "one",
        revision: 1,
        allowed_connection_ids: [],
        manager: null,
        specialist: null,
        role_routes: {},
        fallback_routes: [],
        run_budget_usd: null,
        tender_budget_usd: null,
        max_requests: 10,
        rationale: "",
        updated_at: null,
        spent_usd: 0,
        reserved_usd: 0,
        restore_reconciliation_required: false,
        spend_history_may_be_incomplete: false,
      };
    return new Response(JSON.stringify(data));
  });
  const user = userEvent.setup();
  render(
    <QueryClientProvider
      client={
        new QueryClient({ defaultOptions: { queries: { retry: false } } })
      }
    >
      <App />
    </QueryClientProvider>,
  );
  const input = await screen.findByRole("textbox", {
    name: "Message to Tender Manager",
  });
  await user.type(input, "Check the drainage quantities first");
  await user.click(screen.getByRole("button", { name: "Work" }));
  expect(
    await screen.findByRole("navigation", { name: "Work sections" }),
  ).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "Manager" }));
  expect(
    screen.getByRole("textbox", { name: "Message to Tender Manager" }),
  ).toHaveValue("Check the drainage quantities first");
  await user.click(screen.getByRole("button", { name: "Settings" }));
  await user.click(
    await screen.findByRole("menuitem", { name: "All settings" }),
  );
  expect(
    await screen.findByRole("heading", { name: "Settings" }),
  ).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "Back to the tender" }));
  expect(
    await screen.findByRole("textbox", { name: "Message to Tender Manager" }),
  ).toHaveValue("Check the drainage quantities first");
});

it("keeps the tender read-only when the local workspace revision is incompatible", async () => {
  vi.stubEnv("VITE_QUANTIX_API_BASE", "http://localhost/api");
  vi.stubEnv("VITE_QUANTIX_API_TOKEN", "test");
  const tender = {
    id: "blocked",
    name: "Blocked tender",
    status: "active",
    revision: 1,
    created_at: "",
    updated_at: "",
  };
  vi.stubGlobal("fetch", async (url: string) => {
    let data: unknown = [];
    if (url.endsWith("/tenders")) data = [tender];
    else if (url.endsWith("/tenders/blocked"))
      data = {
        tender,
        artifact_count: 1,
        evidence_count: 1,
        coverage: {
          registered: 1,
          extracted: 1,
          needs_attention: 0,
          unsupported: 0,
          failed: 0,
        },
        areas: [],
        findings: [],
        plan: null,
        active_runs: [],
        boq_count: 0,
      };
    else if (url.endsWith("/health"))
      data = {
        version: "old",
        workspace_revision: 0,
        ai_setup_revision: 7,
        provider_ready: true,
        model: "old",
        home: "C:/local",
        capabilities: [],
      };
    else if (url.endsWith("/settings"))
      data = {
        provider_ready: true,
        model: "old",
        home: "C:/local",
        default_currency: "EGP",
      };
    return new Response(JSON.stringify(data));
  });
  render(
    <QueryClientProvider
      client={
        new QueryClient({ defaultOptions: { queries: { retry: false } } })
      }
    >
      <App />
    </QueryClientProvider>,
  );
  expect(
    await screen.findByRole("heading", { name: "Workspace update required" }),
  ).toBeInTheDocument();
  expect(
    screen.queryByRole("heading", { name: "Tender Manager" }),
  ).not.toBeInTheDocument();
  const user = userEvent.setup();
  for (const section of ["Documents", "Work", "Estimate", "Submission"]) {
    await user.click(screen.getByRole("button", { name: section }));
    expect(
      screen.getByRole("heading", { name: "Workspace update required" }),
    ).toBeVisible();
    expect(
      screen.queryByRole("navigation", { name: /sections/ }),
    ).not.toBeInTheDocument();
  }
  await user.click(screen.getByRole("button", { name: "Settings" }));
  await user.click(
    await screen.findByRole("menuitem", { name: "All settings" }),
  );
  expect(
    await screen.findByRole("heading", { name: "Settings" }),
  ).toBeInTheDocument();
});

it("opens the current document inline and preserves Manager conversation context in the balanced route", async () => {
  vi.stubEnv("VITE_QUANTIX_API_BASE", "http://localhost/api");
  vi.stubEnv("VITE_QUANTIX_API_TOKEN", "test");
  const tender = {
    id: "one",
    name: "Drainage works",
    status: "active",
    revision: 1,
    created_at: "",
    updated_at: "",
  };
  const artifact = {
    id: "workbook",
    tender_id: "one",
    name: "Quantities.xlsx",
    relative_path: "BOQ/Quantities.xlsx",
    kind: "spreadsheet",
    version: 3,
    content_hash: "workbook-hash",
    size: 2,
    status: "extracted",
    area: "East",
    metadata: { sheets: [{ name: "Main" }] },
    warnings: [],
    is_current: true,
    created_at: "",
  };
  vi.stubGlobal("fetch", async (url: string) => {
    const path = new URL(url).pathname;
    let data: unknown = [];
    if (path.endsWith("/tenders")) data = [tender];
    else if (path.endsWith("/tenders/one"))
      data = {
        tender,
        artifact_count: 1,
        evidence_count: 0,
        coverage: {
          registered: 1,
          extracted: 1,
          needs_attention: 0,
          unsupported: 0,
          failed: 0,
        },
        areas: ["East"],
        findings: [],
        plan: null,
        active_runs: [],
        boq_count: 0,
      };
    else if (path.endsWith("/artifacts")) data = [artifact];
    else if (path.endsWith("/settings"))
      data = { home: "C:/local", default_currency: "EGP" };
    else if (path.endsWith("/health"))
      data = {
        version: "test",
        workspace_revision: 2,
        office_revision: 2,
        ai_setup_revision: 7,
        provider_ready: false,
        model: "",
        home: "C:/local",
        capabilities: [],
      };
    else if (path.endsWith("/ai-policy"))
      data = {
        tender_id: "one",
        revision: 1,
        allowed_connection_ids: [],
        manager: null,
        specialist: null,
        role_routes: {},
        fallback_routes: [],
        run_budget_usd: null,
        tender_budget_usd: null,
        max_requests: 10,
        rationale: "",
        spent_usd: 0,
        reserved_usd: 0,
        restore_reconciliation_required: false,
        spend_history_may_be_incomplete: false,
      };
    else if (path.endsWith("/messages") || path.endsWith("/runs")) data = [];
    else if (path.endsWith("/pending-message")) data = null;
    else if (path.endsWith("/evidence")) data = [];
    return Response.json(data);
  });
  render(
    <QueryClientProvider
      client={
        new QueryClient({ defaultOptions: { queries: { retry: false } } })
      }
    >
      <App />
    </QueryClientProvider>,
  );
  const user = userEvent.setup();
  expect(
    await screen.findByRole("textbox", { name: "Message to Tender Manager" }),
  ).toBeInTheDocument();
  const showWorkspace = screen.queryByRole("button", {
    name: "Show workspace",
  });
  if (showWorkspace) await user.click(showWorkspace);
  await user.click(
    within(
      screen.getByRole("complementary", { name: "Tender workspace" }),
    ).getByRole("button", { name: /^Documents/ }),
  );
  expect(
    screen.getByRole("region", { name: "Workspace documents" }),
  ).toHaveTextContent("Quantities.xlsx");
  await user.click(screen.getByRole("button", { name: /^Quantities.xlsx/ }));
  expect(
    await screen.findByRole("region", { name: "Source document" }),
  ).toHaveTextContent("Quantities.xlsx");
  expect(window.location.hash).toContain("artifact_id=workbook");
  expect(window.location.hash).toContain("origin=%2Ftenders%2Fone%2Fmanager");
  expect(
    screen.getByRole("textbox", { name: "Message to Tender Manager" }),
  ).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "Hide workspace" }));
  expect(
    screen.queryByRole("region", { name: "Source document" }),
  ).not.toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "Show workspace" }));
  expect(
    screen.getByRole("region", { name: "Source document" }),
  ).toHaveTextContent("Quantities.xlsx");
});
