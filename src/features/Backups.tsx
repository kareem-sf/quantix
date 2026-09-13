import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import {
  choosePackage,
  nativeDesktop,
  useApi,
  useRefresh,
  useResource,
  type Schema,
} from "../api";
import { ErrorNotice, Loading } from "../components/common";
import { FieldError } from "../components/FieldError";
import { createDraftScope, useFormDraft } from "./useFormDraft";

const restorationKey = ["prepared-restoration"];
type PreparedRestoration = {
  path: string;
  serviceStopped: boolean;
};

export function Backups({
  onAttention,
}: {
  onAttention?: (attention: boolean) => void;
} = {}) {
  const api = useApi(),
    refresh = useRefresh(),
    client = useQueryClient();
  const backups = useResource<Schema<"BackupRecord">[]>("/backups");
  const pendingRestore = useResource<Schema<"RestoreReady"> | null>(
    "/backups/pending",
  );
  const latestRestore = useResource<Schema<"RestoreOutcome"> | null>(
    "/backups/latest",
  );
  const prepared = useQuery<PreparedRestoration | null>({
    queryKey: restorationKey,
    queryFn: () => null,
    initialData: null,
    enabled: false,
    gcTime: Infinity,
  });
  const ready = pendingRestore.data;
  const [path, setPath] = useState(prepared.data?.path ?? ""),
    [checked, setChecked] = useState<{
      path: string;
      result: Schema<"BackupInspection">;
    } | null>(null),
    [busy, setBusy] = useState(false),
    [error, setError] = useState<unknown>(null),
    [notice, setNotice] = useState("");
  const draft = useFormDraft(
    createDraftScope(
      "settings",
      "office",
      "restore-note",
      checked?.result.backup.sha256 ?? "unselected",
    ),
    { rationale: "" },
    ["rationale"],
  );
  const { rationale } = draft.value;
  const setRationale = (value: string) => draft.setField("rationale", value);
  const locked =
    busy || pendingRestore.isPending || !!pendingRestore.error || !!ready;
  const attention =
    !!error || !!pendingRestore.error || !!latestRestore.error || !!ready;
  useEffect(() => {
    onAttention?.(attention);
  }, [attention, onAttention]);
  async function act(action: () => Promise<void>) {
    setBusy(true);
    setError(null);
    setNotice("");
    try {
      await action();
    } catch (failure) {
      setError(failure);
    } finally {
      setBusy(false);
    }
  }
  function selectPath(value: string) {
    if (ready) return;
    setPath(value);
    setChecked(null);
    setRationale("");
    setError(null);
  }
  async function download(backup: Schema<"BackupRecord">) {
    const blob = await api.blob("/backups/" + backup.id + "/download"),
      url = URL.createObjectURL(blob),
      link = document.createElement("a");
    link.href = url;
    link.download = backup.filename;
    document.body.append(link);
    link.click();
    link.remove();
    window.setTimeout(() => URL.revokeObjectURL(url), 1000);
  }
  return (
    <section className="settings-backups">
      <h2>Backups and recovery</h2>
      <p className="muted">
        Save your tender records and original documents together. Connection
        keys are not included in backups.
      </p>
      <ErrorNotice error={latestRestore.error} />
      {latestRestore.isPending ? (
        <Loading>Checking restoration history…</Loading>
      ) : latestRestore.isError ? (
        <button
          className="text-button"
          type="button"
          onClick={() => void latestRestore.refetch()}
        >
          Check restoration history again
        </button>
      ) : null}
      {latestRestore.data ? (
        <div className="restoration-result">
          <h3>Last restoration</h3>
          <p>{latestRestore.data.detail}</p>
          <p className="field-help">
            Completed{" "}
            {new Date(latestRestore.data.completed_at).toLocaleString()}
          </p>
          {latestRestore.data.recovery_evidence_path ? (
            <p className="field-help">
              Recovery evidence in your data folder:{" "}
              <code>{latestRestore.data.recovery_evidence_path}</code>
            </p>
          ) : null}
        </div>
      ) : null}
      <button
        type="button"
        className="button"
        disabled={locked}
        onClick={() =>
          void act(async () => {
            await api.post<Schema<"BackupRecord">>("/backups");
            await refresh();
            setNotice("Backup created. You can save a copy below.");
          })
        }
      >
        {busy ? "Working…" : "Create backup"}
      </button>
      <ErrorNotice error={error || backups.error || pendingRestore.error} />
      {pendingRestore.isPending ? (
        <Loading>Checking restoration status…</Loading>
      ) : pendingRestore.isError ? (
        <button
          className="text-button"
          type="button"
          onClick={() => void pendingRestore.refetch()}
        >
          Check restoration status again
        </button>
      ) : null}
      {notice ? (
        <p role="status" className="success-text">
          {notice}
        </p>
      ) : null}
      {backups.isPending ? (
        <Loading>Loading backups…</Loading>
      ) : backups.data?.length ? (
        <ul className="backup-list">
          {backups.data.map((backup) => (
            <li key={backup.id}>
              <div>
                <strong>{new Date(backup.created_at).toLocaleString()}</strong>
                <p>
                  {backup.tender_count}{" "}
                  {backup.tender_count === 1 ? "tender" : "tenders"} ·{" "}
                  {backup.original_count} originals
                  {backup.purpose === "before_restore"
                    ? " · Safety backup"
                    : ""}
                </p>
                <MissingOriginals count={backup.unavailable_original_count} />
              </div>
              <button
                type="button"
                className="text-button"
                aria-label={"Save a copy of " + backup.filename}
                disabled={busy}
                onClick={() => void act(() => download(backup))}
              >
                Save a copy
              </button>
            </li>
          ))}
        </ul>
      ) : backups.data ? (
        <p className="field-help">No backups have been created yet.</p>
      ) : null}
      <details className="restore-controls" open={!!ready || undefined}>
        <summary>Restore a saved backup</summary>
        <p className="muted">
          The checked backup will replace this workspace when Quantix next
          starts. The current workspace will first be kept in a checked backup
          where possible. Damaged files are retained with a recovery record.
        </p>
        <label>
          Backup file
          <input
            disabled={locked}
            value={path}
            onChange={(event) => selectPath(event.target.value)}
            placeholder="Path to a Quantix backup ZIP"
          />
        </label>
        <div className="inline-actions">
          {nativeDesktop() ? (
            <button
              type="button"
              className="button"
              disabled={locked}
              onClick={() =>
                void act(async () => {
                  const selected = await choosePackage("zip");
                  if (selected) selectPath(selected);
                })
              }
            >
              Choose backup
            </button>
          ) : null}
          <button
            type="button"
            className="button"
            disabled={locked || !path.trim()}
            onClick={() =>
              void act(async () => {
                const selected = path.trim();
                setChecked(null);
                const result = await api.post<Schema<"BackupInspection">>(
                  "/backups/inspect",
                  { path: selected } satisfies Schema<"InspectBackupRequest">,
                );
                setChecked({ path: selected, result });
              })
            }
          >
            Check backup
          </button>
        </div>
        {checked && !ready ? (
          <div className="restore-preview">
            <p>{checked.result.detail}</p>
            <p>
              {checked.result.backup.tender_count} tenders ·{" "}
              {checked.result.backup.original_count} saved originals ·{" "}
              {new Date(checked.result.backup.created_at).toLocaleString()}
            </p>
            <MissingOriginals
              count={checked.result.backup.unavailable_original_count}
            />
            <label>
              Reason for restoring
              <textarea
                disabled={busy}
                maxLength={4000}
                value={rationale}
                onChange={(event) => setRationale(event.target.value)}
                rows={2}
              />
              <FieldError error={error} name="rationale" />
            </label>
            <button
              type="button"
              className="button"
              disabled={busy || !rationale.trim()}
              onClick={() =>
                void act(async () => {
                  const acceptedRevision = draft.revision;
                  const result = await api.post<Schema<"RestoreReady">>(
                    "/backups/restore",
                    {
                      path: checked.path,
                      expected_sha256: checked.result.backup.sha256,
                      engineer_confirmed: true,
                      rationale: rationale.trim(),
                    } satisfies Schema<"RestoreBackupRequest">,
                  );
                  draft.markAccepted(acceptedRevision);
                  client.setQueryData(["/backups/pending"], result);
                  client.setQueryData<PreparedRestoration>(restorationKey, {
                    path: checked.path,
                    serviceStopped: false,
                  });
                })
              }
            >
              Prepare this restoration
            </button>
          </div>
        ) : null}
        {ready ? (
          <div role="status" className="restore-ready">
            <p>{ready.detail}</p>
            <p>
              Close Quantix, then open Start-Quantix again to restore the
              checked backup.
            </p>
            <button
              type="button"
              className="button"
              disabled={busy}
              onClick={() =>
                void act(async () => {
                  if (!prepared.data?.serviceStopped) {
                    await api.post("/shutdown");
                    client.setQueryData<PreparedRestoration>(restorationKey, {
                      path: prepared.data?.path ?? path,
                      serviceStopped: true,
                    });
                  }
                  if (nativeDesktop()) await invoke("close_quantix");
                  else
                    setNotice(
                      "The local service has stopped. Open Start-Quantix to restore the checked backup.",
                    );
                })
              }
            >
              Close Quantix for restoration
            </button>
          </div>
        ) : null}
      </details>
    </section>
  );
}

function MissingOriginals({ count }: { count: number }) {
  return count > 0 ? (
    <p className="warning-text">
      {count} original {count === 1 ? "file was" : "files were"} unavailable
      when this backup was created.
    </p>
  ) : null;
}
