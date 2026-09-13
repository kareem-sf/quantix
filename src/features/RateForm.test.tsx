import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ApiContext, createApi, type Schema } from "../api";
import { RateForm } from "./RateForm";

it("preserves decimal input and a failed rate decision without confirming source quantities", async () => {
  const item: Schema<"EstimateItem"> = {
    id: "row",
    tender_id: "one",
    artifact_id: "file",
    source_id: "source",
    document: "BOQ.xlsx",
    sheet: "BOQ",
    locator: "A2:E2",
    description: "Concrete",
    unit: "m3",
    unit_cell: "D2",
    quantity_cell: "E2",
    quantity_candidates: { E2: "12" },
    supplied_quantity: "12",
    effective_quantity: "12",
    quantity_basis: "supplied_boq",
    confirmed: false,
    issues: [],
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
    async (_url, init) => {
      const submitted = JSON.parse(String(init?.body));
      expect(submitted).toMatchObject({
        unit_rate: "123.450000",
        confirm_source: false,
        engineer_confirmed: true,
        vat_percent: null,
        tax_basis: "unknown",
        rationale: "Budget rate pending supplier quotation",
      });
      return new Response(
        JSON.stringify({ detail: "Rate could not be saved." }),
        { status: 400 },
      );
    },
  );
  const user = userEvent.setup();
  render(
    <QueryClientProvider client={new QueryClient()}>
      <ApiContext.Provider value={api}>
        <RateForm item={item} tenderId="one" defaultCurrency="EGP" />
      </ApiContext.Provider>
    </QueryClientProvider>,
  );
  await user.type(screen.getByLabelText("Unit rate"), "123.450000");
  await user.type(screen.getByLabelText("Source date"), "2026-09-01");
  await user.type(screen.getByLabelText("Location or market"), "Cairo");
  await user.type(
    screen.getByLabelText("Rate conditions"),
    "Supply only, delivery excluded",
  );
  await user.type(
    screen.getByLabelText("Rate decision note"),
    "Budget rate pending supplier quotation",
  );
  await user.click(screen.getByRole("checkbox"));
  await user.click(screen.getByRole("button", { name: "Save rate decision" }));
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "Rate could not be saved.",
  );
  expect(screen.getByLabelText("Unit rate")).toHaveValue("123.450000");
  expect(screen.queryByText("Rate decision recorded.")).not.toBeInTheDocument();
});
