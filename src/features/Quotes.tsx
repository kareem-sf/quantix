import { useCallback, useState, type FormEvent } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import {
  errorText,
  tenderPath,
  useApi,
  useRefresh,
  useResource,
  type Schema,
} from "../api";
import {
  ErrorNotice,
  Loading,
  Modal,
  Status,
  statusLabel,
} from "../components/common";
import { Citations, type SourceSelection } from "./Sources";
import { FieldError } from "../components/FieldError";
import { createDraftScope, useFormDraft } from "./useFormDraft";
import { MailReplyCheck } from "./MailReplyCheck";

type SourceAction = (source: SourceSelection) => void;
const blockedStates = new Set([
  "sending",
  "attempting",
  "sent",
  "partially_sent",
  "uncertain",
]);
const quotePath = (tenderId: string, quoteId: string) =>
  `${tenderPath(tenderId)}/quotes/${encodeURIComponent(quoteId)}`;
const isBlocked = (quote: Schema<"QuoteRecord">) =>
  blockedStates.has(quote.status) ||
  blockedStates.has(quote.delivery_history_status ?? "");
const splitAddresses = (value: string) => [
  ...new Set(
    value
      .split(/[,;\n]+/)
      .map((item) => item.trim())
      .filter(Boolean),
  ),
];

type QuotesProps = {
  tenderId: string;
  onSource?: SourceAction;
  selectedId?: string | null;
  onSelect?: (id: string | null) => void;
  onSettings?: (quoteId?: string) => void;
};
export function Quotes(props: QuotesProps) {
  return <QuoteWorkspace key={props.tenderId} {...props} />;
}
function QuoteWorkspace({
  tenderId,
  onSource,
  selectedId,
  onSelect,
  onSettings,
}: QuotesProps) {
  const quotes = useResource<Schema<"QuoteRecord">[]>(
    `${tenderPath(tenderId)}/quotes`,
  );
  const refresh = useRefresh();
  const [localSelected, setLocalSelected] = useState<string | null>(null);
  const selected = selectedId === undefined ? localSelected : selectedId;
  const setSelected = (id: string | null) => {
    setLocalSelected(id);
    onSelect?.(id);
  };
  const [editing, setEditing] = useState<
    Schema<"QuoteRecord"> | null | undefined
  >(undefined);
  const closeEditor = useCallback(() => setEditing(undefined), []);
  return (
    <section className="correspondence" aria-labelledby="quotes-heading">
      <div className="section-heading">
        <h2 id="quotes-heading">Supplier quotations</h2>
        <button className="button" onClick={() => setEditing(null)}>
          New quotation request
        </button>
      </div>
      <p className="muted">
        Prepare requests, review the exact message and attachments, and record
        supplier replies.
      </p>
      <MailReplyCheck
        onSettings={
          onSettings ? () => onSettings(selected ?? undefined) : undefined
        }
      />
      <ErrorNotice error={quotes.error} />
      {quotes.isPending ? <Loading>Loading quotation requests…</Loading> : null}
      {quotes.data?.length === 0 ? (
        <p className="muted">
          No quotation requests have been saved for this Tender.
        </p>
      ) : null}
      <ul className="quote-list">
        {quotes.data?.map((quote) => (
          <li key={quote.id}>
            <button
              className="quote-list-button"
              aria-label={`Review request: ${quote.subject}`}
              aria-expanded={selected === quote.id}
              onClick={() => setSelected(quote.id)}
            >
              <span>
                <strong>{quote.subject}</strong>
                <span className="muted">To: {quote.to.join(", ")}</span>
                {quote.delivery_history_status &&
                quote.delivery_history_status !== quote.status ? (
                  <span className="muted">
                    Local delivery history:{" "}
                    {statusLabel(quote.delivery_history_status)}
                  </span>
                ) : null}
              </span>
              <Status value={quote.delivery_history_status ?? quote.status} />
            </button>
          </li>
        ))}
      </ul>
      {selected && editing === undefined ? (
        <QuoteDetail
          key={selected}
          tenderId={tenderId}
          quoteId={selected}
          onSource={onSource}
          onEdit={setEditing}
          onClose={() => setSelected(null)}
        />
      ) : null}
      {editing !== undefined ? (
        <DraftEditor
          tenderId={tenderId}
          initial={editing}
          onSource={onSource}
          onClose={closeEditor}
          onSaved={async (quote) => {
            await refresh();
            setSelected(quote.id);
            setEditing(undefined);
          }}
        />
      ) : null}
    </section>
  );
}

function DraftEditor({
  tenderId,
  initial,
  onSource,
  onClose,
  onSaved,
}: {
  tenderId: string;
  initial: Schema<"QuoteRecord"> | null;
  onSource?: SourceAction;
  onClose: () => void;
  onSaved: (quote: Schema<"QuoteRecord">) => Promise<void>;
}) {
  const api = useApi();
  const artifacts = useResource<Schema<"Artifact">[]>(
    `${tenderPath(tenderId)}/artifacts`,
  );
  const draftState = useFormDraft(
    createDraftScope(
      "quotes",
      tenderId,
      `request-${initial?.id ?? "new"}`,
      initial?.updated_at ?? 1,
    ),
    {
      to: initial?.to.join(", ") ?? "",
      cc: initial?.cc?.join(", ") ?? "",
      subject: initial?.subject ?? "",
      body: initial?.body ?? "",
      attachments: initial?.attachment_ids ?? [],
      sources: initial?.source_ids ?? [],
    },
    ["to", "cc", "subject", "body", "attachments", "sources"],
  );
  const { to, cc, subject, body, attachments, sources } = draftState.value;
  const setTo = (value: string) => draftState.setField("to", value),
    setCc = (value: string) => draftState.setField("cc", value),
    setSubject = (value: string) => draftState.setField("subject", value),
    setBody = (value: string) => draftState.setField("body", value);
  const setAttachments = (
    value: string[] | ((current: string[]) => string[]),
  ) =>
    draftState.setField(
      "attachments",
      typeof value === "function" ? value(attachments) : value,
    );
  const setSources = (value: string[]) => draftState.setField("sources", value);
  const [attachmentFilter, setAttachmentFilter] = useState("");
  const [pending, setPending] = useState(false),
    [error, setError] = useState<unknown>(null);
  const currentArtifacts =
    artifacts.data?.filter((artifact) => artifact.is_current) ?? [];
  const olderAttachments =
    initial?.attachments.filter(
      (attachment) =>
        !currentArtifacts.some(
          (artifact) => artifact.id === attachment.artifact_id,
        ),
    ) ?? [];
  const filterText = attachmentFilter.trim().toLowerCase();
  const visibleAttachments = currentArtifacts.filter((artifact) =>
    artifact.relative_path.toLowerCase().includes(filterText),
  );
  const visibleOlderAttachments = olderAttachments.filter(
    (attachment) =>
      attachments.includes(attachment.artifact_id) &&
      attachment.filename.toLowerCase().includes(filterText),
  );
  const visibleSelected =
    visibleAttachments.filter((artifact) => attachments.includes(artifact.id))
      .length + visibleOlderAttachments.length;
  async function save(event: FormEvent) {
    event.preventDefault();
    if (pending) return;
    setPending(true);
    setError(null);
    const acceptedRevision = draftState.revision;
    const draft: Schema<"DraftInput"> = {
      to: splitAddresses(to),
      cc: splitAddresses(cc),
      subject: subject.trim(),
      body,
      attachment_ids: attachments,
      source_ids: sources,
    };
    try {
      if (draft.to.length > 30 || (draft.cc?.length ?? 0) > 20)
        throw new Error("Use at most 30 To recipients and 20 Cc recipients.");
      const quote = initial
        ? await api.patch<Schema<"QuoteRecord">>(
            quotePath(tenderId, initial.id),
            draft,
          )
        : await api.post<Schema<"QuoteRecord">>(
            `${tenderPath(tenderId)}/quotes`,
            draft,
          );
      draftState.markAccepted(acceptedRevision);
      await onSaved(quote);
    } catch (failure) {
      setError(failure);
    } finally {
      setPending(false);
    }
  }
  return (
    <Modal
      title={initial ? "Edit quotation request" : "New quotation request"}
      onClose={pending ? () => {} : onClose}
    >
      <form className="quote-editor" onSubmit={save}>
        <p className="muted">
          Saving a draft sends no mail. Editing clears its earlier approval.
        </p>
        <fieldset disabled={pending}>
          <label>
            To
            <input
              required
              value={to}
              onChange={(event) => setTo(event.target.value)}
            />
            <FieldError error={error} name="to" />
          </label>
          <p className="field-help">
            Plain email addresses, separated by commas. Up to 30 recipients.
          </p>
          <label>
            Cc
            <input value={cc} onChange={(event) => setCc(event.target.value)} />
            <FieldError error={error} name="cc" />
          </label>
          <label>
            Subject
            <input
              required
              maxLength={300}
              value={subject}
              onChange={(event) => setSubject(event.target.value)}
            />
            <FieldError error={error} name="subject" />
          </label>
          <label>
            Message
            <textarea
              required
              rows={7}
              maxLength={100000}
              value={body}
              onChange={(event) => setBody(event.target.value)}
            />
            <FieldError error={error} name="body" />
          </label>
        </fieldset>
        <fieldset disabled={pending || artifacts.isPending}>
          <legend>Attach Tender files ({attachments.length}/20)</legend>
          <p className="field-help">
            Select saved source files from this Tender. Maximum 20 files and 20
            MiB total.
          </p>
          <ErrorNotice error={artifacts.error} />
          <label>
            Filter attachment files
            <input
              value={attachmentFilter}
              onChange={(event) => setAttachmentFilter(event.target.value)}
              placeholder="Type part of a file path…"
            />
          </label>
          <p className="quote-attachment-summary" aria-live="polite">
            {attachments.length} selected
            {attachments.length > visibleSelected
              ? ` · ${attachments.length - visibleSelected} outside this filter`
              : ""}
          </p>
          <div
            className="quote-attachment-selector"
            role="group"
            aria-label="Available attachment files"
          >
            {visibleAttachments.map((artifact) => (
              <label className="correspondence-check" key={artifact.id}>
                <input
                  type="checkbox"
                  checked={attachments.includes(artifact.id)}
                  disabled={
                    !attachments.includes(artifact.id) &&
                    attachments.length >= 20
                  }
                  onChange={(event) =>
                    setAttachments((current) =>
                      event.target.checked
                        ? [...current, artifact.id]
                        : current.filter((id) => id !== artifact.id),
                    )
                  }
                />
                <span>
                  {artifact.relative_path} · Version {artifact.version} ·{" "}
                  {artifact.size.toLocaleString()} bytes
                </span>
              </label>
            ))}
            {visibleOlderAttachments.map((attachment) =>
              attachments.includes(attachment.artifact_id) ? (
                <label
                  className="correspondence-check"
                  key={attachment.artifact_id}
                >
                  <input
                    type="checkbox"
                    checked
                    onChange={() =>
                      setAttachments((current) =>
                        current.filter((id) => id !== attachment.artifact_id),
                      )
                    }
                  />
                  {attachment.filename} · Saved version {attachment.version},
                  outside the current file list
                </label>
              ) : null,
            )}
            {filterText &&
            visibleAttachments.length === 0 &&
            visibleOlderAttachments.length === 0 ? (
              <p className="field-help">No files match this filter.</p>
            ) : null}
          </div>
          {artifacts.data?.length === 0 ? (
            <p className="field-help">
              Import Tender files to attach them here.
            </p>
          ) : null}
        </fieldset>
        <SourcePicker
          tenderId={tenderId}
          ids={sources}
          onChange={setSources}
          onSource={onSource}
          disabled={pending}
        />
        <ErrorNotice error={error || draftState.error} />
        <div className="form-actions">
          <button
            className="button"
            type="button"
            disabled={pending}
            onClick={onClose}
          >
            Cancel
          </button>
          <button
            className="button primary"
            disabled={pending || !to.trim() || !subject.trim() || !body.trim()}
          >
            {pending ? "Saving…" : "Save draft"}
          </button>
        </div>
      </form>
    </Modal>
  );
}

function SourcePicker({
  tenderId,
  ids,
  onChange,
  onSource,
  disabled,
}: {
  tenderId: string;
  ids: string[];
  onChange: (ids: string[]) => void;
  onSource?: SourceAction;
  disabled: boolean;
}) {
  const api = useApi();
  const [term, setTerm] = useState(""),
    [query, setQuery] = useState("");
  const path = `${tenderPath(tenderId)}/search?q=${encodeURIComponent(query)}&mode=words`;
  const results = useQuery({
    queryKey: [path],
    queryFn: () => api.get<Schema<"Evidence">[]>(path),
    enabled: !!query,
    retry: false,
  });
  return (
    <details className="quote-source-picker">
      <summary>Supporting source references ({ids.length})</summary>
      <p className="field-help">
        Search source text in this Tender and select the evidence supporting
        this correspondence.
      </p>
      <label>
        Find supporting sources
        <input
          value={term}
          maxLength={1000}
          onChange={(event) => setTerm(event.target.value)}
          disabled={disabled}
        />
      </label>
      <button
        type="button"
        className="text-button"
        disabled={disabled || !term.trim()}
        onClick={() => setQuery(term.trim())}
      >
        Search source text
      </button>
      <ErrorNotice error={results.error} />
      {results.isFetching ? <Loading>Finding sources…</Loading> : null}
      <ul>
        {results.data?.map((source) => (
          <li key={source.id}>
            <button
              type="button"
              className="source-result"
              disabled={
                disabled || (!ids.includes(source.id) && ids.length >= 100)
              }
              onClick={() =>
                onChange(
                  ids.includes(source.id)
                    ? ids.filter((id) => id !== source.id)
                    : [...ids, source.id],
                )
              }
            >
              {ids.includes(source.id) ? "Remove" : "Add"}:{" "}
              {source.artifact_name} · {source.locator}
              <span>{source.text.slice(0, 240)}</span>
            </button>
          </li>
        ))}
      </ul>
      {results.data?.length === 0 ? (
        <p className="field-help">No source text matched.</p>
      ) : null}
      {ids.map((id) => (
        <div className="inline-actions" key={id}>
          {onSource ? (
            <Citations ids={[id]} tenderId={tenderId} onOpen={onSource} />
          ) : (
            <span>{id}</span>
          )}
          <button
            className="text-button"
            type="button"
            disabled={disabled}
            onClick={() => onChange(ids.filter((existing) => existing !== id))}
          >
            Remove source
          </button>
        </div>
      ))}
    </details>
  );
}

function QuoteDetail({
  tenderId,
  quoteId,
  onSource,
  onEdit,
  onClose,
}: {
  tenderId: string;
  quoteId: string;
  onSource?: SourceAction;
  onEdit: (quote: Schema<"QuoteRecord">) => void;
  onClose: () => void;
}) {
  const preview = useResource<Schema<"QuotePreview">>(
    `${quotePath(tenderId, quoteId)}/preview`,
  );
  if (preview.isPending) return <Loading>Loading exact message…</Loading>;
  if (!preview.data) return <ErrorNotice error={preview.error} />;
  return (
    <QuoteReview
      key={preview.data.fingerprint}
      preview={preview.data}
      tenderId={tenderId}
      onSource={onSource}
      onEdit={onEdit}
      onClose={onClose}
    />
  );
}

function QuoteReview({
  preview,
  tenderId,
  onSource,
  onEdit,
  onClose,
}: {
  preview: Schema<"QuotePreview">;
  tenderId: string;
  onSource?: SourceAction;
  onEdit: (quote: Schema<"QuoteRecord">) => void;
  onClose: () => void;
}) {
  const api = useApi(),
    refresh = useRefresh(),
    client = useQueryClient();
  const quote = preview.quote,
    base = quotePath(tenderId, quote.id),
    blocked = isBlocked(quote);
  const decisionDraft = useFormDraft(
    createDraftScope("quotes", tenderId, `decision-${quote.id}`, 1),
    { rationale: "", reconciliation: "" },
    ["rationale", "reconciliation"],
  );
  const { rationale, reconciliation } = decisionDraft.value;
  const setRationale = (value: string) =>
      decisionDraft.setField("rationale", value),
    setReconciliation = (value: string) =>
      decisionDraft.setField("reconciliation", value);
  const [confirmed, setConfirmed] = useState(false),
    [restoreRefused, setRestoreRefused] = useState(false);
  const needsReconciliation =
    preview.restore_reconciliation_required || restoreRefused;
  const [pending, setPending] = useState(false),
    [downloading, setDownloading] = useState(false);
  const [error, setError] = useState<unknown>(null),
    [notice, setNotice] = useState("");
  const [sendFailed, setSendFailed] = useState(false);
  const allowed =
    !blocked && ["draft", "approved", "failed"].includes(quote.status);
  const decisionReady =
    confirmed &&
    !!rationale.trim() &&
    (!needsReconciliation || !!reconciliation.trim());
  async function decide(send: boolean) {
    if (
      pending ||
      !decisionReady ||
      !allowed ||
      (send && (!preview.smtp_ready || sendFailed))
    )
      return;
    setPending(true);
    setError(null);
    setNotice("");
    const acceptedRevision = decisionDraft.revision;
    const decision: Schema<"SendDecision"> = {
      fingerprint: preview.fingerprint,
      engineer_confirmed: true,
      rationale: rationale.trim(),
      ...(reconciliation.trim()
        ? { restore_reconciliation: reconciliation.trim() }
        : {}),
    };
    try {
      await api.post<Schema<"QuoteRecord">>(`${base}/approve`, decision);
      if (send) {
        await api.post<Schema<"QuoteRecord">>(`${base}/send`, decision);
        setNotice(
          "Submission result recorded. Review the delivery status above.",
        );
      } else setNotice("Approval recorded. No mail was sent by this action.");
      decisionDraft.markAccepted(acceptedRevision);
      setConfirmed(false);
      await refresh();
    } catch (failure) {
      setError(failure);
      setConfirmed(false);
      if (send) setSendFailed(true);
      if (
        /predates a workspace restore|reconciliation/i.test(errorText(failure))
      ) {
        setRestoreRefused(true);
        setSendFailed(false);
      }
      await refresh();
    } finally {
      setPending(false);
    }
  }
  return (
    <div className="quote-review">
      <div className="section-heading">
        <h3>Review quotation request</h3>
        <button className="text-button" disabled={pending} onClick={onClose}>
          Close request
        </button>
      </div>
      <dl className="quote-envelope">
        <dt>From</dt>
        <dd>{preview.sender ?? "Sender email is not configured"}</dd>
        <dt>To</dt>
        <dd>{quote.to.join(", ")}</dd>
        <dt>Cc</dt>
        <dd>{quote.cc?.join(", ") || "None"}</dd>
        <dt>Subject</dt>
        <dd>{quote.subject}</dd>
        <dt>Mail server</dt>
        <dd>{preview.smtp_host || "Not configured"}</dd>
        <dt>Saved state</dt>
        <dd>
          <Status value={quote.status} />
        </dd>
      </dl>
      {quote.delivery_history_status ? (
        <div className="quote-warning">
          <p>
            Local delivery history: {statusLabel(quote.delivery_history_status)}
          </p>
          {quote.delivery_history_fingerprint &&
          quote.delivery_history_fingerprint !== preview.fingerprint ? (
            <p>
              This saved wording differs from the recorded submission. The
              restored record does not establish what was sent.
            </p>
          ) : null}
          <p>
            Keep this delivery history separate from an older restored draft
            state.
          </p>
        </div>
      ) : null}
      {quote.delivery_detail ? (
        <p className="quote-warning">{quote.delivery_detail}</p>
      ) : null}
      {blocked ? (
        <p className="quote-warning">
          This request has a previous or uncertain submission. Sending and
          editing are blocked. Check the sent mailbox or supplier before
          preparing any new correspondence.
        </p>
      ) : null}
      {quote.refused_recipients.length ? (
        <div className="quote-warning">
          <strong>Refused recipients</strong>
          {quote.refused_recipients.map((recipient) => (
            <p key={recipient.address}>
              {recipient.address} · SMTP code {recipient.code}
            </p>
          ))}
        </div>
      ) : null}
      {preview.warnings.map((warning, index) => (
        <p className="quote-warning" key={index}>
          {warning}
        </p>
      ))}
      <pre className="quote-body">{quote.body}</pre>
      <h4>Attachments ({preview.attachments.length})</h4>
      {preview.attachments.length ? (
        <ul className="quote-attachments">
          {preview.attachments.map((attachment) => (
            <li key={attachment.artifact_id}>
              <strong>{attachment.filename}</strong>
              <p>
                Version {attachment.version} ·{" "}
                {attachment.size.toLocaleString()} bytes ·{" "}
                {attachment.is_current
                  ? "Current source"
                  : "Older source revision"}
              </p>
              <code>SHA-256: {attachment.content_hash}</code>
              {onSource ? (
                <button
                  className="text-button"
                  onClick={() =>
                    onSource({ artifactId: attachment.artifact_id })
                  }
                >
                  Inspect attached source
                </button>
              ) : null}
            </li>
          ))}
        </ul>
      ) : (
        <p className="field-help">No files attached.</p>
      )}
      {onSource ? (
        <Citations
          ids={quote.source_ids ?? []}
          tenderId={tenderId}
          onOpen={onSource}
        />
      ) : null}
      <details className="quote-fingerprint">
        <summary>Message identity and reviewed content</summary>
        <code>Message-ID: {quote.message_id}</code>
        <code>Review fingerprint: {preview.fingerprint}</code>
        {quote.delivery_history_fingerprint ? (
          <code>
            Delivery history fingerprint: {quote.delivery_history_fingerprint}
          </code>
        ) : null}
      </details>
      <div className="inline-actions">
        {allowed ? (
          <button
            className="button"
            disabled={pending}
            onClick={() => onEdit(quote)}
          >
            Edit draft
          </button>
        ) : null}
        <button
          className="button"
          disabled={downloading || pending}
          onClick={async () => {
            setDownloading(true);
            setError(null);
            try {
              const blob = await api.blob(`${base}/eml`),
                url = URL.createObjectURL(blob);
              const link = document.createElement("a");
              link.href = url;
              link.download = "quotation-request.eml";
              document.body.append(link);
              link.click();
              link.remove();
              setTimeout(() => URL.revokeObjectURL(url), 1000);
            } catch (failure) {
              setError(failure);
            } finally {
              setDownloading(false);
            }
          }}
        >
          {downloading ? "Preparing EML…" : "Download EML"}
        </button>
      </div>
      <p className="field-help">
        An EML download does not submit mail or record delivery. Mail sent
        outside Quantix must be checked separately before sending here.
      </p>
      {allowed ? (
        <fieldset disabled={pending}>
          <legend>Engineer approval</legend>
          <p className="muted">
            Approve this exact message for later use, or approve and submit it
            to the listed recipients now. SMTP acceptance does not confirm inbox
            delivery.
          </p>
          {!preview.smtp_ready ? (
            <p className="warning-text">
              Configure SMTP in Settings to send. You can still approve or
              download the request.
            </p>
          ) : null}
          <label>
            Approval note
            <textarea
              rows={3}
              maxLength={4000}
              value={rationale}
              onChange={(event) => setRationale(event.target.value)}
            />
          </label>
          <details
            className="quote-reconciliation"
            open={needsReconciliation || undefined}
          >
            <summary>Delivery check after a workspace restore</summary>
            <p className="field-help">
              If this draft predates a restore, check the sent mailbox or
              supplier and record that this request was not already submitted. A
              reconciliation note cannot override a recorded submission.
            </p>
            <label>
              Restore delivery reconciliation
              <textarea
                rows={3}
                maxLength={4000}
                required={needsReconciliation}
                value={reconciliation}
                onChange={(event) => setReconciliation(event.target.value)}
              />
            </label>
          </details>
          <label className="correspondence-check">
            <input
              type="checkbox"
              checked={confirmed}
              onChange={(event) => setConfirmed(event.target.checked)}
            />
            I have reviewed the recipients, sender, message and attachment
            versions shown above.
          </label>
          {sendFailed ? (
            <div className="quote-warning">
              <p>
                The send request failed. Delivery has not been established by
                this screen. Check the saved status and mailbox before taking
                further action.
              </p>
              <button
                type="button"
                className="text-button"
                onClick={async () => {
                  setPending(true);
                  setConfirmed(false);
                  try {
                    const current = await api.get<Schema<"QuotePreview">>(
                      `${base}/preview`,
                    );
                    client.setQueryData([`${base}/preview`], current);
                    setSendFailed(false);
                    setError(null);
                  } catch (failure) {
                    setError(failure);
                  } finally {
                    setPending(false);
                  }
                }}
              >
                Refresh delivery status for a new review
              </button>
            </div>
          ) : null}
          <div className="form-actions">
            <button
              className="button"
              disabled={!decisionReady}
              onClick={() => void decide(false)}
            >
              Approve only
            </button>
            <button
              className="button primary"
              disabled={!decisionReady || !preview.smtp_ready || sendFailed}
              onClick={() => void decide(true)}
            >
              {pending ? "Recording decision…" : "Approve and send"}
            </button>
          </div>
        </fieldset>
      ) : null}
      <ErrorNotice error={error} />
      {notice ? (
        <p role="status" className="success-text">
          {notice}
        </p>
      ) : null}
      <QuoteReplies
        tenderId={tenderId}
        quoteId={quote.id}
        onSource={onSource}
      />
    </div>
  );
}

function QuoteReplies({
  tenderId,
  quoteId,
  onSource,
}: {
  tenderId: string;
  quoteId: string;
  onSource?: SourceAction;
}) {
  const replies = useResource<Schema<"ReplyRecord">[]>(
    `${quotePath(tenderId, quoteId)}/replies`,
  );
  const [registering, setRegistering] = useState(false);
  const close = useCallback(() => setRegistering(false), []);
  return (
    <section className="quote-replies">
      <div className="section-heading">
        <h3>Supplier replies</h3>
        <button className="button" onClick={() => setRegistering(true)}>
          Register supplier reply
        </button>
      </div>
      <p className="field-help">
        Replies are evidence for review. They do not approve a rate, quantity or
        commercial condition. Automatic matching identifies the thread, not the
        sender’s authority.
      </p>
      <ErrorNotice error={replies.error} />
      {replies.isPending ? <Loading>Loading replies…</Loading> : null}
      {replies.data?.length === 0 ? (
        <p className="field-help">
          No replies registered. Check configured mail in Settings or record a
          reply here.
        </p>
      ) : null}
      {replies.data?.map((reply) => (
        <article className="quote-reply" key={reply.id}>
          <h4>{reply.subject || "Supplier reply"}</h4>
          <p className="muted">
            {reply.sender} ·{" "}
            {reply.origin === "manual"
              ? "Recorded by engineer"
              : "Read from mailbox"}
          </p>
          <p className="muted">
            {reply.date_basis === "retrieved_at"
              ? "Retrieved"
              : "Received (engineer entered)"}
            : {reply.received_at}
          </p>
          {reply.date_header ? (
            <p className="muted">Original Date header: {reply.date_header}</p>
          ) : null}
          <pre className="quote-body">{reply.text}</pre>
          {reply.warnings.map((warning, index) => (
            <p key={index} className="warning-text">
              {warning}
            </p>
          ))}
          {onSource ? (
            <Citations
              ids={reply.source_ids}
              tenderId={tenderId}
              onOpen={onSource}
            />
          ) : (
            <p className="field-help">
              Registered evidence: {reply.source_ids.join(", ")}
            </p>
          )}
          {reply.headers ? (
            <details>
              <summary>Saved message headers</summary>
              <pre className="quote-body">{reply.headers}</pre>
            </details>
          ) : null}
        </article>
      ))}
      {registering ? (
        <ReplyEditor
          tenderId={tenderId}
          quoteId={quoteId}
          onSource={onSource}
          onClose={close}
        />
      ) : null}
    </section>
  );
}

function ReplyEditor({
  tenderId,
  quoteId,
  onSource,
  onClose,
}: {
  tenderId: string;
  quoteId: string;
  onSource?: SourceAction;
  onClose: () => void;
}) {
  const api = useApi(),
    refresh = useRefresh();
  const draft = useFormDraft(
    createDraftScope("quotes", tenderId, `reply-${quoteId}`, 1),
    {
      sender: "",
      receivedAt: "",
      subject: "",
      text: "",
      headers: "",
      sources: [] as string[],
    },
    ["sender", "receivedAt", "subject", "text", "headers", "sources"],
  );
  const { sender, receivedAt, subject, text, headers, sources } = draft.value;
  const setSender = (v: string) => draft.setField("sender", v),
    setReceivedAt = (v: string) => draft.setField("receivedAt", v),
    setSubject = (v: string) => draft.setField("subject", v),
    setText = (v: string) => draft.setField("text", v),
    setHeaders = (v: string) => draft.setField("headers", v),
    setSources = (v: string[]) => draft.setField("sources", v);
  const [pending, setPending] = useState(false),
    [error, setError] = useState<unknown>(null);
  return (
    <Modal
      title="Register supplier reply"
      onClose={pending ? () => {} : onClose}
    >
      <form
        className="quote-editor"
        onSubmit={async (event) => {
          event.preventDefault();
          if (pending) return;
          setError(null);
          if (
            !/(?:Z|[+-]\d{2}:\d{2})$/i.test(receivedAt.trim()) ||
            Number.isNaN(Date.parse(receivedAt))
          ) {
            setError(
              new Error(
                "Enter a valid received date and time including its time zone, such as 2026-09-06T13:00:00+03:00.",
              ),
            );
            return;
          }
          const acceptedRevision = draft.revision;
          setPending(true);
          try {
            await api.post<Schema<"ReplyRecord">>(
              `${quotePath(tenderId, quoteId)}/replies`,
              {
                sender,
                received_at: receivedAt.trim(),
                subject,
                text,
                headers,
                source_ids: sources,
              } satisfies Schema<"ManualReply">,
            );
            draft.markAccepted(acceptedRevision);
            await refresh();
            onClose();
          } catch (failure) {
            setError(failure);
          } finally {
            setPending(false);
          }
        }}
      >
        <fieldset disabled={pending}>
          <label>
            Supplier email
            <input
              type="email"
              required
              value={sender}
              onChange={(event) => setSender(event.target.value)}
            />
            <FieldError error={error} name="sender" />
          </label>
          <label>
            Received date and time with time zone
            <input
              required
              value={receivedAt}
              placeholder="2026-09-06T13:00:00+03:00"
              onChange={(event) => setReceivedAt(event.target.value)}
            />
            <FieldError error={error} name="received_at" />
          </label>
          <p className="field-help">
            Copy the received date with its UTC offset. This is recorded as an
            engineer-entered date.
          </p>
          <label>
            Reply subject
            <input
              value={subject}
              maxLength={500}
              onChange={(event) => setSubject(event.target.value)}
            />
            <FieldError error={error} name="subject" />
          </label>
          <label>
            Reply text
            <textarea
              required
              rows={6}
              maxLength={200000}
              value={text}
              onChange={(event) => setText(event.target.value)}
            />
            <FieldError error={error} name="text" />
          </label>
          <details>
            <summary>Original headers (optional)</summary>
            <label>
              Reply headers
              <textarea
                rows={3}
                maxLength={50000}
                value={headers}
                onChange={(event) => setHeaders(event.target.value)}
              />
              <FieldError error={error} name="headers" />
            </label>
          </details>
        </fieldset>
        <SourcePicker
          tenderId={tenderId}
          ids={sources}
          onChange={setSources}
          onSource={onSource}
          disabled={pending}
        />
        <ErrorNotice error={error || draft.error} />
        <div className="form-actions">
          <button
            className="button"
            type="button"
            disabled={pending}
            onClick={onClose}
          >
            Cancel
          </button>
          <button
            className="button primary"
            disabled={
              pending || !sender.trim() || !text.trim() || !receivedAt.trim()
            }
          >
            {pending ? "Saving…" : "Save supplier reply"}
          </button>
        </div>
      </form>
    </Modal>
  );
}
