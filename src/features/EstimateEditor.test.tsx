import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ApiContext, createApi, type Schema } from "../api";
import { EstimateEditor } from "./EstimateEditor";

it("allows explicit source review when an approved measured quantity resolves an unreadable supplied quantity", async () => {
  const writes: Record<string, unknown>[] = [];
  const item: Schema<"EstimateItem"> = {
    id: "row",
    tender_id: "one",
    artifact_id: "file",
    source_id: "source",
    document: "BOQ.xlsx",
    sheet: "BOQ",
    locator: "row 3",
    description: "Measured boundary",
    unit: "m",
    unit_cell: "B3",
    quantity_cell: null,
    quantity_candidates: {},
    supplied_quantity: null,
    effective_quantity: "12",
    quantity_basis: "approved_measurement",
    confirmed: false,
    issues: ["Supplied quantity could not be read."],
    unit_rate: null,
    rate_ex_vat: null,
    components: [],
    currency: null,
    tax_basis: "unknown",
    vat_percent: null,
    provenance: null,
    line_ex_vat: null,
    line_inc_vat: null,
    quantity_proposals: [],
  };
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async (_address, init) => {
      if (init?.method === "PATCH") {
        writes.push(JSON.parse(String(init.body)));
        return Response.json(item);
      }
      return Response.json({ artifact_name: "BOQ.xlsx", locator: "row 3" });
    },
  );
  const user = userEvent.setup();
  render(
    <QueryClientProvider client={new QueryClient()}>
      <ApiContext.Provider value={api}>
        <EstimateEditor
          tenderId="one"
          item={item}
          defaultCurrency="EGP"
          onSource={() => {}}
          onClose={() => {}}
        />
      </ApiContext.Provider>
    </QueryClientProvider>,
  );
  await user.type(
    screen.getByLabelText("Source review note"),
    "Description and unit checked; approved measurement supplies the quantity",
  );
  await user.click(
    screen.getByRole("checkbox", { name: /I have checked this source row/ }),
  );
  expect(
    screen.getByRole("button", { name: "Confirm source row" }),
  ).toBeEnabled();
  await user.click(screen.getByRole("button", { name: "Confirm source row" }));
  await waitFor(() => expect(writes).toHaveLength(1));
  expect(writes[0]).toMatchObject({
    confirm_source: true,
    engineer_confirmed: true,
  });
  expect(writes[0]).not.toHaveProperty("quantity_cell");
  expect(
    screen.getByText("Supplied quantity could not be read."),
  ).toBeInTheDocument();
});
