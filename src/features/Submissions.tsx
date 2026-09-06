import { useCallback, useState } from "react";
import {
  tenderPath,
  useApi,
  useRefresh,
  useResource,
  type Schema,
} from "../api";
import { ErrorNotice, Loading, Modal } from "../components/ui";
import "../styles/deliverables.css";

export function Submissions({
  tenderId,
  outputs,
}: {
  tenderId: string;
  outputs: Schema<"OutputRecord">[];
}) {
  const api = useApi(),
    refresh = useRefresh(),
    base = tenderPath(tenderId);
  const records = useResource<Schema<"SubmissionRecord">[]>(
    `${base}/submissions`,
  );
  const [selected, setSelected] = useState<string[]>([]),
    [checkRequirements, setCheckRequirements] = useState(true),
    [preview, setPreview] = useState<Schema<"SubmissionPreview"> | null>(null),
    [busy, setBusy] = useState(false),
    [error, setError] = useState<unknown>(null),
    [notice, setNotice] = useState("");
  const close = useCallback(() => setPreview(null), []);
  async function review() {
    setBusy(true);
    setError(null);
    setNotice("");
    try {
      setPreview(
        await api.post<Schema<"SubmissionPreview">>(
          `${base}/submissions/preview`,
          { output_ids: selected, requirement_ids: checkRequirements ? null : [] } satisfies Schema<"SubmissionSelection">,
        ),
      );
    } catch (failure) {
      setError(failure);
    } finally {
      setBusy(false);
    }
  }
  return (
    <section className="submissions-section">
      <h2>Reviewed local exports</h2>
      <p className="muted">
        Select saved drafts, review their current sources and record the exact
        scope before approving a local package.
      </p>
      {outputs.length ? (
        <>
          <label className="checkbox-label">
            <input type="checkbox" checked={checkRequirements} disabled={busy} onChange={(event) => { setCheckRequirements(event.target.checked); setPreview(null); }} />
            Check all registered submission requirements for this export.
          </label>
          {!checkRequirements ? <p className="field-help">This is a partial export. Omitted requirements will be listed for explicit acknowledgement.</p> : null}
          <fieldset className="submission-selection" disabled={busy}>
            <legend>Files to review</legend>
            {outputs.map((output) => (
              <label key={output.id} className="checkbox-label">
                <input
                  type="checkbox"
                  checked={selected.includes(output.id)}
                  disabled={
                    !selected.includes(output.id) && selected.length >= 30
                  }
                  onChange={(event) => {
                    setSelected((current) =>
                      event.target.checked
                        ? [...current, output.id]
                        : current.filter((id) => id !== output.id),
                    );
                    setPreview(null);
                    setNotice("");
                  }}
                />
                {output.filename}
              </label>
            ))}
          </fieldset>
          <button
            type="button"
            className="button"
            disabled={busy || !selected.length}
            onClick={() => void review()}
          >
            {busy ? "Checking selected files…" : "Review selected files"}
          </button>
        </>
      ) : (
        <p className="field-help">
          Create and review draft documents before selecting a local export.
        </p>
      )}
      <ErrorNotice error={error || records.error} />
      {notice ? (
        <p className="success-text" role="status">
          {notice}
        </p>
      ) : null}
      {records.isPending ? <Loading>Loading approved exports…</Loading> : null}
      {records.data?.map((record) => (
        <ExportRecord key={record.id} tenderId={tenderId} record={record} />
      ))}
      {preview ? (
        <ExportReview
          tenderId={tenderId}
          outputIds={selected}
          requirementIds={checkRequirements ? null : []}
          initial={preview}
          onClose={close}
          onApproved={async () => {
            setPreview(null);
            setSelected([]);
            await refresh();
            setNotice("Local export approved. No files were sent.");
          }}
        />
      ) : null}
    </section>
  );
}

function ExportReview({
  tenderId,
  outputIds,
  requirementIds,
  initial,
  onClose,
  onApproved,
}: {
  tenderId: string;
  outputIds: string[];
  requirementIds: string[] | null;
  initial: Schema<"SubmissionPreview">;
  onClose: () => void;
  onApproved: () => Promise<void>;
}) {
  const api = useApi(),
    base = tenderPath(tenderId);
  const [preview, setPreview] = useState(initial),
    [validPreview, setValidPreview] = useState(true),
    [acknowledged, setAcknowledged] = useState<string[]>([]),
    [scope, setScope] = useState(""),
    [rationale, setRationale] = useState(""),
    [confirmed, setConfirmed] = useState(false),
    [busy, setBusy] = useState(false),
    [error, setError] = useState<unknown>(null);
  const canApprove =
    validPreview &&
    !preview.blocking_reasons.length &&
    !!scope.trim() &&
    !!rationale.trim() &&
    confirmed &&
    preview.warnings.every((warning) => acknowledged.includes(warning));
  async function refreshPreview() {
    setBusy(true);
    setError(null);
    setConfirmed(false);
    setAcknowledged([]);
    setValidPreview(false);
    try {
      setPreview(
        await api.post<Schema<"SubmissionPreview">>(
          `${base}/submissions/preview`,
          { output_ids: outputIds, requirement_ids: requirementIds } satisfies Schema<"SubmissionSelection">,
        ),
      );
      setValidPreview(true);
    } catch (failure) {
      setError(failure);
    } finally {
      setBusy(false);
    }
  }
  return (
    <Modal title="Review local export" onClose={onClose}>
      <form
        className="submission-form"
        onSubmit={async (event) => {
          event.preventDefault();
          if (!canApprove || busy) return;
          setBusy(true);
          setError(null);
          try {
            await api.post<Schema<"SubmissionRecord">>(`${base}/submissions`, {
              output_ids: outputIds,
              requirement_ids: requirementIds,
              fingerprint: preview.fingerprint,
              engineer_confirmed: true,
              final_review_confirmed: true,
              acknowledged_scope: scope.trim(),
              acknowledged_gaps: preview.warnings.filter((warning) =>
                acknowledged.includes(warning),
              ),
              rationale: rationale.trim(),
            } satisfies Schema<"SubmissionApproval">);
            await onApproved();
          } catch (failure) {
            setError(failure);
            setValidPreview(false);
            setConfirmed(false);
            setAcknowledged([]);
          } finally {
            setBusy(false);
          }
        }}
      >
        <p className="muted">
          Approval freezes only these files and your stated scope into a local
          ZIP. It does not send files to the client.
        </p>
        <ul className="submission-preview-files">
          {preview.outputs.map((output, index) => (
            <li key={String(output.id ?? index)}>
              <strong>
                {typeof output.filename === "string"
                  ? output.filename
                  : "Selected document"}
              </strong>
              <p className="field-help">
                File SHA-256:{" "}
                <code>
                  {typeof output.sha256 === "string"
                    ? output.sha256
                    : "Unavailable"}
                </code>
              </p>
            </li>
          ))}
        </ul>
        {preview.requirements.length ? (
          <section>
            <h3>Submission requirements in this review</h3>
            {preview.requirements.map((requirement, index) => (
              <p key={String(requirement.id ?? index)}>
                <strong>{String(requirement.title)}</strong> · {String(requirement.status)} · {String(requirement.review_status)}
              </p>
            ))}
          </section>
        ) : null}
        <details>
          <summary>Review identity</summary>
          <code>{preview.fingerprint}</code>
        </details>
        {preview.blocking_reasons.length ? (
          <div className="submission-blockers">
            <strong>Resolve these items before final export</strong>
            <ul>
              {preview.blocking_reasons.map((reason) => (
                <li key={reason}>{reason}</li>
              ))}
            </ul>
          </div>
        ) : null}
        <fieldset className="plain-fieldset" disabled={busy}>
          <label>
            Approved scope
            <textarea
              required
              rows={3}
              maxLength={6000}
              value={scope}
              onChange={(event) => setScope(event.target.value)}
            />
          </label>
          <h3>Scope limits and gaps</h3>
          {preview.warnings.map((warning) => (
            <label key={warning} className="checkbox-label">
              <input
                type="checkbox"
                checked={acknowledged.includes(warning)}
                onChange={(event) =>
                  setAcknowledged((current) =>
                    event.target.checked
                      ? [...current, warning]
                      : current.filter((value) => value !== warning),
                  )
                }
              />
              {warning}
            </label>
          ))}
          <label>
            Export approval note
            <textarea
              required
              rows={3}
              maxLength={4000}
              value={rationale}
              onChange={(event) => setRationale(event.target.value)}
            />
          </label>
          <label className="checkbox-label">
            <input
              type="checkbox"
              required
              checked={confirmed}
              onChange={(event) => setConfirmed(event.target.checked)}
            />
            I completed the final review of these exact files and approve this
            local export.
          </label>
        </fieldset>
        <ErrorNotice error={error} />
        {!validPreview ? (
          <p className="field-help">
            Refresh the review and check the current files and scope limits
            before approving again.
          </p>
        ) : null}
        <div className="form-actions">
          <button
            type="button"
            className="button"
            disabled={busy}
            onClick={onClose}
          >
            Cancel
          </button>
          <button
            type="button"
            className="button"
            disabled={busy}
            onClick={() => void refreshPreview()}
          >
            Refresh export review
          </button>
          <button className="button primary" disabled={busy || !canApprove}>
            {busy ? "Working…" : "Approve local export"}
          </button>
        </div>
      </form>
    </Modal>
  );
}

function ExportRecord({
  tenderId,
  record,
}: {
  tenderId: string;
  record: Schema<"SubmissionRecord">;
}) {
  const api = useApi();
  const [busy, setBusy] = useState(false),
    [error, setError] = useState<unknown>(null);
  return (
    <article className="submission-record">
      <h3>{record.filename}</h3>
      <p className="field-help">
        Approved {new Date(record.created_at).toLocaleString()} · Saved local
        export · No files sent
      </p>
      <p>
        <strong>Approved scope:</strong> {record.acknowledged_scope}
      </p>
      <details>
        <summary>Review record and included files</summary>
        <p>{record.rationale}</p>
        <ul>
          {record.outputs.map((output, index) => (
            <li key={String(output.id ?? index)}>
              {typeof output.filename === "string"
                ? output.filename
                : "Saved document"}
            </li>
          ))}
        </ul>
        {record.acknowledged_gaps.map((gap) => (
          <p key={gap}>{gap}</p>
        ))}
        <p>
          Export SHA-256: <code>{record.sha256}</code>
        </p>
      </details>
      <ErrorNotice error={error} />
      <button
        type="button"
        className="button"
        disabled={busy}
        onClick={async () => {
          setBusy(true);
          setError(null);
          try {
            const blob = await api.blob(
                `${tenderPath(tenderId)}/submissions/${record.id}/download`,
              ),
              url = URL.createObjectURL(blob),
              link = document.createElement("a");
            link.href = url;
            link.download = record.filename;
            document.body.append(link);
            link.click();
            link.remove();
            window.setTimeout(() => URL.revokeObjectURL(url), 1000);
          } catch (failure) {
            setError(failure);
          } finally {
            setBusy(false);
          }
        }}
      >
        {busy ? "Downloading…" : "Download approved export"}
      </button>
    </article>
  );
}
