import { render, screen } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ApiContext, createApi } from "../api";
import { CompanyLibrary } from "./CompanyLibrary";

it("keeps company files separate from the current Tender", () => {
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async () => Response.json({}),
  );
  render(
    <QueryClientProvider client={new QueryClient()}>
      <ApiContext.Provider value={api}>
        <CompanyLibrary />
      </ApiContext.Provider>
    </QueryClientProvider>,
  );
  expect(
    screen.getByText(/stay separate from the current Tender/i),
  ).toBeInTheDocument();
});
