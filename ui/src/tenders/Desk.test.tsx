import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { BoqItem } from "../estimate/queries";
import type { Decision, Staff } from "../office/queries";
import { fakeService, openApp, type FakeState } from "../test/app";
import type { TenderGlance } from "./queries";

const blank: TenderGlance = {
  id: "", name: "", due_date: null, outcome: "open", outcome_at: null, archived: false, created_at: "2026-09-01T08:00:00Z",
  team: "idle", doing: null, waiting: 0, documents: 0, read: 0, items: 0, priced: 0, currency: "", total: null, packages: 0,
  chosen: 0, requirements: 0, ready: 0,
};

/** Every tender at a glance, exactly as given, as the service would count them. */
function desk(glances: Partial<TenderGlance>[], state: Partial<FakeState> = {}) {
  const all = glances.map((g, n) => ({ ...blank, id: `t${n + 1}`, ...g }));
  const service = fakeService({
    tenders: all.map(({ id, name, due_date, created_at, outcome, archived }) => ({ id, name, due_date, created_at, outcome, archived })),
    ...state,
  });
  const fake = service.fetch.getMockImplementation()!;
  service.fetch.mockImplementation(async (input, init) =>
    typeof input !== "string" && new URL(input.url).pathname === "/api/desk"
      ? new Response(JSON.stringify(all), { headers: { "Content-Type": "application/json" } })
      : fake(input, init),
  );
  return service;
}

const figure = (label: string) => screen.getByText(label, { selector: "span" }).parentElement;
/** A table's rows under its header, on the page or in one of its regions. */
const rows = (region?: string) =>
  (region ? within(screen.getByRole("region", { name: region })) : screen).getAllByRole("row").slice(1);

beforeEach(() => {
  vi.useFakeTimers({ toFake: ["Date"] });
  vi.setSystemTime(new Date(2026, 9, 5, 9, 30)); // Monday 5 October 2026
});
afterEach(() => vi.useRealTimers());

describe("Desk", () => {
  it("counts a few figures across the tenders, each from Quantix's own counts", async () => {
    desk([
      { name: "Al Noor Primary School", due_date: "2026-10-08", currency: "SAR", total: "1250000.00" },
      { name: "Riyadh Warehouse", currency: "SAR", total: "480000.00" },
      { name: "Jubail Housing", outcome: "won", outcome_at: "2026-05-01T10:00:00Z" },
      { name: "Dammam Clinic", outcome: "lost", outcome_at: "2025-11-01T10:00:00Z" },
      { name: "Khobar Mall", outcome: "submitted", outcome_at: "2026-09-01T10:00:00Z" },
      { name: "Tabuk School", archived: true, due_date: "2026-10-06", total: "9000000.00", currency: "SAR" },
    ]);
    openApp("/desk");

    expect(await screen.findByText("Monday 5 October")).toBeInTheDocument();
    expect(figure("Open tenders")).toHaveTextContent("Open tenders21 closes this week");
    expect(figure("Priced so far")).toHaveTextContent("Priced so farSAR 1.7M2 of 2 open tenders priced, before VAT");
    expect(figure("Closed this year")).toHaveTextContent("Closed this year21 awaiting a result");
    expect(figure("Won")).toHaveTextContent("Won50%1 won of 2 decided");
  });

  it("shows a dash where there is nothing to count yet", async () => {
    desk([{ name: "Al Noor Primary School", due_date: "2026-10-30" }]);
    openApp("/desk");

    expect(await screen.findByText("None close this week")).toBeInTheDocument();
    expect(figure("Priced so far")).toHaveTextContent("Priced so far–0 of 1 open tenders priced, before VAT");
    expect(figure("Won")).toHaveTextContent("Won–0 won of 0 decided");
  });

  it("never adds up prices in different currencies", async () => {
    desk([
      { name: "Al Noor Primary School", currency: "SAR", total: "1250000.00" },
      { name: "Dubai Depot", currency: "AED", total: "800000.00" },
    ]);
    openApp("/desk");

    await screen.findByRole("heading", { name: "Nothing needs you right now" });
    expect(figure("Priced so far")).toHaveTextContent("Priced so far2 tenders2 of 2 open tenders priced, before VAT");
  });

  it("puts the tenders closing soonest first, and says how near each is", async () => {
    desk([
      { name: "Hail Depot", due_date: "2026-10-25" },
      { name: "Riyadh Warehouse" },
      { name: "Al Noor Primary School", due_date: "2026-10-05", currency: "SAR", total: "1250000.00" },
      { name: "Dammam Clinic", due_date: "2026-10-06" },
      { name: "Khobar Mall", due_date: "2026-10-08" },
      { name: "Tabuk School", due_date: "2026-10-01" },
    ]);
    openApp("/desk");

    await screen.findByRole("heading", { name: "Nothing needs you right now" });
    const closing = within(screen.getByRole("region", { name: "Closing dates" })).getAllByRole("link");
    expect(closing.map((l) => l.textContent)).toEqual([
      "1OctTabuk SchoolCloses 1 Oct",
      "5OctAl Noor Primary SchoolCloses 5 Oct · SAR 1.3M",
      "6OctDammam ClinicCloses 6 Oct",
      "8OctKhobar MallCloses 8 Oct",
      "25OctHail DepotCloses 25 Oct",
      "–dateRiyadh WarehouseNo due date yet",
    ]);
    expect(rows("Open tenders").map((row) => within(row).getAllByRole("cell")[1].textContent)).toEqual([
      "1 Octclosed",
      "5 Octtoday",
      "6 Octtomorrow",
      "8 Oct3 days",
      "25 Oct20 days",
      "no due date",
    ]);
  });

  it("shows where each open tender stands, its total and what waits in it, and opens it where the engineer left it", async () => {
    localStorage.setItem("quantix.place", JSON.stringify({ last: "t1", screens: { t1: "/tenders/t1/estimate" } }));
    desk([
      {
        name: "Al Noor Primary School", team: "working", doing: "Pricing the concrete", waiting: 2, documents: 12, read: 12,
        items: 10, priced: 7, currency: "SAR", total: "1250000.00",
      },
    ]);
    const router = openApp("/tenders");

    const [row] = await screen.findAllByRole("row").then((all) => all.slice(1));
    expect(within(row).getAllByRole("cell").map((c) => c.textContent)).toEqual([
      "Al Noor Primary School", "no due date", "Pricing · 7 of 10", "SAR 1,250,000.00", "2",
    ]);
    expect(within(row).getByTitle("The team is working")).toBeInTheDocument();
    await userEvent.click(within(row).getByText("Pricing · 7 of 10"));
    expect(router.state.location.pathname).toBe("/tenders/t1/estimate");
  });

  it("lists what needs the engineer under each tender, each one click from where it is decided", async () => {
    const rania: Staff = { id: "s1", name: "Rania Farouk", role: "Tender Manager", is_manager: true, status: "active", now: null, profile: {} };
    const asked = (id: string, raisedBy: string, title: string): Decision => ({
      id, raised_by: raisedBy, title, text: "72 working days or 4 months?", options: ["72 days"], subject_kind: null,
      subject_id: null, sources: null, status: "waiting", answer: null, created_at: "2026-10-04T09:00:00Z",
    });
    const line: BoqItem = {
      id: "i1", section: null, item: "3.1", description: "Excavation", unit: "m3", quantity: "1240", status: "reviewed",
      proposed_by: "s2", reason: null, reviewed_by: "s1", review_note: null,
      source: { document_id: "d1", document_name: "Bill.xlsx", page: 1, quote: "A2=3.1" },
    };
    fakeService({
      tenders: [{ id: "t1", name: "Al Noor Primary School", due_date: "2026-10-06", created_at: "2026-09-23T08:00:00Z" }],
      staff: [rania],
      items: [line],
      decisions: [asked("q1", "s1", "Site support period"), asked("q2", "s7", "Bond wording")],
    });
    const router = openApp("/desk");

    expect(await screen.findByRole("heading", { name: "3 things need you" })).toBeInTheDocument();
    const needs = screen.getByRole("region", { name: "Needs you" });
    expect(within(needs).getByRole("link", { name: "Al Noor Primary School tomorrow" })).toHaveAttribute("href", "/tenders/t1");
    expect(await within(needs).findByRole("link", { name: /1 BOQ item to approve/ })).toHaveAttribute(
      "href",
      "/tenders/t1/estimate?show=waiting",
    );
    expect(within(needs).getByRole("button", { name: /^Bond wording/ })).toBeInTheDocument(); // not on the team any more
    await userEvent.click(await within(needs).findByRole("button", { name: /^Rania asks: Site support period/ }));

    expect(router.state.location.pathname).toBe("/tenders/t1");
    const panel = await screen.findByRole("complementary", { name: "Team" });
    expect(within(panel).getByRole("heading", { name: "Rania Farouk" })).toBeInTheDocument();
  });

  it("says who is working now, and when every tender is with the team", async () => {
    desk([
      { name: "Al Noor Primary School", team: "working", doing: "Pricing the concrete" },
      { name: "Riyadh Warehouse", team: "working" },
      { name: "Dammam Clinic" },
    ]);
    openApp("/desk");

    expect(await screen.findByText("Every tender is with the team.")).toBeInTheDocument();
    const working = within(screen.getByRole("region", { name: "Working now" })).getAllByRole("link");
    expect(working.map((l) => l.textContent)).toEqual(["Al Noor Primary SchoolPricing the concrete", "Riyadh WarehouseWorking"]);
  });

  it("says so when no tender is open", async () => {
    desk([{ name: "Jubail Housing", outcome: "won", outcome_at: "2026-05-01T10:00:00Z" }]);
    openApp("/desk");

    expect(await screen.findByText("No open tenders.")).toBeInTheDocument();
    expect(screen.getByText("No team is working.")).toBeInTheDocument();
    expect(within(screen.getByRole("region", { name: "Open tenders" })).getByText("None.")).toBeInTheDocument();
  });
});

describe("The tenders register", () => {
  const closed = [
    { name: "Al Noor Primary School", due_date: "2026-10-30" },
    { name: "Riyadh Warehouse" },
    { name: "Khobar Mall", due_date: "2026-10-08" },
    { name: "Hail Depot", outcome: "submitted" as const, outcome_at: "2026-10-02T10:00:00Z" },
    { name: "Tabuk School", outcome: "submitted" as const, outcome_at: "2026-09-01T10:00:00Z", created_at: "2026-09-30T08:00:00Z" },
    { name: "Jubail Housing", outcome: "won" as const, outcome_at: "2026-05-01T10:00:00Z", documents: 3, read: 3, items: 5, priced: 5 },
  ];

  it("lists open tenders soonest due first", async () => {
    desk(closed);
    openApp("/tenders");

    await screen.findByRole("tab", { name: "Open 3", selected: true });
    expect(rows().map((row) => within(row).getAllByRole("cell")[0].textContent)).toEqual([
      "Khobar Mall", "Al Noor Primary School", "Riyadh Warehouse",
    ]);
  });

  it("lists closed tenders latest decided first, with how each went and the stage it reached", async () => {
    desk(closed);
    const router = openApp("/tenders");

    await userEvent.click(await screen.findByRole("tab", { name: "Submitted 2" }));
    expect(router.state.location.search).toBe("?show=submitted");
    expect(screen.getByRole("columnheader", { name: "Stage reached" })).toBeInTheDocument();
    expect(rows().map((row) => [...within(row).getAllByRole("cell")].slice(0, 2).map((c) => c.textContent))).toEqual([
      ["Hail Depot", "Submitted 2 Oct 2026"],
      ["Tabuk School", expect.stringMatching(/^Submitted 1 Sept? 2026$/)],
    ]);

    await userEvent.click(screen.getByRole("tab", { name: "Won 1" }));
    expect(within(rows()[0]).getAllByRole("cell")[2]).toHaveTextContent("Submission · no checklist");
    await userEvent.click(screen.getByRole("tab", { name: "Open 3" }));
    expect(router.state.location.search).toBe("");
  });

  it("opens on the tab named in the address", async () => {
    desk(closed);
    openApp("/tenders?show=won");

    expect(await screen.findByRole("tab", { name: "Won 1" })).toHaveAttribute("aria-selected", "true");
    expect(screen.getByRole("link", { name: "Jubail Housing" })).toBeInTheDocument();
  });

  it("finds a tender by every word typed", async () => {
    desk(closed);
    openApp("/tenders");

    await userEvent.type(await screen.findByLabelText("Find a tender"), "school noor");
    expect(rows().map((row) => within(row).getAllByRole("cell")[0].textContent)).toEqual(["Al Noor Primary School"]);
    await userEvent.type(screen.getByLabelText("Find a tender"), " warehouse");
    expect(screen.getByText("None.")).toBeInTheDocument();
  });
});
