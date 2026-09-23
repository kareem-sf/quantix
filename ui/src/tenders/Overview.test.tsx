import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import type { Priced } from "../estimate/queries";
import { fakeService, openApp } from "../test/app";

const tender = { id: "t1", name: "Synthetic school", due_date: null, created_at: "2026-09-23T10:00:00Z" };
const doc = {
  id: "d1",
  path: "Pkg/Bill.xlsx",
  name: "Bill.xlsx",
  kind: "spreadsheet",
  size: 10,
  status: "read",
  note: null,
  page_count: 1,
  group_name: null,
  description: null,
};
const priced = (id: string, item_status: string, amount: string | null): Priced =>
  ({ id, section: null, item: id, description: "Excavation", unit: "m3", quantity: "10", item_status, rate: null, amount }) as Priced;

describe("Overview", () => {
  it("shows where the BOQ and the price stand", async () => {
    fakeService({
      tenders: [tender],
      documents: [doc],
      priced: [priced("3.1", "approved", "100.00"), priced("3.2", "proposed", null)],
      summary: {
        currency: "SAR", priced: 1, items: 2, waiting: 0, net: "100.00", preliminaries: "0.00", overheads: "0.00",
        profit: "0.00", adjustment: "0.00", total: "100.00", vat_rate: null, vat: null, total_with_vat: null, unpriced: ["3.2"],
      },
    });
    openApp("/tenders/t1");

    expect(await screen.findByText("2 items · 1 approved")).toBeInTheDocument();
    expect(screen.getByText("1 of 2 priced · SAR 100.00")).toBeInTheDocument();
  });

  it("deletes a tender only after asking", async () => {
    const service = fakeService({ tenders: [tender], documents: [doc] });
    const router = openApp("/tenders/t1");

    await userEvent.click(await screen.findByRole("button", { name: "Delete this tender" }));
    await userEvent.click(screen.getByRole("button", { name: "Keep it" }));
    expect(service.state.tenders).toHaveLength(1);

    await userEvent.click(screen.getByRole("button", { name: "Delete this tender" }));
    await userEvent.click(screen.getByRole("button", { name: "Delete tender" }));
    await waitFor(() => expect(service.state.tenders).toHaveLength(0));
    await waitFor(() => expect(router.state.location.pathname).not.toBe("/tenders/t1"));
  });
});
