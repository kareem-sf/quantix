import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { fakeService, openApp } from "../test/app";

describe("Quantix shell", () => {
  it("starts the first tender and opens its overview", async () => {
    fakeService();
    const router = openApp("/");

    await userEvent.type(await screen.findByLabelText("Tender name"), "Al Noor Primary School");
    await userEvent.type(screen.getByLabelText(/Submission date/), "2026-10-14");
    await userEvent.click(screen.getByRole("button", { name: "Start tender" }));

    expect(await screen.findByRole("heading", { name: "Nothing needs you right now" })).toBeInTheDocument();
    expect(within(screen.getByRole("main")).getByText("Al Noor Primary School")).toBeInTheDocument();
    expect(router.state.location.pathname).toBe("/tenders/t1");
    const sidebar = screen.getByRole("navigation", { name: "Quantix" });
    expect(sidebar).toHaveTextContent("Al Noor Primary School");
    expect(sidebar).toHaveTextContent("due 14 Oct");
  });

  it("opens the newest tender from the start page", async () => {
    fakeService({
      tenders: [{ id: "t9", name: "Riyadh Warehouse", due_date: null, created_at: "2026-09-20T08:00:00Z" }],
    });
    openApp("/");

    expect(await screen.findByRole("heading", { name: "Nothing needs you right now" })).toBeInTheDocument();
    expect(within(screen.getByRole("main")).getByText("Riyadh Warehouse")).toBeInTheDocument();
    expect(screen.getByText("No due date yet.")).toBeInTheDocument();
  });

  it("shows the service's reason when a tender can't be created", async () => {
    fakeService({ fail: { "/tenders": "The disk is full." } });
    openApp("/new");

    await userEvent.type(screen.getByLabelText("Tender name"), "Clinic");
    await userEvent.click(screen.getByRole("button", { name: "Start tender" }));

    expect(await screen.findByText("Couldn’t create the tender: The disk is full.")).toBeInTheDocument();
  });

  it("says so when a tender doesn't exist", async () => {
    fakeService();
    openApp("/tenders/missing");

    expect(await screen.findByText("Tender not found.")).toBeInTheDocument();
  });
});
