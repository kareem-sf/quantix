import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { createMemoryRouter, RouterProvider } from "react-router";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Decision } from "../office/queries";
import { fakeService, openApp } from "../test/app";
import { useNotifications } from "./notify";

// the desktop app: Tauri is there, its window answers and a clicked Windows notification can be listened to
const desktop = vi.hoisted(() => ({
  invoke: vi.fn(),
  clicked: null as null | ((event: { payload: string }) => void),
  maximised: false,
  resized: null as null | (() => void),
  minimize: vi.fn(async () => {}),
  toggleMaximize: vi.fn(async () => {}),
  close: vi.fn(async () => {}),
}));
vi.mock("@tauri-apps/api/core", () => ({ isTauri: () => true, invoke: desktop.invoke }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: async (_: string, handler: (event: { payload: string }) => void) => {
    desktop.clicked = handler;
    return () => (desktop.clicked = null);
  },
}));
vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({
    isMaximized: async () => desktop.maximised,
    onResized: async (check: () => void) => {
      desktop.resized = check;
      return () => {};
    },
    minimize: desktop.minimize,
    toggleMaximize: desktop.toggleMaximize,
    close: desktop.close,
  }),
}));

const school = { id: "t1", name: "Al Noor Primary School", due_date: null, created_at: "2026-09-23T08:00:00Z" };
const warehouse = { id: "t9", name: "Riyadh Warehouse", due_date: null, created_at: "2026-09-20T08:00:00Z" };
const question: Decision = {
  id: "q1", raised_by: "s1", title: "Site support period", text: "72 working days or 4 months?", options: ["72 days"],
  subject_kind: null, subject_id: null, sources: null, status: "waiting", answer: null, created_at: "2026-09-28T09:00:00Z",
};

beforeEach(() => {
  desktop.invoke.mockClear();
  desktop.maximised = false;
});
afterEach(() => vi.restoreAllMocks());

function Watching() {
  useNotifications();
  return null;
}

/** Quantix in the background, watching the Desk; `look` is its next look at every tender. */
async function watch() {
  const router = createMemoryRouter([{ path: "*", element: <Watching /> }], { initialEntries: ["/desk"] });
  const queries = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={queries}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
  const look = async () => {
    await act(() => queries.refetchQueries());
    await act(() => new Promise((resolve) => setTimeout(resolve, 10))); // the query tells the screen on its next tick
  };
  await look(); // the first look finds nothing new
  return { router, look };
}

describe("Windows notifications", () => {
  it("tells the engineer of a decision that newly waits while Quantix is behind other windows", async () => {
    vi.spyOn(document, "hasFocus").mockReturnValue(false);
    const service = fakeService({ tenders: [school] });
    const { look } = await watch();
    expect(desktop.invoke).not.toHaveBeenCalled();

    service.state.decisions.push(question);
    await look();
    expect(desktop.invoke).toHaveBeenCalledWith("notify", {
      title: "Al Noor Primary School",
      body: "1 new decision needs you",
      open: "/tenders/t1",
    });
  });

  it("gathers news from several tenders into one notification that opens the Desk", async () => {
    vi.spyOn(document, "hasFocus").mockReturnValue(false);
    const service = fakeService({ tenders: [school, warehouse], decisions: [] }); // the fake's question waits on each tender
    const { look } = await watch();

    service.state.decisions.push(question);
    await look();
    expect(desktop.invoke).toHaveBeenCalledWith("notify", {
      title: "Several tenders",
      body: "Al Noor Primary School: 1 new decision needs you\nRiyadh Warehouse: 1 new decision needs you",
      open: "/desk",
    });
  });

  it("stays quiet while Quantix is in front", async () => {
    vi.spyOn(document, "hasFocus").mockReturnValue(true);
    const service = fakeService({ tenders: [school] });
    const { look } = await watch();

    service.state.decisions.push(question);
    await look();
    expect(desktop.invoke).not.toHaveBeenCalled();
  });

  it("sends only what Settings asks for", async () => {
    vi.spyOn(document, "hasFocus").mockReturnValue(false);
    const service = fakeService({ tenders: [school], officeState: "working" });
    service.state.settings.notifications = "decisions";
    const { look } = await watch();

    service.state.officeState = "idle"; // the team finished
    await look();
    expect(desktop.invoke).not.toHaveBeenCalled();
    service.state.decisions.push(question);
    await look();
    expect(desktop.invoke).toHaveBeenCalledTimes(1);

    service.state.settings.notifications = "off";
    service.state.decisions.push({ ...question, id: "q2" });
    await look();
    expect(desktop.invoke).toHaveBeenCalledTimes(1);
  });

  it("opens the place a notification is about when the engineer clicks it", async () => {
    fakeService({ tenders: [school] });
    const { router } = await watch();

    await act(async () => desktop.clicked!({ payload: "/tenders/t1" }));
    expect(router.state.location.pathname).toBe("/tenders/t1");
  });
});

describe("The desktop window", () => {
  it("draws its own minimise, maximise and close, since it has no Windows frame", async () => {
    fakeService({ tenders: [school] });
    openApp("/tenders/t1");
    const bar = screen.getByRole("banner");

    await userEvent.click(await within(bar).findByRole("button", { name: "Minimise" }));
    expect(desktop.minimize).toHaveBeenCalled();
    await userEvent.click(within(bar).getByRole("button", { name: "Maximise" }));
    expect(desktop.toggleMaximize).toHaveBeenCalled();

    desktop.maximised = true; // the window reports itself resized, now maximised
    await act(async () => desktop.resized!());
    expect(await within(bar).findByRole("button", { name: "Restore" })).toBeInTheDocument();

    await userEvent.click(within(bar).getByRole("button", { name: "Close" }));
    expect(desktop.close).toHaveBeenCalled();
  });
});
