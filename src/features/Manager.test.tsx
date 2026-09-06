import { render, screen } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ApiContext, createApi, type Schema } from "../api";
import { Manager } from "./Manager";

it("keeps an asynchronous manager failure visible after it leaves the active runs", async () => {
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async (url) =>
      new Response(
        JSON.stringify(
          String(url).endsWith("/runs")
            ? [
                {
                  id: "run",
                  tender_id: "one",
                  kind: "manager",
                  instruction: "Review scope",
                  status: "failed",
                  progress: 0,
                  detail: "Review failed",
                  result: {},
                  usage: {},
                  error: "Provider unavailable",
                  created_at: "2026-09-06T12:00:00Z",
                  updated_at: "2026-09-06T12:01:00Z",
                },
              ]
            : [],
        ),
      ),
  );
  const overview: Schema<"Overview"> = {
    tender: {
      id: "one",
      name: "Test tender",
      status: "active",
      revision: 1,
      created_at: "",
      updated_at: "",
    },
    artifact_count: 1,
    evidence_count: 1,
    coverage: {
      registered: 1,
      extracted: 1,
      needs_attention: 0,
      unsupported: 0,
      failed: 0,
    },
    areas: [],
    findings: [],
    plan: null,
    active_runs: [],
    boq_count: 0,
  };
  render(
    <QueryClientProvider
      client={
        new QueryClient({ defaultOptions: { queries: { retry: false } } })
      }
    >
      <ApiContext.Provider value={api}>
        <Manager
          overview={overview}
          artifacts={[]}
          onImport={() => {}}
          onSettings={() => {}}
          onSource={() => {}}
        />
      </ApiContext.Provider>
    </QueryClientProvider>,
  );
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "Provider unavailable",
  );
});
