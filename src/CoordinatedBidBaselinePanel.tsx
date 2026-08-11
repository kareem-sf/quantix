import { FormEvent, useCallback, useEffect, useRef, useState } from "react";

import type { CoordinatedBidBaseline } from "./bindings/CoordinatedBidBaseline";
import type { CoordinatedBidBaselineCategory } from "./bindings/CoordinatedBidBaselineCategory";
import type { CoordinatedBidBaselineDecision } from "./bindings/CoordinatedBidBaselineDecision";
import type { CoordinatedBidBaselinePage } from "./bindings/CoordinatedBidBaselinePage";
import {
  assembleCoordinatedBidBaseline,
  decideCoordinatedBidBaseline,
  inspectCoordinatedBidBaselines,
} from "./quantixHost";

interface CoordinatedBidBaselinePanelProps {
  tenderId: string;
  runtimeReady: boolean;
  refreshToken: number;
  reportCommandFailure: () => void;
  onTenderStateChange: () => void;
}

const PAGE_SIZE = 4;

function humanize(value: string) {
  return value.replace(/_/g, " ");
}

function lines(value: string) {
  return value
    .split("\n")
    .map((item) => item.trim())
    .filter(Boolean);
}

function BaselineCard({ baseline }: { baseline: CoordinatedBidBaseline }) {
  const categories = new Set(
    baseline.bindings.map((binding) => binding.category),
  );

  return (
    <article className="record-card">
      <p className="eyebrow">
        Coordinated Bid Baseline v{baseline.version} / Tender revision{" "}
        {baseline.tender_revision}
      </p>
      <h3>
        {baseline.current ? "Current exact baseline" : "Immutable history"}
      </h3>
      <p>{baseline.explanation}</p>
      <p>
        Work Plan {baseline.plan_id} v{baseline.plan_version} / activation{" "}
        {baseline.activation_id}
      </p>
      <p>
        Coordinator {baseline.coordinator_profile_id} v
        {baseline.coordinator_profile_version}
      </p>
      <p>
        Baseline manifest <code>{baseline.manifest_sha256}</code>
      </p>

      <div className="status-row">
        {[
          "technical",
          "programme",
          "procurement",
          "contractual",
          "risk",
          "query",
          "qualification",
          "exclusion",
          "submission",
          "commercial",
        ].map((category) => (
          <span className="status-pill" key={category}>
            {categories.has(category as CoordinatedBidBaselineCategory)
              ? "Bound"
              : "Missing"}{" "}
            / {humanize(category)}
          </span>
        ))}
      </div>

      {baseline.blockers.length ? (
        <div className="notice notice-warning">
          <strong>Approval blockers</strong>
          <ul>
            {baseline.blockers.map((blocker, index) => (
              <li key={`${blocker.code}-${index}`}>
                {humanize(blocker.code)}: {blocker.summary}
                {blocker.references.length
                  ? ` (${blocker.references.join(", ")})`
                  : ""}
              </li>
            ))}
          </ul>
        </div>
      ) : null}

      {baseline.contradictions.length ? (
        <div className="notice notice-warning">
          <strong>Cross-workstream contradictions</strong>
          <ul>
            {baseline.contradictions.map((contradiction, index) => (
              <li key={`${contradiction.key}-${index}`}>
                {humanize(contradiction.category)} / {contradiction.summary} ({" "}
                {contradiction.references.join(", ")})
              </li>
            ))}
          </ul>
        </div>
      ) : null}

      <details>
        <summary>{baseline.bindings.length} exact source bindings</summary>
        <div className="record-list">
          {baseline.bindings.map((binding, index) => (
            <article
              className="record-card record-card-compact"
              key={`${binding.kind}-${binding.reference_id}-${binding.version}-${index}`}
            >
              <strong>
                {humanize(binding.category)} / {humanize(binding.kind)}
              </strong>
              <p>{binding.summary}</p>
              <p>
                {binding.reference_id} v{binding.version} / {binding.source}
              </p>
              <p>
                Manifest <code>{binding.manifest_sha256}</code>
              </p>
              {binding.supporting_review_id ? (
                <p>Supporting review {binding.supporting_review_id}</p>
              ) : null}
              {binding.approval_id ? (
                <p>Approval {binding.approval_id}</p>
              ) : null}
            </article>
          ))}
        </div>
      </details>

      {baseline.approval ? (
        <div className="notice">
          <strong>
            {humanize(baseline.approval.decision)} by{" "}
            {baseline.approval.decided_by} as{" "}
            {humanize(baseline.approval.acting_role)}
          </strong>
          <p>{baseline.approval.rationale}</p>
          <p>
            Approval {baseline.approval.approval_id} at{" "}
            {baseline.approval.created_at}
          </p>
          <p>
            Supporting reviews{" "}
            <code>{baseline.approval.supporting_reviews_sha256}</code>
          </p>
          <p>
            Approval <code>{baseline.approval.approval_sha256}</code>
          </p>
          {baseline.approval.conditions.length ? (
            <p>Conditions: {baseline.approval.conditions.join(" / ")}</p>
          ) : null}
          {baseline.approval.exceptions.length ? (
            <p>Exceptions: {baseline.approval.exceptions.join(" / ")}</p>
          ) : null}
        </div>
      ) : null}
    </article>
  );
}

export function CoordinatedBidBaselinePanel({
  tenderId,
  runtimeReady,
  refreshToken,
  reportCommandFailure,
  onTenderStateChange,
}: CoordinatedBidBaselinePanelProps) {
  const [page, setPage] = useState<CoordinatedBidBaselinePage>();
  const [beforeVersion, setBeforeVersion] = useState<number | null>(null);
  const [cursorStack, setCursorStack] = useState<(number | null)[]>([]);
  const [loading, setLoading] = useState(false);
  const [mutating, setMutating] = useState(false);
  const [decision, setDecision] =
    useState<CoordinatedBidBaselineDecision>("approve");
  const [rationale, setRationale] = useState("");
  const [conditions, setConditions] = useState("");
  const [exceptions, setExceptions] = useState("");
  const requestGeneration = useRef(0);
  const actionActive = useRef(false);

  const load = useCallback(
    async (
      requestedBefore: number | null,
      requestedStack: (number | null)[],
    ) => {
      const generation = ++requestGeneration.current;
      setLoading(true);
      try {
        const next = await inspectCoordinatedBidBaselines(
          tenderId,
          requestedBefore,
          PAGE_SIZE,
        );
        if (generation !== requestGeneration.current) return;
        setPage(next);
        setBeforeVersion(requestedBefore);
        setCursorStack(requestedStack);
      } catch {
        if (generation === requestGeneration.current) reportCommandFailure();
      } finally {
        if (generation === requestGeneration.current) setLoading(false);
      }
    },
    [reportCommandFailure, tenderId],
  );

  useEffect(() => {
    void load(null, []);
    return () => {
      requestGeneration.current += 1;
    };
  }, [load, refreshToken]);

  async function execute(action: () => Promise<unknown>) {
    if (actionActive.current) return;
    actionActive.current = true;
    requestGeneration.current += 1;
    setMutating(true);
    try {
      await action();
      onTenderStateChange();
      await load(null, []);
    } catch {
      reportCommandFailure();
    } finally {
      actionActive.current = false;
      setMutating(false);
    }
  }

  const busy = loading || mutating;
  const head = beforeVersion === null ? page?.items[0] : undefined;
  const canAssemble =
    runtimeReady &&
    (page?.lifecycle_phase === "active_production" ||
      page?.lifecycle_phase === "integrated_review");
  const canDecide =
    head?.current &&
    !head.approval &&
    page?.lifecycle_phase === "integrated_review";

  function submitDecision(event: FormEvent) {
    event.preventDefault();
    if (!head || !rationale.trim()) return;
    void execute(() =>
      decideCoordinatedBidBaseline(
        tenderId,
        head.baseline_id,
        head.version,
        head.manifest_sha256,
        decision,
        rationale.trim(),
        lines(conditions),
        lines(exceptions),
      ),
    );
  }

  return (
    <section className="workspace-section">
      <div className="section-heading">
        <div>
          <p className="eyebrow">Integrated Review</p>
          <h2>Coordinated Bid Baseline</h2>
          <p>
            The Coordinator assembles exact reviewed work without changing its
            meaning. Only the Tendering Manager can approve the bound baseline
            for Package Production.
          </p>
        </div>
        <button
          className="button-secondary"
          disabled={busy}
          onClick={() => void load(beforeVersion, cursorStack)}
        >
          Refresh
        </button>
      </div>

      <div className="status-row">
        <span className="status-pill">
          Lifecycle / {humanize(page?.lifecycle_phase ?? "loading")}
        </span>
        <span className="status-pill">
          {head?.blockers.length ?? 0} blockers
        </span>
        <span className="status-pill">
          {head?.contradictions.length ?? 0} contradictions
        </span>
      </div>

      {canAssemble && beforeVersion === null ? (
        <button
          disabled={busy}
          onClick={() =>
            void execute(() =>
              assembleCoordinatedBidBaseline(tenderId, head?.version ?? null),
            )
          }
        >
          {head ? "Assemble exact successor" : "Assemble baseline"}
        </button>
      ) : null}

      {page?.items.length ? (
        <div className="record-list">
          {page.items.map((baseline) => (
            <BaselineCard
              key={`${baseline.baseline_id}:${baseline.version}`}
              baseline={baseline}
            />
          ))}
        </div>
      ) : (
        <p>No Coordinated Bid Baseline has been assembled.</p>
      )}

      {canDecide && head ? (
        <form className="record-card" onSubmit={submitDecision}>
          <p className="eyebrow">Exact EITL decision</p>
          <h3>Approve, return, or reject baseline v{head.version}</h3>
          <label>
            Decision
            <select
              value={decision}
              onChange={(event) =>
                setDecision(
                  event.target.value as CoordinatedBidBaselineDecision,
                )
              }
              disabled={busy}
            >
              <option value="approve">Approve for Package Production</option>
              <option value="return">Return to Active Production</option>
              <option value="reject">Reject baseline</option>
            </select>
          </label>
          <label>
            Decision rationale
            <textarea
              value={rationale}
              onChange={(event) => setRationale(event.target.value)}
              disabled={busy}
            />
          </label>
          <label>
            Conditions, one per line
            <textarea
              value={conditions}
              onChange={(event) => setConditions(event.target.value)}
              disabled={busy}
            />
          </label>
          <label>
            Disclosed exceptions, one per line
            <textarea
              value={exceptions}
              onChange={(event) => setExceptions(event.target.value)}
              disabled={busy}
            />
          </label>
          <button disabled={busy || !rationale.trim()} type="submit">
            Record exact Manager decision
          </button>
        </form>
      ) : null}

      <div className="button-row">
        <button
          className="button-secondary"
          disabled={busy || !page?.next_before_version}
          onClick={() => {
            if (!page?.next_before_version) return;
            void load(page.next_before_version, [
              ...cursorStack,
              beforeVersion,
            ]);
          }}
        >
          Older
        </button>
        <button
          className="button-secondary"
          disabled={busy || cursorStack.length === 0}
          onClick={() => {
            const previous = cursorStack[cursorStack.length - 1] ?? null;
            void load(previous, cursorStack.slice(0, -1));
          }}
        >
          Newer
        </button>
      </div>
    </section>
  );
}
