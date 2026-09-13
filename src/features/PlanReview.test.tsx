import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ApiContext, createApi, type Schema } from "../api";
import { PlanReview } from "./PlanReview";

const SUPPORTED_DRAFT_OUTPUTS = [
  "draft_documents",
  "drawing_measurements",
  "findings",
  "plan",
  "price_proposals",
  "programme_proposal",
  "project_map_nodes",
  "quantity_proposals",
  "quote_drafts",
  "submission_requirements",
  "summary",
  "unit_rate_proposals",
  "web_findings",
];

function review(): Schema<"PlanReview"> {
  return {
    tender_id: "one",
    plan_id: "plan",
    plan_version: 2,
    plan_status: "proposed",
    scope: {
      title: "Initial review",
      plan_version: 2,
      tender_revision: 4,
      source_ids: ["source"],
      source_revision: 4,
    },
    tasks: [
      {
        id: "task",
        title: "Check coverage",
        description: "Review the supplied package.",
        role: "coverage",
        status: "ready",
        source_ids: ["source"],
        route: null,
      },
    ],
    routes: [
      {
        kind: "manager",
        task_id: null,
        role: "Tender Manager",
        route: {
          connection_id: "connection",
          model_id: "model",
          reasoning: null,
          max_output_tokens: 1024,
          web_search: false,
          max_search_calls: 3,
        },
        connection_revision: 2,
        model: { display_name: "Model" },
        readiness: "ready",
        account_name: "AI account",
        provider: "openai",
        data_destination: "OpenAI API",
        billing: "metered",
        provider_managed_extras: false,
        spending_detail: "Tender budget applies.",
      },
    ],
    delegation: {
      version: 1,
      purpose: "Coordinate the current Tender work plan: Initial review",
      source_scope: "reviewed_tender",
      artifacts: [
        {
          artifact_id: "source",
          version: 1,
          content_hash: "a".repeat(64),
        },
      ],
      tools: [
        {
          id: "source.read",
          version: 1,
          description: "Read the reviewed Tender sources.",
          read_only: true,
        },
      ],
      allowed_draft_outputs: ["summary", "findings"],
      route_options: [
        {
          id: "route-option",
          route: {
            connection_id: "connection",
            model_id: "model",
            reasoning: null,
            max_output_tokens: 1024,
            web_search: false,
            max_search_calls: 3,
          },
          connection_revision: 2,
          model_revision: "model-revision",
          model: { display_name: "Model" },
          account_name: "AI account",
          provider: "openai",
          data_destination: "OpenAI API",
          billing: "metered",
          provider_managed_extras: false,
          readiness: "ready",
        },
      ],
      max_staff: 12,
      max_assignments: 24,
      max_depth: 1,
      max_concurrency: 1,
      max_requests: 4,
      max_search_calls: 0,
      run_budget_usd: 1,
      tender_budget_usd: 5,
    },
    delegation_proposal_version: 0,
    delegation_options: {
      artifacts: [
        {
          artifact_id: "source",
          name: "Package.pdf",
          display_name: "Package.pdf",
          relative_path: "Package.pdf",
          version: 1,
          content_hash: "a".repeat(64),
          status: "current",
        },
      ],
      tools: [
        {
          id: "source.read",
          version: 1,
          description: "Read the reviewed Tender sources.",
          read_only: true,
        },
      ],
      route_options: [
        {
          id: "route-option",
          route: {
            connection_id: "connection",
            model_id: "model",
            reasoning: null,
            max_output_tokens: 1024,
            web_search: false,
            max_search_calls: 3,
          },
          connection_revision: 2,
          model_revision: "model-revision",
          model: { display_name: "Model" },
          account_name: "AI account",
          provider: "openai",
          data_destination: "OpenAI API",
          billing: "metered",
          provider_managed_extras: false,
          readiness: "ready",
        },
      ],
      supported_draft_outputs: [...SUPPORTED_DRAFT_OUTPUTS],
      selected: null,
    },
    ai_summary:
      "This plan uses the reviewed account and keeps the Tender scope unchanged.",
    meaningful_changes: [
      {
        code: "route_changed",
        detail: "The manager route changed.",
        before: "old",
        after: "new",
      },
    ],
    blockers: [],
    snapshot: {
      tender_revision: 4,
      source_revision: 4,
      policy_revision: 2,
      allowed_connection_ids: ["connection"],
      connection_revisions: { connection: 2 },
      model_revisions: { model: "1" },
      route_intents: [],
      run_budget_usd: 1,
      tender_budget_usd: 5,
      max_requests: 4,
      spent_usd: 0,
      reserved_usd: 0,
      usage_fingerprint: "usage",
      provider_managed_extras: {},
      allowed_destinations: [],
      restore_reconciliation_required: false,
      spend_history_may_be_incomplete: false,
      active_run_ids: [],
    },
    fingerprint: "a".repeat(64),
    can_approve: true,
    reviewed_at: "2026-09-09T10:00:00Z",
  };
}

it("posts the displayed fingerprint and retains the note when approval conflicts", async () => {
  const user = userEvent.setup();
  let posted: unknown;
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async (url, init) => {
      if (init?.method === "POST") {
        posted = JSON.parse(String(init.body));
        return new Response(
          JSON.stringify({ detail: "The plan changed. Review it again." }),
          { status: 409 },
        );
      }
      return new Response(JSON.stringify(review()));
    },
  );
  render(
    <QueryClientProvider
      client={
        new QueryClient({ defaultOptions: { queries: { retry: false } } })
      }
    >
      <ApiContext.Provider value={api}>
        <PlanReview tenderId="one" planId="plan" onBack={() => {}} />
      </ApiContext.Provider>
    </QueryClientProvider>,
  );
  await screen.findByRole("heading", { name: "AI for this plan" });
  await user.click(screen.getByText("Add a decision note (optional)"));
  await user.type(
    screen.getByRole("textbox", { name: /decision note/i }),
    "Keep the reviewed scope.",
  );
  await user.click(screen.getByRole("button", { name: /Approve & start/i }));
  expect((await screen.findAllByRole("alert"))[0]).toHaveTextContent(
    "plan changed",
  );
  expect(screen.getByRole("textbox", { name: /decision note/i })).toHaveValue(
    "Keep the reviewed scope.",
  );
  expect(posted).toEqual({
    fingerprint: "a".repeat(64),
    engineer_confirmed: true,
    rationale: "Keep the reviewed scope.",
  });
});

it("saves an exact calculation runtime and its fixed execution tool", async () => {
  const user = userEvent.setup();
  const value = review();
  const runtime: Schema<"ReviewedCodeRuntime"> = {
    engine: "monty",
    fingerprint: "c".repeat(64),
    version: "0.9.3",
    image_id: null,
    library_versions: { monty: "0.9.3" },
    seconds: 8,
    memory_mib: 64,
    cpus: 1,
    processes: 1,
    output_mib: 2,
    tool_calls: 4,
  };
  value.delegation_options!.code_runtimes = [runtime];
  let patchBody: Schema<"DelegationProposalEdit"> | null = null;
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async (_url, init) => {
      if (init?.method === "PATCH") {
        patchBody = JSON.parse(String(init.body));
        return Response.json({});
      }
      return Response.json(value);
    },
  );
  render(
    <QueryClientProvider
      client={
        new QueryClient({ defaultOptions: { queries: { retry: false } } })
      }
    >
      <ApiContext.Provider value={api}>
        <PlanReview tenderId="one" planId="plan" />
      </ApiContext.Provider>
    </QueryClientProvider>,
  );
  await screen.findByText(value.delegation!.purpose);
  await user.click(
    screen.getByRole("button", { name: "Edit delegation choices" }),
  );
  await user.click(
    screen.getByRole("checkbox", { name: /Monty calculation sandbox/ }),
  );
  await user.click(
    screen.getByRole("button", { name: "Save delegation choices" }),
  );

  await waitFor(() => expect(patchBody).not.toBeNull());
  expect(patchBody).toEqual(
    expect.objectContaining({
      code_runtimes: [runtime],
      tool_ids: ["source.read", "execute_tool_code"],
    }),
  );
});

it("keeps the viewed fingerprint when background data changes, then requires a fresh review after conflict", async () => {
  const user = userEvent.setup();
  let latest = review();
  let submitted = "";
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async (_url, init) => {
      if (init?.method === "POST") {
        submitted = JSON.parse(String(init.body)).fingerprint;
        return new Response(JSON.stringify({ detail: "Review changed." }), {
          status: 409,
        });
      }
      return new Response(JSON.stringify(latest));
    },
  );
  render(
    <QueryClientProvider client={client}>
      <ApiContext.Provider value={api}>
        <PlanReview tenderId="one" planId="plan" />
      </ApiContext.Provider>
    </QueryClientProvider>,
  );
  await screen.findByText("Check coverage");
  latest = {
    ...review(),
    fingerprint: "b".repeat(64),
    tasks: [{ ...review().tasks[0], title: "Review revised scope" }],
  };
  await act(async () => {
    client.setQueryData(["/tenders/one/plans/plan/review"], latest);
  });
  expect(screen.queryByText("Review revised scope")).not.toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: /Approve & start/ }));
  expect(submitted).toBe("a".repeat(64));
  await screen.findByText("Review revised scope");
  expect(
    screen.getByRole("button", { name: /Approve & start/ }),
  ).toBeDisabled();
  await user.click(
    screen.getByRole("button", { name: "I reviewed the updated arrangements" }),
  );
  expect(screen.getByRole("button", { name: /Approve & start/ })).toBeEnabled();
});

it("shows mixed billing and each destination while keeping specialist controls in More options", async () => {
  const user = userEvent.setup();
  const value = review();
  value.routes[0] = {
    ...value.routes[0],
    billing: "subscription",
    account_name: "Grok account",
    data_destination: "xAI",
    spending_detail: "Subscription allowance",
  };
  value.routes.push({
    ...review().routes[0],
    kind: "specialist",
    task_id: "task",
    role: "Coverage",
    route: {
      ...review().routes[0].route,
      connection_id: "paid",
      reasoning: "high",
    },
  });
  value.can_approve = false;
  value.blockers = [
    {
      code: "spending",
      detail: "Review the metered route limit.",
      repair_target: "/tenders/one/work?view=ai",
      connection_id: "paid",
      model_id: "model",
      task_id: "task",
    },
  ];
  const targets: string[] = [];
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async () => new Response(JSON.stringify(value)),
  );
  render(
    <QueryClientProvider
      client={
        new QueryClient({ defaultOptions: { queries: { retry: false } } })
      }
    >
      <ApiContext.Provider value={api}>
        <PlanReview
          tenderId="one"
          planId="plan"
          onRepair={(target) => targets.push(target)}
        />
      </ApiContext.Provider>
    </QueryClientProvider>,
  );
  await screen.findByRole("heading", { name: "AI for this plan" });
  expect(
    screen.getByText("Grok account · Model", { selector: "strong" }),
  ).toBeVisible();
  expect(
    screen
      .getAllByText("AI account · Model")
      .some((node) => !node.closest("details")),
  ).toBe(true);
  expect(
    screen.getByText(/Subscription allowance; Metered routes/),
  ).toBeVisible();
  expect(screen.getByText(/Reasoning high/)).not.toBeVisible();
  await user.click(screen.getByText("More options"));
  expect(screen.getByText(/Reasoning high/)).toBeVisible();
  await user.click(screen.getByRole("button", { name: "Review spending" }));
  expect(targets).toEqual(["/tenders/one/work?view=ai"]);
  expect(
    screen.getByRole("button", { name: /Approve & start/ }),
  ).toBeDisabled();
});

it("prominently discloses paid provider extras beside metered specialist spending", async () => {
  const value = review();
  value.routes[0] = {
    ...value.routes[0],
    billing: "subscription",
    provider_managed_extras: true,
    account_name: "Grok account",
    data_destination: "xAI",
    spending_detail: "Subscription allowance",
  };
  value.routes.push({
    ...review().routes[0],
    kind: "specialist",
    task_id: "task",
    route: { ...review().routes[0].route, connection_id: "metered-specialist" },
  });
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async () => new Response(JSON.stringify(value)),
  );
  render(
    <QueryClientProvider
      client={
        new QueryClient({ defaultOptions: { queries: { retry: false } } })
      }
    >
      <ApiContext.Provider value={api}>
        <PlanReview tenderId="one" planId="plan" />
      </ApiContext.Provider>
    </QueryClientProvider>,
  );
  const warning = await screen.findByText(
    "Provider extras may incur charges; Quantix cannot guarantee a cap.",
  );
  expect(warning).toBeVisible();
  expect(warning.closest("details")).toBeNull();
  expect(
    screen.getByText(
      /Subscription with paid provider extras enabled; Metered routes/,
    ),
  ).toBeVisible();
  expect(
    screen.getAllByText("USD 1").some((element) => !element.closest("details")),
  ).toBe(true);
});

it("keeps six long task descriptions compact and preserves their full scope and sources on expansion", async () => {
  const user = userEvent.setup();
  const value = review();
  value.tasks = Array.from({ length: 6 }, (_, index) => ({
    ...value.tasks[0],
    id: `task-${index}`,
    title: `Review task ${index + 1}`,
    description:
      index === 5
        ? "Review each supplied drawing and retain the exact document revision and source location for every engineering finding before preparing the reviewed scope and marking the areas that still need the engineer to inspect the original drawing. " +
          "Preserve the supplied quantities and limits. ".repeat(3)
        : `Review the supplied drawings for area ${index + 1}. ` +
          "Retain the original quantities and flag any missing engineering information for review. ".repeat(
            4,
          ),
  }));
  const opened: unknown[] = [];
  let submitted: unknown;
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async (url, init) => {
      if (init?.method === "POST") {
        submitted = JSON.parse(String(init.body));
        return new Response(JSON.stringify({}));
      }
      return new Response(
        JSON.stringify(
          String(url).includes("/evidence/")
            ? {
                artifact_id: "drawing",
                artifact_name: "Drawing.pdf",
                page: 2,
                locator: "Page 2",
              }
            : value,
        ),
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
        <PlanReview
          tenderId="one"
          planId="plan"
          onSource={(selection) => opened.push(selection)}
        />
      </ApiContext.Provider>
    </QueryClientProvider>,
  );
  await screen.findByText("Review task 6");
  for (const task of value.tasks) {
    const detail = screen.getByText(task.title).closest("details")!;
    expect(detail).not.toHaveAttribute("open");
    const preview = detail.querySelector(".review-task-preview")!.textContent!;
    expect(preview.length).toBeLessThanOrEqual(140);
    expect(task.description.startsWith(preview.replace(/…$/, ""))).toBe(true);
    const fullText = detail.querySelector(".review-task-detail > p")!;
    expect(fullText.textContent).toBe(task.description);
    expect(fullText).not.toBeVisible();
  }
  await user.click(screen.getByText("Review task 1"));
  expect(screen.getByText(value.tasks[0].description.trim())).toBeVisible();
  await user.click(
    await screen.findByRole("button", { name: "Drawing.pdf · Page 2" }),
  );
  expect(opened).toEqual([
    expect.objectContaining({
      sourceId: "source",
      artifactId: "drawing",
      page: 2,
    }),
  ]);
  await user.click(
    screen.getByRole("button", { name: "Approve & start 6 tasks" }),
  );
  expect(submitted).toEqual(
    expect.objectContaining({
      fingerprint: value.fingerprint,
      engineer_confirmed: true,
    }),
  );
});

it("leads the side panel with meaningful changes and keeps subscription-only detail in More options", async () => {
  const user = userEvent.setup();
  const value = review();
  value.routes[0] = {
    ...value.routes[0],
    provider: "grok_build",
    billing: "subscription",
    data_destination: "Official grok_build client",
    spending_detail: "Subscription allowance",
  };
  value.meaningful_changes = [
    {
      code: "connection_changed",
      detail: "AI account revision changed.",
      before: 4,
      after: 6,
    },
  ];
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async () => new Response(JSON.stringify(value)),
  );
  render(
    <QueryClientProvider
      client={
        new QueryClient({ defaultOptions: { queries: { retry: false } } })
      }
    >
      <ApiContext.Provider value={api}>
        <PlanReview tenderId="one" planId="plan" />
      </ApiContext.Provider>
    </QueryClientProvider>,
  );
  await screen.findByText("AI account revision changed.");
  const aside = document.querySelector(".plan-review-aside")!;
  expect(aside.firstElementChild).toHaveClass("review-changes-card");
  expect(
    within(aside.firstElementChild as HTMLElement).getByText("4", {
      selector: "dd",
    }),
  ).toBeVisible();
  expect(
    within(aside.firstElementChild as HTMLElement).getByText("6", {
      selector: "dd",
    }),
  ).toBeVisible();
  expect(screen.getByText("Data destination: xAI · Grok")).toBeVisible();
  expect(screen.getByText(value.ai_summary)).not.toBeVisible();
  expect(document.querySelector(".review-visible-budget")).toBeNull();
  expect(screen.getByText("USD 1")).not.toBeVisible();
  expect(screen.getByText("Official grok_build client")).not.toBeVisible();
  await user.click(screen.getByText("More options"));
  expect(screen.getByText(value.ai_summary)).toBeVisible();
  expect(screen.getByText("USD 1")).toBeVisible();
  expect(screen.getByText("Official grok_build client")).toBeVisible();
});

it("shows the reviewed delegation and posts a narrowed typed proposal before approving the refreshed fingerprint", async () => {
  const user = userEvent.setup();
  const value = review();
  let current = value;
  let patchBody: unknown;
  let approvalBody: unknown;
  const narrowed = review();
  narrowed.fingerprint = "b".repeat(64);
  narrowed.delegation_proposal_version = 1;
  narrowed.delegation = {
    ...narrowed.delegation!,
    source_scope: "selected_sources",
    artifacts: [],
    max_staff: 3,
    max_assignments: 24,
    max_requests: 2,
    max_search_calls: 0,
  };
  narrowed.delegation_options = {
    ...narrowed.delegation_options!,
    selected: {
      tender_id: "one",
      plan_id: "plan",
      version: 1,
      source_scope: "selected_sources",
      artifact_ids: [],
      tool_ids: ["source.read"],
      allowed_draft_outputs: ["summary", "findings"],
      route_option_ids: ["route-option"],
      max_staff: 3,
      max_assignments: 24,
      max_depth: 2,
      max_concurrency: 2,
      max_requests: 2,
      max_search_calls: 0,
      updated_at: "2026-09-09T10:01:00Z",
    },
  };
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async (url, init) => {
      if (init?.method === "PATCH") {
        patchBody = JSON.parse(String(init.body));
        current = narrowed;
        return new Response(
          JSON.stringify(narrowed.delegation_options!.selected),
        );
      }
      if (init?.method === "POST") {
        approvalBody = JSON.parse(String(init.body));
        return new Response(JSON.stringify({}));
      }
      return new Response(JSON.stringify(current));
    },
  );
  render(
    <QueryClientProvider
      client={
        new QueryClient({ defaultOptions: { queries: { retry: false } } })
      }
    >
      <ApiContext.Provider value={api}>
        <PlanReview tenderId="one" planId="plan" />
      </ApiContext.Provider>
    </QueryClientProvider>,
  );

  expect(await screen.findByText(value.delegation!.purpose)).toBeVisible();
  expect(screen.getByText("All 1 current reviewed source")).toBeVisible();
  expect(screen.getByText("12 staff · 24 assignments")).toBeVisible();
  expect(screen.getAllByText("AI account · Model").length).toBeGreaterThan(0);
  expect(screen.getByText("Allowed Tender tools")).toBeVisible();
  expect(screen.getByText("Summary")).toBeVisible();

  await user.click(
    screen.getByRole("button", { name: "Edit delegation choices" }),
  );
  await user.click(screen.getByRole("radio", { name: /Selected sources/ }));
  await user.click(screen.getByRole("checkbox", { name: /Package\.pdf/ }));
  const staff = screen.getByRole("spinbutton", { name: /Maximum staff/ });
  await user.clear(staff);
  await user.type(staff, "3");
  const requests = screen.getByRole("spinbutton", { name: /Maximum requests/ });
  await user.clear(requests);
  await user.type(requests, "2");
  const depth = screen.getByRole("spinbutton", {
    name: /Maximum delegation depth/,
  });
  await user.clear(depth);
  await user.type(depth, "2");
  const concurrency = screen.getByRole("spinbutton", {
    name: /Simultaneous assignments/,
  });
  await user.clear(concurrency);
  await user.type(concurrency, "2");
  expect(
    screen.getByRole("button", { name: /Approve & start/ }),
  ).toBeDisabled();
  await user.click(
    screen.getByRole("button", { name: "Save delegation choices" }),
  );

  await waitFor(() => expect(patchBody).toBeTruthy());
  expect(patchBody).toEqual({
    expected_version: 0,
    source_scope: "selected_sources",
    artifact_ids: [],
    tool_ids: ["source.read"],
    allowed_draft_outputs: ["summary", "findings"],
    route_option_ids: ["route-option"],
    max_staff: 3,
    max_assignments: 24,
    max_depth: 2,
    max_concurrency: 2,
    max_requests: 2,
    max_search_calls: 0,
  });
  expect(screen.getByText("3 staff · 24 assignments")).toBeVisible();
  expect(screen.getByRole("button", { name: /Approve & start/ })).toBeEnabled();
  await user.click(screen.getByRole("button", { name: /Approve & start/ }));
  await waitFor(() =>
    expect(approvalBody).toEqual(
      expect.objectContaining({ fingerprint: "b".repeat(64) }),
    ),
  );
});

it("retains a local delegation edit after a stale conflict and offers review recovery", async () => {
  const user = userEvent.setup();
  const value = review();
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async (_url, init) => {
      if (init?.method === "PATCH") {
        return new Response(JSON.stringify({ detail: "Delegation changed." }), {
          status: 409,
        });
      }
      return new Response(JSON.stringify(value));
    },
  );
  render(
    <QueryClientProvider
      client={
        new QueryClient({ defaultOptions: { queries: { retry: false } } })
      }
    >
      <ApiContext.Provider value={api}>
        <PlanReview tenderId="one" planId="plan" />
      </ApiContext.Provider>
    </QueryClientProvider>,
  );
  await screen.findByText(value.delegation!.purpose);
  await user.click(
    screen.getByRole("button", { name: "Edit delegation choices" }),
  );
  const staff = screen.getByRole("spinbutton", { name: /Maximum staff/ });
  await user.clear(staff);
  await user.type(staff, "7");
  await user.click(
    screen.getByRole("button", { name: "Save delegation choices" }),
  );
  expect(
    await screen.findByText(/changed while you were editing/),
  ).toBeVisible();
  expect(staff).toHaveValue(7);
  expect(
    screen.getByRole("button", { name: "Reload current review" }),
  ).toBeVisible();
  expect(
    screen.getByRole("button", { name: /Approve & start/ }),
  ).toBeDisabled();
});

it("loads and selects a source from a later artifact page", async () => {
  const user = userEvent.setup();
  const value = review();
  value.delegation_options!.artifacts = Array.from(
    { length: 50 },
    (_, index) => ({
      artifact_id: `source-${index}`,
      name: `Package-${index}.pdf`,
      display_name: `Package-${index}.pdf`,
      relative_path: `Package-${index}.pdf`,
      version: 1,
      content_hash: `${String(index).padStart(2, "0")}${"a".repeat(62)}`,
      status: "current",
    }),
  );
  value.delegation!.artifacts = value.delegation_options!.artifacts!.map(
    (item) => ({
      artifact_id: item.artifact_id,
      version: item.version,
      content_hash: item.content_hash,
    }),
  );
  const later = {
    items: [
      {
        artifact_id: "late-source",
        name: "Later-spec.pdf",
        display_name: "Later-spec.pdf",
        relative_path: "revisions/Later-spec.pdf",
        version: 2,
        content_hash: "b".repeat(64),
        status: "current",
      },
    ],
    next_offset: null,
    total: 51,
  } satisfies Schema<"DelegationArtifactOptionPage">;
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async (url) => {
      if (String(url).includes("/delegation/artifacts"))
        return new Response(JSON.stringify(later));
      return new Response(JSON.stringify(value));
    },
  );
  render(
    <QueryClientProvider
      client={
        new QueryClient({ defaultOptions: { queries: { retry: false } } })
      }
    >
      <ApiContext.Provider value={api}>
        <PlanReview tenderId="one" planId="plan" />
      </ApiContext.Provider>
    </QueryClientProvider>,
  );
  await screen.findByText(value.delegation!.purpose);
  await user.click(
    screen.getByRole("button", { name: "Edit delegation choices" }),
  );
  await user.click(screen.getByRole("radio", { name: /Selected sources/ }));
  await user.click(screen.getByRole("button", { name: "Next sources" }));
  await screen.findByText("Later-spec.pdf");
  await user.click(screen.getByRole("checkbox", { name: /Later-spec\.pdf/ }));
  expect(screen.getByText("51 selected")).toBeVisible();
});

it("shows historical task-only reviews without synthesizing delegation scope", async () => {
  const value = review();
  value.plan_status = "approved";
  value.delegation = null;
  value.delegation_options = null;
  value.delegation_proposal_version = 0;
  value.can_approve = false;
  value.blockers = [
    {
      code: "plan",
      detail: "This approved task plan is historical only.",
      repair_target: "/tenders/one/work",
    },
  ];
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async () => new Response(JSON.stringify(value)),
  );
  render(
    <QueryClientProvider
      client={
        new QueryClient({ defaultOptions: { queries: { retry: false } } })
      }
    >
      <ApiContext.Provider value={api}>
        <PlanReview tenderId="one" planId="plan" />
      </ApiContext.Provider>
    </QueryClientProvider>,
  );
  expect(await screen.findByText(/has no delegation scope/)).toBeVisible();
  expect(
    screen.queryByRole("button", { name: "Edit delegation choices" }),
  ).toBeNull();
  expect(
    screen.getByRole("button", { name: /Approve & start/ }),
  ).toBeDisabled();
});

it("removes an allowed output, saves it, reopens the actual review, and re-adds it explicitly", async () => {
  const user = userEvent.setup();
  const initial = review();
  let current = initial;
  const patchBodies: unknown[] = [];
  const saved = review();
  saved.fingerprint = "b".repeat(64);
  saved.delegation_proposal_version = 1;
  saved.delegation = {
    ...saved.delegation!,
    allowed_draft_outputs: ["findings"],
  };
  saved.delegation_options = {
    ...saved.delegation_options!,
    supported_draft_outputs: [...SUPPORTED_DRAFT_OUTPUTS],
    selected: {
      tender_id: "one",
      plan_id: "plan",
      version: 1,
      source_scope: "reviewed_tender",
      artifact_ids: ["source"],
      tool_ids: ["source.read"],
      allowed_draft_outputs: ["findings"],
      route_option_ids: ["route-option"],
      max_staff: 12,
      max_assignments: 24,
      max_depth: 1,
      max_concurrency: 1,
      max_requests: 4,
      max_search_calls: 0,
      updated_at: "2026-09-09T10:01:00Z",
    },
  };
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async (_url, init) => {
      if (init?.method === "PATCH") {
        patchBodies.push(JSON.parse(String(init.body)));
        current =
          patchBodies.length === 1
            ? saved
            : {
                ...saved,
                delegation_options: {
                  ...saved.delegation_options!,
                  selected: {
                    ...saved.delegation_options!.selected!,
                    allowed_draft_outputs: ["findings", "summary"],
                  },
                },
                delegation: {
                  ...saved.delegation!,
                  allowed_draft_outputs: ["findings", "summary"],
                },
                delegation_proposal_version: 2,
                fingerprint: "c".repeat(64),
              };
        return new Response(
          JSON.stringify(current.delegation_options!.selected),
        );
      }
      return new Response(JSON.stringify(current));
    },
  );
  render(
    <QueryClientProvider
      client={
        new QueryClient({ defaultOptions: { queries: { retry: false } } })
      }
    >
      <ApiContext.Provider value={api}>
        <PlanReview tenderId="one" planId="plan" />
      </ApiContext.Provider>
    </QueryClientProvider>,
  );

  await screen.findByText(initial.delegation!.purpose);
  await user.click(
    screen.getByRole("button", { name: "Edit delegation choices" }),
  );
  const summary = screen.getByRole("checkbox", { name: "Summary" });
  expect(summary).toBeChecked();
  await user.click(summary);
  expect(summary).not.toBeChecked();
  await user.click(
    screen.getByRole("button", { name: "Save delegation choices" }),
  );
  await waitFor(() => expect(patchBodies).toHaveLength(1));
  expect(patchBodies[0]).toEqual(
    expect.objectContaining({
      expected_version: 0,
      allowed_draft_outputs: ["findings"],
    }),
  );

  await user.click(
    screen.getByRole("button", { name: "Edit delegation choices" }),
  );
  expect(screen.getByRole("checkbox", { name: "Summary" })).not.toBeChecked();
  await user.click(screen.getByRole("checkbox", { name: "Summary" }));
  await user.click(
    screen.getByRole("button", { name: "Save delegation choices" }),
  );
  await waitFor(() => expect(patchBodies).toHaveLength(2));
  expect(patchBodies[1]).toEqual(
    expect.objectContaining({
      expected_version: 1,
      allowed_draft_outputs: ["findings", "summary"],
    }),
  );
});

it("saves a valid source selection when the delegation envelope is null but the typed options are present", async () => {
  const user = userEvent.setup();
  const initial = review();
  initial.delegation = null;
  initial.delegation_options = {
    ...initial.delegation_options!,
    supported_draft_outputs: [...SUPPORTED_DRAFT_OUTPUTS],
    selected: null,
  };
  initial.can_approve = false;
  const saved = review();
  saved.delegation = null;
  saved.delegation_options = {
    ...initial.delegation_options!,
    selected: {
      tender_id: "one",
      plan_id: "plan",
      version: 1,
      source_scope: "selected_sources",
      artifact_ids: ["source"],
      tool_ids: ["source.read"],
      allowed_draft_outputs: [...SUPPORTED_DRAFT_OUTPUTS],
      route_option_ids: ["route-option"],
      max_staff: 6,
      max_assignments: 24,
      max_depth: 1,
      max_concurrency: 1,
      max_requests: 4,
      max_search_calls: 0,
      updated_at: "2026-09-09T10:01:00Z",
    },
  };
  saved.delegation_proposal_version = 1;
  saved.can_approve = false;
  let body: unknown;
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async (_url, init) => {
      if (init?.method === "PATCH") {
        body = JSON.parse(String(init.body));
        return new Response(JSON.stringify(saved.delegation_options!.selected));
      }
      return new Response(JSON.stringify(body ? saved : initial));
    },
  );
  render(
    <QueryClientProvider
      client={
        new QueryClient({ defaultOptions: { queries: { retry: false } } })
      }
    >
      <ApiContext.Provider value={api}>
        <PlanReview tenderId="one" planId="plan" />
      </ApiContext.Provider>
    </QueryClientProvider>,
  );

  expect(await screen.findByText(/Whole current Tender/)).toBeVisible();
  await user.click(
    screen.getByRole("button", { name: "Edit delegation choices" }),
  );
  await user.click(screen.getByRole("radio", { name: /Selected sources/ }));
  await user.click(screen.getByRole("checkbox", { name: /Package\.pdf/ }));
  const staff = screen.getByRole("spinbutton", { name: /Maximum staff/ });
  await user.clear(staff);
  await user.type(staff, "6");
  await user.click(
    screen.getByRole("button", { name: "Save delegation choices" }),
  );
  await waitFor(() =>
    expect(body).toEqual(
      expect.objectContaining({
        expected_version: 0,
        source_scope: "selected_sources",
        artifact_ids: ["source"],
        max_staff: 6,
      }),
    ),
  );
});

it("includes later artifact pages when switching a selected source review to whole Tender scope", async () => {
  const user = userEvent.setup();
  const initial = review();
  const artifacts = Array.from({ length: 50 }, (_, index) => ({
    artifact_id: `source-${index}`,
    name: `Package-${index}.pdf`,
    display_name: `Package-${index}.pdf`,
    relative_path: `Package-${index}.pdf`,
    version: 1,
    content_hash: `${String(index).padStart(2, "0")}${"a".repeat(62)}`,
    status: "current",
  }));
  const firstPage = {
    items: artifacts,
    next_offset: 50,
    total: 51,
  } satisfies Schema<"DelegationArtifactOptionPage">;
  const secondPage = {
    items: [
      {
        artifact_id: "late-source",
        name: "Later-spec.pdf",
        display_name: "Later-spec.pdf",
        relative_path: "revisions/Later-spec.pdf",
        version: 2,
        content_hash: "b".repeat(64),
        status: "current",
      },
    ],
    next_offset: null,
    total: 51,
  } satisfies Schema<"DelegationArtifactOptionPage">;
  initial.delegation = {
    ...initial.delegation!,
    source_scope: "selected_sources",
    artifacts: [
      {
        artifact_id: "source-0",
        version: 1,
        content_hash: artifacts[0].content_hash,
      },
    ],
  };
  initial.delegation_options = {
    ...initial.delegation_options!,
    artifacts,
    supported_draft_outputs: [...SUPPORTED_DRAFT_OUTPUTS],
  };
  let body: unknown;
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async (url, init) => {
      if (init?.method === "PATCH") {
        body = JSON.parse(String(init.body));
        return new Response(JSON.stringify({}));
      }
      const path = String(url);
      if (path.includes("delegation/artifacts?offset=50"))
        return new Response(JSON.stringify(secondPage));
      if (path.includes("delegation/artifacts?offset=0"))
        return new Response(JSON.stringify(firstPage));
      return new Response(JSON.stringify(initial));
    },
  );
  render(
    <QueryClientProvider
      client={
        new QueryClient({ defaultOptions: { queries: { retry: false } } })
      }
    >
      <ApiContext.Provider value={api}>
        <PlanReview tenderId="one" planId="plan" />
      </ApiContext.Provider>
    </QueryClientProvider>,
  );

  await screen.findByText(initial.delegation!.purpose);
  await user.click(
    screen.getByRole("button", { name: "Edit delegation choices" }),
  );
  await user.click(
    screen.getByRole("radio", { name: /Whole reviewed Tender/ }),
  );
  await waitFor(() =>
    expect(screen.getByText("All 51 current reviewed sources")).toBeVisible(),
  );
  await user.click(
    screen.getByRole("button", { name: "Save delegation choices" }),
  );
  await waitFor(() =>
    expect(body).toEqual(
      expect.objectContaining({
        source_scope: "reviewed_tender",
        artifact_ids: expect.arrayContaining(["source-0", "late-source"]),
      }),
    ),
  );
  expect((body as { artifact_ids: string[] }).artifact_ids).toHaveLength(51);
});

it("keeps a current approved delegation scope read-only", async () => {
  const value = review();
  value.plan_status = "approved";
  value.can_approve = false;
  value.blockers = [
    {
      code: "plan",
      detail: "This work plan is already approved.",
      repair_target: "/tenders/one/work",
    },
  ];
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async () => new Response(JSON.stringify(value)),
  );
  render(
    <QueryClientProvider
      client={
        new QueryClient({ defaultOptions: { queries: { retry: false } } })
      }
    >
      <ApiContext.Provider value={api}>
        <PlanReview tenderId="one" planId="plan" />
      </ApiContext.Provider>
    </QueryClientProvider>,
  );
  expect(await screen.findByText("Approved scope")).toBeVisible();
  expect(
    screen.queryByRole("button", { name: "Edit delegation choices" }),
  ).toBeNull();
  expect(screen.getByText(/approved delegation limits/)).toBeVisible();
});

it("keeps a superseded delegation scope read-only and labels the historical guidance", async () => {
  const value = review();
  value.plan_status = "superseded";
  value.can_approve = false;
  value.blockers = [
    {
      code: "plan",
      detail: "This work plan was superseded.",
      repair_target: "/tenders/one/work",
    },
  ];
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async () => new Response(JSON.stringify(value)),
  );
  render(
    <QueryClientProvider
      client={
        new QueryClient({ defaultOptions: { queries: { retry: false } } })
      }
    >
      <ApiContext.Provider value={api}>
        <PlanReview tenderId="one" planId="plan" />
      </ApiContext.Provider>
    </QueryClientProvider>,
  );
  expect(await screen.findByText("Superseded review")).toBeVisible();
  expect(
    screen.queryByRole("button", { name: "Edit delegation choices" }),
  ).toBeNull();
  expect(
    screen.getByText(/superseded and cannot authorize new assignments/),
  ).toBeVisible();
});
