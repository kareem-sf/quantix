import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { Package } from "../subcontract/queries";
import { fakeService, openApp } from "../test/app";

const school = { id: "t1", name: "Al Noor Primary School", due_date: null, created_at: "2026-09-23T08:00:00Z" };

afterEach(() => vi.restoreAllMocks());

describe("Opening Quantix", () => {
  it("waits for the service while it starts, rather than showing an empty office", async () => {
    const service = fakeService();
    service.fetch.mockRejectedValue(new TypeError("Failed to fetch"));
    const router = openApp("/");

    expect(await screen.findByText("Waiting for the Quantix service… it starts with the app.")).toBeInTheDocument();
    expect(router.state.location.pathname).toBe("/");
  });

  it("shows quiet placeholder rows while a screen's data arrives", async () => {
    fakeService({ tenders: [school] });
    openApp("/desk");

    expect(screen.getByRole("status", { name: "Opening…" })).toBeInTheDocument();
    expect(await screen.findByRole("heading", { level: 1 })).toBeInTheDocument();
    expect(screen.queryByRole("status", { name: "Opening…" })).not.toBeInTheDocument();
  });
});

describe("A screen that fails", () => {
  it("says so in its own area, with the sidebar and title bar still there to go elsewhere", async () => {
    vi.spyOn(console, "error").mockImplementation(() => {});
    // a quote that lacks a line of its package, which the levelling table can't lay out
    const broken = {
      id: "p1", name: "Groundworks", kind: "subcontract", created_by: "s2",
      items: [{ id: "i1", section: null, item: "3.1", description: "Excavation", unit: "m3", quantity: "1240", our_rate: "18.50" }],
      enquiries: [], recommended_quote_id: null, recommendation: null, recommended_by: null, reviewed_by: null, review_note: null,
      selected_quote_id: null,
      quotes: [{ id: "q1", company: "Gulf Groundworks", document_id: "d2", document: "Gulf.pdf", cells: {}, exclusions: [],
        quoted_total: "0", exclusions_total: "0", levelled_total: null, rank: 1, proposed_by: "s2" }],
    } as unknown as Package;
    fakeService({ tenders: [school], packages: [broken] });
    const router = openApp("/tenders/t1/subcontract");

    expect(await screen.findByText("Something went wrong on this screen.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Try again" })).toBeInTheDocument();
    expect(screen.getByRole("banner")).toHaveTextContent("Al Noor Primary SchoolSubcontract");

    await userEvent.click(within(screen.getByRole("navigation", { name: "Quantix" })).getByRole("link", { name: "Estimate" }));
    expect(router.state.location.pathname).toBe("/tenders/t1/estimate");
    expect(await screen.findByRole("heading", { name: "Estimate" })).toBeInTheDocument();
    expect(screen.queryByText("Something went wrong on this screen.")).not.toBeInTheDocument();
  });
});
