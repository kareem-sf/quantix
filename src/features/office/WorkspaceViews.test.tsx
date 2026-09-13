import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ApiContext, createApi, type Schema } from "@/api";
import { WorkspaceDocuments } from "./WorkspaceViews";

vi.mock("../Measurements", () => ({
  Measurements: ({ artifactId }: { artifactId: string }) => (
    <section aria-label="Measurements">{artifactId}</section>
  ),
}));

it("resets per-source measurement state when switching from a drawing to a workbook", async () => {
  const user = userEvent.setup();
  const drawing: Schema<"Artifact"> = {
    id: "drawing",
    tender_id: "one",
    name: "Plan.pdf",
    relative_path: "Plan.pdf",
    kind: "pdf",
    version: 1,
    content_hash: "drawing-hash",
    size: 1,
    status: "extracted",
    area: "General",
    metadata: { page_count: 1 },
    warnings: [],
    is_current: true,
    created_at: "",
  };
  const workbook: Schema<"Artifact"> = {
    ...drawing,
    id: "workbook",
    name: "Quantities.xlsx",
    relative_path: "Quantities.xlsx",
    kind: "spreadsheet",
    content_hash: "workbook-hash",
    metadata: {
      sheets: [{ name: "BOQ", state: "visible", max_row: 1, max_column: 1 }],
    },
  };
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async (address) =>
      String(address).includes("/preview")
        ? Response.json(
            { detail: "Synthetic preview unavailable" },
            { status: 503 },
          )
        : Response.json([]),
  );
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  const tree = (artifact: Schema<"Artifact">) => (
    <QueryClientProvider client={client}>
      <ApiContext.Provider value={api}>
        <WorkspaceDocuments
          tenderId="one"
          artifacts={[drawing, workbook]}
          selection={{
            artifactId: artifact.id,
            artifact,
            version: artifact.version,
            contentHash: artifact.content_hash,
          }}
          onSource={() => {}}
          onCloseSource={() => {}}
          onImport={() => {}}
        />
      </ApiContext.Provider>
    </QueryClientProvider>
  );
  const view = render(tree(drawing));
  await user.click(
    await screen.findByRole("button", { name: "Measure drawing" }),
  );
  expect(
    screen.getByRole("region", { name: "Measurements" }),
  ).toHaveTextContent("drawing");
  view.rerender(tree(workbook));
  expect(
    screen.queryByRole("region", { name: "Measurements" }),
  ).not.toBeInTheDocument();
  expect(
    screen.getByRole("region", { name: "Source document" }),
  ).toHaveTextContent("Quantities.xlsx");
});
