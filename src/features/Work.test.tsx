import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ApiContext, createApi } from "../api";
import { Work } from "./Work";
import { beforeEach } from "vitest";

// The side pane now starts collapsed. These tests are about what it does once
// it is open, so they restore the saved "open" preference the app honours.
beforeEach(() => {
  localStorage.setItem("quantix.right-workspace.v2", "split");
});

it("lets an engineer start a ready task from an approved plan and shows start failures", async () => {
  const task = {
    id: "task",
    tender_id: "one",
    plan_id: "plan",
    title: "Review scope",
    description: "Check scope inclusions",
    role: "scope_review",
    status: "ready",
    source_ids: [],
    result: {},
    run_id: null,
    created_at: "",
    updated_at: "",
  };
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async (url, init) => {
      if (init?.method === "POST")
        return new Response(
          JSON.stringify({ detail: "Configure the AI connection first." }),
          { status: 400 },
        );
      return new Response(
        JSON.stringify(
          String(url).endsWith("/plans")
            ? [
                {
                  id: "plan",
                  title: "Scope review",
                  version: 1,
                  status: "approved",
                  tasks: [task],
                },
              ]
            : String(url).endsWith("/tasks")
              ? [task]
              : [],
        ),
      );
    },
  );
  const user = userEvent.setup();
  render(
    <QueryClientProvider client={new QueryClient()}>
      <ApiContext.Provider value={api}>
        <Work
          tenderId="one"
          onChanges={() => {}}
          onSource={() => {}}
          recordView="task"
          recordId="task"
        />
      </ApiContext.Provider>
    </QueryClientProvider>,
  );
  const summary = await screen.findByText("Check scope inclusions", {
    selector: "details > p",
  });
  expect(summary.parentElement).toHaveAttribute("open");
  await user.click(screen.getByRole("button", { name: "Run task" }));
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "Configure the AI connection first.",
  );
});

it("focuses and scrolls the persisted finding selected by a decisions record link", async () => {
  const calls: HTMLElement[] = [];
  const prior = Object.getOwnPropertyDescriptor(
    HTMLElement.prototype,
    "scrollIntoView",
  );
  Object.defineProperty(HTMLElement.prototype, "scrollIntoView", {
    configurable: true,
    value: function (this: HTMLElement) {
      calls.push(this);
    },
  });
  const finding = {
    id: "original-finding",
    tender_id: "one",
    title: "Check the fire rating",
    detail: "The drawing omits the wall rating.",
    kind: "question",
    state: "proposed",
    source_ids: [],
    origin: "agent",
    run_id: "run",
    is_stale: false,
    created_at: "",
    updated_at: "",
  };
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async (url) =>
      new Response(
        JSON.stringify(String(url).endsWith("/findings") ? [finding] : []),
      ),
  );
  try {
    render(
      <QueryClientProvider
        client={
          new QueryClient({ defaultOptions: { queries: { retry: false } } })
        }
      >
        <ApiContext.Provider value={api}>
          <Work
            tenderId="one"
            recordView="decisions"
            recordId="original-finding"
            onChanges={() => {}}
            onSource={() => {}}
          />
        </ApiContext.Provider>
      </QueryClientProvider>,
    );
    const original = (await screen.findByText("Check the fire rating")).closest(
      "article",
    )!;
    expect(original).toHaveAttribute("id", "finding-original-finding");
    expect(original).toHaveFocus();
    expect(calls).toContain(original);
  } finally {
    if (prior)
      Object.defineProperty(HTMLElement.prototype, "scrollIntoView", prior);
    else Reflect.deleteProperty(HTMLElement.prototype, "scrollIntoView");
  }
});

it("opens a canonical run and preserves recovery controls in Activity", async () => {
  const user = userEvent.setup();
  const run = {
    id: "failed-run",
    tender_id: "one",
    kind: "conversation",
    status: "failed",
    progress: 0,
    detail: "The manager could not reply.",
    error: "Model check required.",
    created_at: "2026-09-09T10:00:00Z",
    updated_at: "2026-09-09T10:00:00Z",
    result: {},
    usage: {},
    instruction: "Hello",
  };
  let resumed = false;
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async (url, init) => {
      if (init?.method === "POST") {
        resumed = String(url).endsWith("/failed-run/resume");
        return new Response(JSON.stringify({}));
      }
      return new Response(
        JSON.stringify(String(url).endsWith("/runs") ? [run] : []),
      );
    },
  );
  render(
    <QueryClientProvider
      client={
        new QueryClient({ defaultOptions: { queries: { retry: false } } })
      }
    >
      <ApiContext.Provider value={api}>
        <Work
          tenderId="one"
          onChanges={() => {}}
          onSource={() => {}}
          recordView="run"
          recordId="failed-run"
        />
      </ApiContext.Provider>
    </QueryClientProvider>,
  );
  // A failed run now leads with the reason, not a generic detail line.
  await screen.findByText("Model check required.");
  expect(screen.getByText("Tender Manager")).toBeVisible();
  await user.click(screen.getByRole("button", { name: "Resume" }));
  expect(resumed).toBe(true);
  expect(screen.getByRole("button", { name: "Run details" })).toBeVisible();
  expect(screen.queryByText("Supplier quotes")).not.toBeInTheDocument();
});
