import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ApiContext, createApi, type Schema } from "../api";
import { SourceDrawer, type SourceSelection } from "./Sources";

const workbook = {
  id: "workbook-v2",
  tender_id: "one",
  name: "Quantities.xlsx",
  relative_path: "BOQ/Quantities.xlsx",
  kind: "spreadsheet",
  version: 2,
  content_hash: "workbook-hash-v2",
  size: 1,
  status: "extracted",
  area: "East",
  metadata: {
    sheets: [
      { name: "Main", state: "visible", max_row: 60, max_column: 4 },
      { name: "Later works", state: "hidden", max_row: 65, max_column: 4 },
    ],
  },
  warnings: [],
  is_current: false,
  created_at: "",
} satisfies Schema<"Artifact">;

function workbookRow(sheet: string, row: number): Schema<"Evidence"> {
  return {
    id: `${sheet.replaceAll(" ", "-")}-${row}`,
    artifact_id: workbook.id,
    artifact_name: workbook.name,
    relative_path: workbook.relative_path,
    locator: `sheet:${sheet}!A${row}:D${row}`,
    sheet,
    cell_range: `A${row}:D${row}`,
    page: null,
    text: `${sheet} quantity row ${row}`,
    kind: "spreadsheet_row",
    metadata: {
      cells: [
        { coordinate: `B${row}`, value: 12, formula: null, cached_value: null },
      ],
    },
    score: 0,
  };
}

function renderWorkbook({
  artifact = workbook,
  selection = { artifactId: workbook.id },
  source,
  evidence,
  standalone = false,
  presentation = "modal",
}: {
  artifact?: Schema<"Artifact">;
  selection?: SourceSelection;
  source?: Schema<"Evidence">;
  evidence: (query: URLSearchParams) => Response;
  standalone?: boolean;
  presentation?: "modal" | "inline";
}) {
  const reads: URL[] = [];
  const changes: SourceSelection[] = [];
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async (address, init) => {
      expect(init?.method).toBe("GET");
      const url = new URL(String(address));
      reads.push(url);
      if (url.pathname.endsWith("/evidence")) return evidence(url.searchParams);
      if (source && url.pathname.endsWith(`/evidence/${source.id}`))
        return Response.json(source);
      if (url.pathname.endsWith(`/artifacts/${artifact.id}`))
        return Response.json(artifact);
      throw new Error(`Unexpected source request: ${url.pathname}`);
    },
  );
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  const tree = (next: SourceSelection) => (
    <QueryClientProvider client={client}>
      <ApiContext.Provider value={api}>
        <SourceDrawer
          tenderId="one"
          selection={next}
          artifacts={[artifact]}
          presentation={presentation}
          onClose={() => {}}
          onSelectionChange={
            standalone ? undefined : (value) => changes.push(value)
          }
        />
      </ApiContext.Provider>
    </QueryClientProvider>
  );
  const view = render(tree(selection));
  return {
    user: userEvent.setup(),
    reads,
    changes,
    rerenderSelection: (next: SourceSelection) => view.rerender(tree(next)),
  };
}

it("copies the full hash of the displayed source version", async () => {
  const { user } = renderWorkbook({
    evidence: () => Response.json([workbookRow("Main", 1)]),
  });
  const write = vi.spyOn(navigator.clipboard, "writeText").mockResolvedValue();
  await user.click(await screen.findByRole("button", { name: "Copy Hash" }));
  expect(write).toHaveBeenCalledWith(workbook.content_hash);
  expect(screen.getByRole("button", { name: "Copied" })).toBeInTheDocument();
});

it("renders the source viewer inline without modal focus trapping", async () => {
  const { user } = renderWorkbook({
    presentation: "inline",
    evidence: () => Response.json([workbookRow("Main", 1)]),
  });
  expect(
    await screen.findByRole("region", { name: "Source document" }),
  ).toBeInTheDocument();
  expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  expect(screen.queryByTestId("source-modal-backdrop")).not.toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "Close source viewer" }));
});

it("opens recorded worksheets absent from the first evidence page without scanning rows", async () => {
  const artifact = {
    ...workbook,
    metadata: {
      sheets: [
        null,
        "Unrecorded string",
        {},
        { name: 123 },
        ...workbook.metadata.sheets,
        { name: "Later works" },
      ],
    },
  };
  const { user, reads } = renderWorkbook({
    artifact,
    standalone: true,
    evidence: (query) =>
      Response.json(
        query.get("sheet") === "Later works"
          ? [workbookRow("Later works", 1)]
          : Array.from({ length: 30 }, (_, i) => workbookRow("Main", i + 1)),
      ),
  });
  expect(await screen.findByText("Main quantity row 1")).toBeInTheDocument();
  const sheet = screen.getByRole("combobox", { name: "Sheet" });
  expect(within(sheet).getAllByRole("option")).toHaveLength(3);
  await user.selectOptions(sheet, "Later works");
  await user.click(screen.getByRole("button", { name: "Apply" }));
  expect(
    await screen.findByText("Later works quantity row 1"),
  ).toBeInTheDocument();
  expect(screen.queryByText("Main quantity row 1")).not.toBeInTheDocument();
  expect(
    reads.filter((url) => url.pathname.endsWith("/evidence")),
  ).toHaveLength(2);
  expect(reads.at(-1)?.searchParams.get("sheet")).toBe("Later works");
  expect(reads.at(-1)?.searchParams.get("offset")).toBe("0");
});

it("paginates within an applied cell range and resets to the first rows when the sheet changes", async () => {
  const { user, reads, changes } = renderWorkbook({
    selection: {
      artifactId: workbook.id,
      version: 2,
      contentHash: workbook.content_hash,
      origin: "/tenders/one/documents?area=East",
    },
    evidence: (query) => {
      const sheet = query.get("sheet");
      if (sheet === "Later works" && query.get("cell_range") === "A31:D61")
        return Response.json(
          query.get("offset") === "30"
            ? [workbookRow(sheet, 61)]
            : Array.from({ length: 30 }, (_, i) => workbookRow(sheet, i + 31)),
        );
      if (sheet === "Main") return Response.json([workbookRow(sheet, 31)]);
      return Response.json([workbookRow("Main", 1)]);
    },
  });
  await user.selectOptions(
    screen.getByRole("combobox", { name: "Sheet" }),
    "Later works",
  );
  await user.type(
    screen.getByRole("textbox", { name: "Cell range" }),
    "A31:D61",
  );
  await user.click(screen.getByRole("button", { name: "Apply" }));
  expect(
    await screen.findByText("Later works quantity row 31"),
  ).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "Next sources" }));
  expect(
    await screen.findByText("Later works quantity row 61"),
  ).toBeInTheDocument();
  expect(reads.at(-1)?.searchParams.get("offset")).toBe("30");
  expect(reads.at(-1)?.searchParams.get("sheet")).toBe("Later works");
  expect(reads.at(-1)?.searchParams.get("cell_range")).toBe("A31:D61");
  await user.selectOptions(
    screen.getByRole("combobox", { name: "Sheet" }),
    "Main",
  );
  await user.click(screen.getByRole("button", { name: "Apply" }));
  expect(await screen.findByText("Main quantity row 31")).toBeInTheDocument();
  expect(reads.at(-1)?.searchParams.get("offset")).toBe("0");
  expect(changes.at(-1)).toMatchObject({
    artifactId: workbook.id,
    version: 2,
    contentHash: workbook.content_hash,
    sheet: "Main",
    cellRange: "A31:D61",
    origin: "/tenders/one/documents?area=East",
  });
});

it("clears a citation range to browse its whole sheet without repeating the cited row", async () => {
  const source = workbookRow("Later works", 31);
  const { user, changes, reads } = renderWorkbook({
    selection: {
      sourceId: source.id,
      artifactId: workbook.id,
      version: 2,
      contentHash: workbook.content_hash,
      origin: "/tenders/one/work?view=decisions&record=finding-1",
    },
    source,
    evidence: (query) => {
      expect(query.get("sheet")).toBe("Later works");
      expect(query.has("cell_range")).toBe(false);
      return Response.json([workbookRow("Later works", 1), source]);
    },
  });
  expect(await screen.findByText(source.text)).toBeInTheDocument();
  expect(screen.getByRole("textbox", { name: "Cell range" })).toHaveValue(
    "A31:D31",
  );
  await user.click(screen.getByRole("button", { name: "Clear range" }));
  expect(
    await screen.findByText("Later works quantity row 1"),
  ).toBeInTheDocument();
  expect(screen.getAllByText(source.text)).toHaveLength(1);
  expect(changes.at(-1)).toMatchObject({
    artifactId: workbook.id,
    version: 2,
    contentHash: workbook.content_hash,
    sheet: "Later works",
    origin: "/tenders/one/work?view=decisions&record=finding-1",
  });
  expect(changes.at(-1)).not.toHaveProperty("sourceId");
  expect(changes.at(-1)?.cellRange).toBeUndefined();
  expect(
    reads.filter((url) => url.pathname.endsWith("/evidence")),
  ).toHaveLength(1);
});

it("restores the URL worksheet and range after local navigation while keeping its return context", async () => {
  const original: SourceSelection = {
    artifactId: workbook.id,
    version: 2,
    contentHash: workbook.content_hash,
    sheet: "Later works",
    cellRange: "B31:D31",
    origin: "/tenders/one/estimate?view=boq&record=item-1",
  };
  const { user, rerenderSelection, changes, reads } = renderWorkbook({
    selection: original,
    evidence: (query) =>
      Response.json([
        workbookRow(
          query.get("sheet") ?? "Main",
          query.get("cell_range") === "B31:D31" ? 31 : 1,
        ),
      ]),
  });
  expect(
    await screen.findByText("Later works quantity row 31"),
  ).toBeInTheDocument();
  expect(reads.at(-1)?.searchParams.get("cell_range")).toBe("B31:D31");
  await user.click(screen.getByRole("button", { name: "Clear range" }));
  expect(
    await screen.findByText("Later works quantity row 1"),
  ).toBeInTheDocument();
  rerenderSelection(changes.at(-1)!);
  rerenderSelection(original);
  expect(
    await screen.findByText("Later works quantity row 31"),
  ).toBeInTheDocument();
  expect(screen.getByRole("textbox", { name: "Cell range" })).toHaveValue(
    "B31:D31",
  );
  expect(screen.getByRole("combobox", { name: "Sheet" })).toHaveValue(
    "Later works",
  );
  expect(changes.at(-1)?.origin).toBe(original.origin);
});

it("keeps a rejected range editable and displays the actual service error beside the range field", async () => {
  const { user } = renderWorkbook({
    evidence: (query) =>
      query.has("cell_range")
        ? Response.json(
            { detail: "Enter a valid cell range such as A1:D20." },
            { status: 409 },
          )
        : Response.json([workbookRow("Main", 1)]),
  });
  await user.type(
    screen.getByRole("textbox", { name: "Cell range" }),
    "D20:A1",
  );
  await user.click(screen.getByRole("button", { name: "Apply" }));
  const navigation = screen.getByRole("form", { name: "Worksheet navigation" });
  expect(await within(navigation).findByRole("alert")).toHaveTextContent(
    "Enter a valid cell range such as A1:D20.",
  );
  expect(screen.getByRole("textbox", { name: "Cell range" })).toHaveValue(
    "D20:A1",
  );
  await user.click(screen.getByRole("button", { name: "Clear range" }));
  expect(await screen.findByText("Main quantity row 1")).toBeInTheDocument();
  expect(within(navigation).queryByRole("alert")).not.toBeInTheDocument();
});

it("opens drawing measurements for the exact source and returns to the preserved source view without saving", async () => {
  vi.stubGlobal(
    "URL",
    class extends URL {
      static createObjectURL() {
        return "blob:preview";
      }
      static revokeObjectURL() {}
    },
  );
  const reads: string[] = [],
    writes: string[] = [];
  const artifact = {
    id: "drawing",
    tender_id: "one",
    name: "Plan.pdf",
    relative_path: "Drawings/Plan.pdf",
    kind: "pdf",
    version: 3,
    content_hash: "hash-v3",
    size: 1,
    status: "extracted",
    area: "",
    metadata: { page_count: 12 },
    warnings: [],
    is_current: true,
    created_at: "",
  } satisfies Schema<"Artifact">;
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async (address, init) => {
      const url = new URL(String(address));
      reads.push(url.pathname + url.search);
      if (init?.method !== "GET") writes.push(url.pathname);
      if (url.pathname.endsWith("/preview"))
        return new Response(new Blob(["image"], { type: "image/png" }));
      if (url.pathname.endsWith("/measurement-page"))
        return Response.json({
          artifact_id: "drawing",
          name: "Plan.pdf",
          relative_path: artifact.relative_path,
          page: Number(url.searchParams.get("page")),
          page_count: 12,
          page_size: [200, 100],
          version: 3,
          content_hash: "hash-v3",
          is_current: true,
        });
      return Response.json([]);
    },
  );
  try {
    const user = userEvent.setup();
    render(
      <QueryClientProvider client={new QueryClient()}>
        <ApiContext.Provider value={api}>
          <SourceDrawer
            tenderId="one"
            selection={{
              artifactId: "drawing",
              page: 12,
              version: 3,
              contentHash: "hash-v3",
            }}
            artifacts={[artifact]}
            onClose={() => {}}
          />
        </ApiContext.Provider>
      </QueryClientProvider>,
    );
    await user.click(
      await screen.findByRole("button", { name: "Measure drawing" }),
    );
    expect(
      await screen.findByRole("heading", { name: "Drawing measurements" }),
    ).toBeInTheDocument();
    expect(
      reads.some((path) =>
        path.endsWith(
          "/tenders/one/artifacts/drawing/measurement-page?page=12",
        ),
      ),
    ).toBe(true);
    await user.click(screen.getByRole("button", { name: "Back to source" }));
    expect(
      screen.getByRole("heading", { name: "Source text" }),
    ).toBeInTheDocument();
    expect(screen.getByText("Page 12 of 12")).toBeInTheDocument();
    expect(screen.getByRole("textbox", { name: "Page number" })).toHaveValue(
      "12",
    );
    expect(writes).toHaveLength(0);
  } finally {
    vi.unstubAllGlobals();
  }
});

it("loads a historical citation artifact and downloads its original through the authenticated route", async () => {
  vi.stubGlobal(
    "URL",
    class extends URL {
      static createObjectURL() {
        return "blob:original";
      }
      static revokeObjectURL() {}
    },
  );
  const downloads: string[] = [],
    reads: string[] = [];
  const click = vi
    .spyOn(HTMLAnchorElement.prototype, "click")
    .mockImplementation(function (this: HTMLAnchorElement) {
      downloads.push(this.download);
    });
  const api = createApi(
    { base_url: "http://localhost/api", token: "source-token" },
    async (address, init) => {
      const path = new URL(String(address)).pathname;
      reads.push(path);
      expect(new Headers(init?.headers).get("Authorization")).toBe(
        "Bearer source-token",
      );
      if (path.endsWith("/original"))
        return new Response(
          new Blob(["original PDF"], { type: "application/pdf" }),
        );
      if (path.endsWith("/preview"))
        return new Response(new Blob(["PNG"], { type: "image/png" }));
      if (path.endsWith("/artifacts/old"))
        return Response.json({
          id: "old",
          tender_id: "one",
          name: "Previous Plan.pdf",
          relative_path: "Drawings/Previous Plan.pdf",
          kind: "pdf",
          version: 1,
          is_current: false,
          metadata: {},
          warnings: [],
        });
      return Response.json({
        id: "old-source",
        artifact_id: "old",
        artifact_name: "Previous Plan.pdf",
        relative_path: "Drawings/Previous Plan.pdf",
        locator: "page 2",
        page: 2,
        text: "Earlier drawing note",
        metadata: {},
      });
    },
  );
  try {
    const user = userEvent.setup();
    render(
      <QueryClientProvider client={new QueryClient()}>
        <ApiContext.Provider value={api}>
          <SourceDrawer
            tenderId="one"
            selection={{ sourceId: "old-source" }}
            artifacts={[]}
            onClose={() => {}}
          />
        </ApiContext.Provider>
      </QueryClientProvider>,
    );
    await user.click(
      await screen.findByRole("button", { name: "Download original" }),
    );
    await waitFor(() => expect(downloads).toEqual(["Previous Plan.pdf"]));
    expect(reads).toContain("/api/tenders/one/artifacts/old");
    expect(reads).toContain("/api/tenders/one/artifacts/old/preview");
    expect(reads).toContain("/api/tenders/one/artifacts/old/original");
  } finally {
    click.mockRestore();
    vi.unstubAllGlobals();
  }
});

it("bounds PDF navigation, supports a page jump, and keeps exact source context", async () => {
  vi.stubGlobal(
    "URL",
    class extends URL {
      static createObjectURL() {
        return "blob:preview";
      }
      static revokeObjectURL() {}
    },
  );
  const artifact = {
    id: "drawing",
    tender_id: "one",
    name: "Plan.pdf",
    relative_path: "Drawings/Plan.pdf",
    kind: "pdf",
    version: 3,
    content_hash: "hash-v3",
    size: 1,
    status: "extracted",
    area: "East",
    metadata: { page_count: 12 },
    warnings: [],
    is_current: true,
    created_at: "",
  } satisfies Schema<"Artifact">;
  const reads: string[] = [];
  const changes: unknown[] = [];
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async (address) => {
      const url = new URL(String(address));
      reads.push(url.pathname + url.search);
      if (url.pathname.endsWith("/preview"))
        return new Response(new Blob(["image"], { type: "image/png" }));
      return Response.json({
        id: "source",
        artifact_id: "drawing",
        artifact_name: "Plan.pdf",
        relative_path: artifact.relative_path,
        locator: "Sheet A · A1:B4",
        page: 12,
        sheet: "Sheet A",
        cell_range: "A1:B4",
        text: "Grid reference",
        kind: "text",
        metadata: {},
        score: 0,
      });
    },
  );
  try {
    const user = userEvent.setup();
    render(
      <QueryClientProvider client={new QueryClient()}>
        <ApiContext.Provider value={api}>
          <SourceDrawer
            tenderId="one"
            selection={{
              sourceId: "source",
              artifactId: "drawing",
              version: 3,
              page: 12,
              sheet: "Sheet A",
              cellRange: "A1:B4",
              origin: "/tenders/one/work",
            }}
            artifacts={[artifact]}
            onSelectionChange={(selection) => changes.push(selection)}
            onClose={() => {}}
          />
        </ApiContext.Provider>
      </QueryClientProvider>,
    );
    expect(await screen.findByText("Page 12 of 12")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Next page" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Previous page" })).toBeEnabled();
    await user.clear(screen.getByLabelText("Page number"));
    await user.type(screen.getByLabelText("Page number"), "2");
    await user.click(screen.getByRole("button", { name: "Go to page" }));
    expect(await screen.findByText("Page 2 of 12")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Hide preview" }));
    expect(
      screen.queryByRole("img", { name: "Plan.pdf, page 2" }),
    ).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Preview" })).toHaveAttribute(
      "aria-expanded",
      "false",
    );
    await user.click(screen.getByRole("button", { name: "Preview" }));
    expect(
      await screen.findByRole("img", { name: "Plan.pdf, page 2" }),
    ).toBeVisible();
    expect(screen.getByLabelText("Page number")).toHaveValue("2");
    expect(reads).toContain(
      "/api/tenders/one/artifacts/drawing/preview?page=2",
    );
    expect(changes.at(-1)).toMatchObject({
      sourceId: "source",
      artifactId: "drawing",
      version: 3,
      page: 2,
      sheet: "Sheet A",
      cellRange: "A1:B4",
      origin: "/tenders/one/work",
    });
  } finally {
    vi.unstubAllGlobals();
  }
});

it("explains an out-of-range URL page and clamps it to the available range", async () => {
  vi.stubGlobal(
    "URL",
    class extends URL {
      static createObjectURL() {
        return "blob:preview";
      }
      static revokeObjectURL() {}
    },
  );
  const artifact = {
    id: "drawing",
    tender_id: "one",
    name: "Plan.pdf",
    relative_path: "Plan.pdf",
    kind: "pdf",
    version: 1,
    content_hash: "hash",
    size: 1,
    status: "extracted",
    area: "",
    metadata: { page_count: 12 },
    warnings: [],
    is_current: true,
    created_at: "",
  } satisfies Schema<"Artifact">;
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async (address) => {
      const path = new URL(String(address)).pathname;
      if (path.endsWith("/preview"))
        return new Response(new Blob(["image"], { type: "image/png" }));
      return Response.json({
        id: "source",
        artifact_id: "drawing",
        artifact_name: "Plan.pdf",
        relative_path: "Plan.pdf",
        locator: "page 12",
        page: 12,
        text: "Drawing",
        kind: "text",
        metadata: {},
        score: 0,
      });
    },
  );
  try {
    render(
      <QueryClientProvider client={new QueryClient()}>
        <ApiContext.Provider value={api}>
          <SourceDrawer
            tenderId="one"
            selection={{ sourceId: "source", artifactId: "drawing", page: 99 }}
            artifacts={[artifact]}
            onClose={() => {}}
          />
        </ApiContext.Provider>
      </QueryClientProvider>,
    );
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Page 99 is outside the available range",
    );
    expect(await screen.findByText("Page 12 of 12")).toBeInTheDocument();
  } finally {
    vi.unstubAllGlobals();
  }
});
