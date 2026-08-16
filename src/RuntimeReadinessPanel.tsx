import { useCallback, useEffect, useState } from "react";

import type { RuntimeReadiness } from "./bindings/RuntimeReadiness";
import type { RuntimeReadinessIssue } from "./bindings/RuntimeReadinessIssue";
import type { RuntimeReadinessState } from "./bindings/RuntimeReadinessState";
import {
  cancelRuntimePreparation,
  inspectRuntimeReadiness,
  repairRuntimeReadiness,
} from "./quantixHost";

type RuntimeView =
  | { kind: "checking" }
  | { kind: "preparing" }
  | { kind: "outcome"; outcome: RuntimeReadiness }
  | { kind: "error" };

const stateCopy: Record<
  RuntimeReadinessState,
  { title: string; summary: string }
> = {
  ready: {
    title: "AI office ready",
    summary:
      "Codex subscription access and the private Docling runtime passed their readiness checks.",
  },
  preparing: {
    title: "Preparing AI office",
    summary: "Quantix is preparing the exact managed document runtime.",
  },
  missing_executable: {
    title: "Runtime component missing",
    summary: "A required, pinned runtime component is not available.",
  },
  incompatible_version: {
    title: "Runtime update required",
    summary:
      "A runtime component does not match the version approved for Quantix v0.",
  },
  missing_model: {
    title: "Document models required",
    summary:
      "The pinned Docling models must be prepared before tender documents can be read.",
  },
  authentication_required: {
    title: "Codex sign-in required",
    summary:
      "Open Codex and sign in with ChatGPT before tender work begins. Quantix uses the Codex-managed session and never asks for an API key or credentials.",
  },
  interrupted_preparation: {
    title: "Preparation was interrupted",
    summary:
      "Quantix stopped safely before publishing partial runtime readiness.",
  },
  repair_required: {
    title: "Runtime repair required",
    summary:
      "A supervised runtime check failed and tender work remains locked.",
  },
};

const issueCopy: Record<RuntimeReadinessIssue, string> = {
  setup_incomplete: "Complete Application Home setup first.",
  codex_executable_missing: "The approved Codex executable is missing.",
  uv_executable_missing: "The bundled uv runtime manager is missing.",
  docling_executable_missing:
    "The managed Docling environment has not been prepared.",
  codex_version_incompatible: "Codex does not match the approved version.",
  uv_version_incompatible: "uv does not match the approved version.",
  docling_version_incompatible: "Docling does not match the approved version.",
  runtime_resource_integrity_failed:
    "A bundled runtime file does not match its approved cryptographic digest.",
  docling_environment_invalid:
    "The locked Docling environment or managed Python has changed.",
  docling_models_missing:
    "One or more approved Docling model files are missing.",
  codex_authentication_required:
    "Open Codex, sign in with the Engineer User's ChatGPT subscription, then check again.",
  codex_subscription_required:
    "Codex is signed in without an eligible ChatGPT subscription. Connect the Engineer User's subscription in Codex, then check again.",
  runtime_preparation_active: "Runtime preparation is already in progress.",
  runtime_preparation_interrupted: "The previous preparation did not complete.",
  runtime_preparation_failed:
    "The last preparation attempt failed or was cancelled.",
  runtime_probe_failed:
    "A runtime returned an invalid or unsuccessful readiness response.",
};

interface RuntimeReadinessPanelProps {
  onReadyChange: (ready: boolean) => void;
}

export function RuntimeReadinessPanel({
  onReadyChange,
}: RuntimeReadinessPanelProps) {
  const [view, setView] = useState<RuntimeView>({ kind: "checking" });

  const accept = useCallback(
    (outcome: RuntimeReadiness) => {
      setView({ kind: "outcome", outcome });
      onReadyChange(outcome.state === "ready");
    },
    [onReadyChange],
  );

  const inspect = useCallback(async () => {
    setView({ kind: "checking" });
    onReadyChange(false);
    try {
      accept(await inspectRuntimeReadiness());
    } catch {
      setView({ kind: "error" });
    }
  }, [accept, onReadyChange]);

  const repair = useCallback(async () => {
    setView({ kind: "preparing" });
    onReadyChange(false);
    try {
      accept(await repairRuntimeReadiness());
    } catch {
      setView({ kind: "error" });
    }
  }, [accept, onReadyChange]);

  const cancel = useCallback(async () => {
    try {
      await cancelRuntimePreparation();
    } catch {
      setView({ kind: "error" });
    }
  }, []);

  useEffect(() => {
    void inspect();
  }, [inspect]);

  const outcome = view.kind === "outcome" ? view.outcome : undefined;
  const copy = outcome ? stateCopy[outcome.state] : undefined;
  const active = view.kind === "checking" || view.kind === "preparing";

  return (
    <section className="runtime-office" aria-labelledby="runtime-title">
      <div className="runtime-office__heading">
        <div>
          <p className="section-label">Runtime readiness</p>
          <h2 id="runtime-title">Supervised AI and document tools</h2>
        </div>
        <p>
          Quantix verifies exact versions, bounded process behavior, local
          models, and Codex subscription access before opening the tender
          office.
        </p>
      </div>

      <div className="runtime-status" aria-live="polite">
        <div className="status-heading">
          <span
            className={`status-indicator status-indicator--${outcome?.state ?? view.kind}`}
            aria-hidden="true"
          />
          <div>
            <p className="status-kicker">AI office</p>
            <h3>
              {copy?.title ??
                (view.kind === "preparing"
                  ? "Preparing managed runtime"
                  : view.kind === "checking"
                    ? "Checking runtime"
                    : "Runtime unavailable")}
            </h3>
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
                Codex, uv, Docling, and local model readiness verified.
              </p>
            )}
            <dl className="runtime-versions">
              <div>
                <dt>Codex</dt>
                <dd>{outcome.codex_version ?? "Not verified"}</dd>
              </div>
              <div>
                <dt>uv</dt>
                <dd>{outcome.uv_version ?? "Not verified"}</dd>
              </div>
              <div>
                <dt>Docling</dt>
                <dd>{outcome.docling_version ?? "Not verified"}</dd>
              </div>
            </dl>
          </div>
        ) : null}

        {active ? (
          <p className="status-message">
            <span className="spinner" aria-hidden="true" />
            {view.kind === "preparing"
              ? "Preparing exact runtime and model files…"
              : "Inspecting supervised runtime readiness…"}
          </p>
        ) : null}

        {view.kind === "error" ? (
          <p className="error-message" role="alert">
            The Quantix host did not return runtime readiness.
          </p>
        ) : null}

        <div className="runtime-actions">
          {view.kind === "preparing" ? (
            <button type="button" onClick={() => void cancel()}>
              Cancel preparation
            </button>
          ) : (
            <button
              type="button"
              onClick={() => void inspect()}
              disabled={active}
            >
              Check runtime
            </button>
          )}
          {outcome?.repair_available ? (
            <button
              className="runtime-actions__primary"
              type="button"
              onClick={() => void repair()}
              disabled={active}
            >
              Prepare and verify
            </button>
          ) : null}
        </div>
      </div>
    </section>
  );
}
