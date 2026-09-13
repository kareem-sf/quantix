import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ApiContext, createApi } from "../api";
import { Watchers } from "./Watchers";

it("says a watch cannot send a quotation", async () => {
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async () =>
      Response.json({
        id: "watch-1",
        scope: "this Tender",
        trigger: "addendum",
        schedule: "0 * * * *",
        timezone: "UTC",
        stop_condition: "manual",
        budget: 10,
        notification_policy: "meaningful_change",
        state: "draft",
        fingerprint: "a".repeat(64),
        missed_checks: 0,
      }),
  );
  render(
    <QueryClientProvider client={new QueryClient()}>
      <ApiContext.Provider value={api}>
        <Watchers tenderId="one" />
      </ApiContext.Provider>
    </QueryClientProvider>,
  );
  expect(
    screen.getByText(/cannot send a quotation or release a bid/i),
  ).toBeInTheDocument();
  await userEvent.click(screen.getByRole("button", { name: "Watch for addenda" }));
  expect(
    await screen.findByText(/activate it after you review/i),
  ).toBeInTheDocument();
});
