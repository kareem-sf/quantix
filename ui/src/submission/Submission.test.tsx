import { cleanup, screen, waitFor, within } from "@testing-library/react";
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
  reviewed_by: "s1",
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
    draft: {
      id: "dr1",
      title: "Method statement",
      body: "Pour sequence.",
      status: "reviewed",
      proposed_by: "s3",
      reviewed_by: "s1",
      review_note: "Follows clause 7.6.",
    },
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

  it("removes a duplicate from the checklist after asking", async () => {
    const service = fakeService({ tenders: [tender], staff: [layla], requirements: checklist() });
    openApp("/tenders/t1/submission?item=r3");

    await userEvent.click(await screen.findByRole("button", { name: "Remove from checklist" }));
    await userEvent.click(screen.getByRole("button", { name: "Keep" }));
    expect(service.state.requirements).toHaveLength(3);
    await userEvent.click(screen.getByRole("button", { name: "Remove from checklist" }));
    await userEvent.click(screen.getByRole("button", { name: "Remove it" }));
    await waitFor(() => expect(service.state.requirements.map((r) => r.id)).toEqual(["r1", "r2"]));
  });

  it("marks a requirement the engineer provides as ready", async () => {
    const service = fakeService({ tenders: [tender], staff: [layla], requirements: checklist() });
    openApp("/tenders/t1/submission?item=r1");

    await userEvent.click(await screen.findByRole("button", { name: "I have this ready" }));
    await waitFor(() => expect(service.state.requirements[0].state).toBe("ready"));
  });

  it("reopens a draft the engineer approved, with the reason", async () => {
    const approved = { id: "dr2", title: "Programme", body: "Earthworks first.", status: "approved", proposed_by: "s3", reviewed_by: "s1", review_note: "Covers every line." };
    const service = fakeService({ tenders: [tender], staff: [layla], requirements: [requirement("r4", "Programme", "ready", { draft: approved })] });
    openApp("/tenders/t1/submission?item=r4");

    await userEvent.click(await screen.findByRole("button", { name: "Reopen" }));
    await userEvent.type(screen.getByLabelText("Why it needs doing again"), "Add the yard gravel line.");
    await userEvent.click(screen.getByRole("button", { name: "Send" }));
    await waitFor(() => expect(service.state.reopened).toEqual([{ kind: "draft", id: "dr2", reason: "Add the yard gravel line." }]));
  });

  it("builds the package on the engineer's computer and reports what is not ready", async () => {
    const blocker = (message: string) => ({ severity: "blocker", message, refs: [], accepted_by: null, reason: null });
    const audit = [blocker("1 checklist item isn’t ready: Bid bond"), blocker("VAT isn't recorded.")];
    const service = fakeService({ tenders: [tender], staff: [layla], requirements: checklist(), audit });
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

  it("says when there is no checklist yet", async () => {
    fakeService({ tenders: [{ ...tender, due_date: null }] });
    openApp("/tenders/t1/submission");

    expect(await screen.findByText(/No checklist yet\. Ask the office to list what the tender requires/)).toBeInTheDocument();
    expect(screen.getByText("What the tender asks you to submit")).toBeInTheDocument();
  });

  it("adds a requirement the office missed", async () => {
    const service = fakeService({ tenders: [tender], staff: [layla], requirements: checklist() });
    openApp("/tenders/t1/submission");

    await userEvent.click(await screen.findByRole("button", { name: "Add a requirement" }));
    expect(screen.getByLabelText("Section")).toHaveValue("Commercial");
    await userEvent.type(screen.getByLabelText("Requirement"), "Zakat certificate");
    await userEvent.click(screen.getByRole("button", { name: "Add" }));

    await waitFor(() => expect(service.state.requirements.at(-1)).toMatchObject({ section: "Commercial", title: "Zakat certificate" }));
    expect(await screen.findByRole("button", { name: /^Zakat certificate.*Added by you.*Missing$/ })).toBeInTheDocument();
    expect(screen.getByLabelText("Requirement")).toHaveValue("");
  });

  it("says why a requirement couldn't be added", async () => {
    const refused = "The checklist already has that requirement.";
    fakeService({ tenders: [tender], requirements: checklist(), fail: { "/tenders/t1/requirements": refused } });
    openApp("/tenders/t1/submission");

    await userEvent.click(await screen.findByRole("button", { name: "Add a requirement" }));
    await userEvent.type(screen.getByLabelText("Requirement"), "Site visit certificate");
    await userEvent.click(screen.getByRole("button", { name: "Add" }));
    expect(await screen.findByText(refused)).toBeInTheDocument();
  });

  it("adds the engineer's own file to a requirement, which makes it ready", async () => {
    const service = fakeService({ tenders: [tender], staff: [layla], requirements: checklist() });
    openApp("/tenders/t1/submission?item=r1");

    const panel = await screen.findByRole("complementary", { name: "Bid bond, 1% of the tender price" });
    await userEvent.upload(within(panel).getByLabelText("Add the file"), new File(["%PDF-1.7"], "Bid bond.pdf", { type: "application/pdf" }));
    await waitFor(() => expect(service.state.requirements[0]).toMatchObject({ file_name: "Bid bond.pdf", state: "ready" }));
    expect(await within(panel).findByText("File added: Bid bond.pdf")).toBeInTheDocument();
    expect(within(panel).getByText("Replace the file")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /^Bid bond, 1% of the tender price.*Ready · file added$/ })).toBeInTheDocument();
  });

  it("says why a file couldn't be added", async () => {
    const refused = "Quantix keeps PDF, Word, Excel and image files.";
    fakeService({ tenders: [tender], requirements: checklist(), fail: { "/requirements/r1/file": refused } });
    openApp("/tenders/t1/submission?item=r1");

    const panel = await screen.findByRole("complementary", { name: "Bid bond, 1% of the tender price" });
    await userEvent.upload(within(panel).getByLabelText("Add the file"), new File(["MZ"], "setup.exe"));
    expect(await within(panel).findByText(refused)).toBeInTheDocument();
  });

  it("sends a draft back with what to change", async () => {
    const service = fakeService({ tenders: [tender], staff: [layla], requirements: checklist() });
    openApp("/tenders/t1/submission?item=r2");

    const panel = await screen.findByRole("complementary", { name: "Method statement for concrete works" });
    expect(within(panel).getByText(/Reviewed by the Manager/)).toHaveTextContent("Reviewed by the Manager: Follows clause 7.6.");
    await userEvent.click(within(panel).getByRole("button", { name: "Send back" }));
    const reason = within(panel).getByLabelText("What to change");
    expect(reason).toHaveAttribute("placeholder", "Tell Layla what to change");
    await userEvent.type(reason, "Add the pour sequence for the raft.");
    await userEvent.click(within(panel).getByRole("button", { name: "Send back" }));

    await waitFor(() => expect(service.state.decided).toEqual([{ id: "dr1", approve: false, reason: "Add the pour sequence for the raft." }]));
    expect(service.state.requirements[1].state).toBe("missing");
  });

  it("keeps a draft with the Manager until he has reviewed it, and says when the office approved one itself", async () => {
    const draft = (id: string, status: string) =>
      ({ id, title: "Draft", body: "Earthworks first.", status, proposed_by: "s3", reviewed_by: status === "proposed" ? null : "s1", review_note: null });
    fakeService({
      tenders: [tender], staff: [layla],
      requirements: [
        requirement("r5", "Programme", "manager", { draft: draft("dr5", "proposed") }),
        requirement("r6", "Quality plan", "ready", { draft: draft("dr6", "office_approved") }),
      ],
    });
    openApp("/tenders/t1/submission?item=r5");

    const manager = await screen.findByRole("complementary", { name: "Programme" });
    expect(within(manager).getByText("With the Manager for review")).toBeInTheDocument();
    expect(within(manager).queryByRole("button", { name: "Approve draft" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: /^Programme.*Draft · with the Manager$/ })).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: /^Quality plan.*Ready · approved by the office after the Manager's review$/ }));
    const office = await screen.findByRole("complementary", { name: "Quality plan" });
    expect(within(office).getByText("Drafted by Layla · approved by the office, not reviewed by you")).toBeInTheDocument();
    expect(within(office).getByRole("button", { name: "Reopen" })).toBeInTheDocument();
  });

  it("takes back a mark of ready", async () => {
    const service = fakeService({
      tenders: [tender], staff: [layla],
      requirements: [requirement("r3", "Site visit certificate", "ready", { ready_note: "Signed copy in the safe" })],
    });
    openApp("/tenders/t1/submission?item=r3");

    const panel = await screen.findByRole("complementary", { name: "Site visit certificate" });
    expect(within(panel).getByText("Marked ready: Signed copy in the safe")).toBeInTheDocument();
    expect(within(panel).queryByRole("button", { name: "I have this ready" })).not.toBeInTheDocument();
    await userEvent.click(within(panel).getByRole("button", { name: "Not ready after all" }));
    await waitFor(() => expect(service.state.requirements[0]).toMatchObject({ ready_note: null, state: "missing" }));
  });

  it("closes a requirement with Esc or Close", async () => {
    fakeService({ tenders: [tender], staff: [layla], requirements: checklist() });
    openApp("/tenders/t1/submission?item=r1");

    await screen.findByRole("complementary", { name: "Bid bond, 1% of the tender price" });
    await userEvent.keyboard("{Escape}");
    expect(screen.queryByRole("complementary")).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: /^Bid bond, 1% of the tender price/ }));
    await userEvent.click(within(screen.getByRole("complementary")).getByRole("button", { name: "Close" }));
    expect(screen.queryByRole("complementary")).not.toBeInTheDocument();
  });

  // Bug: closing a requirement drops the list's filter (Submission.tsx:232 and :244 set no params), unlike opening one
  // (keep) and unlike the Estimate's panel, so the engineer working through "Needs you" is thrown back to All.
  it.fails("keeps the filter the engineer chose when a requirement is closed", async () => {
    fakeService({ tenders: [tender], staff: [layla], requirements: checklist() });
    const router = openApp("/tenders/t1/submission?show=review");

    await userEvent.click(await screen.findByRole("button", { name: /^Method statement for concrete works/ }));
    expect(router.state.location.search).toBe("?show=review&item=r2");
    await userEvent.keyboard("{Escape}");
    expect(router.state.location.search).toBe("?show=review");
    expect(screen.queryByText("Bid bond, 1% of the tender price")).not.toBeInTheDocument();
  });

  it("says where the priced BOQ goes in the client's own workbook, or that it will be in Quantix's layout", async () => {
    const columns = [{ document_name: "Bill.xlsx", sheet: 2, rate_column: "F", amount_column: "G" }];
    fakeService({ tenders: [tender], requirements: checklist(), columns });
    openApp("/tenders/t1/submission");
    expect(await screen.findByText("Priced BOQ in the client’s Bill.xlsx, sheet 2: rates in F, amounts in G.")).toBeInTheDocument();
    cleanup();

    fakeService({ tenders: [tender], requirements: checklist() });
    openApp("/tenders/t1/submission");
    expect(
      await screen.findByText("The office hasn’t matched the client’s BOQ columns yet, so the priced BOQ will be in Quantix’s layout."),
    ).toBeInTheDocument();
  });

  it("says when the priced BOQ adds up to the tender total, and when rounding took it under", async () => {
    const service = fakeService({ tenders: [tender], requirements: checklist() });
    let totals = ["133048.08", "133048.08"];
    const fake = service.fetch.getMockImplementation()!;
    service.fetch.mockImplementation(async (input, init) => {
      if (typeof input === "string" || !input.url.endsWith("/export")) return fake(input, init);
      const [priced_total, summary_total] = totals;
      const built = { folder: "Synthetic school 2026-09-23 1000", files: ["Priced Bill.xlsx"], priced_total, summary_total, factor: "1", not_ready: [] };
      return new Response(JSON.stringify(built), { headers: { "Content-Type": "application/json" } });
    });
    openApp("/tenders/t1/submission");

    await userEvent.click(await screen.findByRole("button", { name: "Build package" }));
    expect(await screen.findByText("The priced BOQ adds up to 133,048.08, the tender total.")).toBeInTheDocument();
    expect(screen.queryByText(/^Not ready/)).not.toBeInTheDocument();
    totals = ["133048.00", "133048.08"];
    await userEvent.click(screen.getByRole("button", { name: "Build package" }));
    expect(
      await screen.findByText("The priced BOQ adds up to 133,048.00, 0.08 under the tender total of 133,048.08 from rounding the rates."),
    ).toBeInTheDocument();
  });

  it("opens the built package's folder on the engineer's computer", async () => {
    const service = fakeService({ tenders: [tender], requirements: checklist() });
    openApp("/tenders/t1/submission");

    await userEvent.click(await screen.findByRole("button", { name: "Build package" }));
    await userEvent.click(await screen.findByRole("button", { name: "Open folder" }));
    await waitFor(() => expect(service.state.folders).toEqual(["Synthetic school 2026-09-23 1000"]));
  });

  it("says why the package couldn't be built, or its folder opened", async () => {
    const service = fakeService({
      tenders: [tender], requirements: checklist(),
      fail: { "/tenders/t1/export": "Close Priced Bill.xlsx in Excel, then build again." },
    });
    openApp("/tenders/t1/submission");

    await userEvent.click(await screen.findByRole("button", { name: "Build package" }));
    expect(await screen.findByText("Close Priced Bill.xlsx in Excel, then build again.")).toBeInTheDocument();

    service.state.fail = { "/exports/Synthetic%20school%202026-09-23%201000/open": "That package is not in the exports folder." };
    await userEvent.click(screen.getByRole("button", { name: "Build package" }));
    await userEvent.click(await screen.findByRole("button", { name: "Open folder" }));
    expect(await screen.findByText("That package is not in the exports folder.")).toBeInTheDocument();
  });

  it("leaves building the package to the engineer, even when the office works on its own", async () => {
    const autonomous = { office_mode: "autonomous" as const, office_ai: null, tender_allowance: null, notifications: "all" as const };
    const service = fakeService({ tenders: [tender], staff: [layla], requirements: checklist(), settings: autonomous });
    openApp("/tenders/t1/submission");

    expect(await screen.findByRole("button", { name: "Build package" })).toBeEnabled();
    expect(screen.getByText("The package is built on your computer. Quantix never sends anything to the client.")).toBeInTheDocument();
    expect(service.state.exports).toEqual([]);
  });
});
