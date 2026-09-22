import type { WorkspaceSection } from "../navigation/routes";

/** Tender stages shown in the sidebar, in reading order. The plan, team and
 * activity live in the Manager's right panel and every decision is made in the
 * chat, so old Work links open the Manager. */
export const tenderSections = [
  "manager",
  "documents",
  "estimate",
  "submission",
] as const satisfies readonly WorkspaceSection[];

export type TenderSection = (typeof tenderSections)[number] | "work";

export const sectionLabels: Record<TenderSection, string> = {
  manager: "Manager",
  documents: "Documents",
  work: "Work",
  estimate: "Estimate",
  submission: "Submission",
};

const workViews = new Set([
  "plan",
  "plan-review",
  "task",
  "tasks",
  "finding",
  "decisions",
  "reviews",
  "scope",
  "run",
  "activity",
  "ai",
]);
const managerViews = new Set(["output", "approval"]);
const estimateViews = new Set(["boq", "takeoff", "proposals", "quotes"]);
const submissionViews = new Set(["requirements", "documents", "package"]);

/** The area that owns a record view, so a link opens where the record lives. */
export function sectionForView(
  view: string,
  fallback: TenderSection,
): TenderSection {
  if (workViews.has(view)) return "manager";
  if (managerViews.has(view)) return "manager";
  if (estimateViews.has(view)) return "estimate";
  if (submissionViews.has(view)) return "submission";
  return fallback;
}
