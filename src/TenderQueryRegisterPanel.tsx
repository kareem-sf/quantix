import { FormEvent, useCallback, useEffect, useRef, useState } from "react";

import type { AgentTaskInputReference } from "./bindings/AgentTaskInputReference";
import type { DecideTenderQueryTreatmentCommand } from "./bindings/DecideTenderQueryTreatmentCommand";
import type { TenderQuery } from "./bindings/TenderQuery";
import type { TenderQueryPage } from "./bindings/TenderQueryPage";
import type { TenderQueryTreatment } from "./bindings/TenderQueryTreatment";
import type { TenderQueryType } from "./bindings/TenderQueryType";
import {
  createTenderQuery,
  decideTenderQueryTreatment,
  inspectTenderQueries,
  reviseTenderQuery,
} from "./quantixHost";

interface TenderQueryRegisterPanelProps {
  tenderId: string;
  runtimeReady: boolean;
  refreshToken: number;
  reportCommandFailure: () => void;
  onTenderStateChange: () => void;
}

const treatments: TenderQueryTreatment[] = [
  "internal_resolution",
  "external_rfi_drafting",
  "approved_assumption",
  "qualification",
  "exclusion",
  "allowance",
  "blocked",
];

export function TenderQueryRegisterPanel({
  tenderId,
  runtimeReady,
  refreshToken,
  reportCommandFailure,
  onTenderStateChange,
}: TenderQueryRegisterPanelProps) {
  const [page, setPage] = useState<TenderQueryPage>();
  const [cursor, setCursor] = useState<string | null>(null);
  const [cursorStack, setCursorStack] = useState<(string | null)[]>([]);
  const [loading, setLoading] = useState(false);
  const [question, setQuestion] = useState("");
  const [gap, setGap] = useState("");
  const [queryType, setQueryType] = useState<TenderQueryType>(
    "missing_information",
  );
  const [ownerKey, setOwnerKey] = useState("");
  const [taskKey, setTaskKey] = useState("");
  const [recordId, setRecordId] = useState("");
  const [recordVersion, setRecordVersion] = useState("1");
  const [evidenceKind, setEvidenceKind] = useState("source_evidence");
  const [evidenceReference, setEvidenceReference] = useState("");
  const [evidenceVersion, setEvidenceVersion] = useState("1");
  const [dueAt, setDueAt] = useState("");
  const [material, setMaterial] = useState(true);
  const [releaseBlocking, setReleaseBlocking] = useState(true);
  const requestGeneration = useRef(0);

  const load = useCallback(
    async (nextCursor: string | null, nextStack: (string | null)[]) => {
      const generation = ++requestGeneration.current;
      setLoading(true);
      try {
        const next = await inspectTenderQueries(tenderId, nextCursor, 8);
        if (requestGeneration.current !== generation) return;
        setPage(next);
        setCursor(nextCursor);
        setCursorStack(nextStack);
        setOwnerKey((current) =>
          current || !next.owner_profiles[0]
            ? current
            : `${next.owner_profiles[0].profile_id}:${next.owner_profiles[0].version}`,
        );
        setTaskKey((current) => current || next.production_task_keys[0] || "");
      } catch {
        if (requestGeneration.current === generation) reportCommandFailure();
      } finally {
        if (requestGeneration.current === generation) setLoading(false);
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

  const mutate = async (command: () => Promise<unknown>) => {
    setLoading(true);
    try {
      await command();
      onTenderStateChange();
      await load(null, []);
      return true;
    } catch {
      reportCommandFailure();
      return false;
    } finally {
      setLoading(false);
    }
  };

  const handleCreate = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const [ownerProfileId, ownerVersion] = ownerKey.split(":");
    const version = Number(evidenceVersion);
    const affectedRecordVersion = Number(recordVersion);
    if (
      !ownerProfileId ||
      !ownerVersion ||
      !question.trim() ||
      !gap.trim() ||
      !taskKey.trim() ||
      !evidenceReference.trim() ||
      !Number.isInteger(version) ||
      version < 1 ||
      (recordId.trim() &&
        (!Number.isInteger(affectedRecordVersion) ||
          affectedRecordVersion < 1)) ||
      !dueAt
    ) {
      return;
    }
    void mutate(() =>
      createTenderQuery({
        tender_id: tenderId,
        query_type: queryType,
        question: question.trim(),
        ambiguity_or_gap: gap.trim(),
        owner_profile_id: ownerProfileId,
        owner_profile_version: Number(ownerVersion),
        evidence: [
          {
            kind: evidenceKind.trim(),
            reference: evidenceReference.trim(),
            version,
          },
        ],
        affected_records: recordId.trim()
          ? [
              {
                record_id: recordId.trim(),
                version: affectedRecordVersion,
              },
            ]
          : [],
        affected_task_keys: [taskKey.trim()],
        due_at: new Date(dueAt).toISOString(),
        material,
        release_blocking: releaseBlocking,
        proposed_treatments: [],
      }),
    ).then((succeeded) => {
      if (succeeded) {
        setQuestion("");
        setGap("");
        setEvidenceReference("");
        setRecordId("");
      }
    });
  };

  if (page && !page.query_register_open) {
    return (
      <section className="office-card">
        <h2>Query Register</h2>
        <p>
          Import and verify the Tender package to open the controlled register.
        </p>
      </section>
    );
  }

  return (
    <section className="office-card">
      <div className="section-heading">
        <div>
          <p className="eyebrow">Controlled Tender Queries</p>
          <h2>Query Register</h2>
        </div>
        <button
          type="button"
          disabled={loading}
          onClick={() => void load(null, [])}
        >
          Refresh
        </button>
      </div>
      {page ? (
        <p>
          {page.total_current_count} current · {page.overdue_count} overdue ·{" "}
          {page.release_blocking_count} release-blocking
        </p>
      ) : null}

      {page?.owner_profiles.length ? (
        <form className="stacked-form" onSubmit={handleCreate}>
          <h3>Register exact evidence gap</h3>
          <div className="form-grid">
            <label>
              Query type
              <select
                value={queryType}
                onChange={(event) =>
                  setQueryType(event.target.value as TenderQueryType)
                }
              >
                <option value="missing_information">Missing information</option>
                <option value="ambiguity">Ambiguity</option>
                <option value="contradiction">Contradiction</option>
                <option value="responsibility_sensitive">
                  Responsibility-sensitive
                </option>
              </select>
            </label>
            <label>
              Owner
              <input
                list={`query-owner-options-${tenderId}`}
                value={ownerKey}
                onChange={(event) => setOwnerKey(event.target.value)}
              />
              <datalist id={`query-owner-options-${tenderId}`}>
                {page.owner_profiles.map((profile) => (
                  <option
                    key={`${profile.profile_id}:${profile.version}`}
                    value={`${profile.profile_id}:${profile.version}`}
                  >
                    {profile.identity} · v{profile.version}
                  </option>
                ))}
              </datalist>
            </label>
            <label>
              Affected task key
              <input
                list={`query-task-options-${tenderId}`}
                value={taskKey}
                onChange={(event) => setTaskKey(event.target.value)}
              />
              <datalist id={`query-task-options-${tenderId}`}>
                {page.production_task_keys.map((key) => (
                  <option key={key} value={key} />
                ))}
              </datalist>
            </label>
            <label>
              Affected Tender Record ID (optional)
              <input
                value={recordId}
                onChange={(event) => setRecordId(event.target.value)}
              />
            </label>
            <label>
              Affected record version
              <input
                min="1"
                type="number"
                value={recordVersion}
                onChange={(event) => setRecordVersion(event.target.value)}
              />
            </label>
            <label>
              Due date
              <input
                type="datetime-local"
                value={dueAt}
                onChange={(event) => setDueAt(event.target.value)}
              />
            </label>
          </div>
          <label>
            Exact question
            <textarea
              value={question}
              onChange={(event) => setQuestion(event.target.value)}
            />
          </label>
          <label>
            Evidence gap / ambiguity
            <textarea
              value={gap}
              onChange={(event) => setGap(event.target.value)}
            />
          </label>
          <div className="form-grid">
            <label>
              Evidence kind
              <input
                value={evidenceKind}
                onChange={(event) => setEvidenceKind(event.target.value)}
              />
            </label>
            <label>
              Exact evidence reference
              <input
                value={evidenceReference}
                onChange={(event) => setEvidenceReference(event.target.value)}
                placeholder="artifact-id#ordinal"
              />
            </label>
            <label>
              Evidence version
              <input
                min="1"
                type="number"
                value={evidenceVersion}
                onChange={(event) => setEvidenceVersion(event.target.value)}
              />
            </label>
          </div>
          <label>
            <input
              type="checkbox"
              checked={material}
              onChange={(event) => setMaterial(event.target.checked)}
            />{" "}
            Material to affected work
          </label>
          <label>
            <input
              type="checkbox"
              checked={releaseBlocking}
              onChange={(event) => setReleaseBlocking(event.target.checked)}
            />{" "}
            Blocks release
          </label>
          <button disabled={!runtimeReady || loading} type="submit">
            Register query
          </button>
        </form>
      ) : null}

      <div className="record-list">
        {page?.items.map((query) => (
          <article
            className="record-card"
            key={`${query.query_id}:${query.version}`}
          >
            <div className="section-heading">
              <div>
                <p className="eyebrow">
                  {query.query_type.replace(/_/g, " ")} · v{query.version}
                </p>
                <h3>{query.question}</h3>
              </div>
              <span className="status-pill">
                {query.status.replace(/_/g, " ")}
              </span>
            </div>
            <p>{query.ambiguity_or_gap}</p>
            <p>
              Owner {query.owner_profile_id} v{query.owner_profile_version} ·
              due {query.due_at}
              {query.overdue ? " · OVERDUE" : ""}
              {queryBlocksRelease(query) ? " · RELEASE BLOCKING" : ""}
            </p>
            <details>
              <summary>Exact Evidence and dependency impact</summary>
              <ul>
                {query.evidence.map((reference) => (
                  <li
                    key={`${reference.kind}:${reference.reference}:${reference.version}`}
                  >
                    {reference.kind}:{reference.reference}:v{reference.version}
                  </li>
                ))}
              </ul>
              <p>Affected tasks: {query.affected_task_keys.join(", ")}</p>
              <p>
                Affected records:{" "}
                {query.affected_records.length
                  ? query.affected_records
                      .map((record) => `${record.record_id}:v${record.version}`)
                      .join(", ")
                  : "none"}
              </p>
              <h4>Typed invalidations</h4>
              {query.invalidations.length ? (
                <ul>
                  {query.invalidations.map((invalidation) => (
                    <li key={invalidation.invalidation_id}>
                      {invalidation.target_kind}:{invalidation.target_id}
                      {invalidation.target_version === null
                        ? ""
                        : `:v${invalidation.target_version}`}{" "}
                      ({invalidation.reason})
                    </li>
                  ))}
                </ul>
              ) : (
                <p>None.</p>
              )}
              {query.proposed_treatments.map((proposal, index) => (
                <p key={`${proposal.treatment}:${index}`}>
                  Proposed {proposal.treatment.replace(/_/g, " ")} by{" "}
                  {proposal.proposed_by}: {proposal.rationale}
                </p>
              ))}
              {query.approved_treatment ? (
                <div>
                  <p>
                    Approved{" "}
                    {query.approved_treatment.treatment.replace(/_/g, " ")} by{" "}
                    {query.approved_treatment.decided_by} as{" "}
                    {query.approved_treatment.acting_role}.
                  </p>
                  <p>Rationale: {query.approved_treatment.rationale}</p>
                  <p>Treatment: {query.approved_treatment.treatment_details}</p>
                  <p>
                    {query.approved_treatment.closes_query
                      ? "Closes this exact Query version."
                      : "Query remains open under this exact treatment."}
                  </p>
                </div>
              ) : null}
              <h4>Responses</h4>
              {query.responses.length ? (
                query.responses.map((response) => (
                  <div key={response.response_id}>
                    <p>
                      {response.registered_by} · {response.created_at}
                    </p>
                    <p>{response.response}</p>
                    <ul>
                      {response.evidence.map((reference) => (
                        <li
                          key={`${response.response_id}:${reference.kind}:${reference.reference}:${reference.version}`}
                        >
                          {reference.kind}:{reference.reference}:v
                          {reference.version}
                        </li>
                      ))}
                    </ul>
                  </div>
                ))
              ) : (
                <p>No registered responses.</p>
              )}
            </details>
            {!query.approved_treatment ? (
              <QueryDecisionForm
                tenderId={tenderId}
                query={query}
                disabled={!runtimeReady || loading}
                onDecide={(command) =>
                  mutate(() => decideTenderQueryTreatment(command))
                }
              />
            ) : null}
            <QueryResponseForm
              query={query}
              disabled={!runtimeReady || loading}
              onRespond={(response, responseEvidence) =>
                mutate(() =>
                  reviseTenderQuery({
                    tender_id: tenderId,
                    query_id: query.query_id,
                    base_version: query.version,
                    query_type: query.query_type,
                    question: query.question,
                    ambiguity_or_gap: query.ambiguity_or_gap,
                    owner_profile_id: query.owner_profile_id,
                    owner_profile_version: query.owner_profile_version,
                    evidence: query.evidence,
                    affected_records: query.affected_records,
                    affected_task_keys: query.affected_task_keys,
                    due_at: query.due_at,
                    material: query.material,
                    release_blocking: query.release_blocking,
                    proposed_treatments: query.proposed_treatments.map(
                      (proposal) => ({
                        treatment: proposal.treatment,
                        rationale: proposal.rationale,
                      }),
                    ),
                    response,
                    response_evidence: [responseEvidence],
                  }),
                )
              }
            />
          </article>
        ))}
      </div>
      {page ? (
        <div className="button-row">
          <button
            type="button"
            disabled={loading || cursorStack.length === 0}
            onClick={() => {
              const previous = cursorStack[cursorStack.length - 1] ?? null;
              void load(previous, cursorStack.slice(0, -1));
            }}
          >
            Newer
          </button>
          <button
            type="button"
            disabled={loading || !page.next_cursor}
            onClick={() =>
              void load(page.next_cursor ?? null, cursorStack.concat(cursor))
            }
          >
            Older
          </button>
        </div>
      ) : null}
    </section>
  );
}

function queryBlocksRelease(query: TenderQuery) {
  return (
    query.release_blocking &&
    (!query.approved_treatment ||
      query.approved_treatment.treatment === "external_rfi_drafting" ||
      query.approved_treatment.treatment === "blocked")
  );
}

function QueryDecisionForm({
  tenderId,
  query,
  disabled,
  onDecide,
}: {
  tenderId: string;
  query: TenderQuery;
  disabled: boolean;
  onDecide: (command: DecideTenderQueryTreatmentCommand) => Promise<boolean>;
}) {
  const [treatment, setTreatment] = useState<TenderQueryTreatment>(
    "approved_assumption",
  );
  const [rationale, setRationale] = useState("");
  const [details, setDetails] = useState("");
  const [closes, setCloses] = useState(false);
  return (
    <form
      className="stacked-form"
      onSubmit={(event) => {
        event.preventDefault();
        if (!rationale.trim() || !details.trim()) return;
        void onDecide({
          tender_id: tenderId,
          query_id: query.query_id,
          query_version: query.version,
          treatment,
          rationale: rationale.trim(),
          treatment_details: details.trim(),
          closes_query: closes,
        }).then((succeeded) => {
          if (succeeded) {
            setRationale("");
            setDetails("");
          }
        });
      }}
    >
      <h4>Manager Query Treatment</h4>
      <select
        value={treatment}
        onChange={(event) =>
          setTreatment(event.target.value as TenderQueryTreatment)
        }
      >
        {treatments.map((candidate) => (
          <option key={candidate} value={candidate}>
            {candidate.replace(/_/g, " ")}
          </option>
        ))}
      </select>
      <textarea
        value={rationale}
        onChange={(event) => setRationale(event.target.value)}
        placeholder="Attributable rationale"
      />
      <textarea
        value={details}
        onChange={(event) => setDetails(event.target.value)}
        placeholder="Exact treatment and consequence"
      />
      <label>
        <input
          type="checkbox"
          checked={closes}
          onChange={(event) => setCloses(event.target.checked)}
        />{" "}
        Close exact Query version
      </label>
      <button disabled={disabled} type="submit">
        Approve treatment
      </button>
    </form>
  );
}

function QueryResponseForm({
  query,
  disabled,
  onRespond,
}: {
  query: TenderQuery;
  disabled: boolean;
  onRespond: (
    response: string,
    evidence: AgentTaskInputReference,
  ) => Promise<boolean>;
}) {
  const [response, setResponse] = useState("");
  const [evidenceKind, setEvidenceKind] = useState("source_evidence");
  const [evidenceReference, setEvidenceReference] = useState("");
  const [evidenceVersion, setEvidenceVersion] = useState("1");
  return (
    <form
      className="stacked-form"
      onSubmit={(event) => {
        event.preventDefault();
        const version = Number(evidenceVersion);
        if (
          !response.trim() ||
          !evidenceKind.trim() ||
          !evidenceReference.trim() ||
          !Number.isInteger(version) ||
          version < 1
        ) {
          return;
        }
        void onRespond(response.trim(), {
          kind: evidenceKind.trim(),
          reference: evidenceReference.trim(),
          version,
        }).then((succeeded) => {
          if (succeeded) {
            setResponse("");
            setEvidenceReference("");
          }
        });
      }}
    >
      <label>
        Register attributable response
        <textarea
          value={response}
          onChange={(event) => setResponse(event.target.value)}
        />
      </label>
      <div className="form-grid">
        <label>
          Response evidence kind
          <input
            value={evidenceKind}
            onChange={(event) => setEvidenceKind(event.target.value)}
          />
        </label>
        <label>
          Exact response evidence reference
          <input
            value={evidenceReference}
            onChange={(event) => setEvidenceReference(event.target.value)}
          />
        </label>
        <label>
          Response evidence version
          <input
            min="1"
            type="number"
            value={evidenceVersion}
            onChange={(event) => setEvidenceVersion(event.target.value)}
          />
        </label>
      </div>
      <button disabled={disabled || !query.current} type="submit">
        Publish response as successor
      </button>
    </form>
  );
}
