import { useCallback, useEffect, useId, useRef, useState } from "react";

import type { TenderBackupRecord } from "./bindings/TenderBackupRecord";
import type { TenderIntegrityIssue } from "./bindings/TenderIntegrityIssue";
import type { TenderIntegrityReport } from "./bindings/TenderIntegrityReport";
import type { TenderRecoveryDecision } from "./bindings/TenderRecoveryDecision";
import type { TenderRecoveryRecord } from "./bindings/TenderRecoveryRecord";
import {
  inspectTenderBackups,
  inspectTenderIntegrity,
  inspectTenderRecoveries,
  prepareTenderRecovery,
  resolveTenderRecovery,
} from "./quantixHost";
import { QuantixDialog } from "./ui/QuantixDialog";
import "./WorkspaceRecoveryCenter.css";

interface WorkspaceRecoveryCenterProps {
  tenderId: string;
  tenderName: string;
  variant?: "recovery" | "backups";
  onClose: () => void;
  onRecovered: () => void | Promise<void>;
  onMoveToTrash: (rationale: string) => void | Promise<void>;
  onDeletePermanently: (
    rationale: string,
    confirmationTenderName: string,
  ) => void | Promise<void>;
  requestedAction?: "move_to_trash" | "delete_permanently" | null;
  onRequestedActionHandled?: () => void;
}

interface RecoverySnapshot {
  report: TenderIntegrityReport;
  backups: TenderBackupRecord[];
  recoveries: TenderRecoveryRecord[];
}

const issueCopy: Record<
  TenderIntegrityIssue,
  { cause: string; capability: string; impact: string; remediation: string }
> = {
  audit_chain_invalid: {
    cause: "The immutable audit chain no longer matches its recorded history.",
    capability: "Audit history and attribution",
    impact: "Quantix cannot safely accept new work or alter this Tender.",
    remediation:
      "Inspect a verified recovery candidate before approving any replacement.",
  },
  database_integrity_invalid: {
    cause:
      "The SQLite database failed page or relational integrity verification.",
    capability: "Tender records and workspace state",
    impact:
      "Reading or writing records could produce incomplete or misleading results.",
    remediation:
      "Use a verified backup candidate and review its diagnostic evidence.",
  },
  inspection_unavailable: {
    cause:
      "Required storage was unavailable while Quantix was inspecting the Tender.",
    capability: "Integrity inspection",
    impact:
      "The current health state cannot be trusted until inspection completes.",
    remediation:
      "Check local storage availability, then run the inspection again.",
  },
  referenced_content_missing: {
    cause:
      "Canonical content referenced by this Tender is missing from local storage.",
    capability: "Source files and Evidence",
    impact: "Some records may no longer have their required source material.",
    remediation: "Review verified backups for a complete content set.",
  },
  referenced_content_mismatch: {
    cause: "Canonical content no longer matches its recorded digest or size.",
    capability: "Evidence provenance",
    impact:
      "Quantix cannot prove that affected content is the content originally recorded.",
    remediation:
      "Compare a verified recovery candidate before Engineer approval.",
  },
  manifest_invalid: {
    cause:
      "A canonical task, permission, Evidence, or Agent Run manifest failed semantic verification.",
    capability: "Governed Agent work and permissions",
    impact: "The affected work history cannot be safely resumed.",
    remediation:
      "Inspect the available candidate diagnostics and approve only a verified replacement.",
  },
  schema_mismatch: {
    cause: "The Tender Store structure or identity failed verification.",
    capability: "Tender Store compatibility",
    impact: "Quantix cannot safely interpret the stored records.",
    remediation:
      "Use a candidate created from a compatible, verified Tender Store.",
  },
  storage_layout_invalid: {
    cause:
      "The Tender Store contains an unsafe link, unexpected entry, or invalid storage layout.",
    capability: "Local Tender storage",
    impact:
      "Opening the Tender could expose records outside its governed store.",
    remediation:
      "Repair the storage layout or review a verified backup with the Engineer.",
  },
  tender_identity_mismatch: {
    cause:
      "The immutable Tender identity does not match the directory selected for recovery.",
    capability: "Tender identity and isolation",
    impact: "Quantix cannot prove that the records belong to this Tender.",
    remediation:
      "Reject mismatched candidates and inspect a candidate with the exact Tender identity.",
  },
};

const backupStateCopy: Record<TenderBackupRecord["state"], string> = {
  creating: "Creating and verifying",
  ready: "Verified and ready",
  failed: "Verification failed",
};

const recoveryStateCopy: Record<TenderRecoveryRecord["state"], string> = {
  preparing: "Verifying candidate",
  awaiting_approval: "Awaiting Engineer approval",
  applying: "Applying approved replacement",
  applied: "Applied",
  rejected: "Rejected",
  failed: "Recovery failed",
};

function errorMessage(error: unknown): string {
  if (error instanceof Error && error.message) return error.message;
  if (typeof error === "object" && error !== null && "message" in error) {
    const message = (error as { message?: unknown }).message;
    if (typeof message === "string" && message) return message;
  }
  return "Quantix could not complete the recovery inspection. Try again.";
}

export function WorkspaceRecoveryCenter({
  tenderId,
  tenderName,
  variant = "recovery",
  onClose,
  onRecovered,
  onMoveToTrash,
  onDeletePermanently,
  requestedAction,
  onRequestedActionHandled,
}: WorkspaceRecoveryCenterProps) {
  const forBackups = variant === "backups";
  const titleId = useId();
  const mounted = useRef(true);
  const [snapshot, setSnapshot] = useState<RecoverySnapshot>();
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string>();
  const [operation, setOperation] = useState<string>();
  const [rationales, setRationales] = useState<Record<string, string>>({});
  const [destructiveAction, setDestructiveAction] = useState<
    "trash" | "permanent" | null
  >(null);
  const [destructiveRationale, setDestructiveRationale] = useState("");
  const [confirmationTenderName, setConfirmationTenderName] = useState("");

  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  const refresh = useCallback(async () => {
    setLoading(true);
    setError(undefined);
    try {
      const [report, backups, recoveries] = await Promise.all([
        inspectTenderIntegrity(tenderId),
        inspectTenderBackups(tenderId),
        inspectTenderRecoveries(tenderId),
      ]);
      if (mounted.current) setSnapshot({ report, backups, recoveries });
    } catch (inspectionError) {
      if (mounted.current) setError(errorMessage(inspectionError));
    } finally {
      if (mounted.current) setLoading(false);
    }
  }, [tenderId]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  useEffect(() => {
    if (!requestedAction) return;
    setDestructiveAction(
      requestedAction === "move_to_trash" ? "trash" : "permanent",
    );
    onRequestedActionHandled?.();
  }, [onRequestedActionHandled, requestedAction]);

  const handlePrepare = async (backupId: string) => {
    setOperation(`prepare:${backupId}`);
    setError(undefined);
    try {
      await prepareTenderRecovery(tenderId, backupId);
      await refresh();
    } catch (prepareError) {
      setError(errorMessage(prepareError));
    } finally {
      setOperation(undefined);
    }
  };

  const handleDecision = async (
    recoveryId: string,
    decision: TenderRecoveryDecision,
  ) => {
    const rationale = rationales[recoveryId]?.trim() ?? "";
    if (!rationale) return;

    setOperation(`${decision}:${recoveryId}`);
    setError(undefined);
    try {
      await resolveTenderRecovery(tenderId, recoveryId, decision, rationale);
      if (decision === "approve_replacement") {
        await onRecovered();
      } else {
        await refresh();
      }
    } catch (decisionError) {
      setError(errorMessage(decisionError));
      await refresh().catch(() => undefined);
    } finally {
      setOperation(undefined);
    }
  };

  const closeDestructiveDialog = () => {
    if (operation) return;
    setDestructiveAction(null);
    setDestructiveRationale("");
    setConfirmationTenderName("");
  };

  const handleDestructiveAction = async () => {
    const rationale = destructiveRationale.trim();
    if (
      !rationale ||
      (destructiveAction === "permanent" &&
        confirmationTenderName !== tenderName)
    ) {
      return;
    }

    const action = destructiveAction;
    setOperation(action === "trash" ? "move-to-trash" : "delete-permanently");
    setError(undefined);
    try {
      if (action === "trash") {
        await onMoveToTrash(rationale);
      } else {
        await onDeletePermanently(rationale, confirmationTenderName);
      }
      setDestructiveAction(null);
      setDestructiveRationale("");
      setConfirmationTenderName("");
      onClose();
    } catch (actionError) {
      setError(errorMessage(actionError));
    } finally {
      setOperation(undefined);
    }
  };

  const report = snapshot?.report;
  const isBusy = loading || operation !== undefined;

  return (
    <section
      className="workspace-recovery"
      aria-labelledby={titleId}
      aria-busy={isBusy}
    >
      <header className="workspace-recovery__header">
        <div>
          <p className="workspace-recovery__eyebrow">
            {forBackups ? "Tender backups" : "Tender recovery"}
          </p>
          <h2 id={titleId}>
            {forBackups ? `Backups for ${tenderName}` : `Recover ${tenderName}`}
          </h2>
          <p className="workspace-recovery__subtitle">
            {forBackups
              ? "Quantix verifies every backup before listing it here. Nothing changes the current Tender without your explicit Engineer decision."
              : "This Tender is isolated until its records and provenance can be verified. Recovery never changes the current store without your explicit Engineer decision."}
          </p>
        </div>
        <button
          className="workspace-recovery__close"
          type="button"
          onClick={onClose}
          aria-label={forBackups ? "Close backups" : "Close recovery"}
        >
          ×
        </button>
      </header>

      <div className="workspace-recovery__body">
        {error ? (
          <div className="workspace-recovery__error" role="alert">
            <strong>Recovery inspection needs attention</strong>
            <p>{error}</p>
          </div>
        ) : null}

        {loading && !snapshot ? (
          <p
            className="workspace-recovery__status"
            role="status"
            aria-live="polite"
          >
            Inspecting Tender integrity and verified recovery records…
          </p>
        ) : null}

        {!loading && !snapshot && !error ? (
          <p className="workspace-recovery__empty">
            No recovery report is available yet. Run the inspection again.
          </p>
        ) : null}

        {report ? (
          <>
            <section
              className="workspace-recovery__summary"
              aria-labelledby={`${titleId}-summary`}
            >
              <div>
                <p className="workspace-recovery__eyebrow">
                  Current safety state
                </p>
                <h3 id={`${titleId}-summary`}>
                  {report.state === "ready"
                    ? "Integrity verified"
                    : "Recovery required"}
                </h3>
              </div>
              <code
                className={`workspace-recovery__state workspace-recovery__state--${report.state}`}
              >
                {report.state}
              </code>
              {report.state === "ready" ? (
                <button
                  type="button"
                  onClick={() => void onRecovered()}
                  disabled={isBusy}
                >
                  Open Tender
                </button>
              ) : (
                <div className="workspace-recovery__actions workspace-recovery__destructive-actions">
                  <button
                    type="button"
                    className="workspace-recovery__secondary"
                    onClick={() => setDestructiveAction("trash")}
                    disabled={isBusy}
                  >
                    Move to Trash
                  </button>
                  <button
                    type="button"
                    className="workspace-recovery__danger"
                    onClick={() => setDestructiveAction("permanent")}
                    disabled={isBusy}
                  >
                    Delete Permanently
                  </button>
                </div>
              )}
            </section>

            <section aria-labelledby={`${titleId}-issues`}>
              <div className="workspace-recovery__section-heading">
                <div>
                  <p className="workspace-recovery__eyebrow">
                    Integrity findings
                  </p>
                  <h3 id={`${titleId}-issues`}>Why access is paused</h3>
                </div>
                <button
                  type="button"
                  onClick={() => void refresh()}
                  disabled={isBusy}
                >
                  {loading ? "Checking…" : "Recheck"}
                </button>
              </div>
              {report.issues.length === 0 ? (
                <p className="workspace-recovery__empty">
                  No integrity issues were reported.
                </p>
              ) : (
                <div className="workspace-recovery__issue-list">
                  {report.issues.map((issue) => {
                    const copy = issueCopy[issue];
                    return (
                      <article
                        className="workspace-recovery__issue"
                        key={issue}
                      >
                        <div className="workspace-recovery__issue-heading">
                          <h4>{copy.cause}</h4>
                          <code>{issue}</code>
                        </div>
                        <dl>
                          <div>
                            <dt>Affected capability</dt>
                            <dd>{copy.capability}</dd>
                          </div>
                          <div>
                            <dt>Impact</dt>
                            <dd>{copy.impact}</dd>
                          </div>
                          <div>
                            <dt>Safe remediation</dt>
                            <dd>{copy.remediation}</dd>
                          </div>
                        </dl>
                      </article>
                    );
                  })}
                </div>
              )}
            </section>

            <section aria-labelledby={`${titleId}-backups`}>
              <div className="workspace-recovery__section-heading">
                <div>
                  <p className="workspace-recovery__eyebrow">
                    Verified backups
                  </p>
                  <h3 id={`${titleId}-backups`}>Recovery sources</h3>
                </div>
              </div>
              {snapshot?.backups.length ? (
                <ul className="workspace-recovery__record-list">
                  {snapshot.backups.map((backup) => {
                    const actionKey = `prepare:${backup.backup_id}`;
                    return (
                      <li key={backup.backup_id}>
                        <div className="workspace-recovery__record-copy">
                          <strong>{backupStateCopy[backup.state]}</strong>
                          <span>
                            {backup.created_at} ·{" "}
                            {String(backup.content_object_count)} content
                            objects
                          </span>
                          <code>
                            {backup.diagnostic_code ?? "No diagnostic code"}
                          </code>
                        </div>
                        {backup.state === "ready" ? (
                          <button
                            type="button"
                            onClick={() => void handlePrepare(backup.backup_id)}
                            disabled={isBusy}
                          >
                            {operation === actionKey
                              ? "Preparing…"
                              : "Prepare candidate"}
                          </button>
                        ) : null}
                      </li>
                    );
                  })}
                </ul>
              ) : (
                <p className="workspace-recovery__empty">
                  No verified backup is available for this Tender.
                </p>
              )}
            </section>

            <section aria-labelledby={`${titleId}-candidates`}>
              <p className="workspace-recovery__eyebrow">Recovery candidates</p>
              <h3 id={`${titleId}-candidates`}>Engineer decisions</h3>
              {snapshot?.recoveries.length ? (
                <div className="workspace-recovery__candidate-list">
                  {snapshot.recoveries.map((recovery) => {
                    const rationale = rationales[recovery.recovery_id] ?? "";
                    const actionKey = `approve_replacement:${recovery.recovery_id}`;
                    const decisionKey = `reject:${recovery.recovery_id}`;
                    return (
                      <article
                        className="workspace-recovery__candidate"
                        key={recovery.recovery_id}
                      >
                        <div className="workspace-recovery__candidate-heading">
                          <div>
                            <h4>{recoveryStateCopy[recovery.state]}</h4>
                            <p>
                              Backup revision{" "}
                              {recovery.backup_source?.revision ?? "unknown"} ·
                              current revision{" "}
                              {recovery.current_source?.revision ?? "unknown"}
                            </p>
                          </div>
                          <code>
                            {recovery.diagnostic_code ?? "No diagnostic code"}
                          </code>
                        </div>
                        {recovery.state === "awaiting_approval" ? (
                          <>
                            <label
                              htmlFor={`${titleId}-rationale-${recovery.recovery_id}`}
                            >
                              Engineer rationale{" "}
                              <span aria-hidden="true">*</span>
                            </label>
                            <textarea
                              id={`${titleId}-rationale-${recovery.recovery_id}`}
                              value={rationale}
                              onChange={(event) =>
                                setRationales((current) => ({
                                  ...current,
                                  [recovery.recovery_id]: event.target.value,
                                }))
                              }
                              maxLength={500}
                              placeholder="Explain the evidence for this decision"
                              disabled={isBusy}
                            />
                            <div className="workspace-recovery__actions">
                              <button
                                type="button"
                                onClick={() =>
                                  void handleDecision(
                                    recovery.recovery_id,
                                    "approve_replacement",
                                  )
                                }
                                disabled={isBusy || !rationale.trim()}
                              >
                                {operation === actionKey
                                  ? "Approving…"
                                  : "Approve replacement"}
                              </button>
                              <button
                                type="button"
                                className="workspace-recovery__secondary"
                                onClick={() =>
                                  void handleDecision(
                                    recovery.recovery_id,
                                    "reject",
                                  )
                                }
                                disabled={isBusy || !rationale.trim()}
                              >
                                {operation === decisionKey
                                  ? "Rejecting…"
                                  : "Reject candidate"}
                              </button>
                            </div>
                          </>
                        ) : recovery.decision_record ? (
                          <p className="workspace-recovery__decision">
                            Engineer rationale:{" "}
                            {recovery.decision_record.rationale}
                          </p>
                        ) : null}
                      </article>
                    );
                  })}
                </div>
              ) : (
                <p className="workspace-recovery__empty">
                  No recovery candidate has been prepared.
                </p>
              )}
            </section>
          </>
        ) : null}
      </div>

      <QuantixDialog
        isOpen={destructiveAction === "trash"}
        title={`Move ${tenderName} to Trash`}
        onOpenChange={(open) => {
          if (!open) closeDestructiveDialog();
        }}
      >
        <p className="workspace-recovery__dialog-copy">
          This moves the Quantix-controlled Tender Store to Trash so it can be
          restored later. External source packages remain untouched.
        </p>
        <label
          className="workspace-recovery__dialog-field"
          htmlFor={`${titleId}-trash-rationale`}
        >
          Reason for moving to Trash <span aria-hidden="true">*</span>
          <textarea
            id={`${titleId}-trash-rationale`}
            autoFocus
            maxLength={500}
            value={destructiveRationale}
            disabled={operation !== undefined}
            onChange={(event) => setDestructiveRationale(event.target.value)}
            placeholder="Explain why this Tender should be moved to Trash"
          />
        </label>
        <div className="workspace-recovery__dialog-actions">
          <button
            type="button"
            className="workspace-recovery__secondary"
            onClick={closeDestructiveDialog}
            disabled={operation !== undefined}
          >
            Cancel
          </button>
          <button
            type="button"
            onClick={() => void handleDestructiveAction()}
            disabled={operation !== undefined || !destructiveRationale.trim()}
          >
            {operation === "move-to-trash" ? "Moving…" : "Move to Trash"}
          </button>
        </div>
      </QuantixDialog>

      <QuantixDialog
        isOpen={destructiveAction === "permanent"}
        title={`Delete ${tenderName} permanently`}
        onOpenChange={(open) => {
          if (!open) closeDestructiveDialog();
        }}
      >
        <p className="workspace-recovery__dialog-copy workspace-recovery__dialog-copy--danger">
          This action is irreversible. It removes the Tender Store and every
          identifiable Quantix-controlled backup, Portable Tender Archive,
          delivery export, Agent workspace, staging or quarantine item, and
          Tender-specific log. Original source packages and copies outside
          Quantix remain untouched.
        </p>
        <p className="workspace-recovery__dialog-copy">
          Provider cleanup stays pending while a provider is unavailable. If
          Store damage prevents complete reference discovery, the receipt is
          marked incomplete and manual provider review may be required.
        </p>
        <label
          className="workspace-recovery__dialog-field"
          htmlFor={`${titleId}-permanent-rationale`}
        >
          Reason for permanent deletion <span aria-hidden="true">*</span>
          <textarea
            id={`${titleId}-permanent-rationale`}
            autoFocus
            maxLength={500}
            value={destructiveRationale}
            disabled={operation !== undefined}
            onChange={(event) => setDestructiveRationale(event.target.value)}
            placeholder="Explain why this Tender must be deleted permanently"
          />
        </label>
        <label
          className="workspace-recovery__dialog-field"
          htmlFor={`${titleId}-permanent-confirmation`}
        >
          Type <strong>{tenderName}</strong> to confirm
          <input
            id={`${titleId}-permanent-confirmation`}
            value={confirmationTenderName}
            disabled={operation !== undefined}
            onChange={(event) => setConfirmationTenderName(event.target.value)}
            autoComplete="off"
          />
        </label>
        <div className="workspace-recovery__dialog-actions">
          <button
            type="button"
            className="workspace-recovery__secondary"
            onClick={closeDestructiveDialog}
            disabled={operation !== undefined}
          >
            Cancel
          </button>
          <button
            className="workspace-recovery__danger"
            type="button"
            onClick={() => void handleDestructiveAction()}
            disabled={
              operation !== undefined ||
              !destructiveRationale.trim() ||
              confirmationTenderName !== tenderName
            }
          >
            {operation === "delete-permanently"
              ? "Deleting…"
              : "Delete Permanently"}
          </button>
        </div>
      </QuantixDialog>
    </section>
  );
}
