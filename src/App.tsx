import {
  Check,
  ChevronDown,
  Circle,
  CircleAlert,
  CircleX,
  LoaderCircle,
} from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";

import type { SetupIssue } from "./bindings/SetupIssue";
import type { GeneralApplicationPreferences } from "./bindings/GeneralApplicationPreferences";
import type { RuntimePreparationActivity } from "./bindings/RuntimePreparationActivity";
import type { RuntimePreparationProgress } from "./bindings/RuntimePreparationProgress";
import {
  applyGeneralApplicationPreferences,
  DEFAULT_GENERAL_APPLICATION_PREFERENCES,
} from "./applicationPreferences";
import { ManagerWorkspace } from "./ManagerWorkspace";
import {
  ensureQuantixSetup,
  inspectRuntimePreparationProgress,
  inspectRuntimeReadiness,
  refreshApplicationSettings,
  repairRuntimeReadiness,
  resumeManagerIntakes,
  validateQuantixUpdateRestart,
} from "./quantixHost";
import "./App.css";

type AppState =
  | {
      kind: "checking";
      stage: StartupStage;
      runtimeProgress?: RuntimePreparationProgress;
    }
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

const waitForPoll = () =>
  new Promise<void>((resolve) =>
    window.setTimeout(resolve, RUNTIME_PREPARATION_POLL_MS),
  );

async function readRuntimeProgress(
  accept: (progress: RuntimePreparationProgress) => void,
) {
  try {
    accept(await inspectRuntimePreparationProgress());
  } catch {
    // The readiness command remains authoritative if progress inspection is
    // momentarily unavailable during a development Host restart.
  }
}

async function repairRuntimeWithProgress(
  accept: (progress: RuntimePreparationProgress) => void,
) {
  let settled = false;
  const repair = repairRuntimeReadiness();
  const settlement = repair.then(
    () => {
      settled = true;
    },
    () => {
      settled = true;
    },
  );
  while (!settled) {
    await readRuntimeProgress(accept);
    if (!settled) {
      await Promise.race([waitForPoll(), settlement]);
    }
  }
  await readRuntimeProgress(accept);
  return repair;
}

async function waitForRuntimePreparation(
  accept: (progress: RuntimePreparationProgress) => void,
) {
  let readiness = await inspectRuntimeReadiness();
  while (readiness.state === "preparing") {
    await readRuntimeProgress(accept);
    await waitForPoll();
    readiness = await inspectRuntimeReadiness();
  }
  await readRuntimeProgress(accept);
  return readiness;
}

function formatElapsed(
  startedAt: number | bigint | null,
  endedAt: number | bigint | null,
  now: number,
) {
  if (startedAt === null) {
    return null;
  }
  const startedAtNumber = Number(startedAt);
  const endedAtNumber = endedAt === null ? now : Number(endedAt);
  const elapsedSeconds = Math.max(
    0,
    Math.floor((endedAtNumber - startedAtNumber) / 1000),
  );
  if (elapsedSeconds < 60) {
    return `${elapsedSeconds}s`;
  }
  const minutes = Math.floor(elapsedSeconds / 60);
  const seconds = elapsedSeconds % 60;
  return `${minutes}m ${seconds.toString().padStart(2, "0")}s`;
}

function formatBytes(bytes: number | bigint) {
  const bytesNumber = Number(bytes);
  if (bytesNumber < 1024 * 1024) {
    return `${Math.max(0, Math.round(bytesNumber / 1024))} KB`;
  }
  return `${(bytesNumber / (1024 * 1024)).toFixed(1)} MB`;
}

function RuntimeActivityIcon({
  activity,
}: {
  activity: RuntimePreparationActivity;
}) {
  if (activity.status === "complete") {
    return <Check size={12} aria-hidden="true" />;
  }
  if (activity.status === "active") {
    return <LoaderCircle size={13} aria-hidden="true" />;
  }
  if (activity.status === "failed") {
    return <CircleX size={13} aria-hidden="true" />;
  }
  return <Circle size={10} aria-hidden="true" />;
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
  const [clock, setClock] = useState(() => Date.now());
  const runtimeProgressActive =
    state.kind === "checking" &&
    state.stage === "runtime_install" &&
    state.runtimeProgress?.status === "preparing";

  useEffect(() => {
    if (!runtimeProgressActive) {
      return;
    }
    const timer = window.setInterval(() => setClock(Date.now()), 1000);
    return () => window.clearInterval(timer);
  }, [runtimeProgressActive]);

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
        runtimeReadiness = await repairRuntimeWithProgress((runtimeProgress) =>
          setState({
            kind: "checking",
            stage: "runtime_install",
            runtimeProgress,
          }),
        );
      }
      if (runtimeReadiness?.state === "preparing") {
        setState({ kind: "checking", stage: "runtime_install" });
        runtimeReadiness = await waitForRuntimePreparation((runtimeProgress) =>
          setState({
            kind: "checking",
            stage: "runtime_install",
            runtimeProgress,
          }),
        );
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
    const runtimeProgress = state.runtimeProgress;
    const activeRuntimeActivity = runtimeProgress?.activities.find(
      (activity) => activity.status === "active",
    );
    const totalElapsed = runtimeProgress
      ? formatElapsed(
          runtimeProgress.started_at_epoch_ms,
          runtimeProgress.status === "preparing"
            ? null
            : runtimeProgress.updated_at_epoch_ms,
          clock,
        )
      : null;
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
            <strong>{activeRuntimeActivity?.title ?? stageCopy.title}</strong>
            <span>{activeRuntimeActivity?.detail ?? stageCopy.summary}</span>
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
        {stage === "runtime_install" ? (
          <details className="quantix-startup__details" open>
            <summary>
              <span>
                Setup details
                {totalElapsed ? <small>{totalElapsed} elapsed</small> : null}
              </span>
              <ChevronDown size={15} aria-hidden="true" />
            </summary>
            {runtimeProgress ? (
              <>
                {runtimeProgress.model_files_written !== null &&
                runtimeProgress.model_bytes_written !== null ? (
                  <output className="quantix-startup__live-observation">
                    <LoaderCircle size={12} aria-hidden="true" />
                    {runtimeProgress.model_files_written.toLocaleString()} model
                    files · {formatBytes(runtimeProgress.model_bytes_written)}
                    written
                  </output>
                ) : null}
                <ol aria-label="Local AI setup activity">
                  {runtimeProgress.activities.map((activity) => {
                    const activityElapsed = formatElapsed(
                      activity.started_at_epoch_ms,
                      activity.finished_at_epoch_ms,
                      clock,
                    );
                    return (
                      <li
                        className={`is-${activity.status}`}
                        key={activity.step}
                      >
                        <span className="quantix-startup__activity-icon">
                          <RuntimeActivityIcon activity={activity} />
                        </span>
                        <div>
                          <strong>{activity.title}</strong>
                          <p>{activity.detail}</p>
                        </div>
                        {activityElapsed ? (
                          <time>{activityElapsed}</time>
                        ) : null}
                      </li>
                    );
                  })}
                </ol>
              </>
            ) : (
              <p className="quantix-startup__details-waiting">
                Waiting for the local Host to report its first operation...
              </p>
            )}
          </details>
        ) : null}
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
