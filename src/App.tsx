import { useCallback, useEffect, useState } from "react";

import type { SetupIssue } from "./bindings/SetupIssue";
import type { SetupOutcome } from "./bindings/SetupOutcome";
import type { SetupState } from "./bindings/SetupState";
import { ensureQuantixSetup } from "./quantixHost";
import { TenderWorkspace } from "./TenderWorkspace";
import "./App.css";

type SetupView =
  | { kind: "checking" }
  | { kind: "outcome"; outcome: SetupOutcome }
  | { kind: "error" };

const stateCopy: Record<SetupState, { title: string; summary: string }> = {
  ready: {
    title: "Tender office ready",
    summary:
      "The local Quantix foundation is ready for Engineer-controlled work.",
  },
  warning: {
    title: "Ready with warnings",
    summary:
      "Setup completed, but the Engineer should review the checks below.",
  },
  authentication_required: {
    title: "Authentication required",
    summary:
      "A connected service requires the Engineer to sign in before work can continue.",
  },
  missing_capability: {
    title: "Capability required",
    summary: "This device is missing a capability required by Quantix.",
  },
  unsupported_version: {
    title: "Update required",
    summary:
      "This Application Home was created by a newer, unsupported Quantix version.",
  },
  repair_required: {
    title: "Repair required",
    summary:
      "Quantix stopped safely without changing the existing Application Home.",
  },
};

const issueCopy: Record<SetupIssue, string> = {
  application_home_unavailable: "The local Application Home is unavailable.",
  device_protection_disabled: "Device storage protection is disabled.",
  device_protection_unverified:
    "Device storage protection could not be verified.",
  installation_catalogue_corrupt:
    "The installation catalogue is incomplete or corrupt.",
  insufficient_free_space:
    "At least 1 GB of free space is required for first-run setup.",
  storage_not_writable: "Quantix cannot write to its local Application Home.",
  storage_permissions_unverified:
    "Local storage permissions could not be verified.",
  unrecognized_application_home:
    "Existing, unrecognized Quantix data was preserved for Engineer review.",
  unsafe_storage_location:
    "The Application Home resolves through an unsafe linked storage location.",
  unsafe_storage_permissions:
    "Local storage permissions allow access beyond the current Engineer.",
  unsupported_installation_version:
    "Install a compatible Quantix version to use this Application Home.",
};

function App() {
  const [setup, setSetup] = useState<SetupView>({ kind: "checking" });

  const runSetup = useCallback(async () => {
    setSetup({ kind: "checking" });

    try {
      setSetup({ kind: "outcome", outcome: await ensureQuantixSetup() });
    } catch {
      setSetup({ kind: "error" });
    }
  }, []);

  useEffect(() => {
    void runSetup();
  }, [runSetup]);

  const outcome = setup.kind === "outcome" ? setup.outcome : undefined;
  const copy = outcome ? stateCopy[outcome.state] : undefined;

  return (
    <div className="app-shell">
      <header className="app-header">
        <span className="wordmark">Quantix</span>
        <span className="environment">Local desktop</span>
      </header>

      <main className="connection-layout">
        <section className="introduction" aria-labelledby="page-title">
          <p className="eyebrow">First-run setup</p>
          <h1 id="page-title">Engineer-controlled tender office</h1>
          <p>
            Quantix verifies its private local foundation before any tender work
            begins. The Engineer remains in control of every consequential
            action.
          </p>
          <button
            className="connection-button"
            type="button"
            onClick={() => void runSetup()}
            disabled={setup.kind === "checking"}
          >
            {setup.kind === "checking" ? "Checking setup…" : "Run setup checks"}
          </button>
        </section>

        <section className="status-panel" aria-labelledby="setup-title">
          <div className="status-heading" aria-live="polite">
            <span
              className={`status-indicator status-indicator--${outcome?.state ?? setup.kind}`}
              aria-hidden="true"
            />
            <div>
              <p className="status-kicker">Application Home</p>
              <h2 id="setup-title">
                {copy?.title ??
                  (setup.kind === "checking"
                    ? "Checking local setup"
                    : "Setup unavailable")}
              </h2>
            </div>
          </div>

          {outcome && copy ? (
            <div className="setup-outcome">
              <p className="outcome-summary">{copy.summary}</p>
              {outcome.issues.length > 0 ? (
                <ul className="issue-list">
                  {outcome.issues.map((issue) => (
                    <li key={issue}>{issueCopy[issue]}</li>
                  ))}
                </ul>
              ) : (
                <p className="success-message">
                  {outcome.setup_performed
                    ? "Application Home created and verified."
                    : "Existing Application Home verified."}
                </p>
              )}
            </div>
          ) : null}

          {setup.kind === "checking" ? (
            <p className="status-message">
              <span className="spinner" aria-hidden="true" />
              Verifying storage, permissions, and installation state…
            </p>
          ) : null}

          {setup.kind === "error" ? (
            <p className="error-message" role="alert">
              The local Quantix host did not respond. Restart the dev app and
              try again.
            </p>
          ) : null}
        </section>
      </main>

      {outcome && (outcome.state === "ready" || outcome.state === "warning") ? (
        <TenderWorkspace />
      ) : null}

      <div className="structural-rail" aria-hidden="true" />
    </div>
  );
}

export default App;
