import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ApiContext, createApi, type Schema } from "../../api";
import { groupJobs, JobHistory, jobSummaryLine } from "./JobHistory";

const today = new Date();
const at = (minutes: number) =>
  new Date(today.getTime() - minutes * 60_000).toISOString();

function run(id: string, changes: Partial<Schema<"Run">>): Schema<"Run"> {
  return {
    id,
    tender_id: "one",
    kind: "manager",
    instruction: "review the package",
    status: "completed",
    progress: 0,
    detail: "",
    result: {},
    usage: {},
    error: null,
    created_at: at(60),
    updated_at: at(55),
    ...changes,
  } as Schema<"Run">;
}

const runs = [
  run("a1", {
    status: "failed",
    error: "The provider stopped its reply with an error part-way through.",
    created_at: at(50),
    updated_at: at(45),
  }),
  run("a2", {
    status: "failed",
    error: "AI allowance reached. This Tender has used about USD 1.92.",
    created_at: at(40),
    updated_at: at(31),
  }),
  run("q", {
    instruction: "Who are you?",
    created_at: at(90),
    updated_at: at(90),
  }),
];

it("groups resumed attempts at the same request into one job", () => {
  const jobs = groupJobs(runs);
  expect(jobs.map((job) => [job.request, job.attempts.length])).toEqual([
    ["review the package", 2],
    ["Who are you?", 1],
  ]);
  expect(jobs[0].latest.id).toBe("a2");
});

it("lists jobs newest first with one plain reason and one Continue", async () => {
  const user = userEvent.setup();
  const requests: string[] = [];
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async (url, init) => {
      requests.push(
        `${init?.method ?? "GET"} ${new URL(String(url)).pathname}`,
      );
      return new Response("{}");
    },
  );
  render(
    <QueryClientProvider
      client={
        new QueryClient({ defaultOptions: { queries: { retry: false } } })
      }
    >
      <ApiContext.Provider value={api}>
        <JobHistory tenderId="one" runs={runs} />
      </ApiContext.Provider>
    </QueryClientProvider>,
  );
  // Jobs may fall on either side of midnight, so read them across day groups.
  const items = screen.getAllByRole("listitem");
  expect(items).toHaveLength(2);
  expect(items[0]).toHaveTextContent("review the package");
  expect(items[0]).toHaveTextContent(
    "Stopped: the spending limit for this tender was reached.",
  );
  expect(items[0]).toHaveTextContent("stopped · 19m 00s");
  expect(
    within(items[1]).queryByRole("button", { name: "Continue" }),
  ).not.toBeInTheDocument();
  expect(
    within(items[0]).getAllByRole("button", { name: "Continue" }),
  ).toHaveLength(1);

  await user.click(
    within(items[0]).getByRole("button", { name: "2 attempts" }),
  );
  const attempts = within(items[0]).getByRole("list", { name: "Attempts" });
  expect(attempts).toHaveTextContent(
    "stopped: the AI service dropped its reply",
  );

  await user.click(within(items[0]).getByRole("button", { name: "Continue" }));
  expect(requests).toContain("POST /api/runs/a2/resume");
});

it("keeps provider details in a collapsed technical log for each job", async () => {
  const user = userEvent.setup();
  const requests: string[] = [];
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async (url) => {
      const path = new URL(String(url)).pathname;
      requests.push(path);
      return new Response(
        JSON.stringify({
          run_status: "completed",
          history_key: "h",
          cursor: null,
          before_cursor: null,
          has_more: false,
          has_earlier: false,
          items: [
            {
              event_id: 1,
              run_id: "q",
              actor_label: "Tender Manager",
              category: "model_request",
              phase: "completed",
              message: "AI request",
              created_at: at(90),
              provider: "google",
              model: "gemini-test",
              preview: "",
              capture_status: "captured",
              detail_available: false,
            },
          ],
        }),
      );
    },
  );
  render(
    <QueryClientProvider
      client={
        new QueryClient({ defaultOptions: { queries: { retry: false } } })
      }
    >
      <ApiContext.Provider value={api}>
        <JobHistory
          tenderId="one"
          runs={[run("q", { kind: "analysis", instruction: "" })]}
        />
      </ApiContext.Provider>
    </QueryClientProvider>,
  );
  await user.click(
    screen.getByRole("button", { name: /Analysing the tender package/ }),
  );
  expect(screen.queryByText(/gemini-test/)).not.toBeInTheDocument();
  expect(requests.some((path) => path.includes("/activity"))).toBe(false);

  await user.click(screen.getByRole("button", { name: "Technical log" }));
  expect(await screen.findByText("google · gemini-test")).toBeInTheDocument();
  expect(requests.some((path) => /\/runs\/q\/activity$/.test(path))).toBe(true);
});

it("sums what every attempt of a job did into one line", async () => {
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async (url) =>
      new URL(String(url)).pathname.endsWith("/job-summaries")
        ? Response.json({
            jobs: [
              {
                run_id: "a1",
                documents: ["Spec.pdf", "BOQ.xlsx"],
                searches: 2,
                page_views: 0,
                staff_hired: 1,
                colleagues_asked: ["Layla"],
                drafts_saved: 0,
                proposals: 0,
              },
              {
                run_id: "a2",
                documents: ["Spec.pdf", "Drawings.pdf"],
                searches: 1,
                page_views: 0,
                staff_hired: 0,
                colleagues_asked: ["Layla"],
                drafts_saved: 0,
                proposals: 0,
              },
            ],
          })
        : new Response("{}"),
  );
  render(
    <QueryClientProvider client={new QueryClient()}>
      <ApiContext.Provider value={api}>
        <JobHistory tenderId="one" runs={runs} />
      </ApiContext.Provider>
    </QueryClientProvider>,
  );
  expect(
    await screen.findByText(
      "Read 3 documents · searched 3 times · hired 1 staff member · asked 1 colleague",
    ),
  ).toBeInTheDocument();
  expect(jobSummaryLine([])).toBe("");
});
