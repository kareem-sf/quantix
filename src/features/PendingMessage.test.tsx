import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ApiContext, createApi } from "../api";
import { PendingMessage } from "./PendingMessage";

it("requires explicit confirmation before a held instruction is sent", async () => {
  const user = userEvent.setup();
  const calls: string[] = [];
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async (url, init) => {
      calls.push(`${init?.method ?? "GET"} ${String(url)}`);
      return new Response(
        JSON.stringify({ outcome: "immediate", run: { id: "run" } }),
        { status: 200 },
      );
    },
  );
  render(
    <QueryClientProvider client={new QueryClient()}>
      <ApiContext.Provider value={api}>
        <PendingMessage
          tenderId="one"
          pending={{
            id: "pending",
            tender_id: "one",
            content: "Review the drawings",
            action: null,
            status: "held",
            hold_reason: "stopped",
            idempotency_key: "key",
            wait_for_run_ids: [],
            revision: 1,
            created_at: "",
            updated_at: "",
          }}
          onChanged={() => {}}
        />
      </ApiContext.Provider>
    </QueryClientProvider>,
  );
  expect(screen.getByText("Held for your confirmation")).toBeInTheDocument();
  expect(
    screen.queryByText("Review the drawings", { selector: "textarea" }),
  ).not.toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "Confirm and send" }));
  expect(calls.some((call) => call.includes("/pending-message/confirm"))).toBe(
    true,
  );
});

it("edits and cancels only the displayed pending revision without dispatching", async () => {
  const user = userEvent.setup();
  const requests: Array<{ method: string; body: unknown }> = [];
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async (_url, init) => {
      requests.push({
        method: init?.method ?? "GET",
        body: init?.body ? JSON.parse(String(init.body)) : null,
      });
      return new Response(JSON.stringify({}));
    },
  );
  render(
    <QueryClientProvider client={new QueryClient()}>
      <ApiContext.Provider value={api}>
        <PendingMessage
          tenderId="one"
          pending={{
            id: "pending",
            tender_id: "one",
            content: "Review drawings",
            action: "review_documents",
            status: "held",
            hold_reason: "permission",
            idempotency_key: "key",
            wait_for_run_ids: [],
            revision: 3,
            created_at: "",
            updated_at: "",
          }}
          onChanged={() => {}}
        />
      </ApiContext.Provider>
    </QueryClientProvider>,
  );
  await user.click(screen.getByRole("button", { name: "Edit" }));
  await user.clear(
    screen.getByRole("textbox", { name: "Pending instruction" }),
  );
  await user.type(
    screen.getByRole("textbox", { name: "Pending instruction" }),
    "Review revised drawings",
  );
  await user.click(screen.getByRole("button", { name: "Save instruction" }));
  expect(requests[0]).toEqual({
    method: "PATCH",
    body: {
      pending_id: "pending",
      expected_revision: 3,
      content: "Review revised drawings",
      action: "review_documents",
    },
  });
  await user.click(screen.getByRole("button", { name: "Cancel instruction" }));
  expect(requests[1]).toEqual({
    method: "DELETE",
    body: { pending_id: "pending", expected_revision: 3 },
  });
  expect(requests).toHaveLength(2);
});
