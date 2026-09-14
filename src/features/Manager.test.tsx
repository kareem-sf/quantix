import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { vi } from "vitest";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ApiContext, createApi, type Schema } from "../api";
import { Manager } from "./Manager";

it("keeps an asynchronous manager failure visible after it leaves the active runs", async () => {
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async (url) =>
      new Response(
        JSON.stringify(
          String(url).endsWith("/runs")
            ? [
                {
                  id: "run",
                  tender_id: "one",
                  kind: "manager",
                  instruction: "Review scope",
                  status: "failed",
                  progress: 0,
                  detail: "Review failed",
                  result: {},
                  usage: {},
                  error: "Provider unavailable",
                  created_at: "2026-09-06T12:00:00Z",
                  updated_at: "2026-09-06T12:01:00Z",
                },
              ]
            : [],
        ),
      ),
  );
  const overview: Schema<"Overview"> = {
    tender: {
      id: "one",
      name: "Test tender",
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
    areas: [],
    findings: [],
    plan: null,
    active_runs: [],
    boq_count: 0,
  };
  render(
    <QueryClientProvider
      client={
        new QueryClient({ defaultOptions: { queries: { retry: false } } })
      }
    >
      <ApiContext.Provider value={api}>
        <Manager
          overview={overview}
          artifacts={[]}
          onImport={() => {}}
          onSettings={() => {}}
          onSource={() => {}}
        />
      </ApiContext.Provider>
    </QueryClientProvider>,
  );
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "Provider unavailable",
  );
});

it("merges the newest page and concurrent replies without losing older history or its reading position", async () => {
  const user = userEvent.setup();
  const message = (index: number): Schema<"Message"> => ({
    id: `message-${index}`,
    tender_id: "history",
    role: index % 2 ? "engineer" : "manager",
    content: `Dialogue ${index}`,
    created_at: new Date(Date.UTC(2026, 8, 9, 10, 0, index)).toISOString(),
    source_ids: [],
    result_links: [],
  });
  const dialogue = Array.from({ length: 55 }, (_, index) => message(index + 1));
  const path = "/tenders/history/messages?limit=50";
  const olderCursors: string[] = [];
  let resolveOlder: (() => void) | undefined;
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async (url) => {
      const parsed = new URL(String(url));
      if (parsed.pathname.endsWith("/messages")) {
        const cursor = parsed.searchParams.get("cursor");
        if (cursor) {
          olderCursors.push(cursor);
          const index = dialogue.findIndex((item) => item.id === cursor);
          const items = dialogue.slice(Math.max(0, index - 50), index);
          await new Promise<void>((resolve) => {
            resolveOlder = resolve;
          });
          return new Response(
            JSON.stringify({
              items,
              next_cursor: index > 50 ? items[0].id : null,
            }),
          );
        }
        const items = dialogue.slice(-50);
        return new Response(
          JSON.stringify({
            items,
            next_cursor: dialogue.length > 50 ? items[0].id : null,
          }),
        );
      }
      return new Response(
        JSON.stringify(
          parsed.pathname.endsWith("/pending-message") ? null : [],
        ),
      );
    },
  );
  const overview: Schema<"Overview"> = {
    tender: {
      id: "history",
      name: "History tender",
      status: "active",
      revision: 1,
      created_at: "",
      updated_at: "",
      name_source: "engineer",
    },
    artifact_count: 0,
    evidence_count: 0,
    coverage: {
      registered: 0,
      extracted: 0,
      needs_attention: 0,
      unsupported: 0,
      failed: 0,
    },
    areas: [],
    findings: [],
    plan: null,
    active_runs: [],
    boq_count: 0,
  };
  const rectangles = vi
    .spyOn(HTMLElement.prototype, "getBoundingClientRect")
    .mockImplementation(function (this: HTMLElement) {
      const scroller = this.closest<HTMLElement>(".manager-scroll");
      const index = scroller
        ? Array.from(scroller.querySelectorAll("[data-message-id]")).indexOf(
            this,
          )
        : -1;
      const top = index >= 0 ? index * 20 - (scroller?.scrollTop ?? 0) : 0;
      return {
        top,
        bottom: top + 20,
        left: 0,
        right: 100,
        height: 20,
        width: 100,
        x: 0,
        y: top,
        toJSON: () => ({}),
      };
    });
  try {
    render(
      <QueryClientProvider client={client}>
        <ApiContext.Provider value={api}>
          <Manager
            overview={overview}
            artifacts={[]}
            onImport={() => {}}
            onSettings={() => {}}
            onSource={() => {}}
          />
        </ApiContext.Provider>
      </QueryClientProvider>,
    );
    await screen.findByText("Dialogue 55");
    expect(screen.queryByText("Dialogue 1")).not.toBeInTheDocument();
    const scroller = document.querySelector<HTMLElement>(".manager-scroll")!;
    Object.defineProperty(scroller, "scrollHeight", {
      configurable: true,
      get: () => scroller.querySelectorAll("[data-message-id]").length * 20,
    });
    Object.defineProperty(scroller, "clientHeight", {
      configurable: true,
      value: 100,
    });
    scroller.scrollTop = 80;
    fireEvent.scroll(scroller);
    await act(async () => {
      dialogue.push(message(56));
      await client.invalidateQueries({ queryKey: [path] });
    });
    expect(await screen.findByText("Dialogue 56")).toBeInTheDocument();
    expect(scroller.querySelectorAll("[data-message-id]")).toHaveLength(51);
    expect(scroller.scrollTop).toBe(80);
    await user.click(
      screen.getByRole("button", { name: "Load earlier messages" }),
    );
    expect(olderCursors).toEqual(["message-6"]);
    await act(async () => {
      dialogue.push(message(57));
      await client.invalidateQueries({ queryKey: [path] });
    });
    expect(await screen.findByText("Dialogue 57")).toBeInTheDocument();
    await act(async () => {
      resolveOlder?.();
    });
    await screen.findByText("Dialogue 1");
    await waitFor(() =>
      expect(scroller.querySelectorAll("[data-message-id]")).toHaveLength(57),
    );
    expect(
      Array.from(
        scroller.querySelectorAll<HTMLElement>("[data-message-id]"),
      ).map((node) => node.dataset.messageId),
    ).toEqual(dialogue.map((item) => item.id));
    expect(scroller.scrollTop).toBe(180);
    expect(
      screen.queryByRole("button", { name: "Load earlier messages" }),
    ).not.toBeInTheDocument();
    await act(async () => {
      await client.invalidateQueries({ queryKey: [path] });
    });
    expect(
      screen.queryByRole("button", { name: "Load earlier messages" }),
    ).not.toBeInTheDocument();
  } finally {
    rectangles.mockRestore();
  }
});

it("uses the explicit review documents action instead of placing the import log in chat", async () => {
  const user = userEvent.setup();
  let request: { action?: string; content?: string } | undefined;
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async (_url, init) => {
      if (String(_url).endsWith("/ai-policy"))
        return new Response(
          JSON.stringify({ manager: { model_id: "checked-model" } }),
        );
      if (init?.method !== "POST")
        return new Response(
          JSON.stringify(String(_url).endsWith("/pending-message") ? null : []),
        );
      if (init?.method === "POST") request = JSON.parse(String(init.body));
      return new Response(
        JSON.stringify({
          outcome: "immediate",
          run: {
            id: "run",
            tender_id: "one",
            kind: "manager",
            instruction: "",
            status: "queued",
            progress: 0,
            detail: "",
            result: {},
            usage: {},
            error: null,
            created_at: "",
            updated_at: "",
          },
        }),
      );
    },
  );
  const overview: Schema<"Overview"> = {
    tender: {
      id: "one",
      name: "Test tender",
      status: "active",
      revision: 1,
      created_at: "",
      updated_at: "",
      name_source: "engineer",
    },
    artifact_count: 2,
    evidence_count: 0,
    coverage: {
      registered: 2,
      extracted: 0,
      needs_attention: 0,
      unsupported: 0,
      failed: 0,
    },
    areas: [],
    findings: [],
    plan: null,
    active_runs: [],
    boq_count: 0,
  };
  render(
    <QueryClientProvider
      client={
        new QueryClient({ defaultOptions: { queries: { retry: false } } })
      }
    >
      <ApiContext.Provider value={api}>
        <Manager
          overview={overview}
          artifacts={[]}
          onImport={() => {}}
          onSettings={() => {}}
          onSource={() => {}}
        />
      </ApiContext.Provider>
    </QueryClientProvider>,
  );
  await user.click(
    await screen.findByRole("button", { name: "Review documents" }),
  );
  expect(request).toMatchObject({
    action: "review_documents",
    content: expect.stringContaining("Review the tender documents"),
  });
});

it("places a stopped run's notice after the conversation, where the engineer is reading", async () => {
  const messages: Schema<"Message">[] = [
    {
      id: "m1",
      tender_id: "one",
      role: "engineer",
      content: "hi",
      created_at: "2026-09-06T12:00:00Z",
      source_ids: [],
      result_links: [],
    },
  ];
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async (url) =>
      new Response(
        JSON.stringify(
          String(url).endsWith("/runs")
            ? [
                {
                  id: "run",
                  tender_id: "one",
                  kind: "conversation",
                  instruction: "hi",
                  status: "failed",
                  progress: 0,
                  detail: "Work needs attention.",
                  result: {},
                  usage: {},
                  error: "The local client ended without submitting.",
                  created_at: "2026-09-06T12:00:00Z",
                  updated_at: "2026-09-06T12:01:00Z",
                },
              ]
            : String(url).includes("/messages")
              ? messages
              : [],
        ),
      ),
  );
  const overview: Schema<"Overview"> = {
    tender: {
      id: "one",
      name: "Test tender",
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
    areas: [],
    findings: [],
    plan: null,
    active_runs: [],
    boq_count: 0,
  };
  render(
    <QueryClientProvider
      client={
        new QueryClient({ defaultOptions: { queries: { retry: false } } })
      }
    >
      <ApiContext.Provider value={api}>
        <Manager
          overview={overview}
          artifacts={[]}
          onImport={() => {}}
          onSettings={() => {}}
          onSource={() => {}}
        />
      </ApiContext.Provider>
    </QueryClientProvider>,
  );
  const notice = await screen.findByRole("alert");
  const message = await screen.findByText("hi");
  // Rendered above the history the notice scrolls out of sight, and stopped
  // work then looks exactly like no answer at all.
  expect(
    message.compareDocumentPosition(notice) & Node.DOCUMENT_POSITION_FOLLOWING,
  ).toBeTruthy();
});

it("attaches saved activity to its originating engineer instruction after completion", async () => {
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async (url) => {
      const path = new URL(String(url)).pathname;
      const body = path.endsWith("/messages")
        ? [
            {
              id: "instruction",
              tender_id: "one",
              role: "engineer",
              content: "Check foundation quantities",
              source_ids: [],
              run_id: "completed-run",
              created_at: "2026-09-13T10:00:00Z",
            },
          ]
        : path.endsWith("/activity")
          ? {
              items: [
                {
                  event_id: 1,
                  run_id: "completed-run",
                  operation_id: null,
                  parent_operation_id: null,
                  actor_id: "manager",
                  actor_label: "Tender Manager",
                  assignment_id: null,
                  category: "tool",
                  phase: "completed",
                  message: "Foundation quantities checked",
                  created_at: "2026-09-13T10:01:00Z",
                  tool: null,
                  provider: null,
                  model: null,
                  preview: "",
                  detail_available: false,
                  capture_status: "captured",
                  elapsed_ms: null,
                  source_ids: [],
                },
              ],
              cursor: "1",
              before_cursor: null,
              has_more: false,
              has_earlier: false,
              reset_required: false,
              run_status: "completed",
              run_detail: "Work finished",
              run_updated_at: "2026-09-13T10:01:00Z",
              history_key: "history",
            }
          : path.endsWith("/pending-message")
            ? null
            : [];
      return new Response(JSON.stringify(body));
    },
  );
  const overview: Schema<"Overview"> = {
    tender: {
      id: "one",
      name: "Test",
      status: "active",
      revision: 1,
      created_at: "",
      updated_at: "",
      name_source: "engineer",
    },
    artifact_count: 0,
    evidence_count: 0,
    coverage: {
      registered: 0,
      extracted: 0,
      needs_attention: 0,
      unsupported: 0,
      failed: 0,
    },
    areas: [],
    findings: [],
    plan: null,
    active_runs: [],
    boq_count: 0,
  };
  render(
    <QueryClientProvider
      client={
        new QueryClient({ defaultOptions: { queries: { retry: false } } })
      }
    >
      <ApiContext.Provider value={api}>
        <Manager
          overview={overview}
          artifacts={[]}
          onImport={() => {}}
          onSettings={() => {}}
          onSource={() => {}}
        />
      </ApiContext.Provider>
    </QueryClientProvider>,
  );
  const instruction = await screen.findByText("Check foundation quantities");
  const event = await screen.findByText("Foundation quantities checked");
  expect(
    instruction.compareDocumentPosition(event) &
      Node.DOCUMENT_POSITION_FOLLOWING,
  ).toBeTruthy();
  // Finished work folds into one "How the Tender Manager worked" timeline.
  expect(
    screen.getAllByRole("region", { name: "How the Tender Manager worked" }),
  ).toHaveLength(1);
});
