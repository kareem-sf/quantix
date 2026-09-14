import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ApiContext, createApi, type Schema } from "../api";
import { Team } from "./Team";

const staff = (
  id: string,
  name: string,
  role: string,
): Schema<"StaffMember"> => ({
  id,
  tender_id: "one",
  name,
  role,
  specialisms: ["BOQ audit"],
  background: "Fifteen years pricing public buildings.",
  working_style: "Checks every quantity twice.",
  status: "active",
  portrait_seed: `seed-${id}`,
  created_run_id: "run-1",
  created_at: "2026-09-14T08:00:00Z",
  updated_at: "2026-09-14T08:00:00Z",
});

const assignment = (
  id: string,
  staffId: string,
  changes: Partial<Schema<"Assignment">>,
): Schema<"Assignment"> => ({
  id,
  tender_id: "one",
  run_id: "run-1",
  staff_id: staffId,
  title: "Check the slab",
  brief: "Report the ground slab grade.",
  expected_result: "Grade and thickness with sources",
  source_ids: [],
  connection_id: "account",
  model_id: "model-a",
  status: "queued",
  detail: "",
  usage: {},
  created_at: "2026-09-14T08:00:00Z",
  updated_at: "2026-09-14T08:05:00Z",
  ...changes,
});

function renderTeam(managerRunId?: string) {
  const posts: Array<{ path: string; body: unknown }> = [];
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async (address, options) => {
      const path = new URL(String(address)).pathname;
      if (options?.method === "POST") {
        posts.push({ path, body: JSON.parse(String(options.body ?? "null")) });
        return Response.json({
          id: "instruction",
          root_id: "run-1",
          kind: "constraint",
          state: "admitted",
          replayed: false,
          created_at: "",
        });
      }
      if (path.endsWith("/work-brief")) return Response.json({ brief: null });
      if (path.endsWith("/evidence/source-1"))
        return Response.json({
          id: "source-1",
          artifact_id: "spec",
          artifact_name: "Concrete spec.pdf",
          relative_path: "Specs/Concrete spec.pdf",
          locator: "page 2",
          page: 2,
          text: "C30/37",
          kind: "text",
          metadata: {},
          score: 0,
        });
      if (path.endsWith("/team"))
        return Response.json({
          staff: [
            staff("samir", "Samir Haddad", "Senior Quantity Surveyor"),
            staff("lina", "Lina Farouk", "Planning Engineer"),
          ],
          assignments: [
            assignment("a-2", "lina", {
              title: "Draft the programme",
              status: "waiting",
              question: "Is night work allowed?",
            }),
            assignment("a-1", "samir", {
              status: "completed",
              result: {
                summary: "Ground slab is C30/37, 250 mm.",
                findings: [
                  {
                    title: "Slab grade",
                    detail: "C30/37 applies.",
                    kind: "requirement",
                    source_ids: ["source-1"],
                  },
                ],
                source_ids: [],
              },
              usage: { requests: 2, input_tokens: 900, output_tokens: 100 },
            }),
          ],
        } satisfies Schema<"TeamView">);
      throw new Error(`Unexpected request: ${path}`);
    },
  );
  render(
    <QueryClientProvider
      client={
        new QueryClient({ defaultOptions: { queries: { retry: false } } })
      }
    >
      <ApiContext.Provider value={api}>
        <Team tenderId="one" managerRunId={managerRunId} onSource={vi.fn()} />
      </ApiContext.Provider>
    </QueryClientProvider>,
  );
  return posts;
}

it("shows every staff member, their work, questions and cited results", async () => {
  const user = userEvent.setup();
  renderTeam();
  const roster = await screen.findByRole("list", { name: "Staff" });
  expect(within(roster).getByText("Samir Haddad")).toBeVisible();
  expect(within(roster).getByText("1 open · 1 total")).toBeVisible();
  const work = screen.getByRole("list", { name: "Assignments" });
  expect(within(work).getByText("Is night work allowed?")).toBeVisible();
  await user.click(within(work).getByText("Check the slab"));
  expect(screen.getByText("Ground slab is C30/37, 250 mm.")).toBeVisible();
  expect(
    await screen.findByRole("button", { name: /Concrete spec.pdf/ }),
  ).toBeVisible();
  expect(screen.getByText("model-a · 2 requests · 1,000 tokens")).toBeVisible();
});

it("filters the work to one staff member", async () => {
  const user = userEvent.setup();
  renderTeam();
  await user.click(await screen.findByRole("button", { name: /Samir Haddad/ }));
  const work = screen.getByRole("list", { name: "Assignments" });
  expect(
    within(work).queryByText("Draft the programme"),
  ).not.toBeInTheDocument();
  expect(screen.getByText("Work for Samir Haddad")).toBeVisible();
  await user.click(screen.getByRole("button", { name: "Show all" }));
  expect(within(work).getByText("Draft the programme")).toBeVisible();
});

it("steers the running Manager", async () => {
  const user = userEvent.setup();
  const posts = renderTeam("run-1");
  await user.type(
    await screen.findByLabelText("Steer the Manager while it works"),
    "Use revision C drawings only",
  );
  await user.click(screen.getByRole("button", { name: "Send" }));
  await waitFor(() => expect(posts).toHaveLength(1));
  expect(posts[0].path).toBe("/api/tenders/one/runs/run-1/steering");
  expect(posts[0].body).toMatchObject({
    kind: "constraint",
    content: "Use revision C drawings only",
  });
  expect(await screen.findByText(/applies it at its next step/)).toBeVisible();
});
