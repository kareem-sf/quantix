import { CircleAlert, LoaderCircle } from "lucide-react";
import { type FormEvent, useEffect, useMemo, useRef, useState } from "react";

import type { AgentTaskInputReference } from "./bindings/AgentTaskInputReference";
import type { TenderQuery } from "./bindings/TenderQuery";
import type { TenderQueryTreatment } from "./bindings/TenderQueryTreatment";
import type { TenderRecordEngineerDecisionKind } from "./bindings/TenderRecordEngineerDecisionKind";
import type { TenderRecordEvidence } from "./bindings/TenderRecordEvidence";
import type { TenderRecordInspection } from "./bindings/TenderRecordInspection";
import { evidenceTextAttributes } from "./evidenceTypography";
import {
  decideTenderQueryTreatment,
  decideTenderRecord,
  inspectTenderQueries,
  inspectTenderRecord,
} from "./quantixHost";
import {
  type EvidenceReviewConflict,
  type EvidenceReviewTarget,
} from "./TenderEvidenceReview";

export type TenderRecordDecisionTarget = {
  recordId: string;
  version: number;
};

interface TenderRecordDecisionProps {
  tenderId: string;
  recordTargets: TenderRecordDecisionTarget[];
  busy: boolean;
  onReviewEvidence: (
    target: EvidenceReviewTarget,
    conflicts?: EvidenceReviewConflict[],
  ) => void;
  reportCommandFailure: () => void;
  onDecided: () => void;
  onClose: () => void;
}

const QUERY_PAGE_SIZE = 8;
const QUERY_TREATMENTS: TenderQueryTreatment[] = [
  "internal_resolution",
  "external_rfi_drafting",
  "approved_assumption",
  "qualification",
  "exclusion",
  "allowance",
  "blocked",
];

const TREATMENT_LABELS: Record<TenderQueryTreatment, string> = {
  internal_resolution: "Resolve internally",
  external_rfi_drafting: "Ask the client (RFI)",
  approved_assumption: "Approve as a stated assumption",
  qualification: "Qualify the offer",
  exclusion: "Exclude from the offer",
  allowance: "Carry as an allowance",
  blocked: "Block dependent work",
};

const trustCopy: Record<TenderRecordInspection["trust_class"], string> = {
  ai_proposal: "AI proposal",
  deterministic_fact: "Deterministic fact",
  verified: "Independently verified",
  engineer_verified: "Engineer verified",
  approved_assumption: "Approved assumption",
  unresolved_gap: "Unresolved gap",
  prior_decision: "Prior decision",
};

function treatmentPermitsDependentWork(treatment: TenderQueryTreatment) {
  return treatment !== "external_rfi_drafting" && treatment !== "blocked";
}

function evidenceKey(evidence: TenderRecordEvidence) {
  return `${evidence.reference.artifact_id}-${evidence.reference.version}-${evidence.reference.ordinal}`;
}

function citationConflicts(
  record: TenderRecordInspection,
  evidence: TenderRecordEvidence,
): EvidenceReviewConflict[] {
  const conflicts: EvidenceReviewConflict[] = [];
  const seen = new Set<string>();
  const sameCitation = (candidate: TenderRecordEvidence) =>
    candidate.reference.artifact_id === evidence.reference.artifact_id &&
    candidate.reference.version === evidence.reference.version &&
    candidate.reference.ordinal === evidence.reference.ordinal;
  for (const contradiction of record.contradictions) {
    if (!contradiction.evidence.some(sameCitation)) continue;
    for (const candidate of contradiction.evidence) {
      if (sameCitation(candidate)) continue;
      const key = evidenceKey(candidate);
      if (seen.has(key)) continue;
      seen.add(key);
      conflicts.push({
        artifactId: candidate.reference.artifact_id,
        version: candidate.reference.version,
        ordinal: candidate.reference.ordinal,
        label: candidate.package_path,
      });
    }
  }
  return conflicts;
}

function recordCanBeVerified(record: TenderRecordInspection) {
  return (
    record.fields.length > 0 &&
    record.fields.every(
      (field) =>
        (field.basis_kind === "evidence" && field.evidence.length > 0) ||
        field.basis_kind === "engineer_entry" ||
        field.basis_kind === "calculation_run",
    )
  );
}

function recordAssumptionIsApprovable(record: TenderRecordInspection) {
  return (
    record.kind === "assumption" &&
    record.fields.length > 0 &&
    record.fields.every(
      (field) =>
        field.basis_kind === "assumption" &&
        field.evidence.length === 0 &&
        Boolean(field.basis_description?.trim()),
    )
  );
}

function queryIsAwaitingDecision(query: TenderQuery) {
  return (
    query.current &&
    query.approved_treatment === null &&
    !["closed", "treatment_approved", "blocked"].includes(query.status)
  );
}

function queryEvidenceTarget(
  reference: AgentTaskInputReference,
): EvidenceReviewTarget | null {
  if (reference.kind !== "source_evidence") return null;
  const separator = reference.reference.lastIndexOf("#");
  if (separator < 1) return null;
  const artifactId = reference.reference.slice(0, separator);
  const ordinal = Number(reference.reference.slice(separator + 1));
  if (!artifactId || !Number.isInteger(ordinal) || ordinal < 1) return null;
  return {
    artifactId,
    version: reference.version,
    ordinal,
    label: `${artifactId.slice(0, 12)} · v${reference.version}`,
  };
}

function EvidenceCitation({
  evidence,
  conflicts,
  onReviewEvidence,
}: {
  evidence: TenderRecordEvidence;
  conflicts: EvidenceReviewConflict[];
  onReviewEvidence: (
    target: EvidenceReviewTarget,
    conflicts?: EvidenceReviewConflict[],
  ) => void;
}) {
  return (
    <button
      type="button"
      className="tender-record-decision__citation"
      title="Open the original source at this passage"
      onClick={() =>
        onReviewEvidence(
          {
            artifactId: evidence.reference.artifact_id,
            version: evidence.reference.version,
            ordinal: evidence.reference.ordinal,
            label: evidence.package_path,
          },
          conflicts,
        )
      }
    >
      <strong>{evidence.package_path}</strong>
      <span>
        v{evidence.reference.version} ·{" "}
        {evidence.location.kind.replace(/_/g, " ")} #
        {evidence.reference.ordinal}
      </span>
      <blockquote {...evidenceTextAttributes(evidence.location)}>
        {evidence.location.original_text}
      </blockquote>
    </button>
  );
}

function RecordDecision({
  tenderId,
  record,
  busy,
  onReviewEvidence,
  reportCommandFailure,
  onDecided,
}: {
  tenderId: string;
  record: TenderRecordInspection;
  busy: boolean;
  onReviewEvidence: (
    target: EvidenceReviewTarget,
    conflicts?: EvidenceReviewConflict[],
  ) => void;
  reportCommandFailure: () => void;
  onDecided: () => void;
}) {
  const [rationale, setRationale] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const verifiable = recordCanBeVerified(record);
  const approvableAssumption = recordAssumptionIsApprovable(record);

  const submit = async (decision: TenderRecordEngineerDecisionKind) => {
    const exactRationale = rationale.trim();
    if (!exactRationale || submitting) return;
    setSubmitting(true);
    try {
      await decideTenderRecord(
        tenderId,
        record.record_id,
        record.version,
        decision,
        exactRationale,
      );
      onDecided();
    } catch {
      reportCommandFailure();
      setSubmitting(false);
    }
  };

  return (
    <article
      className="tender-record-decision__item"
      data-testid="tender-record-decision-record"
    >
      <header>
        <p className="section-label">
          Tender record · {record.kind.replace(/_/g, " ")} · version{" "}
          {record.version}
        </p>
        <h3>{record.title}</h3>
        <p>
          Waiting for your decision. Trust: {trustCopy[record.trust_class]}.
          <code>
            {record.record_id} · v{record.version}
          </code>
        </p>
      </header>
      {record.contradictions.map((contradiction) => (
        <aside
          className="tender-record-decision__contradiction"
          key={contradiction.field_name}
        >
          <strong>Conflicting readings: {contradiction.field_name}</strong>
          <p>{contradiction.summary}</p>
          {contradiction.evidence.map((evidence) => (
            <EvidenceCitation
              key={evidenceKey(evidence)}
              evidence={evidence}
              conflicts={citationConflicts(record, evidence)}
              onReviewEvidence={onReviewEvidence}
            />
          ))}
        </aside>
      ))}
      <div className="tender-record-decision__fields">
        {record.fields.map((field) => (
          <section key={field.name}>
            <h4>{field.name.replace(/_/g, " ")}</h4>
            <p className="tender-record-decision__value">
              {field.value ?? "No supported value in the supplied evidence"}
            </p>
            <p className="tender-record-decision__basis">
              Basis: {field.basis_kind.replace(/_/g, " ")}
              {field.basis_description ? ` — ${field.basis_description}` : ""}
              {field.uncertainty ? ` · Uncertainty: ${field.uncertainty}` : ""}
            </p>
            {field.evidence.map((evidence) => (
              <EvidenceCitation
                key={evidenceKey(evidence)}
                evidence={evidence}
                conflicts={citationConflicts(record, evidence)}
                onReviewEvidence={onReviewEvidence}
              />
            ))}
          </section>
        ))}
      </div>
      <form
        className="tender-record-decision__form"
        onSubmit={(event) => event.preventDefault()}
      >
        <label htmlFor="tender-record-decision-rationale">
          Decision rationale (recorded exactly with this decision)
          <textarea
            id="tender-record-decision-rationale"
            rows={3}
            maxLength={4000}
            value={rationale}
            disabled={busy || submitting}
            onChange={(event) => setRationale(event.target.value)}
          />
        </label>
        <div className="tender-record-decision__actions">
          {verifiable ? (
            <button
              type="button"
              className="manager-workspace__primary"
              disabled={busy || submitting || !rationale.trim()}
              onClick={() => void submit("verify")}
            >
              {submitting ? "Recording…" : "Verify against the source"}
            </button>
          ) : null}
          {approvableAssumption ? (
            <button
              type="button"
              className="manager-workspace__primary"
              disabled={busy || submitting || !rationale.trim()}
              onClick={() => void submit("approve_assumption")}
            >
              {submitting ? "Recording…" : "Approve as stated assumption"}
            </button>
          ) : null}
          <button
            type="button"
            className="manager-workspace__secondary"
            disabled={busy || submitting || !rationale.trim()}
            onClick={() => void submit("reject")}
          >
            Return to the Manager with this reason
          </button>
        </div>
      </form>
    </article>
  );
}

function QueryDecision({
  tenderId,
  query,
  busy,
  onReviewEvidence,
  reportCommandFailure,
  onDecided,
}: {
  tenderId: string;
  query: TenderQuery;
  busy: boolean;
  onReviewEvidence: (
    target: EvidenceReviewTarget,
    conflicts?: EvidenceReviewConflict[],
  ) => void;
  reportCommandFailure: () => void;
  onDecided: () => void;
}) {
  const proposed = query.proposed_treatments[0];
  const [treatment, setTreatment] = useState<TenderQueryTreatment>(
    proposed?.treatment ?? "approved_assumption",
  );
  const [rationale, setRationale] = useState("");
  const [details, setDetails] = useState("");
  const [closes, setCloses] = useState(false);
  const [submitting, setSubmitting] = useState(false);

  const closesAllowed =
    treatmentPermitsDependentWork(treatment) &&
    !(treatment === "internal_resolution" && query.responses.length === 0);

  const submit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const exactRationale = rationale.trim();
    const exactDetails = details.trim();
    if (!exactRationale || !exactDetails || submitting) return;
    setSubmitting(true);
    try {
      await decideTenderQueryTreatment({
        tender_id: tenderId,
        query_id: query.query_id,
        query_version: query.version,
        treatment,
        rationale: exactRationale,
        treatment_details: exactDetails,
        closes_query: closesAllowed && closes,
      });
      onDecided();
    } catch {
      reportCommandFailure();
      setSubmitting(false);
    }
  };

  return (
    <article
      className="tender-record-decision__item"
      data-testid="tender-record-decision-query"
    >
      <header>
        <p className="section-label">
          Tender query · {query.query_type.replace(/_/g, " ")} · version{" "}
          {query.version}
        </p>
        <h3>{query.question}</h3>
        <p>{query.ambiguity_or_gap}</p>
        <p>
          {query.status.replace(/_/g, " ")}
          {query.overdue ? " · overdue" : ""}
          {query.release_blocking ? " · blocks release" : ""}
          <code>
            {query.query_id} · v{query.version}
          </code>
        </p>
      </header>
      {query.evidence.length > 0 ? (
        <section className="tender-record-decision__query-evidence">
          <h4>Cited evidence</h4>
          <ul>
            {query.evidence.map((reference) => {
              const target = queryEvidenceTarget(reference);
              return (
                <li key={`${reference.kind}-${reference.reference}`}>
                  {target ? (
                    <button
                      type="button"
                      className="tender-record-decision__citation-link"
                      onClick={() => onReviewEvidence(target)}
                    >
                      {reference.reference} · v{reference.version}
                    </button>
                  ) : (
                    <code>
                      {reference.kind}:{reference.reference}:v
                      {reference.version}
                    </code>
                  )}
                </li>
              );
            })}
          </ul>
        </section>
      ) : null}
      {query.proposed_treatments.length > 0 ? (
        <section>
          <h4>Proposed treatments</h4>
          <ul>
            {query.proposed_treatments.map((proposal, index) => (
              <li key={`${proposal.treatment}-${index}`}>
                {TREATMENT_LABELS[proposal.treatment]} — {proposal.rationale}
              </li>
            ))}
          </ul>
        </section>
      ) : null}
      {query.responses.length > 0 ? (
        <section>
          <h4>Registered responses</h4>
          <ul>
            {query.responses.map((response) => (
              <li key={response.response_id}>{response.response}</li>
            ))}
          </ul>
        </section>
      ) : null}
      <form className="tender-record-decision__form" onSubmit={submit}>
        <label htmlFor="tender-query-treatment">
          Treatment
          <select
            id="tender-query-treatment"
            value={treatment}
            disabled={busy || submitting}
            onChange={(event) =>
              setTreatment(event.target.value as TenderQueryTreatment)
            }
          >
            {QUERY_TREATMENTS.map((candidate) => (
              <option key={candidate} value={candidate}>
                {TREATMENT_LABELS[candidate]}
              </option>
            ))}
          </select>
        </label>
        <label htmlFor="tender-query-rationale">
          Decision rationale (recorded exactly with this decision)
          <textarea
            id="tender-query-rationale"
            rows={3}
            maxLength={4000}
            value={rationale}
            disabled={busy || submitting}
            onChange={(event) => setRationale(event.target.value)}
          />
        </label>
        <label htmlFor="tender-query-details">
          Exact treatment and consequence
          <textarea
            id="tender-query-details"
            rows={3}
            maxLength={4000}
            value={details}
            disabled={busy || submitting}
            onChange={(event) => setDetails(event.target.value)}
          />
        </label>
        <label className="tender-record-decision__checkbox">
          <input
            type="checkbox"
            checked={closesAllowed && closes}
            disabled={busy || submitting || !closesAllowed}
            onChange={(event) => setCloses(event.target.checked)}
          />
          Close this exact query version with this treatment
        </label>
        <div className="tender-record-decision__actions">
          <button
            type="submit"
            className="manager-workspace__primary"
            disabled={
              busy || submitting || !rationale.trim() || !details.trim()
            }
          >
            {submitting ? "Recording…" : "Approve treatment"}
          </button>
        </div>
      </form>
    </article>
  );
}

function RecordSummary({
  record,
  onReviewEvidence,
}: {
  record: TenderRecordInspection;
  onReviewEvidence: (
    target: EvidenceReviewTarget,
    conflicts?: EvidenceReviewConflict[],
  ) => void;
}) {
  return (
    <article
      className="tender-record-decision__item is-readonly"
      data-testid="tender-record-decision-summary"
    >
      <header>
        <p className="section-label">
          Tender record · {record.kind.replace(/_/g, " ")} · version{" "}
          {record.version}
        </p>
        <h3>{record.title}</h3>
        <p>
          Already decided: {record.verification_status.replace(/_/g, " ")} ·{" "}
          {trustCopy[record.trust_class]}
        </p>
      </header>
      {record.reviews.map((review) => (
        <p key={review.review_id} className="tender-record-decision__review">
          {review.outcome.replace(/_/g, " ")} by {review.decided_by}:{" "}
          {review.rationale}
        </p>
      ))}
      {record.fields.flatMap((field) =>
        field.evidence.map((evidence) => (
          <EvidenceCitation
            key={evidenceKey(evidence)}
            evidence={evidence}
            conflicts={citationConflicts(record, evidence)}
            onReviewEvidence={onReviewEvidence}
          />
        )),
      )}
    </article>
  );
}

export function TenderRecordDecision({
  tenderId,
  recordTargets,
  busy,
  onReviewEvidence,
  reportCommandFailure,
  onDecided,
  onClose,
}: TenderRecordDecisionProps) {
  const [records, setRecords] = useState<TenderRecordInspection[] | null>(null);
  const [pendingQueries, setPendingQueries] = useState<TenderQuery[]>([]);
  const recordTargetsRef = useRef(recordTargets);
  recordTargetsRef.current = recordTargets;
  const recordTargetsKey = recordTargets
    .map((target) => `${target.recordId}:${target.version}`)
    .join("|");

  useEffect(() => {
    let active = true;
    setRecords(null);
    setPendingQueries([]);
    const recordTargets = recordTargetsRef.current;
    const referenced = new Set(
      recordTargets.map((target) => `${target.recordId}:${target.version}`),
    );
    const load = async () => {
      try {
        const inspections = await Promise.all(
          recordTargets.map((target) =>
            inspectTenderRecord(tenderId, target.recordId, target.version),
          ),
        );
        if (!active) return;
        setRecords(inspections);
        const queries: TenderQuery[] = [];
        let cursor: string | null = null;
        for (;;) {
          const page = await inspectTenderQueries(
            tenderId,
            cursor,
            QUERY_PAGE_SIZE,
          );
          if (!active) return;
          queries.push(
            ...page.items.filter(
              (query) =>
                queryIsAwaitingDecision(query) &&
                query.affected_records.some((reference) =>
                  referenced.has(`${reference.record_id}:${reference.version}`),
                ),
            ),
          );
          if (!page.next_cursor) break;
          cursor = page.next_cursor;
        }
        if (active) setPendingQueries(queries);
      } catch {
        if (active) reportCommandFailure();
      }
    };
    if (recordTargets.length > 0) void load();
    return () => {
      active = false;
    };
  }, [recordTargetsKey, reportCommandFailure, tenderId]);

  const pendingRecord = useMemo(
    () => records?.find((record) => record.verification_status === "proposed"),
    [records],
  );

  return (
    <section
      className="tender-record-decision"
      data-testid="tender-record-decision"
      aria-labelledby="tender-record-decision-title"
    >
      <header className="tender-record-decision__header">
        <div>
          <p className="section-label">Tender records &amp; queries</p>
          <h2 id="tender-record-decision-title">
            {pendingRecord || pendingQueries.length > 0
              ? "Decide one item at a time"
              : "Cited Tender records"}
          </h2>
          <p>
            Every decision binds the exact version shown here and returns you to
            the Manager conversation.
          </p>
        </div>
        <button
          type="button"
          className="manager-workspace__secondary"
          onClick={onClose}
        >
          Back to Manager
        </button>
      </header>

      {!records ? (
        <p className="tender-record-decision__loading" role="status">
          <LoaderCircle size={16} aria-hidden="true" /> Opening the cited
          records…
        </p>
      ) : pendingRecord ? (
        <RecordDecision
          tenderId={tenderId}
          record={pendingRecord}
          busy={busy}
          onReviewEvidence={onReviewEvidence}
          reportCommandFailure={reportCommandFailure}
          onDecided={onDecided}
        />
      ) : pendingQueries.length > 0 ? (
        <QueryDecision
          tenderId={tenderId}
          query={pendingQueries[0]}
          busy={busy}
          onReviewEvidence={onReviewEvidence}
          reportCommandFailure={reportCommandFailure}
          onDecided={onDecided}
        />
      ) : (
        <>
          <p className="tender-record-decision__empty" role="status">
            <CircleAlert size={16} aria-hidden="true" /> Nothing in these cited
            records is waiting for your decision.
          </p>
          {records.map((record) => (
            <RecordSummary
              key={`${record.record_id}-${record.version}`}
              record={record}
              onReviewEvidence={onReviewEvidence}
            />
          ))}
        </>
      )}
    </section>
  );
}
