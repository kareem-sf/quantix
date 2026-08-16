import {
  FormEvent,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";

import type { DocumentRegister } from "./bindings/DocumentRegister";
import type { ExternalRfiDraft } from "./bindings/ExternalRfiDraft";
import type { ExternalRfiEligibleQueryPage } from "./bindings/ExternalRfiEligibleQueryPage";
import type { ExternalRfiPage } from "./bindings/ExternalRfiPage";
import type { ExternalRfiResponseCandidatePage } from "./bindings/ExternalRfiResponseCandidatePage";
import type { TenderQueryTreatment } from "./bindings/TenderQueryTreatment";
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

interface ExternalRfiPanelProps {
  tenderId: string;
  runtimeReady: boolean;
  register?: DocumentRegister;
  refreshToken: number;
  reportCommandFailure: () => void;
  onTenderStateChange: () => void;
}

const responseTreatments: TenderQueryTreatment[] = [
  "internal_resolution",
  "approved_assumption",
  "qualification",
  "exclusion",
  "allowance",
  "blocked",
];

interface InterpretationDraft {
  interpretation: string;
  rationale: string;
  treatmentDetails: string;
  treatment: TenderQueryTreatment;
  material: boolean;
  closesQuery: boolean;
}

const emptyInterpretationDraft = (): InterpretationDraft => ({
  interpretation: "",
  rationale: "",
  treatmentDetails: "",
  treatment: "qualification",
  material: true,
  closesQuery: false,
});

function lines(value: string) {
  return value
    .split("\n")
    .map((item) => item.trim())
    .filter(Boolean);
}

function documentKey(artifactId: string, version: number) {
  return `${artifactId}:${version}`;
}

export function ExternalRfiPanel({
  tenderId,
  runtimeReady,
  register,
  refreshToken,
  reportCommandFailure,
  onTenderStateChange,
}: ExternalRfiPanelProps) {
  const [page, setPage] = useState<ExternalRfiPage>();
  const [eligible, setEligible] = useState<ExternalRfiEligibleQueryPage>();
  const [cursor, setCursor] = useState<string | null>(null);
  const [eligibleCursor, setEligibleCursor] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [queryId, setQueryId] = useState("");
  const [context, setContext] = useState("");
  const [responseNeed, setResponseNeed] = useState("");
  const [dueAt, setDueAt] = useState("");
  const [organization, setOrganization] = useState("");
  const [attention, setAttention] = useState("");
  const [email, setEmail] = useState("");
  const [attachmentKey, setAttachmentKey] = useState("");
  const [commitments, setCommitments] = useState("");
  const [revision, setRevision] = useState<ExternalRfiDraft>();
  const [approvalRationales, setApprovalRationales] = useState<
    Record<string, string>
  >({});
  const [responseDocumentKeys, setResponseDocumentKeys] = useState<
    Record<string, string>
  >({});
  const [responseCandidatePages, setResponseCandidatePages] = useState<
    Record<string, ExternalRfiResponseCandidatePage>
  >({});
  const [responseCandidateCursorStacks, setResponseCandidateCursorStacks] =
    useState<Record<string, Array<string | null>>>({});
  const [interpretationDrafts, setInterpretationDrafts] = useState<
    Record<string, InterpretationDraft>
  >({});
  const [lastExportPath, setLastExportPath] = useState("");
  const requestGeneration = useRef(0);

  const registeredDocuments = useMemo(
    () =>
      register?.documents.filter(
        (document) => document.registration_state === "registered",
      ) ?? [],
    [register],
  );

  const load = useCallback(
    async (nextCursor: string | null, nextEligibleCursor: string | null) => {
      const generation = ++requestGeneration.current;
      setLoading(true);
      try {
        const [nextPage, nextEligible] = await Promise.all([
          inspectExternalRfis(tenderId, nextCursor, 8),
          inspectExternalRfiEligibleQueries(tenderId, nextEligibleCursor, 8),
        ]);
        const responsePages = await Promise.all(
          nextPage.items
            .filter((draft) => draft.approval)
            .map(
              async (draft) =>
                [
                  draft.rfi_id,
                  await inspectExternalRfiResponseCandidates(
                    tenderId,
                    draft.approval!.approval_id,
                    null,
                    64,
                  ),
                ] as const,
            ),
        );
        if (generation !== requestGeneration.current) return;
        setPage(nextPage);
        setEligible(nextEligible);
        setResponseCandidatePages(Object.fromEntries(responsePages));
        setResponseCandidateCursorStacks(
          Object.fromEntries(responsePages.map(([rfiId]) => [rfiId, [null]])),
        );
        setCursor(nextCursor);
        setEligibleCursor(nextEligibleCursor);
        setQueryId((current) =>
          nextEligible.items.some((item) => item.query_ref.query_id === current)
            ? current
            : nextEligible.items[0]?.query_ref.query_id || "",
        );
        setAttachmentKey((current) =>
          current || !registeredDocuments[0]
            ? current
            : documentKey(
                registeredDocuments[0].artifact_id,
                registeredDocuments[0].version,
              ),
        );
      } catch {
        if (generation === requestGeneration.current) reportCommandFailure();
      } finally {
        if (generation === requestGeneration.current) setLoading(false);
      }
    },
    [registeredDocuments, reportCommandFailure, tenderId],
  );

  const turnResponseCandidatePage = useCallback(
    async (draft: ExternalRfiDraft, direction: "newer" | "older") => {
      const approval = draft.approval;
      if (!approval) return;
      const stack = responseCandidateCursorStacks[draft.rfi_id] ?? [null];
      const nextCursor =
        direction === "older"
          ? responseCandidatePages[draft.rfi_id]?.next_cursor
          : stack[stack.length - 2];
      if (
        nextCursor === undefined ||
        (nextCursor === null && direction === "older")
      ) {
        return;
      }
      const generation = ++requestGeneration.current;
      setLoading(true);
      try {
        const nextPage = await inspectExternalRfiResponseCandidates(
          tenderId,
          approval.approval_id,
          nextCursor,
          64,
        );
        if (generation !== requestGeneration.current) return;
        setResponseCandidatePages((current) => ({
          ...current,
          [draft.rfi_id]: nextPage,
        }));
        setResponseCandidateCursorStacks((current) => ({
          ...current,
          [draft.rfi_id]:
            direction === "older"
              ? [...(current[draft.rfi_id] ?? [null]), nextCursor]
              : (current[draft.rfi_id] ?? [null]).slice(0, -1),
        }));
        setResponseDocumentKeys((current) => ({
          ...current,
          [draft.rfi_id]: "",
        }));
      } catch {
        reportCommandFailure();
      } finally {
        if (generation === requestGeneration.current) setLoading(false);
      }
    },
    [
      reportCommandFailure,
      responseCandidateCursorStacks,
      responseCandidatePages,
      tenderId,
    ],
  );

  useEffect(() => {
    void load(null, null);
    return () => {
      requestGeneration.current += 1;
    };
  }, [load, refreshToken]);

  const mutate = async (command: () => Promise<unknown>) => {
    setLoading(true);
    try {
      await command();
      onTenderStateChange();
      await load(null, null);
      return true;
    } catch {
      reportCommandFailure();
      return false;
    } finally {
      setLoading(false);
    }
  };

  const selectedQuery = eligible?.items.find(
    (item) => item.query_ref.query_id === queryId,
  );

  const draftCommand = () => {
    const queryRefs = revision
      ? revision.current_query_refs
      : selectedQuery
        ? [selectedQuery.query_ref]
        : [];
    const parsedDueAt = new Date(dueAt);
    if (
      queryRefs.length === 0 ||
      !context.trim() ||
      !responseNeed.trim() ||
      !dueAt ||
      Number.isNaN(parsedDueAt.getTime()) ||
      !organization.trim() ||
      !attention.trim()
    ) {
      return undefined;
    }
    const attachment = registeredDocuments.find(
      (document) =>
        documentKey(document.artifact_id, document.version) === attachmentKey,
    );
    return {
      query_refs: queryRefs,
      contractual_context: context.trim(),
      response_need: responseNeed.trim(),
      attachments: attachment
        ? [
            {
              kind: "source_artifact",
              reference: attachment.artifact_id,
              version: attachment.version,
            },
          ]
        : [],
      due_at: parsedDueAt.toISOString(),
      recipient: {
        organization: organization.trim(),
        attention: attention.trim(),
        email: email.trim() || null,
      },
      affected_commitments: lines(commitments),
    };
  };

  const handleDraft = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const command = draftCommand();
    if (!command) return;
    const operation = revision
      ? () =>
          reviseExternalRfiDraft({
            tender_id: tenderId,
            rfi_id: revision.rfi_id,
            base_version: revision.version,
            additional_evidence: revision.source_evidence,
            ...command,
          })
      : () =>
          createExternalRfiDraft({
            tender_id: tenderId,
            additional_evidence: [],
            ...command,
          });
    void mutate(operation).then((succeeded) => {
      if (succeeded) {
        setRevision(undefined);
        setContext("");
        setResponseNeed("");
        setCommitments("");
      }
    });
  };

  const beginRevision = (draft: ExternalRfiDraft) => {
    setRevision(draft);
    setQueryId(draft.current_query_refs[0]?.query_id ?? "");
    setContext(draft.contractual_context);
    setResponseNeed(draft.response_need);
    setDueAt(draft.due_at.slice(0, 16));
    setOrganization(draft.recipient.organization);
    setAttention(draft.recipient.attention);
    setEmail(draft.recipient.email ?? "");
    setCommitments(draft.affected_commitments.join("\n"));
    const attachment = draft.attachments[0];
    setAttachmentKey(
      attachment ? documentKey(attachment.reference, attachment.version) : "",
    );
  };

  const registerResponse = (draft: ExternalRfiDraft) => {
    const approval = draft.approval;
    const document = responseCandidatePages[draft.rfi_id]?.items.find(
      (candidate) =>
        documentKey(
          candidate.source_artifact_id,
          candidate.source_artifact_version,
        ) === responseDocumentKeys[draft.rfi_id],
    );
    if (!approval || !document) return;
    void mutate(() =>
      registerExternalRfiResponse({
        tender_id: tenderId,
        rfi_id: draft.rfi_id,
        rfi_version: draft.version,
        approval_id: approval.approval_id,
        source_artifact_id: document.source_artifact_id,
        source_artifact_version: document.source_artifact_version,
      }),
    );
  };

  const interpretResponse = (
    responseLinkId: string,
    exactQueryId: string,
    issuedQueryVersion: number,
    currentQueryVersion: number,
    currentQueryManifestSha256: string,
    exactDraft: InterpretationDraft,
  ) => {
    if (
      !exactDraft.interpretation.trim() ||
      !exactDraft.rationale.trim() ||
      !exactDraft.treatmentDetails.trim()
    ) {
      return;
    }
    void mutate(() =>
      interpretExternalRfiResponse({
        tender_id: tenderId,
        response_link_id: responseLinkId,
        query_id: exactQueryId,
        issued_query_version: issuedQueryVersion,
        base_query_version: currentQueryVersion,
        base_query_manifest_sha256: currentQueryManifestSha256,
        material: exactDraft.material,
        interpretation: exactDraft.interpretation.trim(),
        treatment: exactDraft.treatment,
        rationale: exactDraft.rationale.trim(),
        treatment_details: exactDraft.treatmentDetails.trim(),
        closes_query: exactDraft.closesQuery,
      }),
    ).then((succeeded) => {
      if (succeeded) {
        const key = `${responseLinkId}:${exactQueryId}`;
        setInterpretationDrafts((current) => {
          const next = { ...current };
          delete next[key];
          return next;
        });
      }
    });
  };

  const updateInterpretationDraft = (
    key: string,
    update: Partial<InterpretationDraft>,
  ) => {
    setInterpretationDrafts((current) => ({
      ...current,
      [key]: {
        ...(current[key] ?? emptyInterpretationDraft()),
        ...update,
      },
    }));
  };

  return (
    <section className="office-card">
      <div className="section-heading">
        <div>
          <p className="eyebrow">Controlled external clarification</p>
          <h2 id="external-rfi-title">External RFI Register</h2>
        </div>
        <button
          type="button"
          disabled={loading}
          onClick={() => void load(null, null)}
        >
          Refresh
        </button>
      </div>
      <p>
        Quantix drafts, reviews, approves, and exports exact bytes for human
        issue. It never sends, uploads, or submits an RFI.
      </p>
      {page ? (
        <p>
          {page.total_current_count} current · {page.approved_for_issue_count}{" "}
          approved on current Evidence
        </p>
      ) : null}

      {revision || eligible?.items.length ? (
        <form className="stacked-form" onSubmit={handleDraft}>
          <h3>
            {revision
              ? `Revise ${revision.rfi_id} v${revision.version}`
              : "Draft from exact Query"}
          </h3>
          <div className="form-grid">
            <label>
              Approved External RFI Query
              <select
                value={queryId}
                onChange={(event) => setQueryId(event.target.value)}
                disabled={Boolean(revision)}
              >
                {revision
                  ? revision.current_query_refs.map((reference) => (
                      <option
                        key={`${reference.query_id}:${reference.version}`}
                        value={reference.query_id}
                      >
                        {reference.query_id} · current v{reference.version}
                      </option>
                    ))
                  : eligible?.items.map((item) => (
                      <option
                        key={`${item.query_ref.query_id}:${item.query_ref.version}`}
                        value={item.query_ref.query_id}
                      >
                        {item.question} · v{item.query_ref.version}
                      </option>
                    ))}
              </select>
            </label>
            <label>
              Response due
              <input
                type="datetime-local"
                value={dueAt}
                onChange={(event) => setDueAt(event.target.value)}
              />
            </label>
            <label>
              Recipient organization
              <input
                value={organization}
                onChange={(event) => setOrganization(event.target.value)}
              />
            </label>
            <label>
              Attention
              <input
                value={attention}
                onChange={(event) => setAttention(event.target.value)}
              />
            </label>
            <label>
              Proposed email (optional)
              <input
                type="email"
                value={email}
                onChange={(event) => setEmail(event.target.value)}
              />
            </label>
            <label>
              Attachment from Document Register
              <select
                value={attachmentKey}
                onChange={(event) => setAttachmentKey(event.target.value)}
              >
                <option value="">No attachment</option>
                {registeredDocuments.map((document) => (
                  <option
                    key={documentKey(document.artifact_id, document.version)}
                    value={documentKey(document.artifact_id, document.version)}
                  >
                    {document.package_path} · v{document.version}
                  </option>
                ))}
              </select>
            </label>
          </div>
          <label>
            Exact contractual context
            <textarea
              value={context}
              onChange={(event) => setContext(event.target.value)}
            />
          </label>
          <label>
            Response needed
            <textarea
              value={responseNeed}
              onChange={(event) => setResponseNeed(event.target.value)}
            />
          </label>
          <label>
            Affected commitments (one per line)
            <textarea
              value={commitments}
              onChange={(event) => setCommitments(event.target.value)}
            />
          </label>
          <div className="intake-actions">
            <button type="submit" disabled={loading || !runtimeReady}>
              {revision ? "Publish revised draft" : "Create RFI draft"}
            </button>
            {revision ? (
              <button
                type="button"
                className="button-secondary"
                onClick={() => setRevision(undefined)}
              >
                Cancel revision
              </button>
            ) : null}
          </div>
        </form>
      ) : (
        <p className="catalogue-message">
          No current Query has a Manager-approved External RFI Drafting
          treatment.
        </p>
      )}

      <div className="record-list">
        {page?.items.map((draft) => {
          const passed = draft.review?.outcome === "passed";
          const responseCandidates =
            responseCandidatePages[draft.rfi_id]?.items ?? [];
          const responseDocumentKey = responseDocumentKeys[draft.rfi_id] ?? "";
          return (
            <article
              className="record-card"
              key={`${draft.rfi_id}:${draft.version}`}
            >
              <div className="section-heading">
                <div>
                  <p className="eyebrow">
                    {draft.current ? "Current" : "Historical"} · v
                    {draft.version}
                  </p>
                  <h3>{draft.questions[0]?.question ?? draft.rfi_id}</h3>
                </div>
                <span>
                  {draft.approved_for_issue
                    ? "Approved for human issue"
                    : (draft.review?.outcome ?? "Draft")}
                </span>
              </div>
              <p>{draft.contractual_context}</p>
              <div>
                <strong>Exact questions</strong>
                <ul>
                  {draft.questions.map((question) => (
                    <li key={`${question.query_id}:${question.query_version}`}>
                      {question.question} — {question.ambiguity_or_gap}
                    </li>
                  ))}
                </ul>
              </div>
              <p>
                <strong>Response needed:</strong> {draft.response_need}
              </p>
              <p>
                <strong>Recipient:</strong> {draft.recipient.organization} ·{" "}
                {draft.recipient.attention}
              </p>
              <p>
                <strong>Due:</strong> {draft.due_at}
              </p>
              <div>
                <strong>Exact Evidence</strong>
                <ul>
                  {draft.source_evidence.map((reference) => (
                    <li
                      key={`evidence-${documentKey(reference.reference, reference.version)}`}
                    >
                      {reference.kind} · {reference.reference} · v
                      {reference.version}
                    </li>
                  ))}
                </ul>
              </div>
              <div>
                <strong>Attachments</strong>
                <ul>
                  {draft.attachments.map((reference) => (
                    <li
                      key={`attachment-${documentKey(reference.reference, reference.version)}`}
                    >
                      {reference.kind} · {reference.reference} · v
                      {reference.version}
                    </li>
                  ))}
                </ul>
              </div>
              <p>
                <strong>Affected work:</strong>{" "}
                {draft.affected_task_keys.join(", ") || "None"}
              </p>
              <p>
                <strong>Affected commitments:</strong>{" "}
                {draft.affected_commitments.join(", ") || "None"}
              </p>
              <p>
                <strong>Exact manifest:</strong> {draft.manifest_sha256}
              </p>
              {!draft.evidence_current ? (
                <p role="alert">
                  The exact Query basis has changed. This draft is stale and
                  cannot be reviewed or issued.
                </p>
              ) : null}
              {draft.review?.findings.map((finding) => (
                <p key={finding.code}>
                  <strong>
                    {finding.severity} · {finding.code}:
                  </strong>{" "}
                  {finding.summary}
                </p>
              ))}
              <div className="intake-actions">
                <button
                  type="button"
                  className="button-secondary"
                  disabled={
                    loading || !draft.current || !draft.revision_allowed
                  }
                  onClick={() => beginRevision(draft)}
                >
                  Revise
                </button>
                <button
                  type="button"
                  disabled={
                    loading ||
                    !runtimeReady ||
                    !draft.current ||
                    !draft.evidence_current ||
                    Boolean(draft.review)
                  }
                  onClick={() =>
                    void mutate(() =>
                      runExternalRfiReview({
                        tender_id: tenderId,
                        rfi_id: draft.rfi_id,
                        version: draft.version,
                      }),
                    )
                  }
                >
                  Independent review
                </button>
              </div>
              {passed && !draft.approval ? (
                <div className="stacked-form">
                  <label>
                    Manager approval rationale
                    <textarea
                      value={approvalRationales[draft.rfi_id] ?? ""}
                      onChange={(event) =>
                        setApprovalRationales((current) => ({
                          ...current,
                          [draft.rfi_id]: event.target.value,
                        }))
                      }
                    />
                  </label>
                  <button
                    type="button"
                    disabled={
                      loading ||
                      !(approvalRationales[draft.rfi_id] ?? "").trim()
                    }
                    onClick={() =>
                      void mutate(() =>
                        approveExternalRfiForIssue({
                          tender_id: tenderId,
                          rfi_id: draft.rfi_id,
                          version: draft.version,
                          manifest_sha256: draft.manifest_sha256,
                          rationale: (
                            approvalRationales[draft.rfi_id] ?? ""
                          ).trim(),
                        }),
                      ).then(
                        (succeeded) =>
                          succeeded &&
                          setApprovalRationales((current) => ({
                            ...current,
                            [draft.rfi_id]: "",
                          })),
                      )
                    }
                  >
                    Approve exact version for issue
                  </button>
                </div>
              ) : null}
              {draft.approval ? (
                <div className="stacked-form">
                  <button
                    type="button"
                    disabled={loading || !draft.approved_for_issue}
                    onClick={() =>
                      void exportApprovedExternalRfi({
                        tender_id: tenderId,
                        rfi_id: draft.rfi_id,
                        version: draft.version,
                        approval_sha256: draft.approval!.approval_sha256,
                      })
                        .then((record) => {
                          setLastExportPath(record.path);
                          onTenderStateChange();
                        })
                        .catch(reportCommandFailure)
                    }
                  >
                    Export verified bytes for human issue
                  </button>
                  <label>
                    Intake-registered response Source Artifact
                    <select
                      value={responseDocumentKey}
                      onChange={(event) =>
                        setResponseDocumentKeys((current) => ({
                          ...current,
                          [draft.rfi_id]: event.target.value,
                        }))
                      }
                    >
                      <option value="">Select response document</option>
                      {responseCandidates.map((document) => (
                        <option
                          key={`response-${documentKey(document.source_artifact_id, document.source_artifact_version)}`}
                          value={documentKey(
                            document.source_artifact_id,
                            document.source_artifact_version,
                          )}
                        >
                          {document.package_path} · v
                          {document.source_artifact_version}
                        </option>
                      ))}
                    </select>
                  </label>
                  {responseCandidates.length === 0 ? (
                    <p>
                      Import the received response through Intake after this
                      approval; original Tender inputs are not eligible.
                    </p>
                  ) : null}
                  <div className="intake-actions">
                    <button
                      type="button"
                      className="button-secondary"
                      disabled={
                        loading ||
                        (responseCandidateCursorStacks[draft.rfi_id]?.length ??
                          1) <= 1
                      }
                      onClick={() =>
                        void turnResponseCandidatePage(draft, "newer")
                      }
                    >
                      Previous response documents
                    </button>
                    <button
                      type="button"
                      className="button-secondary"
                      disabled={
                        loading ||
                        !responseCandidatePages[draft.rfi_id]?.next_cursor
                      }
                      onClick={() =>
                        void turnResponseCandidatePage(draft, "older")
                      }
                    >
                      Next response documents
                    </button>
                  </div>
                  <button
                    type="button"
                    disabled={loading || !responseDocumentKey}
                    onClick={() => registerResponse(draft)}
                  >
                    Link response from Intake
                  </button>
                </div>
              ) : null}
              {draft.responses.map((response) =>
                draft.query_refs.map((reference) => {
                  const currentReference = draft.current_query_refs.find(
                    (candidate) => candidate.query_id === reference.query_id,
                  );
                  const interpreted = draft.interpretations.some(
                    (item) =>
                      item.response_link_id === response.response_link_id &&
                      item.query_id === reference.query_id,
                  );
                  const draftKey = `${response.response_link_id}:${reference.query_id}`;
                  const exactDraft =
                    interpretationDrafts[draftKey] ??
                    emptyInterpretationDraft();
                  return (
                    <div
                      className="stacked-form"
                      key={`${response.response_link_id}:${reference.query_id}`}
                    >
                      <h4>
                        Manager response interpretation · Query v
                        {reference.version}
                      </h4>
                      {interpreted ? (
                        <p>
                          Interpretation recorded as an immutable Query
                          successor.
                        </p>
                      ) : (
                        <>
                          <label>
                            Interpretation
                            <textarea
                              value={exactDraft.interpretation}
                              onChange={(event) =>
                                updateInterpretationDraft(draftKey, {
                                  interpretation: event.target.value,
                                })
                              }
                            />
                          </label>
                          <label>
                            Treatment
                            <select
                              value={exactDraft.treatment}
                              onChange={(event) =>
                                updateInterpretationDraft(draftKey, {
                                  treatment: event.target
                                    .value as TenderQueryTreatment,
                                })
                              }
                            >
                              {responseTreatments.map((item) => (
                                <option key={item} value={item}>
                                  {item.replace(/_/g, " ")}
                                </option>
                              ))}
                            </select>
                          </label>
                          <label>
                            Manager rationale
                            <textarea
                              value={exactDraft.rationale}
                              onChange={(event) =>
                                updateInterpretationDraft(draftKey, {
                                  rationale: event.target.value,
                                })
                              }
                            />
                          </label>
                          <label>
                            Exact treatment details
                            <textarea
                              value={exactDraft.treatmentDetails}
                              onChange={(event) =>
                                updateInterpretationDraft(draftKey, {
                                  treatmentDetails: event.target.value,
                                })
                              }
                            />
                          </label>
                          <label>
                            <input
                              type="checkbox"
                              checked={exactDraft.material}
                              onChange={(event) =>
                                updateInterpretationDraft(draftKey, {
                                  material: event.target.checked,
                                })
                              }
                            />{" "}
                            Material interpretation
                          </label>
                          <label>
                            <input
                              type="checkbox"
                              checked={exactDraft.closesQuery}
                              onChange={(event) =>
                                updateInterpretationDraft(draftKey, {
                                  closesQuery: event.target.checked,
                                })
                              }
                            />{" "}
                            Close Query (only a permitting treatment)
                          </label>
                          <button
                            type="button"
                            disabled={loading || !currentReference}
                            onClick={() =>
                              currentReference &&
                              interpretResponse(
                                response.response_link_id,
                                reference.query_id,
                                reference.version,
                                currentReference.version,
                                currentReference.manifest_sha256,
                                exactDraft,
                              )
                            }
                          >
                            Record exact interpretation
                          </button>
                        </>
                      )}
                    </div>
                  );
                }),
              )}
            </article>
          );
        })}
      </div>
      {lastExportPath ? (
        <p role="status">
          Verified export created at {lastExportPath}. Quantix did not send it.
        </p>
      ) : null}
      <div className="intake-actions">
        <button
          type="button"
          className="button-secondary"
          disabled={loading || !cursor}
          onClick={() => void load(null, eligibleCursor)}
        >
          Newest RFIs
        </button>
        <button
          type="button"
          className="button-secondary"
          disabled={loading || !page?.next_cursor}
          onClick={() => void load(page?.next_cursor ?? null, eligibleCursor)}
        >
          Older RFIs
        </button>
        <button
          type="button"
          className="button-secondary"
          disabled={loading || !eligibleCursor}
          onClick={() => void load(cursor, null)}
        >
          Newest eligible Queries
        </button>
        <button
          type="button"
          className="button-secondary"
          disabled={loading || !eligible?.next_cursor}
          onClick={() => void load(cursor, eligible?.next_cursor ?? null)}
        >
          More eligible Queries
        </button>
      </div>
    </section>
  );
}
