import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { fakeService, openApp } from "../test/app";
import type { BoqItem, Priced, Summary } from "./queries";

const tender = { id: "t1", name: "Synthetic school", due_date: null, created_at: "2026-09-23T10:00:00Z" };
const priya = {
  id: "s3",
  name: "Priya Nair",
  role: "Estimator",
  is_manager: false,
  status: "active",
  now: null,
  profile: {},
};
const rebar: BoqItem = {
  id: "i43",
  section: null,
  item: "4.3",
  description: "Slab reinforcement",
  unit: "t",
  quantity: "28.1",
  status: "approved",
  proposed_by: "s2",
  reason: null,
  source: { document_id: "d1", document_name: "Bill.xlsx", page: 1, quote: "A2=4.3 | B2=Slab reinforcement" },
};
const priced: Priced = {
  ...rebar,
  item_status: "approved",
  amount: "98012.80",
  rate: {
    id: "r1",
    basis: "quote",
    rate: "3488.00",
    unit_rate: null,
    lines: [
      { kind: "labour", resource: "Steel fixer gang", quantity: "16", unit: "hr", rate: "62", wastage: "0", cost: "992.00" },
      {
        kind: "material",
        resource: "Rebar B500B cut and bent",
        quantity: "1",
        unit: "t",
        rate: "2300",
        wastage: "0.05",
        cost: "2415.00",
      },
    ],
    source_document: "Quote.pdf",
    document_id: "d2",
    page: 1,
    quote: "Rebar B500B cut and bent 2,300.00",
    library_id: null,
    note: "Steel price from Al-Rajhi’s quote of 17 September, excluding delivery.",
    status: "proposed",
    proposed_by: "s3",
  },
};
const summary: Summary = {
  currency: "SAR",
  priced: 1,
  items: 1,
  waiting: 1,
  net: "98012.80",
  preliminaries: "7841.02",
  overheads: "5292.69",
  profit: "7780.66",
  adjustment: "0.00",
  total: "118927.17",
  vat_rate: "0.15",
  vat: "17839.08",
  total_with_vat: "136766.25",
  unpriced: [],
};

describe("Pricing", () => {
  it("shows a rate's build-up and basis, and approves it into the library", async () => {
    const service = fakeService({ tenders: [tender], staff: [priya], items: [rebar], priced: [priced], summary });
    openApp("/tenders/t1/estimate?item=i43");

    const panel = await screen.findByRole("complementary", { name: "Item 4.3" });
    expect(await within(panel).findByText("1.00 t incl. 5% waste × 2,300.00")).toBeInTheDocument();
    expect(within(panel).getByText("3,488.00")).toBeInTheDocument();
    expect(within(panel).getByText("From a quote")).toBeInTheDocument();
    expect(within(panel).getByRole("link", { name: "Quote.pdf, page 1" })).toBeInTheDocument();
    expect(screen.getByText("SAR 98,012.80")).toBeInTheDocument();

    await userEvent.click(within(panel).getByLabelText("Save to the company library"));
    await userEvent.click(within(panel).getByRole("button", { name: "Approve rate" }));
    await waitFor(() => expect(service.state.decided).toEqual([{ id: "r1", approve: true, reason: null, save_to_library: true }]));
  });

  it("summarises the price with markups and VAT", async () => {
    fakeService({
      tenders: [tender],
      items: [rebar],
      priced: [priced],
      summary,
      markups: {
        id: "mk1",
        preliminaries: "0.08",
        overheads: "0.05",
        profit: "0.07",
        adjustment: "0.00",
        note: "Company markups for schools.",
        status: "proposed",
        proposed_by: "s1",
      },
    });
    openApp("/tenders/t1/estimate?view=summary");

    const panel = await screen.findByRole("complementary", { name: "Price summary" });
    expect(within(panel).getByText("Preliminaries 8%")).toBeInTheDocument();
    expect(within(panel).getByText("SAR 118,927.17")).toBeInTheDocument();
    expect(within(panel).getByText("SAR 136,766.25")).toBeInTheDocument();
    expect(within(panel).getByText("The total includes 1 rate waiting for you.")).toBeInTheDocument();
    expect(within(panel).getByRole("button", { name: "Approve markups" })).toBeInTheDocument();
  });

  it("keeps a company library", async () => {
    const service = fakeService({});
    openApp("/library");

    expect(await screen.findByText(/Empty\./)).toBeInTheDocument();
    await userEvent.type(screen.getByLabelText("Name"), "Steel fixer gang");
    await userEvent.type(screen.getByLabelText("Unit"), "hr");
    await userEvent.type(screen.getByLabelText("Rate"), "62");
    await userEvent.type(screen.getByLabelText("Source"), "Payroll 2026");
    await userEvent.selectOptions(screen.getByLabelText("Kind"), "Labour");
    await userEvent.click(screen.getByRole("button", { name: "Add" }));
    await waitFor(() =>
      expect(service.state.library[0]).toMatchObject({ kind: "labour", name: "Steel fixer gang", rate: "62", currency: "SAR" }),
    );
  });
});
