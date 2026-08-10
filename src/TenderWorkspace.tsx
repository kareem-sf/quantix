import { FormEvent, useCallback, useEffect, useState } from "react";

import type { DocumentRegister } from "./bindings/DocumentRegister";
import type { SourceRelationshipKind } from "./bindings/SourceRelationshipKind";
import type { TenderCatalogueEntry } from "./bindings/TenderCatalogueEntry";
import type { TenderBackupRecord } from "./bindings/TenderBackupRecord";
import type { TenderIntegrityReport } from "./bindings/TenderIntegrityReport";
import type { TenderPackageImportResult } from "./bindings/TenderPackageImportResult";
import type { TenderPackageSourceKind } from "./bindings/TenderPackageSourceKind";
import type { TenderSummary } from "./bindings/TenderSummary";
import type { TenderRecoveryDecision } from "./bindings/TenderRecoveryDecision";
import type { TenderRecoveryRecord } from "./bindings/TenderRecoveryRecord";
import { AgentRunOffice } from "./AgentRunOffice";
import { BidDecisionPanel } from "./BidDecisionPanel";
import { DocumentEvidenceOffice } from "./DocumentEvidenceOffice";
import { ExternalRfiPanel } from "./ExternalRfiPanel";
import { TenderBackupPanel } from "./TenderBackupPanel";
import { TenderRecoveryPanel } from "./TenderRecoveryPanel";
import { TenderRecordsPanel } from "./TenderRecordsPanel";
import { TenderOfficePanel } from "./TenderOfficePanel";
import { TenderQueryRegisterPanel } from "./TenderQueryRegisterPanel";
import {
  chooseAndImportTenderPackage,
  confirmSourceRelationship,
  createTenderBackup,
  createTender,
  inspectDocumentRegister,
  inspectTenderBackups,
  inspectTenderIntegrity,
  inspectTenderRecoveries,
  listTenders,
  openTender,
  prepareTenderRecovery,
  resolveTenderRecovery,
  reviseTender,
} from "./quantixHost";

type CatalogueState =
  | { kind: "loading" }
  | { kind: "ready"; tenders: TenderCatalogueEntry[] }
  | { kind: "error" };

interface TenderWorkspaceProps {
  runtimeReady: boolean;
}

export function TenderWorkspace({ runtimeReady }: TenderWorkspaceProps) {
  const [catalogue, setCatalogue] = useState<CatalogueState>({
    kind: "loading",
  });
  const [selected, setSelected] = useState<TenderSummary>();
  const [recovery, setRecovery] = useState<TenderIntegrityReport>();
  const [backups, setBackups] = useState<TenderBackupRecord[]>([]);
  const [recoveries, setRecoveries] = useState<TenderRecoveryRecord[]>([]);
  const [documentRegister, setDocumentRegister] = useState<DocumentRegister>();
  const [lastIntake, setLastIntake] = useState<TenderPackageImportResult>();
  const [newName, setNewName] = useState("");
  const [revisionName, setRevisionName] = useState("");
  const [priorVersionKey, setPriorVersionKey] = useState("");
  const [replacementVersionKey, setReplacementVersionKey] = useState("");
  const [relationshipKind, setRelationshipKind] =
    useState<SourceRelationshipKind>("replacement");
  const [busy, setBusy] = useState(false);
  const [commandFailed, setCommandFailed] = useState(false);
  const [tenderStateVersion, setTenderStateVersion] = useState(0);
  const [productionScheduling, setProductionScheduling] = useState(false);
  const reportCommandFailure = useCallback(() => setCommandFailed(true), []);
  const reportTenderStateChange = useCallback(
    () => setTenderStateVersion((version) => version + 1),
    [],
  );

  const refreshBackupRecovery = async (tenderId: string) => {
    const [nextBackups, nextRecoveries] = await Promise.all([
      inspectTenderBackups(tenderId),
      inspectTenderRecoveries(tenderId),
    ]);
    setBackups(nextBackups);
    setRecoveries(nextRecoveries);
  };

  useEffect(() => {
    let active = true;
    void listTenders()
      .then((tenders) => {
        if (active) setCatalogue({ kind: "ready", tenders });
      })
      .catch(() => {
        if (active) setCatalogue({ kind: "error" });
      });
    return () => {
      active = false;
    };
  }, []);

  const updateTender = (tender: TenderSummary) => {
    setCatalogue((current) => {
      if (current.kind !== "ready") return current;
      const updated: TenderCatalogueEntry = {
        tender_id: tender.tender_id,
        summary: tender,
        integrity: {
          tender_id: tender.tender_id,
          state: "ready",
          issues: [],
          recovery_choices: [],
        },
      };
      const tenders = current.tenders
        .filter((candidate) => candidate.tender_id !== tender.tender_id)
        .concat(updated)
        .sort((left, right) =>
          (left.summary?.name ?? left.tender_id).localeCompare(
            right.summary?.name ?? right.tender_id,
          ),
        );
      return { kind: "ready", tenders };
    });
    setSelected(tender);
    setRecovery(undefined);
    setRevisionName(tender.name);
  };

  const runCommand = async (command: () => Promise<TenderSummary>) => {
    setBusy(true);
    setCommandFailed(false);
    try {
      updateTender(await command());
      return true;
    } catch {
      setCommandFailed(true);
      return false;
    } finally {
      setBusy(false);
    }
  };

  const handleCreate = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const name = newName.trim();
    if (!name) return;
    void runCommand(() => createTender(name)).then((created) => {
      if (created) {
        setNewName("");
        setDocumentRegister({ query_register_open: false, documents: [] });
        setLastIntake(undefined);
        setPriorVersionKey("");
        setReplacementVersionKey("");
        setBackups([]);
        setRecoveries([]);
      }
    });
  };

  const handleRevise = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const name = revisionName.trim();
    if (!selected || !name) return;
    void runCommand(() => reviseTender(selected.tender_id, name));
  };

  const handleOpen = async (tenderId: string) => {
    setBusy(true);
    setCommandFailed(false);
    setDocumentRegister(undefined);
    setPriorVersionKey("");
    setReplacementVersionKey("");
    try {
      updateTender(await openTender(tenderId));
      setDocumentRegister(await inspectDocumentRegister(tenderId));
      await refreshBackupRecovery(tenderId);
      setLastIntake(undefined);
    } catch {
      try {
        const integrity = await inspectTenderIntegrity(tenderId);
        if (integrity.state === "recovery_required") {
          setSelected(undefined);
          setRecovery(integrity);
          await refreshBackupRecovery(tenderId);
          return;
        }
      } catch {
        // The command failure below remains the authoritative UI state.
      }
      setCommandFailed(true);
    } finally {
      setBusy(false);
    }
  };

  const inspectRecovery = async (tenderId: string) => {
    setBusy(true);
    setCommandFailed(false);
    try {
      const integrity = await inspectTenderIntegrity(tenderId);
      if (integrity.state === "ready") {
        await handleOpen(tenderId);
      } else {
        setSelected(undefined);
        setRecovery(integrity);
        await refreshBackupRecovery(tenderId);
      }
    } catch {
      setCommandFailed(true);
    } finally {
      setBusy(false);
    }
  };

  const handleCreateBackup = async (tenderId: string) => {
    setBusy(true);
    setCommandFailed(false);
    try {
      await createTenderBackup(tenderId);
      await refreshBackupRecovery(tenderId);
    } catch {
      setCommandFailed(true);
      await refreshBackupRecovery(tenderId).catch(() => undefined);
    } finally {
      setBusy(false);
    }
  };

  const handlePrepareRecovery = async (tenderId: string, backupId: string) => {
    setBusy(true);
    setCommandFailed(false);
    try {
      await prepareTenderRecovery(tenderId, backupId);
      await refreshBackupRecovery(tenderId);
    } catch {
      setCommandFailed(true);
      await refreshBackupRecovery(tenderId).catch(() => undefined);
    } finally {
      setBusy(false);
    }
  };

  const handleResolveRecovery = async (
    tenderId: string,
    recoveryId: string,
    decision: TenderRecoveryDecision,
    rationale: string,
  ) => {
    setBusy(true);
    setCommandFailed(false);
    try {
      await resolveTenderRecovery(tenderId, recoveryId, decision, rationale);
      if (decision === "approve_replacement") {
        updateTender(await openTender(tenderId));
        setDocumentRegister(await inspectDocumentRegister(tenderId));
      }
      await refreshBackupRecovery(tenderId);
    } catch {
      setCommandFailed(true);
      await refreshBackupRecovery(tenderId).catch(() => undefined);
    } finally {
      setBusy(false);
    }
  };

  const handlePackageChoice = async (sourceKind: TenderPackageSourceKind) => {
    if (!selected) return;
    setBusy(true);
    setCommandFailed(false);
    try {
      const imported = await chooseAndImportTenderPackage(
        selected.tender_id,
        sourceKind,
      );
      if (imported) {
        setLastIntake(imported);
        setDocumentRegister(await inspectDocumentRegister(selected.tender_id));
        updateTender(await openTender(selected.tender_id));
        setPriorVersionKey("");
        setReplacementVersionKey("");
      }
    } catch {
      setCommandFailed(true);
    } finally {
      setBusy(false);
    }
  };

  const versionKey = (artifactId: string, version: number) =>
    `${artifactId}:${version}`;
  const registeredDocuments =
    documentRegister?.documents.filter(
      (document) => document.registration_state === "registered",
    ) ?? [];

  const handleRelationshipConfirmation = async (
    event: FormEvent<HTMLFormElement>,
  ) => {
    event.preventDefault();
    if (!selected) return;
    const prior = registeredDocuments.find(
      (document) =>
        versionKey(document.artifact_id, document.version) === priorVersionKey,
    );
    const replacement = registeredDocuments.find(
      (document) =>
        versionKey(document.artifact_id, document.version) ===
        replacementVersionKey,
    );
    if (!prior || !replacement || priorVersionKey === replacementVersionKey)
      return;

    setBusy(true);
    setCommandFailed(false);
    try {
      setDocumentRegister(
        await confirmSourceRelationship(
          selected.tender_id,
          prior.artifact_id,
          prior.version,
          replacement.artifact_id,
          replacement.version,
          relationshipKind,
        ),
      );
      updateTender(await openTender(selected.tender_id));
      setPriorVersionKey("");
      setReplacementVersionKey("");
    } catch {
      setCommandFailed(true);
    } finally {
      setBusy(false);
    }
  };

  return (
    <section className="tender-office" aria-labelledby="tender-catalogue-title">
      <div className="tender-office__heading">
        <div>
          <p className="section-label">Tender stores</p>
          <h2 id="tender-catalogue-title">Tender Catalogue</h2>
        </div>
        <p>
          Each Tender is an auditable, self-contained source of truth beneath
          the Quantix Application Home.
        </p>
      </div>

      <div className="tender-office__layout">
        <div className="catalogue-panel">
          <form className="tender-form" onSubmit={handleCreate}>
            <label htmlFor="new-tender-name">New Tender name</label>
            <div className="tender-form__row">
              <input
                id="new-tender-name"
                value={newName}
                onChange={(event) => setNewName(event.target.value)}
                maxLength={200}
                autoComplete="off"
                disabled={busy || !runtimeReady}
              />
              <button
                type="submit"
                disabled={busy || !runtimeReady || !newName.trim()}
              >
                Create Tender
              </button>
            </div>
          </form>

          {!runtimeReady ? (
            <p className="catalogue-message" role="status">
              Local Tender inspection and recovery remain available. New work
              waits for Provider runtime readiness.
            </p>
          ) : null}

          {catalogue.kind === "loading" ? (
            <p className="catalogue-message" aria-live="polite">
              Loading Tender Catalogue…
            </p>
          ) : null}
          {catalogue.kind === "error" ? (
            <p className="catalogue-error" role="alert">
              The Tender Catalogue is unavailable. Run Setup checks and try
              again.
            </p>
          ) : null}
          {catalogue.kind === "ready" && catalogue.tenders.length === 0 ? (
            <p className="catalogue-message">
              No Tenders yet. Create the first controlled Tender Store above.
            </p>
          ) : null}
          {catalogue.kind === "ready" && catalogue.tenders.length > 0 ? (
            <ul className="tender-list" aria-label="Available Tenders">
              {catalogue.tenders.map((tender) => (
                <li key={tender.tender_id}>
                  <button
                    type="button"
                    className={
                      selected?.tender_id === tender.tender_id ||
                      recovery?.tender_id === tender.tender_id
                        ? "tender-row tender-row--selected"
                        : "tender-row"
                    }
                    onClick={() =>
                      void (tender.summary
                        ? handleOpen(tender.tender_id)
                        : inspectRecovery(tender.tender_id))
                    }
                    disabled={busy}
                  >
                    <span>
                      {tender.summary?.name ??
                        `Tender ${tender.tender_id.slice(0, 12)}`}
                    </span>
                    {tender.summary ? (
                      <small>
                        Revision {tender.summary.revision} ·{" "}
                        {tender.summary.lifecycle_phase.replace(/_/g, " ")} ·{" "}
                        {tender.summary.audit_event_count} audit events
                      </small>
                    ) : (
                      <small>Recovery Required · read-only</small>
                    )}
                  </button>
                </li>
              ))}
            </ul>
          ) : null}
        </div>

        <aside className="tender-detail" aria-live="polite">
          {selected ? (
            <>
              <p className="section-label">Opened Tender</p>
              <h3>{selected.name}</h3>
              <dl>
                <div>
                  <dt>Current revision</dt>
                  <dd>{selected.revision}</dd>
                </div>
                <div>
                  <dt>Audit events</dt>
                  <dd>{selected.audit_event_count}</dd>
                </div>
                <div>
                  <dt>Lifecycle</dt>
                  <dd>{selected.lifecycle_phase.replace(/_/g, " ")}</dd>
                </div>
                <div>
                  <dt>Chain head</dt>
                  <dd>{selected.audit_chain_head.slice(0, 12)}…</dd>
                </div>
              </dl>
              <TenderBackupPanel
                tenderId={selected.tender_id}
                backups={backups}
                recoveries={recoveries}
                busy={busy}
                onCreate={() => void handleCreateBackup(selected.tender_id)}
                onPrepare={(backupId) =>
                  void handlePrepareRecovery(selected.tender_id, backupId)
                }
                onResolve={(recoveryId, decision, rationale) =>
                  void handleResolveRecovery(
                    selected.tender_id,
                    recoveryId,
                    decision,
                    rationale,
                  )
                }
              />
              <form className="tender-form" onSubmit={handleRevise}>
                <label htmlFor="revised-tender-name">Revise Tender name</label>
                <input
                  id="revised-tender-name"
                  value={revisionName}
                  onChange={(event) => setRevisionName(event.target.value)}
                  maxLength={200}
                  autoComplete="off"
                  disabled={busy || !runtimeReady}
                />
                <button
                  type="submit"
                  disabled={busy || !runtimeReady || !revisionName.trim()}
                >
                  Save immutable revision
                </button>
              </form>
              <section
                className="intake-panel"
                aria-labelledby="tender-intake-title"
              >
                <div className="intake-panel__heading">
                  <div>
                    <p className="section-label">Controlled source</p>
                    <h4 id="tender-intake-title">Tender Package intake</h4>
                  </div>
                  <span
                    className={
                      documentRegister?.query_register_open
                        ? "status-badge status-badge--ready"
                        : "status-badge"
                    }
                  >
                    Query Register{" "}
                    {documentRegister?.query_register_open
                      ? "open"
                      : "not open"}
                  </span>
                </div>
                <p>
                  Choose one source. Quantix copies verified documents into this
                  Tender; the original can then be moved or disconnected.
                </p>
                <div className="intake-actions">
                  <button
                    type="button"
                    onClick={() => void handlePackageChoice("directory")}
                    disabled={busy || !runtimeReady}
                  >
                    Choose project directory
                  </button>
                  <button
                    type="button"
                    className="button-secondary"
                    onClick={() => void handlePackageChoice("zip_archive")}
                    disabled={busy || !runtimeReady}
                  >
                    Choose ZIP archive
                  </button>
                </div>
                {lastIntake ? (
                  <p className="intake-result" role="status">
                    Intake registered {lastIntake.registered_count} of{" "}
                    {lastIntake.discovered_count} discovered entries.{" "}
                    {lastIntake.exception_count} require review.
                  </p>
                ) : null}
              </section>
              <AgentRunOffice
                key={`agents-${selected.tender_id}`}
                tenderId={selected.tender_id}
                runtimeReady={runtimeReady}
                reportCommandFailure={reportCommandFailure}
                refreshToken={tenderStateVersion}
                productionScheduling={productionScheduling}
              />
              <section
                className="document-register"
                aria-labelledby="document-register-title"
              >
                <div className="document-register__heading">
                  <div>
                    <p className="section-label">Canonical evidence</p>
                    <h4 id="document-register-title">Document Register</h4>
                  </div>
                  <span>{documentRegister?.documents.length ?? 0} entries</span>
                </div>
                {documentRegister && documentRegister.documents.length > 0 ? (
                  <DocumentEvidenceOffice
                    key={selected.tender_id}
                    tenderId={selected.tender_id}
                    runtimeReady={runtimeReady}
                    register={documentRegister}
                    updateRegister={setDocumentRegister}
                    reportCommandFailure={reportCommandFailure}
                  />
                ) : (
                  <p className="catalogue-message">
                    No source documents registered. Intake opens the empty Query
                    Register and records every accepted document or exception.
                  </p>
                )}
                {registeredDocuments.length >= 2 ? (
                  <form
                    className="relationship-form"
                    onSubmit={handleRelationshipConfirmation}
                  >
                    <div className="relationship-form__heading">
                      <div>
                        <p className="section-label">Engineer decision</p>
                        <h5>Confirm source relationship</h5>
                      </div>
                      <p>
                        Quantix never infers supersession. Confirm only a known
                        addendum or replacement.
                      </p>
                    </div>
                    <div className="relationship-form__fields">
                      <label>
                        Prior version
                        <select
                          value={priorVersionKey}
                          onChange={(event) =>
                            setPriorVersionKey(event.target.value)
                          }
                          disabled={busy || !runtimeReady}
                        >
                          <option value="">Select prior document</option>
                          {registeredDocuments.map((document) => (
                            <option
                              key={`prior-${versionKey(
                                document.artifact_id,
                                document.version,
                              )}`}
                              value={versionKey(
                                document.artifact_id,
                                document.version,
                              )}
                            >
                              {document.package_path} · v{document.version}
                            </option>
                          ))}
                        </select>
                      </label>
                      <label>
                        Relationship
                        <select
                          value={relationshipKind}
                          onChange={(event) =>
                            setRelationshipKind(
                              event.target.value as SourceRelationshipKind,
                            )
                          }
                          disabled={busy || !runtimeReady}
                        >
                          <option value="replacement">Replacement</option>
                          <option value="addendum">Addendum</option>
                        </select>
                      </label>
                      <label>
                        New version
                        <select
                          value={replacementVersionKey}
                          onChange={(event) =>
                            setReplacementVersionKey(event.target.value)
                          }
                          disabled={busy || !runtimeReady}
                        >
                          <option value="">Select new document</option>
                          {registeredDocuments.map((document) => (
                            <option
                              key={`replacement-${versionKey(
                                document.artifact_id,
                                document.version,
                              )}`}
                              value={versionKey(
                                document.artifact_id,
                                document.version,
                              )}
                            >
                              {document.package_path} · v{document.version}
                            </option>
                          ))}
                        </select>
                      </label>
                    </div>
                    <button
                      type="submit"
                      disabled={
                        busy ||
                        !runtimeReady ||
                        !priorVersionKey ||
                        !replacementVersionKey ||
                        priorVersionKey === replacementVersionKey
                      }
                    >
                      Confirm relationship
                    </button>
                  </form>
                ) : null}
              </section>
              <TenderRecordsPanel
                key={`records-${selected.tender_id}`}
                tenderId={selected.tender_id}
                runtimeReady={runtimeReady}
                register={documentRegister}
                reportCommandFailure={reportCommandFailure}
              />
              <TenderQueryRegisterPanel
                key={`queries-${selected.tender_id}`}
                tenderId={selected.tender_id}
                runtimeReady={runtimeReady}
                refreshToken={tenderStateVersion}
                reportCommandFailure={reportCommandFailure}
                onTenderStateChange={reportTenderStateChange}
              />
              <ExternalRfiPanel
                key={`external-rfis-${selected.tender_id}`}
                tenderId={selected.tender_id}
                runtimeReady={runtimeReady}
                register={documentRegister}
                refreshToken={tenderStateVersion}
                reportCommandFailure={reportCommandFailure}
                onTenderStateChange={reportTenderStateChange}
              />
              <BidDecisionPanel
                key={`bid-decision-${selected.tender_id}`}
                tenderId={selected.tender_id}
                runtimeReady={runtimeReady}
                reportCommandFailure={reportCommandFailure}
                onTenderStateChange={reportTenderStateChange}
              />
              <TenderOfficePanel
                key={`tender-office-${selected.tender_id}`}
                tenderId={selected.tender_id}
                runtimeReady={runtimeReady}
                reportCommandFailure={reportCommandFailure}
                refreshToken={tenderStateVersion}
                onTenderStateChange={reportTenderStateChange}
                onProductionSchedulingChange={setProductionScheduling}
              />
            </>
          ) : recovery ? (
            <>
              <TenderRecoveryPanel
                report={recovery}
                refreshing={busy}
                onRefresh={() => void inspectRecovery(recovery.tender_id)}
              />
              <TenderBackupPanel
                tenderId={recovery.tender_id}
                backups={backups}
                recoveries={recoveries}
                busy={busy}
                canCreate={false}
                onCreate={() => void handleCreateBackup(recovery.tender_id)}
                onPrepare={(backupId) =>
                  void handlePrepareRecovery(recovery.tender_id, backupId)
                }
                onResolve={(recoveryId, decision, rationale) =>
                  void handleResolveRecovery(
                    recovery.tender_id,
                    recoveryId,
                    decision,
                    rationale,
                  )
                }
              />
            </>
          ) : (
            <div className="tender-detail__empty">
              <p className="section-label">Tender detail</p>
              <h3>
                Select a Tender to inspect its current canonical revision.
              </h3>
            </div>
          )}
          {commandFailed ? (
            <p className="catalogue-error" role="alert">
              Quantix did not change the Tender. Review the command and try
              again.
            </p>
          ) : null}
        </aside>
      </div>
    </section>
  );
}
