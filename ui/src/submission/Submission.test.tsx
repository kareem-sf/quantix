import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { fakeService, openApp } from "../test/app";
import type { Requirement } from "./queries";

const tender = { id: "t1", name: "Synthetic school", due_date: "2026-10-14", created_at: "2026-09-23T10:00:00Z" };
const layla = {
  id: "s3",
  name: "Layla Nasser",
  role: "Tender Coordinator",
  is_manager: false,
  status: "active",
  now: null,
  profile: {},
};
const requirement = (id: string, title: string, state: string, extra: Partial<Requirement> = {}): Requirement => ({
  id,
  section: "Commercial",
  title,
  document_id: "d1",
  document_name: "ITT.pdf",
  page: 4,
  quote: `7.3 ${title}`,
  added_by: "s3",
  state,
  draft: null,
  ready_note: null,
  file_name: null,
  ...extra,
});
const checklist = () => [
  requirement("r1", "Bid bond, 1% of the tender price", "missing"),
  requirement("r2", "Method statement for concrete works", "review", {
    section: "Technical",
    draft: { id: "dr1", title: "Method statement", body: "Pour sequence.", status: "proposed", proposed_by: "s3" },
  }),
  requirement("r3", "Site visit certificate", "ready", { ready_note: "" }),
];

describe("Submission", () => {
  it("lists what the tender requires, grouped, with where each stands", async () => {
    fakeService({ tenders: [tender], staff: [layla], requirements: checklist() });
    openApp("/tenders/t1/submission");

    expect(await screen.findByText("1 of 3 ready · submit by 14 Oct")).toBeInTheDocument();
    expect(screen.getByText("Technical")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Method statement for concrete works.*Draft · needs you/ })).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Missing · 1" }));
    expect(screen.queryByText("Site visit certificate")).not.toBeInTheDocument();
    expect(screen.getByText("Bid bond, 1% of the tender price")).toBeInTheDocument();
  });

  it("approves a draft after reading it and its source", async () => {
    const service = fakeService({ tenders: [tender], staff: [layla], requirements: checklist() });
    openApp("/tenders/t1/submission?item=r2");

    const panel = await screen.findByRole("complementary", { name: "Method statement for concrete works" });
    expect(within(panel).getByRole("link", { name: "ITT.pdf, page 4" })).toHaveAttribute("href", "/tenders/t1/documents?doc=d1&page=4");
    expect(within(panel).getByText("Drafted by Layla")).toBeInTheDocument();
    expect(within(panel).getByText("Pour sequence.")).toBeInTheDocument();
    await userEvent.click(within(panel).getByRole("button", { name: "Approve draft" }));
    await waitFor(() => expect(service.state.requirements[1].draft?.status).toBe("approved"));
  });

  it("marks a requirement the engineer provides as ready", async () => {
    const service = fakeService({ tenders: [tender], staff: [layla], requirements: checklist() });
    openApp("/tenders/t1/submission?item=r1");

    await userEvent.click(await screen.findByRole("button", { name: "I have this ready" }));
    await waitFor(() => expect(service.state.requirements[0].state).toBe("ready"));
  });

  it("builds the package on the engineer's computer and reports what is not ready", async () => {
    const service = fakeService({ tenders: [tender], staff: [layla], requirements: checklist() });
    openApp("/tenders/t1/submission");

    await userEvent.click(await screen.findByRole("checkbox", { name: "Markups in the rates" }));
    await userEvent.click(screen.getByRole("button", { name: "Build package · 2 not ready" }));
    expect(await screen.findByText("Built: Synthetic school 2026-09-23 1000")).toBeInTheDocument();
    expect(service.state.exports).toEqual([{ spread_markups: false }]);
    expect(
      screen.getByText("The priced BOQ adds up to 133,048.09, 0.01 over the tender total of 133,048.08 from rounding the rates."),
    ).toBeInTheDocument();
    expect(screen.getByText("Not ready: Bid bond.")).toBeInTheDocument();
  });
});
