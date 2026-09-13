import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { beforeEach, vi } from "vitest";
import { ApiContext, createApi, type Schema } from "../api";
import { FactoryReset, ResetRecovery } from "./FactoryReset";

const native = vi.hoisted(() => ({ desktop: true, invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({
  isTauri: () => native.desktop,
  invoke: native.invoke,
}));

const preview: Schema<"ResetPreview"> = {
  supported: true,
  home: "C:/Users/Synthetic/.quantix",
  tender_count: 3,
  artifact_count: 14,
  account_count: 2,
  backup_count: 4,
  blockers: [],
  fingerprint: "fresh-preview",
};
const ready: Schema<"ResetStatus"> = {
  reset_id: "a".repeat(32),
  state: "ready",
  detail: "Ready to remove Quantix data.",
  credentials_cleared: true,
  fingerprint: "fresh-preview",
};

beforeEach(() => {
  native.desktop = true;
  native.invoke.mockReset().mockImplementation(async (command: string) => {
    if (command === "reset_support") return true;
    if (command === "finish_reset") return undefined;
    throw new Error(`Unexpected native command: ${command}`);
  });
});

function setup(
  options: {
    preview?: Schema<"ResetPreview">;
    status?: Schema<"ResetStatus"> | null;
    recovery?: boolean;
    post?: () => Promise<Response>;
    readStatus?: () => Promise<Response>;
  } = {},
) {
  let currentPreview = options.preview ?? preview;
  let status = options.status === undefined ? ready : options.status;
  const reads: string[] = [];
  const writes: Array<{ path: string; body: unknown }> = [];
  const events: string[] = [];
  const api = createApi(
    { base_url: "http://localhost/api", token: "synthetic" },
    async (address, init) => {
      const path = new URL(String(address)).pathname.replace(/^\/api/, "");
      events.push(`${init?.method} ${path}`);
      if (init?.method === "POST") {
        writes.push({ path, body: JSON.parse(String(init.body)) });
        return options.post ? options.post() : Response.json(ready);
      }
      reads.push(path);
      if (path === "/reset/preview") return Response.json(currentPreview);
      if (path === "/reset/status")
        return options.readStatus
          ? options.readStatus()
          : Response.json(status);
      throw new Error(`Unexpected read: ${path}`);
    },
  );
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  client.setQueryData(["synthetic-tender-data"], {
    content: "private cached content",
  });
  const ui = render(
    <QueryClientProvider client={client}>
      <ApiContext.Provider value={api}>
        {options.recovery ? <ResetRecovery /> : <FactoryReset />}
      </ApiContext.Provider>
    </QueryClientProvider>,
  );
  return {
    ...ui,
    writes,
    reads,
    events,
    client,
    user: userEvent.setup({ delay: null }),
    setPreview: (next: Schema<"ResetPreview">) => {
      currentPreview = next;
    },
    setStatus: (next: Schema<"ResetStatus"> | null) => {
      status = next;
    },
  };
}

async function openDialog(user: ReturnType<typeof userEvent.setup>) {
  await user.click(
    await screen.findByRole("button", { name: "Reset Quantix…" }),
  );
  return screen.findByRole("dialog", { name: "Reset Quantix?" });
}

it("requires exact typed confirmation with the folder and counts visible, and cancels without changes", async () => {
  localStorage.setItem("quantix-theme", "dark");
  const { user, writes, client } = setup();
  const dialog = await openDialog(user);
  expect(within(dialog).getByText(preview.home)).toBeVisible();
  for (const count of ["3", "14", "2", "4"])
    expect(within(dialog).getByText(count)).toBeVisible();
  const confirm = within(dialog).getByRole("button", {
    name: "Reset and close",
  });
  expect(confirm).toBeDisabled();
  await user.type(screen.getByLabelText("Type RESET to confirm"), "reset");
  expect(confirm).toBeDisabled();
  await user.clear(screen.getByLabelText("Type RESET to confirm"));
  await user.type(screen.getByLabelText("Type RESET to confirm"), "RESET");
  expect(confirm).toBeEnabled();
  expect(
    Object.values(localStorage).some((value) => value.includes("RESET")),
  ).toBe(false);
  await user.click(within(dialog).getByRole("button", { name: "Cancel" }));
  expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  expect(writes).toEqual([]);
  expect(localStorage.getItem("quantix-theme")).toBe("dark");
  expect(client.getQueryData(["synthetic-tender-data"])).toBeDefined();
  const reopened = await openDialog(user);
  expect(within(reopened).getByLabelText("Type RESET to confirm")).toHaveValue(
    "",
  );
});

it("clears only Quantix storage and cached records after acceptance, then passes the exact reset ID to native cleanup", async () => {
  for (const key of [
    "quantix-theme",
    "quantix.form-draft.v1:synthetic",
    "quantix.manager-draft.v1:one",
    "quantix.plan-review-note.v1:one",
  ])
    localStorage.setItem(key, "private synthetic data");
  localStorage.setItem("another-app-preference", "keep");
  const { user, writes, client } = setup();
  await openDialog(user);
  await user.type(screen.getByLabelText("Type RESET to confirm"), "RESET");
  await user.dblClick(screen.getByRole("button", { name: "Reset and close" }));
  await screen.findByText(/Quantix is closing to remove/);
  expect(writes).toEqual([
    {
      path: "/reset",
      body: { fingerprint: preview.fingerprint, confirmation: "RESET" },
    },
  ]);
  expect(native.invoke).toHaveBeenCalledWith("finish_reset", {
    resetId: ready.reset_id,
  });
  expect(
    native.invoke.mock.calls.filter(([command]) => command === "finish_reset"),
  ).toHaveLength(1);
  expect(Object.keys(localStorage)).toEqual(["another-app-preference"]);
  expect(client.getQueryCache().getAll()).toHaveLength(0);
  expect(
    screen.queryByRole("button", { name: "Cancel" }),
  ).not.toBeInTheDocument();
});

it("does not read reset data or send a reset from a browser preview", async () => {
  native.desktop = false;
  const { reads, writes } = setup();
  expect(screen.getByText(/Open the Quantix desktop app/)).toBeVisible();
  expect(
    screen.queryByRole("button", { name: "Reset Quantix…" }),
  ).not.toBeInTheDocument();
  expect(reads).toEqual([]);
  expect(writes).toEqual([]);
  expect(native.invoke).not.toHaveBeenCalled();
});

it("blocks the flow when the native executable lacks reset_support", async () => {
  native.invoke.mockRejectedValue("unknown command reset_support");
  const { reads, writes } = setup();
  expect(
    await screen.findByText(/This desktop app cannot finish a reset/),
  ).toBeVisible();
  expect(reads).toEqual([]);
  expect(writes).toEqual([]);
});

it("rechecks native support before POST and preserves confirmation when that check fails", async () => {
  const { user, writes, client } = setup();
  await openDialog(user);
  await user.type(screen.getByLabelText("Type RESET to confirm"), "RESET");
  native.invoke.mockRejectedValue("unknown command reset_support");
  await user.click(screen.getByRole("button", { name: "Reset and close" }));
  await screen.findByText(/This desktop app cannot finish a reset/);
  expect(writes).toEqual([]);
  expect(screen.getByLabelText("Type RESET to confirm")).toHaveValue("RESET");
  expect(client.getQueryData(["synthetic-tender-data"])).toBeDefined();
});

it("disables confirmation during active work and refreshes its blockers", async () => {
  const { user, writes, setPreview } = setup({
    preview: { ...preview, blockers: ["An AI account setup is running."] },
  });
  await openDialog(user);
  await user.type(screen.getByLabelText("Type RESET to confirm"), "RESET");
  expect(screen.getByText("An AI account setup is running.")).toBeVisible();
  expect(
    screen.getByRole("button", { name: "Reset and close" }),
  ).toBeDisabled();
  setPreview({ ...preview, fingerprint: "new-idle-preview" });
  await user.click(screen.getByRole("button", { name: "Check again" }));
  await waitFor(() =>
    expect(screen.getByLabelText("Type RESET to confirm")).toHaveValue(""),
  );
  expect(writes).toEqual([]);
});

it("shows a stale-preview rejection and requires confirmation against the refreshed counts", async () => {
  const { user, setPreview, writes } = setup({
    status: null,
    post: async () =>
      Response.json(
        { detail: "Saved data changed. Review the reset again." },
        { status: 409 },
      ),
  });
  await openDialog(user);
  await user.type(screen.getByLabelText("Type RESET to confirm"), "RESET");
  setPreview({ ...preview, tender_count: 7, fingerprint: "new-fingerprint" });
  await user.click(screen.getByRole("button", { name: "Reset and close" }));
  await screen.findByText("Saved data changed. Review the reset again.");
  await waitFor(() =>
    expect(screen.getByLabelText("Type RESET to confirm")).toHaveValue(""),
  );
  expect(within(screen.getByRole("dialog")).getByText("7")).toBeVisible();
  expect(writes).toHaveLength(1);
  expect(native.invoke).not.toHaveBeenCalledWith(
    "finish_reset",
    expect.anything(),
  );
});

it("keeps an uncertain reset locked until its status can be checked", async () => {
  let canRead = false;
  const { user, writes } = setup({
    post: async () => {
      throw new TypeError("connection lost");
    },
    readStatus: async () => {
      if (!canRead) throw new TypeError("connection lost");
      return Response.json({
        ...ready,
        state: "credential_error",
        credentials_cleared: false,
      });
    },
  });
  await openDialog(user);
  await user.type(screen.getByLabelText("Type RESET to confirm"), "RESET");
  await user.click(screen.getByRole("button", { name: "Reset and close" }));
  await screen.findByText(/The reset may already be confirmed/);
  await user.keyboard("{Escape}");
  expect(screen.getByRole("dialog")).toBeVisible();
  expect(
    screen.queryByRole("button", { name: "Cancel" }),
  ).not.toBeInTheDocument();
  canRead = true;
  await user.click(screen.getByRole("button", { name: "Check reset status" }));
  expect(
    await screen.findByRole("heading", { name: "Finish resetting Quantix" }),
  ).toBeVisible();
  expect(writes).toHaveLength(1);
});

it("waits for an explicit retry of failed credentials and uses the confirmed status fingerprint", async () => {
  const status = {
    ...ready,
    state: "credential_error" as const,
    credentials_cleared: false,
    fingerprint: "accepted-before-restart",
    detail: "Close the AI sign-in window and retry.",
  };
  const { user, writes, setStatus } = setup({
    recovery: true,
    status,
    post: async () => {
      setStatus(ready);
      return Response.json(ready);
    },
  });
  await screen.findByText(status.detail);
  expect(writes).toEqual([]);
  expect(native.invoke).not.toHaveBeenCalledWith(
    "finish_reset",
    expect.anything(),
  );
  await user.click(
    screen.getByRole("button", { name: "Retry reset and close" }),
  );
  await screen.findByText(/Quantix is closing to remove/);
  expect(writes).toEqual([
    {
      path: "/reset",
      body: { fingerprint: status.fingerprint, confirmation: "RESET" },
    },
  ]);
});

it.each(["ready", "failed", "deleting"] as const)(
  "finishes a %s reset only after explicit recovery retry, without deleting credentials again",
  async (state) => {
    const { user, writes } = setup({
      recovery: true,
      status: { ...ready, state },
    });
    await screen.findByRole("button", { name: "Retry reset and close" });
    expect(native.invoke).not.toHaveBeenCalled();
    await user.click(
      screen.getByRole("button", { name: "Retry reset and close" }),
    );
    await screen.findByText(/Quantix is closing to remove/);
    expect(writes).toEqual([]);
    expect(native.invoke).toHaveBeenCalledWith("finish_reset", {
      resetId: ready.reset_id,
    });
  },
);

it("retains native cleanup launch errors and retries without returning to Settings", async () => {
  native.invoke.mockImplementation(async (command: string) => {
    if (command === "reset_support") return true;
    throw "Close the other Quantix window and retry.";
  });
  const { user, writes } = setup();
  await openDialog(user);
  await user.type(screen.getByLabelText("Type RESET to confirm"), "RESET");
  await user.click(screen.getByRole("button", { name: "Reset and close" }));
  await screen.findByText("Close the other Quantix window and retry.");
  expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  expect(
    screen.queryByRole("button", { name: "Cancel" }),
  ).not.toBeInTheDocument();
  native.invoke.mockResolvedValue(true);
  await user.click(
    screen.getByRole("button", { name: "Retry reset and close" }),
  );
  await screen.findByText(/Quantix is closing to remove/);
  expect(writes).toHaveLength(1);
});
