import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ApiContext, createApi } from "../api";
import { Estimate } from "./Estimate";

it("keeps incomplete pricing visible when routine source refresh fails", async () => {
  const user = userEvent.setup();
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async (url, init) => {
      if (init?.method === "POST") {
        expect(init.body).toBeUndefined();
        return new Response(
          JSON.stringify({ detail: "Source refresh is unavailable." }),
          { status: 400 },
        );
      }
      return new Response(
        JSON.stringify(
          String(url).endsWith("/outputs") ||
            String(url).endsWith("/rate-proposals")
            ? []
            : {
                tender_id: "one",
                items: [],
                totals: [],
                complete: false,
                refresh_required: true,
                unpriced_count: 1,
                unconfirmed_count: 1,
                unknown_vat_count: 1,
                unresolved_quantity_count: 0,
                blocking_reasons: ["One source row needs confirmation."],
                coverage_note:
                  "Candidate identification does not establish complete BOQ coverage.",
              },
        ),
      );
    },
  );
  render(
    <QueryClientProvider client={new QueryClient()}>
      <ApiContext.Provider value={api}>
        <Estimate tenderId="one" defaultCurrency="EGP" onSource={() => {}} />
      </ApiContext.Provider>
    </QueryClientProvider>,
  );
  expect(
    await screen.findByText("One source row needs confirmation."),
  ).toBeInTheDocument();
  expect(screen.queryByText("Pricing complete")).not.toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "Refresh source rows" }));
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "Source refresh is unavailable.",
  );
  expect(
    screen.getByText("One source row needs confirmation."),
  ).toBeInTheDocument();
});
