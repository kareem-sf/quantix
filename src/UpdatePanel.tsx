import { useCallback, useEffect, useState } from "react";

import type { UpdateDiagnostic } from "./bindings/UpdateDiagnostic";
import type { UpdateState } from "./bindings/UpdateState";
import type { UpdateStatus } from "./bindings/UpdateStatus";
import {
  checkQuantixUpdate,
  decideQuantixUpdate,
  installQuantixUpdate,
  restartQuantixAfterUpdate,
  retryQuantixUpdateRepair,
  validateQuantixUpdateRestart,
} from "./quantixHost";

type Props = {
  onWorkAvailabilityChange: (available: boolean) => void;
  onTerminalState?: () => void;
};

const stateCopy: Record<UpdateState, string> = {
  idle: "No update has been checked",
  awaiting_approval: "Engineer decision required",
  approved: "Approved and waiting for safe installation",
  denied: "Update denied by the Engineer",
  installing: "Installing the approved signed update",
  restart_validation_required: "Restart required to validate the installation",
  ready: "Updated installation validated",
  rejected: "Unsafe update rejected",
  repair_required: "Update repair required",
  rolled_back: "Prior installation validated after rollback",
};

const diagnosticCopy: Record<UpdateDiagnostic, string> = {
  invalid_manifest: "The update manifest is invalid.",
  downgrade_rejected: "Downgrades are not permitted.",
  unsupported_platform: "The artifact does not support this platform.",
  installation_schema_incompatible:
    "The Application Home schema is incompatible.",
  tender_store_incompatible: "The Tender Store schema is incompatible.",
  codex_incompatible: "The bundled Codex runtime is incompatible.",
  ocr_incompatible: "The locked OCR runtime is incompatible.",
  runtime_incompatible: "The locked runtime manifest is incompatible.",
  approval_required: "The exact update has not been approved.",
  active_work: "Active Quantix work must become quiescent first.",
  verified_backup_required:
    "Create a current verified backup for every Tender before installing.",
  unsigned_artifact: "The artifact is unsigned.",
  wrong_signing_key: "The artifact was not signed by the Quantix release key.",
  artifact_tampered: "The downloaded artifact identity does not match.",
  download_failed: "The signed artifact could not be downloaded.",
  installation_failed:
    "Installation failed. Use Update repair to restore the authenticated prior application.",
  installation_interrupted:
    "Installation was interrupted; the prior application remains recoverable.",
  restart_validation_failed:
    "Application Home, runtime, schema, or Tender validation failed after restart.",
  updater_configuration_missing:
    "This build has no valid Quantix release key or HTTPS update endpoint. Installation is disabled.",
  updater_unavailable: "The signed update source is unavailable.",
};

function permitsTenderWork(state: UpdateState): boolean {
  return ![
    "installing",
    "restart_validation_required",
    "repair_required",
  ].includes(state);
}

function diagnosticFrom(error: unknown): UpdateDiagnostic | null {
  if (
    typeof error === "object" &&
    error !== null &&
    "diagnostic" in error &&
    typeof error.diagnostic === "string" &&
    error.diagnostic in diagnosticCopy
  ) {
    return error.diagnostic as UpdateDiagnostic;
  }
  return null;
}

export function UpdatePanel({
  onWorkAvailabilityChange,
  onTerminalState,
}: Props) {
  const [status, setStatus] = useState<UpdateStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<UpdateDiagnostic | null>(null);
  const [rationale, setRationale] = useState("");

  const acceptStatus = useCallback(
    (next: UpdateStatus) => {
      setStatus(next);
      setError(next.diagnostic);
      onWorkAvailabilityChange(permitsTenderWork(next.state));
      if (next.state === "ready" || next.state === "rolled_back") {
        onTerminalState?.();
      }
    },
    [onTerminalState, onWorkAvailabilityChange],
  );

  useEffect(() => {
    let active = true;
    setBusy(true);
    void validateQuantixUpdateRestart()
      .then((next) => {
        if (active) acceptStatus(next);
      })
      .catch((reason: unknown) => {
        if (active) {
          setError(diagnosticFrom(reason) ?? "updater_unavailable");
          onWorkAvailabilityChange(false);
        }
      })
      .finally(() => {
        if (active) setBusy(false);
      });
    return () => {
      active = false;
    };
  }, [acceptStatus, onWorkAvailabilityChange]);

  const run = async (operation: () => Promise<UpdateStatus>) => {
    setBusy(true);
    setError(null);
    try {
      acceptStatus(await operation());
    } catch (reason) {
      setError(diagnosticFrom(reason) ?? "updater_unavailable");
    } finally {
      setBusy(false);
    }
  };

  const offer = status?.offer;

  return (
    <section className="update-office" aria-labelledby="update-title">
      <div className="runtime-office__heading">
        <div>
          <p className="eyebrow">Signed application updates</p>
          <h2 id="update-title">Host-controlled update</h2>
          <p>
            Quantix presents exact release evidence and installs nothing until
            the Engineer approves the signed artifact.
          </p>
        </div>
        <button
          className="connection-button"
          type="button"
          disabled={
            busy ||
            status?.state === "installing" ||
            status?.state === "restart_validation_required" ||
            status?.state === "repair_required"
          }
          onClick={() => void run(checkQuantixUpdate)}
        >
          {busy ? "Checking..." : "Check for update"}
        </button>
      </div>

      <div className="status-panel">
        <div className="status-heading" aria-live="polite">
          <span
            className={`status-indicator status-indicator--${status?.state ?? "checking"}`}
            aria-hidden="true"
          />
          <div>
            <p className="status-kicker">Update status</p>
            <h3>
              {status ? stateCopy[status.state] : "Validating update state"}
            </h3>
          </div>
        </div>

        {offer ? (
          <dl className="update-evidence">
            <div>
              <dt>Version</dt>
              <dd>
                {offer.current_version} to {offer.version}
              </dd>
            </div>
            <div>
              <dt>Signed artifact SHA-256</dt>
              <dd className="mono-value">{offer.artifact.sha256}</dd>
            </div>
            <div>
              <dt>Signature identity</dt>
              <dd className="mono-value">{offer.artifact.signature_sha256}</dd>
            </div>
            <div>
              <dt>Compatibility</dt>
              <dd>
                Home schema {offer.compatibility.installation_schema_version};
                Tender schema {offer.compatibility.tender_schema_version}; Codex{" "}
                {offer.compatibility.codex_version}; OCR{" "}
                {offer.compatibility.ocr_version}; runtime manifest schema{" "}
                {offer.compatibility.runtime_manifest_schema_version}
              </dd>
            </div>
            <div>
              <dt>{offer.release.title}</dt>
              <dd>
                {offer.release.notes} Published {offer.release.published_at}.
              </dd>
            </div>
            <div>
              <dt>Impact</dt>
              <dd>
                {offer.impact.summary}
                {offer.impact.stored_data_may_change
                  ? " Current verified Tender backups are mandatory."
                  : " No stored-data change is declared."}
              </dd>
            </div>
            {status?.decision_history.map((decision) => (
              <div key={decision.current_hash}>
                <dt>Engineer decision #{decision.sequence}</dt>
                <dd>
                  {decision.decision} by {decision.decided_by} acting as{" "}
                  {decision.acting_role} at {decision.decided_at}. Rationale:{" "}
                  {decision.rationale}. Previous record SHA-256{" "}
                  {decision.preceding_hash}; record SHA-256{" "}
                  {decision.current_hash}.
                </dd>
              </div>
            ))}
          </dl>
        ) : null}

        {error ? (
          <p className="error-message" role="alert">
            {diagnosticCopy[error]}
          </p>
        ) : null}

        {offer && status?.state === "awaiting_approval" ? (
          <div className="update-actions">
            <label>
              Decision rationale
              <textarea
                value={rationale}
                maxLength={4000}
                disabled={busy}
                onChange={(event) => setRationale(event.target.value)}
                placeholder="Record why this exact signed release should be installed or denied."
              />
            </label>
            <button
              type="button"
              disabled={busy || rationale.trim().length === 0}
              onClick={() =>
                void run(() =>
                  decideQuantixUpdate(offer.update_id, "approve", rationale),
                )
              }
            >
              Approve exact update
            </button>
            <button
              type="button"
              disabled={busy || rationale.trim().length === 0}
              onClick={() =>
                void run(() =>
                  decideQuantixUpdate(offer.update_id, "deny", rationale),
                )
              }
            >
              Deny
            </button>
          </div>
        ) : null}

        {offer && status?.state === "approved" ? (
          <button
            className="connection-button"
            type="button"
            disabled={busy}
            onClick={() =>
              void run(() => installQuantixUpdate(offer.update_id))
            }
          >
            Install approved signed update
          </button>
        ) : null}

        {offer && status?.state === "restart_validation_required" ? (
          <div className="update-actions">
            <p>
              Restart Quantix now. Tender work remains blocked until the Host
              revalidates the Application Home, runtimes, schemas, and every
              Tender Store in the new process.
            </p>
            <button
              className="connection-button"
              type="button"
              disabled={busy}
              onClick={() =>
                void run(() => restartQuantixAfterUpdate(offer.update_id))
              }
            >
              Restart Quantix and validate
            </button>
          </div>
        ) : null}

        {offer && status?.state === "repair_required" ? (
          <div className="update-actions">
            <p role="alert">
              Quantix cannot resume Tender work until the authenticated prior
              application is restored and validated. Retry recovery; if the
              helper cannot be scheduled, this state remains blocked and the
              diagnostic stays visible.
            </p>
            <button
              className="connection-button"
              type="button"
              disabled={busy}
              onClick={() =>
                void run(() => retryQuantixUpdateRepair(offer.update_id))
              }
            >
              Retry recovery and restart prior version
            </button>
          </div>
        ) : null}
      </div>
    </section>
  );
}
