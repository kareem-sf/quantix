import { LoaderCircle } from "lucide-react";
import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type FormEvent,
} from "react";

import type { ExternalRfiDraft } from "./bindings/ExternalRfiDraft";
import type { ExternalRfiEligibleQueryPage } from "./bindings/ExternalRfiEligibleQueryPage";
import type { ExternalRfiExportRecord } from "./bindings/ExternalRfiExportRecord";
import type { ExternalRfiPage } from "./bindings/ExternalRfiPage";
import type { ExternalRfiQueryReference } from "./bindings/ExternalRfiQueryReference";
import type { ExternalRfiResponseCandidatePage } from "./bindings/ExternalRfiResponseCandidatePage";
import type { TenderQueryTreatment } from "./bindings/TenderQueryTreatment";
import type { WorkspaceExternalRfiSummary } from "./bindings/WorkspaceExternalRfiSummary";
import {
  approveExternalRfiForIssue,
  createExternalRfiDraft,
  exportApprovedExternalRfi,
  inspectExternalRfiEligibleQueries,
  inspectExternalRfiResponseCandidates,
  inspectExternalRfis,
  interpretExternalRfiResponse,
  registerExternalRfiResponse,
  reviseExternalRfiDraft,
  runExternalRfiReview,
} from "./quantixHost";

interface TenderRfiReviewProps {
  tenderId: string;
  summaries: WorkspaceExternalRfiSummary[];
  interpretationFirst: boolean;
  runtimeReady: boolean;
  reportCommandFailure: () => void;
  onRefresh: () => Promise<void>;
  onClose: () => void;
}

const REVIEW_PENDING_STATUSES = [
  "awaiting_review",
  "review_failed",
  "awaiting_approval",
] as const;

const TREATMENT_OPTIONS: Array<{
  value: TenderQueryTreatment;
  label: string;
}> = [
  { value: "internal_resolution", label: "Resolved internally" },
  { value: "approved_assumption", label: "Approved assumption" },
  { value: "qualification", label: "Qualification" },
  { value: "exclusion", label: "Exclusion" },
  { value: "allowance", label: "Allowance" },
  { value: "blocked", label: "Blocked" },
];

const STATUS_LABELS: Record<WorkspaceExternalRfiSummary["status"], string> = {
  awaiting_review: "Waiting for independent review",
  review_failed: "Review found problems",
  awaiting_approval: "Review passed — awaiting approval",
  approved_for_issue: "Approved for issue",
  response_awaiting_interpretation: "Response awaiting interpretation",
  query_basis_stale: "The exact question basis changed",
};

interface DraftFormState {
  revision: { rfiId: string; baseVersion: number } | null;
  queryIds: string[];
  contractualContext: string;
  responseNeed: string;
  dueAt: string;
  organization: string;
  attention: string;
  email: string;
  commitments: string;
}

const emptyDraftForm = (): DraftFormState => ({
  revision: null,
  queryIds: [],
  contractualContext: "",
  responseNeed: "",
  dueAt: "",
  organization: "",
  attention: "",
  email: "",
  commitments: "",
});

function formatDateTime(value: string): string {
  return new Date(value).toLocaleString();
}

function lines(value: string): string[] {
  return value
    .split("\n")
    .map((item) => item.trim())
    .filter(Boolean);
}

function responseDocumentKey(artifactId: string, version: number): string {
  return `${artifactId}:${version}`;
}

export function TenderRfiReview({
  tenderId,
  summaries,
  interpretationFirst,
  runtimeReady,
  reportCommandFailure,
  onRefresh,
  onClose,
}: TenderRfiReviewProps) {
  const [page, setPage] = useState<ExternalRfiPage | null>(null);
  const [eligible, setEligible] = useState<ExternalRfiEligibleQueryPage | null>(
    null,
  );
  const [loading, setLoading] = useState(true);
  const [pending, setPending] = useState(false);
  const [form, setForm] = useState<DraftFormState>(emptyDraftForm);
  const [approvalRationale, setApprovalRationale] = useState("");
  const [exportRecord, setExportRecord] =
    useState<ExternalRfiExportRecord | null>(null);
  const [responseCandidates, setResponseCandidates] =
    useState<ExternalRfiResponseCandidatePage | null>(null);
  const [selectedResponseKey, setSelectedResponseKey] = useState("");
  const [interpretation, setInterpretation] = useState({
    text: "",
    treatment: "qualification" as TenderQueryTreatment,
    rationale: "",
    treatmentDetails: "",
    material: true,
    closesQuery: false,
  });
  const requestGeneration = useRef(0);

  const loadRfis = useCallback(
    async (nextEligibleCursor: string | null = null) => {
      const generation = ++requestGeneration.current;
      setLoading(true);
      try {
        const [nextPage, nextEligible] = await Promise.all([
          inspectExternalRfis(tenderId, null, 8),
          inspectExternalRfiEligibleQueries(tenderId, nextEligibleCursor, 8),
        ]);
        if (generation !== requestGeneration.current) return;
        setPage(nextPage);
        setEligible(nextEligible);
      } catch {
        if (generation === requestGeneration.current) reportCommandFailure();
      } finally {
        if (generation === requestGeneration.current) setLoading(false);
      }
    },
    [reportCommandFailure, tenderId],
  );

  useEffect(() => {
    void loadRfis();
    return () => {
      requestGeneration.current += 1;
    };
  }, [loadRfis]);

  const mutate = useCallback(
    async (work: () => Promise<unknown>): Promise<boolean> => {
      setPending(true);
      try {
        await work();
        await loadRfis();
        await onRefresh();
        return true;
      } catch {
        reportCommandFailure();
        return false;
      } finally {
        setPending(false);
      }
    },
    [loadRfis, onRefresh, reportCommandFailure],
  );

  const reviewSummary =
    summaries.find((summary) =>
      (REVIEW_PENDING_STATUSES as readonly string[]).includes(summary.status),
    ) ??
    summaries.find(
      (summary) =>
        summary.status !== "response_awaiting_interpretation" &&
        summary.status !== "approved_for_issue",
    ) ??
    null;
  const responseSummary =
    summaries.find(
      (summary) => summary.status === "response_awaiting_interpretation",
    ) ??
    summaries.find((summary) => summary.status === "approved_for_issue") ??
    null;

  const reviewDraft = page?.items.find(
    (item) => item.rfi_id === reviewSummary?.rfi_id,
  );
  const responseDraft = page?.items.find(
    (item) => item.rfi_id === responseSummary?.rfi_id,
  );

  const eligibleQueries = eligible?.items ?? [];
  const queryRefsById = useMemo(() => {
    const references = new Map<string, ExternalRfiQueryReference>();
    for (const item of eligibleQueries) {
      references.set(item.query_ref.query_id, item.query_ref);
    }
    for (const reference of reviewDraft?.current_query_refs ?? []) {
      references.set(reference.query_id, reference);
    }
    return references;
  }, [eligibleQueries, reviewDraft]);

  const approval = responseDraft?.approval ?? null;

  useEffect(() => {
    if (!approval) {
      setResponseCandidates(null);
      setSelectedResponseKey("");
      return;
    }
    let active = true;
    inspectExternalRfiResponseCandidates(tenderId, approval.approval_id)
      .then((nextPage) => {
        if (active) setResponseCandidates(nextPage);
      })
      .catch(() => {
        if (active) reportCommandFailure();
      });
    return () => {
      active = false;
    };
  }, [approval, reportCommandFailure, tenderId]);

  useEffect(() => {
    if (form.revision || eligible === null) return;
    setForm((current) => ({
      ...current,
      queryIds: current.queryIds.filter((queryId) =>
        queryRefsById.has(queryId),
      ),
    }));
  }, [eligible, form.revision, queryRefsById]);

  const beginRevision = (draft: ExternalRfiDraft) => {
    setForm({
      revision: { rfiId: draft.rfi_id, baseVersion: draft.version },
      queryIds: draft.current_query_refs.map((reference) => reference.query_id),
      contractualContext: draft.contractual_context,
      responseNeed: draft.response_need,
      dueAt: draft.due_at.slice(0, 16),
      organization: draft.recipient.organization,
      attention: draft.recipient.attention,
      email: draft.recipient.email ?? "",
      commitments: draft.affected_commitments.join("\n"),
    });
  };

  const submitDraftForm = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const queryRefs = form.queryIds
      .map((queryId) => queryRefsById.get(queryId))
      .filter((reference): reference is ExternalRfiQueryReference =>
        Boolean(reference),
      );
    const dueAt = new Date(form.dueAt);
    if (
      queryRefs.length === 0 ||
      !form.contractualContext.trim() ||
      !form.responseNeed.trim() ||
      Number.isNaN(dueAt.getTime()) ||
      !form.organization.trim() ||
      !form.attention.trim()
    ) {
      return;
    }
    const shared = {
      query_refs: queryRefs,
      contractual_context: form.contractualContext.trim(),
      response_need: form.responseNeed.trim(),
      attachments: form.revision ? (reviewDraft?.attachments ?? []) : [],
      due_at: dueAt.toISOString(),
      recipient: {
        organization: form.organization.trim(),
        attention: form.attention.trim(),
        email: form.email.trim() || null,
      },
      affected_commitments: lines(form.commitments),
    };
    const revision = form.revision;
    void mutate(async () => {
      if (revision && reviewDraft) {
        await reviseExternalRfiDraft({
          tender_id: tenderId,
          rfi_id: revision.rfiId,
          base_version: revision.baseVersion,
          additional_evidence: reviewDraft.source_evidence,
          ...shared,
        });
      } else {
        await createExternalRfiDraft({
          tender_id: tenderId,
          additional_evidence: [],
          ...shared,
        });
      }
      setForm(emptyDraftForm());
    });
  };

  const runReview = (draft: ExternalRfiDraft) => {
    void mutate(() =>
      runExternalRfiReview({
        tender_id: tenderId,
        rfi_id: draft.rfi_id,
        version: draft.version,
      }),
    );
  };

  const approveForIssue = (draft: ExternalRfiDraft) => {
    const rationale = approvalRationale.trim();
    if (!rationale) return;
    void mutate(async () => {
      await approveExternalRfiForIssue({
        tender_id: tenderId,
        rfi_id: draft.rfi_id,
        version: draft.version,
        manifest_sha256: draft.manifest_sha256,
        rationale,
      });
      setApprovalRationale("");
    });
  };

  const exportForIssue = (draft: ExternalRfiDraft) => {
    const approvalForExport = draft.approval;
    if (!approvalForExport || !draft.approved_for_issue) return;
    void mutate(async () => {
      const record = await exportApprovedExternalRfi({
        tender_id: tenderId,
        rfi_id: draft.rfi_id,
        version: draft.version,
        approval_sha256: approvalForExport.approval_sha256,
      });
      setExportRecord(record);
    });
  };

  const registerResponse = (draft: ExternalRfiDraft) => {
    const approvalForResponse = draft.approval;
    const selected = responseCandidates?.items.find(
      (candidate) =>
        responseDocumentKey(
          candidate.source_artifact_id,
          candidate.source_artifact_version,
        ) === selectedResponseKey,
    );
    if (!approvalForResponse || !selected) return;
    void mutate(() =>
      registerExternalRfiResponse({
        tender_id: tenderId,
        rfi_id: draft.rfi_id,
        rfi_version: draft.version,
        approval_id: approvalForResponse.approval_id,
        source_artifact_id: selected.source_artifact_id,
        source_artifact_version: selected.source_artifact_version,
      }),
    ).then((succeeded) => {
      if (succeeded) setSelectedResponseKey("");
    });
  };

  const pendingInterpretation = responseDraft
    ? (responseDraft.responses.flatMap((response) =>
        responseDraft.query_refs
          .filter(
            (reference) =>
              !responseDraft.interpretations.some(
                (candidate) =>
                  candidate.response_link_id === response.response_link_id &&
                  candidate.query_id === reference.query_id,
              ),
          )
          .map((reference) => ({
            response,
            reference,
          })),
      )[0] ?? null)
    : null;

  const recordInterpretation = () => {
    if (!pendingInterpretation || !responseDraft) return;
    const currentReference = responseDraft.current_query_refs.find(
      (candidate) =>
        candidate.query_id === pendingInterpretation.reference.query_id,
    );
    if (
      !currentReference ||
      !interpretation.text.trim() ||
      !interpretation.rationale.trim() ||
      !interpretation.treatmentDetails.trim()
    ) {
      return;
    }
    void mutate(() =>
      interpretExternalRfiResponse({
        tender_id: tenderId,
        response_link_id: pendingInterpretation.response.response_link_id,
        query_id: pendingInterpretation.reference.query_id,
        issued_query_version: pendingInterpretation.reference.version,
        base_query_version: currentReference.version,
        base_query_manifest_sha256: currentReference.manifest_sha256,
        material: interpretation.material,
        interpretation: interpretation.text.trim(),
        treatment: interpretation.treatment,
        rationale: interpretation.rationale.trim(),
        treatment_details: interpretation.treatmentDetails.trim(),
        closes_query: interpretation.closesQuery,
      }),
    ).then((succeeded) => {
      if (!succeeded) return;
      setInterpretation({
        text: "",
        treatment: "qualification",
        rationale: "",
        treatmentDetails: "",
        material: true,
        closesQuery: false,
      });
      onClose();
    });
  };

  const eligibleReady =
    eligibleQueries.length > 0 || eligible?.next_cursor != null;

  const renderQuestions = (draft: ExternalRfiDraft) => (
    <div className="tender-rfi__questions">
      <h4>Questions asked</h4>
      <ul>
        {draft.questions.map((question) => (
          <li key={`${question.query_id}:${question.query_version}`}>
            <strong>{question.question}</strong>
            <span>{question.ambiguity_or_gap}</span>
            <code>
              Tender question {question.query_id} · v{question.query_version}
            </code>
          </li>
        ))}
      </ul>
    </div>
  );

  const renderDraftForm = () => (
    <form
      className="tender-rfi__form"
      onSubmit={submitDraftForm}
      data-testid="tender-rfi-gather-form"
    >
      <fieldset className="tender-rfi__queries">
        <legend>
          {form.revision ? "Questions in this draft" : "Questions to include"}
        </legend>
        {eligibleQueries.map((item) => (
          <label
            key={`${item.query_ref.query_id}:${item.query_ref.version}`}
            className="tender-rfi__checkbox"
          >
            <input
              type="checkbox"
              checked={form.queryIds.includes(item.query_ref.query_id)}
              disabled={pending || loading || form.revision === null}
              onChange={(event) =>
                setForm((current) => ({
                  ...current,
                  queryIds: event.target.checked
                    ? [...current.queryIds, item.query_ref.query_id]
                    : current.queryIds.filter(
                        (queryId) => queryId !== item.query_ref.query_id,
                      ),
                }))
              }
            />
            <span>
              {item.question}
              <small>
                {item.ambiguity_or_gap} · response needed by{" "}
                {formatDateTime(item.due_at)}
              </small>
            </span>
          </label>
        ))}
        {form.revision
          ? (reviewDraft?.current_query_refs ?? [])
              .filter(
                (reference) =>
                  !eligibleQueries.some(
                    (item) => item.query_ref.query_id === reference.query_id,
                  ),
              )
              .map((reference) => (
                <label
                  key={`${reference.query_id}:${reference.version}`}
                  className="tender-rfi__checkbox"
                >
                  <input
                    type="checkbox"
                    checked={form.queryIds.includes(reference.query_id)}
                    disabled
                  />
                  <span>
                    Current question {reference.query_id}
                    <small>
                      Kept from the current draft · v{reference.version}
                    </small>
                  </span>
                </label>
              ))
          : null}
        {eligible?.next_cursor ? (
          <button
            type="button"
            className="manager-workspace__text-button"
            disabled={loading}
            onClick={() => void loadRfis(eligible.next_cursor)}
          >
            Show more routed questions
          </button>
        ) : null}
      </fieldset>
      <label>
        Exact contractual context
        <textarea
          value={form.contractualContext}
          disabled={pending || loading}
          onChange={(event) =>
            setForm((current) => ({
              ...current,
              contractualContext: event.target.value,
            }))
          }
        />
      </label>
      <label>
        Response needed
        <textarea
          value={form.responseNeed}
          disabled={pending || loading}
          onChange={(event) =>
            setForm((current) => ({
              ...current,
              responseNeed: event.target.value,
            }))
          }
        />
      </label>
      <label>
        Response needed by
        <input
          type="datetime-local"
          value={form.dueAt}
          disabled={pending || loading}
          onChange={(event) =>
            setForm((current) => ({ ...current, dueAt: event.target.value }))
          }
        />
      </label>
      <label>
        Recipient organization
        <input
          value={form.organization}
          disabled={pending || loading}
          onChange={(event) =>
            setForm((current) => ({
              ...current,
              organization: event.target.value,
            }))
          }
        />
      </label>
      <label>
        Attention
        <input
          value={form.attention}
          disabled={pending || loading}
          onChange={(event) =>
            setForm((current) => ({
              ...current,
              attention: event.target.value,
            }))
          }
        />
      </label>
      <label>
        Proposed email (optional)
        <input
          type="email"
          value={form.email}
          disabled={pending || loading}
          onChange={(event) =>
            setForm((current) => ({ ...current, email: event.target.value }))
          }
        />
      </label>
      <label>
        Affected commitments (one per line)
        <textarea
          value={form.commitments}
          disabled={pending || loading}
          onChange={(event) =>
            setForm((current) => ({
              ...current,
              commitments: event.target.value,
            }))
          }
        />
      </label>
      <div className="tender-rfi__actions">
        <button
          type="submit"
          className="manager-workspace__primary"
          disabled={pending || loading || form.queryIds.length === 0}
        >
          {form.revision
            ? "Publish revised draft"
            : "Create External RFI draft"}
        </button>
        {form.revision ? (
          <button
            type="button"
            className="manager-workspace__secondary"
            disabled={pending || loading}
            onClick={() => setForm(emptyDraftForm())}
          >
            Cancel revision
          </button>
        ) : null}
      </div>
    </form>
  );

  const renderReviewSection = () => {
    if (!reviewSummary) return null;
    const draft = reviewDraft;
    return (
      <section
        className="tender-rfi__card"
        aria-label="External RFI review"
        data-testid="tender-rfi-review-section"
      >
        <h3>
          External RFI{" "}
          <code>
            {reviewSummary.rfi_id} · v{reviewSummary.version}
          </code>
        </h3>
        <p className="tender-rfi__status">
          {STATUS_LABELS[reviewSummary.status]}
        </p>
        {!draft ? (
          <p>Loading the exact draft…</p>
        ) : (
          <>
            {renderQuestions(draft)}
            <div className="tender-rfi__evidence">
              <h4>Linked evidence</h4>
              <ul>
                {draft.source_evidence.map((reference) => (
                  <li
                    key={`${reference.kind}-${reference.reference}-${reference.version}`}
                  >
                    <code>
                      {reference.kind} {reference.reference} · v
                      {reference.version}
                    </code>
                  </li>
                ))}
              </ul>
            </div>
            <p>
              <strong>Recipient:</strong> {draft.recipient.organization} ·{" "}
              {draft.recipient.attention}
            </p>
            <p>
              <strong>Response needed by:</strong>{" "}
              {formatDateTime(draft.due_at)}
            </p>
            {reviewSummary.status === "query_basis_stale" ? (
              <p role="alert">
                The exact Tender question basis changed after this draft was
                written. Revise it onto the current questions before it can be
                reviewed or issued.
              </p>
            ) : null}
            {draft.review ? (
              <div className="tender-rfi__findings">
                <h4>Independent review result: {draft.review.outcome}</h4>
                {draft.review.findings.length === 0 ? (
                  <p>No findings were reported.</p>
                ) : (
                  <ul>
                    {draft.review.findings.map((finding) => (
                      <li key={finding.code}>
                        <strong>{finding.severity}</strong> · {finding.summary}
                      </li>
                    ))}
                  </ul>
                )}
                {draft.review.outcome === "failed" ? (
                  <p>
                    Resolve the findings with a revision, then run a new
                    independent review.
                  </p>
                ) : null}
              </div>
            ) : null}
            {draft.revision_allowed ? (
              form.revision?.rfiId === draft.rfi_id ? (
                renderDraftForm()
              ) : (
                <div className="tender-rfi__actions">
                  <button
                    type="button"
                    className="manager-workspace__secondary"
                    disabled={pending || loading}
                    onClick={() => beginRevision(draft)}
                  >
                    Revise draft
                  </button>
                  {!draft.review &&
                  reviewSummary.status === "awaiting_review" ? (
                    <button
                      type="button"
                      className="manager-workspace__primary"
                      disabled={pending || loading || !runtimeReady}
                      onClick={() => runReview(draft)}
                    >
                      Run independent review
                    </button>
                  ) : null}
                </div>
              )
            ) : null}
            {draft.review?.outcome === "passed" && !draft.approval ? (
              <form
                className="tender-rfi__form"
                onSubmit={(event) => {
                  event.preventDefault();
                  approveForIssue(draft);
                }}
              >
                <h4>Approve for issue</h4>
                <label>
                  Approval rationale
                  <textarea
                    value={approvalRationale}
                    disabled={pending || loading}
                    onChange={(event) =>
                      setApprovalRationale(event.target.value)
                    }
                  />
                </label>
                <button
                  type="submit"
                  className="manager-workspace__primary"
                  disabled={
                    pending || loading || approvalRationale.trim().length === 0
                  }
                >
                  Approve for issue
                </button>
              </form>
            ) : null}
          </>
        )}
      </section>
    );
  };

  const renderResponseSection = () => {
    if (!responseSummary) return null;
    const draft = responseDraft;
    return (
      <section
        className="tender-rfi__card"
        aria-label="External RFI export and responses"
        data-testid="tender-rfi-response-section"
      >
        <h3>
          External RFI{" "}
          <code>
            {responseSummary.rfi_id} · v{responseSummary.version}
          </code>
        </h3>
        <p className="tender-rfi__status">
          {STATUS_LABELS[responseSummary.status]}
        </p>
        {!draft ? (
          <p>Loading the exact draft…</p>
        ) : (
          <>
            {renderQuestions(draft)}
            <div className="tender-rfi__export">
              <h4>Export the approved wording</h4>
              <p>
                Quantix writes a verified file with the approved questions. You
                deliver that file to the recipient outside Quantix. Quantix
                never sends, uploads, or submits an RFI.
              </p>
              <button
                type="button"
                className="manager-workspace__primary"
                disabled={pending || loading || !draft.approved_for_issue}
                onClick={() => exportForIssue(draft)}
              >
                Export verified file
              </button>
              {(exportRecord &&
              exportRecord.approval_id === draft.approval?.approval_id
                ? [exportRecord]
                : []
              )
                .concat(draft.exports)
                .filter(
                  (record, index, all) =>
                    all.findIndex(
                      (candidate) => candidate.export_id === record.export_id,
                    ) === index,
                )
                .map((record) => (
                  <article
                    className="tender-rfi__record"
                    key={record.export_id}
                    data-testid="tender-rfi-export-record"
                  >
                    <strong>
                      Exported {formatDateTime(record.created_at)} by you
                    </strong>
                    <span>
                      {record.bytes_verified
                        ? "File verified after writing"
                        : "Verification failed"}
                      {" · "}
                      {Number(record.size_bytes).toLocaleString()} bytes
                    </span>
                    <code>{record.path}</code>
                  </article>
                ))}
            </div>
            <div className="tender-rfi__responses">
              <h4>Received responses</h4>
              {draft.responses.length === 0 ? (
                <p>
                  No response is registered yet. Import the received file
                  through the Tender package intake first; Quantix then offers
                  it here.
                </p>
              ) : (
                <ul>
                  {draft.responses.map((response) => (
                    <li key={response.response_link_id}>
                      <strong>
                        Response file{" "}
                        <code>
                          {response.source_artifact_id} · v
                          {response.source_artifact_version}
                        </code>
                      </strong>
                      <span>
                        Registered {formatDateTime(response.created_at)} by you
                      </span>
                      {draft.interpretations
                        .filter(
                          (candidate) =>
                            candidate.response_link_id ===
                            response.response_link_id,
                        )
                        .map((candidate) => (
                          <p key={candidate.interpretation_id}>
                            Interpreted against Tender question{" "}
                            {candidate.query_id}: {candidate.interpretation}
                          </p>
                        ))}
                    </li>
                  ))}
                </ul>
              )}
              {approval && draft.approved_for_issue ? (
                <>
                  <label>
                    Response document from the Tender package intake
                    <select
                      value={selectedResponseKey}
                      disabled={pending || loading}
                      onChange={(event) =>
                        setSelectedResponseKey(event.target.value)
                      }
                    >
                      <option value="">Choose the received response</option>
                      {(responseCandidates?.items ?? []).map((candidate) => (
                        <option
                          key={responseDocumentKey(
                            candidate.source_artifact_id,
                            candidate.source_artifact_version,
                          )}
                          value={responseDocumentKey(
                            candidate.source_artifact_id,
                            candidate.source_artifact_version,
                          )}
                        >
                          {candidate.package_path} · v
                          {candidate.source_artifact_version}
                        </option>
                      ))}
                    </select>
                  </label>
                  <button
                    type="button"
                    className="manager-workspace__secondary"
                    disabled={pending || loading || !selectedResponseKey}
                    onClick={() => registerResponse(draft)}
                  >
                    Register response
                  </button>
                </>
              ) : null}
            </div>
            {pendingInterpretation ? (
              <form
                className="tender-rfi__form"
                onSubmit={(event) => {
                  event.preventDefault();
                  recordInterpretation();
                }}
              >
                <h4>Interpret the response</h4>
                <p>
                  The registered response stays preserved with its exact source
                  file. Your interpretation records one answer against the
                  question, without replacing the question or earlier answers.
                </p>
                <p>
                  <strong>Exact response evidence:</strong>{" "}
                  <code>
                    source_artifact{" "}
                    {pendingInterpretation.response.source_artifact_id} · v
                    {pendingInterpretation.response.source_artifact_version}
                  </code>
                </p>
                <p>
                  <strong>Question:</strong>{" "}
                  {responseDraft.questions.find(
                    (question) =>
                      question.query_id ===
                      pendingInterpretation.reference.query_id,
                  )?.question ?? pendingInterpretation.reference.query_id}
                </p>
                <label>
                  Interpretation
                  <textarea
                    value={interpretation.text}
                    disabled={pending || loading}
                    onChange={(event) =>
                      setInterpretation((current) => ({
                        ...current,
                        text: event.target.value,
                      }))
                    }
                  />
                </label>
                <label>
                  Treatment
                  <select
                    value={interpretation.treatment}
                    disabled={pending || loading}
                    onChange={(event) =>
                      setInterpretation((current) => ({
                        ...current,
                        treatment: event.target.value as TenderQueryTreatment,
                      }))
                    }
                  >
                    {TREATMENT_OPTIONS.map((option) => (
                      <option key={option.value} value={option.value}>
                        {option.label}
                      </option>
                    ))}
                  </select>
                </label>
                <label>
                  Manager rationale
                  <textarea
                    value={interpretation.rationale}
                    disabled={pending || loading}
                    onChange={(event) =>
                      setInterpretation((current) => ({
                        ...current,
                        rationale: event.target.value,
                      }))
                    }
                  />
                </label>
                <label>
                  Exact treatment details
                  <textarea
                    value={interpretation.treatmentDetails}
                    disabled={pending || loading}
                    onChange={(event) =>
                      setInterpretation((current) => ({
                        ...current,
                        treatmentDetails: event.target.value,
                      }))
                    }
                  />
                </label>
                <label className="tender-rfi__checkbox">
                  <input
                    type="checkbox"
                    checked={interpretation.material}
                    disabled={pending || loading}
                    onChange={(event) =>
                      setInterpretation((current) => ({
                        ...current,
                        material: event.target.checked,
                      }))
                    }
                  />
                  This interpretation is material to the bid
                </label>
                <label className="tender-rfi__checkbox">
                  <input
                    type="checkbox"
                    checked={interpretation.closesQuery}
                    disabled={pending || loading}
                    onChange={(event) =>
                      setInterpretation((current) => ({
                        ...current,
                        closesQuery: event.target.checked,
                      }))
                    }
                  />
                  Close the Tender question (only a permitting treatment)
                </label>
                <button
                  type="submit"
                  className="manager-workspace__primary"
                  disabled={
                    pending ||
                    loading ||
                    !interpretation.text.trim() ||
                    !interpretation.rationale.trim() ||
                    !interpretation.treatmentDetails.trim()
                  }
                >
                  Record interpretation
                </button>
              </form>
            ) : null}
          </>
        )}
      </section>
    );
  };

  const renderGatherSection = () => (
    <section
      className="tender-rfi__card"
      aria-label="Start a new External RFI"
      data-testid="tender-rfi-gather-section"
    >
      <h3>Ask a new controlled question</h3>
      <p>
        Gather the Tender questions the Manager routed for external drafting,
        address them to the recipient, and Quantix prepares the draft for
        independent review and your approval.
      </p>
      {form.revision ? (
        <p>
          A revision is open on the draft above. Publish it there, or cancel it
          to start a new External RFI.
        </p>
      ) : !eligibleReady && !eligible?.next_cursor ? (
        <p>
          No Tender question is currently routed for a controlled External RFI.
        </p>
      ) : (
        renderDraftForm()
      )}
    </section>
  );

  return (
    <section
      className="tender-rfi"
      data-testid="tender-rfi-review"
      aria-labelledby="tender-rfi-title"
    >
      <header className="tender-rfi__header">
        <div>
          <p className="section-label">Tender decision workspace</p>
          <h2 id="tender-rfi-title">External RFI</h2>
          <p>
            Quantix drafts, reviews, approves, and exports the exact wording for
            human issue. It never sends, uploads, or submits an RFI.
          </p>
        </div>
        <button
          type="button"
          className="manager-workspace__secondary"
          onClick={onClose}
        >
          Back to Manager conversation
        </button>
      </header>
      {loading && !page ? (
        <p className="tender-rfi__loading" role="status">
          <LoaderCircle size={16} aria-hidden="true" /> Loading the External RFI
          workspace…
        </p>
      ) : null}
      {interpretationFirst ? (
        <>
          {renderResponseSection()}
          {renderReviewSection()}
          {renderGatherSection()}
        </>
      ) : (
        <>
          {renderReviewSection()}
          {renderResponseSection()}
          {renderGatherSection()}
        </>
      )}
    </section>
  );
}
