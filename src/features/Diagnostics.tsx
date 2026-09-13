import { useEffect } from "react";
import { useResource, type Schema } from "../api";
import { ErrorNotice, Loading } from "../components/common";

export function Diagnostics({ open = false }: { open?: boolean } = {}) {
  const status = useResource<Schema<"DiagnosticsStatus">>("/diagnostics", open);
  useEffect(() => {
    if (open) void status.refetch();
  }, [open, status.refetch]);
  if (status.isPending) return <Loading>Loading technical details…</Loading>;
  if (!status.data) return <ErrorNotice error={status.error} />;
  const value = status.data;
  return (
    <div className="diagnostics-details">
      <div
        className={`diagnostics-status ${value.available ? "available" : "unavailable"}`}
        role="status"
      >
        <strong>{value.available ? "Recording is on" : "Recording is unavailable"}</strong>
        <p>{value.detail}</p>
      </div>
      <label>
        Diagnostics folder
        <input readOnly value={value.directory} />
      </label>
      <dl className="diagnostics-facts">
        <div>
          <dt>Session</dt>
          <dd>{value.session_id}</dd>
        </div>
        <div>
          <dt>Retention</dt>
          <dd>{value.retention_days} days</dd>
        </div>
        <div>
          <dt>File limit</dt>
          <dd>{formatBytes(value.max_file_bytes)}</dd>
        </div>
        <div>
          <dt>Retention target</dt>
          <dd>{formatBytes(value.max_total_bytes)}</dd>
        </div>
        <div>
          <dt>Rotated files per process</dt>
          <dd>{value.backup_count}</dd>
        </div>
      </dl>
      <p className="field-help">
        The retention target is approximate while active log files remain
        open.
      </p>
      <p className="field-help">
        Quantix records technical events here without raw Tender content. If
        you ask for support, share the request reference shown in the error and
        this diagnostics folder.
      </p>
      <button
        type="button"
        className="text-button"
        onClick={() => void status.refetch()}
      >
        Refresh recording status
      </button>
      {status.error ? <ErrorNotice error={status.error} /> : null}
    </div>
  );
}

function formatBytes(value: number) {
  if (value < 1024 * 1024) return `${Math.round(value / 1024)} KB`;
  return `${(value / (1024 * 1024)).toFixed(1)} MB`;
}
