import { render, screen } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ApiContext, createApi } from "../api";
import { TenderBrief } from "./TenderBrief";

it("keeps an unknown measurement method visible", async () => {
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async () =>
      Response.json({
        tender_id: "one",
        revision: 1,
        measurement_method: null,
        currencies: [],
        timezone: null,
      }),
  );
  render(
    <QueryClientProvider client={new QueryClient()}>
      <ApiContext.Provider value={api}>
        <TenderBrief tenderId="one" />
      </ApiContext.Provider>
    </QueryClientProvider>,
  );
  expect(await screen.findAllByText("Not yet confirmed")).not.toHaveLength(0);
});
