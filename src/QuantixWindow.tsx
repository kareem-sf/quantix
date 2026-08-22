import { LazyMotion, MotionConfig } from "motion/react";
import { type PropsWithChildren, useRef, useSyncExternalStore } from "react";

import { WindowTitleBar, type WindowTitleBarProps } from "./WindowTitleBar";
import { QuantixDepthScene } from "./motion/QuantixDepthScene";
import { quantixSmoothEase } from "./motion/motionPresets";
import "./QuantixWindow.css";

export type QuantixWindowMotionState = "ready" | "static";

export type QuantixWindowProps = PropsWithChildren<
  WindowTitleBarProps & {
    /** Ready workspaces receive the one-shot shell entrance and depth motion. */
    motionState?: QuantixWindowMotionState;
    /** Saved application preference. The operating-system preference is additive. */
    reducedMotion?: boolean;
  }
>;

const loadMotionFeatures = () =>
  import("./motion/motionFeatures").then((module) => module.default);

const REDUCED_MOTION_QUERY = "(prefers-reduced-motion: reduce)";

function subscribeToReducedMotion(onChange: () => void) {
  const query = window.matchMedia(REDUCED_MOTION_QUERY);
  query.addEventListener("change", onChange);
  return () => query.removeEventListener("change", onChange);
}

function readReducedMotion() {
  return window.matchMedia(REDUCED_MOTION_QUERY).matches;
}

function useSystemReducedMotion() {
  return useSyncExternalStore(
    subscribeToReducedMotion,
    readReducedMotion,
    () => false,
  );
}

export function QuantixWindow({
  children,
  motionState = "ready",
  reducedMotion = false,
  ...titleBar
}: QuantixWindowProps) {
  const customTitleBarEnabled =
    titleBar.enabled ??
    (typeof __QUANTIX_WINDOWS_TITLEBAR__ !== "undefined" &&
      __QUANTIX_WINDOWS_TITLEBAR__);
  const systemReducedMotion = useSystemReducedMotion();
  const shouldReduceMotion = reducedMotion || systemReducedMotion;
  const playEntrance = useRef(
    motionState === "ready" && !shouldReduceMotion,
  ).current;

  const animated = motionState === "ready" && !shouldReduceMotion;

  return (
    <LazyMotion features={loadMotionFeatures} strict>
      <MotionConfig
        reducedMotion={shouldReduceMotion ? "always" : "never"}
        transition={quantixSmoothEase}
      >
        <div
          className={`quantix-window${customTitleBarEnabled ? " has-custom-title-bar" : ""}${animated ? " has-depth-motion" : " is-motion-static"}`}
          data-motion-state={motionState}
          data-reduced-motion={shouldReduceMotion ? "true" : "false"}
        >
          <WindowTitleBar {...titleBar} enabled={customTitleBarEnabled} />
          <QuantixDepthScene animated={animated} playEntrance={playEntrance}>
            {children}
          </QuantixDepthScene>
        </div>
      </MotionConfig>
    </LazyMotion>
  );
}
