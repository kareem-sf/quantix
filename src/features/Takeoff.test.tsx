import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ApiContext, createApi, type Schema } from "../api";
import { TakeoffRow } from "./Takeoff";

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

it("records the engineer's review of a drawing line with a note", async () => {
  const user = userEvent.setup();
  const posts: Array<{ path: string; body: unknown }> = [];
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async (address, options) => {
      const path = new URL(String(address)).pathname;
      if (options?.method === "POST") {
        posts.push({ path, body: JSON.parse(String(options.body)) });
        return Response.json({});
      }
      throw new Error(`Unexpected request: ${path}`);
    },
  );
  render(
    <QueryClientProvider client={new QueryClient()}>
      <ApiContext.Provider value={api}>
        <TakeoffRow
          tenderId="one"
          line={line("missing", {
            description: "Precast manholes",
            unit: "nr",
            quantity: "3",
            boq_item_id: null,
            boq: null,
            comparison: "not_in_boq",
          })}
          onSource={vi.fn()}
        />
      </ApiContext.Provider>
    </QueryClientProvider>,
  );
  await user.click(screen.getByText("Precast manholes"));
  await user.type(
    screen.getByLabelText("Your note (optional)"),
    "Raise as a clarification",
  );
  await user.click(screen.getByRole("button", { name: "Accept" }));
  await waitFor(() => expect(posts).toHaveLength(1));
  expect(posts[0]).toEqual({
    path: "/api/tenders/one/takeoff/missing/review",
    body: { decision: "accepted", note: "Raise as a clarification" },
  });
});
