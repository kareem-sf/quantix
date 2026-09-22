import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ApiContext, createApi, type Schema } from "../../api";
import { Estimate } from "../Estimate";
import { buildRows, formatNumber } from "./model";

function item(
  id: string,
  changes: Partial<Schema<"EstimateItem">>,
): Schema<"EstimateItem"> {
  return {
    id,
    tender_id: "one",
    artifact_id: "boq",
    source_id: `source-${id}`,
    document: "BOQ.xlsx",
    sheet: "Civil",
    locator: "row 12",
    description: "Cast concrete foundations",
    unit: "m3",
    unit_cell: "D12",
    quantity_cell: "E12",
    quantity_candidates: {},
    supplied_quantity: "1240",
    effective_quantity: "1240",
    quantity_basis: "supplied",
    confirmed: true,
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
    row_reference: "3.1",
    ...changes,
  };
}

function line(
  id: string,
  changes: Partial<Schema<"TakeoffLine">>,
): Schema<"TakeoffLine"> {
  return {
    id,
    tender_id: "one",
    run_id: "run",
    author: "Samir Haddad",
    description: "Pad footings concrete",
    location: "Grid A-C/1-3",
    unit: "m3",
    quantity: "1300",
    method: "schedule",
    working: "7 x 2.0 x 2.0 x 0.5",
    source_ids: [],
    boq_item_id: "footings",
    boq: {
      description: "Cast concrete foundations",
      unit: "m3",
      quantity: "1240",
    },
    comparison: "differs",
    difference: "60",
    difference_percent: "4.8",
    status: "proposed",
    review_note: "",
    is_current: true,
    created_at: "",
    updated_at: "",
    ...changes,
  };
}

const items = [
  item("footings", {}),
  item("blinding", {
    row_reference: "3.2",
    description: "Blinding concrete",
    effective_quantity: "86",
    unit_rate: "210.50",
    currency: "SAR",
    line_ex_vat: "18103.00",
  }),
  item("panel", {
    row_reference: "28.4",
    description: "Fire alarm control panel",
    unit: "no",
    effective_quantity: "1",
  }),
];

const takeoff = [
  line("differs", {}),
  line("missing", {
    description: "Precast manholes",
    unit: "nr",
    quantity: "3",
    boq_item_id: null,
    boq: null,
    comparison: "not_in_boq",
    difference: null,
    difference_percent: null,
  }),
];

const rateProposal = {
  id: "rate-a",
  tender_id: "one",
  item_id: "panel",
  payload: {
    unit_rate: "48500",
    currency: "SAR",
    tax_basis: "excluding_vat",
    vat_percent: "15",
    provenance: {
      basis: "observed",
      observed_on: "2026-09-01",
      location: "Riyadh",
      conditions: "Supply only.",
      source_url: null,
    },
  },
  basis: {},
  basis_fingerprint: "b",
  approved_basis_fingerprint: null,
  source_ids: [],
  run_id: null,
  status: "proposed",
  is_current: true,
  created_at: "2026-09-06T10:00:00Z",
} as unknown as Schema<"RateProposalRecord">;

function renderEstimate() {
  const posts: Array<{ path: string; body: unknown }> = [];
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async (address, options) => {
      const path = new URL(String(address)).pathname;
      if (options?.method === "POST") {
        posts.push({
          path,
          body: options.body ? JSON.parse(String(options.body)) : null,
        });
        return Response.json(
          path.endsWith("/messages") ? { outcome: "immediate" } : {},
        );
      }
      if (path.endsWith("/takeoff")) return Response.json(takeoff);
      if (path.endsWith("/rate-proposals"))
        return Response.json([rateProposal]);
      if (path.endsWith("/estimate"))
        return Response.json({
          tender_id: "one",
          items,
          totals: [
            {
              currency: "SAR",
              priced_subtotal_ex_vat: "18103.00",
              total_ex_vat: null,
              total_inc_vat: null,
            },
          ],
          complete: false,
          refresh_required: false,
          unpriced_count: 2,
          unconfirmed_count: 0,
          unknown_vat_count: 3,
          unresolved_quantity_count: 0,
          blocking_reasons: [],
          coverage_note: "",
        });
      throw new Error(`Unexpected request: ${path}`);
    },
  );
  render(
    <QueryClientProvider
      client={
        new QueryClient({ defaultOptions: { queries: { retry: false } } })
      }
    >
      <ApiContext.Provider value={api}>
        <Estimate tenderId="one" defaultCurrency="SAR" onSource={vi.fn()} />
      </ApiContext.Provider>
    </QueryClientProvider>,
  );
  return { posts, user: userEvent.setup() };
}

it("joins BOQ rows, drawing quantities and proposed rates into one table", async () => {
  const { user } = renderEstimate();
  const table = await screen.findByRole("table", { name: "BOQ" });
  expect(screen.queryByRole("tablist")).not.toBeInTheDocument();
  expect(screen.getByLabelText("Pricing state")).toHaveTextContent(
    "SAR 18,103.00 excluding VAT · 1 of 3 rows priced",
  );

  const footings = within(table)
    .getByText("Cast concrete foundations")
    .closest("tr")!;
  expect(within(footings).getByText("1,240 m3")).toBeInTheDocument();
  expect(
    await within(footings).findByText("Drawings 1,300 m3 (+4.8%)"),
  ).toBeInTheDocument();
  expect(within(footings).getByText("Waiting for you")).toBeInTheDocument();

  const panel = within(table)
    .getByText("Fire alarm control panel")
    .closest("tr")!;
  expect(within(panel).getByText("Proposed 48,500 SAR")).toBeInTheDocument();
  expect(
    within(panel).getByRole("button", { name: "Review" }),
  ).toBeInTheDocument();

  const blinding = within(table).getByText("Blinding concrete").closest("tr")!;
  expect(within(blinding).getByText("Priced")).toBeInTheDocument();
  expect(within(blinding).getByText("18,103.00")).toBeInTheDocument();

  const extra = screen.getByRole("region", {
    name: "On the drawings, not in the BOQ",
  });
  expect(within(extra).getByText("Precast manholes")).toBeInTheDocument();

  await user.click(screen.getByRole("button", { name: /Waiting for you/ }));
  expect(
    within(table).queryByText("Blinding concrete"),
  ).not.toBeInTheDocument();
  expect(
    within(table).getByText("Fire alarm control panel"),
  ).toBeInTheDocument();
});

it("accepts a drawing quantity from its row", async () => {
  const { posts, user } = renderEstimate();
  await user.click(
    await screen.findByRole("button", {
      name: "Accept the drawing quantity for Pad footings concrete",
    }),
  );
  await waitFor(() => expect(posts).toHaveLength(1));
  expect(posts[0]).toEqual({
    path: "/api/tenders/one/takeoff/differs/review",
    body: { decision: "accepted", note: "" },
  });
});

it("asks the Tender Manager for a takeoff from the drawings", async () => {
  const { posts, user } = renderEstimate();
  await user.click(
    await screen.findByRole("button", { name: "Take off from drawings" }),
  );
  await waitFor(() => expect(posts).toHaveLength(1));
  expect(posts[0].path).toBe("/api/tenders/one/messages");
  expect(String((posts[0].body as { content: string }).content)).toMatch(
    /quantity takeoff/,
  );
  expect(await screen.findByText(/Sent to the Tender Manager/)).toBeVisible();
});

it("keeps lines for rows missing from the BOQ and groups numbers without rounding", () => {
  const { rows, unmatched } = buildRows(items, takeoff, "not a list");
  expect(rows.map((row) => row.status)).toEqual([
    "waiting",
    "priced",
    "unpriced",
  ]);
  expect(unmatched.map((entry) => entry.id)).toEqual(["missing"]);
  expect(formatNumber("1234567.500")).toBe("1,234,567.500");
  expect(formatNumber(null)).toBeNull();
});
