import { FormEvent, useEffect, useState } from "react";

import type { TenderSummary } from "./bindings/TenderSummary";
import {
  createTender,
  listTenders,
  openTender,
  reviseTender,
} from "./quantixHost";

type CatalogueState =
  | { kind: "loading" }
  | { kind: "ready"; tenders: TenderSummary[] }
  | { kind: "error" };

export function TenderWorkspace() {
  const [catalogue, setCatalogue] = useState<CatalogueState>({
    kind: "loading",
  });
  const [selected, setSelected] = useState<TenderSummary>();
  const [newName, setNewName] = useState("");
  const [revisionName, setRevisionName] = useState("");
  const [busy, setBusy] = useState(false);
  const [commandFailed, setCommandFailed] = useState(false);

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
      const tenders = current.tenders
        .filter((candidate) => candidate.tender_id !== tender.tender_id)
        .concat(tender)
        .sort((left, right) => left.name.localeCompare(right.name));
      return { kind: "ready", tenders };
    });
    setSelected(tender);
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
      if (created) setNewName("");
    });
  };

  const handleRevise = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const name = revisionName.trim();
    if (!selected || !name) return;
    void runCommand(() => reviseTender(selected.tender_id, name));
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
                disabled={busy}
              />
              <button type="submit" disabled={busy || !newName.trim()}>
                Create Tender
              </button>
            </div>
          </form>

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
                      selected?.tender_id === tender.tender_id
                        ? "tender-row tender-row--selected"
                        : "tender-row"
                    }
                    onClick={() =>
                      void runCommand(() => openTender(tender.tender_id))
                    }
                    disabled={busy}
                  >
                    <span>{tender.name}</span>
                    <small>
                      Revision {tender.revision} · {tender.audit_event_count}{" "}
                      audit events
                    </small>
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
                  <dt>Chain head</dt>
                  <dd>{selected.audit_chain_head.slice(0, 12)}…</dd>
                </div>
              </dl>
              <form className="tender-form" onSubmit={handleRevise}>
                <label htmlFor="revised-tender-name">Revise Tender name</label>
                <input
                  id="revised-tender-name"
                  value={revisionName}
                  onChange={(event) => setRevisionName(event.target.value)}
                  maxLength={200}
                  autoComplete="off"
                  disabled={busy}
                />
                <button type="submit" disabled={busy || !revisionName.trim()}>
                  Save immutable revision
                </button>
              </form>
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
