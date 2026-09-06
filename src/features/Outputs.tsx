import { useCallback, useState } from "react";
import { Download, FileText } from "lucide-react";
import { tenderPath, useApi, useResource, type Schema } from "../api";
import { ErrorNotice, Loading, Status } from "../components/ui";
import { OutputForm } from "./OutputForm";
import { Submissions } from "./Submissions";
import { SubmissionRequirements } from "./SubmissionRequirements";

export function Outputs({ tenderId }: { tenderId: string }) {
  const api = useApi(),
    base = tenderPath(tenderId);
  const outputs = useResource<Schema<"OutputRecord">[]>(`${base}/outputs`);
  const [kind, setKind] = useState<Schema<"OutputRequest">["kind"] | null>(
      null,
    ),
    [error, setError] = useState<unknown>(null),
    [downloading, setDownloading] = useState<string | null>(null);
  const close = useCallback(() => setKind(null), []);
  const [otherKind, setOtherKind] =
    useState<Schema<"OutputRequest">["kind"]>("technical_docx");
  async function download(output: Schema<"OutputRecord">) {
    setDownloading(output.id);
    setError(null);
    try {
      const blob = await api.blob(`${base}/outputs/${output.id}/download`);
      const url = URL.createObjectURL(blob);
      const link = document.createElement("a");
      link.href = url;
      link.download = output.filename;
      document.body.append(link);
      link.click();
      link.remove();
      window.setTimeout(() => URL.revokeObjectURL(url), 1000);
    } catch (failure) {
      setError(failure);
    } finally {
      setDownloading(null);
    }
  }
  return (
    <>
      <section className="outputs-section">
        <div className="section-heading">
          <div>
            <h2>Draft documents</h2>
            <p className="muted">
              Create a review copy from the current tender records.
            </p>
          </div>
          <div className="inline-actions">
            <button className="button" onClick={() => setKind("boq_xlsx")}>
              Create BOQ workbook
            </button>
            <button className="button" onClick={() => setKind("analysis_docx")}>
              Create analysis document
            </button>
          </div>
        </div>
        <div className="other-outputs">
          <label>
            Other document type
            <select
              value={otherKind}
              onChange={(event) =>
                setOtherKind(
                  event.target.value as Schema<"OutputRequest">["kind"],
                )
              }
            >
              <option value="technical_docx">Technical document</option>
              <option value="registers_xlsx">Tender registers</option>
              <option value="comparison_xlsx">Quotation comparison</option>
              <option value="programme_xlsx">Construction programme</option>
              <option value="client_boq">Client BOQ copy</option>
            </select>
          </label>
          <button
            type="button"
            className="button"
            onClick={() => setKind(otherKind)}
          >
            Create selected document
          </button>
        </div>
        <ErrorNotice error={outputs.error || error} />
        {outputs.isPending ? <Loading>Loading draft documents…</Loading> : null}
        {outputs.data?.length === 0 ? (
          <p className="muted output-empty">
            No draft documents have been created.
          </p>
        ) : null}
        {outputs.data?.map((output) => (
          <article className="document-row" key={output.id}>
            <FileText size={26} />
            <div className="document-content">
              <strong>{output.filename}</strong>
              <div className="file-meta">
                <Status value={output.status} />
                <span>{new Date(output.created_at).toLocaleString()}</span>
              </div>
              {output.blocking_reasons.length ? (
                <details className="document-warnings">
                  <summary>Items requiring review</summary>
                  {output.blocking_reasons.map((reason) => (
                    <p key={reason}>{reason}</p>
                  ))}
                </details>
              ) : null}
            </div>
            <button
              className="button"
              disabled={downloading === output.id}
              onClick={() => void download(output)}
            >
              <Download size={17} />
              {downloading === output.id ? "Downloading…" : "Download"}
            </button>
          </article>
        ))}
        {kind ? (
          <OutputForm tenderId={tenderId} kind={kind} onClose={close} />
        ) : null}
      </section>
      <SubmissionRequirements tenderId={tenderId} />
      <Submissions tenderId={tenderId} outputs={outputs.data ?? []} />
    </>
  );
}
