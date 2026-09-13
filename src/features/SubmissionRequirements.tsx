import { useCallback, useState, type FormEvent } from "react";
import { useQuery } from "@tanstack/react-query";
import { ClipboardCheck, Plus } from "lucide-react";
import {
  tenderPath,
  useApi,
  useRefresh,
  useResource,
  type Schema,
} from "../api";
import { ErrorNotice, Loading, Modal } from "../components/common";
import { EvidencePicker } from "./EvidencePicker";
import { Citations, SourceDrawer, type SourceSelection } from "./Sources";
import { FieldError } from "../components/FieldError";
import { createDraftScope, useFormDraft } from "./useFormDraft";
import type { SubmissionView } from "./Outputs";

type Requirement = Schema<"RequirementRecord">;
type Deliverable = Schema<"RequirementProposal">["deliverable_kind"];
type Decision = Schema<"RequirementDecision">["decision"];
const deliverables: Record<Deliverable, string> = {
  boq_xlsx: "Consolidated BOQ workbook",
  client_boq: "Priced client BOQ",
  analysis_docx: "Tender analysis",
  technical_docx: "Technical document",
  registers_xlsx: "Tender registers",
  comparison_xlsx: "Quotation comparison",
  programme_xlsx: "Construction programme",
};
const decisionLabels: Record<Decision, string> = {
  approve: "Approve this requirement",
  withdraw: "Withdraw this requirement",
  satisfied: "Reviewed — satisfied by linked documents",
  exception: "Reviewed — record an exception",
  reopen: "Reopen completion review",
};
const sourceReasons: Record<string, string> = {
  source_revision_changed:
    "A supporting file has a newer or different revision.",
  source_evidence_changed:
    "The supporting source text or its location has changed.",
  source_bytes_changed:
    "The saved source file no longer matches its recorded hash.",
  source_unavailable: "A preserved supporting source is unavailable.",
};

function reviewLabel(requirement: Requirement) {
  if (requirement.status === "withdrawn") return "Withdrawn";
  if (requirement.status === "proposed") return "Awaiting requirement approval";
  if (!requirement.is_current) return "Source needs attention";
  if (requirement.review_status === "pending")
    return "Completion review needed";
  if (!requirement.review_is_current) return "Completion review needs updating";
  return requirement.review_status === "satisfied"
    ? "Reviewed — satisfied"
    : "Reviewed — exception";
}

type RegisterProps = {
  tenderId: string;
  selectedId?: string;
  onSelect?: (id: string | null) => void;
  onNavigate?: (view: SubmissionView, recordId?: string) => void;
  onSource?: (source: SourceSelection) => void;
};
export function SubmissionRequirements(props: RegisterProps) {
  return <RequirementRegister key={props.tenderId} {...props} />;
}
function RequirementRegister({
  tenderId,
  selectedId,
  onSelect,
  onNavigate,
  onSource,
}: RegisterProps) {
  const api = useApi(),
    base = tenderPath(tenderId);
  const [creating, setCreating] = useState(false),
    [localSelected, setLocalSelected] = useState<string | null>(null);
  const selected = selectedId ?? localSelected;
  const setSelected = (id: string | null) => {
    setLocalSelected(id);
    onSelect?.(id);
  };
  const [includeWithdrawn, setIncludeWithdrawn] = useState(false),
    [offset, setOffset] = useState(0);
  const [source, setSource] = useState<SourceSelection | null>(null);
  const requirements = useResource<Requirement[]>(
    `${base}/requirements?include_withdrawn=${includeWithdrawn}&offset=${offset}&limit=30`,
  );
  const artifacts = useQuery({
    queryKey: [base, "requirement-source-artifacts"],
    queryFn: () =>
      api.get<Schema<"Artifact">[]>(`${base}/artifacts?include_history=true`),
    enabled: !!source,
  });
  const closeCreate = useCallback(() => setCreating(false), []);
  const closeDetail = () => setSelected(null);
  const closeSource = useCallback(() => setSource(null), []);
  const openSource = onSource ?? setSource;
  return (
    <section
      className="submission-requirements"
      aria-labelledby="submission-requirements-heading"
    >
      <div className="section-heading">
        <div>
          <h2 id="submission-requirements-heading">
            <ClipboardCheck size={21} /> Submission requirements
          </h2>
          <p className="muted">
            Record what the tender asks for, approve the requirement and review
            the documents that address it.
          </p>
        </div>
        <button className="button" onClick={() => setCreating(true)}>
          <Plus size={17} /> Propose a requirement
        </button>
      </div>
      <p className="requirements-intro">
        Source references support each requirement. A saved document alone does
        not establish compliance; the engineer records satisfaction or an
        exception after review.
      </p>
      <label className="requirements-checkbox">
        <input
          type="checkbox"
          checked={includeWithdrawn}
          onChange={(event) => {
            setIncludeWithdrawn(event.target.checked);
            setOffset(0);
          }}
        />
        Show withdrawn requirements
      </label>
      <ErrorNotice error={requirements.error} />
      {requirements.isPending ? (
        <Loading>Loading submission requirements…</Loading>
      ) : null}
      {requirements.data?.length === 0 ? (
        <p className="requirements-empty">
          No requirements are recorded in this view. Propose one from the tender
          instructions or ask the Tender Manager to identify them.
        </p>
      ) : null}
      <div className="requirements-list">
        {requirements.data?.map((requirement) => (
          <article key={requirement.id} className="requirement-card">
            <div className="requirement-card-heading">
              <button
                className="requirement-title"
                onClick={() => setSelected(requirement.id)}
              >
                {requirement.title}
              </button>
              <span
                className={`requirement-state ${requirement.review_is_current && requirement.review_status === "satisfied" ? "requirement-satisfied" : ""}`}
              >
                {reviewLabel(requirement)}
              </span>
            </div>
            <p>{requirement.detail}</p>
            <p className="muted">
              Applicability: {applicabilityLabel(requirement.applicability)}
            </p>
            {requirement.condition ? (
              <p>Conditions: {requirement.condition}</p>
            ) : null}
            {requirement.exceptions?.length ? (
              <p>
                {requirement.exceptions.length} stated exception(s) — inspect
                before deciding.
              </p>
            ) : null}
            <div className="requirement-meta">
              <span>{deliverables[requirement.deliverable_kind]}</span>
              <span>
                {requirement.linked_outputs.length} linked document(s)
              </span>
              {requirement.due_date ? (
                <span
                  className={requirement.overdue ? "requirement-overdue" : ""}
                >
                  Due {requirement.due_date}
                  {requirement.overdue ? " · date has passed" : ""}
                </span>
              ) : null}
            </div>
            <Citations
              ids={requirement.source_ids}
              tenderId={tenderId}
              onOpen={openSource}
            />
            <button
              className="button"
              onClick={() => setSelected(requirement.id)}
            >
              Review requirement
            </button>
          </article>
        ))}
      </div>
      <div className="inline-actions">
        <button
          className="button"
          disabled={offset === 0}
          onClick={() => setOffset(Math.max(0, offset - 30))}
        >
          Previous requirements
        </button>
        <button
          className="button"
          disabled={(requirements.data?.length ?? 0) < 30}
          onClick={() => setOffset(offset + 30)}
        >
          More requirements
        </button>
      </div>
      {creating ? (
        <RequirementCreate
          tenderId={tenderId}
          onClose={closeCreate}
          onCreated={(identifier) => {
            setCreating(false);
            setSelected(identifier);
            setOffset(0);
          }}
        />
      ) : null}
      {selected ? (
        <RequirementDetail
          tenderId={tenderId}
          requirementId={selected}
          onClose={closeDetail}
          onSource={openSource}
          onNavigate={onNavigate}
        />
      ) : null}
      {source ? (
        <SourceDrawer
          tenderId={tenderId}
          selection={source}
          artifacts={artifacts.data ?? []}
          onClose={closeSource}
        />
      ) : null}
    </section>
  );
}

function RequirementCreate({
  tenderId,
  onClose,
  onCreated,
}: {
  tenderId: string;
  onClose: () => void;
  onCreated: (id: string) => void;
}) {
  const api = useApi(),
    refresh = useRefresh();
  const draft = useFormDraft(
    createDraftScope("submission", tenderId, "requirement-new", 1),
    {
      title: "",
      detail: "",
      kind: "technical_docx" as Deliverable,
      due: "",
      sources: [] as string[],
      sourceQuote: "",
      applicability: "unknown" as "unknown" | "conditional" | "unconditional",
      condition: "",
      exceptions: "",
    },
    [
      "title",
      "detail",
      "kind",
      "due",
      "sources",
      "sourceQuote",
      "applicability",
      "condition",
      "exceptions",
    ],
  );
  const { title, detail, kind, due, sources } = draft.value;
  const setTitle = (value: string) => draft.setField("title", value),
    setDetail = (value: string) => draft.setField("detail", value),
    setKind = (value: Deliverable) => draft.setField("kind", value),
    setDue = (value: string) => draft.setField("due", value),
    setSources = (value: string[]) => draft.setField("sources", value);
  const [busy, setBusy] = useState(false),
    [error, setError] = useState<unknown>(null);
  async function submit(event: FormEvent) {
    event.preventDefault();
    if (busy || !title.trim() || !detail.trim() || !sources.length) return;
    setBusy(true);
    setError(null);
    try {
      const acceptedRevision = draft.revision;
      const created = await api.post<Requirement>(
        `${tenderPath(tenderId)}/requirements`,
        {
          title,
          detail,
          deliverable_kind: kind,
          due_date: due || null,
          source_ids: sources,
          source_quote: draft.value.sourceQuote,
          applicability: draft.value.applicability,
          condition: draft.value.condition,
          exceptions: draft.value.exceptions
            .split("\n")
            .map((value) => value.trim())
            .filter(Boolean),
        } satisfies Schema<"RequirementProposal">,
      );
      draft.markAccepted(acceptedRevision);
      void refresh();
      onCreated(created.id);
    } catch (caught) {
      setError(caught);
    } finally {
      setBusy(false);
    }
  }
  return (
    <Modal title="Propose a submission requirement" onClose={onClose}>
      <form
        className="requirement-form"
        onSubmit={(event) => void submit(event)}
      >
        <p className="muted">
          Use the supplied tender instructions. This proposal needs a separate
          engineer approval.
        </p>
        <ErrorNotice error={error} />
        <fieldset disabled={busy}>
          <label>
            Requirement title
            <input
              required
              maxLength={300}
              value={title}
              onChange={(event) => setTitle(event.target.value)}
            />
            <FieldError error={error} name="title" />
          </label>
          <label>
            What the tender requires
            <textarea
              required
              maxLength={6000}
              value={detail}
              onChange={(event) => setDetail(event.target.value)}
            />
            <FieldError error={error} name="detail" />
          </label>
          <label>
            Required document type
            <select
              aria-label="Required document type"
              value={kind}
              onChange={(event) => setKind(event.target.value as Deliverable)}
            >
              {Object.entries(deliverables).map(([value, label]) => (
                <option key={value} value={value}>
                  {label}
                </option>
              ))}
            </select>
          </label>
          <label>
            Exact source clause
            <textarea
              value={draft.value.sourceQuote}
              onChange={(event) =>
                draft.setField("sourceQuote", event.target.value)
              }
            />
            <FieldError error={error} name="source_quote" />
          </label>
          <label>
            When this requirement applies
            <select
              value={draft.value.applicability}
              onChange={(event) =>
                draft.setField(
                  "applicability",
                  event.target.value as typeof draft.value.applicability,
                )
              }
            >
              <option value="unknown">Not established — needs review</option>
              <option value="conditional">
                Only when stated conditions apply
              </option>
              <option value="unconditional">Always applies</option>
            </select>
            <FieldError error={error} name="applicability" />
          </label>
          <label>
            Stated conditions
            <textarea
              value={draft.value.condition}
              onChange={(event) =>
                draft.setField("condition", event.target.value)
              }
            />
            <FieldError error={error} name="condition" />
          </label>
          <label>
            Stated exceptions (one per line)
            <textarea
              value={draft.value.exceptions}
              onChange={(event) =>
                draft.setField("exceptions", event.target.value)
              }
            />
            <FieldError error={error} name="exceptions" />
          </label>
          <p className="muted">
            Keep the exact clause, including conditions and exceptions. Saving
            this proposal does not record an applicability review.
          </p>
          <label>
            Stated due date (optional)
            <input
              type="date"
              value={due}
              onChange={(event) => setDue(event.target.value)}
            />
          </label>
          <p className="muted">
            Enter a date only when the supporting instructions state it. A past
            date is shown for review.
          </p>
          <EvidencePicker
            tenderId={tenderId}
            selected={sources}
            onChange={setSources}
          />
          <div className="inline-actions">
            <button
              type="submit"
              className="button primary"
              disabled={
                !title.trim() ||
                !detail.trim() ||
                sources.length === 0 ||
                sources.length > 50
              }
            >
              {busy ? "Saving proposal…" : "Save requirement proposal"}
            </button>
            <button type="button" className="button" onClick={onClose}>
              Cancel
            </button>
          </div>
        </fieldset>
      </form>
    </Modal>
  );
}

function RequirementDetail({
  tenderId,
  requirementId,
  onClose,
  onSource,
  onNavigate,
}: {
  tenderId: string;
  requirementId: string;
  onClose: () => void;
  onSource: (source: SourceSelection) => void;
  onNavigate?: RegisterProps["onNavigate"];
}) {
  const requirement = useResource<Requirement>(
    `${tenderPath(tenderId)}/requirements/${encodeURIComponent(requirementId)}`,
  );
  const current = requirement.data;
  return (
    <Modal title={current?.title ?? "Submission requirement"} onClose={onClose}>
      <div className="requirement-detail">
        <ErrorNotice error={requirement.error} />
        {requirement.isPending ? <Loading>Opening requirement…</Loading> : null}
        {current ? (
          <>
            <span className="requirement-state">{reviewLabel(current)}</span>
            <p>{current.detail}</p>
            <RequirementApplicability requirement={current} />
            <dl className="requirement-facts">
              <dt>Required document</dt>
              <dd>{deliverables[current.deliverable_kind]}</dd>
              <dt>Origin</dt>
              <dd>
                {current.origin === "manager"
                  ? "Tender Manager proposal"
                  : "Engineer proposal"}
              </dd>
              <dt>Due date</dt>
              <dd>{current.due_date ?? "No date recorded"}</dd>
              <dt>Approval state</dt>
              <dd>{current.status}</dd>
            </dl>
            {current.warnings.map((warning) => (
              <p className="requirement-warning" key={warning}>
                {warning}
              </p>
            ))}
            {current.recheck_reasons.map((reason) => (
              <p className="requirement-warning" key={reason}>
                {sourceReasons[reason] ?? reason}
              </p>
            ))}
            <Citations
              ids={current.source_ids}
              tenderId={tenderId}
              onOpen={onSource}
            />
            <details>
              <summary>Recorded source versions</summary>
              <div className="requirement-source-list">
                {current.sources.map((source) => (
                  <div key={source.source_id}>
                    <strong>
                      {source.artifact_name} · {source.locator}
                    </strong>
                    <p>
                      {source.relative_path} · version {source.version}
                    </p>
                    <p>
                      {source.is_current
                        ? "Current preserved source"
                        : "Source needs recheck"}
                    </p>
                    <small>SHA-256 {source.content_hash}</small>
                  </div>
                ))}
              </div>
            </details>
            {current.reviewed_at ? (
              <div className="requirement-review-note">
                <h4>Recorded completion review</h4>
                <p>{current.review_rationale}</p>
                <small>
                  {current.reviewed_at}
                  {current.review_is_current
                    ? " · current"
                    : " · needs updating"}
                </small>
              </div>
            ) : null}
            <RequirementLinks
              key={`links:${current.id}:${current.audit.length}`}
              tenderId={tenderId}
              requirement={current}
              onNavigate={onNavigate}
            />
            {current.status !== "withdrawn" ? (
              <RequirementDecisionForm
                key={`decision:${current.id}:${current.audit.length}`}
                tenderId={tenderId}
                requirement={current}
              />
            ) : (
              <p className="muted">
                This requirement was withdrawn. Its source and review history
                remain available.
              </p>
            )}
            <details>
              <summary>
                Engineer decision history ({current.audit.length})
              </summary>
              <ol className="requirement-audit">
                {current.audit.map((event) => (
                  <li key={event.id}>
                    <strong>
                      {event.action === "link_output"
                        ? "Document linked"
                        : event.action === "unlink_output"
                          ? "Document link removed"
                          : decisionLabels[event.action]}
                    </strong>
                    <p>{event.rationale}</p>
                    <small>{event.created_at}</small>
                  </li>
                ))}
              </ol>
            </details>
          </>
        ) : null}
      </div>
    </Modal>
  );
}

function RequirementDecisionForm({
  tenderId,
  requirement,
}: {
  tenderId: string;
  requirement: Requirement;
}) {
  const api = useApi(),
    refresh = useRefresh();
  const allowed: Decision[] = !requirement.is_current
    ? ["withdraw"]
    : requirement.status === "proposed"
      ? ["approve", "withdraw"]
      : ["satisfied", "exception", "reopen", "withdraw"];
  const draft = useFormDraft(
    createDraftScope(
      "submission",
      tenderId,
      `requirement-decision-${requirement.id}`,
      requirement.audit.length,
    ),
    { decision: allowed[0], rationale: "" },
    ["decision", "rationale"],
  );
  const { decision, rationale } = draft.value;
  const setDecision = (value: Decision) => draft.setField("decision", value),
    setRationale = (value: string) => draft.setField("rationale", value);
  const [consent, setConsent] = useState(false),
    [busy, setBusy] = useState(false),
    [error, setError] = useState<unknown>(null);
  const chosen = allowed.includes(decision) ? decision : allowed[0];
  const [applicabilityReviewed, setApplicabilityReviewed] = useState(false);
  const needsApplicabilityReview =
    ["approve", "satisfied", "exception"].includes(chosen) &&
    !requirement.applicability_reviewed &&
    ((requirement.applicability ?? "unknown") !== "unconditional" ||
      (requirement.exceptions?.length ?? 0) > 0);
  async function submit(event: FormEvent) {
    event.preventDefault();
    if (
      busy ||
      !consent ||
      !rationale.trim() ||
      (needsApplicabilityReview && !applicabilityReviewed)
    )
      return;
    setBusy(true);
    setError(null);
    try {
      const acceptedRevision = draft.revision;
      await api.post<Requirement>(
        `${tenderPath(tenderId)}/requirements/${requirement.id}/decision`,
        {
          decision: chosen,
          applicability_reviewed: applicabilityReviewed,
          engineer_confirmed: true,
          rationale,
        } satisfies Schema<"RequirementDecision">,
      );
      draft.markAccepted(acceptedRevision);
      setConsent(false);
      setApplicabilityReviewed(false);
      void refresh();
    } catch (caught) {
      setError(caught);
    } finally {
      setBusy(false);
    }
  }
  const satisfiedBlocked =
    chosen === "satisfied" &&
    (!requirement.linked_outputs.length ||
      requirement.linked_outputs.some((output) => !output.is_current));
  return (
    <form
      className="requirement-form requirement-decision"
      onSubmit={(event) => void submit(event)}
    >
      <h4>Engineer decision</h4>
      <ErrorNotice error={error} />
      <fieldset disabled={busy}>
        <label>
          Decision
          <select
            aria-label="Requirement decision"
            value={chosen}
            onChange={(event) => {
              setDecision(event.target.value as Decision);
              setConsent(false);
              setApplicabilityReviewed(false);
            }}
          >
            {allowed.map((action) => (
              <option key={action} value={action}>
                {decisionLabels[action]}
              </option>
            ))}
          </select>
        </label>
        {satisfiedBlocked ? (
          <p className="requirement-warning">
            Link the current document that addresses the requirement before
            marking it satisfied.
          </p>
        ) : null}
        {chosen === "exception" ? (
          <p className="muted">
            Explain the outstanding requirement and why this exception is being
            recorded. A selected exception will remain visible in the final
            export review.
          </p>
        ) : null}
        <label>
          Decision rationale
          <textarea
            required
            value={rationale}
            maxLength={4000}
            onChange={(event) => setRationale(event.target.value)}
          />
          <FieldError error={error} name="rationale" />
        </label>
        <label className="requirements-checkbox">
          <input
            type="checkbox"
            checked={consent}
            onChange={(event) => setConsent(event.target.checked)}
          />
          I reviewed the requirement, its sources and any linked documents, and
          confirm this decision.
        </label>
        {needsApplicabilityReview ? (
          <div>
            <p className="muted">
              Check the exact clause, conditions and exceptions before this
              decision. Applicability has not yet been reviewed.
            </p>
            <label className="requirements-checkbox">
              <input
                type="checkbox"
                checked={applicabilityReviewed}
                onChange={(event) =>
                  setApplicabilityReviewed(event.target.checked)
                }
              />
              I checked when this requirement applies, including its conditions
              and exceptions.
            </label>
            <FieldError error={error} name="applicability_reviewed" />
          </div>
        ) : null}
        <button
          type="submit"
          className="button primary"
          disabled={
            !consent ||
            !rationale.trim() ||
            satisfiedBlocked ||
            (needsApplicabilityReview && !applicabilityReviewed)
          }
        >
          {busy ? "Recording decision…" : "Record engineer decision"}
        </button>
      </fieldset>
    </form>
  );
}

function applicabilityLabel(value: Requirement["applicability"]) {
  return value === "unconditional"
    ? "Always applies"
    : value === "conditional"
      ? "Only when stated conditions apply"
      : "Not established — needs review";
}

function RequirementApplicability({
  requirement,
}: {
  requirement: Requirement;
}) {
  return (
    <section aria-label="Requirement applicability">
      <dl className="requirement-facts">
        <dt>Applicability</dt>
        <dd>{applicabilityLabel(requirement.applicability)}</dd>
        <dt>Conditions</dt>
        <dd>{requirement.condition || "None recorded"}</dd>
        <dt>Applicability review</dt>
        <dd>
          {requirement.applicability_reviewed
            ? "Reviewed by engineer"
            : "Not yet reviewed"}
        </dd>
      </dl>
      <h4>Exact source clause</h4>
      <blockquote className="whitespace-pre-wrap">
        {requirement.source_quote ||
          "No exact clause recorded. Inspect the cited source."}
      </blockquote>
      <h4>Stated exceptions</h4>
      {requirement.exceptions?.length ? (
        <ul>
          {requirement.exceptions.map((exception, index) => (
            <li key={index}>{exception}</li>
          ))}
        </ul>
      ) : (
        <p>None recorded</p>
      )}
    </section>
  );
}

function RequirementLinks({
  tenderId,
  requirement,
  onNavigate,
}: {
  tenderId: string;
  requirement: Requirement;
  onNavigate?: RegisterProps["onNavigate"];
}) {
  const api = useApi(),
    refresh = useRefresh();
  const outputs = useResource<Schema<"OutputRecord">[]>(
    `${tenderPath(tenderId)}/outputs`,
  );
  const draft = useFormDraft(
    createDraftScope(
      "submission",
      tenderId,
      `requirement-links-${requirement.id}`,
      requirement.audit.length,
    ),
    { selected: "", rationale: "" },
    ["selected", "rationale"],
  );
  const { selected, rationale } = draft.value;
  const setSelected = (value: string) => draft.setField("selected", value),
    setRationale = (value: string) => draft.setField("rationale", value);
  const [consent, setConsent] = useState(false),
    [busy, setBusy] = useState(false),
    [error, setError] = useState<unknown>(null);
  const [removing, setRemoving] = useState<string | null>(null);
  const linkable =
    outputs.data?.filter(
      (output) =>
        output.kind === requirement.deliverable_kind &&
        !requirement.linked_outputs.some(
          (link) => link.output_id === output.id,
        ),
    ) ?? [];
  async function change(event: FormEvent) {
    event.preventDefault();
    if (busy || !consent || !rationale.trim() || (!selected && !removing))
      return;
    setBusy(true);
    setError(null);
    const acceptedRevision = draft.revision;
    const base = `${tenderPath(tenderId)}/requirements/${requirement.id}/outputs`;
    try {
      if (removing)
        await api.post<Requirement>(`${base}/${removing}/unlink`, {
          engineer_confirmed: true,
          rationale,
        } satisfies Schema<"EngineerDecision">);
      else
        await api.post<Requirement>(base, {
          output_id: selected,
          engineer_confirmed: true,
          rationale,
        } satisfies Schema<"RequirementOutputLink">);
      draft.markAccepted(acceptedRevision);
      setRemoving(null);
      setConsent(false);
      void refresh();
    } catch (caught) {
      setError(caught);
    } finally {
      setBusy(false);
    }
  }
  return (
    <section className="requirement-links">
      <h4>Linked generated documents</h4>
      {requirement.linked_outputs.length === 0 ? (
        <p className="muted">No generated documents are linked.</p>
      ) : (
        <ul>
          {requirement.linked_outputs.map((output) => (
            <li key={output.output_id}>
              <button
                type="button"
                className="text-button"
                onClick={() => onNavigate?.("documents", output.output_id)}
                disabled={!onNavigate}
              >
                {output.filename}
              </button>
              <span>
                {output.is_current
                  ? "Current document and working basis"
                  : "Document needs attention"}
              </span>
              {output.recheck_reasons.map((reason) => (
                <p className="requirement-warning" key={reason}>
                  {reason}
                </p>
              ))}
              <small>{output.link_rationale}</small>
              {requirement.status === "approved" ? (
                <button
                  type="button"
                  className="button"
                  disabled={busy}
                  onClick={() => {
                    setRemoving(output.output_id);
                    setSelected("");
                    setConsent(false);
                  }}
                >
                  Remove this link
                </button>
              ) : null}
            </li>
          ))}
        </ul>
      )}
      {requirement.status === "approved" &&
      (requirement.is_current || removing) ? (
        <form
          className="requirement-form"
          onSubmit={(event) => void change(event)}
        >
          <ErrorNotice error={error || outputs.error} />
          <fieldset disabled={busy}>
            {removing ? (
              <p>
                Remove the link to{" "}
                <strong>
                  {
                    requirement.linked_outputs.find(
                      (output) => output.output_id === removing,
                    )?.filename
                  }
                </strong>
                . The document and earlier decision history remain saved.
              </p>
            ) : (
              <>
                <label>
                  Generated document
                  <select
                    aria-label="Generated document for requirement"
                    value={selected}
                    onChange={(event) => {
                      setSelected(event.target.value);
                      setConsent(false);
                    }}
                  >
                    <option value="">Choose a document</option>
                    {linkable.map((output) => (
                      <option key={output.id} value={output.id}>
                        {output.filename}
                      </option>
                    ))}
                  </select>
                </label>
                {outputs.isPending ? (
                  <Loading>Loading generated documents…</Loading>
                ) : linkable.length === 0 ? (
                  <p className="muted">
                    Create a{" "}
                    {deliverables[requirement.deliverable_kind].toLowerCase()}{" "}
                    draft to link it here.{" "}
                    {onNavigate ? (
                      <button
                        type="button"
                        className="text-button"
                        onClick={() =>
                          onNavigate(
                            "documents",
                            `new:${requirement.deliverable_kind}`,
                          )
                        }
                      >
                        Create required document
                      </button>
                    ) : null}
                  </p>
                ) : null}
              </>
            )}
            <label>
              Document link rationale
              <textarea
                required
                maxLength={4000}
                value={rationale}
                onChange={(event) => setRationale(event.target.value)}
              />
              <FieldError error={error} name="rationale" />
            </label>
            <label className="requirements-checkbox">
              <input
                type="checkbox"
                checked={consent}
                onChange={(event) => setConsent(event.target.checked)}
              />
              I reviewed this document link. Changing links requires a new
              completion review.
            </label>
            <div className="inline-actions">
              <button
                className="button"
                type="submit"
                disabled={
                  !consent || !rationale.trim() || (!selected && !removing)
                }
              >
                {busy
                  ? "Saving document link…"
                  : removing
                    ? "Remove reviewed link"
                    : "Link reviewed document"}
              </button>
              {removing ? (
                <button
                  className="button"
                  type="button"
                  onClick={() => {
                    setRemoving(null);
                    setConsent(false);
                  }}
                >
                  Cancel removal
                </button>
              ) : null}
            </div>
          </fieldset>
        </form>
      ) : null}
    </section>
  );
}
