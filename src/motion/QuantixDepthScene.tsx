import {
  m,
  type MotionValue,
  useMotionValue,
  useSpring,
  useTransform,
} from "motion/react";
import { type PropsWithChildren, useCallback, useEffect, useRef } from "react";

import { quantixAmbientDrift, quantixShellEntrance } from "./motionPresets";

interface QuantixDepthSceneProps extends PropsWithChildren {
  animated: boolean;
  playEntrance: boolean;
}

interface DepthMotionValues {
  farX: MotionValue<number>;
  farY: MotionValue<number>;
  nearX: MotionValue<number>;
  nearY: MotionValue<number>;
  rotateX: MotionValue<number>;
  rotateY: MotionValue<number>;
}

const depthSpring = {
  stiffness: 150,
  damping: 24,
  mass: 0.72,
};

function resetMotionValue(value: MotionValue<number>) {
  value.stop();
  value.jump(0);
}

function useDepthMotion(
  sceneRef: React.RefObject<HTMLDivElement | null>,
  active: boolean,
): DepthMotionValues {
  const pointerX = useMotionValue(0);
  const pointerY = useMotionValue(0);
  const smoothX = useSpring(pointerX, depthSpring);
  const smoothY = useSpring(pointerY, depthSpring);
  const farX = useTransform(smoothX, (value) => value * -0.42);
  const farY = useTransform(smoothY, (value) => value * -0.42);
  const nearX = useTransform(smoothX, (value) => value * 0.72);
  const nearY = useTransform(smoothY, (value) => value * 0.72);
  const rotateX = useTransform(smoothY, [-6, 6], [0.65, -0.65]);
  const rotateY = useTransform(smoothX, [-6, 6], [-0.65, 0.65]);

  const reset = useCallback(() => {
    resetMotionValue(pointerX);
    resetMotionValue(pointerY);
    resetMotionValue(smoothX);
    resetMotionValue(smoothY);
  }, [pointerX, pointerY, smoothX, smoothY]);

  useEffect(() => {
    const scene = sceneRef.current;
    if (!scene) return;

    const finePointer = window.matchMedia("(pointer: fine)");
    let listening = false;
    let bounds: DOMRect | null = null;

    const cacheBounds = () => {
      bounds = scene.getBoundingClientRect();
    };
    const move = (event: PointerEvent) => {
      const rect = bounds ?? scene.getBoundingClientRect();
      if (rect.width === 0 || rect.height === 0) return;

      const normalizedX = Math.max(
        -1,
        Math.min(1, ((event.clientX - rect.left) / rect.width) * 2 - 1),
      );
      const normalizedY = Math.max(
        -1,
        Math.min(1, ((event.clientY - rect.top) / rect.height) * 2 - 1),
      );
      pointerX.set(normalizedX * 6);
      pointerY.set(normalizedY * 6);
    };
    const leave = () => {
      bounds = null;
      reset();
    };
    const startListening = () => {
      if (listening) return;
      listening = true;
      scene.dataset.quantixParallax = "enabled";
      scene.addEventListener("pointerenter", cacheBounds, { passive: true });
      scene.addEventListener("pointermove", move, { passive: true });
      scene.addEventListener("pointerleave", leave, { passive: true });
      window.addEventListener("blur", leave);
      window.addEventListener("resize", cacheBounds, { passive: true });
    };
    const stopListening = () => {
      if (listening) {
        scene.removeEventListener("pointerenter", cacheBounds);
        scene.removeEventListener("pointermove", move);
        scene.removeEventListener("pointerleave", leave);
        window.removeEventListener("blur", leave);
        window.removeEventListener("resize", cacheBounds);
        listening = false;
      }
      scene.dataset.quantixParallax = "disabled";
      bounds = null;
      reset();
    };
    const syncPointerCapability = () => {
      if (active && finePointer.matches) startListening();
      else stopListening();
    };

    finePointer.addEventListener("change", syncPointerCapability);
    syncPointerCapability();
    return () => {
      finePointer.removeEventListener("change", syncPointerCapability);
      stopListening();
    };
  }, [active, pointerX, pointerY, reset, sceneRef]);

  return { farX, farY, nearX, nearY, rotateX, rotateY };
}

export function QuantixDepthScene({
  animated,
  children,
  playEntrance,
}: QuantixDepthSceneProps) {
  const sceneRef = useRef<HTMLDivElement>(null);
  const { farX, farY, nearX, nearY, rotateX, rotateY } = useDepthMotion(
    sceneRef,
    animated,
  );

  return (
    <div className="quantix-window__content" ref={sceneRef}>
      <m.div
        aria-hidden="true"
        className="quantix-window__depth-envelope quantix-window__depth-envelope--ambient"
        animate={animated ? "active" : "rest"}
        initial={false}
        variants={quantixAmbientDrift}
        style={{ x: farX, y: farY }}
      />
      <m.div
        aria-hidden="true"
        className="quantix-window__depth-envelope quantix-window__depth-envelope--shadow"
        style={{ x: nearX, y: nearY, rotateX, rotateY }}
      />
      <m.div
        aria-hidden="true"
        className="quantix-window__depth-envelope quantix-window__depth-envelope--highlight"
        style={{ x: nearX, y: nearY, rotateX, rotateY }}
      />
      <m.div
        className="quantix-window__stage"
        initial={playEntrance ? "hidden" : false}
        animate="visible"
        variants={quantixShellEntrance}
      >
        {children}
      </m.div>
    </div>
  );
}
