import { useCallback, useEffect, useState } from "react";

import type { TenderOfficeReadiness } from "./bindings/TenderOfficeReadiness";
import { inspectTenderOfficeReadiness } from "./quantixHost";
import "./App.css";

type ReadinessView =
  | { kind: "checking" }
  | { kind: "ready"; readiness: TenderOfficeReadiness }
  | { kind: "error" };

const readinessLabels: Record<TenderOfficeReadiness, string> = {
  ready_for_setup: "Ready for first-run setup",
};

function App() {
  const [readiness, setReadiness] = useState<ReadinessView>({
    kind: "checking",
  });

  const checkReadiness = useCallback(async () => {
    setReadiness({ kind: "checking" });

    try {
      const outcome = await inspectTenderOfficeReadiness();
      setReadiness({ kind: "ready", readiness: outcome });
    } catch {
      setReadiness({ kind: "error" });
    }
  }, []);

  useEffect(() => {
    void checkReadiness();
  }, [checkReadiness]);

  const ready = readiness.kind === "ready";
  const checking = readiness.kind === "checking";

  return (
    <div className="app-shell">
      <header className="app-header">
        <span className="wordmark">Quantix</span>
        <span className="environment">Local desktop</span>
      </header>

      <main className="connection-layout">
        <section className="introduction" aria-labelledby="page-title">
          <h1 id="page-title">Engineer-controlled tender office</h1>
          <p>
            Quantix is running locally and is ready to establish the Tender
            Office under Engineer control.
          </p>
          <button
            className="connection-button"
            type="button"
            onClick={() => void checkReadiness()}
            disabled={checking}
          >
            {checking ? "Checking readiness…" : "Check readiness"}
          </button>
        </section>

        <section className="status-panel" aria-labelledby="readiness-title">
          <div className="status-heading" aria-live="polite">
            <span
              className={`status-indicator status-indicator--${readiness.kind}`}
              aria-hidden="true"
            />
            <h2 id="readiness-title">
              {ready
                ? "Tender office ready"
                : checking
                  ? "Checking tender office"
                  : "Tender office unavailable"}
            </h2>
          </div>

          {ready ? (
            <dl className="readiness-facts">
              <div>
                <dt>Next phase</dt>
                <dd>{readinessLabels[readiness.readiness]}</dd>
              </div>
            </dl>
          ) : null}

          {checking ? (
            <p className="status-message">
              <span className="spinner" aria-hidden="true" />
              Checking readiness…
            </p>
          ) : null}

          {readiness.kind === "error" ? (
            <p className="error-message" role="alert">
              The local Tender Office did not respond. Restart Quantix and try
              again.
            </p>
          ) : null}
        </section>
      </main>

      <div className="structural-rail" aria-hidden="true" />
    </div>
  );
}

export default App;
