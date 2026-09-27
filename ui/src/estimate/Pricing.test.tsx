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
  reviewed_by: "s1",
  review_note: null,
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
    web_page: null,
    note: "Steel price from Al-Rajhi’s quote of 17 September, excluding delivery.",
    status: "reviewed",
    proposed_by: "s3",
    reviewed_by: "s1",
    review_note: "Rate matches the quote.",
  },
};
const summary: Summary = {
  currency: "SAR",
  priced: 1,
  items: 1,
  waiting: 1,
  reviewing: 0,
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

  it("shows a market price from the web with the page it was read from", async () => {
    const web = {
      ...priced.rate!,
      basis: "web",
      source_document: null,
      document_id: null,
      page: null,
      quote: "C35/20 OPC | 230.00 SAR",
      web_page: { url: "https://readymix.example/c35", title: "Green Concrete Readymix", read_at: "2026-09-27T10:00:00Z" },
    };
    fakeService({ tenders: [tender], staff: [priya], items: [rebar], priced: [{ ...priced, rate: web }], summary });
    openApp("/tenders/t1/estimate?item=i43");

    const panel = await screen.findByRole("complementary", { name: "Item 4.3" });
    expect(await within(panel).findByText("A market price from the web")).toBeInTheDocument();
    expect(within(panel).getByRole("link", { name: /^Green Concrete Readymix, read 27 Sept? 2026$/ })).toHaveAttribute(
      "href",
      "https://readymix.example/c35",
    );
    expect(within(panel).getByText("“C35/20 OPC | 230.00 SAR”")).toBeInTheDocument();
  });

  it("reopens a rate the office approved after the Manager's review, with the reason", async () => {
    const approved = { ...priced, rate: { ...priced.rate!, status: "office_approved" } };
    const rania = { ...priya, id: "s1", name: "Rania Farouk", role: "Tender Manager", is_manager: true };
    const service = fakeService({ tenders: [tender], staff: [rania, priya], items: [rebar], priced: [approved], summary });
    openApp("/tenders/t1/estimate?item=i43");

    const panel = await screen.findByRole("complementary", { name: "Item 4.3" });
    expect(await within(panel).findByText("Approved by the office")).toBeInTheDocument();
    expect(within(panel).getByText(/Reviewed by Rania/)).toHaveTextContent("Reviewed by Rania: Rate matches the quote.");
    await userEvent.click(within(panel).getByRole("button", { name: "Reopen" }));
    await userEvent.type(within(panel).getByLabelText("Why it needs doing again"), "Delivery is excluded.");
    await userEvent.click(within(panel).getByRole("button", { name: "Send" }));
    await waitFor(() =>
      expect(service.state.reopened).toEqual([{ kind: "rate", id: "r1", reason: "Delivery is excluded." }]),
    );
  });

  it("shows what Quantix's checks found in a rate, with the Manager's reason and the source", async () => {
    const findings = {
      r1: [
        {
          severity: "warning",
          message: "4.2 is the same item at 3,300.00 per t; 4.3 is 3,488.00.",
          refs: [{ label: "4.2", document_id: null, page: null }],
          accepted_by: "Rania",
          reason: "4.2 is mesh, not bar: a different item.",
        },
        {
          severity: "blocker",
          message: "It still has text to fill in: “[date]”.",
          refs: [{ label: "Quote.pdf, page 1", document_id: "d2", page: 1 }],
          accepted_by: null,
          reason: null,
        },
      ],
    };
    fakeService({ tenders: [tender], staff: [priya], items: [rebar], priced: [priced], summary, findings });
    openApp("/tenders/t1/estimate?item=i43");

    const panel = await screen.findByRole("complementary", { name: "Item 4.3" });
    expect(await within(panel).findByText("Quantix’s checks")).toBeInTheDocument();
    expect(within(panel).getByText("4.2 is the same item at 3,300.00 per t; 4.3 is 3,488.00.")).toBeInTheDocument();
    expect(within(panel).getByText("See 4.2")).toBeInTheDocument();
    expect(within(panel).getByText(/Accepted by Rania/)).toHaveTextContent(
      "Accepted by Rania: 4.2 is mesh, not bar: a different item.",
    );
    expect(within(panel).getByText("Must be fixed:")).toBeInTheDocument();
    expect(within(panel).getAllByRole("link", { name: "Quote.pdf, page 1" })[1]).toHaveAttribute(
      "href",
      "/tenders/t1/documents?doc=d2&page=1",
    );
  });

  it("summarises the price with markups and VAT", async () => {
    fakeService({
      tenders: [tender],
      items: [rebar],
      priced: [priced],
      summary,
      markups: {
        id: "mk1",
        preliminary_items: [
          { item: "Site engineer", quantity: "3", unit: "month", rate: "2500.00", cost: "7500.00" },
          { item: "Plant mobilisation", quantity: "1", unit: "sum", rate: "341.02", cost: "341.02" },
        ],
        overheads: "0.05",
        profit: "0.07",
        adjustment: "0.00",
        note: "Company markups for schools.",
        status: "reviewed",
        proposed_by: "s3",
        reviewed_by: "s1",
        review_note: "Staff for the programme's months.",
      },
    });
    openApp("/tenders/t1/estimate?view=summary");

    const panel = await screen.findByRole("complementary", { name: "Price summary" });
    expect(within(panel).getByText("Preliminaries")).toBeInTheDocument();
    const items = within(panel).getByRole("list", { name: "Preliminaries" });
    expect(within(items).getByText("Site engineer · 3 month × 2,500.00")).toBeInTheDocument();
    expect(within(items).getByText("7,500.00")).toBeInTheDocument();
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
    await userEvent.type(screen.getByLabelText("Currency"), "SAR");
    await userEvent.type(screen.getByLabelText("Source"), "Payroll 2026");
    await userEvent.selectOptions(screen.getByLabelText("Kind"), "Labour");
    await userEvent.click(screen.getByRole("button", { name: "Add" }));
    await waitFor(() =>
      expect(service.state.library[0]).toMatchObject({ kind: "labour", name: "Steel fixer gang", rate: "62", currency: "SAR" }),
    );
  });
});
