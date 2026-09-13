import { invoke } from "@tauri-apps/api/core";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { nativeDesktop, useApi, useResource, type Schema } from "../api";
import { ErrorNotice, Loading, Modal } from "../components/common";
import { BrandMark } from "../app/BrandMark";
import { useSplashHold } from "../app/splash/splash-hold";

type ResetReceipt = Schema<"ResetReceipt">;
type ResetStatus = Schema<"ResetStatus">;
const ResetAccepted = createContext<((receipt: ResetReceipt) => void) | null>(
  null,
);
const desktopRequired =
  "Open the Quantix desktop app on Windows to reset this device. Reset is unavailable in a browser preview or on an unsupported platform.";
const desktopUpdate =
  "Close and reopen Quantix with the updated desktop app, then try again. This desktop app cannot finish a reset.";

async function requireResetSupport() {
  if (!nativeDesktop()) throw new Error(desktopRequired);
  try {
    if ((await invoke<boolean>("reset_support")) !== true)
      throw new Error(desktopRequired);
  } catch (error) {
    throw error instanceof Error && error.message === desktopRequired
      ? error
      : new Error(desktopUpdate);
  }
  return true;
}

/** Keep pending reset recovery outside every component that reads Tender data. */
export function ResetGate({ children }: { children: ReactNode }) {
  const health = useResource<Schema<"Health">>("/health");
  useSplashHold(health.isPending);
  const [receipt, setReceipt] = useState<ResetReceipt | null>(null);
  const [locked, setLocked] = useState(false);
  const accept = useCallback((next: ResetReceipt) => {
    setReceipt(next);
    setLocked(true);
  }, []);
  useEffect(() => {
    if (health.data?.reset_pending) setLocked(true);
  }, [health.data?.reset_pending]);
  if (locked || health.data?.reset_pending)
    return <ResetRecovery receipt={receipt} />;
  if (health.isPending)
    return (
      <div className="connection-screen">
        <Loading>Checking your Tender Office…</Loading>
      </div>
    );
  if (!health.data)
    return (
      <div className="connection-screen">
        <h1>Quantix could not check this workspace</h1>
        <ErrorNotice error={health.error} />
        <button
          className="button primary"
          onClick={() => void health.refetch()}
        >
          Try again
        </button>
      </div>
    );
  return (
    <ResetAccepted.Provider value={accept}>{children}</ResetAccepted.Provider>
  );
}

export function FactoryReset({
  onOpenBackups,
}: {
  onOpenBackups?: () => void;
}) {
  const api = useApi();
  const accept = useContext(ResetAccepted);
  const [localReceipt, setLocalReceipt] = useState<ResetReceipt | null>(null);
  const [open, setOpen] = useState(false);
  const desktop = nativeDesktop();
  const support = useQuery({
    queryKey: ["reset-native-support"],
    queryFn: requireResetSupport,
    enabled: desktop,
    retry: false,
    refetchOnWindowFocus: false,
  });
  const preview = useQuery({
    queryKey: ["/reset/preview"],
    queryFn: ({ signal }) =>
      api.get<Schema<"ResetPreview">>("/reset/preview", signal),
    enabled: desktop && support.isSuccess,
    retry: false,
    refetchOnWindowFocus: false,
  });
  if (localReceipt) return <ResetRecovery receipt={localReceipt} />;
  const available = support.isSuccess && preview.data?.supported;
  return (
    <section className="settings-focused factory-reset">
      <h2>Reset Quantix</h2>
      <p>
        Delete all Quantix data on this device and start again with a fresh
        Tender Office.
      </p>
      <p className="muted">
        This removes Tenders, imported copies, outputs, AI accounts and private
        sign-ins, saved credentials, preferences and drafts, cached software,
        temporary files, logs and backups.
      </p>
      <p className="muted">
        Original files, chosen exports and independent app sign-ins outside the
        Quantix data folder stay untouched. No backup is made automatically.
      </p>
      {onOpenBackups ? (
        <p className="field-help">
          To keep a copy first,{" "}
          <button type="button" className="text-button" onClick={onOpenBackups}>
            open Backups
          </button>{" "}
          and export it outside the Quantix data folder.
        </p>
      ) : null}
      {!desktop ? <p role="status">{desktopRequired}</p> : null}
      {desktop && support.isPending ? (
        <Loading>Checking desktop reset support…</Loading>
      ) : null}
      <ErrorNotice error={support.error || preview.error} />
      {support.error ? (
        <button className="button" onClick={() => void support.refetch()}>
          Check again
        </button>
      ) : null}
      {support.isSuccess && preview.isPending ? (
        <Loading>Checking saved data…</Loading>
      ) : null}
      {preview.error ? (
        <button className="button" onClick={() => void preview.refetch()}>
          Check again
        </button>
      ) : null}
      {preview.data && !preview.data.supported ? (
        <p role="status">{desktopRequired}</p>
      ) : null}
      {available ? (
        <button
          className="button reset-button"
          onClick={async () => {
            const fresh = await preview.refetch();
            if (fresh.data && !fresh.error) setOpen(true);
          }}
          disabled={preview.isFetching}
        >
          Reset Quantix…
        </button>
      ) : null}
      {open && preview.data ? (
        <ResetConfirmation
          preview={preview.data}
          onClose={() => setOpen(false)}
          onAccepted={accept ?? setLocalReceipt}
          onRefresh={async () => {
            const fresh = await preview.refetch();
            if (fresh.error) throw fresh.error;
          }}
        />
      ) : null}
    </section>
  );
}

function ResetConfirmation({
  preview,
  onClose,
  onAccepted,
  onRefresh,
}: {
  preview: Schema<"ResetPreview">;
  onClose: () => void;
  onAccepted: (receipt: ResetReceipt) => void;
  onRefresh: () => Promise<void>;
}) {
  const api = useApi();
  const [confirmation, setConfirmation] = useState("");
  const [pending, setPending] = useState(false);
  const [uncertain, setUncertain] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const submitting = useRef(false);
  const fingerprint = useRef(preview.fingerprint);
  useEffect(() => {
    if (fingerprint.current !== preview.fingerprint) {
      fingerprint.current = preview.fingerprint;
      setConfirmation("");
    }
  }, [preview.fingerprint]);

  async function checkAcceptance() {
    const status = await api.get<ResetStatus | null>("/reset/status");
    if (status) onAccepted(status);
    else setUncertain(false);
    return status;
  }
  async function submit() {
    if (
      submitting.current ||
      confirmation !== "RESET" ||
      preview.blockers.length ||
      !preview.supported
    )
      return;
    submitting.current = true;
    setPending(true);
    setError(null);
    let submitted = false;
    try {
      await requireResetSupport();
      submitted = true;
      setUncertain(true);
      const receipt = await api.post<ResetReceipt>("/reset", {
        fingerprint: preview.fingerprint,
        confirmation: "RESET",
      } satisfies Schema<"ResetRequest">);
      onAccepted(receipt);
    } catch (failure) {
      setError(failure);
      if (submitted) {
        try {
          if (!(await checkAcceptance())) await onRefresh();
        } catch {
          setUncertain(true);
        }
      }
    } finally {
      submitting.current = false;
      setPending(false);
    }
  }
  return (
    <Modal
      title="Reset Quantix?"
      onClose={() => {
        if (!pending && !uncertain) onClose();
      }}
    >
      <div
        className={`reset-confirmation${pending || uncertain ? " reset-confirmation-locked" : ""}`}
      >
        <p>
          All Quantix data in this folder will be deleted. This cannot be
          undone. Quantix will close; open it again to start fresh and repeat AI
          setup.
        </p>
        <dl className="reset-counts">
          <div>
            <dt>Tenders</dt>
            <dd>{preview.tender_count}</dd>
          </div>
          <div>
            <dt>Imported copies</dt>
            <dd>{preview.artifact_count}</dd>
          </div>
          <div>
            <dt>AI accounts</dt>
            <dd>{preview.account_count}</dd>
          </div>
          <div>
            <dt>Backups</dt>
            <dd>{preview.backup_count}</dd>
          </div>
        </dl>
        <div className="reset-home">
          <strong>Quantix data folder</strong>
          <code>{preview.home}</code>
        </div>
        <p className="muted">
          Outputs, private sign-ins, saved credentials, preferences, drafts,
          caches, software, temporary files and logs are also removed. Originals
          and chosen exports outside this folder stay untouched. No automatic
          backup.
        </p>
        {preview.blockers.length ? (
          <div role="alert">
            <p>Finish or stop the active work before resetting.</p>
            <ul>
              {preview.blockers.map((blocker, index) => (
                <li key={index}>{blocker}</li>
              ))}
            </ul>
            <button
              className="button"
              disabled={pending}
              onClick={() => void onRefresh().catch(setError)}
            >
              Check again
            </button>
          </div>
        ) : null}
        <ErrorNotice error={error} />
        {uncertain && !pending ? (
          <>
            <p role="status">
              The reset may already be confirmed. Check its status before
              continuing.
            </p>
            <button
              className="button primary"
              onClick={async () => {
                if (submitting.current) return;
                submitting.current = true;
                setPending(true);
                try {
                  await checkAcceptance();
                } catch (failure) {
                  setError(failure);
                } finally {
                  submitting.current = false;
                  setPending(false);
                }
              }}
            >
              Check reset status
            </button>
          </>
        ) : (
          <form
            onSubmit={(event) => {
              event.preventDefault();
              void submit();
            }}
          >
            <label>
              Type RESET to confirm
              <input
                value={confirmation}
                onChange={(event) => setConfirmation(event.target.value)}
                autoComplete="off"
                spellCheck={false}
                disabled={pending}
              />
            </label>
            <div className="form-actions">
              <button
                type="button"
                className="button"
                onClick={onClose}
                disabled={pending}
              >
                Cancel
              </button>
              <button
                className="button reset-button"
                disabled={
                  pending ||
                  confirmation !== "RESET" ||
                  !!preview.blockers.length ||
                  !preview.supported
                }
              >
                {pending ? "Resetting…" : "Reset and close"}
              </button>
            </div>
          </form>
        )}
      </div>
    </Modal>
  );
}

function clearSavedResetState() {
  try {
    for (const key of Object.keys(window.localStorage)) {
      if (key === "quantix-theme" || key.startsWith("quantix."))
        window.localStorage.removeItem(key);
    }
  } catch {
    throw new Error(
      "Quantix could not clear its saved preferences and drafts. Close other Quantix windows, then retry the reset.",
    );
  }
}

export function ResetRecovery({
  receipt = null,
}: {
  receipt?: ResetReceipt | null;
}) {
  const api = useApi();
  const client = useQueryClient();
  const [status, setStatus] = useState<ResetStatus | null>(null);
  const [pending, setPending] = useState(true);
  const [closing, setClosing] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const started = useRef(false);
  const busy = useRef(false);

  async function prepare() {
    await client.cancelQueries();
    client.clear();
    clearSavedResetState();
  }
  async function readStatus() {
    const next = await api.get<ResetStatus | null>("/reset/status");
    setStatus(next);
    if (!next)
      throw new Error(
        "The reset record could not be found. Close and reopen Quantix to check recovery.",
      );
    return next;
  }
  async function finish(next: ResetStatus) {
    if (
      !next.credentials_cleared ||
      !["ready", "deleting", "failed"].includes(next.state)
    )
      return false;
    await requireResetSupport();
    await invoke("finish_reset", { resetId: next.reset_id });
    setClosing(true);
    return true;
  }
  useEffect(() => {
    if (started.current) return;
    started.current = true;
    void (async () => {
      try {
        await prepare();
        const next = await readStatus();
        if (receipt?.state === "ready" && receipt.reset_id === next.reset_id)
          await finish(next);
      } catch (failure) {
        setError(nativeError(failure));
      } finally {
        setPending(false);
      }
    })();
    // The accepted reset owns this screen until the desktop exits.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  async function retry() {
    if (busy.current) return;
    busy.current = true;
    setPending(true);
    setError(null);
    try {
      await requireResetSupport();
      await prepare();
      let next = await readStatus();
      if (!next.credentials_cleared) {
        await api.post<ResetReceipt>("/reset", {
          fingerprint: next.fingerprint,
          confirmation: "RESET",
        } satisfies Schema<"ResetRequest">);
        next = await readStatus();
      }
      await finish(next);
    } catch (failure) {
      setError(nativeError(failure));
    } finally {
      busy.current = false;
      setPending(false);
    }
  }
  return (
    <main className="reset-recovery">
      <div className="connection-wordmark">
        <BrandMark size={40} />
        Quantix
      </div>
      <h1>Finish resetting Quantix</h1>
      <p>
        The reset is confirmed. Tender work stays closed until all Quantix data
        has been removed.
      </p>
      {status ? <p role="status">{status.detail}</p> : null}
      <ErrorNotice error={error} />
      {closing ? (
        <p role="status">
          Quantix is closing to remove the remaining data. Open it again after
          it closes to start fresh.
        </p>
      ) : pending ? (
        <Loading>Preparing the reset…</Loading>
      ) : nativeDesktop() ? (
        <button className="button reset-button" onClick={() => void retry()}>
          Retry reset and close
        </button>
      ) : (
        <p role="status">{desktopRequired}</p>
      )}
    </main>
  );
}

function nativeError(error: unknown) {
  return typeof error === "string" ? new Error(error) : error;
}
