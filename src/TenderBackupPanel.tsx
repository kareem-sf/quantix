import { useState } from "react";

import type { TenderBackupRecord } from "./bindings/TenderBackupRecord";
import type { TenderBackupState } from "./bindings/TenderBackupState";
import type { TenderRecoveryDecision } from "./bindings/TenderRecoveryDecision";
import type { TenderRecoveryRecord } from "./bindings/TenderRecoveryRecord";
import type { TenderRecoveryState } from "./bindings/TenderRecoveryState";

const backupStateCopy: Record<TenderBackupState, string> = {
  creating: "Creating and independently verifying",
  ready: "Verified and ready",
  failed: "Failed",
};

const recoveryStateCopy: Record<TenderRecoveryState, string> = {
  preparing: "Verifying candidate",
  awaiting_approval: "Awaiting Engineer approval",
  applying: "Applying approved replacement",
  applied: "Applied",
  rejected: "Rejected",
  failed: "Failed",
};

interface TenderBackupPanelProps {
  tenderId: string;
  backups: TenderBackupRecord[];
  recoveries: TenderRecoveryRecord[];
  busy: boolean;
  canCreate?: boolean;
  onCreate: () => void;
  onPrepare: (backupId: string) => void;
  onResolve: (
    recoveryId: string,
    decision: TenderRecoveryDecision,
    rationale: string,
  ) => void;
}

export function TenderBackupPanel({
  tenderId,
  backups,
  recoveries,
  busy,
  canCreate = true,
  onCreate,
  onPrepare,
  onResolve,
}: TenderBackupPanelProps) {
  const [rationales, setRationales] = useState<Record<string, string>>({});

  return (
    <section className="backup-panel" aria-labelledby={`backup-${tenderId}`}>
      <div className="backup-panel__heading">
        <div>
          <p className="section-label">Verified resilience</p>
          <h4 id={`backup-${tenderId}`}>Backup and recovery</h4>
        </div>
        {canCreate ? (
          <button type="button" onClick={onCreate} disabled={busy}>
            Create verified backup
          </button>
        ) : null}
      </div>
      <p>
        Backups contain one consistent database snapshot and every referenced
        immutable content object. Recovery never merges histories or changes
        this Tender before explicit approval.
      </p>
      {backups.length === 0 ? (
        <p className="catalogue-message">No backup attempts recorded.</p>
      ) : (
        <ul className="backup-list" aria-label="Tender backups">
          {backups.map((backup) => (
            <li key={backup.backup_id}>
              <div>
                <strong>{backupStateCopy[backup.state]}</strong>
                <small>
                  {backup.created_at} · {String(backup.content_object_count)}
                  {" content objects"}
                </small>
                {backup.diagnostic_code ? (
                  <small>Diagnostic: {backup.diagnostic_code}</small>
                ) : null}
              </div>
              {backup.state === "ready" ? (
                <button
                  type="button"
                  className="button-secondary"
                  onClick={() => onPrepare(backup.backup_id)}
                  disabled={busy}
                >
                  Verify recovery candidate
                </button>
              ) : null}
            </li>
          ))}
        </ul>
      )}
      {recoveries.length > 0 ? (
        <div className="recovery-offers">
          <h5>Recovery decisions</h5>
          {recoveries.map((recovery) => {
            const rationale = rationales[recovery.recovery_id] ?? "";
            return (
              <article key={recovery.recovery_id}>
                <strong>{recoveryStateCopy[recovery.state]}</strong>
                <p>
                  Backup revision {recovery.backup_source?.revision ?? "?"} ·
                  current revision {recovery.current_source?.revision ?? "?"}
                </p>
                {recovery.diagnostic_code ? (
                  <p>Diagnostic: {recovery.diagnostic_code}</p>
                ) : null}
                {recovery.state === "awaiting_approval" ? (
                  <>
                    <label>
                      Decision rationale
                      <textarea
                        value={rationale}
                        onChange={(event) =>
                          setRationales((current) => ({
                            ...current,
                            [recovery.recovery_id]: event.target.value,
                          }))
                        }
                        maxLength={500}
                        disabled={busy}
                      />
                    </label>
                    <div className="intake-actions">
                      <button
                        type="button"
                        onClick={() =>
                          onResolve(
                            recovery.recovery_id,
                            "approve_replacement",
                            rationale.trim(),
                          )
                        }
                        disabled={busy || !rationale.trim()}
                      >
                        Approve exact replacement
                      </button>
                      <button
                        type="button"
                        className="button-secondary"
                        onClick={() =>
                          onResolve(
                            recovery.recovery_id,
                            "reject",
                            rationale.trim(),
                          )
                        }
                        disabled={busy || !rationale.trim()}
                      >
                        Keep current Tender
                      </button>
                    </div>
                  </>
                ) : recovery.decision_record ? (
                  <p>
                    Engineer rationale: {recovery.decision_record.rationale} ·
                    recorded for {recovery.decision_record.decided_by}
                  </p>
                ) : null}
              </article>
            );
          })}
        </div>
      ) : null}
    </section>
  );
}
