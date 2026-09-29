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
const rania = { ...omar, id: "s1", name: "Rania Farouk", role: "Tender Manager", is_manager: true };
const source = (quote: string) => ({ document_id: "d1", document_name: "Bill.xlsx", page: 1, quote });
/** Waiting for the engineer once the Tender Manager has reviewed it; "proposed" is still with him. */
const item = (id: string, no: string, description: string, qty: string, unit: string, status = "reviewed"): BoqItem => ({
  id,
  section: "Section 3 · Earthworks",
  item: no,
  description,
  unit,
  quantity: qty,
  status,
  proposed_by: "s2",
  reason: null,
  reviewed_by: status === "proposed" ? null : "s1",
  review_note: status === "proposed" ? null : "Checked against Bill.xlsx row 2.",
  source: source(`A2=${no} | B2=${description} | C2=${unit} | D2=${qty}`),
});
const method: Fact = {
  id: "f1",
  kind: "method_of_measurement",
  label: "Method of measurement",
  value: "POMI",
  status: "reviewed",
  proposed_by: "s2",
  reviewed_by: "s1",
  review_note: "Read on ITT.pdf page 4.",
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

  it("keeps work with the Tender Manager until he has reviewed it", async () => {
    fakeService({ tenders: [tender], staff: [rania, omar], items: [item("i1", "3.1", "Excavation", "1240", "m3", "proposed")] });
    openApp("/tenders/t1/estimate?item=i1");

    expect(await screen.findByText("0 of 1 item priced · 1 with the Manager")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Approve all/ })).not.toBeInTheDocument();
    const panel = await screen.findByRole("complementary", { name: "Item 3.1" });
    expect(within(panel).getByText("With the Manager for review")).toBeInTheDocument();
    expect(within(panel).queryByRole("button", { name: "Approve item" })).not.toBeInTheDocument();
  });

  it("sends an item back with the reason", async () => {
    const service = fakeService({
      tenders: [tender],
      staff: [rania, omar],
      items: [item("i1", "3.1", "Excavation", "1240", "m3")],
    });
    openApp("/tenders/t1/estimate?item=i1");

    const panel = await screen.findByRole("complementary", { name: "Item 3.1" });
    expect(within(panel).getByText(/Reviewed by Rania/)).toHaveTextContent("Reviewed by Rania: Checked against Bill.xlsx row 2.");
    await userEvent.click(await screen.findByRole("button", { name: "Send back" }));
    await userEvent.type(screen.getByLabelText("What to put right"), "Use the drawing quantity.");
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
