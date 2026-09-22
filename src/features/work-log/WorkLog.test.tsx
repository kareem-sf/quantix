import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ApiContext, createApi, type Schema } from "../../api";
import type { RunActivity } from "../activity/types";
import {
  blockHeading,
  buildWorkLog,
  repairGluedWords,
  stopReason,
  workSummary,
} from "./model";
import { WorkLog } from "./WorkLog";

let nextId = 1;
function event(extra: Partial<RunActivity>): RunActivity {
  const id = nextId++;
  return {
    event_id: id,
    run_id: "run",
    operation_id: `op-${id}`,
    parent_operation_id: null,
    actor_id: "manager",
    actor_label: "Tender Manager",
    assignment_id: null,
    category: "tool",
    phase: "completed",
    message: "",
    created_at: `2026-09-15T10:00:${String(id).padStart(2, "0")}Z`,
    tool: null,
    provider: "custom",
    model: "deepseek",
    provider_call_id: null,
    preview: "",
    preview_truncated: false,
    detail_available: true,
    capture_status: "complete",
    elapsed_ms: null,
    source_ids: [],
    artifact_refs: [],
    ...extra,
  } as RunActivity;
}

function reviewItems() {
  nextId = 1;
  return [
    event({
      category: "model_request",
      phase: "started",
      operation_id: "req-1",
    }),
    event({
      category: "reasoning_summary",
      phase: "delta",
      parent_operation_id: "req-1",
      preview: "The date is probably ",
    }),
    event({
      category: "reasoning_summary",
      phase: "delta",
      parent_operation_id: "req-1",
      preview: "in the visit schedule.",
    }),
    event({
      category: "note",
      phase: "completed",
      parent_operation_id: "req-1",
      preview: "I'll read the visit schedule for the date.",
    }),
    event({
      category: "model_request",
      phase: "completed",
      operation_id: "req-1",
    }),
    event({
      operation_id: "tool-1",
      parent_operation_id: "req-1",
      tool: "read_whole_document",
      phase: "prepared",
      fact: {
        kind: "read",
        line: "Reading",
        subject: "8-موعد الزيارة.pdf",
        state: "running",
      },
    }),
    event({
      operation_id: "tool-1",
      parent_operation_id: "req-1",
      tool: "read_whole_document",
      phase: "completed",
      elapsed_ms: 300,
      fact: {
        kind: "read",
        line: "Read",
        subject: "8-موعد الزيارة.pdf",
        result: "pages 1–2",
        found: ["Visit on 21 Sep at 10:00, KSAU-HS site office."],
        open: { artifact_id: "doc-8", page: 1 },
        state: "done",
      },
    }),
    event({
      operation_id: "tool-2",
      parent_operation_id: "req-1",
      tool: "inspect_submission_requirements",
      phase: "blocked",
      fact: {
        kind: "check",
        line: "Couldn't finish: checking submission requirements",
        state: "failed",
        recoverable: true,
      },
    }),
    event({
      operation_id: "tool-3",
      parent_operation_id: "req-1",
      tool: "inspect_submission_requirements",
      phase: "completed",
      fact: {
        kind: "check",
        line: "Checked submission requirements",
        result: "none saved yet",
        state: "done",
      },
    }),
    event({
      operation_id: "tool-4",
      parent_operation_id: "req-1",
      tool: "assign_work",
      phase: "completed",
      fact: {
        kind: "assign",
        line: "Handed work to",
        subject: "Layla Haddad",
        result: "List the specified systems",
        assignment_id: "as-1",
        state: "done",
      },
    }),
    event({
      category: "model_request",
      phase: "started",
      operation_id: "req-staff",
      assignment_id: "as-1",
      actor_label: "Staff member",
    }),
    event({
      category: "note",
      phase: "completed",
      parent_operation_id: "req-staff",
      assignment_id: "as-1",
      preview: "I'll read Systems.pdf first.",
    }),
    event({
      operation_id: "tool-5",
      parent_operation_id: "req-staff",
      assignment_id: "as-1",
      tool: "search_sources",
      phase: "completed",
      fact: {
        kind: "search",
        line: "Searched the documents",
        result: "nothing found",
        details: ["Search terms: “fire alarm”"],
        state: "done",
      },
    }),
  ];
}

it("groups activity into turns with notes, thinking, facts and nested staff work", () => {
  const log = buildWorkLog(reviewItems());
  expect(log.blocks).toHaveLength(1);
  const [block] = log.blocks;
  expect(block.note).toBe("I'll read the visit schedule for the date.");
  expect(block.thinking).toBe("The date is probably in the visit schedule.");
  expect(block.thinkingDone).toBe(true);
  // The rejected call the AI corrected is not shown as a failure.
  expect(block.facts.map((fact) => fact.line)).toEqual([
    "Read",
    "Checked submission requirements",
    "Handed work to",
  ]);
  expect(log.staff.get("as-1")?.blocks[0].facts[0].result).toBe(
    "nothing found",
  );
  expect(workSummary(log)).toEqual([
    "read 1 document",
    "searched once",
    "asked Layla Haddad for list the specified systems",
  ]);
});

it("builds a heading from real actions when the AI wrote no note", () => {
  const log = buildWorkLog(reviewItems());
  expect(blockHeading({ ...log.blocks[0], note: "" })).toBe(
    "Read 1 document, checked submission requirements, handed work to Layla Haddad",
  );
});

it("states stop reasons in engineering terms", () => {
  expect(
    stopReason(
      "failed",
      "AI allowance reached. This Tender has used about USD 1.92",
    ).reason,
  ).toBe("the spending limit for this tender was reached");
  expect(
    stopReason(
      "failed",
      "The provider stopped its reply with an error part-way through.",
    ).reason,
  ).toBe("the AI service dropped its reply");
  expect(stopReason("cancelled").reason).toBe("you stopped it");
  expect(
    stopReason(
      "failed",
      "This task reached its work limit before finishing. Review the saved progress and spending allowance before continuing.",
    ).reason,
  ).toBe("it used all the AI steps allowed for one job");
  expect(
    stopReason(
      "failed",
      "This source row reference already has a different proposal. Review the saved row.",
    ).reason,
  ).toMatch(
    /^a BOQ row clashed with one already saved, so nothing from this job was saved/,
  );
  expect(
    stopReason(
      "failed",
      "The provider service is unavailable. Retry after checking its status and limits.",
    ).reason,
  ).toBe("the AI service was unavailable. Try again shortly");
  expect(
    stopReason(
      "failed",
      "The provider rejected the request. Check the credentials, endpoint, model and permissions.",
    ).reason,
  ).toBe(
    "the AI service refused the request. Check the AI connection in Settings",
  );
  expect(
    stopReason(
      "failed",
      "A Tender tool call was refused. 'This item could not be found in the selected Tender.'",
    ).reason,
  ).toBe("it asked for something that is not in this tender");
  expect(
    stopReason(
      "failed",
      "The AI could not correct its proposal. Factual findings require Tender source evidence.",
    ).reason,
  ).toBe(
    "it could not back its answer with the tender documents, so nothing was saved",
  );
  expect(stopReason("failed", "Something odd happened.").reason).toBe(
    "Something odd happened",
  );
});

function renderLog(
  run: Schema<"Run">,
  items: RunActivity[],
  onSource = vi.fn(),
) {
  const requests: string[] = [];
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async (url, init) => {
      const path = new URL(String(url)).pathname;
      requests.push(`${init?.method ?? "GET"} ${path}`);
      const body = path.endsWith("/activity")
        ? {
            items,
            cursor: "end",
            before_cursor: null,
            has_more: false,
            has_earlier: false,
            reset_required: false,
            run_status: run.status,
            run_detail: "",
            run_updated_at: run.updated_at,
            history_key: "history",
          }
        : path.endsWith("/work-brief")
          ? {
              brief: {
                steps: [
                  { title: "Check the site-visit terms", state: "in_progress" },
                  { title: "Read the package", state: "done" },
                ],
              },
            }
          : {};
      return new Response(JSON.stringify(body));
    },
  );
  render(
    <QueryClientProvider
      client={
        new QueryClient({ defaultOptions: { queries: { retry: false } } })
      }
    >
      <ApiContext.Provider value={api}>
        <WorkLog
          tenderId="one"
          runId="run"
          run={run}
          onSource={onSource}
          onRaiseLimit={() => {}}
        />
      </ApiContext.Provider>
    </QueryClientProvider>,
  );
  return requests;
}

const baseRun = {
  id: "run",
  tender_id: "one",
  kind: "manager",
  instruction: "review the package",
  progress: 0,
  detail: "",
  result: {},
  usage: {},
  error: null,
  created_at: "2026-09-15T10:00:00Z",
  updated_at: "2026-09-15T10:07:12Z",
} as unknown as Schema<"Run">;

it("folds a finished job into one summary line that reopens the log", async () => {
  const user = userEvent.setup();
  const onSource = vi.fn();
  renderLog(
    { ...baseRun, status: "completed" } as Schema<"Run">,
    reviewItems(),
    onSource,
  );
  const summary = await screen.findByRole("button", {
    name: /Worked for 7m 12s · read 1 document · searched once/,
  });
  expect(summary).toHaveAttribute("aria-expanded", "false");
  await user.click(summary);
  await user.click(
    screen.getByRole("button", { name: /I'll read the visit schedule/ }),
  );
  await user.click(screen.getByRole("button", { name: /8-موعد الزيارة\.pdf/ }));
  expect(
    screen.getByText("Visit on 21 Sep at 10:00, KSAU-HS site office."),
  ).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "Open page 1" }));
  expect(onSource).toHaveBeenCalledWith({ artifactId: "doc-8", page: 1 });
  expect(
    screen.getByRole("region", { name: "Layla Haddad's work" }),
  ).toHaveTextContent("I'll read Systems.pdf first.");
  expect(screen.queryByText(/deepseek|custom/i)).not.toBeInTheDocument();
});

it("says why a stopped job stopped, what was kept and what is left, and continues it", async () => {
  const user = userEvent.setup();
  const requests = renderLog(
    {
      ...baseRun,
      status: "failed",
      error:
        "AI allowance reached. This Tender has used about USD 1.92 of its USD 2.00 allowance.",
    } as Schema<"Run">,
    reviewItems(),
  );
  const alert = await screen.findByRole("alert");
  expect(alert).toHaveTextContent(
    "Stopped: the spending limit for this tender was reached.",
  );
  await waitFor(() => expect(alert).toHaveTextContent("Kept: read 1 document"));
  expect(
    await within(alert).findByText(/Check the site-visit terms/),
  ).toBeInTheDocument();
  expect(
    within(alert).getByRole("button", { name: "Raise limit" }),
  ).toBeInTheDocument();
  await user.click(within(alert).getByRole("button", { name: "Continue" }));
  expect(requests).toContain("POST /api/runs/run/resume");
});

it("shows an honest waiting line while the AI has not replied yet", async () => {
  nextId = 1;
  renderLog({ ...baseRun, status: "running" } as Schema<"Run">, [
    event({
      category: "model_request",
      phase: "started",
      operation_id: "req-1",
      created_at: new Date().toISOString(),
    }),
  ]);
  expect(
    await screen.findByText("Waiting for the Tender Manager's reply"),
  ).toBeInTheDocument();
  expect(screen.queryByText(/planning|deciding/i)).not.toBeInTheDocument();
  // Stopping lives on the prompt box send button, not in the log.
  expect(
    screen.queryByRole("button", { name: "Stop" }),
  ).not.toBeInTheDocument();
});

it("puts back a space the AI dropped before a common word in its note", () => {
  const vocabulary = new Set([
    "update",
    "my",
    "brief",
    "check",
    "document",
    "now",
    "working",
    "right",
  ]);
  expect(
    repairGluedWords("then update mybrief and myworking notes.", vocabulary),
  ).toBe("then update my brief and my working notes.");
  expect(
    repairGluedWords("I'll check thatdocument, live rightnow.", vocabulary),
  ).toBe("I'll check that document, live right now.");
  // Real words and words the job used as written stay untouched.
  expect(
    repairGluedWords(
      "I checked myself; the thesis mentions thatch.",
      vocabulary,
    ),
  ).toBe("I checked myself; the thesis mentions thatch.");
  expect(repairGluedWords("These documents", vocabulary)).toBe(
    "These documents",
  );
});
