import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ApiContext, createApi, type Schema } from "../api";
import { SourceDrawer } from "./Sources";

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
    version: 1,
    content_hash: "hash",
    size: 1,
    status: "extracted",
    area: "",
    metadata: {},
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
          page: 1,
          page_count: 1,
          page_size: [200, 100],
          version: 1,
          content_hash: "hash",
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
            selection={{ artifactId: "drawing" }}
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
        path.includes("/tenders/one/artifacts/drawing/measurement-page?page=1"),
      ),
    ).toBe(true);
    await user.click(screen.getByRole("button", { name: "Back to source" }));
    expect(
      screen.getByRole("heading", { name: "Source text" }),
    ).toBeInTheDocument();
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
