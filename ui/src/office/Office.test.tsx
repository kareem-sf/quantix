import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import type { BoqItem } from "../estimate/queries";
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
const ready = {
  office_mode: "engineer" as const,
  office_ai: { connection_id: "c1", model: "m" },
  tender_allowance: null,
  notifications: "all" as const,
};

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
    openApp("/tenders/t1");

    expect(await screen.findByRole("heading", { name: "1 decision needs you" })).toBeInTheDocument();
    expect(await screen.findByText("Omar found the tender security clause.")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: /Tender security wording/ })); // opens Rania's chat

    const card = await screen.findByRole("group", { name: "Tender security wording" });
    await userEvent.click(within(card).getByRole("button", { name: "Yes, 1%" }));
    await userEvent.click(within(card).getByRole("button", { name: "Send answer" }));
    await waitFor(() => expect(service.state.decisions[0]).toMatchObject({ status: "answered", answer: "Yes, 1%" }));
  });

  it("keeps what waits for the engineer's approval at the end of the Manager's chat, counted on the rail", async () => {
    const line: BoqItem = {
      id: "i31",
      section: null,
      item: "3.1",
      description: "Excavation",
      unit: "m3",
      quantity: "1240",
      status: "reviewed",
      proposed_by: "s2",
      reason: null,
      reviewed_by: "s1",
      review_note: "Matches Bill.xlsx.",
      source: { document_id: "d1", document_name: "Bill.xlsx", page: 1, quote: "A2=3.1" },
    };
    fakeService({
      tenders: [tender],
      settings: ready,
      staff: [rania, omar],
      items: [line],
      messages: [said(1, "s1", "s1", "BOQ line 3.1 waits for your approval on the Estimate screen.")],
    });
    openApp("/tenders/t1/office?with=s1");

    const waiting = await screen.findByRole("group", { name: "Waiting for your approval" });
    expect(within(waiting).getByRole("link", { name: "1 BOQ item to approve" })).toHaveAttribute(
      "href",
      "/tenders/t1/estimate?show=waiting",
    );
    expect(within(screen.getByRole("navigation", { name: "Quantix" })).getByLabelText("1 decision needs you")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("tab", { name: "Team room" }));
    await screen.findByText("What the team says to each other");
    expect(screen.queryByRole("group", { name: "Waiting for your approval" })).not.toBeInTheDocument();
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
    const router = openApp("/tenders/t1/decisions/q2"); // an old link opens the question in its asker's chat

    await waitFor(() => expect(router.state.location.pathname).toBe("/tenders/t1"));
    const card = await screen.findByRole("group", { name: "The rate for BOQ item 3.1" });
    expect(within(card).getByRole("link", { name: "BOQ line 3.1" })).toHaveAttribute("href", "/tenders/t1/estimate?item=i31");
    expect(within(card).getByRole("link", { name: "Soils.pdf, page 4: Rock at 1.2 m" })).toHaveAttribute(
      "href",
      "/tenders/t1/documents?doc=d7&page=4",
    );
    expect(await within(card).findByText("18.00 is -40% from the firm's own rates.")).toBeInTheDocument();
    // the Manager's recommended correction comes first, and says so
    const [first, second] = within(card).getAllByRole("button", { pressed: false });
    expect(first).toHaveTextContent("RecommendedPrice excavation in rock with a breaker");
    expect(second).not.toHaveTextContent("Recommended");

    await userEvent.click(within(card).getByRole("button", { name: "Answer in your own words" }));
    await userEvent.type(within(card).getByLabelText("Your answer"), "Use the rock rate from the depot job.");
    await userEvent.click(within(card).getByRole("button", { name: "Send answer" }));
    await waitFor(() =>
      expect(service.state.decisions[0]).toMatchObject({ answer: "Use the rock rate from the depot job." }),
    );
  });

  it("sends the engineer's request to the Manager from the team panel", async () => {
    const service = fakeService({ tenders: [tender], settings: ready, staff: [rania] });
    openApp("/tenders/t1");

    await userEvent.click(await screen.findByRole("button", { name: "Open your chat with Rania" }));
    const panel = screen.getByRole("complementary", { name: "Team" });
    await userEvent.type(within(panel).getByLabelText("Message"), "Review the package{Enter}");
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
    expect(within(room).getByText("Raised a concern")).toBeInTheDocument();
    expect(screen.getByTitle("Rania: Reviewing Omar's result")).toHaveTextContent("Rania: Reviewing Omar's result");

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

  it("folds each turn to a line that opens on the thinking and steps, with the tools on request", async () => {
    const turn = {
      id: 7,
      staff_id: "s1",
      started_at: "2026-09-23T10:43:00Z",
      ended_at: "2026-09-23T10:43:32Z",
      running: false,
      ended: "done",
      note: null,
      doing: "Opening the markups",
      steps: 5,
      log: [
        { kind: "brief" as const, text: "Tender: Synthetic school.\n\nNew for you:\n- Engineer: Is the markup a percentage?" },
        { kind: "thinking" as const, text: "The engineer wants to know how markups are entered." },
        { kind: "note" as const, text: "I'll open the markups record first." },
        {
          kind: "tool" as const,
          tool: "open_record",
          args: '{"kind":"markups"}',
          doing: "Opening the markups",
          result: "Markups: none proposed yet.",
          sent_back: null,
        },
        { kind: "tool" as const, tool: "read_page", args: '{"page":1}', doing: null, result: null, sent_back: "No document has that id." },
      ],
    };
    fakeService({
      tenders: [tender],
      settings: ready,
      staff: [rania, omar],
      messages: [said(1, "engineer", "s1", "Is the markup a percentage?")],
      turns: [turn],
    });
    openApp("/tenders/t1/office?with=s1");

    const room = await screen.findByRole("region", { name: "Conversation" });
    const line = await within(room).findByRole("button", { name: "Worked for 32 s · Opening the markups" });
    expect(line).toHaveAttribute("aria-expanded", "false");
    await userEvent.click(line);

    expect(await within(room).findByText("The engineer wants to know how markups are entered.")).toBeInTheDocument();
    expect(within(room).getByText("I'll open the markups record first.")).toBeInTheDocument();
    expect(within(room).getByText("Opening the markups")).toBeInTheDocument();
    expect(within(room).getByText("Read page")).toBeInTheDocument();
    expect(within(room).getByText("Quantix sent this back: No document has that id.")).toBeInTheDocument();
    expect(within(room).queryByText("open_record")).not.toBeInTheDocument();
    expect(within(room).queryByText(/What Rania was told/)).not.toBeInTheDocument();

    await userEvent.click(within(room).getByRole("switch", { name: "Technical details" }));
    expect(within(room).getByText("open_record")).toBeInTheDocument();
    expect(within(room).getByText("Markups: none proposed yet.")).toBeInTheDocument();
    expect(within(room).getByText(/"kind": "markups"/)).toBeInTheDocument();
    expect(within(room).getByText("What Rania was told at the start")).toBeInTheDocument();
  });

  it("puts a turn and its reply under the person who did them, and takes an answer to their question", async () => {
    // on the real tender Salem's turns showed under the engineer's message, as if the engineer had done them, and
    // his question to the engineer showed nowhere in the chat
    const turn = {
      id: 9,
      staff_id: "s1",
      started_at: "2026-09-23T10:43:00Z",
      ended_at: "2026-09-23T10:43:12Z",
      running: false,
      ended: "done",
      note: null,
      doing: "Briefing Omar",
      steps: 0,
      log: [],
    };
    const asked: Decision = { ...question, status: "waiting", answer: null, created_at: "2026-09-23T10:43:10Z" };
    const service = fakeService({
      tenders: [tender],
      settings: ready,
      staff: [rania, omar],
      messages: [
        { ...said(1, "engineer", "s1", "Get the markups redone."), created_at: "2026-09-23T10:42:00Z" },
        {
          ...said(2, "s1", "s1", "Omar is redoing them.\n\nSources: markups e8d6e257"),
          created_at: "2026-09-23T10:43:11Z",
          sources: [{ label: "The markups" }],
        },
      ],
      turns: [turn],
      decisions: [asked],
    });
    openApp("/tenders/t1/office?with=s1");

    const room = await screen.findByRole("region", { name: "Conversation" });
    const line = await within(room).findByRole("button", { name: "Worked for 12 s · Briefing Omar" });
    const heading = within(room).getByRole("button", { name: "About Rania Farouk" }).parentElement!;
    expect(heading).toContainElement(line);
    expect(heading).toContainElement(within(room).getByText("Omar is redoing them."));
    expect(within(room).queryByText(/Sources: markups/)).not.toBeInTheDocument(); // the link under it says it

    const card = within(room).getByRole("group", { name: "Tender security wording" });
    expect(heading).toContainElement(card);
    // asked before his reply, but still waiting: it stays at the end, where the engineer is
    const reply = within(room).getByText("Omar is redoing them.");
    expect(reply.compareDocumentPosition(card) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(within(card).getByText("Needs your decision")).toBeInTheDocument();
    await userEvent.click(within(card).getByRole("button", { name: "Yes, 1%" }));
    await userEvent.click(within(card).getByRole("button", { name: "Send answer" }));
    await waitFor(() => expect(service.state.decisions[0]).toMatchObject({ status: "answered", answer: "Yes, 1%" }));
  });

  it("says in the chat why a turn stopped, and above the message box why the office is stopped", async () => {
    const failed = {
      id: 8,
      staff_id: "s1",
      started_at: "2026-09-23T10:43:00Z",
      ended_at: "2026-09-23T10:49:40Z",
      running: false,
      ended: "ai_failed",
      note: "Couldn't reach the service. Check the internet connection and the address.",
      doing: null,
      steps: 0,
      log: [],
    };
    fakeService({
      tenders: [tender],
      settings: ready,
      staff: [{ ...rania, now: null }, omar],
      messages: [said(1, "engineer", "s1", "Is the markup a percentage?")],
      turns: [failed],
      officeState: "paused",
      notice: "The office stopped: Couldn't reach the service. Send a message to try again.",
    });
    openApp("/tenders/t1/office?with=s1");

    const room = await screen.findByRole("region", { name: "Conversation" });
    expect(
      await within(room).findByText("Stopped after 6 min 40 s: Couldn't reach the service. Check the internet connection and the address."),
    ).toBeInTheDocument();
    expect(within(room).getByText("The office stopped: Couldn't reach the service. Send a message to try again.")).toBeInTheDocument();
  });

  it("messages a person directly and can stop the office", async () => {
    const service = fakeService({ tenders: [tender], settings: ready, staff: [rania, omar], officeState: "working" });
    openApp("/tenders/t1/office");

    await userEvent.click(await screen.findByRole("tab", { name: "Omar Haddad" }));
    await userEvent.type(screen.getByLabelText("Message"), "Use the BOQ units{Enter}");
    await waitFor(() => expect(service.state.messages.at(-1)).toMatchObject({ channel: "s2", text: "Use the BOQ units" }));

    await userEvent.click(screen.getByRole("button", { name: "Stop the office" }));
    await waitFor(() => expect(service.state.officeState).toBe("paused"));
  });

  it("opens a profile only when asked, and closes it or goes to that person's chat", async () => {
    fakeService({
      tenders: [tender],
      settings: ready,
      staff: [rania, omar],
      messages: [said(1, "s2", "team", "The bond wording is unusual.")],
    });
    openApp("/tenders/t1/office?with=s1");

    await screen.findByRole("heading", { name: "Rania Farouk" });
    expect(screen.queryByRole("complementary", { name: "Rania Farouk" })).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "About Rania" }));
    const card = await screen.findByRole("complementary", { name: "Rania Farouk" });
    expect(within(card).queryByRole("button", { name: "Message Rania" })).not.toBeInTheDocument(); // already her chat
    await userEvent.click(within(card).getByRole("button", { name: "Close" }));
    expect(screen.queryByRole("complementary", { name: "Rania Farouk" })).not.toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "About Rania" }));
    await screen.findByRole("complementary", { name: "Rania Farouk" });
    await userEvent.keyboard("{Escape}");
    expect(screen.queryByRole("complementary", { name: "Rania Farouk" })).not.toBeInTheDocument();

    await userEvent.click(screen.getByRole("tab", { name: "Team room" }));
    await userEvent.click(await screen.findByRole("button", { name: "About Omar Haddad" }));
    const omarCard = await screen.findByRole("complementary", { name: "Omar Haddad" });
    await userEvent.click(within(omarCard).getByRole("button", { name: "Message Omar" }));
    expect(await screen.findByRole("heading", { name: "Omar Haddad" })).toBeInTheDocument();
    expect(screen.queryByRole("complementary", { name: "Omar Haddad" })).not.toBeInTheDocument();
  });

  it("says how the team starts before anyone has joined", async () => {
    fakeService({ tenders: [tender], settings: ready });
    openApp("/tenders/t1/office");

    const panel = await screen.findByRole("complementary", { name: "Team" });
    expect(within(panel).getByText(/The Tender Manager joins when you first write to the office/)).toBeInTheDocument();
    expect(await within(panel).findByText("No messages yet.")).toBeInTheDocument();
    expect(within(panel).getByRole("tab", { name: "Team room" })).toHaveAttribute("aria-selected", "true");
    expect(within(panel).getByLabelText("Message")).toHaveAttribute("placeholder", "Message the team room");
  });

  it("folds three or more turns in a row to one line, keeping the latest in view", async () => {
    const worked = (id: number, minute: number, doing: string) => ({
      id, staff_id: "s1", started_at: `2026-09-23T10:4${minute}:00Z`, ended_at: `2026-09-23T10:4${minute}:30Z`, running: false,
      ended: "done", note: null, doing, steps: 0, log: [],
    });
    fakeService({
      tenders: [tender], settings: ready, staff: [rania, omar],
      turns: [worked(1, 3, "Reading the ITT"), worked(2, 4, "Briefing Omar"), worked(3, 5, "Checking the BOQ")],
    });
    openApp("/tenders/t1/office?with=s1");

    const room = await screen.findByRole("region", { name: "Conversation" });
    const earlier = await within(room).findByRole("button", { name: "2 earlier turns at work" });
    expect(within(room).getByRole("button", { name: "Worked for 30 s · Checking the BOQ" })).toBeInTheDocument();
    expect(within(room).queryByRole("button", { name: /Reading the ITT/ })).not.toBeInTheDocument();

    await userEvent.click(earlier);
    expect(within(room).getAllByRole("button", { name: /^Worked for 30 s/ }).map((b) => b.textContent)).toEqual([
      "Worked for 30 s · Reading the ITT",
      "Worked for 30 s · Briefing Omar",
      "Worked for 30 s · Checking the BOQ",
    ]);
    await userEvent.click(within(room).getByRole("button", { name: "Fold" }));
    expect(within(room).getByRole("button", { name: "2 earlier turns at work" })).toBeInTheDocument();
  });

  it("shows the office's notes on their own, apart from what people said", async () => {
    fakeService({
      tenders: [tender], settings: ready, staff: [rania, omar],
      messages: [
        said(1, "s1", "team", "Omar, measure the slab on A-201."),
        said(2, "office", "team", "The office paused: the AI allowance for this tender is used.", "note"),
      ],
    });
    openApp("/tenders/t1/office");

    const room = await screen.findByRole("region", { name: "Conversation" });
    const note = await within(room).findByText("The office paused: the AI allowance for this tender is used.");
    const heading = within(room).getByRole("button", { name: "About Rania Farouk" }).parentElement!;
    expect(heading).not.toContainElement(note);
    expect(heading).toHaveTextContent("Omar, measure the slab on A-201.");
  });

  it("shows a question the engineer already answered, with the answer", async () => {
    fakeService({
      tenders: [tender], settings: ready, staff: [rania, omar],
      decisions: [{ ...question, status: "answered", answer: "Yes, 1%" }],
    });
    openApp("/tenders/t1/office?with=s1");

    const card = await screen.findByRole("group", { name: "Tender security wording" });
    expect(within(card).getByText("You decided")).toBeInTheDocument();
    expect(within(card).getByText("Yes, 1%")).toBeInTheDocument();
    expect(within(card).queryByRole("button", { name: "Send answer" })).not.toBeInTheDocument();
  });

  it("sends no answer until one is chosen or written", async () => {
    fakeService({ tenders: [tender], settings: ready, staff: [rania, omar], decisions: [{ ...question, status: "waiting", answer: null }] });
    openApp("/tenders/t1/office?with=s1");

    const card = await screen.findByRole("group", { name: "Tender security wording" });
    expect(within(card).getByRole("button", { name: "Send answer" })).toBeDisabled();
    await userEvent.click(within(card).getByRole("button", { name: "Answer in your own words" }));
    await userEvent.type(within(card).getByLabelText("Your answer"), "   ");
    expect(within(card).getByRole("button", { name: "Send answer" })).toBeDisabled();
    await userEvent.click(within(card).getByRole("button", { name: "Ask the client first" }));
    expect(within(card).queryByLabelText("Your answer")).not.toBeInTheDocument();
    expect(within(card).getByRole("button", { name: "Send answer" })).toBeEnabled();
  });

  it("says why an answer or a message couldn't be sent", async () => {
    fakeService({
      tenders: [tender], settings: ready, staff: [rania, omar], decisions: [{ ...question, status: "waiting", answer: null }],
      fail: { "/decisions/q1/answer": "The question was withdrawn.", "/tenders/t1/messages": "The office has used its AI allowance." },
    });
    openApp("/tenders/t1/office?with=s1");

    const card = await screen.findByRole("group", { name: "Tender security wording" });
    await userEvent.click(within(card).getByRole("button", { name: "Yes, 1%" }));
    await userEvent.click(within(card).getByRole("button", { name: "Send answer" }));
    expect(await within(card).findByText("The question was withdrawn.")).toBeInTheDocument();

    await userEvent.type(screen.getByLabelText("Message"), "Carry on{Enter}");
    expect(await screen.findByText("The office has used its AI allowance.")).toBeInTheDocument();
    expect(screen.getByLabelText("Message")).toHaveValue("Carry on"); // kept, to send again
  });

  it("shows only what a profile says in words, and where each task stands", async () => {
    const profiled: Staff = { ...omar, profile: { experience_years: 11, background: "Character_Overview", working_style: "Measures twice." } };
    fakeService({
      tenders: [tender], settings: ready, staff: [rania, profiled],
      tasks: [
        { id: "k1", staff_id: "s2", title: "Measure the slab", brief: "A-201.", status: "done", result: null },
        { id: "k2", staff_id: "s2", title: "Measure the walls", brief: "A-202.", status: "open", result: null },
      ],
    });
    openApp("/tenders/t1/office?with=s2");

    await userEvent.click(await screen.findByRole("button", { name: "About Omar" }));
    const card = await screen.findByRole("complementary", { name: "Omar Haddad" });
    expect(within(card).getByText("Quantity Surveyor · 11 years")).toBeInTheDocument();
    expect(within(card).getByText("Measures twice.")).toBeInTheDocument();
    expect(within(card).queryByText("Background")).not.toBeInTheDocument(); // a placeholder, not words
    expect((await within(card).findByText("Measure the slab")).parentElement).toHaveTextContent("Measure the slabdone");
    expect(within(card).getByText("Measure the walls").parentElement).toHaveTextContent("Measure the wallsworking");
  });
});
