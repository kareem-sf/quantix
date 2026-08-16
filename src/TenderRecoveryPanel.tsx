import type { TenderIntegrityIssue } from "./bindings/TenderIntegrityIssue";
import type { TenderIntegrityReport } from "./bindings/TenderIntegrityReport";
import type { TenderRecoveryChoice } from "./bindings/TenderRecoveryChoice";

const issueCopy: Record<TenderIntegrityIssue, string> = {
  audit_chain_invalid:
    "The immutable Audit Event chain no longer matches its recorded history.",
  database_integrity_invalid:
    "The SQLite database failed page or relational integrity verification.",
  inspection_unavailable:
    "Quantix could not complete integrity verification because required storage was unavailable.",
  referenced_content_missing:
    "Canonical content referenced by this Tender is missing from local storage.",
  referenced_content_mismatch:
    "Canonical content no longer matches its recorded digest or size.",
  manifest_invalid:
    "A canonical task, permission, evidence, or Agent Run manifest failed semantic verification.",
  schema_mismatch:
    "The Tender Store database structure or identity failed verification.",
  storage_layout_invalid:
    "The Tender Store contains an unsafe link, unexpected entry, or invalid storage layout.",
  tender_identity_mismatch:
    "The immutable Tender identity does not match the directory selected for recovery.",
};

const choiceCopy: Record<TenderRecoveryChoice, string> = {
  restore_verified_backup:
    "Restore a separately verified complete backup after inspecting its identity and history.",
  purge_tender:
    "Purge this Tender only after confirming that its retained evidence is no longer required.",
};

interface TenderRecoveryPanelProps {
  report: TenderIntegrityReport;
  refreshing: boolean;
  onRefresh: () => void;
}

export function TenderRecoveryPanel({
  report,
  refreshing,
  onRefresh,
}: TenderRecoveryPanelProps) {
  return (
    <section className="recovery-panel" aria-labelledby="recovery-title">
      <p className="section-label">Read-only safety state</p>
      <h3 id="recovery-title">Recovery Required</h3>
      <p>
        Quantix stopped before accepting work for Tender {report.tender_id}.
        Inspect the evidence below before choosing a recovery path.
      </p>
      <h4>Verification evidence</h4>
      <ul>
        {report.issues.map((issue) => (
          <li key={issue}>{issueCopy[issue]}</li>
        ))}
      </ul>
      <h4>Engineer-controlled choices</h4>
      <ul>
        {report.recovery_choices.map((choice) => (
          <li key={choice}>{choiceCopy[choice]}</li>
        ))}
      </ul>
      <p>
        Recovery actions remain unavailable until their candidates can be
        verified without changing this Tender.
      </p>
      <button type="button" onClick={onRefresh} disabled={refreshing}>
        {refreshing ? "Verifying..." : "Verify integrity again"}
      </button>
    </section>
  );
}
