import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ApiContext, createApi, type Schema } from "../api";
import { OutputWorkingRecords } from "./OutputWorkingRecords";
it("links an incomplete BOQ working row by its exact persisted identity", async () => {
  const repair = vi.fn();
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async () =>
      Response.json({
        items: [
          {
            id: "item/specific",
            description: "Foundation concrete",
            confirmed: false,
            unit_rate: null,
            effective_quantity: "12",
            tax_basis: "unknown",
            vat_percent: null,
          },
        ],
        refresh_required: false,
      }),
  );
  render(
    <QueryClientProvider client={new QueryClient()}>
      <ApiContext.Provider value={api}>
        <OutputWorkingRecords
          tenderId="one"
          output={{ kind: "boq_xlsx" } as Schema<"OutputRecord">}
          onRepair={repair}
        />
      </ApiContext.Provider>
    </QueryClientProvider>,
  );
  const user = userEvent.setup({ delay: null });
  await user.click(
    await screen.findByRole("button", { name: "Foundation concrete" }),
  );
  expect(repair).toHaveBeenCalledWith(
    "/tenders/one/estimate?view=boq&record=item%2Fspecific",
  );
});
