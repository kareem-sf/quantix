import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ApiContext, createApi } from "../api";
import { SearchPreparation } from "./DocumentSearch";

it("reports a failed preparation instead of announcing that Meaning search is ready", async () => {
  let indexed = false;
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async (url, init) => {
      if (String(url).endsWith("/search-index")) {
        expect(init?.body).toBeUndefined();
        indexed = true;
        return new Response(
          JSON.stringify({
            status: "failed",
            ready: false,
            detail: "The search files could not be downloaded.",
            last_error: "The search files could not be downloaded.",
          }),
        );
      }
      return new Response(
        JSON.stringify(
          indexed
            ? {
                status: "failed",
                ready: false,
                detail: "The search files could not be downloaded.",
                last_error: "The search files could not be downloaded.",
              }
            : {
                status: "model_missing",
                ready: false,
                detail: "The local search files must be prepared.",
              },
        ),
      );
    },
  );
  const user = userEvent.setup();
  render(
    <QueryClientProvider client={new QueryClient()}>
      <ApiContext.Provider value={api}>
        <SearchPreparation tenderId="one" documentKey="one" />
      </ApiContext.Provider>
    </QueryClientProvider>,
  );
  await user.click(
    await screen.findByRole("button", { name: "Prepare search" }),
  );
  expect(
    await screen.findByText("Search could not be prepared"),
  ).toBeInTheDocument();
  expect(
    screen.getByText("The search files could not be downloaded."),
  ).toBeInTheDocument();
  expect(
    screen.queryByText("Documents are ready to search."),
  ).not.toBeInTheDocument();
});
