import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ApiContext, createApi } from "../api";
import { Backups } from "./Backups";
import { vi } from "vitest";

it("recovers a staged restoration from the service in a fresh renderer", async () => {
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async (url) =>
      new Response(
        JSON.stringify(
          String(url).endsWith("/latest")
            ? null
            : String(url).endsWith("/pending")
              ? {
                  status: "restore_ready",
                  restore_id: "restore",
                  backup_id: "saved",
                  restart_required: true,
                  detail: "A checked restoration is waiting for restart.",
                }
              : [],
        ),
      ),
  );
  render(
    <QueryClientProvider client={new QueryClient()}>
      <ApiContext.Provider value={api}>
        <Backups />
      </ApiContext.Provider>
    </QueryClientProvider>,
  );
  expect(
    await screen.findByText("A checked restoration is waiting for restart."),
  ).toBeInTheDocument();
  expect(
    screen.getByRole("button", { name: "Close Quantix for restoration" }),
  ).toBeEnabled();
  expect(screen.getByRole("button", { name: "Check backup" })).toBeDisabled();
  expect(
    screen.queryByRole("button", { name: "Prepare this restoration" }),
  ).not.toBeInTheDocument();
});

const inspection = {
  valid: true,
  compatible: true,
  detail: "The backup is ready to inspect.",
  total_uncompressed_bytes: 100,
  backup: {
    id: "saved",
    filename: "backup.zip",
    created_at: "2026-09-06T10:00:00Z",
    size: 100,
    sha256: "a".repeat(64),
    format_version: 1,
    tender_count: 2,
    original_count: 5,
    unavailable_original_count: 1,
    output_count: 0,
    file_count: 7,
    purpose: "manual",
  },
};

it("creates a backup and saves the selected archive through the authenticated download route", async () => {
  let created = false,
    clickedName = "";
  const createObjectURL = vi.fn((blob: Blob) => {
    expect(blob.type).toBe("application/zip");
    return "blob:backup-test";
  });
  const originalURL = URL;
  vi.stubGlobal(
    "URL",
    class extends originalURL {
      static createObjectURL = createObjectURL;
      static revokeObjectURL = vi.fn();
    },
  );
  const click = vi
    .spyOn(HTMLAnchorElement.prototype, "click")
    .mockImplementation(function (this: HTMLAnchorElement) {
      clickedName = this.download;
    });
  const api = createApi(
    { base_url: "http://localhost/api", token: "backup-token" },
    async (url, init) => {
      if (String(url).endsWith("/pending")) return new Response("null");
      if (String(url).endsWith("/latest")) return new Response("null");
      if (String(url).endsWith("/saved/download")) {
        expect(new Headers(init?.headers).get("Authorization")).toBe(
          "Bearer backup-token",
        );
        return new Response("archive", {
          headers: { "Content-Type": "application/zip" },
        });
      }
      if (init?.method === "POST") {
        created = true;
        return new Response(JSON.stringify(inspection.backup));
      }
      return new Response(JSON.stringify(created ? [inspection.backup] : []));
    },
  );
  try {
    const user = userEvent.setup();
    render(
      <QueryClientProvider client={new QueryClient()}>
        <ApiContext.Provider value={api}>
          <Backups />
        </ApiContext.Provider>
      </QueryClientProvider>,
    );
    await screen.findByText("No backups have been created yet.");
    await user.click(screen.getByRole("button", { name: "Create backup" }));
    expect(await screen.findByRole("status")).toHaveTextContent(
      "Backup created.",
    );
    await user.click(
      screen.getByRole("button", { name: "Save a copy of backup.zip" }),
    );
    expect(clickedName).toBe("backup.zip");
    expect(createObjectURL).toHaveBeenCalledOnce();
    await new Promise((resolve) => setTimeout(resolve, 1100));
  } finally {
    click.mockRestore();
    vi.unstubAllGlobals();
  }
});

it("locks the inspected archive during a check and binds preparation to its exact hash", async () => {
  let finishCheck: (response: Response) => void = () => {};
  let staged = false;
  const readyRecord = {
    status: "restore_ready",
    restore_id: "restore",
    backup_id: "saved",
    restart_required: true,
    detail: "Restoration is prepared.",
  };
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async (url, init) => {
      if (String(url).endsWith("/latest")) return new Response("null");
      if (String(url).endsWith("/pending"))
        return new Response(JSON.stringify(staged ? readyRecord : null));
      if (String(url).endsWith("/inspect"))
        return new Promise<Response>((resolve) => {
          finishCheck = resolve;
        });
      if (String(url).endsWith("/shutdown"))
        return new Response(
          JSON.stringify({ detail: "A current run must stop before closing." }),
          { status: 409 },
        );
      if (String(url).endsWith("/restore")) {
        staged = true;
        expect(JSON.parse(String(init?.body))).toEqual({
          path: "C:/backups/one.zip",
          expected_sha256: "a".repeat(64),
          engineer_confirmed: true,
          rationale: "Recover the saved workspace",
        });
        return new Response(JSON.stringify(readyRecord));
      }
      return new Response("[]");
    },
  );
  const user = userEvent.setup();
  const client = new QueryClient();
  const content = (
    <QueryClientProvider client={client}>
      <ApiContext.Provider value={api}>
        <Backups />
      </ApiContext.Provider>
    </QueryClientProvider>
  );
  const view = render(content);
  await user.click(screen.getByText("Restore a saved backup"));
  await user.type(screen.getByLabelText("Backup file"), "C:/backups/one.zip");
  await user.click(screen.getByRole("button", { name: "Check backup" }));
  expect(screen.getByLabelText("Backup file")).toBeDisabled();
  finishCheck(new Response(JSON.stringify(inspection)));
  await screen.findByText("The backup is ready to inspect.");
  expect(
    screen.getByText(
      "1 original file was unavailable when this backup was created.",
    ),
  ).toBeInTheDocument();
  await user.type(
    screen.getByLabelText("Reason for restoring"),
    "Recover the saved workspace",
  );
  await user.click(
    screen.getByRole("button", { name: "Prepare this restoration" }),
  );
  expect(await screen.findByRole("status")).toHaveTextContent(
    "Restoration is prepared.",
  );
  expect(screen.getByLabelText("Backup file")).toBeDisabled();
  view.unmount();
  render(content);
  expect(await screen.findByRole("status")).toHaveTextContent(
    "Restoration is prepared.",
  );
  await user.click(
    screen.getByRole("button", { name: "Close Quantix for restoration" }),
  );
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "A current run must stop before closing.",
  );
  expect(screen.getByRole("status")).toHaveTextContent(
    "Restoration is prepared.",
  );
});

it("retains the checked backup and decision note if restoration preparation fails", async () => {
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async (url) =>
      new Response(
        JSON.stringify(
          String(url).endsWith("/latest")
            ? null
            : String(url).endsWith("/pending")
              ? null
              : String(url).endsWith("/inspect")
                ? inspection
                : String(url).endsWith("/restore")
                  ? { detail: "The backup changed after inspection." }
                  : [],
        ),
        { status: String(url).endsWith("/restore") ? 409 : 200 },
      ),
  );
  const user = userEvent.setup();
  render(
    <QueryClientProvider client={new QueryClient()}>
      <ApiContext.Provider value={api}>
        <Backups />
      </ApiContext.Provider>
    </QueryClientProvider>,
  );
  await user.click(screen.getByText("Restore a saved backup"));
  await user.type(screen.getByLabelText("Backup file"), "C:/backups/one.zip");
  await user.click(screen.getByRole("button", { name: "Check backup" }));
  await user.type(
    await screen.findByLabelText("Reason for restoring"),
    "Recover saved records",
  );
  await user.click(
    screen.getByRole("button", { name: "Prepare this restoration" }),
  );
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "The backup changed after inspection.",
  );
  expect(screen.getByLabelText("Reason for restoring")).toHaveValue(
    "Recover saved records",
  );
  expect(
    screen.queryByRole("button", { name: "Close Quantix for restoration" }),
  ).not.toBeInTheDocument();
});

it("shows a completed recovery after reload without claiming damaged files form a complete backup", async () => {
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async (url) =>
      new Response(
        JSON.stringify(
          String(url).endsWith("/latest")
            ? {
                restore_id: "restored",
                completed_at: "2026-09-06T13:00:00Z",
                before_restore_backup_id: null,
                recovery_evidence_path: "backups/retained.recovery.zip",
                detail:
                  "Workspace restored. The damaged previous workspace was retained as recovery evidence with missing or changed files listed; it is not a complete backup.",
              }
            : String(url).endsWith("/pending")
              ? null
              : [],
        ),
      ),
  );
  render(
    <QueryClientProvider client={new QueryClient()}>
      <ApiContext.Provider value={api}>
        <Backups />
      </ApiContext.Provider>
    </QueryClientProvider>,
  );
  expect(
    await screen.findByRole("heading", { name: "Last restoration" }),
  ).toBeInTheDocument();
  expect(screen.getByText(/it is not a complete backup/)).toBeInTheDocument();
  expect(screen.getByText("backups/retained.recovery.zip")).toBeInTheDocument();
  expect(
    screen.queryByRole("button", { name: "Close Quantix for restoration" }),
  ).not.toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Create backup" })).toBeEnabled();
});

it("keeps a failed completed-restoration check visible and offers an explicit retry", async () => {
  let attempts = 0;
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async (url) => {
      if (String(url).endsWith("/latest")) {
        attempts += 1;
        if (attempts < 3)
          return new Response(
            JSON.stringify({
              detail: "The restoration history could not be read.",
            }),
            { status: 409 },
          );
        return new Response("null");
      }
      return new Response(String(url).endsWith("/pending") ? "null" : "[]");
    },
  );
  const user = userEvent.setup();
  render(
    <QueryClientProvider
      client={
        new QueryClient({ defaultOptions: { queries: { retryDelay: 0 } } })
      }
    >
      <ApiContext.Provider value={api}>
        <Backups />
      </ApiContext.Provider>
    </QueryClientProvider>,
  );
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "The restoration history could not be read.",
  );
  await user.click(
    screen.getByRole("button", { name: "Check restoration history again" }),
  );
  await screen.findByText("No backups have been created yet.");
  expect(attempts).toBe(3);
});
