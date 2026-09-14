import type { PlanStepStatus } from "@/components/ui/ai-planning";
import type { RunActivity } from "./types";

export type RunPlanStep = {
  id: string;
  title: string;
  status: PlanStepStatus;
  durationMs: number | null;
  detail: string;
  count: number;
};

export type RunReasoningPart =
  | { kind: "thought"; text: string }
  | { kind: "action"; title: string; status: PlanStepStatus };

/** What each Tender tool does, in the engineer's terms. */
const TOOL_TITLES: Record<string, string> = {
  search_sources: "Searching tender evidence",
  read_source: "Reading source passages",
  read_whole_document: "Reading a tender document in full",
  read_package_map: "Reviewing the tender package map",
  list_documents: "Reviewing the document register",
  view_document_page: "Inspecting a document page",
  inspect_extraction_coverage: "Checking which pages were read",
  compare_source_versions: "Comparing document revisions",
  trace_change_impact: "Tracing the impact of a revision",
  inspect_estimate: "Reviewing the estimate",
  check_estimate_coverage: "Checking estimate coverage",
  rehearse_submission: "Checking the submission package",
  inspect_project_map: "Reviewing the project map",
  inspect_submission_requirements: "Reviewing submission requirements",
  inspect_generated_documents: "Reviewing drafted documents",
  inspect_tender_records: "Reviewing earlier findings and decisions",
  read_tender_record: "Reading an earlier record",
  list_reusable_notes: "Checking approved company knowledge",
  read_reusable_note: "Reading approved company knowledge",
  calculate_engineering: "Running an engineering calculation",
  check_engineering_calculation: "Checking a calculation",
  calculate_drawing_measurement: "Measuring from a drawing",
  save_work_product: "Saving a working draft",
  read_work_product: "Reading a working draft",
  list_work_products: "Checking saved working drafts",
  save_work_brief: "Updating the work plan",
  propose: "Preparing records for your review",
  proposal_format: "Checking what a record needs",
  list_team: "Checking the team",
  hire_staff: "Hiring a staff member",
  assign_work: "Assigning work",
  read_assignment: "Reviewing a staff member's work",
  answer_staff: "Answering a staff member",
  inspect_quote_requests: "Reviewing supplier requests",
  read_quote_replies: "Reading supplier replies",
  quantix_submit_result: "Checking the answer",
};

const CATEGORY_TITLES: Record<string, string> = {
  routing: "Understanding the request",
  validation: "Verifying the answer against the sources",
  draft: "Writing the answer",
  provider_tool: "Using a provider tool",
};

// "source" events only record which passages were read; they are not steps.
const STEP_CATEGORIES = new Set([
  "routing",
  "tool",
  "validation",
  "draft",
  "provider_tool",
]);

function phaseStatus(phase: string, active: boolean): PlanStepStatus {
  if (phase === "completed" || phase === "observed") return "success";
  if (phase === "failed" || phase === "cancelled") return "error";
  return active ? "active" : "error";
}

function titleOf(item: RunActivity) {
  if (item.tool) return TOOL_TITLES[item.tool] ?? item.message;
  return CATEGORY_TITLES[item.category] ?? item.message;
}

/**
 * The planned and completed steps of one run, derived from its activity:
 * one step per operation, consecutive repeats of the same action merged.
 */
export function runPlan(items: RunActivity[], runActive: boolean) {
  const operations = new Map<string, RunActivity[]>();
  const order: string[] = [];
  // Reasoning and actions are kept with their event position, so the thread
  // reads in the order things actually happened.
  const thoughts: { at: number; part: RunReasoningPart }[] = [];
  for (const item of items) {
    if (
      item.category === "reasoning_summary" ||
      item.category === "reasoning"
    ) {
      const text = item.preview?.trim();
      if (text && item.phase !== "started") {
        const last = thoughts.at(-1)?.part;
        if (last?.kind === "thought" && item.phase === "delta")
          last.text = `${last.text}${text}`;
        else
          thoughts.push({ at: item.event_id, part: { kind: "thought", text } });
      }
      continue;
    }
    if (!STEP_CATEGORIES.has(item.category)) continue;
    const key = item.operation_id ?? `event:${item.event_id}`;
    if (!operations.has(key)) {
      operations.set(key, []);
      order.push(key);
    }
    operations.get(key)!.push(item);
  }

  const steps: RunPlanStep[] = [];
  for (const key of order) {
    const events = operations.get(key)!;
    const latest = events.at(-1)!;
    const first = events[0];
    const title = titleOf(first);
    const status = phaseStatus(latest.phase, runActive);
    const durationMs =
      latest.elapsed_ms ??
      (events.length > 1
        ? Date.parse(latest.created_at) - Date.parse(first.created_at)
        : null);
    const detail =
      latest.phase === "failed" ? latest.message : readable(latest.preview);
    const previous = steps.at(-1);
    if (previous && previous.title === title && previous.status !== "error") {
      previous.count += 1;
      previous.status = status === "error" ? "error" : status;
      previous.durationMs =
        (previous.durationMs ?? 0) + (durationMs ?? 0) || previous.durationMs;
      if (detail) previous.detail = detail;
      continue;
    }
    steps.push({ id: key, title, status, durationMs, detail, count: 1 });
    thoughts.push({
      at: first.event_id,
      part: { kind: "action", title, status },
    });
  }
  const reasoning = thoughts
    .sort((a, b) => a.at - b.at)
    .map((entry) => entry.part);
  return { steps, reasoning };
}

function readable(preview: string | undefined) {
  const text = (preview ?? "").trim();
  if (!text || text.startsWith("{") || text.startsWith("[")) return "";
  return text.length > 600 ? `${text.slice(0, 600)}…` : text;
}

export function formatDuration(ms: number | null) {
  if (ms == null || ms < 0) return undefined;
  if (ms < 1000) return `${Math.max(0.1, ms / 1000).toFixed(1)}s`;
  const seconds = Math.round(ms / 1000);
  return seconds < 60
    ? `${seconds}s`
    : `${Math.floor(seconds / 60)}m ${seconds % 60}s`;
}
