import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ApiContext, createApi } from "../api";
import { SourceBoqForm } from "./SourceBoqForm";

it("binds a replacement to its exact retired row even when row references repeat", async () => {
  const writes: unknown[] = [],
    onCreated = vi.fn();
  const excerpt = "Item 12 Concrete slab m3 28.50";
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async (address, init) => {
      if (init?.method === "POST") {
        writes.push(JSON.parse(String(init.body)));
        return Response.json({ id: "new-row", confirmed: false });
      }
      if (new URL(String(address)).pathname.endsWith("/search"))
        return Response.json({
          hits: [
            {
              id: "new-page",
              artifact_name: "Revised BOQ.pdf",
              locator: "page 7",
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
      return Response.json({
        artifact_name: "Revised BOQ.pdf",
        locator: "page 7",
      });
    },
  );
  render(
    <QueryClientProvider client={new QueryClient()}>
      <ApiContext.Provider value={api}>
        <SourceBoqForm
          tenderId="replacement-tender"
          replacement={{
            id: "old-row-page-7",
            row_reference: "Item 12",
            description: "Concrete slab",
            unit: "m3",
            supplied_quantity: "28.50",
          }}
          onSource={vi.fn()}
          onClose={vi.fn()}
          onCreated={onCreated}
        />
      </ApiContext.Provider>
    </QueryClientProvider>,
  );
  const user = userEvent.setup({ delay: null });
  await user.type(
    screen.getByPlaceholderText("Search imported source text…"),
    "Concrete",
  );
  await user.click(
    await screen.findByRole("button", {
      name: /Revised BOQ.pdf · page 7 Item 12/,
    }),
  );
  await user.type(screen.getByLabelText("Exact source excerpt"), excerpt);
  await user.click(
    screen.getByRole("button", { name: "Save BOQ row proposal" }),
  );
  await vi.waitFor(() =>
    expect(onCreated).toHaveBeenCalledWith({ id: "new-row", confirmed: false }),
  );
  expect(writes).toEqual([
    {
      source_id: "new-page",
      row_reference: "Item 12",
      source_excerpt: excerpt,
      description: "Concrete slab",
      unit: "m3",
      quantity: "28.50",
      replaces_item_id: "old-row-page-7",
    },
  ]);
});
