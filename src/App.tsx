import { CircleAlert, LoaderCircle } from "lucide-react";
import { useCallback, useEffect, useState } from "react";

import type { SetupIssue } from "./bindings/SetupIssue";
import { ManagerWorkspace } from "./ManagerWorkspace";
import {
  ensureQuantixSetup,
  inspectRuntimeReadiness,
  validateQuantixUpdateRestart,
} from "./quantixHost";
import "./App.css";

type AppState =
  | { kind: "checking" }
  | { kind: "ready"; aiAvailable: boolean }
  | { kind: "blocked"; title: string; summary: string };

const setupIssueCopy: Record<SetupIssue, string> = {
  application_home_unavailable: "The local Quantix workspace is unavailable.",
  device_protection_disabled: "Device storage protection must be enabled.",
  device_protection_unverified:
    "Quantix could not verify device storage protection.",
  installation_catalogue_corrupt:
    "The local Quantix installation record needs repair.",
  insufficient_free_space: "This device needs at least 1 GB of free space.",
  storage_not_writable: "Quantix cannot write to its local workspace.",
  storage_permissions_unverified:
    "Quantix could not verify local storage permissions.",
  unrecognized_application_home:
    "Existing unrecognized Quantix data was preserved for review.",
  unsafe_storage_location:
    "The Quantix workspace uses an unsafe linked location.",
  unsafe_storage_permissions:
    "The Quantix workspace can be accessed by other device users.",
  unsupported_installation_version:
    "Install a compatible Quantix version to open this workspace.",
  update_installation_active:
    "Finish or repair the active Quantix update before continuing.",
};

function App() {
  const [state, setState] = useState<AppState>({ kind: "checking" });

  const openWorkspace = useCallback(async () => {
    setState({ kind: "checking" });
    try {
      const setup = await ensureQuantixSetup();
      if (setup.state !== "ready" && setup.state !== "warning") {
        setState({
          kind: "blocked",
          title: "Quantix needs attention",
          summary:
            setup.issues.map((issue) => setupIssueCopy[issue]).join(" ") ||
            "The local workspace could not be opened safely.",
        });
        return;
      }

      const [runtimeResult, updateResult] = await Promise.allSettled([
        inspectRuntimeReadiness(),
        validateQuantixUpdateRestart(),
      ]);
      if (updateResult.status === "rejected") {
        setState({
          kind: "blocked",
          title: "Quantix update state is unavailable",
          summary:
            "Quantix could not verify whether an update needs recovery. Nothing was changed; try again before continuing.",
        });
        return;
      }
      if (
        [
          "installing",
          "restart_validation_required",
          "repair_required",
        ].includes(updateResult.value.state)
      ) {
        setState({
          kind: "blocked",
          title: "Finish the Quantix update",
          summary:
            "Tender records are protected while the signed application update is completed or repaired.",
        });
        return;
      }
      setState({
        kind: "ready",
        aiAvailable:
          runtimeResult.status === "fulfilled" &&
          runtimeResult.value.state === "ready",
      });
    } catch {
      setState({
        kind: "blocked",
        title: "Quantix could not open",
        summary:
          "The local Host did not respond. Nothing was changed; try opening the workspace again.",
      });
    }
  }, []);

  useEffect(() => {
    void openWorkspace();
  }, [openWorkspace]);

  if (state.kind === "checking") {
    return (
      <main className="quantix-startup" aria-live="polite">
        <span className="quantix-startup__mark">Q</span>
        <h1>Quantix</h1>
        <p>
          <LoaderCircle size={16} aria-hidden="true" />
          Opening your workspace…
        </p>
      </main>
    );
  }

  if (state.kind === "blocked") {
    return (
      <main className="quantix-blocked">
        <CircleAlert size={23} aria-hidden="true" />
        <h1>{state.title}</h1>
        <p>{state.summary}</p>
        <button type="button" onClick={() => void openWorkspace()}>
          Try again
        </button>
      </main>
    );
  }

  return <ManagerWorkspace aiAvailable={state.aiAvailable} />;
}

export default App;
