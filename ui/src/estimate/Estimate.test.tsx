import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { fakeService, openApp } from "../test/app";
import type { BoqItem, Fact } from "./queries";

const tender = { id: "t1", name: "Synthetic school", due_date: null, created_at: "2026-09-23T10:00:00Z" };
const omar = {
  id: "s2",
  name: "Omar Haddad",
  role: "Quantity Surveyor",
  is_manager: false,
  status: "active",
  now: null,
  profile: {},
};
const source = (quote: string) => ({ document_id: "d1", document_name: "Bill.xlsx", page: 1, quote });
const item = (id: string, no: string, description: string, qty: string, unit: string, status = "proposed"): BoqItem => ({
  id,
  section: "Section 3 · Earthworks",
  item: no,
  description,
  unit,
  quantity: qty,
  status,
  proposed_by: "s2",
  reason: null,
  source: source(`A2=${no} | B2=${description} | C2=${unit} | D2=${qty}`),
});
const method: Fact = {
  id: "f1",
  kind: "method_of_measurement",
  label: "Method of measurement",
  value: "POMI",
  status: "proposed",
  proposed_by: "s2",
  source: { document_id: "d2", document_name: "ITT.pdf", page: 4, quote: "Measured in accordance with POMI" },
};

describe("Estimate", () => {
  it("shows the BOQ as the office entered it, with each item's source", async () => {
    fakeService({
      tenders: [tender],
      staff: [omar],
      items: [item("i1", "3.1", "Excavation to reduce levels", "1240", "m3"), item("i2", "3.4", "Blinding", "86", "m3", "approved")],
    });
    openApp("/tenders/t1/estimate");

    expect(await screen.findByText("0 of 2 items priced · 1 needs you")).toBeInTheDocument();
    expect(screen.getByText("Section 3 · Earthworks")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: /Excavation to reduce levels/ }));

    const panel = await screen.findByRole("complementary", { name: "Item 3.1" });
    expect(within(panel).getByRole("link", { name: "Bill.xlsx, page 1" })).toHaveAttribute(
      "href",
      "/tenders/t1/documents?doc=d1&page=1",
    );
    expect(within(panel).getByText("A2=3.1 | B2=Excavation to reduce levels | C2=m3 | D2=1240")).toBeInTheDocument();
    expect(within(panel).getByText("Entered by Omar")).toBeInTheDocument();
  });

  it("approves everything waiting in one go", async () => {
    const service = fakeService({ tenders: [tender], items: [item("i1", "3.1", "Excavation", "1240", "m3")] });
    openApp("/tenders/t1/estimate");

    await userEvent.click(await screen.findByRole("button", { name: "Approve all 1" }));
    await waitFor(() => expect(service.state.items[0].status).toBe("approved"));
  });

  it("sends an item back with the reason", async () => {
    const service = fakeService({ tenders: [tender], staff: [omar], items: [item("i1", "3.1", "Excavation", "1240", "m3")] });
    openApp("/tenders/t1/estimate?item=i1");

    await userEvent.click(await screen.findByRole("button", { name: "Reject" }));
    await userEvent.type(screen.getByLabelText("Why reject it"), "Use the drawing quantity.");
    await userEvent.click(screen.getByRole("button", { name: "Send back" }));
    await waitFor(() =>
      expect(service.state.items[0]).toMatchObject({ status: "rejected", reason: "Use the drawing quantity." }),
    );
  });

  it("approves a tender fact and lists gates on the overview", async () => {
    const service = fakeService({ tenders: [tender], facts: [method], items: [item("i1", "3.1", "Excavation", "1240", "m3")] });
    openApp("/tenders/t1");

    expect(await screen.findByRole("heading", { name: "2 decisions need you" })).toBeInTheDocument();
    expect(screen.getByText("1 BOQ item to approve")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("link", { name: /1 tender fact to approve/ }));

    expect(await screen.findByText("Method of measurement:")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Approve" }));
    await waitFor(() => expect(service.state.facts[0].status).toBe("approved"));
  });
});
