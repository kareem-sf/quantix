import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import type { Priced } from "../estimate/queries";
import type { Package } from "../subcontract/queries";
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
  scans_to_read: 0,
  opened: 0,
  cited: 0,
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
        currency: "SAR", priced: 1, items: 2, waiting: 0, reviewing: 0, net: "100.00", preliminaries: "0.00", overheads: "0.00",
        profit: "0.00", adjustment: "0.00", total: "100.00", vat_rate: null, vat: null, total_with_vat: null, unpriced: ["3.2"],
      },
    });
    openApp("/tenders/t1");

    expect(await screen.findByText("2 items · 1 approved")).toBeInTheDocument();
    expect(screen.getByText("1 of 2 priced · SAR 100.00")).toBeInTheDocument();
  });

  it("shows what keeps the tender from release, with the Manager's reason for what he accepted", async () => {
    const finding = (severity: string, message: string, reason: string | null = null) => ({
      severity,
      message,
      refs: severity === "blocker" ? [{ label: "Bill.xlsx, page 1", document_id: "d1", page: 1 }] : [],
      accepted_by: reason ? "Rania" : null,
      reason,
    });
    fakeService({
      tenders: [tender],
      documents: [doc],
      priced: [priced("3.1", "approved", "100.00")],
      audit: [
        finding("blocker", "A row of the client's BOQ has a quantity but isn't in the BOQ."),
        finding("warning", "Quantix couldn't read 1 document: Site.kmz", "It only shows where the site is."),
      ],
    });
    openApp("/tenders/t1");

    expect(await screen.findByRole("heading", { name: "Before release" })).toBeInTheDocument();
    expect(screen.getByText(/1 thing must be fixed before the tender can go/)).toBeInTheDocument();
    expect(screen.getByText("A row of the client's BOQ has a quantity but isn't in the BOQ.")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Bill.xlsx, page 1" })).toHaveAttribute("href", "/tenders/t1/documents?doc=d1&page=1");
    expect(screen.getByText("Accepted by Rania: It only shows where the site is.")).toBeInTheDocument();
  });

  it("says what is with the Tender Manager before it comes to the engineer", async () => {
    const rania = {
      id: "s1",
      name: "Rania Farouk",
      role: "Tender Manager",
      is_manager: true,
      status: "active",
      now: null,
      profile: {},
    };
    const line = (id: string) => ({
      id,
      section: null,
      item: id,
      description: "Excavation",
      unit: "m3",
      quantity: "10",
      status: "proposed",
      proposed_by: "s2",
      reason: null,
      reviewed_by: null,
      review_note: null,
      source: { document_id: "d1", document_name: "Bill.xlsx", page: 1, quote: "" },
    });
    fakeService({ tenders: [tender], documents: [doc], staff: [rania], items: [line("3.1"), line("3.2")] });
    openApp("/tenders/t1");

    expect(await screen.findByText("2 pieces of work with Rania for review before they come to you.")).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Nothing needs you right now" })).toBeInTheDocument();
  });

  it("counts the enquiries the Manager accepted, for the engineer to send", async () => {
    const enquiry = {
      id: "e1",
      company: "Red Sea Membranes",
      email: null,
      subject: "Groundworks enquiry",
      body: "Please price item 3.1.",
      status: "draft",
      created_by: "s2",
      reviewed_by: "s1",
      review_note: "Scope matches the package.",
    };
    const groundworks = { id: "p1", name: "Groundworks", kind: "subcontract", items: [], enquiries: [enquiry], quotes: [] };
    fakeService({ tenders: [tender], documents: [doc], packages: [groundworks as unknown as Package] });
    openApp("/tenders/t1");

    expect(await screen.findByRole("heading", { name: "1 decision needs you" })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: /1 enquiry to send/ })).toHaveAttribute("href", "/tenders/t1/subcontract");
  });

  it("sets the due date where it is shown", async () => {
    const service = fakeService({ tenders: [tender], documents: [doc] });
    openApp("/tenders/t1");

    expect(await screen.findByText(/No due date yet\./)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Set the due date" }));
    await userEvent.type(screen.getByLabelText("Due date"), "2026-09-30");
    await userEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(service.state.tenders[0].due_date).toBe("2026-09-30"));
    expect(await screen.findByRole("button", { name: "Change" })).toBeInTheDocument();
    // and says it is the engineer's own date, not the tender documents'
    await userEvent.hover(screen.getByRole("button", { name: /^Where the due date comes from/ }));
    expect(screen.getByRole("note")).toHaveTextContent(
      "Set by you, not from the tender documentsYou set this date on 28 September 2026. It wasn't taken from the tender documents.",
    );
  });

  it("quotes the page a due date was taken from, one click from it", async () => {
    const source = {
      basis: "document" as const,
      set_by: "Salem",
      set_at: "2026-09-28T07:40:00Z",
      document_id: "d1",
      document_name: "ITT.pdf",
      page: 4,
      quote: "Tenders are due by 14 October 2026.",
    };
    fakeService({ tenders: [{ ...tender, due_date: "2026-10-14", due_date_source: source }], documents: [doc] });
    openApp("/tenders/t1");

    const note = await screen.findByRole("note");
    expect(note).toHaveTextContent("From the tender documents“Tenders are due by 14 October 2026.”");
    expect(within(note).getByRole("link", { name: "ITT.pdf, page 4" })).toHaveAttribute(
      "href",
      "/tenders/t1/documents?doc=d1&page=4",
    );
    expect(note).toHaveTextContent("Entered by Salem.");
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

describe("What the office learned", () => {
  const lesson = (id: string, text: string, source: string) =>
    ({ id, text, topic: "Rates", source, status: "tender", created_at: "2026-09-27T10:00:00Z" }) as const;
  const cubic = "A rate per m3 doesn't fall with a thinner layer: price the cubic metre.";

  it("keeps a lesson as a company rule for later tenders, or drops it", async () => {
    const service = fakeService({
      tenders: [tender],
      documents: [doc],
      lessons: [
        lesson("l1", cubic, "the rate for BOQ item C.2.7.3"),
        lesson("l2", "Road lines are laid by machine, several hundred metres an hour.", "the rate for BOQ item C.9.1"),
      ],
    });
    openApp("/tenders/t1");

    expect(await screen.findByRole("heading", { name: "What the office learned" })).toBeInTheDocument();
    expect(screen.getByText("From the rate for BOQ item C.2.7.3")).toBeInTheDocument();
    await userEvent.click(screen.getAllByRole("button", { name: "Keep for later tenders" })[0]);
    expect(await screen.findByRole("link", { name: "Kept as a company rule" })).toHaveAttribute("href", "/rules");
    expect(service.state.rules.map((r) => [r.topic, r.text])).toEqual([["Rates", cubic]]);

    await userEvent.click(screen.getByRole("button", { name: "Drop" }));
    await waitFor(() => expect(screen.queryByText(/Road lines/)).not.toBeInTheDocument());
    expect(screen.getByText(cubic)).toBeInTheDocument();
  });
});
