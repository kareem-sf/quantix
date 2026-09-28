import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { fakeService, openApp } from "../test/app";
import type { Decision, Message, Staff } from "./queries";

const tender = { id: "t1", name: "Synthetic school", due_date: null, created_at: "2026-09-23T10:00:00Z" };
const rania: Staff = {
  id: "s1",
  name: "Rania Farouk",
  role: "Tender Manager",
  is_manager: true,
  status: "active",
  now: "Reviewing Omar's result",
  profile: { experience_years: 19, opinions: "Distrusts quantities nobody has measured." },
};
const omar: Staff = {
  id: "s2",
  name: "Omar Haddad",
  role: "Quantity Surveyor",
  is_manager: false,
  status: "active",
  now: null,
  profile: { experience_years: 11, working_style: "Measures twice.", opinions: "Never trusts a BOQ quantity." },
};
const at = "2026-09-23T10:42:00Z";
const said = (id: number, sender: string, channel: string, text: string, kind = "message"): Message => ({
  id,
  sender,
  channel,
  kind,
  text,
  sources: null,
  created_at: at,
});
const question: Decision = {
  id: "q1",
  raised_by: "s1",
  title: "Tender security wording",
  text: "The conditions ask for a 1% tender security. Shall we price the bond at 1%?",
  options: ["Yes, 1%", "Ask the client first"],
  subject_kind: null,
  subject_id: null,
  sources: null,
  status: "waiting",
  answer: null,
  created_at: at,
};
const ready = { office_mode: "engineer" as const, office_ai: { connection_id: "c1", model: "m" }, tender_allowance: null };

describe("Overview and decisions", () => {
  it("asks for the office's AI before the team can start", async () => {
    fakeService({ tenders: [tender] });
    openApp("/tenders/t1");
    expect(await screen.findByText(/so the team can start work/)).toBeInTheDocument();
  });

  it("lists what needs the engineer and takes an answer", async () => {
    const service = fakeService({
      tenders: [tender],
      settings: ready,
      staff: [rania, omar],
      decisions: [question],
      messages: [said(1, "s1", "s1", "Omar found the tender security clause.")],
    });
    const router = openApp("/tenders/t1");

    expect(await screen.findByRole("heading", { name: "1 decision needs you" })).toBeInTheDocument();
    expect(await screen.findByText("Omar found the tender security clause.")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("link", { name: /Tender security wording/ }));

    await userEvent.click(await screen.findByRole("radio", { name: /Yes, 1%/ }));
    await userEvent.click(screen.getByRole("button", { name: "Confirm" }));
    await waitFor(() => expect(router.state.location.pathname).toBe("/tenders/t1"));
    expect(service.state.decisions[0]).toMatchObject({ status: "answered", answer: "Yes, 1%" });
  });

  it("shows an escalation with where it shows and what Quantix found, and takes the engineer's own answer", async () => {
    const escalation: Decision = {
      ...question,
      id: "q2",
      title: "The rate for BOQ item 3.1",
      text: "Omar keeps pricing 3.1 as soft digging, but the site is rock.",
      options: ["Price excavation in rock with a breaker", "Ask the client for the soil report"],
      subject_kind: "rate",
      subject_id: "r9",
      sources: [
        { label: "BOQ line 3.1", document_id: null, page: null, boq_item_id: "i31" },
        { label: "Soils.pdf, page 4: Rock at 1.2 m", document_id: "d7", page: 4, boq_item_id: null },
      ],
      status: "waiting",
      answer: null,
    };
    const warning = { severity: "warning", message: "18.00 is -40% from the firm's own rates.", refs: [] };
    const service = fakeService({
      tenders: [tender],
      settings: ready,
      staff: [rania, omar],
      decisions: [escalation],
      findings: { r9: [{ ...warning, accepted_by: null, reason: null }] },
    });
    openApp("/tenders/t1/decisions/q2");

    expect(await screen.findByRole("heading", { name: "The rate for BOQ item 3.1" })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "BOQ line 3.1" })).toHaveAttribute("href", "/tenders/t1/estimate?item=i31");
    expect(screen.getByRole("link", { name: "Soils.pdf, page 4: Rock at 1.2 m" })).toHaveAttribute(
      "href",
      "/tenders/t1/documents?doc=d7&page=4",
    );
    expect(await screen.findByText("18.00 is -40% from the firm's own rates.")).toBeInTheDocument();
    expect(screen.getByRole("radio", { name: /Price excavation in rock with a breaker/ })).toBeInTheDocument();
    // the Manager's recommended correction comes first, and says so
    const [first, second] = screen.getAllByRole("radio").map((r) => r.closest("label")!);
    expect(first).toHaveTextContent("Recommended");
    expect(second).not.toHaveTextContent("Recommended");

    await userEvent.type(screen.getByLabelText("Or answer in your own words"), "Use the rock rate from the depot job.");
    await userEvent.click(screen.getByRole("button", { name: "Confirm" }));
    await waitFor(() =>
      expect(service.state.decisions[0]).toMatchObject({ answer: "Use the rock rate from the depot job." }),
    );
  });

  it("sends the engineer's request to the Manager", async () => {
    const service = fakeService({ tenders: [tender], settings: ready, staff: [rania] });
    openApp("/tenders/t1");

    await userEvent.type(await screen.findByLabelText("Message the office"), "Review the package{Enter}");
    await waitFor(() => expect(service.state.messages.at(-1)).toMatchObject({ channel: "s1", text: "Review the package" }));
  });
});

describe("Office", () => {
  it("shows the team room as the team wrote it, and who each person is", async () => {
    fakeService({
      tenders: [tender],
      settings: ready,
      staff: [rania, omar],
      officeState: "working",
      messages: [
        said(1, "engineer", "team", "Check the tender security, please."),
        said(2, "s1", "team", "Rania asked Omar to find the tender security clause", "task"),
        said(3, "s2", "team", "The bond wording is unusual; it may be unconditional.", "concern"),
      ],
    });
    openApp("/tenders/t1/office");

    const room = await screen.findByRole("region", { name: "Conversation" });
    expect(await within(room).findByText("Check the tender security, please.")).toBeInTheDocument();
    expect(within(room).getByText("Rania asked Omar to find the tender security clause")).toBeInTheDocument();
    expect(within(room).getByText("· raised a concern")).toBeInTheDocument();
    expect(within(screen.getByRole("navigation", { name: "Quantix" })).getByText("Reviewing Omar's result")).toBeInTheDocument();

    await userEvent.click(within(room).getByRole("button", { name: "About Omar Haddad" }));
    const profile = await screen.findByRole("complementary", { name: "Omar Haddad" });
    expect(within(profile).getByText("Never trusts a BOQ quantity.")).toBeInTheDocument();
    expect(within(profile).getByText("Idle")).toBeInTheDocument();
  });

  it("shows a released person's unfinished tasks as not done", async () => {
    const nora: Staff = { ...omar, id: "s3", name: "Nora Al-Otaibi", status: "released" };
    fakeService({
      tenders: [tender],
      settings: ready,
      staff: [rania, nora],
      messages: [said(1, "s3", "team", "The insurance annexure lists six covers.")],
      tasks: [{ id: "k1", staff_id: "s3", title: "Audit the insurance annexure", brief: "Every limit.", status: "open", result: null }],
    });
    openApp("/tenders/t1/office");

    const room = await screen.findByRole("region", { name: "Conversation" });
    await userEvent.click(await within(room).findByRole("button", { name: "About Nora Al-Otaibi" }));
    const profile = await screen.findByRole("complementary", { name: "Nora Al-Otaibi" });
    expect(within(profile).getByText("Released from this tender")).toBeInTheDocument();
    expect(within(profile).getByText("not done")).toBeInTheDocument();
  });

  it("shows what an answer rests on, each opening its source", async () => {
    const answer: Message = {
      ...said(2, "s1", "s1", "Built up from a fixing gang, rebar with 5% wastage, tie wire and a bending machine."),
      sources: [
        { label: "The rate for BOQ item 4.3", boq_item_id: "b43" },
        { label: "Bill.xlsx, page 1", document_id: "d1", page: 1 },
        { label: "Where the tender stands" },
        { label: "Green Concrete Readymix, read 27 Sep 2026", url: "https://readymix.example/c35" },
      ],
    };
    fakeService({
      tenders: [tender],
      settings: ready,
      staff: [rania, omar],
      messages: [said(1, "engineer", "s1", "How did you price the slab reinforcement?"), answer],
    });
    openApp("/tenders/t1/office?with=s1");

    const rate = await screen.findByRole("link", { name: "The rate for BOQ item 4.3" });
    expect(rate).toHaveAttribute("href", "/tenders/t1/estimate?item=b43");
    expect(screen.getByRole("link", { name: "Bill.xlsx, page 1" })).toHaveAttribute(
      "href",
      "/tenders/t1/documents?doc=d1&page=1",
    );
    expect(screen.getByText("Where the tender stands")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Green Concrete Readymix, read 27 Sep 2026" })).toHaveAttribute(
      "href",
      "https://readymix.example/c35",
    );
  });

  it("messages a person directly and can stop the office", async () => {
    const service = fakeService({ tenders: [tender], settings: ready, staff: [rania, omar], officeState: "working" });
    openApp("/tenders/t1/office");

    await userEvent.click(await screen.findByRole("button", { name: "Omar Haddad" }));
    await userEvent.type(screen.getByLabelText("Message"), "Use the BOQ units{Enter}");
    await waitFor(() => expect(service.state.messages.at(-1)).toMatchObject({ channel: "s2", text: "Use the BOQ units" }));

    await userEvent.click(screen.getByRole("button", { name: "Stop the office" }));
    await waitFor(() => expect(service.state.officeState).toBe("paused"));
  });
});
