import type { Transition, Variants } from "motion/react";

export const quantixSpring: Transition = {
  type: "spring",
  stiffness: 280,
  damping: 30,
  mass: 0.86,
};

/** The default workspace layout transition: smooth, bounded, and without overshoot. */
export const quantixSmoothEase: Transition = {
  type: "tween",
  duration: 0.22,
  ease: [0.2, 0, 0, 1],
};

export const quantixShellEntrance: Variants = {
  hidden: {
    opacity: 0,
    y: 8,
    scale: 0.995,
  },
  visible: {
    opacity: 1,
    y: 0,
    scale: 1,
    transition: {
      duration: 0.5,
      ease: [0.2, 0, 0, 1],
      when: "beforeChildren",
      staggerChildren: 0.045,
    },
  },
};

export const quantixAmbientDrift = {
  rest: {
    opacity: 0.32,
    scale: 1,
  },
  active: {
    opacity: [0.46, 0.72, 0.5],
    scale: [1, 1.025, 1],
    transition: {
      duration: 24,
      ease: "easeInOut",
      repeat: Number.POSITIVE_INFINITY,
    },
  },
} satisfies Variants;
