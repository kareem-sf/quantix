import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { beforeEach, vi } from "vitest";
import { ApiContext, createApi, type Schema } from "../../api";
import { TenderOfficeWorkspace } from "./TenderOfficeWorkspace";

// The side pane now starts collapsed. These tests are about what it does once
// it is open, so they restore the saved "open" preference the app honours.
beforeEach(() => {
  localStorage.setItem("quantix.right-workspace.v2", "split");
});

vi.mock("../Manager", () => ({
  Manager: () => (
    <section aria-label="Manager conversation">
      <label>
        Message to Tender Manager
        <input
          aria-label="Message to Tender Manager"
          defaultValue="Keep this instruction"
        />
      </label>
    </section>
  ),
}));
vi.mock("../PlanReview", () => ({
  PlanReview: ({ onBack }: { onBack: () => void }) => (
    <button onClick={onBack}>Request changes</button>
  ),
}));

const overview = {
  tender: {
    id: "one",
    name: "Drainage works",
    status: "active",
    revision: 1,
    created_at: "",
    updated_at: "",
    name_source: "engineer",
  },
  artifact_count: 1,
  evidence_count: 1,
  coverage: {
    registered: 1,
    extracted: 1,
    needs_attention: 0,
    unsupported: 0,
    failed: 0,
  },
  areas: ["East"],
  findings: [],
  plan: null,
  active_runs: [],
  boq_count: 0,
} satisfies Schema<"Overview">;

const artifact = {
  id: "drawing",
  tender_id: "one",
  name: "Drainage plan.pdf",
  relative_path: "Drawings/Drainage plan.pdf",
  kind: "pdf",
  version: 2,
  content_hash: "hash-v2",
  size: 120,
  status: "extracted",
  area: "East",
  metadata: { page_count: 8 },
  warnings: [],
  is_current: true,
  created_at: "",
} satisfies Schema<"Artifact">;

it.each(["work", "tender"])(
  "refreshes addressed product history after a %s change without replacing the selected version",
  async (signal) => {
    let latestVersion = 1;
    const summary = (version: number) => ({
      id: `version-${version}`,
      product_id: "product-live",
      version,
      title: `Draft v${version}`,
      kind: "note",
      status: "draft",
      row_count: 0,
      dependency_state: "current",
      review_reasons: [],
      source_refs: [],
      method_refs: [],
      created_at: "2026-09-13T08:00:00Z",
    });
    const api = createApi(
      { base_url: "http://localhost/api", token: "test" },
      async (address) => {
        const url = new URL(String(address));
        if (url.pathname.endsWith("/rows"))
          return Response.json({ items: [], total: 0, missing: 0 });
        if (url.pathname.endsWith("/versions"))
          return Response.json({
            items: Array.from({ length: latestVersion }, (_, index) =>
              summary(latestVersion - index),
            ),
            total: latestVersion,
            next_offset: null,
          });
        const match = url.pathname.match(/\/versions\/(\d+)$/);
        if (match)
          return Response.json({
            ...summary(Number(match[1])),
            sanitized_content: `Saved content v${match[1]}`,
            rows: [],
            view_schema: {},
          });
        throw new Error(`Unexpected request: ${url.pathname}`);
      },
    );
    const client = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    const run = {
      id: "work-live",
      tender_id: "one",
      kind: "engineering",
      instruction: "Update the draft",
      status: "running",
      progress: 0,
      detail: "Working",
      created_at: "",
      updated_at: "",
    } satisfies Schema<"Run">;
    const tree = (updated: boolean) => (
      <QueryClientProvider client={client}>
        <ApiContext.Provider value={api}>
          <TenderOfficeWorkspace
            overview={{
              ...overview,
              tender: {
                ...overview.tender,
                revision: updated && signal === "tender" ? 2 : 1,
              },
              active_runs: updated && signal === "work" ? [] : [run],
            }}
            artifacts={[artifact]}
            recordView="work-product"
            recordId="product-live"
            onImport={vi.fn()}
            onSettings={vi.fn()}
            onSource={vi.fn()}
          />
        </ApiContext.Provider>
      </QueryClientProvider>
    );
    const view = render(tree(false));
    expect(await screen.findByText("Saved content v1")).toBeVisible();
    expect(
      await screen.findByRole("button", { name: "Version 1 · Draft v1" }),
    ).toBeVisible();
    latestVersion = 2;
    await act(async () => {
      await client.invalidateQueries();
    });
    view.rerender(tree(true));
    const versionTwo = await screen.findByRole("button", {
      name: "Version 2 · Draft v2",
    });
    expect(screen.getByText("Saved content v1")).toBeVisible();
    expect(screen.getByLabelText("Message to Tender Manager")).toHaveValue(
      "Keep this instruction",
    );
    await userEvent.click(versionTwo);
    expect(await screen.findByText("Saved content v2")).toBeVisible();
  },
);

function renderWorkspace(
  props: Partial<React.ComponentProps<typeof TenderOfficeWorkspace>> = {},
) {
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async (address) => {
      const path = new URL(String(address)).pathname;
      if (path.endsWith("/calculations/calc-addressed"))
        return Response.json({
          id: "calc-addressed",
          method_id: "product",
          method_version: "1",
          precision: "0.01",
          rounding: "HALF_UP",
          status: "calculated",
          formula_hash: "formula",
          basis_fingerprint: "basis",
          typed_inputs: { quantity: "12" },
          units: {},
          outputs: {},
          assumptions: ["Addressed calculation assumption"],
          created_at: "",
        });
      if (path.endsWith("/team"))
        return Response.json({ staff: [], assignments: [] });
      if (path.endsWith("/work-brief")) return Response.json({ brief: null });
      if (path.endsWith("/evidence/source-1"))
        return Response.json({
          id: "source-1",
          artifact_id: artifact.id,
          artifact_name: artifact.name,
          relative_path: artifact.relative_path,
          locator: "page 4",
          page: 4,
          text: "Drainage route",
          kind: "text",
          metadata: {},
          score: 0,
        });
      throw new Error(`Unexpected request: ${path}`);
    },
  );
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  const tree = (
    next: Partial<React.ComponentProps<typeof TenderOfficeWorkspace>> = props,
  ) => (
    <QueryClientProvider client={client}>
      <ApiContext.Provider value={api}>
        <TenderOfficeWorkspace
          overview={overview}
          artifacts={[artifact]}
          onImport={() => {}}
          onSettings={() => {}}
          onSource={() => {}}
          onDocuments={() => {}}
          onRepair={() => {}}
          {...next}
        />
      </ApiContext.Provider>
    </QueryClientProvider>
  );
  const view = render(tree());
  return {
    ...view,
    rerenderWorkspace: (
      next: Partial<React.ComponentProps<typeof TenderOfficeWorkspace>>,
    ) => view.rerender(tree({ ...props, ...next })),
  };
}

it("opens an addressed calculation in Reviews and preserves the Manager draft", async () => {
  renderWorkspace({ recordView: "calculation", recordId: "calc-addressed" });
  expect(
    await screen.findByText("Addressed calculation assumption"),
  ).toBeVisible();
  expect(screen.getByLabelText("Message to Tender Manager")).toHaveValue(
    "Keep this instruction",
  );
});

it("starts with the workspace launcher and opens the team without losing the Manager draft", async () => {
  const user = userEvent.setup();
  renderWorkspace();
  expect(
    screen.getByRole("region", { name: "Manager conversation" }),
  ).toBeInTheDocument();
  expect(
    screen.queryByRole("region", { name: "Tender team" }),
  ).not.toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: /^Team/ }));
  expect(await screen.findByText(/No staff yet/)).toBeInTheDocument();
  expect(
    screen.queryByRole("region", { name: "Current document" }),
  ).not.toBeInTheDocument();
  expect(
    screen.getByRole("textbox", { name: "Message to Tender Manager" }),
  ).toHaveValue("Keep this instruction");
});

it("opens the document list", async () => {
  const user = userEvent.setup();
  renderWorkspace();
  await user.click(screen.getByRole("button", { name: /^Documents/ }));
  expect(
    screen.getByRole("region", { name: "Workspace documents" }),
  ).toHaveTextContent("Drainage plan.pdf");
});

it("opens the requested source ahead of its parent result on a collapsed deep link", async () => {
  localStorage.setItem("quantix.right-workspace.v2", "hidden");
  renderWorkspace({
    recordView: "calculation",
    recordId: "calc-addressed",
    sourceSelection: {
      sourceId: "source-1",
      artifactId: artifact.id,
      version: 2,
      contentHash: "hash-v2",
    },
    onCloseSource: () => {},
  });
  expect(
    await screen.findByRole("region", { name: "Source document" }),
  ).toBeVisible();
  expect(screen.getByRole("tab", { name: "Documents" })).toHaveAttribute(
    "aria-selected",
    "true",
  );
});

it("returns to and focuses the Manager when requesting plan changes from an expanded workspace", async () => {
  const user = userEvent.setup();
  renderWorkspace({ recordView: "plan-review", recordId: "plan-1" });
  await user.click(screen.getByRole("button", { name: "Expand workspace" }));
  await user.click(
    await screen.findByRole("button", { name: "Request changes" }),
  );
  expect(
    screen.getByRole("textbox", { name: "Message to Tender Manager" }),
  ).toBeVisible();
  expect(
    screen.getByRole("button", { name: "Show workspace" }),
  ).toBeInTheDocument();
  await waitFor(() =>
    expect(
      screen.getByRole("textbox", { name: "Message to Tender Manager" }),
    ).toHaveFocus(),
  );
});

it("clears visited documents and transient Manager state when changing Tender", async () => {
  const user = userEvent.setup();
  const view = renderWorkspace();
  await user.type(
    screen.getByRole("textbox", { name: "Message to Tender Manager" }),
    " in Tender one",
  );
  await user.click(screen.getByRole("button", { name: /^Documents/ }));
  expect(screen.getByRole("tab", { name: "Documents" })).toBeInTheDocument();
  view.rerenderWorkspace({
    overview: {
      ...overview,
      tender: { ...overview.tender, id: "two", name: "Tender two" },
    },
    artifacts: [],
  });
  expect(
    screen.queryByRole("tab", { name: "Documents" }),
  ).not.toBeInTheDocument();
  expect(
    screen.getByRole("textbox", { name: "Message to Tender Manager" }),
  ).toHaveValue("Keep this instruction");
  expect(screen.getByText("Tender two")).toBeInTheDocument();
});

it("honors View all documents when clearing a source that has a parent result", async () => {
  const user = userEvent.setup();
  const view = renderWorkspace({
    recordView: "calculation",
    recordId: "calc-addressed",
    sourceSelection: {
      sourceId: "source-1",
      artifactId: artifact.id,
      version: 2,
      contentHash: "hash-v2",
    },
    onCloseSource: () => view.rerenderWorkspace({ sourceSelection: null }),
  });
  await screen.findByRole("region", { name: "Source document" });
  await user.click(screen.getByRole("button", { name: "Tender context" }));
  await user.click(screen.getByRole("button", { name: "View all documents" }));
  expect(screen.getByRole("tab", { name: "Documents" })).toHaveAttribute(
    "aria-selected",
    "true",
  );
  expect(
    screen.getByRole("region", { name: "Workspace documents" }),
  ).toBeVisible();
});
