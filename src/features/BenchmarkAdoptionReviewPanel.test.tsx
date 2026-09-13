import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ApiContext, createApi } from "../api";
import { BenchmarkAdoptionReviewPanel } from "./BenchmarkAdoptionReviewPanel";

function setup(state: "critical_block" | "review_required") {
  const writes: unknown[] = [];
  const reads: string[] = [];
  const decision = {
    configuration_hash: "a".repeat(64),
    report_id: "candidate",
    report_hash: "b".repeat(64),
    baseline_report_id: "baseline",
    baseline_report_hash: "c".repeat(64),
    state,
    connection_id: "account",
    model_id: "Synthetic model",
    manager_profile_version: 2,
    reasons: ["Measured benchmark regression"],
    updated_at: "2026-09-13T01:00:00Z",
    review_rationale: null,
  };
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async (_url, init) => {
      if (init?.method !== "GET") writes.push(JSON.parse(String(init?.body)));
      else reads.push(String(_url));
      if (String(_url).includes("/reports/"))
        return new Response(
          JSON.stringify({
            id: "candidate",
            dataset_version: "synthetic-v1",
            evaluator_version: "2",
            overall_completed: false,
            cases: [
              {
                case_id: "boq-01",
                repetition: 1,
                scores: { task_completed: false },
                observed: {
                  latency_seconds: 2,
                  requests: null,
                  input_tokens: null,
                  output_tokens: null,
                  estimated_cost_usd: null,
                },
              },
            ],
          }),
        );
      return new Response(JSON.stringify([decision]));
    },
  );
  render(
    <QueryClientProvider
      client={
        new QueryClient({ defaultOptions: { queries: { retry: false } } })
      }
    >
      <ApiContext.Provider value={api}>
        <BenchmarkAdoptionReviewPanel />
      </ApiContext.Provider>
    </QueryClientProvider>,
  );
  return { writes, reads, decision };
}

it("explains a critical block without offering an override", async () => {
  const { writes } = setup("critical_block");
  await userEvent.click(screen.getByText("AI benchmark reviews"));
  expect(
    await screen.findByText(/Review cannot waive a critical engineering error/),
  ).toBeInTheDocument();
  expect(
    screen.queryByRole("button", { name: "Accept this comparison" }),
  ).not.toBeInTheDocument();
  expect(writes).toEqual([]);
});

it("loads exact verified records on request and displays unknown telemetry", async () => {
  const { reads, decision } = setup("critical_block");
  const user = userEvent.setup({ delay: null });
  await user.click(screen.getByText("AI benchmark reviews"));
  await user.click(await screen.findByText("Exact evaluation records"));
  expect(reads.some((url) => url.includes("/reports/"))).toBe(false);
  await user.click(
    screen.getByRole("button", { name: "Show candidate records" }),
  );
  expect(
    await screen.findByText("boq-01, attempt 1: did not pass"),
  ).toBeInTheDocument();
  await user.click(screen.getByText("boq-01, attempt 1: did not pass"));
  expect(
    screen.getByText(/requests unknown; input\/output tokens unknown/),
  ).toBeInTheDocument();
  expect(reads).toContain(
    `http://localhost/api/benchmark-adoption/reports/candidate?expected_hash=${decision.report_hash}`,
  );
});

it("requires an engineer rationale and confirmation for the exact comparison", async () => {
  const { writes, decision } = setup("review_required");
  const user = userEvent.setup({ delay: null });
  await user.click(screen.getByText("AI benchmark reviews"));
  const button = await screen.findByRole("button", {
    name: "Accept this comparison",
  });
  expect(button).toBeDisabled();
  await user.type(
    screen.getByLabelText("Why is this measured regression acceptable?"),
    "The measured extra work is acceptable.",
  );
  await user.click(screen.getByRole("checkbox"));
  await user.click(button);
  expect(writes).toEqual([
    {
      configuration_hash: decision.configuration_hash,
      report_id: decision.report_id,
      report_hash: decision.report_hash,
      baseline_report_id: decision.baseline_report_id,
      baseline_report_hash: decision.baseline_report_hash,
      engineer_confirmed: true,
      rationale: "The measured extra work is acceptable.",
    },
  ]);
});
