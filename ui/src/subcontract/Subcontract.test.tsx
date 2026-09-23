import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { fakeService, openApp } from "../test/app";
import type { Package, Quote } from "./queries";

const tender = { id: "t1", name: "Synthetic school", due_date: null, created_at: "2026-09-23T10:00:00Z" };
const omar = {
  id: "s2",
  name: "Omar Haddad",
  role: "Commercial",
  is_manager: false,
  status: "active",
  now: null,
  profile: {},
};
const cell = (rate: string, amount: string, page: number | null, quote: string | null) => ({
  rate,
  amount,
  plugged: page === null,
  page,
  quote,
});
const gulf: Quote = {
  id: "q1",
  company: "Gulf Groundworks",
  document_id: "d2",
  document: "Gulf.pdf",
  cells: { i1: cell("17.00", "21080.00", 1, "3.1 Excavation 17.00"), i2: cell("35.00", "34300.00", 1, "6.3 Waterproofing 35.00") },
  exclusions: [{ description: "Dewatering", amount: "5000", page: 1, quote: "Excludes dewatering" }],
  quoted_total: "55380.00",
  exclusions_total: "5000",
  levelled_total: "60380.00",
  rank: 2,
  proposed_by: "s2",
};
const najd: Quote = {
  id: "q2",
  company: "Najd Contracting",
  document_id: "d3",
  document: "Najd.pdf",
  cells: { i1: cell("16.50", "20460.00", 1, "Item 3.1 excavation rate 16.50"), i2: cell("38.00", "37240.00", null, null) },
  exclusions: [],
  quoted_total: "20460.00",
  exclusions_total: "0",
  levelled_total: "57700.00",
  rank: 1,
  proposed_by: "s2",
};
const groundworks = (): Package => ({
  id: "p1",
  name: "Groundworks",
  kind: "subcontract",
  items: [
    { id: "i1", item: "3.1", description: "Excavation", unit: "m3", quantity: "1240", our_rate: "18.50" },
    { id: "i2", item: "6.3", description: "Waterproofing", unit: "m2", quantity: "980", our_rate: "38.00" },
  ],
  enquiries: [
    {
      id: "e1",
      company: "Red Sea Membranes",
      email: "q@rsm.sa",
      subject: "Groundworks enquiry",
      body: "Please price items 3.1 and 6.3.",
      status: "draft",
      created_by: "s2",
    },
  ],
  quotes: [gulf, najd],
  recommended_quote_id: "q2",
  recommendation: "Lowest after levelling. They left out waterproofing, so I used our rate.",
  recommended_by: "s2",
  selected_quote_id: null,
  created_by: "s2",
});

describe("Subcontract", () => {
  it("shows the levelled quotes with gaps, exclusions and each rate's source", async () => {
    fakeService({ tenders: [tender], staff: [omar], packages: [groundworks()] });
    openApp("/tenders/t1/subcontract");

    const table = await screen.findByRole("region", { name: "Groundworks" });
    expect(within(table).getByText("2 quotes · 1 enquiry · levelled by Omar")).toBeInTheDocument();
    expect(within(table).getByRole("link", { name: "17.00" })).toHaveAttribute("href", "/tenders/t1/documents?doc=d2&page=1");
    expect(within(table).getByTitle("Not quoted: our rate is used")).toHaveTextContent("38.00");
    expect(within(table).getByText("+5,000.00")).toBeInTheDocument();
    expect(within(table).getByText("57,700.00")).toBeInTheDocument();
    expect(within(table).getByText("Gulf Groundworks excludes dewatering.")).toBeInTheDocument();
    expect(screen.getByText("Omar recommends Najd Contracting")).toBeInTheDocument();
  });

  it("chooses the recommended quote", async () => {
    const service = fakeService({ tenders: [tender], staff: [omar], packages: [groundworks()] });
    openApp("/tenders/t1/subcontract");

    await userEvent.click(await screen.findByRole("button", { name: "Choose Najd Contracting" }));
    await waitFor(() => expect(service.state.packages[0].selected_quote_id).toBe("q2"));
    expect(await screen.findByText(/is chosen\. Their rates are in the estimate/)).toBeInTheDocument();
  });

  it("opens an enquiry draft for the engineer to send", async () => {
    const service = fakeService({ tenders: [tender], staff: [omar], packages: [groundworks()] });
    openApp("/tenders/t1/subcontract");

    await userEvent.click(await screen.findByRole("button", { name: /Red Sea Membranes/ }));
    expect(screen.getByRole("link", { name: "Open in mail" }).getAttribute("href")).toContain("mailto:q@rsm.sa?subject=Groundworks");
    await userEvent.click(screen.getByRole("button", { name: "Mark as sent" }));
    await waitFor(() => expect(service.state.packages[0].enquiries[0].status).toBe("sent"));
  });
});

describe("Directory", () => {
  it("adds and removes companies", async () => {
    const service = fakeService({ tenders: [tender] });
    openApp("/directory");

    await userEvent.type(await screen.findByLabelText("Company"), "Gulf Groundworks");
    await userEvent.type(screen.getByLabelText("Trades"), "Earthworks");
    await userEvent.click(screen.getByRole("button", { name: "Add" }));
    await waitFor(() =>
      expect(service.state.directory).toEqual([
        expect.objectContaining({ name: "Gulf Groundworks", kind: "subcontractor", trades: "Earthworks", email: null }),
      ]),
    );
    await userEvent.click(await screen.findByRole("button", { name: "Remove" }));
    await waitFor(() => expect(service.state.directory).toEqual([]));
  });
});
