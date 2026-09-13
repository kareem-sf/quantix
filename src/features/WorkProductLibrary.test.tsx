import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { vi } from "vitest";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ApiContext, type Api, type Schema } from "../api";
import { WorkProductLibrary } from "./WorkProductLibrary";
import { WorkProductView } from "./WorkProductView";
import { useWorkProductRows } from "./useWorkProductRows";

const summary = {
  id: "version-2",
  product_id: "product-a",
  version: 2,
  kind: "chart",
  title: "Concrete output comparison",
  author: "staff-a",
  basis: "basis-a",
  status: "draft",
  dependency_state: "needs_review",
  review_reasons: ["source_revision_changed"],
  source_refs: ["source-a", "public_citation:citation-a"],
  method_refs: ["method-a"],
  public_citation_refs: ["public_citation:citation-a"],
  sha256: "a".repeat(64),
  row_count: 60,
  created_at: "2026-09-13T08:00:00Z",
} satisfies Schema<"WorkProductVersionSummary">;

it("refreshes a saved draft's history after a missed short run without losing the selected version", async () => {
  let latestVersion = 2;
  const get = vi.fn(async (path: string) => {
    if (path.includes("/rows?")) return { items: [], total: 0, missing: 0 };
    if (path.includes("?include_rows=false"))
      return {
        ...summary,
        kind: "note",
        sanitized_content: "Selected old version content",
        rows: [],
        view_schema: {},
        method_refs: [],
        source_refs: [],
      };
    return {
      items:
        latestVersion === 2
          ? [summary]
          : [
              {
                ...summary,
                id: "version-3",
                version: 3,
                title: "New saved draft",
              },
              summary,
            ],
      total: latestVersion - 1,
      next_offset: null,
    };
  });
  render(
    <QueryClientProvider client={new QueryClient()}>
      <ApiContext.Provider value={{ get } as unknown as Api}>
        <WorkProductLibrary
          tenderId="tender-a"
          focusedProductId="product-a"
          onSource={vi.fn()}
          onPublicCitation={vi.fn()}
        />
      </ApiContext.Provider>
    </QueryClientProvider>,
  );
  expect(await screen.findByText("Selected old version content")).toBeVisible();
  latestVersion = 3;
  await userEvent.click(
    screen.getByRole("button", { name: "Refresh draft and history" }),
  );
  expect(
    await screen.findByRole("button", { name: "Version 3 · New saved draft" }),
  ).toBeVisible();
  expect(screen.getByText("Selected old version content")).toBeVisible();
});

it("resolves an addressed product outside the first page and retains its history", async () => {
  const get = vi.fn(async (path: string) => {
    if (path.includes("/rows?")) return { items: [], total: 0, missing: 0 };
    if (path.includes("?include_rows=false"))
      return {
        ...summary,
        kind: "note",
        view_schema: {},
        rows: [],
        content: "Exact addressed draft",
        sanitized_content: "Exact addressed draft",
        method_refs: [],
        source_refs: [],
      };
    if (path.includes("/product-a/versions"))
      return { items: [summary], total: 1, next_offset: null };
    return { items: [], total: 0, next_offset: null };
  });
  render(
    <QueryClientProvider client={new QueryClient()}>
      <ApiContext.Provider value={{ get } as unknown as Api}>
        <WorkProductLibrary
          tenderId="tender-a"
          focusedProductId="product-a"
          onSource={vi.fn()}
          onPublicCitation={vi.fn()}
        />
      </ApiContext.Provider>
    </QueryClientProvider>,
  );
  expect(
    await screen.findByRole("region", { name: "Work product detail" }),
  ).toBeVisible();
  expect(get).toHaveBeenCalledWith(
    "/tenders/tender-a/work-products/product-a/versions?offset=0&limit=1",
    expect.anything(),
  );
  expect(
    await screen.findByRole("group", { name: "Version history" }),
  ).toBeVisible();
});

it("retries a failed list read without claiming the Tender has no drafts", async () => {
  const get = vi
    .fn()
    .mockRejectedValueOnce(new Error("Service temporarily unavailable"))
    .mockResolvedValue({ items: [summary], total: 1, next_offset: null });
  render(
    <ApiContext.Provider value={{ get } as unknown as Api}>
      <WorkProductLibrary
        tenderId="tender-a"
        onSource={vi.fn()}
        onPublicCitation={vi.fn()}
      />
    </ApiContext.Provider>,
  );
  expect(
    await screen.findByText("Service temporarily unavailable"),
  ).toBeVisible();
  expect(
    screen.queryByText("No draft work products yet"),
  ).not.toBeInTheDocument();
  await userEvent.click(
    screen.getByRole("button", { name: "Retry draft list" }),
  );
  expect(await screen.findByText(summary.title)).toBeVisible();
  expect(get).toHaveBeenCalledTimes(2);
});

it("does not turn a failed row read into an empty table", async () => {
  let rowReads = 0;
  const api = {
    get: vi.fn(async (path: string) => {
      if (path.includes("/rows?")) {
        if (++rowReads === 1) throw new Error("Row read interrupted");
        return { items: [{ Quantity: "15.00 m³" }], total: 1, missing: 0 };
      }
      if (path.includes("?include_rows=false"))
        return {
          ...summary,
          kind: "table",
          view_schema: {},
          rows: [],
          content: "Saved draft",
          sanitized_content: "Saved draft",
          method_refs: [],
          source_refs: [],
        };
      return { items: [summary], total: 1, next_offset: null };
    }),
  } as unknown as Api;
  render(
    <QueryClientProvider
      client={
        new QueryClient({ defaultOptions: { queries: { retry: false } } })
      }
    >
      <ApiContext.Provider value={api}>
        <WorkProductLibrary
          tenderId="tender-a"
          onSource={vi.fn()}
          onPublicCitation={vi.fn()}
        />
      </ApiContext.Provider>
    </QueryClientProvider>,
  );
  await userEvent.click(
    await screen.findByRole("button", { name: `Open ${summary.title}` }),
  );
  expect(await screen.findByText("Row read interrupted")).toBeVisible();
  expect(screen.queryByText("No table rows saved.")).not.toBeInTheDocument();
  await userEvent.click(
    screen.getByRole("button", { name: "Retry saved draft" }),
  );
  expect(await screen.findByText("15.00 m³")).toBeVisible();
});

it("refreshes a mounted product list when its Tender revision changes", async () => {
  const get = vi
    .fn()
    .mockResolvedValueOnce({ items: [summary], total: 1, next_offset: null })
    .mockResolvedValue({
      items: [
        {
          ...summary,
          id: "version-3",
          version: 3,
          title: "Revised concrete comparison",
        },
      ],
      total: 1,
      next_offset: null,
    });
  const api = { get } as unknown as Api;
  const view = (revision: number) => (
    <ApiContext.Provider value={api}>
      <WorkProductLibrary
        tenderId="tender-a"
        tenderRevision={revision}
        onSource={vi.fn()}
        onPublicCitation={vi.fn()}
      />
    </ApiContext.Provider>
  );
  const rendered = render(view(1));
  expect(await screen.findByText(summary.title)).toBeVisible();
  rendered.rerender(view(2));
  expect(await screen.findByText("Revised concrete comparison")).toBeVisible();
  expect(screen.queryByText(summary.title)).not.toBeInTheDocument();
});

it("opens immutable history, renders a bounded safe chart, and routes exact sources", async () => {
  const onSource = vi.fn();
  const onPublicCitation = vi.fn();
  const rows = Array.from({ length: 50 }, (_, index) => ({
    supplier: `Supplier ${index + 1}`,
    output: index + 1,
  }));
  const api = {
    get: vi.fn(async (path: string) => {
      if (path.endsWith("/work-products?offset=0&limit=50"))
        return { items: [summary], next_offset: null, total: 1 };
      if (path.endsWith("/work-products/product-a/versions?offset=0&limit=50"))
        return {
          items: [
            summary,
            {
              ...summary,
              id: "version-1",
              version: 1,
              title: "Earlier comparison",
            },
          ],
          next_offset: null,
          total: 2,
        };
      if (path.includes("/versions/2?include_rows=false"))
        return {
          ...summary,
          view_schema: { category_field: "supplier", value_field: "output" },
          rows: [],
          content: "<strong>Saved as plain text</strong>",
          sanitized_content: "<strong>Saved as plain text</strong>",
          executed_scripts: 0,
        };
      if (path.endsWith("/versions/2/rows?offset=0&limit=50"))
        return { items: rows, total: 60, missing: 0 };
      if (path.endsWith("/versions/2/rows?offset=50&limit=50"))
        return {
          items: Array.from({ length: 10 }, (_, index) => ({
            supplier: `Supplier ${index + 51}`,
            output: index + 51,
          })),
          total: 60,
          missing: 0,
        };
      throw new Error(`Unexpected GET ${path}`);
    }),
  } as unknown as Api;
  render(
    <QueryClientProvider
      client={
        new QueryClient({ defaultOptions: { queries: { retry: false } } })
      }
    >
      <ApiContext.Provider value={api}>
        <WorkProductLibrary
          tenderId="tender-a"
          onSource={onSource}
          onPublicCitation={onPublicCitation}
        />
      </ApiContext.Provider>
    </QueryClientProvider>,
  );

  expect(await screen.findByText("Concrete output comparison")).toBeVisible();
  expect(screen.getByText("Needs review")).toBeVisible();
  await userEvent.click(
    screen.getByRole("button", { name: "Open Concrete output comparison" }),
  );
  expect(
    await screen.findByRole("img", {
      name: "Concrete output comparison chart",
    }),
  ).toBeVisible();
  expect(screen.getByText("Showing 50 of 60 rows.")).toBeVisible();
  expect(
    screen.getByText("<strong>Saved as plain text</strong>"),
  ).toBeVisible();
  expect(document.querySelector("strong > strong")).not.toBeInTheDocument();

  await userEvent.click(
    screen.getByRole("button", { name: "Open source source-a" }),
  );
  expect(onSource).toHaveBeenCalledWith("source-a");
  await userEvent.click(
    screen.getByRole("button", { name: "Open public citation citation-a" }),
  );
  expect(onPublicCitation).toHaveBeenCalledWith("citation-a");

  const history = screen.getByRole("group", { name: "Version history" });
  expect(
    within(history).getByRole("button", {
      name: /Version 1.*Earlier comparison/,
    }),
  ).toBeVisible();
  await userEvent.click(screen.getByRole("button", { name: "Load more rows" }));
  expect(await screen.findByText("Showing 60 of 60 rows.")).toBeVisible();
  expect(
    screen.queryByRole("button", { name: "Load more rows" }),
  ).not.toBeInTheDocument();
  await waitFor(() => expect(api.get).toHaveBeenCalledTimes(5));
});

it("loads the fifty-first product and version without replacing the current page", async () => {
  const products = Array.from({ length: 50 }, (_, index) => ({
    ...summary,
    id: `product-version-${index}`,
    product_id: `product-${index}`,
    title: `Product ${index + 1}`,
  }));
  const versions = Array.from({ length: 50 }, (_, index) => ({
    ...summary,
    id: `history-${51 - index}`,
    product_id: "product-50",
    version: 51 - index,
    title: `Version title ${51 - index}`,
  }));
  const api = {
    get: vi.fn(async (path: string) => {
      if (path.endsWith("/work-products?offset=0&limit=50"))
        return { items: products, next_offset: 50, total: 51 };
      if (path.endsWith("/work-products?offset=50&limit=50"))
        return {
          items: [
            {
              ...summary,
              id: "product-version-50",
              product_id: "product-50",
              title: "Product 51",
              version: 51,
            },
          ],
          next_offset: null,
          total: 51,
        };
      if (path.endsWith("/work-products/product-50/versions?offset=0&limit=50"))
        return { items: versions, next_offset: 50, total: 51 };
      if (
        path.endsWith("/work-products/product-50/versions?offset=50&limit=50")
      )
        return {
          items: [
            {
              ...summary,
              id: "history-1",
              product_id: "product-50",
              version: 1,
              title: "First version",
            },
          ],
          next_offset: null,
          total: 51,
        };
      if (
        path.includes(
          "/work-products/product-50/versions/51?include_rows=false",
        )
      )
        return {
          ...summary,
          product_id: "product-50",
          version: 51,
          view_schema: {},
          rows: [],
          content: "",
          sanitized_content: "",
          executed_scripts: 0,
        };
      if (
        path.endsWith(
          "/work-products/product-50/versions/51/rows?offset=0&limit=50",
        )
      )
        return { items: [], total: 0, missing: 0 };
      throw new Error(`Unexpected GET ${path}`);
    }),
  } as unknown as Api;
  render(
    <QueryClientProvider
      client={
        new QueryClient({ defaultOptions: { queries: { retry: false } } })
      }
    >
      <ApiContext.Provider value={api}>
        <WorkProductLibrary
          tenderId="tender-a"
          onSource={vi.fn()}
          onPublicCitation={vi.fn()}
        />
      </ApiContext.Provider>
    </QueryClientProvider>,
  );

  await screen.findByText("Product 1");
  await userEvent.click(
    screen.getByRole("button", { name: "Load older draft work products" }),
  );
  expect(await screen.findByText("Product 51")).toBeVisible();
  expect(screen.getByText("Product 1")).toBeVisible();
  await userEvent.click(
    screen.getByRole("button", { name: "Open Product 51" }),
  );
  await screen.findByRole("group", { name: "Version history" });
  await userEvent.click(
    screen.getByRole("button", { name: "Load older versions" }),
  );
  expect(
    await screen.findByRole("button", { name: /Version 1.*First version/ }),
  ).toBeVisible();
  expect(
    screen.getByRole("button", { name: /Version 51.*Version title 51/ }),
  ).toBeVisible();
});

it("falls back to a table for negative charts and preserves omitted row fields as text", () => {
  const row = Object.fromEntries([
    ["item", "Credit"],
    ["value", -25],
    ...Array.from({ length: 21 }, (_, index) => [`extra_${index}`, index]),
    ["nested", { note: "kept as data" }],
  ]);
  render(
    <WorkProductView
      tenderId="tender-a"
      product={{
        ...summary,
        method_refs: [],
        view_schema: { category_field: "item", value_field: "value" },
        rows: [],
        content: "",
        sanitized_content: "",
        executed_scripts: 0,
      }}
      rows={{ items: [row], total: 1, missing: 0 }}
      onSource={vi.fn()}
      onPublicCitation={vi.fn()}
    />,
  );

  expect(screen.queryByRole("img")).not.toBeInTheDocument();
  expect(screen.getByRole("table")).toBeVisible();
  expect(screen.getByText("More row data (JSON)")).toBeVisible();
  expect(
    screen.getByText(/additional or unsupported fields/),
  ).toBeInTheDocument();
  expect(screen.getByText(/kept as data/)).toBeInTheDocument();
});

it("discards a late row page after switching the exact product version", async () => {
  let releaseOld!: (page: Schema<"WorkProductRowPage">) => void;
  const api = {
    get: vi.fn(async (path: string) => {
      if (path.endsWith("/v1?offset=0&limit=50"))
        return { items: [{ value: "v1-first" }], total: 2, missing: 0 };
      if (path.endsWith("/v1?offset=1&limit=50"))
        return new Promise<Schema<"WorkProductRowPage">>((resolve) => {
          releaseOld = resolve;
        });
      if (path.endsWith("/v2?offset=0&limit=50"))
        return { items: [{ value: "v2-current" }], total: 1, missing: 0 };
      throw new Error(`Unexpected GET ${path}`);
    }),
  } as unknown as Api;
  function RowHarness({ version }: { version: string }) {
    const rows = useWorkProductRows(`/rows/${version}`);
    return (
      <div>
        {rows.items.map((item) => (
          <span key={String(item.value)}>{String(item.value)}</span>
        ))}
        <button type="button" onClick={() => void rows.loadMore()}>
          Load row page
        </button>
      </div>
    );
  }
  const view = render(
    <ApiContext.Provider value={api}>
      <RowHarness version="v1" />
    </ApiContext.Provider>,
  );
  await screen.findByText("v1-first");
  await userEvent.click(screen.getByRole("button", { name: "Load row page" }));
  view.rerender(
    <ApiContext.Provider value={api}>
      <RowHarness version="v2" />
    </ApiContext.Provider>,
  );
  expect(await screen.findByText("v2-current")).toBeVisible();
  await act(async () =>
    releaseOld({ items: [{ value: "v1-late" }], total: 2, missing: 0 }),
  );
  expect(screen.queryByText("v1-late")).not.toBeInTheDocument();
  expect(screen.getByText("v2-current")).toBeVisible();
});
