import { useCallback, useEffect, useMemo, useState } from "react";

import type { DocumentRegister } from "./bindings/DocumentRegister";
import type { EvidenceDocument } from "./bindings/EvidenceDocument";
import type { TenderEvidenceReference } from "./bindings/TenderEvidenceReference";
import type { TenderRecordAuthority } from "./bindings/TenderRecordAuthority";
import type { TenderRecordEngineerDecisionKind } from "./bindings/TenderRecordEngineerDecisionKind";
import type { TenderRecordInspection } from "./bindings/TenderRecordInspection";
import type { TenderRecordPage } from "./bindings/TenderRecordPage";
import type { TenderRecordTrustClass } from "./bindings/TenderRecordTrustClass";
import {
  decideTenderRecord,
  createTenderEngineerEntry,
  inspectEvidence,
  inspectTenderRecordAuthorities,
  inspectTenderRecords,
  runTenderRecordExtraction,
  runTenderRecordReview,
} from "./quantixHost";

interface TenderRecordsPanelProps {
  tenderId: string;
  runtimeReady: boolean;
  register?: DocumentRegister;
  reportCommandFailure: () => void;
}

const EVIDENCE_BATCH_SIZE = 256;
const EVIDENCE_BATCH_BYTES = 384 * 1024;
const SELECTED_EVIDENCE_BYTES = 1024 * 1024;
const RECORD_PAGE_SIZE = 4;

const trustCopy: Record<TenderRecordTrustClass, string> = {
  ai_proposal: "AI proposal",
  deterministic_fact: "Deterministic fact",
  verified: "Independently verified",
  engineer_verified: "Engineer verified",
  approved_assumption: "Approved assumption",
  unresolved_gap: "Unresolved gap",
  prior_decision: "Prior decision",
};

interface SelectedEvidence {
  reference: TenderEvidenceReference;
  label: string;
  estimatedBytes: number;
}

function sourceKey(artifactId: string, version: number) {
  return `${artifactId}:${version}`;
}

function boundedEvidenceBatches(document?: EvidenceDocument) {
  if (!document) return [];
  const encoder = new TextEncoder();
  const batches: EvidenceDocument["locations"][] = [];
  let current: EvidenceDocument["locations"] = [];
  let currentBytes = 0;
  for (const location of document.locations) {
    const locationBytes = encoder.encode(JSON.stringify(location)).length;
    if (
      current.length > 0 &&
      (current.length >= EVIDENCE_BATCH_SIZE ||
        currentBytes + locationBytes > EVIDENCE_BATCH_BYTES)
    ) {
      batches.push(current);
      current = [];
      currentBytes = 0;
    }
    current.push(location);
    currentBytes += locationBytes;
  }
  if (current.length > 0) batches.push(current);
  return batches;
}

export function TenderRecordsPanel({
  tenderId,
  runtimeReady,
  register,
  reportCommandFailure,
}: TenderRecordsPanelProps) {
  const [page, setPage] = useState<TenderRecordPage>({
    records: [],
    next_cursor: null,
  });
  const [cursor, setCursor] = useState<string | null>(null);
  const [cursorHistory, setCursorHistory] = useState<(string | null)[]>([]);
  const [busy, setBusy] = useState(false);
  const [selectedSource, setSelectedSource] = useState("");
  const [evidenceDocument, setEvidenceDocument] = useState<EvidenceDocument>();
  const [evidenceBatchIndex, setEvidenceBatchIndex] = useState(0);
  const [authorities, setAuthorities] = useState<TenderRecordAuthority[]>([]);
  const [selectedAuthorityIds, setSelectedAuthorityIds] = useState<Set<string>>(
    () => new Set(),
  );
  const [selectedEvidence, setSelectedEvidence] = useState<SelectedEvidence[]>(
    [],
  );

  const loadRecordPage = useCallback(
    async (nextCursor: string | null) => {
      try {
        setPage(
          await inspectTenderRecords(tenderId, nextCursor, RECORD_PAGE_SIZE),
        );
        setCursor(nextCursor);
      } catch {
        reportCommandFailure();
      }
    },
    [reportCommandFailure, tenderId],
  );

  const refresh = useCallback(async () => {
    await loadRecordPage(cursor);
  }, [cursor, loadRecordPage]);

  useEffect(() => {
    void loadRecordPage(null);
    setCursorHistory([]);
  }, [loadRecordPage]);

  const refreshAuthorities = useCallback(async () => {
    try {
      const nextAuthorities = await inspectTenderRecordAuthorities(tenderId);
      setAuthorities(nextAuthorities);
      const validIds = new Set(
        nextAuthorities.map((authority) => authority.authority_id),
      );
      setSelectedAuthorityIds(
        (selected) => new Set([...selected].filter((id) => validIds.has(id))),
      );
    } catch {
      reportCommandFailure();
    }
  }, [reportCommandFailure, tenderId]);

  useEffect(() => {
    void refreshAuthorities();
  }, [refreshAuthorities]);

  const parsedDocuments = useMemo(
    () =>
      register?.documents.filter(
        (document) =>
          document.registration_state === "registered" &&
          document.parse_state === "parsed",
      ) ?? [],
    [register],
  );

  useEffect(() => {
    if (
      !parsedDocuments.some(
        (document) =>
          sourceKey(document.artifact_id, document.version) === selectedSource,
      )
    ) {
      const first = parsedDocuments[0];
      setSelectedSource(
        first ? sourceKey(first.artifact_id, first.version) : "",
      );
    }
  }, [parsedDocuments, selectedSource]);

  const selectedDocument = parsedDocuments.find(
    (document) =>
      sourceKey(document.artifact_id, document.version) === selectedSource,
  );

  useEffect(() => {
    setEvidenceDocument(undefined);
    setEvidenceBatchIndex(0);
    if (!selectedDocument) return;
    let active = true;
    void inspectEvidence(
      tenderId,
      selectedDocument.artifact_id,
      selectedDocument.version,
    )
      .then((document) => {
        if (active) setEvidenceDocument(document);
      })
      .catch(() => {
        if (active) reportCommandFailure();
      });
    return () => {
      active = false;
    };
  }, [reportCommandFailure, selectedDocument, tenderId]);

  const evidenceBatches = useMemo(
    () => boundedEvidenceBatches(evidenceDocument),
    [evidenceDocument],
  );
  const evidenceBatchCount = evidenceBatches.length;
  const currentEvidenceBatch = evidenceBatches[evidenceBatchIndex] ?? [];
  const currentEvidenceBatchBytes = currentEvidenceBatch.reduce(
    (total, location) =>
      total + new TextEncoder().encode(JSON.stringify(location)).length,
    0,
  );
  const selectedEvidenceBytes = selectedEvidence.reduce(
    (total, evidence) => total + evidence.estimatedBytes,
    0,
  );
  const currentBatchNewCount = currentEvidenceBatch.filter(
    (location) =>
      !selectedEvidence.some(
        (selected) =>
          selected.reference.artifact_id === evidenceDocument?.artifact_id &&
          selected.reference.version === evidenceDocument?.version &&
          selected.reference.ordinal === location.ordinal,
      ),
  ).length;
  const canAddCurrentBatch =
    !!evidenceDocument &&
    currentBatchNewCount > 0 &&
    selectedEvidence.length + currentBatchNewCount <= EVIDENCE_BATCH_SIZE &&
    selectedEvidenceBytes + currentEvidenceBatchBytes <=
      SELECTED_EVIDENCE_BYTES;

  const addCurrentEvidenceBatch = () => {
    if (!evidenceDocument || !canAddCurrentBatch) return;
    setSelectedEvidence((selected) => {
      const existing = new Set(
        selected.map(
          (item) =>
            `${item.reference.artifact_id}:${item.reference.version}:${item.reference.ordinal}`,
        ),
      );
      const additions = currentEvidenceBatch
        .filter(
          (location) =>
            !existing.has(
              `${evidenceDocument.artifact_id}:${evidenceDocument.version}:${location.ordinal}`,
            ),
        )
        .map((location) => ({
          reference: {
            artifact_id: evidenceDocument.artifact_id,
            version: evidenceDocument.version,
            ordinal: location.ordinal,
          },
          label: `${selectedDocument?.package_path ?? evidenceDocument.artifact_id} · v${evidenceDocument.version} · #${location.ordinal}`,
          estimatedBytes: new TextEncoder().encode(JSON.stringify(location))
            .length,
        }));
      return [...selected, ...additions];
    });
  };

  const extractRecords = async () => {
    if (selectedEvidence.length === 0) return;
    setBusy(true);
    try {
      await runTenderRecordExtraction(
        tenderId,
        selectedEvidence.map((evidence) => evidence.reference),
        [...selectedAuthorityIds].map((authorityId) => ({
          authority_id: authorityId,
        })),
      );
      setCursorHistory([]);
      await loadRecordPage(null);
    } catch {
      reportCommandFailure();
    } finally {
      setBusy(false);
    }
  };

  const addEngineerEntry = async () => {
    const value = window.prompt(
      "Enter the exact Engineer-provided value to make available as attributable record basis.",
    );
    if (!value?.trim()) return;
    const description = window.prompt(
      "Describe the engineering basis and why this entry is attributable.",
    );
    if (!description?.trim()) return;
    setBusy(true);
    try {
      const entry = await createTenderEngineerEntry(
        tenderId,
        value.trim(),
        description.trim(),
      );
      await refreshAuthorities();
      setSelectedAuthorityIds((selected) =>
        new Set(selected).add(entry.authority_id),
      );
    } catch {
      reportCommandFailure();
    } finally {
      setBusy(false);
    }
  };

  const reviewRecord = async (record: TenderRecordInspection) => {
    setBusy(true);
    try {
      await runTenderRecordReview(tenderId, record.record_id, record.version);
      await refresh();
    } catch {
      reportCommandFailure();
    } finally {
      setBusy(false);
    }
  };

  const engineerDecision = async (
    record: TenderRecordInspection,
    decision: TenderRecordEngineerDecisionKind,
  ) => {
    const rationale = window.prompt(
      "Record the Engineer rationale for this immutable exact-version decision.",
    );
    if (!rationale?.trim()) return;
    setBusy(true);
    try {
      await decideTenderRecord(
        tenderId,
        record.record_id,
        record.version,
        decision,
        rationale.trim(),
      );
      await refresh();
    } catch {
      reportCommandFailure();
    } finally {
      setBusy(false);
    }
  };

  const nextPage = async () => {
    if (!page.next_cursor) return;
    setCursorHistory((history) => [...history, cursor]);
    await loadRecordPage(page.next_cursor);
  };

  const previousPage = async () => {
    const previousCursor = cursorHistory[cursorHistory.length - 1];
    if (previousCursor === undefined) return;
    setCursorHistory((history) => history.slice(0, -1));
    await loadRecordPage(previousCursor);
  };

  return (
    <section className="tender-records" aria-labelledby="tender-records-title">
      <div className="tender-records__heading">
        <div>
          <p className="section-label">Evidence-backed analysis</p>
          <h4 id="tender-records-title">Tender Records</h4>
        </div>
        <button
          type="button"
          onClick={() => void extractRecords()}
          disabled={busy || !runtimeReady || selectedEvidence.length === 0}
        >
          Propose from selected evidence
        </button>
        <button
          type="button"
          className="button-secondary"
          onClick={() => void addEngineerEntry()}
          disabled={busy}
        >
          Add Engineer entry
        </button>
      </div>
      <p>
        Original source text is authoritative. Proposed values remain distinct
        from verified records, controlled assumptions, and unresolved gaps.
      </p>
      {parsedDocuments.length > 0 ? (
        <div className="record-extraction-controls">
          <label>
            Authoritative source
            <select
              value={selectedSource}
              onChange={(event) => setSelectedSource(event.target.value)}
              disabled={busy}
            >
              {parsedDocuments.map((document) => (
                <option
                  key={sourceKey(document.artifact_id, document.version)}
                  value={sourceKey(document.artifact_id, document.version)}
                >
                  {document.package_path} · v{document.version}
                </option>
              ))}
            </select>
          </label>
          <label>
            Evidence batch
            <select
              value={evidenceBatchIndex}
              onChange={(event) =>
                setEvidenceBatchIndex(Number(event.target.value))
              }
              disabled={busy || evidenceBatchCount === 0}
            >
              {Array.from({ length: evidenceBatchCount }, (_, index) => (
                <option key={index} value={index}>
                  {index + 1} of {evidenceBatchCount} ·{" "}
                  {evidenceBatches[index].length} locations · #
                  {evidenceBatches[index][0]?.ordinal}–#
                  {
                    evidenceBatches[index][evidenceBatches[index].length - 1]
                      ?.ordinal
                  }
                </option>
              ))}
            </select>
          </label>
          <button
            type="button"
            className="button-secondary"
            onClick={addCurrentEvidenceBatch}
            disabled={busy || !canAddCurrentBatch}
          >
            Add batch to run
          </button>
        </div>
      ) : null}
      {selectedEvidence.length > 0 ? (
        <details className="record-evidence-selection" open>
          <summary>
            {selectedEvidence.length} exact locations selected across source
            versions
          </summary>
          <button
            type="button"
            className="button-secondary"
            onClick={() => setSelectedEvidence([])}
            disabled={busy}
          >
            Clear evidence selection
          </button>
          <ul>
            {selectedEvidence.map((evidence) => (
              <li
                key={`${evidence.reference.artifact_id}:${evidence.reference.version}:${evidence.reference.ordinal}`}
              >
                {evidence.label}
              </li>
            ))}
          </ul>
        </details>
      ) : (
        <p className="catalogue-message">
          Add one or more bounded batches from any parsed source version. This
          permits exact original/addendum comparison without loading the whole
          Tender into one run.
        </p>
      )}
      {authorities.length > 0 ? (
        <details className="record-authorities">
          <summary>
            {selectedAuthorityIds.size} of {authorities.length} attributable
            non-source basis entries selected for this run
          </summary>
          <ul>
            {authorities.map((authority) => (
              <li key={authority.authority_id}>
                <label>
                  <input
                    type="checkbox"
                    checked={selectedAuthorityIds.has(authority.authority_id)}
                    onChange={(event) => {
                      setSelectedAuthorityIds((selected) => {
                        const next = new Set(selected);
                        if (event.target.checked) {
                          next.add(authority.authority_id);
                        } else {
                          next.delete(authority.authority_id);
                        }
                        return next;
                      });
                    }}
                    disabled={busy}
                  />{" "}
                  <strong>{authority.kind.replace(/_/g, " ")}</strong>:{" "}
                  {authority.value}
                  {" — "}
                  {authority.description} ({authority.created_by})
                </label>
              </li>
            ))}
          </ul>
        </details>
      ) : null}
      <div
        className="record-trust-legend"
        aria-label="Tender Record trust classes"
      >
        {Object.entries(trustCopy).map(([trust, label]) => (
          <span key={trust} data-trust={trust}>
            {label}
          </span>
        ))}
      </div>
      {page.records.length === 0 ? (
        <p className="catalogue-message">
          No Tender Records yet. Parse authoritative documents, then run the
          restricted Tender Analyst on a bounded evidence batch.
        </p>
      ) : (
        <div className="tender-record-list">
          {page.records.map((record) => (
            <article
              className="tender-record-card"
              data-trust={record.trust_class}
              key={`${record.record_id}:${record.version}`}
            >
              <header>
                <div>
                  <p className="section-label">
                    {record.kind.replace(/_/g, " ")} · version {record.version}
                  </p>
                  <h5>{record.title}</h5>
                </div>
                <span className="record-trust-badge">
                  {trustCopy[record.trust_class]}
                </span>
              </header>
              <p className="record-identity">
                Exact target {record.record_id} · {record.verification_status}
              </p>
              {record.fields.map((field) => (
                <section className="record-field" key={field.name}>
                  <h6>{field.name.replace(/_/g, " ")}</h6>
                  <p>
                    {field.value ?? "No supported value in supplied Evidence"}
                  </p>
                  <p className="record-basis">
                    Basis: {field.basis_kind.replace(/_/g, " ")}
                    {field.basis_description
                      ? ` — ${field.basis_description}`
                      : ""}
                  </p>
                  {field.basis_authority ? (
                    <p className="record-basis-attribution">
                      Exact authority {field.basis_authority.authority_id} ·
                      recorded by {field.basis_authority.created_by} at{" "}
                      {field.basis_authority.created_at}
                    </p>
                  ) : null}
                  {field.original_expression ? (
                    <dl className="deadline-normalization">
                      <div>
                        <dt>Original expression</dt>
                        <dd>{field.original_expression}</dd>
                      </div>
                      <div>
                        <dt>Timezone</dt>
                        <dd>{field.timezone}</dd>
                      </div>
                      <div>
                        <dt>Normalized value</dt>
                        <dd>{field.normalized_value ?? "Not normalized"}</dd>
                      </div>
                      <div>
                        <dt>Uncertainty</dt>
                        <dd>{field.uncertainty ?? "None recorded"}</dd>
                      </div>
                    </dl>
                  ) : null}
                  {field.evidence.map((evidence) => (
                    <details
                      className="record-evidence"
                      key={`${evidence.reference.artifact_id}:${evidence.reference.version}:${evidence.reference.ordinal}`}
                    >
                      <summary>
                        {evidence.package_path} · v{evidence.reference.version}{" "}
                        · {evidence.location.kind} #{evidence.reference.ordinal}
                      </summary>
                      <blockquote lang={evidence.location.language}>
                        {evidence.location.original_text}
                      </blockquote>
                      {evidence.location.translated_text ? (
                        <div className="derived-translation">
                          <strong>
                            Derived translation — non-authoritative
                          </strong>
                          <blockquote>
                            {evidence.location.translated_text}
                          </blockquote>
                        </div>
                      ) : null}
                    </details>
                  ))}
                </section>
              ))}
              {record.contradictions.map((contradiction) => (
                <aside
                  className="record-contradiction"
                  key={contradiction.field_name}
                >
                  <strong>Contradiction: {contradiction.field_name}</strong>
                  <p>{contradiction.summary}</p>
                  <span>{contradiction.evidence.length} exact citations</span>
                </aside>
              ))}
              {record.source_relationships.map((relationship) => (
                <p
                  className="record-relationship"
                  key={relationship.relationship_id}
                >
                  Confirmed {relationship.relationship_kind}:{" "}
                  {relationship.prior_artifact_id} v{relationship.prior_version}{" "}
                  → {relationship.replacement_artifact_id} v
                  {relationship.replacement_version}
                </p>
              ))}
              {record.reviews.map((review) => (
                <p className="record-review" key={review.review_id}>
                  {review.outcome.replace(/_/g, " ")} by {review.decided_by}:{" "}
                  {review.rationale}
                </p>
              ))}
              <footer className="record-actions">
                <button
                  type="button"
                  className="button-secondary"
                  disabled={
                    busy ||
                    !runtimeReady ||
                    record.verification_status !== "proposed" ||
                    record.reviews.length > 0
                  }
                  onClick={() => void reviewRecord(record)}
                >
                  Independent review of v{record.version}
                </button>
                {record.kind === "assumption" ? (
                  <button
                    type="button"
                    disabled={busy}
                    onClick={() =>
                      void engineerDecision(record, "approve_assumption")
                    }
                  >
                    Approve assumption
                  </button>
                ) : (
                  <button
                    type="button"
                    disabled={busy}
                    onClick={() => void engineerDecision(record, "verify")}
                  >
                    Engineer verify
                  </button>
                )}
                <button
                  type="button"
                  className="button-secondary"
                  disabled={busy}
                  onClick={() => void engineerDecision(record, "reject")}
                >
                  Reject
                </button>
              </footer>
            </article>
          ))}
          <nav className="record-pagination" aria-label="Tender Record pages">
            <button
              type="button"
              className="button-secondary"
              disabled={busy || cursorHistory.length === 0}
              onClick={() => void previousPage()}
            >
              Previous records
            </button>
            <button
              type="button"
              className="button-secondary"
              disabled={busy || page.next_cursor === null}
              onClick={() => void nextPage()}
            >
              Next records
            </button>
          </nav>
        </div>
      )}
    </section>
  );
}
