import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { ApprovalCard } from "./approval-card";
import { ChangeTable } from "./change-table";
import { FactList, type Fact } from "./fact-list";
import { formatElapsed, LiveStatus } from "./live-status";
import { PassageCards } from "./passage-cards";
import { QuestionCard } from "./question-card";
import { SearchList } from "./search-list";
import { StatusPill, StatusTable } from "./status-table";
import { TaskRows } from "./task-rows";
import { ThinkingTrace, WorkBlock } from "./work-block";

it("formats elapsed time for engineers, not stopwatches", () => {
  expect(formatElapsed(900)).toBe("0s");
  expect(formatElapsed(42_000)).toBe("42s");
  expect(formatElapsed(132_000)).toBe("2m 12s");
  expect(formatElapsed(3_780_000)).toBe("1h 03m");
});

it("shows the live label with time measured from the real start", () => {
  const since = new Date(Date.now() - 75_000).toISOString();
  render(
    <LiveStatus label="Waiting for the Tender Manager's reply" since={since} />,
  );
  const status = screen.getByRole("status");
  expect(status).toHaveTextContent("Waiting for the Tender Manager's reply");
  expect(status).toHaveTextContent("1m 15s");
});

it("opens a work block only when asked and never advances on its own", async () => {
  const user = userEvent.setup();
  render(
    <WorkBlock
      heading="I'll check the visit schedule for the date."
      durationMs={42_000}
    >
      <p>Read the schedule</p>
    </WorkBlock>,
  );
  const toggle = screen.getByRole("button", { name: /visit schedule/ });
  expect(toggle).toHaveAttribute("aria-expanded", "false");
  expect(toggle).toHaveTextContent("42s");
  await user.click(toggle);
  expect(toggle).toHaveAttribute("aria-expanded", "true");
});

it("keeps thinking folded while it streams, then shows its real duration", async () => {
  const user = userEvent.setup();
  const { rerender } = render(
    <ThinkingTrace text="The schedule says 21 Sep" streaming />,
  );
  expect(screen.getByRole("button", { name: /Thinking/ })).toHaveAttribute(
    "aria-expanded",
    "false",
  );
  rerender(
    <ThinkingTrace text="The schedule says 21 Sep." durationMs={12_400} />,
  );
  const folded = screen.getByRole("button", { name: /Thought for 12 seconds/ });
  expect(folded).toHaveAttribute("aria-expanded", "false");
  await user.click(folded);
  expect(folded).toHaveAttribute("aria-expanded", "true");
});

it("lists real facts, reveals what was found and hides search terms behind Details", async () => {
  const user = userEvent.setup();
  const facts: Fact[] = [
    {
      id: "1",
      kind: "read",
      line: "Read",
      subject: "Systems.pdf",
      result: "page 1",
      state: "done",
      durationMs: 300,
      found: ["Fire alarm: Edwards EST4"],
    },
    {
      id: "2",
      kind: "search",
      line: "Searched the documents",
      result: "nothing found",
      state: "done",
      details: ['"site visit postponed"'],
    },
    { id: "3", kind: "check", line: "Checking the estimate", state: "running" },
  ];
  render(
    <FactList
      facts={facts}
      renderAction={(fact) =>
        fact.kind === "read" ? <button type="button">Open page 1</button> : null
      }
    />,
  );
  const read = screen.getByRole("button", { name: /Systems\.pdf/ });
  expect(read).toHaveTextContent("0.3s");
  await user.click(read);
  expect(screen.getByText("Fire alarm: Edwards EST4")).toBeVisible();
  expect(
    screen.getByRole("button", { name: "Open page 1" }),
  ).toBeInTheDocument();
  await user.click(
    screen.getByRole("button", { name: /Searched the documents/ }),
  );
  expect(screen.queryByText('"site visit postponed"')).not.toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "Details" }));
  expect(screen.getByText('"site visit postponed"')).toBeInTheDocument();
  expect(
    screen.getByRole("button", { name: /Checking the estimate/ }),
  ).toBeDisabled();
});

it("opens a passage at its source", async () => {
  const user = userEvent.setup();
  const onOpen = vi.fn();
  const passage = {
    id: "p1",
    title: "Site visit",
    location: "Page 1",
    text: "Visit on 21 Sep at 10:00.",
    fileName: "8-موعد الزيارة الميدانية 1.pdf",
    fileType: "pdf",
  };
  render(
    <PassageCards
      passages={[passage]}
      heading="What it found"
      onOpen={onOpen}
    />,
  );
  await user.click(screen.getByRole("button", { name: /موعد الزيارة/ }));
  expect(onOpen).toHaveBeenCalledWith(passage);
});

it("shows plan items with their real status and owner", async () => {
  const user = userEvent.setup();
  render(
    <TaskRows
      aria-label="Plan"
      rows={[
        {
          id: "a",
          label: "Map the package",
          note: "20 documents in 6 groups",
          meta: "Tender Manager",
          status: "done",
        },
        {
          id: "b",
          label: "Confirm site-visit terms",
          meta: "Layla Haddad",
          status: "running",
          details: [{ label: "Read the visit schedule", meta: "2 pages" }],
        },
        { id: "c", label: "Take off BOQ quantities", status: "todo" },
      ]}
    />,
  );
  const items = screen.getAllByRole("listitem");
  expect(within(items[0]).getByText("Done")).toBeInTheDocument();
  expect(within(items[1]).getByText("In progress")).toBeInTheDocument();
  expect(within(items[2]).getByText("Not started")).toBeInTheDocument();
  const running = within(items[1]).getByRole("button", {
    name: /Confirm site-visit terms/,
  });
  await user.click(running);
  expect(running).toHaveAttribute("aria-expanded", "true");
});

it("sends answers only when every question is answered and Send is pressed", async () => {
  const user = userEvent.setup();
  const onSubmit = vi.fn();
  render(
    <QuestionCard
      onSubmit={onSubmit}
      questions={[
        {
          id: "q1",
          text: "Use BOQ units or drawing units?",
          askedBy: "Khalid Bin Salem",
          type: "single",
          options: ["BOQ units", "Drawing units"],
        },
        {
          id: "q2",
          text: "Which sections to price first?",
          type: "multiple",
          options: ["Civil", "MEP"],
        },
      ]}
    />,
  );
  expect(screen.getByText("Khalid Bin Salem asks")).toBeInTheDocument();
  await user.click(screen.getByRole("radio", { name: "BOQ units" }));
  expect(onSubmit).not.toHaveBeenCalled();
  await user.click(screen.getByRole("button", { name: "Next" }));
  const send = screen.getByRole("button", { name: "Send answers" });
  expect(send).toBeDisabled();
  await user.click(screen.getByRole("checkbox", { name: "MEP" }));
  await user.click(send);
  expect(onSubmit).toHaveBeenCalledWith({
    q1: { choices: ["BOQ units"] },
    q2: { choices: ["MEP"] },
  });
});

it("approves the option the engineer chose, with no confidence claims", async () => {
  const user = userEvent.setup();
  const onApprove = vi.fn();
  render(
    <ApprovalCard
      title="Approve the plan?"
      onApprove={onApprove}
      secondary={{ label: "Ask for changes", onClick: vi.fn() }}
      options={[
        {
          id: "full",
          summary: "Full review",
          body: "Review all 20 documents.",
          actionLabel: "Approve plan",
        },
        {
          id: "short",
          summary: "Site visit only",
          body: "Only the site-visit terms.",
          actionLabel: "Approve short plan",
        },
      ]}
    />,
  );
  expect(screen.queryByText(/confidence/i)).not.toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "Alternatives" }));
  await user.click(screen.getByRole("button", { name: "Site visit only" }));
  await user.click(screen.getByRole("button", { name: "Approve short plan" }));
  expect(onApprove).toHaveBeenCalledWith(
    expect.objectContaining({ id: "short" }),
  );
});

it("applies only the change rows the engineer keeps selected", async () => {
  const user = userEvent.setup();
  const onApply = vi.fn();
  render(
    <ChangeTable
      title="Proposed quantities"
      columns={["Item", "Quantity"]}
      onApply={onApply}
      rows={[
        { id: "keep", kind: "same", cells: ["3.1 Excavation", "1,240 m³"] },
        { id: "add", kind: "add", cells: ["3.2 Blinding", "86 m³"] },
        { id: "remove", kind: "remove", cells: ["3.9 Dewatering", "1 item"] },
      ]}
    />,
  );
  expect(screen.getByText("1 addition · 1 removal")).toBeInTheDocument();
  await user.click(screen.getByText("3.9 Dewatering"));
  expect(screen.getByText("1 addition")).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "Apply 1 change" }));
  expect(onApply).toHaveBeenCalledWith(["add"]);
});

it("filters table rows by status with real counts", async () => {
  const user = userEvent.setup();
  type Job = { id: string; request: string; status: "done" | "stopped" };
  const statuses = [
    { value: "done" as const, label: "Done", tone: "text-(--success)" },
    { value: "stopped" as const, label: "Stopped", tone: "text-destructive" },
  ];
  render(
    <StatusTable<Job, Job["status"]>
      aria-label="Jobs"
      rows={[
        { id: "1", request: "review the package", status: "stopped" },
        { id: "2", request: "Who are you?", status: "done" },
      ]}
      rowKey={(row) => row.id}
      statusOf={(row) => row.status}
      statuses={statuses}
      columns={[
        { id: "request", header: "Request", cell: (row) => row.request },
        {
          id: "status",
          header: "Status",
          cell: (row) => <StatusPill label={row.status} tone="" />,
        },
      ]}
    />,
  );
  await user.click(screen.getByRole("button", { name: /Stopped/ }));
  const table = screen.getByRole("region", { name: "Jobs" });
  expect(within(table).getByText("review the package")).toBeInTheDocument();
  expect(within(table).queryByText("Who are you?")).not.toBeInTheDocument();
});

it("filters search results and shows an empty state", async () => {
  const user = userEvent.setup();
  const onPick = vi.fn();
  render(
    <SearchList
      label="Search documents"
      placeholder="Search documents…"
      onPick={onPick}
      items={[
        { id: "1", label: "FIRE STATION-ELEC.pdf", hint: "Drawings" },
        { id: "2", label: "1-جدول الكميات.pdf", hint: "Bill of quantities" },
      ]}
    />,
  );
  const input = screen.getByRole("textbox", { name: "Search documents" });
  await user.type(input, "quantities");
  await user.click(screen.getByRole("button", { name: /جدول الكميات/ }));
  expect(onPick).toHaveBeenCalledWith(expect.objectContaining({ id: "2" }));
  await user.clear(input);
  await user.type(input, "zzz");
  expect(screen.getByText("No results")).toBeInTheDocument();
});
