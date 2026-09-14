import { render, screen } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ApiContext, createApi } from "../api";
import { WorkDecisions } from "./WorkDecisions";

it("keeps findings and their source scoped decisions in Work", async () => {
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async () =>
      new Response(
        JSON.stringify([
          {
            id: "finding",
            tender_id: "one",
            title: "Unclear fire stopping",
            detail: "The drawing does not state the rated assembly.",
            kind: "question",
            state: "proposed",
            source_ids: ["source"],
            origin: "agent",
            run_id: "run",
            is_stale: false,
            created_at: "",
            updated_at: "",
          },
        ]),
      ),
  );
  render(
    <QueryClientProvider client={new QueryClient()}>
      <ApiContext.Provider value={api}>
        <WorkDecisions tenderId="one" onSource={() => {}} />
      </ApiContext.Provider>
    </QueryClientProvider>,
  );
  expect(await screen.findByText("Unclear fire stopping")).toBeInTheDocument();
  expect(screen.getByText(/Source-scoped decision/)).toBeInTheDocument();
});
