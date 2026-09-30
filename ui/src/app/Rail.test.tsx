import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import type { BoqItem, Fact } from "../estimate/queries";
import type { Decision } from "../office/queries";
import { fakeService, openApp } from "../test/app";

const school = { id: "t1", name: "Al Noor Primary School", due_date: "2026-10-14", created_at: "2026-09-23T08:00:00Z" };
const clinic = { id: "t2", name: "Dammam Clinic", due_date: "2026-10-01", created_at: "2026-09-01T08:00:00Z" };
const mall = { id: "t3", name: "Khobar Mall · Phase 2", due_date: "2026-10-14", created_at: "2026-09-25T08:00:00Z" };
const warehouse = { id: "t4", name: "Riyadh Warehouse", due_date: null, created_at: "2026-09-20T08:00:00Z" };
const housing = { id: "t5", name: "Jubail Housing", due_date: "2026-09-30", created_at: "2026-08-01T08:00:00Z", archived: true };

const source = { document_id: "d1", document_name: "Bill.xlsx", page: 1, quote: "A2=3.1" };
const excavation: BoqItem = {
  id: "i1", section: null, item: "3.1", description: "Excavation", unit: "m3", quantity: "1240", status: "reviewed",
  proposed_by: "s2", reason: null, reviewed_by: "s1", review_note: "Checked.", source,
};
const currency: Fact = {
  id: "f1", kind: "currency", label: "Currency", value: "SAR", status: "reviewed", proposed_by: "s2", reviewed_by: "s1",
  review_note: "Read on ITT.pdf.", source,
};
const question: Decision = {
  id: "q1", raised_by: "s1", title: "Site support period", text: "72 working days or 4 months?", options: ["72 days"],
  subject_kind: null, subject_id: null, sources: null, status: "waiting", answer: null, created_at: "2026-09-28T09:00:00Z",
};
const sidebar = () => screen.getByRole("navigation", { name: "Quantix" });
const switcher = () => within(sidebar()).getByRole("button", { name: /Switch tender$/ });

describe("The sidebar", () => {
  it("says so while the Quantix service isn't answering", async () => {
    const service = fakeService();
    service.fetch.mockRejectedValue(new TypeError("Failed to fetch"));
    openApp("/library");

    expect(await within(sidebar()).findByText("Waiting for the Quantix service…")).toBeInTheDocument();
  });

  it("offers to start a tender when there is none", async () => {
    fakeService();
    const router = openApp("/library");

    await userEvent.click(await within(sidebar()).findByRole("link", { name: "Start a tender" }));
    expect(router.state.location.pathname).toBe("/new");
  });

  it("counts what waits on each of the tender's screens, and across the tenders on the Desk", async () => {
    fakeService({ tenders: [school], items: [excavation], facts: [currency], decisions: [question] });
    openApp("/tenders/t1/documents");

    expect(await within(sidebar()).findByLabelText("3 decisions need you")).toBeInTheDocument(); // on the Overview
    expect(within(sidebar()).getByRole("link", { name: /^Estimate/ })).toContainElement(
      within(sidebar()).getByLabelText("2 decisions need you in Estimate"),
    );
    expect(within(sidebar()).getByRole("link", { name: /^Desk/ })).toContainElement(
      await within(sidebar()).findByLabelText("3 decisions need you across your tenders"),
    );
    expect(within(sidebar()).getByRole("link", { name: "Documents" })).toHaveAttribute("aria-current", "page");

    await userEvent.keyboard("{Control>}b{/Control}"); // folded, each count is a dot that still says what it counts
    expect(within(sidebar()).getByRole("link", { name: "Estimate" })).toContainElement(
      within(sidebar()).getByLabelText("2 decisions need you in Estimate"),
    );
    expect(within(sidebar()).getByLabelText("2 decisions need you in Estimate")).toBeEmptyDOMElement();
  });

  it("switches to an open tender, soonest due first, with what waits in each", async () => {
    fakeService({ tenders: [school, clinic, mall, warehouse, housing], items: [excavation] });
    openApp("/tenders/t1");

    await userEvent.click(await within(sidebar()).findByRole("button", { name: "Al Noor Primary School, due 14 Oct. Switch tender" }));
    const menu = screen.getByRole("menu", { name: "Tenders" });
    expect(within(menu).getAllByRole("menuitem").map((t) => t.textContent)).toEqual([
      "DCDammam Clinicdue 1 Oct1",
      "KMKhobar Mall · Phase 2due 14 Oct1", // due the same day as Al Noor, and started later
      "ANAl Noor Primary Schooldue 14 Oct1",
      "RWRiyadh Warehouseno due date1",
    ]);
  });

  it("keeps the tender on screen in the switcher even once it is archived", async () => {
    fakeService({ tenders: [school, housing] });
    openApp("/tenders/t5");

    await userEvent.click(await within(sidebar()).findByRole("button", { name: /^Jubail Housing/ }));
    const listed = within(screen.getByRole("menu", { name: "Tenders" })).getAllByRole("menuitem");
    expect(listed.map((t) => t.textContent)).toEqual([expect.stringContaining("Jubail Housing"), expect.stringContaining("Al Noor")]);
  });

  it("closes the switcher with Esc, a click elsewhere, or on starting a new tender", async () => {
    fakeService({ tenders: [school, warehouse] });
    const router = openApp("/tenders/t1");

    await userEvent.click(await within(sidebar()).findByRole("button", { name: /Switch tender$/ }));
    await userEvent.keyboard("{Escape}");
    expect(screen.queryByRole("menu", { name: "Tenders" })).not.toBeInTheDocument();
    expect(switcher()).toHaveAttribute("aria-expanded", "false");

    await userEvent.click(switcher());
    await userEvent.click(screen.getByRole("main"));
    expect(screen.queryByRole("menu", { name: "Tenders" })).not.toBeInTheDocument();

    await userEvent.click(switcher());
    await userEvent.click(within(screen.getByRole("menu", { name: "Tenders" })).getByRole("link", { name: "New tender" }));
    expect(router.state.location.pathname).toBe("/new");
    expect(screen.queryByRole("menu", { name: "Tenders" })).not.toBeInTheDocument();
  });

  it("tells tenders apart by two letters when folded", async () => {
    fakeService({ tenders: [mall] });
    openApp("/tenders/t3");

    await userEvent.keyboard("{Control>}b{/Control}");
    expect(await within(sidebar()).findByRole("button", { name: /Switch tender$/ })).toHaveTextContent(/^KM$/);
    expect(switcher()).toHaveAttribute("title", "Khobar Mall · Phase 2");
  });
});
