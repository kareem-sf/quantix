import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import "./splash.css";

type StartupPreferences = {
  reducedMotion: boolean;
};

type Point = readonly [x: number, y: number];

type EvidenceCue = {
  id: string;
  arrivalStartMs: number;
  arrivalOffsetPx: Point;
  calibrationStartMs: number;
  calibrationOffsetPx: Point;
};

// The splash is a brief handoff, not a startup progress meter. The native
// watchdog remains the failure cap when the renderer or Host never becomes
// ready; a healthy shell should be visible in about one second.
const REFERENCE_ASSEMBLY_COMPLETE_MS = 6250;
const ASSEMBLY_COMPLETE_MS = 900;
const HANDOFF_START_MS = 960;
const TOTAL_DURATION_MS = 1080;
const HANDOFF_FADE_MS = TOTAL_DURATION_MS - HANDOFF_START_MS;
const ARRIVAL_DURATION_MS = 820;
const CALIBRATION_DURATION_MS = 470;

const ARRIVAL_EASING = "cubic-bezier(0.22, 1, 0.36, 1)";
const CALIBRATION_EASING = "cubic-bezier(0.33, 1, 0.68, 1)";

// The cues are intentionally bespoke rather than a generic stagger. Their
// second pass resolves from the inner verticals outward into a stable register.
const EVIDENCE_CUES: readonly EvidenceCue[] = [
  {
    id: "quantix-cell-01",
    arrivalStartMs: 200,
    arrivalOffsetPx: [-12, -4],
    calibrationStartMs: 3925,
    calibrationOffsetPx: [-1.2, -0.6],
  },
  {
    id: "quantix-cell-02",
    arrivalStartMs: 430,
    arrivalOffsetPx: [-6, -11],
    calibrationStartMs: 4090,
    calibrationOffsetPx: [0.8, -0.8],
  },
  {
    id: "quantix-cell-03",
    arrivalStartMs: 660,
    arrivalOffsetPx: [5, -12],
    calibrationStartMs: 3760,
    calibrationOffsetPx: [-0.8, -0.4],
  },
  {
    id: "quantix-cell-04",
    arrivalStartMs: 890,
    arrivalOffsetPx: [12, -5],
    calibrationStartMs: 4280,
    calibrationOffsetPx: [1, -0.6],
  },
  {
    id: "quantix-cell-05",
    arrivalStartMs: 1120,
    arrivalOffsetPx: [-13, 1],
    calibrationStartMs: 3430,
    calibrationOffsetPx: [-0.6, 0.8],
  },
  {
    id: "quantix-cell-06",
    arrivalStartMs: 1380,
    arrivalOffsetPx: [12, 0],
    calibrationStartMs: 3100,
    calibrationOffsetPx: [0.7, -0.8],
  },
  {
    id: "quantix-cell-07",
    arrivalStartMs: 1640,
    arrivalOffsetPx: [-12, 2],
    calibrationStartMs: 3265,
    calibrationOffsetPx: [-0.6, 0.7],
  },
  {
    id: "quantix-cell-08",
    arrivalStartMs: 1900,
    arrivalOffsetPx: [12, 4],
    calibrationStartMs: 3595,
    calibrationOffsetPx: [0.8, 0.5],
  },
  {
    id: "quantix-cell-09",
    arrivalStartMs: 2160,
    arrivalOffsetPx: [-10, 10],
    calibrationStartMs: 4255,
    calibrationOffsetPx: [-0.7, 0.5],
  },
  {
    id: "quantix-cell-10",
    arrivalStartMs: 2420,
    arrivalOffsetPx: [4, 12],
    calibrationStartMs: 4280,
    calibrationOffsetPx: [0.6, 0.5],
  },
];

const root = document.querySelector<HTMLElement>("#splash-root");
const markHost = document.querySelector<HTMLElement>("#quantix-splash-mark");

let ready = false;
let finished = false;
let motionComplete = false;
let reducedMotion = window.matchMedia(
  "(prefers-reduced-motion: reduce)",
).matches;
let animations: Animation[] = [];
let assemblyTimer: number | undefined;
let handoffTimer: number | undefined;
let fadeTimer: number | undefined;

const translate = ([x, y]: Point) => `translate(${x}px, ${y}px)`;

const clearTimer = (timer: number | undefined) => {
  if (timer !== undefined) window.clearTimeout(timer);
};

const clearTimers = () => {
  clearTimer(assemblyTimer);
  clearTimer(handoffTimer);
  clearTimer(fadeTimer);
  assemblyTimer = undefined;
  handoffTimer = undefined;
  fadeTimer = undefined;
};

const clearComponentMotion = () => {
  for (const animation of animations) animation.cancel();
  animations = [];
};

const setFinalMark = () => {
  clearTimers();
  clearComponentMotion();
  motionComplete = true;
};

const finishHandoff = async () => {
  if (finished) return;
  finished = true;
  try {
    await invoke("finish_startup_splash");
  } catch {
    // Native owns the final recovery path if the window disappears mid-handoff.
  }
};

const beginHandoff = () => {
  if (!ready || !motionComplete || finished || fadeTimer !== undefined) return;

  if (reducedMotion) {
    void finishHandoff();
    return;
  }

  if (root && typeof root.animate === "function") {
    root.animate([{ opacity: 1 }, { opacity: 0 }], {
      duration: HANDOFF_FADE_MS,
      easing: "linear",
      fill: "forwards",
    });
  }

  fadeTimer = window.setTimeout(() => {
    fadeTimer = undefined;
    void finishHandoff();
  }, HANDOFF_FADE_MS);
};

const resolveToFinalMark = () => {
  clearComponentMotion();
  motionComplete = true;
  beginHandoff();
};

const playEvidenceCell = (cue: EvidenceCue, cell: SVGElement) => {
  const arrivalEndMs = cue.arrivalStartMs + ARRIVAL_DURATION_MS;
  const calibrationEndMs = cue.calibrationStartMs + CALIBRATION_DURATION_MS;
  return cell.animate(
    [
      {
        offset: 0,
        opacity: 0,
        transform: translate(cue.arrivalOffsetPx),
      },
      {
        offset: cue.arrivalStartMs / REFERENCE_ASSEMBLY_COMPLETE_MS,
        opacity: 0,
        transform: translate(cue.arrivalOffsetPx),
        easing: ARRIVAL_EASING,
      },
      {
        offset: arrivalEndMs / REFERENCE_ASSEMBLY_COMPLETE_MS,
        opacity: 1,
        transform: translate(cue.calibrationOffsetPx),
      },
      {
        offset: cue.calibrationStartMs / REFERENCE_ASSEMBLY_COMPLETE_MS,
        opacity: 1,
        transform: translate(cue.calibrationOffsetPx),
        easing: CALIBRATION_EASING,
      },
      {
        offset: calibrationEndMs / REFERENCE_ASSEMBLY_COMPLETE_MS,
        opacity: 1,
        transform: "translate(0px, 0px)",
      },
      {
        offset: 1,
        opacity: 1,
        transform: "translate(0px, 0px)",
      },
    ],
    { duration: ASSEMBLY_COMPLETE_MS, easing: "linear", fill: "both" },
  );
};

const playAuthorityDatum = (datum: SVGElement) =>
  datum.animate(
    [
      { offset: 0, opacity: 0, transform: "translate(-12px, -6.37px)" },
      {
        offset: 4550 / REFERENCE_ASSEMBLY_COMPLETE_MS,
        opacity: 0,
        transform: "translate(-12px, -6.37px)",
        easing: ARRIVAL_EASING,
      },
      {
        offset: 5670 / REFERENCE_ASSEMBLY_COMPLETE_MS,
        opacity: 1,
        transform: "translate(1.225px, 0.65px)",
        easing: CALIBRATION_EASING,
      },
      {
        offset: 6020 / REFERENCE_ASSEMBLY_COMPLETE_MS,
        opacity: 1,
        transform: "translate(-0.35px, -0.186px)",
        easing: CALIBRATION_EASING,
      },
      { offset: 1, opacity: 1, transform: "translate(0px, 0px)" },
    ],
    { duration: ASSEMBLY_COMPLETE_MS, easing: "linear", fill: "both" },
  );

const playEvidenceCalibration = () => {
  if (!root || !markHost || reducedMotion) {
    if (reducedMotion) {
      setFinalMark();
      beginHandoff();
    }
    return;
  }

  const cells = EVIDENCE_CUES.map((cue) =>
    markHost.querySelector<SVGElement>(`#${cue.id}`),
  );
  const datum = markHost.querySelector<SVGElement>("#quantix-authority-datum");

  if (
    cells.some((cell) => cell === null) ||
    !datum ||
    typeof Element.prototype.animate !== "function"
  ) {
    // Do not register an incomplete renderer. Native keeps its short watchdog
    // for this recovery path rather than leaving a hidden main window behind.
    return;
  }

  animations = EVIDENCE_CUES.map((cue, index) =>
    playEvidenceCell(cue, cells[index] as SVGElement),
  );
  animations.push(playAuthorityDatum(datum));

  assemblyTimer = window.setTimeout(() => {
    assemblyTimer = undefined;
    clearComponentMotion();
  }, ASSEMBLY_COMPLETE_MS);
  handoffTimer = window.setTimeout(() => {
    handoffTimer = undefined;
    resolveToFinalMark();
  }, HANDOFF_START_MS);
};

const handleReady = () => {
  if (ready) return;
  ready = true;
  beginHandoff();
};

const applyPreferences = (preferences: StartupPreferences) => {
  reducedMotion = preferences.reducedMotion;
  if (reducedMotion) {
    setFinalMark();
    beginHandoff();
  }
};

const subscribe = async () => {
  try {
    await listen("quantix-startup-ready", handleReady);
    await listen<StartupPreferences>("quantix-startup-preferences", (event) => {
      applyPreferences(event.payload);
    });
    if (await invoke<boolean>("inspect_startup_display_ready")) handleReady();
  } catch {
    // A plain-browser preview has no Tauri bridge. In the native window the
    // host watchdog remains the escape hatch if subscription or inspection
    // becomes unavailable.
  }
};

void subscribe();
playEvidenceCalibration();

window.addEventListener("beforeunload", () => {
  clearTimers();
  clearComponentMotion();
});
