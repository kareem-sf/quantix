import {
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { ApiContext, createApi } from "../api";
import { AgentLibrary } from "./AgentLibrary";

const generationSettings = {
  temperature: 0.2,
  top_p: 0.9,
  reasoning: "medium",
  max_output_tokens: 4096,
  output_mode: "auto" as const,
  native_tools: [],
  max_search_calls: 3,
  max_native_tool_calls: 3,
};

const profile = {
  display_name: "Amina Hassan",
  role: "Quantity surveying",
  title: "Senior Quantity Surveyor",
  specialisms: ["Bills of quantities"],
  persona: "A practical quantity surveyor who keeps commercial work auditable.",
  personality: {
    description: "A careful quantity surveying professional.",
    traits: ["careful"],
    communication_style: "Use plain construction language.",
    problem_solving_style: "Check quantities and rates separately.",
    collaboration_style: "Share traceable working notes.",
    uncertainty_handling: "Label every allowance and gap.",
    initiative: "Raise material commercial risks early.",
    explanation_style: "Lead with the decision needed.",
    language_preferences: ["English"],
    working_habits: ["Keep units with quantities"],
  },
  responsibilities: ["Check quantities"],
  objectives: ["Give the Manager a traceable commercial position"],
  methods: ["Reconcile each item against current evidence"],
  deliverables: ["Checked quantity and rate schedule"],
  success_criteria: ["Every quantity and rate has a stated basis"],
  context_needs: ["Current Tender documents"],
  requested_tool_ids: ["read_source"],
  creation_reason: "Reusable quantity-surveying support for Tender work.",
};

function definition(overrides: Record<string, unknown> = {}) {
  return {
    id: "definition-1",
    version: 1,
    lifecycle: "active",
    profile,
    generation_settings: generationSettings,
    fingerprint: "a".repeat(64),
    source_definition_id: null,
    source_definition_version: null,
    created_at: "2026-09-12T10:00:00Z",
    updated_at: "2026-09-12T10:00:00Z",
    ...overrides,
  };
}

function tender() {
  return {
    id: "tender-1",
    name: "School extension",
    status: "open",
    revision: 1,
    created_at: "2026-09-12T10:00:00Z",
    updated_at: "2026-09-12T10:00:00Z",
  };
}

function setup(fetcher: typeof fetch) {
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    fetcher,
  );
  const user = userEvent.setup({ delay: null });
  render(
    <ApiContext.Provider value={api}>
      <AgentLibrary />
    </ApiContext.Provider>,
  );
  return user;
}

it("starts with no seeded roster and creates a complete manual definition", async () => {
  const writes: unknown[] = [];
  const user = setup(async (url, init) => {
    const parsed = new URL(String(url));
    if (
      parsed.pathname === "/api/agent-definitions" &&
      init?.method === "POST"
    ) {
      const body = JSON.parse(String(init.body));
      writes.push(body);
      return Response.json(
        definition({
          profile: body.profile,
          generation_settings: body.generation_settings,
        }),
      );
    }
    if (parsed.pathname === "/api/agent-definitions") return Response.json([]);
    if (parsed.pathname === "/api/tenders") return Response.json([]);
    return Response.json({ detail: "Unexpected request" }, { status: 404 });
  });

  expect(await screen.findByText("No saved professionals yet")).toBeVisible();
  await user.click(
    screen.getByRole("button", { name: "Create professional manually" }),
  );
  expect(screen.getByRole("dialog")).toBeVisible();

  const values: Record<string, string> = {
    "Display name": "Amina Hassan",
    Role: "Quantity surveying",
    Title: "Senior Quantity Surveyor",
    "Professional description": "A practical commercial professional.",
    "How this professional works": "Careful and evidence-led.",
    "Communication style": "Use plain construction language.",
    "Problem-solving style": "Check quantities and rates separately.",
    "Collaboration style": "Share traceable working notes.",
    "Uncertainty handling": "Label every allowance and gap.",
    Initiative: "Raise material risks early.",
    "Explanation style": "Lead with the decision needed.",
    Specialisms: "Bills of quantities",
    Responsibilities: "Check quantities",
    Objectives: "Give the Manager a traceable commercial position",
    Methods: "Reconcile each item against current evidence",
    Deliverables: "Checked quantity and rate schedule",
    "Success criteria": "Every quantity and rate has a stated basis",
    "Context needed": "Current Tender documents",
    "Creation reason": "Reusable quantity-surveying support.",
  };
  for (const [label, value] of Object.entries(values)) {
    fireEvent.change(screen.getByLabelText(label), { target: { value } });
  }
  fireEvent.change(screen.getByLabelText("Requested tools"), {
    target: { value: "read_source\ncalculate" },
  });
  await user.click(screen.getByRole("button", { name: "Create professional" }));

  await waitFor(() => expect(writes).toHaveLength(1));
  expect(writes[0]).toEqual({
    profile: expect.objectContaining({
      display_name: "Amina Hassan",
      requested_tool_ids: ["read_source", "calculate"],
    }),
    generation_settings: expect.objectContaining({
      output_mode: "auto",
      max_output_tokens: 8192,
    }),
    idempotency_key: expect.any(String),
  });
  expect(await screen.findByText("Amina Hassan")).toBeVisible();
});

it("opens and focuses the first missing required profile option", async () => {
  const user = setup(async (url) => {
    const parsed = new URL(String(url));
    if (parsed.pathname === "/api/agent-definitions") return Response.json([]);
    if (parsed.pathname === "/api/tenders") return Response.json([]);
    return Response.json({ detail: "Unexpected request" }, { status: 404 });
  });
  await screen.findByText("No saved professionals yet");
  await user.click(
    screen.getByRole("button", { name: "Create professional manually" }),
  );
  fireEvent.change(screen.getByLabelText("Display name"), {
    target: { value: "Amina Hassan" },
  });
  fireEvent.change(screen.getByLabelText("Role"), {
    target: { value: "Quantity surveying" },
  });
  fireEvent.change(screen.getByLabelText("Title"), {
    target: { value: "Senior Quantity Surveyor" },
  });
  fireEvent.change(screen.getByLabelText("Professional description"), {
    target: { value: "A practical commercial professional." },
  });
  fireEvent.change(screen.getByLabelText("How this professional works"), {
    target: { value: "Careful and evidence-led." },
  });
  await user.click(screen.getByRole("button", { name: "Create professional" }));

  expect(await screen.findByRole("alert")).toHaveTextContent(
    "Complete Specialisms before saving this professional.",
  );
  expect(screen.getByLabelText("Specialisms")).toHaveFocus();
  expect(
    screen.getByText(/More profile options.*required/).closest("details"),
  ).toHaveAttribute("open");
});

it("sends generation and exact-version reuse through real Tender Manager tasks", async () => {
  const managerMessages: Array<{
    path: string;
    body: Record<string, unknown>;
  }> = [];
  const user = setup(async (url, init) => {
    const parsed = new URL(String(url));
    if (parsed.pathname === "/api/agent-definitions")
      return Response.json(
        managerMessages.length
          ? [
              definition(),
              definition({
                id: "definition-mep",
                profile: { ...profile, display_name: "MEP estimator" },
              }),
            ]
          : [definition()],
      );
    if (parsed.pathname === "/api/tenders") return Response.json([tender()]);
    if (parsed.pathname === "/api/tenders/tender-1/messages") {
      managerMessages.push({
        path: parsed.pathname,
        body: JSON.parse(String(init?.body)),
      });
      return Response.json({});
    }
    return Response.json({ detail: "Unexpected request" }, { status: 404 });
  });

  expect(await screen.findByText("Amina Hassan")).toBeVisible();
  await user.click(screen.getByRole("button", { name: "Generate from brief" }));
  await user.selectOptions(screen.getByLabelText("Tender"), "tender-1");
  await user.type(
    screen.getByLabelText("Professional brief"),
    "Create a senior MEP estimator for design-and-build tenders.",
  );
  await user.click(screen.getByRole("button", { name: "Ask Tender Manager" }));
  expect(
    await screen.findByText(
      "The Tender Manager is creating the professional definition.",
    ),
  ).toBeVisible();
  await user.click(
    screen.getByRole("button", { name: "Refresh professionals" }),
  );
  expect(await screen.findByText("MEP estimator")).toBeVisible();

  await user.click(
    screen.getByRole("button", { name: "Use Amina Hassan for a Tender" }),
  );
  await user.selectOptions(screen.getByLabelText("Tender"), "tender-1");
  await user.type(
    screen.getByLabelText("First work brief"),
    "Check the priced bill and report material gaps.",
  );
  await user.click(screen.getByRole("button", { name: "Ask Tender Manager" }));

  await waitFor(() => expect(managerMessages).toHaveLength(2));
  expect(managerMessages[0].body).toEqual({
    content: expect.stringContaining("senior MEP estimator"),
    action: "review_documents",
    idempotency_key: expect.any(String),
  });
  expect(managerMessages[1].body).toEqual({
    content: expect.stringMatching(
      /definition-1[\s\S]*version 1[\s\S]*priced bill/i,
    ),
    action: "review_documents",
    idempotency_key: expect.any(String),
  });
  for (const message of managerMessages) {
    expect(message.body).not.toHaveProperty("connection_id");
    expect(message.body).not.toHaveProperty("credentials");
    expect(message.body).not.toHaveProperty("grant");
    expect(message.body).not.toHaveProperty("source_ids");
  }
});

it("edits by expected version, exposes history, and exports only the portable profile", async () => {
  const writes: Array<{ path: string; method: string; body?: unknown }> = [];
  const click = vi
    .spyOn(HTMLAnchorElement.prototype, "click")
    .mockImplementation(() => undefined);
  const createObjectURL = vi
    .spyOn(URL, "createObjectURL")
    .mockReturnValue("blob:definition");
  const revokeObjectURL = vi
    .spyOn(URL, "revokeObjectURL")
    .mockImplementation(() => undefined);
  const user = setup(async (url, init) => {
    const parsed = new URL(String(url));
    writes.push({
      path: parsed.pathname + parsed.search,
      method: init?.method ?? "GET",
      body: init?.body ? JSON.parse(String(init.body)) : undefined,
    });
    if (parsed.pathname === "/api/agent-definitions/definition-1/versions")
      return Response.json([
        definition({
          version: 2,
          profile: { ...profile, display_name: "Amina updated" },
        }),
        definition(),
      ]);
    if (parsed.pathname === "/api/agent-definitions/definition-1/export")
      return Response.json({
        schema_version: 1,
        definition_id: "definition-1",
        version: 1,
        fingerprint: "a".repeat(64),
        profile,
        generation_settings: generationSettings,
      });
    if (
      parsed.pathname === "/api/agent-definitions/definition-1" &&
      init?.method === "PATCH"
    ) {
      const body = JSON.parse(String(init.body));
      return Response.json(definition({ version: 2, profile: body.profile }));
    }
    if (parsed.pathname === "/api/agent-definitions")
      return Response.json([definition()]);
    if (parsed.pathname === "/api/tenders") return Response.json([]);
    return Response.json({ detail: "Unexpected request" }, { status: 404 });
  });

  expect(await screen.findByText("Amina Hassan")).toBeVisible();
  await user.click(
    screen.getByRole("button", { name: "Show version history" }),
  );
  expect(await screen.findByText("Amina updated · version 2")).toBeVisible();
  expect(screen.getByText("Amina Hassan · version 1")).toBeVisible();

  await user.click(screen.getByRole("button", { name: "Export Amina Hassan" }));
  await waitFor(() => expect(click).toHaveBeenCalledTimes(1));
  expect(createObjectURL).toHaveBeenCalledTimes(1);
  expect(revokeObjectURL).toHaveBeenCalledWith("blob:definition");

  await user.click(screen.getByRole("button", { name: "Edit Amina Hassan" }));
  const dialog = screen.getByRole("dialog");
  fireEvent.change(within(dialog).getByLabelText("Display name"), {
    target: { value: "Amina updated" },
  });
  await user.click(
    within(dialog).getByRole("button", { name: "Save new version" }),
  );
  await waitFor(() =>
    expect(writes).toContainEqual({
      path: "/api/agent-definitions/definition-1",
      method: "PATCH",
      body: expect.objectContaining({
        expected_version: 1,
        profile: expect.objectContaining({ display_name: "Amina updated" }),
        idempotency_key: expect.any(String),
      }),
    }),
  );
  expect(await screen.findByText("Amina updated")).toBeVisible();

  click.mockRestore();
  createObjectURL.mockRestore();
  revokeObjectURL.mockRestore();
});

it("duplicates an exact version and requires confirmation before retirement", async () => {
  const mutations: Array<{ path: string; body: Record<string, unknown> }> = [];
  let duplicateAttempts = 0;
  const user = setup(async (url, init) => {
    const parsed = new URL(String(url));
    if (init?.method === "POST") {
      const body = JSON.parse(String(init.body));
      mutations.push({ path: parsed.pathname, body });
      if (parsed.pathname.endsWith("/duplicate")) {
        duplicateAttempts += 1;
        if (duplicateAttempts === 1)
          return Response.json(
            { detail: "The local service response was interrupted." },
            { status: 503 },
          );
        return Response.json(
          definition({
            id: "definition-copy",
            profile: { ...profile, display_name: body.display_name },
            source_definition_id: "definition-1",
            source_definition_version: 1,
          }),
        );
      }
      if (parsed.pathname.endsWith("/retire"))
        return Response.json(definition({ lifecycle: "retired" }));
    }
    if (parsed.pathname === "/api/agent-definitions")
      return Response.json([definition()]);
    if (parsed.pathname === "/api/tenders") return Response.json([]);
    return Response.json({ detail: "Unexpected request" }, { status: 404 });
  });

  expect(await screen.findByText("Amina Hassan")).toBeVisible();
  await user.click(
    screen.getByRole("button", { name: "Duplicate Amina Hassan" }),
  );
  const copyName = screen.getByLabelText("Copy name");
  await user.clear(copyName);
  await user.type(copyName, "Amina commercial review");
  await user.click(screen.getByRole("button", { name: "Create copy" }));
  expect(
    (
      await screen.findAllByText("The local service response was interrupted.")
    )[0],
  ).toBeVisible();
  await user.click(screen.getByRole("button", { name: "Create copy" }));
  expect(await screen.findByText("Amina commercial review")).toBeVisible();

  await user.click(screen.getByRole("button", { name: "Retire Amina Hassan" }));
  expect(
    screen.getByText(/Existing Tender work keeps its saved version/),
  ).toBeVisible();
  await user.click(screen.getByRole("button", { name: "Retire professional" }));
  await waitFor(() => expect(mutations).toHaveLength(3));
  expect(mutations[0]).toEqual({
    path: "/api/agent-definitions/definition-1/duplicate",
    body: {
      source_version: 1,
      display_name: "Amina commercial review",
      idempotency_key: expect.any(String),
    },
  });
  expect(mutations[1]).toEqual(mutations[0]);
  expect(mutations[2]).toEqual({
    path: "/api/agent-definitions/definition-1/retire",
    body: { expected_version: 1, idempotency_key: expect.any(String) },
  });
  expect(screen.queryByText("Amina Hassan")).not.toBeInTheDocument();
});
