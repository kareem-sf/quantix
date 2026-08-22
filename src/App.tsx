import { CircleAlert, LoaderCircle } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

import type { ManagerWorkspaceProjection } from "./bindings/ManagerWorkspaceProjection";
import type { GeneralApplicationPreferences } from "./bindings/GeneralApplicationPreferences";
import type { SetupIssue } from "./bindings/SetupIssue";
import { ManagerWorkspace } from "./ManagerWorkspace";
import { QuantixMark } from "./QuantixMark";
import { QuantixWindow } from "./QuantixWindow";
import type { WindowTitleBarMenu } from "./WindowTitleBar";
import {
  applyGeneralApplicationPreferences,
  DEFAULT_GENERAL_APPLICATION_PREFERENCES,
} from "./applicationPreferences";
import {
  ensureQuantixSetup,
  inspectApplicationSettings,
  inspectManagerWorkspace,
  validateQuantixUpdateRestart,
} from "./quantixHost";
import "./App.css";

type AppState =
  | { kind: "opening"; showStatus: boolean; stage: OpeningStage }
  | {
      kind: "ready";
      initialProjection: ManagerWorkspaceProjection;
      initialPreferences: GeneralApplicationPreferences;
      setupWarnings: SetupIssue[];
    }
  | { kind: "blocked"; title: string; summary: string };

type OpeningStage = "workspace" | "restoring";

const OPENING_STATUS_DELAY_MS = 700;

const INACTIVE_TITLE_BAR_MENUS: readonly WindowTitleBarMenu[] = [
  "File",
  "Edit",
  "View",
  "Help",
].map((label) => ({
  id: label.toLowerCase(),
  label,
  items: [
    {
      id: `${label.toLowerCase()}-unavailable`,
      label: "Available after the workspace opens",
      disabled: true,
    },
  ],
}));

const openingCopy: Record<OpeningStage, string> = {
  workspace: "Opening the local workspace…",
  restoring: "Restoring your Tender workspace…",
};

function setupIssueCopy(issue: SetupIssue): string {
  switch (issue) {
    case "application_home_unavailable":
      return "The local Quantix workspace is unavailable.";
    case "installation_catalogue_corrupt":
      return "The local Quantix installation record needs repair.";
    case "storage_not_writable":
      return "Quantix cannot write to its local workspace.";
    case "storage_permissions_unverified":
      return "Quantix could not verify local storage permissions.";
    case "unrecognized_application_home":
      return "Existing unrecognized Quantix data was preserved for review.";
    case "unsafe_storage_location":
      return "The Quantix workspace uses an unsafe linked location.";
    case "unsafe_storage_permissions":
      return "The Quantix workspace can be accessed by other device users.";
    case "unsupported_installation_version":
      return "Install a compatible Quantix version to open this workspace.";
    case "update_installation_active":
      return "Finish or repair the active Quantix update before continuing.";
    default:
      return "Review the local workspace security warning before adding confidential Tender material.";
  }
}

function runningInTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

function notifyStartupReady() {
  if (!runningInTauri()) return;
  void invoke("notify_startup_display_ready").catch(() => undefined);
}

function reportStartupPreferences(preferences: { reducedMotion: boolean }) {
  if (!runningInTauri()) return;
  void invoke("report_startup_splash_preferences", { preferences }).catch(
    () => undefined,
  );
}

function App() {
  const [state, setState] = useState<AppState>({
    kind: "opening",
    showStatus: false,
    stage: "workspace",
  });
  const startupStarted = useRef(false);
  const startupDisplayReadyNotified = useRef(false);
  const statusTimer = useRef<number | null>(null);

  const openWorkspace = useCallback(async () => {
    if (statusTimer.current !== null) {
      window.clearTimeout(statusTimer.current);
    }
    setState({ kind: "opening", showStatus: false, stage: "workspace" });
    statusTimer.current = window.setTimeout(() => {
      setState((current) =>
        current.kind === "opening" ? { ...current, showStatus: true } : current,
      );
    }, OPENING_STATUS_DELAY_MS);

    try {
      const setup = await ensureQuantixSetup();
      if (setup.state !== "ready" && setup.state !== "warning") {
        setState({
          kind: "blocked",
          title: "Quantix needs attention",
          summary:
            setup.issues.map(setupIssueCopy).join(" ") ||
            "The local workspace could not be opened safely.",
        });
        return;
      }

      setState((current) =>
        current.kind === "opening"
          ? { ...current, stage: "restoring" }
          : current,
      );
      const [updateResult, projectionResult, settingsResult] =
        await Promise.allSettled([
          validateQuantixUpdateRestart(),
          inspectManagerWorkspace(),
          inspectApplicationSettings(),
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
      if (projectionResult.status === "rejected") {
        setState({
          kind: "blocked",
          title: "Tender workspace unavailable",
          summary:
            "Quantix could not read the local Tender workspace. Nothing was changed; try again before continuing.",
        });
        return;
      }

      const initialPreferences =
        settingsResult.status === "fulfilled"
          ? settingsResult.value.general_preferences
          : DEFAULT_GENERAL_APPLICATION_PREFERENCES;
      applyGeneralApplicationPreferences(initialPreferences);
      reportStartupPreferences({
        reducedMotion: initialPreferences.reduced_motion,
      });

      setState({
        kind: "ready",
        initialProjection: projectionResult.value,
        initialPreferences,
        setupWarnings: setup.state === "warning" ? setup.issues : [],
      });
    } catch {
      setState({
        kind: "blocked",
        title: "Quantix could not open",
        summary:
          "The local Host did not respond. Nothing was changed; try opening the workspace again.",
      });
    } finally {
      if (statusTimer.current !== null) {
        window.clearTimeout(statusTimer.current);
        statusTimer.current = null;
      }
    }
  }, []);

  useEffect(() => {
    if (startupDisplayReadyNotified.current) return;
    startupDisplayReadyNotified.current = true;
    notifyStartupReady();
  }, []);

  useEffect(() => {
    if (startupStarted.current) return;
    startupStarted.current = true;
    void openWorkspace();
    return () => {
      if (statusTimer.current !== null) {
        window.clearTimeout(statusTimer.current);
      }
    };
  }, [openWorkspace]);

  if (state.kind === "opening") {
    return (
      <QuantixWindow
        menus={INACTIVE_TITLE_BAR_MENUS}
        canToggleSidebar={false}
        motionState="static"
      >
        <main className="quantix-startup" aria-busy="true">
          <div className="quantix-startup__brand" aria-label="Quantix">
            <QuantixMark className="quantix-startup__mark" />
            <span>Quantix</span>
          </div>
          {state.showStatus ? (
            <p className="quantix-startup__status" role="status">
              <LoaderCircle size={15} aria-hidden="true" />
              {openingCopy[state.stage]}
            </p>
          ) : null}
        </main>
      </QuantixWindow>
    );
  }

  if (state.kind === "blocked") {
    return (
      <QuantixWindow
        menus={INACTIVE_TITLE_BAR_MENUS}
        canToggleSidebar={false}
        motionState="static"
      >
        <main className="quantix-blocked">
          <div className="quantix-blocked__brand" aria-label="Quantix">
            <QuantixMark className="quantix-blocked__mark" />
            <span>Quantix</span>
          </div>
          <CircleAlert size={23} aria-hidden="true" />
          <h1>{state.title}</h1>
          <p>{state.summary}</p>
          <button type="button" onClick={() => void openWorkspace()}>
            Try again
          </button>
        </main>
      </QuantixWindow>
    );
  }

  return (
    <ManagerWorkspace
      initialProjection={state.initialProjection}
      initialPreferences={state.initialPreferences}
      setupWarnings={state.setupWarnings}
    />
  );
}

export default App;
