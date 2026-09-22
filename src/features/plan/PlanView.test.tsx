import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ApiContext, createApi, type Schema } from "../../api";
import { PlanView } from "./PlanView";

const overview = {
  tender: {
    id: "one",
    name: "Fire Station",
    status: "active",
    revision: 1,
    created_at: "",
    updated_at: "",
    name_source: "engineer",
  },
  artifact_count: 20,
  evidence_count: 0,
  coverage: {
    registered: 20,
    extracted: 19,
    needs_attention: 1,
    unsupported: 0,
    failed: 0,
  },
  areas: [],
  findings: [],
  plan: null,
  active_runs: [],
  boq_count: 0,
} as unknown as Schema<"Overview">;

function renderPlan(brief: unknown, waiting: unknown) {
  const requests: string[] = [];
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async (url, init) => {
      const path = new URL(String(url)).pathname;
      requests.push(`${init?.method ?? "GET"} ${path}`);
      return new Response(
        JSON.stringify(
          path.endsWith("/work-brief")
            ? brief
            : path.endsWith("/waiting")
              ? waiting
              : {},
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
        <PlanView overview={overview} />
      </ApiContext.Provider>
    </QueryClientProvider>,
  );
  return requests;
}

const longNote =
  "Two bounded text assignments returned no usable content (work limit) and the booklet text extracts in a legacy font encoding, so keyword search of it is unreliable. Needs an engineer decision on a bounded image-based pass.";

it("shows the live plan as full-width steps with owner and status on one line", async () => {
  const user = userEvent.setup();
  const requests = renderPlan(
    {
      brief: {
        outcome: "Review the package for the site visit and scope.",
        status: "in_progress",
        next_step: "Check clarification round 02.",
        created_at: "2026-09-15T10:04:00Z",
        steps: [
          {
            title: "Map the package",
            state: "done",
            note: "20 documents in 6 groups",
            owner: "Tender Manager",
          },
          {
            title: "Sweep the 92-page technical booklet",
            state: "blocked",
            note: longNote,
            owner: "Yousef Al-Dosari / Layla Haddad",
          },
          {
            title: "Take off BOQ quantities",
            state: "to_do",
            owner: "Khalid Bin Salem",
          },
        ],
      },
    },
    { total: 3, items: [] },
  );
  const steps = await screen.findByRole("list", { name: "Plan steps" });
  const rows = within(steps).getAllByRole("listitem");
  expect(rows[0]).toHaveTextContent("Map the package");
  expect(
    within(rows[0]).getByText("Tender Manager · done"),
  ).toBeInTheDocument();
  expect(
    within(rows[1]).getByText("Yousef Al-Dosari / Layla Haddad · blocked"),
  ).toBeInTheDocument();
  expect(
    within(rows[2]).getByText("Khalid Bin Salem · not started"),
  ).toBeInTheDocument();
  await user.click(within(rows[1]).getByRole("button", { name: "more" }));
  expect(within(rows[1]).getByRole("button", { name: "less" })).toHaveAttribute(
    "aria-expanded",
    "true",
  );
  expect(
    screen.getByText("Next: Check clarification round 02."),
  ).toBeInTheDocument();
  // Nothing is approved from the panel; it points to the chat.
  expect(
    screen.getByText("3 things are waiting for you in the chat."),
  ).toBeInTheDocument();
  expect(
    screen.queryByRole("button", { name: /Approve/ }),
  ).not.toBeInTheDocument();
  expect(requests.every((request) => request.startsWith("GET"))).toBe(true);
});

it("says plainly when there is no plan", async () => {
  renderPlan({ brief: null }, { items: [], total: 0 });
  expect(
    await screen.findByText(
      "No plan yet. The Tender Manager writes one when it starts a real job.",
    ),
  ).toBeInTheDocument();
  expect(screen.queryByText(/waiting for you/)).not.toBeInTheDocument();
});
