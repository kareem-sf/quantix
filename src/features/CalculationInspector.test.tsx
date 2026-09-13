import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { vi } from "vitest";
import { ApiContext, type Api, type Schema } from "../api";
import { CalculationInspector } from "./CalculationInspector";

it("loads an exact Tender calculation and renders its saved inputs as data", async () => {
  const record = {
    id: "calculation-a",
    tender_id: "tender-a",
    method_id: "product",
    method_version: "1",
    formula_hash: "a".repeat(64),
    typed_inputs: { quantity: "12.5", factor: "8" },
    units: { quantity: "m", factor: "m" },
    assumptions: ["Synthetic wastage excluded."],
    precision: "0.01",
    rounding: "HALF_UP",
    outputs: { product: "100.00", unit: "m²" },
    checks: [],
    basis_fingerprint: "b".repeat(64),
    status: "calculated",
    dimension_error: false,
    limitations: [],
    created_at: "2026-09-13T08:00:00Z",
  } satisfies Schema<"CalculationRecord">;
  const api = {
    get: vi.fn(async () => record),
  } as unknown as Api;
  render(
    <ApiContext.Provider value={api}>
      <CalculationInspector
        tenderId="tender-a"
        calculationId="calculation-a"
        label="Method reference 1"
      />
    </ApiContext.Provider>,
  );
  expect(api.get).not.toHaveBeenCalled();
  await userEvent.click(screen.getByText("Method reference 1"));

  expect(await screen.findByText("product · v1")).toBeVisible();
  expect(screen.getByText("Synthetic wastage excluded.")).toBeVisible();
  await userEvent.click(screen.getByText("Typed inputs"));
  expect(screen.getByText(/12\.5/)).toBeInTheDocument();
  expect(api.get).toHaveBeenCalledWith(
    "/tenders/tender-a/calculations/calculation-a",
  );
});
