import {
  Circle,
  CircleAlert,
  CircleCheck,
  CircleDot,
  type LucideIcon,
} from "lucide-react";
import { tenderPath, useResource, type Schema } from "../../api";
import { ErrorNotice } from "../../components/common";
import { Badge } from "@/components/ui/badge";
import type { SourceSelection } from "../Sources";

type Brief = Schema<"WorkBrief">;

const statusLabel: Record<Brief["status"], string> = {
  in_progress: "In progress",
  waiting_for_engineer: "Waiting for you",
  complete: "Complete",
};

const stepIcon: Record<Schema<"BriefStep">["state"], [LucideIcon, string]> = {
  done: [CircleCheck, "Done"],
  in_progress: [CircleDot, "In progress"],
  blocked: [CircleAlert, "Blocked"],
  to_do: [Circle, "To do"],
};

const ownerLabel: Record<Schema<"BriefQuestion">["owner"], string> = {
  engineer: "For you",
  manager: "Manager",
  colleague: "A colleague",
};

/** The Tender Manager's saved progress on its current multi-step work. */
export function CurrentWork({
  tenderId,
  onSource,
}: {
  tenderId: string;
  onSource: (source: SourceSelection) => void;
}) {
  const state = useResource<Schema<"WorkBriefState">>(
    `${tenderPath(tenderId)}/work-brief`,
    true,
  );
  if (state.error) return <ErrorNotice error={state.error} />;
  const brief = state.data?.brief;
  // Nothing is shown until the Manager has saved a brief for real work.
  if (!brief) return null;
  const steps = brief.steps ?? [];
  const questions = brief.open_questions ?? [];
  const doneWhen = brief.done_when ?? [];
  const settled = brief.settled ?? [];
  const drafts = brief.work_products ?? [];
  const progressCurrent = brief.progress_current !== false;
  return (
    <section
      aria-label="Current work"
      className="flex flex-col gap-3 rounded-xl border bg-card p-4"
    >
      <header className="flex items-start justify-between gap-3">
        <div className="flex min-w-0 flex-col gap-0.5">
          <h2 className="text-sm font-semibold tracking-tight">Current work</h2>
          <p className="text-sm" dir="auto">
            {brief.outcome}
          </p>
        </div>
        <Badge
          variant={
            brief.status === "waiting_for_engineer" ? "default" : "secondary"
          }
        >
          {progressCurrent
            ? statusLabel[brief.status]
            : "Progress needs updating"}
        </Badge>
      </header>
      {!progressCurrent ? (
        <div className="text-sm" role="status">
          <p>Later work was saved. This progress note needs updating.</p>
          {brief.latest_work_run_id ? (
            <a
              className="text-brand-ink underline"
              href={`#/tenders/${encodeURIComponent(tenderId)}/work?view=run&record=${encodeURIComponent(brief.latest_work_run_id)}`}
            >
              View later work and results
            </a>
          ) : null}
        </div>
      ) : null}
      {progressCurrent && brief.next_step ? (
        <p className="text-sm" dir="auto">
          <span className="font-medium">Next: </span>
          {brief.next_step}
        </p>
      ) : null}
      {steps.length ? (
        <ol aria-label="Steps" className="flex flex-col gap-1.5">
          {steps.map((step, index) => {
            const [Icon, label] = stepIcon[step.state];
            return (
              <li key={index} className="flex items-start gap-2 text-sm">
                <Icon
                  className={
                    step.state === "blocked"
                      ? "mt-0.5 size-4 shrink-0 text-destructive"
                      : step.state === "done"
                        ? "mt-0.5 size-4 shrink-0 text-muted-foreground"
                        : "mt-0.5 size-4 shrink-0"
                  }
                  aria-hidden="true"
                />
                <span className="min-w-0" dir="auto">
                  <span className="sr-only">{label}: </span>
                  <span
                    className={
                      step.state === "done"
                        ? "text-muted-foreground"
                        : undefined
                    }
                  >
                    {step.title}
                  </span>
                  {step.note ? (
                    <span className="block text-xs text-muted-foreground">
                      {step.note}
                    </span>
                  ) : null}
                </span>
              </li>
            );
          })}
        </ol>
      ) : null}
      {questions.length ? (
        <section aria-label="Open questions" className="flex flex-col gap-1">
          <h3 className="text-xs font-medium text-muted-foreground">
            Open questions
          </h3>
          <ul className="flex flex-col gap-1.5">
            {questions.map((question, index) => (
              <li key={index} className="text-sm" dir="auto">
                {question.text}
                <span className="block text-xs text-muted-foreground">
                  {ownerLabel[question.owner]}
                  {question.affects ? ` · Affects ${question.affects}` : ""}
                </span>
              </li>
            ))}
          </ul>
        </section>
      ) : null}
      {brief.dependency_state === "needs_review" ? (
        <p className="text-xs text-destructive" role="status">
          A source behind this work changed after it was saved.
        </p>
      ) : null}
      <details className="text-sm">
        <summary className="cursor-pointer text-xs text-muted-foreground">
          More options
        </summary>
        <div className="mt-2 flex flex-col gap-3">
          {!progressCurrent && brief.next_step ? (
            <section>
              <h3 className="text-xs font-medium">
                Previously recorded next step
              </h3>
              <p>{brief.next_step}</p>
            </section>
          ) : null}
          {doneWhen.length ? (
            <section aria-label="Done when">
              <h3 className="text-xs font-medium">Done when</h3>
              <ul className="mt-1 list-disc ps-5">
                {doneWhen.map((check, index) => (
                  <li key={index} dir="auto">
                    {check}
                  </li>
                ))}
              </ul>
            </section>
          ) : null}
          {settled.length ? (
            <section aria-label="Settled points">
              <h3 className="text-xs font-medium">Settled points</h3>
              <ul className="mt-1 flex flex-col gap-1.5">
                {settled.map((point, index) => (
                  <li key={index} dir="auto">
                    {point.text}
                    {point.source_ids?.length ? (
                      <span className="ms-1 inline-flex flex-wrap gap-1">
                        {point.source_ids.map((sourceId, sourceIndex) => (
                          <button
                            key={sourceId}
                            type="button"
                            className="text-xs text-brand-ink underline-offset-4 hover:underline"
                            onClick={() => onSource({ sourceId })}
                          >
                            Source {sourceIndex + 1}
                          </button>
                        ))}
                      </span>
                    ) : null}
                  </li>
                ))}
              </ul>
            </section>
          ) : null}
          {drafts.length ? (
            <section aria-label="Saved drafts">
              <h3 className="text-xs font-medium">Saved drafts</h3>
              <ul className="mt-1 flex flex-col gap-1">
                {drafts.map((draft) => (
                  <li key={draft.product_id} dir="auto">
                    <a
                      className="text-brand-ink underline-offset-4 hover:underline"
                      href={`#/tenders/${encodeURIComponent(tenderId)}/manager?view=work-product&record=${encodeURIComponent(draft.product_id)}`}
                    >
                      {draft.title}
                    </a>
                    <span className="text-xs text-muted-foreground">
                      {" "}
                      · version {draft.version}
                      {draft.dependency_state === "needs_review"
                        ? " · needs review"
                        : ""}
                    </span>
                  </li>
                ))}
              </ul>
            </section>
          ) : null}
          <p className="text-xs text-muted-foreground">
            Saved by the Tender Manager · version {brief.version} ·{" "}
            {new Date(brief.created_at).toLocaleString()}. This is a progress
            record, not an approval.
          </p>
        </div>
      </details>
    </section>
  );
}
