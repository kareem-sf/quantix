import { useState } from "react";
import { useApi, useRefresh, useResource } from "../api";
import { ErrorNotice, Loading } from "../components/common";

// Replaced with generated Schema<"SandboxStatus"> when the root router is wired.
type RuntimeStatus = {
  state:
    | "not_installed"
    | "prerequisite_required"
    | "setup_required"
    | "stopped"
    | "ready"
    | "needs_repair"
    | "busy";
  detail: string;
  next_action: string;
  composition_available: boolean;
  python_available: boolean;
  active_runs: number;
  image_id: string | null;
  library_versions: Record<string, string>;
  limits: {
    cpus: number;
    memory_mib: number;
    seconds: number;
    processes: number;
    output_mib: number;
  };
};
type RuntimeAction = "setup" | "repair" | "remove" | "start" | "stop";

export function SandboxSettings() {
  const status = useResource<RuntimeStatus>("/sandbox");
  const api = useApi();
  const refresh = useRefresh();
  const [pending, setPending] = useState<RuntimeAction | null>(null);
  const [error, setError] = useState<unknown>(null);
  const [confirmRemove, setConfirmRemove] = useState(false);

  async function act(action: RuntimeAction) {
    if (pending) return;
    setPending(action);
    setError(null);
    try {
      await api.post<RuntimeStatus>("/sandbox/actions", { action });
      setConfirmRemove(false);
    } catch (cause) {
      setError(cause);
    } finally {
      await refresh();
      setPending(null);
    }
  }

  if (status.isPending) return <Loading>Checking local Python…</Loading>;
  if (!status.data) return <ErrorNotice error={status.error} />;
  const data = status.data;
  const busy = !!pending || data.state === "busy" || data.active_runs > 0;
  const next: RuntimeAction =
    data.state === "stopped"
      ? "start"
      : data.state === "needs_repair"
        ? "repair"
        : "setup";

  return (
    <section className="stack" aria-labelledby="python-settings-title">
      <div>
        <h2 id="python-settings-title">Local calculations and Python</h2>
        <p>
          Run reviewed methods on selected Tender files. Original files stay
          unchanged.
        </p>
      </div>
      <p role="status">
        {pending
          ? "Preparing the isolated runtime. This can take several minutes."
          : data.detail}
      </p>
      {error != null && <ErrorNotice error={error} />}
      {data.state !== "ready" && (
        <div>
          <button type="button" disabled={busy} onClick={() => void act(next)}>
            {data.state === "prerequisite_required"
              ? "Check Windows setup again"
              : next === "start"
                ? "Start local Python"
                : next === "repair"
                  ? "Repair local Python"
                  : "Set up local Python"}
          </button>
          <p>{data.next_action}</p>
          {data.state === "prerequisite_required" && (
            <p>
              Open Windows Features and enable Windows Subsystem for Linux and
              Virtual Machine Platform. Complete the WSL 2 installation and any
              requested restart, then return here.
            </p>
          )}
        </div>
      )}
      <p>
        Tool composition:{" "}
        {data.composition_available ? "available" : "runtime setup required"}.
        Python libraries:{" "}
        {data.python_available ? "available" : "setup required"}.
      </p>
      <details>
        <summary>More options</summary>
        <p>
          Python runs without network access or host-folder mounts. Only
          selected input copies enter its disposable container. Generated files
          and execution receipts remain in Quantix.
        </p>
        <p>
          Default limits: {data.limits.seconds} seconds, {data.limits.cpus}{" "}
          processors, {data.limits.memory_mib} MiB memory,{" "}
          {data.limits.processes} processes and {data.limits.output_mib} MiB
          output. Reviewed methods can request limits within the supported
          bounds.
        </p>
        {data.image_id && (
          <p>
            Image:{" "}
            <code style={{ overflowWrap: "anywhere" }}>{data.image_id}</code>
          </p>
        )}
        {Object.keys(data.library_versions).length > 0 && (
          <ul>
            {Object.entries(data.library_versions).map(([name, version]) => (
              <li key={name}>
                {name} {version}
              </li>
            ))}
          </ul>
        )}
        <div className="actions">
          <button
            type="button"
            disabled={busy}
            onClick={() => void act("repair")}
          >
            Repair runtime
          </button>
          {data.state === "ready" && (
            <button
              type="button"
              disabled={busy}
              onClick={() => void act("stop")}
            >
              Stop runtime
            </button>
          )}
          <button
            type="button"
            disabled={busy || data.state === "not_installed"}
            onClick={() => setConfirmRemove(true)}
          >
            Remove runtime
          </button>
        </div>
        {confirmRemove && (
          <div role="group" aria-label="Remove local Python">
            <p>
              This removes Quantix’s private Python machine and libraries. Saved
              Tender files, outputs and execution receipts remain available.
            </p>
            <button
              type="button"
              disabled={busy}
              onClick={() => void act("remove")}
            >
              Remove private machine
            </button>
            <button
              type="button"
              disabled={busy}
              onClick={() => setConfirmRemove(false)}
            >
              Keep runtime
            </button>
          </div>
        )}
      </details>
    </section>
  );
}
