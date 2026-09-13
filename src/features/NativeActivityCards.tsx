import type { Schema } from "../api";

export function NativeSessionCard({
  session,
  capabilities,
}: {
  session: Schema<"NativeSessionBinding">;
  capabilities: Schema<"NativeClientCapability">[];
}) {
  return (
    <article className="space-y-2 rounded-lg border p-3">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <strong>
          {session.protocol === "codex" ? "Codex client" : "Grok client"} ·{" "}
          {session.model_id}
        </strong>
        <span>{session.state}</span>
      </div>
      <p className="text-xs text-muted-foreground">
        Runtime <code>{session.runtime_revision}</code> · connection revision{" "}
        {session.connection_revision} · profile version{" "}
        {session.profile_version}
      </p>
      <details>
        <summary className="cursor-pointer text-xs">
          Exact session proof
        </summary>
        <dl className="mt-2 grid gap-1 text-xs">
          <NativeProof
            label="Scope fingerprint"
            value={session.scope_fingerprint}
          />
          <NativeProof
            label="Settings fingerprint"
            value={session.settings_fingerprint}
          />
          {session.provider_session_id ? (
            <NativeProof
              label="Provider session"
              value={session.provider_session_id}
            />
          ) : null}
        </dl>
      </details>
      {capabilities.length ? (
        <ul className="space-y-1 text-xs">
          {capabilities.map((item) => (
            <li key={item.id}>
              <strong>{item.supported ? "Available" : "Unavailable"}</strong> ·{" "}
              {item.detail}{" "}
              <span className="text-muted-foreground">({item.source})</span>
            </li>
          ))}
        </ul>
      ) : null}
    </article>
  );
}

export function NativeArtifactCard({
  artifact,
  downloading,
  onDownload,
}: {
  artifact: Schema<"NativeArtifact">;
  downloading: boolean;
  onDownload: () => void;
}) {
  return (
    <article className="space-y-2 rounded-lg border p-3">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <strong>{artifact.filename}</strong>
        <span>
          {artifact.status === "unreviewed"
            ? "Unreviewed provider draft"
            : artifact.status}
        </span>
      </div>
      <p className="text-xs text-muted-foreground">
        {formatBytes(artifact.size_bytes)} ·{" "}
        {artifact.model_id ?? "Model not recorded"}
      </p>
      <NativeProof label="SHA-256" value={artifact.content_hash} />
      {artifact.input_artifacts?.length ? (
        <details>
          <summary className="cursor-pointer text-xs">
            Exact input files
          </summary>
          <ul className="mt-1 space-y-1 text-xs">
            {artifact.input_artifacts.map((item) => (
              <li key={`${item.artifact_id}:${item.version}`}>
                {item.artifact_id} · version {item.version} ·{" "}
                <code className="wrap-anywhere">{item.content_hash}</code>
              </li>
            ))}
          </ul>
        </details>
      ) : null}
      <button
        type="button"
        className="button"
        disabled={downloading}
        onClick={onDownload}
      >
        {downloading ? "Downloading…" : `Download ${artifact.filename}`}
      </button>
    </article>
  );
}

export function NativeProof({
  label,
  value,
}: {
  label: string;
  value: string;
}) {
  return (
    <div className="min-w-0 text-xs">
      <dt className="text-muted-foreground">{label}</dt>
      <dd>
        <code className="wrap-anywhere">{value}</code>
      </dd>
    </div>
  );
}

function formatBytes(bytes: number) {
  return bytes < 1024
    ? `${bytes} bytes`
    : `${(bytes / 1024).toLocaleString(undefined, { maximumFractionDigits: 1 })} KiB`;
}
