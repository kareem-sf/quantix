import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ApiContext, createApi, type Schema } from "../api";
import { Takeoff } from "./Takeoff";

function line(
  id: string,
  changes: Partial<Schema<"TakeoffLine">>,
): Schema<"TakeoffLine"> {
  return {
    id,
    tender_id: "one",
    run_id: "run",
    author: "Samir Haddad",
    description: "Pad footings concrete",
    location: "Grid A-C/1-3",
    unit: "m3",
    quantity: "14",
    method: "schedule",
    working: "7 x 2.0 x 2.0 x 0.5",
    source_ids: [],
    boq_item_id: "item",
    boq: {
      description: "Cast concrete foundations",
      unit: "m3",
      quantity: "12.5",
    },
    comparison: "differs",
    difference: "1.5",
    difference_percent: "12.0",
    status: "proposed",
    review_note: "",
    is_current: true,
    created_at: "",
    updated_at: "",
    ...changes,
  };
}

function renderTakeoff() {
  const posts: Array<{ path: string; body: unknown }> = [];
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async (address, options) => {
      const path = new URL(String(address)).pathname;
      if (options?.method === "POST") {
        posts.push({ path, body: JSON.parse(String(options.body)) });
        return Response.json(
          path.endsWith("/messages") ? { outcome: "immediate" } : {},
        );
      }
      if (path.endsWith("/takeoff"))
        return Response.json([
          line("differs", {}),
          line("missing", {
            description: "Precast manholes",
            unit: "nr",
            quantity: "3",
            boq_item_id: null,
            boq: null,
            comparison: "not_in_boq",
            difference: null,
            difference_percent: null,
          }),
          line("matched", {
            description: "Ground beam",
            comparison: "matches",
            quantity: "12.5",
          }),
        ]);
      throw new Error(`Unexpected request: ${path}`);
    },
  );
  render(
    <QueryClientProvider
      client={
        new QueryClient({ defaultOptions: { queries: { retry: false } } })
      }
    >
      <ApiContext.Provider value={api}>
        <Takeoff tenderId="one" onSource={vi.fn()} />
      </ApiContext.Provider>
    </QueryClientProvider>,
  );
  return posts;
}

it("shows what needs a decision first and compares drawing and BOQ quantities", async () => {
  const user = userEvent.setup();
  renderTakeoff();
  const list = await screen.findByRole("list", { name: "Takeoff lines" });
  expect(within(list).getByText("Pad footings concrete")).toBeVisible();
  expect(within(list).getByText("Precast manholes")).toBeVisible();
  expect(within(list).queryByText("Ground beam")).not.toBeInTheDocument();
  expect(within(list).getByText("BOQ 12.5 m3 (+12.0%)")).toBeVisible();
  await user.click(screen.getByRole("button", { name: /Missing from BOQ/ }));
  expect(
    within(screen.getByRole("list", { name: "Takeoff lines" })).queryByText(
      "Pad footings concrete",
    ),
  ).not.toBeInTheDocument();
});

it("records the engineer's review with a note", async () => {
  const user = userEvent.setup();
  const posts = renderTakeoff();
  await user.click(await screen.findByText("Precast manholes"));
  const row = screen.getByText("Precast manholes").closest("details")!;
  await user.type(
    within(row).getByLabelText("Your note (optional)"),
    "Raise as a clarification",
  );
  await user.click(within(row).getByRole("button", { name: "Accept" }));
  await waitFor(() => expect(posts).toHaveLength(1));
  expect(posts[0]).toEqual({
    path: "/api/tenders/one/takeoff/missing/review",
    body: { decision: "accepted", note: "Raise as a clarification" },
  });
});

it("asks the Tender Manager for a takeoff", async () => {
  const user = userEvent.setup();
  const posts = renderTakeoff();
  await user.click(
    await screen.findByRole("button", { name: "Ask for a new takeoff" }),
  );
  await waitFor(() => expect(posts).toHaveLength(1));
  expect(posts[0].path).toBe("/api/tenders/one/messages");
  expect(String((posts[0].body as { content: string }).content)).toMatch(
    /quantity takeoff/,
  );
  expect(await screen.findByText(/Sent to the Tender Manager/)).toBeVisible();
});
