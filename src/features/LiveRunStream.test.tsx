import {
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ApiContext, createApi } from "../api";
import { LiveRunStream } from "./LiveRunStream";

const activity = (id: number, extra = {}) => ({
  event_id: id,
  run_id: "run",
  operation_id: `op-${id}`,
  parent_operation_id: null,
  actor_id: "manager",
  actor_label: "Tender Manager",
  assignment_id: null,
  category: "tool",
  phase: "completed",
  message: `Work event ${id}`,
  created_at: "2026-09-13T12:00:00Z",
  tool: "read_sources",
  provider: "openai",
  model: "selected-model",
  preview: "Readable result preview",
  detail_available: true,
  capture_status: "captured",
  elapsed_ms: 1200,
  source_ids: [],
  ...extra,
});
const page = (items = [activity(1)], extra = {}) => ({
  items,
  cursor: "end",
  before_cursor: "begin",
  has_more: false,
  has_earlier: false,
  reset_required: false,
  run_status: "completed",
  run_detail: "Work finished",
  run_updated_at: "2026-09-13T12:00:02Z",
  history_key: "history",
  ...extra,
});
function setup(respond: (url: URL) => unknown) {
  const requests: { url: URL; init?: RequestInit }[] = [];
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async (url, init) => {
      const parsed = new URL(String(url));
      requests.push({ url: parsed, init });
      return new Response(JSON.stringify(respond(parsed)));
    },
  );
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  render(
    <QueryClientProvider client={client}>
      <ApiContext.Provider value={api}>
        <LiveRunStream tenderId="tender" runId="run" />
      </ApiContext.Provider>
    </QueryClientProvider>,
  );
  return { requests, client };
}

it("keeps completed activity visible and opens full paged detail without a second observer or work mutation", async () => {
  const { requests } = setup((url) =>
    url.pathname.endsWith("/1")
      ? {
          activity: activity(1),
          text:
            url.searchParams.get("offset") === "0"
              ? "First input page"
              : "Final output page",
          offset: Number(url.searchParams.get("offset")),
          total_chars: 32,
          next_offset: url.searchParams.get("offset") === "0" ? 16 : 32,
          has_more: url.searchParams.get("offset") === "0",
          redacted: false,
          unavailable_fields: [],
        }
      : page(),
  );
  expect(await screen.findByText("Work event 1")).toBeVisible();
  fireEvent.click(screen.getByRole("button", { name: "Inspect activity" }));
  const inspector = await screen.findByRole("dialog");
  fireEvent.click(
    within(inspector).getByRole("button", {
      name: "Open details: Work event 1",
    }),
  );
  expect(await within(inspector).findByText("First input page")).toBeVisible();
  fireEvent.click(
    within(inspector).getByRole("button", { name: "Next detail page" }),
  );
  expect(await within(inspector).findByText("Final output page")).toBeVisible();
  expect(
    requests.filter(({ url }) => url.pathname.endsWith("/activity")),
  ).toHaveLength(1);
  expect(requests.every(({ init }) => init?.method === "GET")).toBe(true);
});

it("loads older activity in order and searches the full server history", async () => {
  const { requests } = setup((url) =>
    url.searchParams.has("before")
      ? page([activity(1)])
      : url.searchParams.get("q") === "foundation"
        ? page([activity(8, { message: "Foundation checked" })])
        : page([activity(3)], { has_earlier: true }),
  );
  await screen.findByText("Work event 3");
  fireEvent.click(screen.getByRole("button", { name: "Inspect activity" }));
  const inspector = screen.getByRole("dialog");
  fireEvent.click(
    within(inspector).getByRole("button", { name: "Load earlier activity" }),
  );
  await within(inspector).findByText("Work event 1");
  const rows = within(inspector).getAllByRole("listitem");
  expect(rows[0]).toHaveTextContent("Work event 1");
  expect(rows[1]).toHaveTextContent("Work event 3");
  fireEvent.change(
    within(inspector).getByRole("searchbox", { name: "Search activity" }),
    { target: { value: "foundation" } },
  );
  expect(
    await within(inspector).findByText("Foundation checked"),
  ).toBeVisible();
  expect(
    requests.some(({ url }) => url.searchParams.get("q") === "foundation"),
  ).toBe(true);
});

it("replaces stale history on reset instead of merging events from the previous history", async () => {
  let response = page([activity(1)], { run_status: "running" });
  const { client } = setup(() => response);
  await screen.findByText("Work event 1");
  response = page([activity(9)], {
    reset_required: true,
    history_key: "new-history",
  });
  await client.invalidateQueries();
  expect(await screen.findByText("Work event 9")).toBeVisible();
  await waitFor(() =>
    expect(screen.queryByText("Work event 1")).not.toBeInTheDocument(),
  );
});

it("uses server character offsets for Unicode detail pages and opens captured source references", async () => {
  const onSource = vi.fn();
  const record = activity(1, {
    source_ids: ["source-7"],
    artifact_refs: [{ artifact_id: "drawing-4", page: 3 }],
  });
  const urls: URL[] = [];
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async (url) => {
      const parsed = new URL(String(url));
      urls.push(parsed);
      const body = parsed.pathname.endsWith("/1")
        ? {
            activity: record,
            text: parsed.searchParams.get("offset") === "0" ? "😀" : "complete",
            offset: Number(parsed.searchParams.get("offset")),
            next_offset: parsed.searchParams.get("offset") === "0" ? 1 : 9,
            total_chars: 9,
            has_more: parsed.searchParams.get("offset") === "0",
            redacted: false,
            unavailable_fields: [],
          }
        : page([record]);
      return new Response(JSON.stringify(body));
    },
  );
  render(
    <QueryClientProvider client={new QueryClient()}>
      <ApiContext.Provider value={api}>
        <LiveRunStream tenderId="tender" runId="run" onSource={onSource} />
      </ApiContext.Provider>
    </QueryClientProvider>,
  );
  await screen.findByText("Work event 1");
  fireEvent.click(
    screen.getByRole("button", { name: "Open details: Work event 1" }),
  );
  const inspector = screen.getByRole("dialog");
  await within(inspector).findByText("😀");
  fireEvent.click(
    within(inspector).getByRole("button", { name: "Next detail page" }),
  );
  await within(inspector).findByText("complete");
  expect(urls.at(-1)?.searchParams.get("offset")).toBe("1");
  fireEvent.click(
    within(inspector).getByRole("button", { name: "Open source 1" }),
  );
  expect(onSource).toHaveBeenCalledWith({ sourceId: "source-7" });
  fireEvent.click(
    screen.getByRole("button", { name: "Open details: Work event 1" }),
  );
  fireEvent.click(
    await screen.findByRole("button", {
      name: "Open referenced file 1 · page 3",
    }),
  );
  expect(onSource).toHaveBeenCalledWith({ artifactId: "drawing-4", page: 3 });
});

it("retains actual person choices while server filters return no events and identifies shortened previews", async () => {
  setup((url) =>
    url.searchParams.get("errors_only") === "true"
      ? page([])
      : page([
          activity(1, {
            actor_id: "staff-7",
            actor_label: "Mona · Quantity surveyor",
            preview_truncated: true,
          }),
        ]),
  );
  await screen.findByText("Work event 1");
  fireEvent.click(screen.getByRole("button", { name: "Inspect activity" }));
  const inspector = screen.getByRole("dialog");
  expect(
    within(inspector).getByText(
      "Preview shortened · open captured details for more",
    ),
  ).toBeVisible();
  fireEvent.change(
    within(inspector).getByRole("combobox", { name: "Filter by person" }),
    { target: { value: "staff-7" } },
  );
  await within(inspector).findByText("Work event 1");
  fireEvent.click(
    within(inspector).getByRole("checkbox", { name: "Errors only" }),
  );
  await within(inspector).findByText(
    "No captured activity matches these filters.",
  );
  expect(
    within(inspector).getByRole("option", { name: "Mona · Quantity surveyor" }),
  ).toBeInTheDocument();
});

it("keeps a bounded history window with older, later and latest navigation", async () => {
  setup((url) => {
    const start =
      url.searchParams.get("before") === "301"
        ? 201
        : url.searchParams.get("before") === "201"
          ? 101
          : url.searchParams.get("before") === "101"
            ? 1
            : 301;
    return page(
      Array.from({ length: 100 }, (_, i) =>
        activity(start + i, {
          preview: "",
          provider: null,
          model: null,
          tool: null,
          elapsed_ms: null,
          detail_available: false,
        }),
      ),
      {
        cursor: String(start + 99),
        before_cursor: String(start),
        has_earlier: start > 1,
      },
    );
  });
  await screen.findByText("Work event 400");
  fireEvent.click(screen.getByRole("button", { name: "Inspect activity" }));
  const inspector = screen.getByRole("dialog");
  for (const id of [201, 101, 1]) {
    fireEvent.click(
      within(inspector).getByRole("button", { name: "Load earlier activity" }),
    );
    await within(inspector).findByText(`Work event ${id}`);
  }
  expect(
    within(inspector).queryByText("Work event 400"),
  ).not.toBeInTheDocument();
  expect(within(inspector).getAllByRole("listitem").length).toBeLessThanOrEqual(
    300,
  );
  fireEvent.click(
    within(inspector).getByRole("button", { name: "Load later activity" }),
  );
  await within(inspector).findByText("Work event 400");
  expect(within(inspector).queryByText("Work event 1")).not.toBeInTheDocument();
  fireEvent.click(
    within(inspector).getByRole("button", { name: "Jump to latest" }),
  );
  await waitFor(() =>
    expect(within(inspector).getAllByRole("listitem")).toHaveLength(100),
  );
}, 30000);

it("retains saved events when updates fail and reconnects from the same cursor", async () => {
  let failed = false;
  const { client, requests } = setup(() => {
    if (failed) throw new Error("Connection unavailable");
    return page([activity(1)]);
  });
  await screen.findByText("Work event 1");
  failed = true;
  await client.invalidateQueries();
  expect(await screen.findByText("Updates paused")).toBeVisible();
  expect(screen.getByText("Work event 1")).toBeVisible();
  failed = false;
  fireEvent.click(screen.getByRole("button", { name: "Reconnect updates" }));
  await waitFor(() =>
    expect(screen.queryByText("Updates paused")).not.toBeInTheDocument(),
  );
  expect(requests.at(-1)?.url.searchParams.get("after")).toBe("end");
});

it("joins readable draft chunks by operation and keeps structured payloads in details", async () => {
  setup(() =>
    page([
      activity(1, {
        category: "draft",
        phase: "delta",
        operation_id: "draft-one",
        preview: "Concrete ",
      }),
      activity(2, {
        category: "draft",
        phase: "delta",
        operation_id: "draft-one",
        preview: "quantities checked.",
      }),
      activity(3, { preview: '{"private_input":{"cells":[1,2,3]}}' }),
    ]),
  );
  expect(await screen.findByText("Concrete quantities checked.")).toBeVisible();
  expect(
    screen.queryByText('{"private_input":{"cells":[1,2,3]}}'),
  ).not.toBeInTheDocument();
  expect(
    screen.getByText("Structured detail captured. Open details to inspect it."),
  ).toBeVisible();
});

it("shows every loaded operation inline with tool, elapsed and expandable child activity", async () => {
  setup(() =>
    page(
      Array.from({ length: 12 }, (_, i) =>
        activity(
          i + 1,
          i === 1
            ? {
                parent_operation_id: "op-1",
                category: "reasoning_summary",
                tool: null,
              }
            : {},
        ),
      ),
    ),
  );
  await screen.findByText("Work event 12");
  expect(screen.getByText("Work event 1")).toBeVisible();
  expect(screen.getAllByText("Tool: read_sources").length).toBeGreaterThan(0);
  expect(screen.getAllByText("1.2s elapsed").length).toBeGreaterThan(0);
  expect(screen.getByText(/Thinking summary/)).toBeVisible();
  fireEvent.click(
    screen.getByRole("button", { name: "Hide child activity: Work event 1" }),
  );
  expect(screen.queryByText("Work event 2")).not.toBeInTheDocument();
  fireEvent.click(
    screen.getByRole("button", { name: "Show child activity: Work event 1" }),
  );
  expect(screen.getByText("Work event 2")).toBeVisible();
});

it("opens input and outcome captures for the selected operation without losing the selected event", async () => {
  setup((url) =>
    /\/activity\/\d+$/.test(url.pathname)
      ? {
          activity: activity(Number(url.pathname.split("/").at(-1))),
          text: url.pathname.endsWith("/1")
            ? "Original inputs"
            : "Final outcome",
          offset: 0,
          next_offset: 14,
          total_chars: 14,
          has_more: false,
          input_event_id: 1,
          output_event_id: 2,
        }
      : page([activity(2)]),
  );
  await screen.findByText("Work event 2");
  fireEvent.click(
    screen.getByRole("button", { name: "Open details: Work event 2" }),
  );
  const inspector = screen.getByRole("dialog");
  await within(inspector).findByText("Final outcome");
  fireEvent.click(within(inspector).getByRole("button", { name: "Inputs" }));
  expect(await within(inspector).findByText("Original inputs")).toBeVisible();
  fireEvent.click(within(inspector).getByRole("button", { name: "Outcome" }));
  expect(await within(inspector).findByText("Final outcome")).toBeVisible();
});

it("reloads full details when restored history reuses an event ID", async () => {
  let restored = false;
  const { client } = setup((url) =>
    url.pathname.endsWith("/1")
      ? {
          activity: activity(1),
          text: restored ? "Restored detail" : "Old detail",
          offset: 0,
          next_offset: 12,
          total_chars: 12,
          has_more: false,
        }
      : page([activity(1)], {
          history_key: restored ? "restored" : "history",
          reset_required: restored,
        }),
  );
  await screen.findByText("Work event 1");
  fireEvent.click(
    screen.getByRole("button", { name: "Open details: Work event 1" }),
  );
  await screen.findByText("Old detail");
  restored = true;
  await client.invalidateQueries({ queryKey: ["run-activity"] });
  await waitFor(() =>
    expect(screen.queryByText("Old detail")).not.toBeInTheDocument(),
  );
  const inspector = screen.getByRole("dialog");
  fireEvent.click(
    within(inspector).getByRole("button", {
      name: "Open details: Work event 1",
    }),
  );
  expect(await screen.findByText("Restored detail")).toBeVisible();
});

it("returns to live updates after closing an inspector paused on earlier work", async () => {
  let advanced = false;
  setup((url) =>
    page([activity(url.searchParams.has("before") ? 1 : advanced ? 3 : 2)], {
      run_status: "running",
      has_earlier: true,
    }),
  );
  await screen.findByText("Work event 2");
  fireEvent.click(screen.getByRole("button", { name: "Inspect activity" }));
  const inspector = screen.getByRole("dialog");
  fireEvent.click(
    within(inspector).getByRole("button", { name: "Load earlier activity" }),
  );
  await within(inspector).findByText("Work event 1");
  advanced = true;
  fireEvent.click(within(inspector).getByRole("button", { name: "Close" }));
  expect(await screen.findByText("Work event 3")).toBeVisible();
});
