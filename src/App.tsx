import { useCallback, useEffect, useState } from "react";

import type { HostCommandInterface } from "./bindings/HostCommandInterface";
import type { HostRuntime } from "./bindings/HostRuntime";
import type { QuantixHostStatus } from "./bindings/QuantixHostStatus";
import type { RendererAssetSource } from "./bindings/RendererAssetSource";
import { inspectQuantixHost } from "./quantixHost";
import "./App.css";

type ConnectionView =
  | { kind: "checking" }
  | { kind: "connected"; status: QuantixHostStatus }
  | { kind: "error" };

const runtimeLabels: Record<HostRuntime, string> = {
  local_tauri_desktop: "Local Tauri desktop",
};

const commandInterfaceLabels: Record<HostCommandInterface, string> = {
  named_domain_commands: "Named domain commands",
};

const rendererAssetLabels: Record<RendererAssetSource, string> = {
  bundled_local: "Bundled local interface",
};

function App() {
  const [connection, setConnection] = useState<ConnectionView>({
    kind: "checking",
  });

  const checkConnection = useCallback(async () => {
    setConnection({ kind: "checking" });

    try {
      const status = await inspectQuantixHost();
      setConnection({ kind: "connected", status });
    } catch {
      setConnection({ kind: "error" });
    }
  }, []);

  useEffect(() => {
    void checkConnection();
  }, [checkConnection]);

  const connected = connection.kind === "connected";
  const checking = connection.kind === "checking";

  return (
    <div className="app-shell">
      <header className="app-header">
        <span className="wordmark">Quantix</span>
        <span className="environment">Local desktop</span>
      </header>

      <main className="connection-layout">
        <section className="introduction" aria-labelledby="page-title">
          <h1 id="page-title">Tender office control plane</h1>
          <p>
            Quantix is running locally. The trusted Rust Host owns domain
            operations and is ready to receive commands.
          </p>
          <button
            className="connection-button"
            type="button"
            onClick={() => void checkConnection()}
            disabled={checking}
          >
            {checking ? "Checking connection…" : "Check connection"}
          </button>
        </section>

        <section className="status-panel" aria-labelledby="host-status-title">
          <div className="status-heading" aria-live="polite">
            <span
              className={`status-indicator status-indicator--${connection.kind}`}
              aria-hidden="true"
            />
            <h2 id="host-status-title">
              {connected
                ? "Rust Host connected"
                : checking
                  ? "Checking Rust Host"
                  : "Rust Host unavailable"}
            </h2>
          </div>

          {connected ? (
            <dl className="host-facts">
              <div>
                <dt>Runtime</dt>
                <dd>{runtimeLabels[connection.status.runtime]}</dd>
              </div>
              <div>
                <dt>Interface</dt>
                <dd>
                  {commandInterfaceLabels[connection.status.command_interface]}
                </dd>
              </div>
              <div>
                <dt>Renderer</dt>
                <dd>
                  {rendererAssetLabels[connection.status.renderer_assets]}
                </dd>
              </div>
            </dl>
          ) : null}

          {checking ? (
            <p className="status-message">
              <span className="spinner" aria-hidden="true" />
              Checking connection…
            </p>
          ) : null}

          {connection.kind === "error" ? (
            <p className="error-message" role="alert">
              The local Rust Host did not respond. Restart Quantix and try
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
