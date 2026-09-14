import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ApiContext, createApi, type Schema } from "../api";
import { PlanReview } from "./PlanReview";

function plan(status: string): Schema<"WorkPlan"> {
  return {
    id: "plan",
    tender_id: "one",
    title: "Initial review",
    version: 2,
    status,
    tasks: [
      {
        id: "task",
        tender_id: "one",
        plan_id: "plan",
        title: "Check coverage",
        description: "Review the supplied package.",
        role: "Quantity Surveyor",
        status: "ready",
        source_ids: [],
        created_at: "",
        updated_at: "",
      },
    ],
    created_at: "",
    updated_at: "",
  };
}

function renderReview(status: string) {
  const posts: Array<{ path: string; body: unknown }> = [];
  const onApproved = vi.fn();
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async (address, options) => {
      const path = new URL(String(address)).pathname;
      if (options?.method === "POST") {
        posts.push({ path, body: JSON.parse(String(options.body)) });
        return Response.json(plan("approved"));
      }
      if (path.endsWith("/plans")) return Response.json([plan(status)]);
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
        <PlanReview tenderId="one" planId="plan" onApproved={onApproved} />
      </ApiContext.Provider>
    </QueryClientProvider>,
  );
  return { posts, onApproved };
}

it("approves a proposed plan with the engineer's note", async () => {
  const user = userEvent.setup();
  const { posts, onApproved } = renderReview("proposed");
  expect(
    await screen.findByRole("heading", { name: "Initial review" }),
  ).toBeVisible();
  expect(screen.getByText("Check coverage")).toBeVisible();
  await user.type(
    screen.getByLabelText("Note for the Manager (optional)"),
    "Start with drainage",
  );
  await user.click(screen.getByRole("button", { name: "Approve and start" }));
  await waitFor(() => expect(onApproved).toHaveBeenCalledOnce());
  expect(posts).toEqual([
    {
      path: "/api/tenders/one/plans/plan/approve",
      body: { rationale: "Start with drainage" },
    },
  ]);
});

it("does not offer approval once the plan is no longer proposed", async () => {
  renderReview("approved");
  expect(
    await screen.findByRole("heading", { name: "Initial review" }),
  ).toBeVisible();
  expect(
    screen.queryByRole("button", { name: "Approve and start" }),
  ).not.toBeInTheDocument();
});
