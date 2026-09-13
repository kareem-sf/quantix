import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { afterEach, vi } from "vitest";
import { ApiContext, createApi } from "../api";
import { NewTender } from "./TenderDialogs";

const native = vi.hoisted(() => ({
  desktop: false,
  invoke: vi.fn(),
  drop: undefined as undefined | ((event: { payload: unknown }) => void),
}));
vi.mock("@tauri-apps/api/core", () => ({
  isTauri: () => native.desktop,
  invoke: native.invoke,
}));
vi.mock("@tauri-apps/api/webview", () => ({
  getCurrentWebview: () => ({
    onDragDropEvent: async (handler: (event: { payload: unknown }) => void) => {
      native.drop = handler;
      return () => undefined;
    },
  }),
}));

afterEach(() => {
  native.desktop = false;
  native.invoke.mockReset();
  native.drop = undefined;
});

const tender = {
  id: "t1",
  name: "Drainage works",
  revision: 1,
  status: "active",
  created_at: "",
  updated_at: "",
};

function renderDialog(
  respond: (path: string, body: unknown) => Response,
  onCreated = vi.fn(),
) {
  const calls: { path: string; body: unknown }[] = [];
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async (url, init) => {
      const path = new URL(String(url)).pathname;
      const body = init?.body ? JSON.parse(String(init.body)) : undefined;
      calls.push({ path, body });
      return respond(path, body);
    },
  );
  render(
    <QueryClientProvider client={new QueryClient()}>
      <ApiContext.Provider value={api}>
        <NewTender onClose={() => {}} onCreated={onCreated} />
      </ApiContext.Provider>
    </QueryClientProvider>,
  );
  return { calls, onCreated, user: userEvent.setup() };
}

it("asks only for the package; there is no name to type", async () => {
  renderDialog(() => Response.json(tender));
  expect(screen.queryByLabelText("Tender name")).not.toBeInTheDocument();
  await waitFor(() =>
    expect(screen.getByLabelText("Project folder or ZIP")).toHaveFocus(),
  );
  expect(screen.getByRole("button", { name: "Create tender" })).toBeDisabled();
});

it("keeps the package after the local service rejects creation", async () => {
  const { user } = renderDialog(
    () =>
      new Response(JSON.stringify({ detail: "Unable to save tender." }), {
        status: 500,
      }),
    vi.fn(() => {
      throw new Error("Must not report success");
    }),
  );
  await user.type(
    screen.getByLabelText("Project folder or ZIP"),
    "D:\\Tenders\\Drainage works",
  );
  await user.click(screen.getByRole("button", { name: "Create tender" }));
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "Unable to save tender.",
  );
  expect(screen.getByLabelText("Project folder or ZIP")).toHaveValue(
    "D:\\Tenders\\Drainage works",
  );
});

it("creates an unnamed tender and imports its package in one step", async () => {
  const { user, calls, onCreated } = renderDialog((path) =>
    Response.json(path === "/api/tenders" ? tender : { id: "run" }),
  );
  await user.type(
    screen.getByLabelText("Project folder or ZIP"),
    '"D:\\Tenders\\Drainage works"',
  );
  await user.click(screen.getByRole("button", { name: "Create tender" }));
  await waitFor(() => expect(onCreated).toHaveBeenCalledWith(tender));
  expect(calls.filter((call) => call.body)).toEqual([
    { path: "/api/tenders", body: {} },
    {
      path: "/api/tenders/t1/imports",
      body: { source_path: "D:\\Tenders\\Drainage works" },
    },
  ]);
});

it("retries only the import when the tender was created but the package failed", async () => {
  let importFails = true;
  const { user, calls, onCreated } = renderDialog((path) => {
    if (path === "/api/tenders") return Response.json(tender);
    if (path.endsWith("/imports") && importFails)
      return new Response(JSON.stringify({ detail: "Folder not found." }), {
        status: 404,
      });
    return Response.json({ id: "run" });
  });
  await user.type(
    screen.getByLabelText("Project folder or ZIP"),
    "D:\\Missing",
  );
  await user.click(screen.getByRole("button", { name: "Create tender" }));
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "Folder not found.",
  );
  expect(onCreated).not.toHaveBeenCalled();
  importFails = false;
  await user.click(
    screen.getByRole("button", { name: "Try the import again" }),
  );
  await waitFor(() => expect(onCreated).toHaveBeenCalledWith(tender));
  expect(
    calls.filter((call) => call.path === "/api/tenders" && call.body),
  ).toHaveLength(1);
});

it("accepts a folder dropped on the desktop window", async () => {
  native.desktop = true;
  renderDialog(() => Response.json(tender));
  expect(screen.getByText("Drop the project folder or ZIP here")).toBeVisible();
  await waitFor(() => expect(native.drop).toBeDefined());
  act(() =>
    native.drop?.({
      payload: { type: "drop", paths: ["D:\\Tenders\\Harbour Dam"] },
    }),
  );
  expect(await screen.findByText("D:\\Tenders\\Harbour Dam")).toBeVisible();
  expect(screen.getByText("Harbour Dam")).toBeVisible();
  expect(screen.getByRole("button", { name: "Create tender" })).toBeEnabled();
});
