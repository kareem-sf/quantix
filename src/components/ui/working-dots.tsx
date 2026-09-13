/**
 * Liquid "working" indicator using the gooey filter from Skiper UI (skiper64).
 * Decorative only: pair it with visible text that says what is happening.
 */
import { motion, useReducedMotion } from "motion/react";
import { cn } from "@/lib/utils";
import { GOOEY_FILTER } from "./effect-filters";

export function WorkingDots({ className }: { className?: string }) {
  const reduce = useReducedMotion();
  return (
    <span
      aria-hidden="true"
      className={cn("inline-flex items-center gap-[3px] px-1", className)}
      style={{ filter: GOOEY_FILTER }}
    >
      {[0, 1, 2].map((index) => (
        <motion.span
          key={index}
          className="size-2 rounded-full bg-current"
          animate={reduce ? undefined : { y: [0, -4, 0], scale: [1, 1.3, 1] }}
          transition={{
            duration: 0.9,
            repeat: Number.POSITIVE_INFINITY,
            delay: index * 0.14,
            ease: "easeInOut",
          }}
        />
      ))}
    </span>
  );
}
