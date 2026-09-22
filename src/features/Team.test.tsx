import { render, screen, within } from "@testing-library/react";
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
      if (path.endsWith("/activity"))
        return Response.json({
          items: [],
          has_more: false,
          has_earlier: false,
          reset_required: false,
          run_status: "running",
          run_detail: "",
          run_updated_at: "",
          history_key: "h",
        });
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

it("lists the Manager and every colleague with what they are doing or last did", async () => {
  const user = userEvent.setup();
  renderTeam();
  const team = await screen.findByRole("list", { name: "Team" });
  const rows = within(team).getAllByRole("listitem");
  expect(rows[0]).toHaveTextContent("Tender Manager");
  expect(rows[0]).toHaveTextContent("Ready for your next request");
  expect(team).toHaveTextContent(
    "Asked the Tender Manager: Is night work allowed?",
  );
  expect(rows[1]).toHaveTextContent("Samir Haddad · Senior Quantity Surveyor");
  expect(rows[1]).toHaveTextContent("Finished: Ground slab is C30/37, 250 mm.");
  expect(screen.queryByText(/model-a|tokens|requests/)).not.toBeInTheDocument();

  await user.click(
    within(rows[1]).getByRole("button", { name: /Samir Haddad/ }),
  );
  const card = screen.getByRole("article", { name: "Samir Haddad" });
  expect(
    within(card).getByText("Fifteen years pricing public buildings."),
  ).toBeVisible();
  const work = within(card).getByRole("list", {
    name: "Work for Samir Haddad",
  });
  await user.click(within(work).getByText("Check the slab"));
  expect(
    within(card).getByText("Ground slab is C30/37, 250 mm."),
  ).toBeVisible();
  expect(
    await within(card).findByRole("button", { name: /Concrete spec.pdf/ }),
  ).toBeVisible();
});

it("keeps finished and stopped work behind Past work", async () => {
  const user = userEvent.setup();
  renderTeam();
  const toggle = await screen.findByRole("button", { name: "Past work (1)" });
  expect(
    screen.queryByRole("list", { name: "Past work" }),
  ).not.toBeInTheDocument();
  await user.click(toggle);
  expect(
    within(screen.getByRole("list", { name: "Past work" })).getByText(
      "Check the slab",
    ),
  ).toBeVisible();
});
