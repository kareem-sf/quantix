import { runPlan } from "./run-plan";
import type { RunActivity } from "./types";

let event = 0;
function activity(values: Partial<RunActivity>): RunActivity {
  event += 1;
  return {
    event_id: event,
    run_id: "run",
    operation_id: null,
    parent_operation_id: null,
    actor_id: "manager",
    actor_label: "Tender Manager",
    assignment_id: null,
    category: "tool",
    phase: "completed",
    message: "",
    created_at: `2026-09-14T10:00:${String(event).padStart(2, "0")}Z`,
    tool: null,
    provider: null,
    model: null,
    provider_call_id: null,
    preview: "",
    preview_truncated: false,
    detail_available: false,
    capture_status: "captured",
    elapsed_ms: null,
    ...values,
  } as RunActivity;
}

it("turns activity into plain engineering steps and merges repeats", () => {
  const items = [
    activity({ category: "run", phase: "started", operation_id: "run" }),
    activity({ category: "routing", phase: "completed", operation_id: "r" }),
    activity({ tool: "search_sources", phase: "started", operation_id: "s1" }),
    activity({
      tool: "search_sources",
      phase: "completed",
      operation_id: "s1",
    }),
    activity({
      tool: "search_sources",
      phase: "completed",
      operation_id: "s2",
    }),
    activity({
      tool: "read_whole_document",
      phase: "started",
      operation_id: "w",
    }),
    activity({
      category: "reasoning_summary",
      phase: "observed",
      preview: "The bid bond is in the conditions.",
    }),
  ];
  const { steps, reasoning } = runPlan(items, true);
  expect(steps.map((step) => [step.title, step.status, step.count])).toEqual([
    ["Understanding the request", "success", 1],
    ["Searching tender evidence", "success", 2],
    ["Reading a tender document in full", "active", 1],
  ]);
  expect(reasoning[0]).toEqual({
    kind: "action",
    title: "Understanding the request",
    status: "success",
  });
  expect(reasoning.some((part) => part.kind === "thought")).toBe(true);
});

it("marks unfinished steps of a stopped run as needing attention", () => {
  const { steps } = runPlan(
    [activity({ tool: "read_source", phase: "started", operation_id: "x" })],
    false,
  );
  expect(steps[0].status).toBe("error");
});
