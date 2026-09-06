import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ApiContext, createApi } from "../api";
import { SearchPreparation } from "./DocumentSearch";

it("reports a failed preparation run instead of announcing that Meaning search is ready", async () => {
  const run = {
    id: "index-job",
    tender_id: "one",
    kind: "index",
    instruction: "Prepare search",
    status: "queued",
    progress: 0,
    detail: "Waiting to prepare search.",
    result: {},
    usage: {},
    created_at: "2026-09-06T10:00:00Z",
    updated_at: "2026-09-06T10:00:00Z",
  };
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async (url, init) => {
      if (String(url).endsWith("/search-index")) {
        expect(init?.body).toBeUndefined();
        return new Response(JSON.stringify(run));
      }
      if (String(url).endsWith("/runs/index-job"))
        return new Response(
          JSON.stringify({
            ...run,
            status: "failed",
            error: "The search files could not be downloaded.",
          }),
        );
      return new Response(
        JSON.stringify({
          status: "model_missing",
          ready: false,
          detail: "The local search files must be prepared.",
        }),
      );
    },
  );
  const user = userEvent.setup();
  render(
    <QueryClientProvider client={new QueryClient()}>
      <ApiContext.Provider value={api}>
        <SearchPreparation tenderId="one" activeRuns={[]} documentKey="one" />
      </ApiContext.Provider>
    </QueryClientProvider>,
  );
  await user.click(
    await screen.findByRole("button", { name: "Prepare search" }),
  );
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "The search files could not be downloaded.",
  );
  expect(
    screen.queryByText("Meaning search is ready for the current documents."),
  ).not.toBeInTheDocument();
});
