import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { createMemoryRouter, RouterProvider } from "react-router";
import { describe, expect, it, vi } from "vitest";
import type { Tender } from "../api/client";
import { routes } from "./router";

function json(body: unknown, status = 200) {
  return new Response(JSON.stringify(body), { status, headers: { "Content-Type": "application/json" } });
}

/** An in-memory stand-in for the local service, at the fetch boundary. */
function service(tenders: Tender[] = [], failCreate = false) {
  const fetch = vi.fn(async (request: Request) => {
    const path = new URL(request.url).pathname;
    if (path === "/api/tenders" && request.method === "GET") return json(tenders);
    if (path === "/api/tenders" && request.method === "POST") {
      if (failCreate) return json({ detail: "The disk is full." }, 500);
      const body = await request.json();
      const tender = { id: `t${tenders.length + 1}`, created_at: "2026-09-23T10:00:00Z", ...body };
      tenders.unshift(tender);
      return json(tender, 201);
    }
    const found = tenders.find((t) => path === `/api/tenders/${t.id}`);
    return found ? json(found) : json({ detail: "Tender not found." }, 404);
  });
  vi.stubGlobal("fetch", fetch);
  return fetch;
}

function open(path: string) {
  const router = createMemoryRouter(routes, { initialEntries: [path] });
  const queries = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={queries}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
  return router;
}

describe("Quantix shell", () => {
  it("starts the first tender and opens its overview", async () => {
    service();
    const router = open("/");

    await userEvent.type(await screen.findByLabelText("Tender name"), "Al Noor Primary School");
    await userEvent.type(screen.getByLabelText(/Submission date/), "2026-10-14");
    await userEvent.click(screen.getByRole("button", { name: "Start tender" }));

    expect(await screen.findByRole("heading", { name: "Al Noor Primary School" })).toBeInTheDocument();
    expect(router.state.location.pathname).toBe("/tenders/t1");
    const sidebar = screen.getByRole("navigation", { name: "Quantix" });
    expect(sidebar).toHaveTextContent("Al Noor Primary School");
    expect(sidebar).toHaveTextContent("due 14 Oct");
  });

  it("opens the newest tender from the start page", async () => {
    service([{ id: "t9", name: "Riyadh Warehouse", due_date: null, created_at: "2026-09-20T08:00:00Z" }]);
    open("/");

    expect(await screen.findByRole("heading", { name: "Riyadh Warehouse" })).toBeInTheDocument();
    expect(screen.getByText("No due date yet.")).toBeInTheDocument();
  });

  it("shows the service's reason when a tender can't be created", async () => {
    service([], true);
    open("/new");

    await userEvent.type(screen.getByLabelText("Tender name"), "Clinic");
    await userEvent.click(screen.getByRole("button", { name: "Start tender" }));

    expect(await screen.findByText("Couldn’t create the tender: The disk is full.")).toBeInTheDocument();
  });

  it("says so when a tender doesn't exist", async () => {
    service();
    open("/tenders/missing");

    expect(await screen.findByText("Tender not found.")).toBeInTheDocument();
  });
});
