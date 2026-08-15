import { Check, CircleAlert, LoaderCircle } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";

import type { SetupIssue } from "./bindings/SetupIssue";
import type { GeneralApplicationPreferences } from "./bindings/GeneralApplicationPreferences";
import {
  applyGeneralApplicationPreferences,
  DEFAULT_GENERAL_APPLICATION_PREFERENCES,
} from "./applicationPreferences";
import { ManagerWorkspace } from "./ManagerWorkspace";
import {
  ensureQuantixSetup,
  inspectRuntimeReadiness,
  refreshApplicationSettings,
  repairRuntimeReadiness,
  resumeManagerIntakes,
  validateQuantixUpdateRestart,
} from "./quantixHost";
import "./App.css";

type AppState =
  | { kind: "checking"; stage: StartupStage }
  | {
      kind: "ready";
      aiAvailable: boolean;
      generalPreferences: GeneralApplicationPreferences;
    }
  | { kind: "blocked"; title: string; summary: string };

type StartupStage =
  "workspace" | "runtime_check" | "runtime_install" | "providers" | "opening";

const startupStageCopy: Record<
  StartupStage,
  { title: string; summary: string; step: number }
> = {
  workspace: {
    title: "Checking workspace",
    summary: "Verifying local storage and recovery state.",
    step: 0,
  },
  runtime_check: {
    title: "Checking local AI tools",
    summary: "Verifying the pinned Codex, Python, and Docling runtime.",
    step: 1,
  },
  runtime_install: {
    title: "Installing local AI tools",
    summary:
      "First-time setup is preparing Python, Docling, and local document models. This can take several minutes.",
    step: 1,
  },
  providers: {
    title: "Checking AI connections",
    summary: "Loading available providers, models, and reasoning options.",
    step: 2,
  },
  opening: {
    title: "Opening workspace",
    summary: "Restoring your last active Tender and Manager conversation.",
    step: 3,
  },
};

const startupSteps = [
  "Workspace",
  "Local AI tools",
  "AI connections",
  "Workspace ready",
];

const RUNTIME_PREPARATION_POLL_MS = 500;

async function waitForRuntimePreparation() {
  let readiness = await inspectRuntimeReadiness();
  while (readiness.state === "preparing") {
    await new Promise((resolve) =>
      window.setTimeout(resolve, RUNTIME_PREPARATION_POLL_MS),
    );
    readiness = await inspectRuntimeReadiness();
  }
  return readiness;
}

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
  const [state, setState] = useState<AppState>({
    kind: "checking",
    stage: "workspace",
  });
  const startupStarted = useRef(false);

  const openWorkspace = useCallback(async () => {
    setState({ kind: "checking", stage: "workspace" });
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

      setState({ kind: "checking", stage: "runtime_check" });
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
      let runtimeReadiness =
        runtimeResult.status === "fulfilled" ? runtimeResult.value : undefined;
      if (
        runtimeReadiness &&
        runtimeReadiness.state !== "ready" &&
        runtimeReadiness.repair_available
      ) {
        setState({ kind: "checking", stage: "runtime_install" });
        runtimeReadiness = await repairRuntimeReadiness();
      }
      if (runtimeReadiness?.state === "preparing") {
        setState({ kind: "checking", stage: "runtime_install" });
        runtimeReadiness = await waitForRuntimePreparation();
      }
      setState({ kind: "checking", stage: "providers" });
      const settings = await refreshApplicationSettings().catch(() => null);
      if (settings) {
        applyGeneralApplicationPreferences(settings.general_preferences);
      }
      const aiAvailable = settings
        ? settings.provider_connections.some(
            (connection) => connection.status === "ready",
          )
        : runtimeReadiness?.state === "ready";
      setState({ kind: "checking", stage: "opening" });
      await new Promise<void>((resolve) =>
        window.requestAnimationFrame(() => resolve()),
      );
      setState({
        kind: "ready",
        aiAvailable,
        generalPreferences:
          settings?.general_preferences ??
          DEFAULT_GENERAL_APPLICATION_PREFERENCES,
      });
      if (aiAvailable) {
        void resumeManagerIntakes().catch(() => undefined);
      }
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
    if (startupStarted.current) {
      return;
    }
    startupStarted.current = true;
    void openWorkspace();
  }, [openWorkspace]);

  if (state.kind === "checking") {
    const stage = state.stage ?? "runtime_install";
    const stageCopy = startupStageCopy[stage];
    return (
      <main className="quantix-startup" aria-busy="true">
        <span className="quantix-startup__mark">Q</span>
        <h1>Quantix</h1>
        <div
          className="quantix-startup__status"
          role="status"
          aria-live="polite"
        >
          <LoaderCircle size={16} aria-hidden="true" />
          <div>
            <strong>{stageCopy.title}</strong>
            <span>{stageCopy.summary}</span>
          </div>
        </div>
        <ol
          className="quantix-startup__steps"
          aria-label="Workspace opening progress"
        >
          {startupSteps.map((label, index) => {
            const status =
              index < stageCopy.step
                ? "complete"
                : index === stageCopy.step
                  ? "current"
                  : "pending";
            return (
              <li className={`is-${status}`} key={label}>
                <span aria-hidden="true">
                  {status === "complete" ? <Check size={12} /> : index + 1}
                </span>
                {label}
              </li>
            );
          })}
        </ol>
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

  return (
    <ManagerWorkspace
      aiAvailable={state.aiAvailable}
      initialPreferences={state.generalPreferences}
    />
  );
}

export default App;
