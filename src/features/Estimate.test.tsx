import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ApiContext, createApi } from "../api";
import { Estimate } from "./Estimate";

it("keeps retired source rows actionable and requires a separate exclusion decision", async () => {
  const user = userEvent.setup({ delay: null }),
    onSource = vi.fn(),
    writes: unknown[] = [];
  const retired = {
    id: "old-row",
    description: "Old slab row",
    source_id: "old-page",
    artifact_id: "old-pdf",
    row_reference: "Item 12",
    current_artifact_id: "new-pdf",
    unit: "m3",
    supplied_quantity: "24.50",
  };
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async (address, init) => {
      if (init?.method === "POST") {
        expect(new URL(String(address)).pathname).toBe(
          "/api/tenders/retired-tender/estimate/source-rows/old-row/exclude",
        );
        writes.push(JSON.parse(String(init.body)));
        return Response.json(
          { detail: "Check the replacement source first." },
          { status: 409 },
        );
      }
      return Response.json({
        items: [],
        retired_source_rows: [retired],
        totals: [],
        complete: false,
        refresh_required: false,
        blocking_reasons: ["A source row needs review."],
        coverage_note: "Coverage needs review.",
      });
    },
  );
  render(
    <QueryClientProvider client={new QueryClient()}>
      <ApiContext.Provider value={api}>
        <Estimate
          tenderId="retired-tender"
          defaultCurrency="EGP"
          onSource={onSource}
        />
      </ApiContext.Provider>
    </QueryClientProvider>,
  );
  expect(await screen.findByText("Old slab row")).toBeVisible();
  await user.click(
    screen.getByRole("button", { name: "Inspect previous source" }),
  );
  expect(onSource).toHaveBeenCalledWith({ sourceId: "old-page" });
  await user.click(screen.getByRole("button", { name: "Add replacement row" }));
  expect(screen.getByLabelText("Row reference")).toHaveValue("Item 12");
  expect(screen.getByLabelText("Exact source excerpt")).toHaveValue("");
  await user.click(screen.getByRole("button", { name: "Cancel" }));
  await user.click(screen.getByRole("button", { name: "No longer required" }));
  await user.type(
    screen.getByLabelText("Reason this row is no longer required"),
    "The revised scope removes this slab.",
  );
  const save = screen.getByRole("button", { name: "Record exclusion" });
  expect(save).toBeDisabled();
  await user.click(
    screen.getByRole("checkbox", { name: /I checked the revised source/ }),
  );
  await user.click(save);
  expect(
    await screen.findByText("Check the replacement source first."),
  ).toBeVisible();
  expect(
    screen.getByLabelText("Reason this row is no longer required"),
  ).toHaveValue("The revised scope removes this slab.");
  expect(
    screen.getByRole("checkbox", { name: /I checked the revised source/ }),
  ).not.toBeChecked();
  expect(writes).toEqual([
    {
      engineer_confirmed: true,
      rationale: "The revised scope removes this slab.",
      current_artifact_id: "new-pdf",
    },
  ]);
}, 15000);

it("saves an exact source BOQ proposal and opens the returned unconfirmed row", async () => {
  const user = userEvent.setup({ delay: null }),
    onSource = vi.fn(),
    writes: unknown[] = [],
    decisions: unknown[] = [];
  const excerpt = "Item 12 Concrete slab m3 24.50";
  const item = {
    id: "saved-boq-row",
    tender_id: "source-tender",
    artifact_id: "pdf",
    source_id: "source-page",
    document: "BOQ.pdf",
    sheet: "",
    locator: "page 2",
    description: "Concrete slab",
    unit: "m3",
    unit_cell: null,
    quantity_cell: null,
    quantity_candidates: {},
    supplied_quantity: "24.50",
    effective_quantity: "24.50",
    quantity_basis: "supplied",
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
    row_reference: "Item 12",
    source_excerpt: excerpt,
    origin: "engineer",
    source_proposal: {
      source_id: "source-page",
      row_reference: "Item 12",
      source_excerpt: excerpt,
      description: "Concrete slab",
      unit: "m3",
      quantity: "24.50",
    },
  };
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async (address, init) => {
      const path = new URL(String(address)).pathname;
      if (init?.method === "PATCH") {
        decisions.push(JSON.parse(String(init.body)));
        return Response.json({ ...item, confirmed: true });
      }
      if (init?.method === "POST") {
        writes.push(JSON.parse(String(init.body)));
        expect(path).toBe("/api/tenders/source-tender/estimate/source-rows");
        return Response.json(item);
      }
      if (path.endsWith("/search"))
        return Response.json({
          hits: [
            {
              id: "source-page",
              artifact_name: "BOQ.pdf",
              locator: "page 2",
              text: excerpt,
              kind: "pdf",
            },
          ],
          requested_mode: "auto",
          actual_mode: "words",
          ranking_version: "rrf-1",
          coverage: { truncated: false, scanned: 1, ceiling: 2000 },
          limitations: [],
        });
      if (path.endsWith("/evidence/source-page"))
        return Response.json({
          artifact_name: "BOQ.pdf",
          locator: "page 2",
          text: excerpt,
        });
      return Response.json({
        items: [],
        totals: [],
        complete: false,
        refresh_required: false,
        blocking_reasons: [],
        coverage_note: "No rows saved.",
      });
    },
  );
  render(
    <QueryClientProvider client={new QueryClient()}>
      <ApiContext.Provider value={api}>
        <Estimate
          tenderId="source-tender"
          defaultCurrency="EGP"
          onSource={onSource}
        />
      </ApiContext.Provider>
    </QueryClientProvider>,
  );
  await user.click(
    await screen.findByRole("button", { name: "Add BOQ row from source" }),
  );
  await user.type(
    screen.getByPlaceholderText("Search imported source text…"),
    "Concrete",
  );
  await user.click(
    await screen.findByRole("button", { name: /BOQ.pdf · page 2 Item 12/ }),
  );
  await user.click(
    await screen.findByRole("button", { name: "BOQ.pdf · page 2" }),
  );
  expect(onSource).toHaveBeenCalledWith({ sourceId: "source-page" });
  await user.type(screen.getByLabelText("Row reference"), "Item 12");
  await user.type(screen.getByLabelText("Exact source excerpt"), excerpt);
  await user.type(screen.getByLabelText("Description"), "Concrete slab");
  await user.type(screen.getByLabelText("Unit"), "m3");
  await user.type(screen.getByLabelText("Supplied quantity"), "24.50");
  await user.click(
    screen.getByRole("button", { name: "Save BOQ row proposal" }),
  );
  expect(
    await screen.findByRole("dialog", { name: "Review estimate row" }),
  ).toBeVisible();
  expect(screen.getByText(excerpt)).toBeVisible();
  expect(screen.getByText("Item 12")).toBeVisible();
  expect(
    screen.getByRole("checkbox", { name: /I have checked this source row/ }),
  ).not.toBeChecked();
  expect(writes).toEqual([
    {
      source_id: "source-page",
      row_reference: "Item 12",
      source_excerpt: excerpt,
      description: "Concrete slab",
      unit: "m3",
      quantity: "24.50",
      replaces_item_id: null,
    },
  ]);
  expect(
    screen.queryByLabelText("Quantity source cell"),
  ).not.toBeInTheDocument();
  await user.type(
    screen.getByLabelText("Source review note"),
    "Checked exact cited row.",
  );
  expect(
    screen.getByRole("button", { name: "Confirm source row" }),
  ).toBeDisabled();
  await user.click(
    screen.getByRole("checkbox", { name: /I have checked this source row/ }),
  );
  await user.click(screen.getByRole("button", { name: "Confirm source row" }));
  expect(
    await screen.findByText("Source confirmation recorded."),
  ).toBeVisible();
  expect(decisions).toEqual([
    {
      engineer_confirmed: true,
      rationale: "Checked exact cited row.",
      confirm_source: true,
    },
  ]);
}, 15000);

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
