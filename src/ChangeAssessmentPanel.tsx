import { FormEvent, useCallback, useEffect, useRef, useState } from "react";

import type { ChangeAssessment } from "./bindings/ChangeAssessment";
import type { ChangeAssessmentClassification } from "./bindings/ChangeAssessmentClassification";
import type { ChangeAssessmentPage } from "./bindings/ChangeAssessmentPage";
import { evidenceLanguageTag } from "./evidenceTypography";
import {
  decideChangeAssessment,
  inspectChangeAssessments,
} from "./quantixHost";

interface ChangeAssessmentPanelProps {
  tenderId: string;
  refreshToken: number;
  reportCommandFailure: () => void;
  onTenderStateChange: () => void;
}

const PAGE_SIZE = 4;

function humanize(value: string) {
  return value.replace(/_/g, " ");
}

function SourceVersion({
  label,
  source,
}: {
  label: string;
  source: ChangeAssessment["prior_source"];
}) {
  return (
    <article className="record-card record-card-compact">
      <strong>{label}</strong>
      <p>
        {source.package_path} / {humanize(source.document_type)}
      </p>
      <p>
        {source.artifact_id} v{source.version}
      </p>
      <p>
        Content <code>{source.sha256}</code>
      </p>
      <p>
        {source.evidence_count} exact Evidence location
        {source.evidence_count === 1 ? "" : "s"}
      </p>
      {source.evidence_preview.length ? (
        <details>
          <summary>
            Inspect {source.evidence_preview.length} bounded Evidence excerpt
            {source.evidence_preview.length === 1 ? "" : "s"}
          </summary>
          <div className="record-list">
            {source.evidence_preview.map((evidence) => (
              <article
                className="record-card record-card-compact"
                key={`${source.artifact_id}:${source.version}:${evidence.ordinal}`}
              >
                <strong>
                  Evidence #{evidence.ordinal} / {humanize(evidence.kind)}
                </strong>
                <p>{evidence.structural_path}</p>
                <p className="evidence-authority">
                  Authoritative original-language text
                </p>
                <blockquote
                  dir="auto"
                  lang={evidenceLanguageTag(evidence.language)}
                >
                  {evidence.original_text}
                </blockquote>
                {evidence.translated_text ? (
                  <div className="evidence-translation">
                    <p>Derived translation / non-authoritative</p>
                    <blockquote dir="auto">
                      {evidence.translated_text}
                    </blockquote>
                  </div>
                ) : null}
                <p>
                  {humanize(evidence.language)} / text{" "}
                  <code>{evidence.text_sha256}</code>
                  {evidence.truncated ? " / excerpt truncated" : ""}
                </p>
              </article>
            ))}
          </div>
        </details>
      ) : (
        <p>No parsed Evidence locations are registered for this version.</p>
      )}
    </article>
  );
}

function AssessmentCard({ assessment }: { assessment: ChangeAssessment }) {
  return (
    <article className="record-card">
      <p className="eyebrow">
        Change Assessment #{assessment.assessment_sequence} /{" "}
        {assessment.status}
      </p>
      <h3>{humanize(assessment.relationship_kind)} evidence change</h3>
      <p>
        Relationship {assessment.relationship_id} / opened from{" "}
        {humanize(assessment.lifecycle_before)}
      </p>
      <p>
        Assessment manifest <code>{assessment.manifest_sha256}</code>
      </p>
      <div className="record-list">
        <SourceVersion
          label="Prior immutable source"
          source={assessment.prior_source}
        />
        <SourceVersion
          label="New immutable source"
          source={assessment.replacement_source}
        />
      </div>

      <div className="notice">
        <strong>Deadline effect</strong>
        <p>{assessment.deadline_effect}</p>
      </div>

      <details open={assessment.status === "pending"}>
        <summary>{assessment.impacts.length} typed provenance impacts</summary>
        <div className="record-list">
          {assessment.impacts.map((impact) => (
            <article
              className="record-card record-card-compact"
              key={[impact.kind, impact.object_id, impact.object_version].join(
                ":",
              )}
            >
              <strong>
                {humanize(impact.kind)} / {humanize(impact.consequence)}
              </strong>
              <p>{impact.summary}</p>
              <p>
                {impact.object_id}
                {impact.object_version
                  ? " v" + String(impact.object_version)
                  : ""}
              </p>
              <details>
                <summary>
                  {impact.dependencies.length} exact provenance link
                  {impact.dependencies.length === 1 ? "" : "s"}
                </summary>
                <ul>
                  {impact.dependencies.map((dependency) => (
                    <li
                      key={[
                        dependency.kind,
                        dependency.object_id,
                        dependency.object_version,
                        dependency.dependency_kind,
                      ].join(":")}
                    >
                      {humanize(dependency.dependency_kind)} from{" "}
                      {humanize(dependency.kind)} {dependency.object_id}
                      {dependency.object_version
                        ? ` v${dependency.object_version}`
                        : ""}
                    </li>
                  ))}
                </ul>
              </details>
            </article>
          ))}
        </div>
      </details>

      <div className="record-list">
        <article className="record-card record-card-compact">
          <strong>Affected commitments</strong>
          {assessment.affected_commitments.length ? (
            <ul>
              {assessment.affected_commitments.map((item, index) => (
                <li key={String(index) + "-" + item}>{item}</li>
              ))}
            </ul>
          ) : (
            <p>No current commitment dependency was identified.</p>
          )}
        </article>
        <article className="record-card record-card-compact">
          <strong>Proposed targeted rework</strong>
          <ul>
            {assessment.proposed_rework.map((item, index) => (
              <li key={String(index) + "-" + item}>{item}</li>
            ))}
          </ul>
        </article>
        <article className="record-card record-card-compact">
          <strong>Unchanged scope</strong>
          <ul>
            {assessment.unchanged_scope.map((item, index) => (
              <li key={String(index) + "-" + item}>{item}</li>
            ))}
          </ul>
        </article>
      </div>

      <div
        className={
          assessment.approval_consequences.length
            ? "notice notice-warning"
            : "notice"
        }
      >
        <strong>Approval consequences</strong>
        {assessment.approval_consequences.length ? (
          <ul>
            {assessment.approval_consequences.map((item) => (
              <li key={item.reference}>
                {item.reference}: {item.consequence}
              </li>
            ))}
          </ul>
        ) : (
          <p>No approval consequences; existing approvals remain valid.</p>
        )}
      </div>

      {assessment.decision ? (
        <div className="notice">
          <strong>
            {humanize(assessment.decision.classification)} by{" "}
            {assessment.decision.decided_by} as{" "}
            {humanize(assessment.decision.acting_role)}
          </strong>
          <p>{assessment.decision.rationale}</p>
          <p>
            Decision <code>{assessment.decision.manifest_sha256}</code> at{" "}
            {assessment.decision.created_at}
          </p>
          {assessment.resolution_baseline_id ? (
            <p>
              Resolved by {assessment.resolution_baseline_id} v
              {assessment.resolution_baseline_version}
            </p>
          ) : null}
        </div>
      ) : null}
    </article>
  );
}

export function ChangeAssessmentPanel({
  tenderId,
  refreshToken,
  reportCommandFailure,
  onTenderStateChange,
}: ChangeAssessmentPanelProps) {
  const [page, setPage] = useState<ChangeAssessmentPage>();
  const [beforeSequence, setBeforeSequence] = useState<number | null>(null);
  const [cursorStack, setCursorStack] = useState<(number | null)[]>([]);
  const [classification, setClassification] =
    useState<ChangeAssessmentClassification>("material");
  const [rationale, setRationale] = useState("");
  const [loading, setLoading] = useState(false);
  const [mutating, setMutating] = useState(false);
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
        const next = await inspectChangeAssessments(
          tenderId,
          requestedBefore,
          PAGE_SIZE,
        );
        if (generation !== requestGeneration.current) return;
        setPage(next);
        setBeforeSequence(requestedBefore);
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

  async function submitDecision(event: FormEvent) {
    event.preventDefault();
    const active = page?.active;
    if (
      actionActive.current ||
      !active ||
      active.status !== "pending" ||
      !rationale.trim()
    )
      return;
    actionActive.current = true;
    requestGeneration.current += 1;
    setMutating(true);
    try {
      await decideChangeAssessment(
        tenderId,
        active.assessment_id,
        active.manifest_sha256,
        classification,
        rationale.trim(),
      );
      setRationale("");
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
  const active = page?.active;
  const activeIsVisible = Boolean(
    active &&
    page?.items.some(
      (assessment) => assessment.assessment_id === active.assessment_id,
    ),
  );

  return (
    <section className="workspace-section">
      <div className="section-heading">
        <div>
          <p className="eyebrow">Evidence-led change control</p>
          <h2 id="change-assessment-title">Change Assessment</h2>
          <p>
            A confirmed Addendum or Replacement freezes affected work until the
            Tendering Manager classifies its exact typed dependency impact.
          </p>
        </div>
        <button
          className="button-secondary"
          disabled={busy}
          onClick={() => void load(beforeSequence, cursorStack)}
        >
          Refresh
        </button>
      </div>

      <div className="status-row">
        <span className="status-pill">
          {active ? humanize(active.status) : "No open assessment"}
        </span>
        <span className="status-pill">
          {active?.impacts.length ?? 0} current impacts
        </span>
      </div>

      {page?.items.length ? (
        <div className="record-list">
          {page.items.map((assessment) => (
            <AssessmentCard
              assessment={assessment}
              key={assessment.assessment_id}
            />
          ))}
        </div>
      ) : (
        <p>No confirmed source change has opened an assessment.</p>
      )}

      {active && !activeIsVisible ? (
        <div className="record-list" aria-label="Current active assessment">
          <AssessmentCard assessment={active} />
        </div>
      ) : null}

      {active?.status === "pending" ? (
        <form className="record-card" onSubmit={submitDecision}>
          <p className="eyebrow">Exact EITL classification</p>
          <h3>Classify assessment #{active.assessment_sequence}</h3>
          <label>
            Classification
            <select
              value={classification}
              onChange={(event) =>
                setClassification(
                  event.target.value as ChangeAssessmentClassification,
                )
              }
              disabled={busy}
            >
              <option value="material">
                Material / targeted rework required
              </option>
              <option value="irrelevant">
                Irrelevant / preserve current work
              </option>
            </select>
          </label>
          <label>
            Manager rationale
            <textarea
              value={rationale}
              onChange={(event) => setRationale(event.target.value)}
              disabled={busy}
            />
          </label>
          <button disabled={busy || !rationale.trim()} type="submit">
            Record immutable classification
          </button>
        </form>
      ) : null}

      <div className="button-row">
        <button
          className="button-secondary"
          disabled={busy || !page?.next_before_sequence}
          onClick={() => {
            if (!page?.next_before_sequence) return;
            void load(page.next_before_sequence, [
              ...cursorStack,
              beforeSequence,
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
