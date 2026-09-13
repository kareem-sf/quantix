import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ApiContext, createApi } from "../api";
import { LocalCodeInspector } from "./LocalCodeInspector";

const run = {
  id: "a".repeat(32),
  engine: "monty",
  root_run_id: "root-a",
  actor_id: "manager",
  assignment_id: null,
  legacy_attribution: false,
  status: "running",
  phase: "calling_reviewed_tool",
  created_at: "2026-09-13T00:00:00Z",
  duration_seconds: 0,
  detail: "",
  root_active: true,
  method: null,
};
function setup(fetcher: typeof fetch) {
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    fetcher,
  );
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return (
    <QueryClientProvider client={client}>
      <ApiContext.Provider value={api}>
        <LocalCodeInspector tenderId="tender-a" runId="root-a" />
      </ApiContext.Provider>
    </QueryClientProvider>
  );
}

it("loads only on opening, checks evidence on demand and labels root cancellation", async () => {
  const reads: string[] = [],
    writes: string[] = [];
  render(
    setup(async (url, options) => {
      if (options?.method === "POST") {
        writes.push(String(url));
        return new Response("{}");
      }
      reads.push(String(url));
      if (String(url).includes(`/monty/${run.id}`))
        return new Response(
          JSON.stringify({
            run,
            code: "quantity * 7",
            code_sha256: "hash",
            record_integrity: "verified",
            limits: { seconds: 10 },
            runtime: { engine_version: "0.0.23" },
            inputs: [],
            outputs: [],
            logs: [],
            input_values: { quantity: 6 },
            inputs_sha256: "input-hash",
            output: 42,
            calls: [],
          }),
        );
      return new Response(JSON.stringify({ items: [run], next_cursor: null }));
    }),
  );
  expect(reads).toEqual([]);
  const user = userEvent.setup({ delay: null });
  await user.click(screen.getByText("Local calculations and code"));
  expect(
    await screen.findByText("Monty tool composition — running"),
  ).toBeInTheDocument();
  expect(reads).toHaveLength(1);
  await user.click(
    screen.getByRole("button", { name: "Inspect code and evidence" }),
  );
  expect(
    await screen.findByText("Receipt, code and saved-file hashes verified."),
  ).toBeInTheDocument();
  await user.click(screen.getByText("Executed code"));
  expect(screen.getByText("quantity * 7")).toBeInTheDocument();
  expect(
    screen.getByText(/Stop cancels the whole work request and its colleagues/),
  ).toBeInTheDocument();
  await user.click(
    screen.getByRole("button", { name: "Stop this work request" }),
  );
  await waitFor(() =>
    expect(writes).toEqual(["http://localhost/api/runs/root-a/cancel"]),
  );
});

it("retains a verification failure and permits retry without showing unverified code", async () => {
  let attempt = 0;
  render(
    setup(async (url) => {
      if (String(url).includes(`/monty/${run.id}`)) {
        attempt++;
        return new Response(
          JSON.stringify({ detail: "The saved code changed." }),
          { status: 409 },
        );
      }
      return new Response(
        JSON.stringify({
          items: [
            {
              ...run,
              status: "failed",
              root_active: false,
              legacy_attribution: true,
            },
          ],
          next_cursor: "page-two",
        }),
      );
    }),
  );
  const user = userEvent.setup({ delay: null });
  await user.click(screen.getByText("Local calculations and code"));
  await user.click(
    await screen.findByRole("button", { name: "Inspect code and evidence" }),
  );
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "The saved code changed.",
  );
  expect(screen.queryByText("Executed code")).not.toBeInTheDocument();
  expect(
    screen.getByText(
      /Historical record: colleague attribution was not captured/,
    ),
  ).toBeInTheDocument();
  expect(
    screen.getByRole("button", { name: "Load more code records" }),
  ).toBeInTheDocument();
  await user.click(
    screen.getByRole("button", { name: "Inspect code and evidence" }),
  );
  await waitFor(() => expect(attempt).toBe(2));
});
