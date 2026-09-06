import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ApiContext, createApi, type Schema } from "../api";
import { Files } from "./Files";

it("sends area and reading-status filters to Meaning search and shows the matched passage", async () => {
  const artifact: Schema<"Artifact"> = {
    id: "pdf",
    tender_id: "one",
    relative_path: "Area B/Scope.pdf",
    name: "Scope.pdf",
    version: 1,
    content_hash: "hash",
    size: 100,
    kind: "pdf",
    status: "needs_attention",
    area: "Area B",
    metadata: {},
    warnings: [],
    is_current: true,
    created_at: "",
  };
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async (address) => {
      const url = new URL(String(address));
      if (url.pathname.endsWith("/search-status"))
        return new Response(
          JSON.stringify({ ready: true, status: "ready", detail: "Ready" }),
        );
      expect(url.searchParams.get("mode")).toBe("meaning");
      expect(url.searchParams.get("area")).toBe("Area B");
      expect(url.searchParams.get("status")).toBe("needs_attention");
      return new Response(
        JSON.stringify([
          {
            id: "evidence",
            artifact_id: "pdf",
            artifact_name: "Scope.pdf",
            locator: "p. 4",
            text: "Unrelated introduction",
            metadata: {
              semantic_match: {
                text: "Matched passage: drainage to the inspection chamber.",
              },
            },
          },
        ]),
      );
    },
  );
  const user = userEvent.setup();
  render(
    <QueryClientProvider
      client={
        new QueryClient({ defaultOptions: { queries: { retry: false } } })
      }
    >
      <ApiContext.Provider value={api}>
        <Files
          tenderId="one"
          artifacts={[artifact]}
          meaningAvailable
          onImport={() => {}}
          onSource={() => {}}
        />
      </ApiContext.Provider>
    </QueryClientProvider>,
  );
  await user.selectOptions(screen.getByLabelText("Filter by area"), "Area B");
  await user.selectOptions(
    screen.getByLabelText("Filter by processing status"),
    "needs_attention",
  );
  await user.selectOptions(screen.getByLabelText("Search method"), "meaning");
  await user.type(screen.getByLabelText("Search source text"), "drainage");
  expect(
    await screen.findByText(
      "Matched passage: drainage to the inspection chamber.",
    ),
  ).toBeInTheDocument();
  expect(screen.queryByText("Unrelated introduction")).not.toBeInTheDocument();
});

it("keeps Both selected and reports meaning-search failures without substituting Words", async () => {
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async (url) =>
      new Response(
        JSON.stringify(
          String(url).includes("/search-status")
            ? {
                status: "stale",
                ready: false,
                detail: "The source documents have changed.",
              }
            : String(url).includes("mode=combined")
              ? { detail: "Prepare meaning search for the changed documents." }
              : [],
        ),
        { status: String(url).includes("mode=combined") ? 409 : 200 },
      ),
  );
  const user = userEvent.setup();
  render(
    <QueryClientProvider
      client={
        new QueryClient({ defaultOptions: { queries: { retry: false } } })
      }
    >
      <ApiContext.Provider value={api}>
        <Files
          tenderId="one"
          artifacts={[]}
          meaningAvailable
          onImport={() => {}}
          onSource={() => {}}
        />
      </ApiContext.Provider>
    </QueryClientProvider>,
  );
  await user.selectOptions(screen.getByLabelText("Search method"), "combined");
  await user.type(screen.getByLabelText("Search source text"), "drainage");
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "Prepare meaning search for the changed documents.",
  );
  expect(screen.getByLabelText("Search method")).toHaveValue("combined");
});

it("fetches actual historical files and opens the selected original revision", async () => {
  const shared = {
    tender_id: "one",
    relative_path: "BOQ.pdf",
    name: "BOQ.pdf",
    content_hash: "hash",
    size: 100,
    kind: "pdf",
    status: "extracted",
    area: "Area A",
    metadata: {},
    warnings: [],
    created_at: "",
  };
  const current: Schema<"Artifact"> = {
    ...shared,
    id: "new",
    version: 2,
    is_current: true,
  };
  const old: Schema<"Artifact"> = {
    ...shared,
    id: "old",
    version: 1,
    is_current: false,
  };
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async (url) =>
      new Response(
        JSON.stringify(
          String(url).includes("include_history=true") ? [current, old] : [],
        ),
      ),
  );
  let opened: unknown;
  const user = userEvent.setup();
  render(
    <QueryClientProvider client={new QueryClient()}>
      <ApiContext.Provider value={api}>
        <Files
          tenderId="one"
          artifacts={[current]}
          onImport={() => {}}
          onSource={(source) => {
            opened = source;
          }}
        />
      </ApiContext.Provider>
    </QueryClientProvider>,
  );
  await user.selectOptions(screen.getByLabelText("Filter by revision"), "all");
  const previous = await screen.findByText("Previous revision");
  await user.click(
    within(previous.closest("article")!).getByRole("button", {
      name: "BOQ.pdf",
    }),
  );
  expect(opened).toMatchObject({
    artifactId: "old",
    artifact: { id: "old", version: 1, is_current: false },
  });
  expect(
    screen.getByText(
      "Search uses current file versions. Earlier versions can be opened from the register.",
    ),
  ).toBeInTheDocument();
});

it("explains when document filters exclude all search results", async () => {
  const user = userEvent.setup();
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async () =>
      new Response(
        JSON.stringify([
          {
            id: "source",
            artifact_id: "pdf",
            artifact_name: "Scope.pdf",
            locator: "p. 1",
            text: "Drainage works",
            kind: "text",
          },
        ]),
      ),
  );
  const shared = {
    tender_id: "one",
    relative_path: "",
    version: 1,
    content_hash: "hash",
    size: 10,
    area: "",
    metadata: {},
    warnings: [],
    is_current: true,
    created_at: "",
  };
  const artifacts: Schema<"Artifact">[] = [
    {
      ...shared,
      id: "pdf",
      name: "Scope.pdf",
      kind: "pdf",
      status: "extracted",
    },
    {
      ...shared,
      id: "cad",
      name: "Plan.dwg",
      kind: "cad",
      status: "unsupported",
    },
  ];
  render(
    <QueryClientProvider client={new QueryClient()}>
      <ApiContext.Provider value={api}>
        <Files
          tenderId="one"
          artifacts={artifacts}
          onImport={() => {}}
          onSource={() => {}}
        />
      </ApiContext.Provider>
    </QueryClientProvider>,
  );
  await user.selectOptions(
    screen.getByLabelText("Filter by processing status"),
    "unsupported",
  );
  await user.type(screen.getByLabelText("Search source text"), "Drainage");
  expect(
    await screen.findByText(
      "No source matches these search and document filters.",
    ),
  ).toBeInTheDocument();
});
