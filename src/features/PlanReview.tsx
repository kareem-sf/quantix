import { useEffect, useRef, useState } from "react";
import {
  AlertTriangle,
  ArrowLeft,
  Check,
  ChevronDown,
  ExternalLink as ExternalLinkIcon,
  FileText,
  MessageSquare,
  Play,
} from "lucide-react";
import { ApiError, tenderPath, useApi, useResource, type Schema } from "../api";
import { ErrorNotice, Loading, Status } from "../components/common";
import {
  DelegationReview,
  type DelegationReviewState,
} from "./DelegationReview";
import { Citations, type SourceSelection } from "./Sources";

export function PlanReview(props: PlanReviewProps) {
  return (
    <ViewedPlanReview key={`${props.tenderId}:${props.planId}`} {...props} />
  );
}
type PlanReviewProps = {
  tenderId: string;
  planId: string;
  onBack?: () => void;
  onSource?: (source: SourceSelection) => void;
  onRepair?: (target: string) => void;
  onApproved?: () => void;
};

function ViewedPlanReview({
  tenderId,
  planId,
  onBack,
  onSource,
  onRepair,
  onApproved,
}: PlanReviewProps) {
  const api = useApi();
  const query = useResource<Schema<"PlanReview">>(
    `${tenderPath(tenderId)}/plans/${encodeURIComponent(planId)}/review`,
  );
  const [viewed, setViewed] = useState<Schema<"PlanReview"> | null>(null);
  const [note, setNote] = useState(() => readReviewNote(tenderId, planId));
  const [approving, setApproving] = useState(false);
  const approvingRef = useRef(false);
  const [error, setError] = useState<unknown>(null);
  const [conflict, setConflict] = useState(false);
  const [freshConflict, setFreshConflict] = useState(false);
  const [approved, setApproved] = useState(false);
  const [noteStorageError, setNoteStorageError] = useState<Error | null>(null);
  const [delegationState, setDelegationState] = useState<DelegationReviewState>(
    {
      dirty: false,
      editing: false,
      saving: false,
      conflict: false,
    },
  );
  const review = viewed;

  useEffect(() => {
    if (!viewed && query.data && !query.isFetching) setViewed(query.data);
  }, [query.data, query.isFetching, viewed]);

  function changeNote(value: string) {
    setNote(value);
    setNoteStorageError(saveReviewNote(tenderId, planId, value));
  }

  async function refreshCurrentReview() {
    const latest = await query.refetch();
    if (latest.data && !latest.error) {
      setViewed(latest.data);
      return latest.data;
    }
    return null;
  }

  async function approve() {
    if (
      !review ||
      approvingRef.current ||
      approved ||
      conflict ||
      delegationState.dirty ||
      delegationState.editing ||
      delegationState.saving ||
      delegationState.conflict ||
      !review.can_approve
    )
      return;
    approvingRef.current = true;
    setApproving(true);
    setError(null);
    try {
      await api.post<Schema<"PlanApprovalResult">>(
        `${tenderPath(tenderId)}/plans/${encodeURIComponent(planId)}/review/approve-and-start`,
        {
          fingerprint: review.fingerprint,
          engineer_confirmed: true,
          // A note is welcome, not required: approving is itself the decision.
          rationale:
            note.trim() || "Approved by the engineer from the plan review.",
        } satisfies Schema<"PlanReviewApproval">,
      );
      setApproved(true);
      onApproved?.();
    } catch (failure) {
      setError(failure);
      if (isConflict(failure)) {
        setConflict(true);
        setFreshConflict(false);
        const latest = await query.refetch();
        if (latest.data && !latest.error) {
          setViewed(latest.data);
          setFreshConflict(true);
        }
      }
    } finally {
      approvingRef.current = false;
      setApproving(false);
    }
  }

  if (query.isPending || (!viewed && query.isFetching))
    return <Loading>Loading the current plan review…</Loading>;
  if (!review) return <ErrorNotice error={query.error} />;
  const blockers = review.blockers ?? [];
  const destinations = unique(
    review.routes.map((route) => route.data_destination).filter(Boolean),
  );
  const accountRoutes = [
    ...new Map(
      review.routes.map((route) => [
        `${route.route.connection_id}:${route.route.model_id}:${route.billing}:${route.data_destination}`,
        route,
      ]),
    ).values(),
  ];
  const hasPaidRoutes = review.routes.some(
    (route) =>
      route.billing === "metered" ||
      route.billing === "unknown" ||
      route.provider_managed_extras,
  );
  return (
    <section className="plan-review" aria-labelledby="plan-review-title">
      <div className="plan-review-topbar">
        {onBack ? (
          <button type="button" className="text-button" onClick={onBack}>
            <ArrowLeft size={17} /> Back to conversation
          </button>
        ) : (
          <span />
        )}
        <span className="review-date">
          Prepared {formatDate(review.reviewed_at)}
        </span>
      </div>
      <header className="plan-review-header">
        <div>
          <h1 id="plan-review-title">Review work plan</h1>
          <p className="muted">
            {review.tasks.length} tasks · {review.scope.title}
          </p>
        </div>
        <span
          className={`review-state ${review.can_approve && !blockers.length ? "review-ready" : "review-blocked"}`}
        >
          {review.can_approve && !blockers.length
            ? "Ready for your review"
            : "Review required"}
        </span>
      </header>

      {conflict ? (
        <div className="plan-review-conflict" role="alert">
          <AlertTriangle size={19} />
          <div>
            <strong>The plan changed while you were reviewing it.</strong>
            <p>
              {freshConflict
                ? "Fresh arrangements are loaded below. Your decision note is still saved. Review the changed scope and AI choices again before approving."
                : "Your decision note is still saved. Load the latest review before approving."}
            </p>
            {freshConflict ? (
              <button
                type="button"
                className="text-button"
                onClick={() => setConflict(false)}
              >
                I reviewed the updated arrangements
              </button>
            ) : (
              <button
                type="button"
                className="text-button"
                disabled={query.isFetching}
                onClick={async () => {
                  const latest = await query.refetch();
                  if (latest.data && !latest.error) {
                    setViewed(latest.data);
                    setFreshConflict(true);
                  }
                }}
              >
                Load latest review
              </button>
            )}
          </div>
        </div>
      ) : null}
      <ErrorNotice error={error || query.error} />
      <ErrorNotice error={noteStorageError} />

      <div className="plan-review-grid">
        <div className="plan-review-main">
          <section className="review-card review-scope-card">
            <div className="review-card-heading">
              <FileText size={20} />
              <div>
                <h2>Scope and tasks</h2>
                <p className="field-help">
                  This is the work that will start after your explicit approval.
                </p>
              </div>
            </div>
            <ol className="review-task-list">
              {review.tasks.map((task, index) => (
                <li key={task.id} id={`review-task-${task.id}`}>
                  <span className="review-task-number">{index + 1}</span>
                  <ReviewTask
                    task={task}
                    tenderId={tenderId}
                    onSource={onSource}
                  />
                </li>
              ))}
            </ol>
          </section>

          <DelegationReview
            tenderId={tenderId}
            planId={planId}
            review={review}
            onRefreshReview={refreshCurrentReview}
            onReviewUpdated={(latest) => {
              setViewed(latest);
              setConflict(false);
              setFreshConflict(false);
            }}
            onStateChange={setDelegationState}
          />

          <details className="review-card review-note-card" open={!!note}>
            <summary>
              <span>
                <MessageSquare size={19} /> Add a decision note (optional)
              </span>
              <ChevronDown size={18} />
            </summary>
            <label>
              Decision note
              <textarea
                aria-label="Decision note"
                dir="auto"
                rows={4}
                maxLength={4000}
                value={note}
                onChange={(event) => changeNote(event.target.value)}
                placeholder="Record why you are approving or requesting this arrangement…"
              />
            </label>
            <p className="field-help">
              Saved for this Tender and plan so a conflict does not lose your
              reasoning.
            </p>
          </details>
        </div>

        <aside className="plan-review-aside">
          {review.meaningful_changes?.length ? (
            <section className="review-card review-changes-card">
              <div className="review-card-heading">
                <AlertTriangle size={20} />
                <div>
                  <h2>AI setup updated</h2>
                  <p className="field-help">
                    Review these changes before starting.
                  </p>
                </div>
              </div>
              <div className="review-change-list">
                {review.meaningful_changes.map((change) => (
                  <article key={`${change.code}:${change.detail}`}>
                    <strong>{change.detail}</strong>
                    {change.before != null || change.after != null ? (
                      <dl>
                        <div>
                          <dt>Before</dt>
                          <dd>{valueLabel(change.before)}</dd>
                        </div>
                        <div>
                          <dt>After</dt>
                          <dd>{valueLabel(change.after)}</dd>
                        </div>
                      </dl>
                    ) : null}
                  </article>
                ))}
              </div>
            </section>
          ) : null}
          <section className="review-card review-ai-card">
            <h2>AI for this plan</h2>
            {accountRoutes.map((route) => (
              <div
                className="review-ai-identity"
                key={`${route.route.connection_id}:${route.route.model_id}:${route.billing}:${route.data_destination}`}
              >
                <span className="review-ai-avatar">
                  {route.account_name.slice(0, 1).toUpperCase()}
                </span>
                <div>
                  <strong>
                    {route.account_name} · {modelName(route)}
                  </strong>
                  <p>
                    Data destination: {destinationLabel(route.data_destination)}
                  </p>
                  {route.provider_managed_extras ? (
                    <p className="warning-text">
                      <strong>
                        Provider extras may incur charges; Quantix cannot
                        guarantee a cap.
                      </strong>
                    </p>
                  ) : null}
                  {route.kind === "fallback" ? (
                    <p>Saved alternative · Requires manual selection</p>
                  ) : null}
                </div>
              </div>
            ))}
            <dl className="review-ai-facts">
              <div>
                <dt>Account and model check</dt>
                <dd>
                  {review.routes.length > 0 &&
                  review.routes.every(
                    (route) => route.readiness === "ready",
                  ) ? (
                    <>
                      <Check size={16} /> Ready
                    </>
                  ) : (
                    "Needs attention"
                  )}
                </dd>
              </div>
              <div>
                <dt>Spending</dt>
                <dd>{spendingLabel(review)}</dd>
              </div>
            </dl>
            {hasPaidRoutes ? (
              <div className="review-visible-budget">
                <span>
                  Run allowance{" "}
                  <strong>{money(review.snapshot.run_budget_usd)}</strong>
                </span>
                <span>
                  Tender allowance{" "}
                  <strong>{money(review.snapshot.tender_budget_usd)}</strong>
                </span>
                <span>
                  Recorded{" "}
                  <strong>
                    USD {review.snapshot.spent_usd.toLocaleString()}
                  </strong>
                </span>
              </div>
            ) : null}
            <details className="review-more-options">
              <summary>More options</summary>
              <p className="review-ai-summary" dir="auto">
                {review.ai_summary}
              </p>
              <div className="review-route-list">
                {review.routes.map((route, index) => (
                  <article key={`${route.kind}:${route.task_id ?? index}`}>
                    <div>
                      <strong>{route.role || route.kind}</strong>
                      <Status value={route.readiness} />
                    </div>
                    <p>
                      {route.account_name} · {modelName(route)}
                    </p>
                    <p className="field-help">{route.spending_detail}</p>
                    <p className="field-help">
                      Data destination: {route.data_destination}
                    </p>
                    <p className="field-help">
                      Connection revision {route.connection_revision} ·
                      Reasoning {route.route.reasoning || "Provider default"} ·
                      Output {route.route.max_output_tokens.toLocaleString()}{" "}
                      tokens
                      {route.route.web_search
                        ? ` · Hosted search up to ${route.route.max_search_calls} calls`
                        : ""}
                    </p>
                  </article>
                ))}
              </div>
              <dl className="review-budget-facts">
                <div>
                  <dt>Data destinations</dt>
                  <dd>
                    {destinations.length
                      ? destinations.join(" · ")
                      : "Not available"}
                  </dd>
                </div>
                <div>
                  <dt>Run allowance</dt>
                  <dd>{money(review.snapshot.run_budget_usd)}</dd>
                </div>
                <div>
                  <dt>Tender allowance</dt>
                  <dd>{money(review.snapshot.tender_budget_usd)}</dd>
                </div>
                <div>
                  <dt>Recorded spending</dt>
                  <dd>USD {review.snapshot.spent_usd.toLocaleString()}</dd>
                </div>
                <div>
                  <dt>Held for work</dt>
                  <dd>USD {review.snapshot.reserved_usd.toLocaleString()}</dd>
                </div>
                <div>
                  <dt>Tender revision</dt>
                  <dd>{review.scope.tender_revision}</dd>
                </div>
                <div>
                  <dt>Source revision</dt>
                  <dd>{review.scope.source_revision}</dd>
                </div>
                <div>
                  <dt>Source basis</dt>
                  <dd>
                    {(review.scope.source_ids ?? []).length} source
                    {(review.scope.source_ids ?? []).length === 1 ? "" : "s"}
                  </dd>
                </div>
              </dl>
            </details>
          </section>
          {blockers.length ? (
            <section className="review-card review-blockers-card">
              <h2>What needs attention</h2>
              <p className="field-help">
                Use a repair link, then return to this review and check the
                current arrangements again.
              </p>
              <ul>
                {blockers.map((blocker, index) => (
                  <li key={`${blocker.code}:${blocker.detail}:${index}`}>
                    <strong>{blocker.detail}</strong>
                    <RepairLink
                      code={blocker.code}
                      target={blocker.repair_target}
                      onRepair={onRepair}
                    />
                  </li>
                ))}
              </ul>
            </section>
          ) : null}
        </aside>
      </div>

      <footer className="plan-review-footer">
        <p>
          {approved
            ? "Tasks are starting. Their progress will appear in Work."
            : review.can_approve && !blockers.length
              ? `Approval starts these ${review.tasks.length} tasks with the AI setup shown above.`
              : "Resolve the listed items before starting work."}
        </p>
        <div className="inline-actions">
          {onBack ? (
            <button type="button" className="text-button" onClick={onBack}>
              Request changes
            </button>
          ) : null}
          <button
            type="button"
            className="button primary review-approve-button"
            disabled={
              approved ||
              approving ||
              conflict ||
              delegationState.dirty ||
              delegationState.editing ||
              delegationState.saving ||
              delegationState.conflict ||
              !review.can_approve ||
              blockers.length > 0
            }
            onClick={() => void approve()}
          >
            <Play size={16} />
            {approved
              ? "Tasks starting…"
              : approving
                ? "Starting…"
                : `Approve & start ${review.tasks.length} tasks`}
          </button>
        </div>
      </footer>
    </section>
  );
}

function RepairLink({
  code,
  target,
  onRepair,
}: {
  code: Schema<"PlanReviewBlocker">["code"];
  target: string;
  onRepair?: (target: string) => void;
}) {
  const label = repairLabel(code);
  if (onRepair)
    return (
      <button
        type="button"
        className="text-button repair-link"
        onClick={() => onRepair(target)}
      >
        {label} <ExternalLinkIcon size={13} />
      </button>
    );
  return (
    <a
      className="text-button repair-link"
      href={target.startsWith("#") ? target : `#${target}`}
    >
      {label} <ExternalLinkIcon size={13} />
    </a>
  );
}

function ReviewTask({
  task,
  tenderId,
  onSource,
}: {
  task: Schema<"PlanReviewTask">;
  tenderId: string;
  onSource?: (source: SourceSelection) => void;
}) {
  const [open, setOpen] = useState(false);
  return (
    <details
      className="review-task-copy"
      onToggle={(event) => setOpen(event.currentTarget.open)}
    >
      <summary className="review-task-summary">
        <span>
          <strong>{task.title}</strong>
          <span className="review-task-preview">
            {taskPreview(task.description)}
          </span>
        </span>
        <ChevronDown className="review-task-chevron" size={18} />
      </summary>
      <div className="review-task-detail">
        <p>{task.description}</p>
        {open && onSource && task.source_ids?.length ? (
          <Citations
            ids={task.source_ids}
            tenderId={tenderId}
            onOpen={onSource}
          />
        ) : null}
      </div>
    </details>
  );
}

function taskPreview(description: string) {
  const sentenceEnd = description.search(/[.!?؟](?:\s|$)/u);
  const end = sentenceEnd >= 0 ? sentenceEnd + 1 : description.length;
  if (end <= 140) return description.slice(0, end);
  const prefix = description.slice(0, 139);
  const wordEnd = prefix.lastIndexOf(" ");
  return `${prefix.slice(0, wordEnd > 80 ? wordEnd : 139).trimEnd()}…`;
}

function destinationLabel(destination: string) {
  if (destination === "Official grok_build client") return "xAI · Grok";
  if (destination === "Official codex client") return "OpenAI · ChatGPT";
  return destination;
}

function modelName(route: Schema<"PlanReviewRoute">) {
  const model = route.model as { display_name?: unknown; id?: unknown };
  return typeof model.display_name === "string"
    ? model.display_name
    : route.route.model_id;
}

function spendingLabel(review: Schema<"PlanReview">) {
  const billings = new Set(review.routes.map((route) => route.billing));
  const labels = [];
  if (billings.has("subscription"))
    labels.push(
      review.routes.some((route) => route.provider_managed_extras)
        ? "Subscription with paid provider extras enabled"
        : "Subscription allowance",
    );
  if (billings.has("metered"))
    labels.push("Metered routes · Tender budget applies");
  if (billings.has("unknown"))
    labels.push("Unknown pricing · Review allowance");
  return labels.join("; ") || "Review required";
}

function repairLabel(code: Schema<"PlanReviewBlocker">["code"]) {
  return code === "sign_in"
    ? "Sign in"
    : code === "model_check"
      ? "Check model"
      : code === "spending"
        ? "Review spending"
        : code === "sources"
          ? "Review sources"
          : code === "running_work"
            ? "View current work"
            : code === "configuration" || code === "connection"
              ? "Review AI setup"
              : "Review plan";
}

function money(value: number | null | undefined) {
  return value == null ? "Not set" : `USD ${value.toLocaleString()}`;
}

function valueLabel(value: unknown) {
  if (value == null || value === "") return "No previous value";
  if (typeof value === "object") return JSON.stringify(value);
  return String(value);
}

function unique(values: string[]) {
  return [...new Set(values)];
}

function formatDate(value: string) {
  const date = new Date(value);
  return Number.isNaN(date.getTime())
    ? ""
    : date.toLocaleString([], { dateStyle: "medium", timeStyle: "short" });
}

function isConflict(failure: unknown) {
  if (failure instanceof ApiError) return failure.status === 409;
  return (
    typeof failure === "object" &&
    failure !== null &&
    "status" in failure &&
    failure.status === 409
  );
}

function reviewNoteKey(tenderId: string, planId: string) {
  return `quantix.plan-review-note.v1:${tenderId}:${planId}`;
}

function readReviewNote(tenderId: string, planId: string) {
  try {
    return window.localStorage.getItem(reviewNoteKey(tenderId, planId)) ?? "";
  } catch {
    return "";
  }
}

function saveReviewNote(tenderId: string, planId: string, value: string) {
  try {
    const key = reviewNoteKey(tenderId, planId);
    if (value) window.localStorage.setItem(key, value);
    else window.localStorage.removeItem(key);
    return null;
  } catch {
    return new Error("The decision note could not be saved on this device.");
  }
}
