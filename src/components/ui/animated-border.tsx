/**
 * A small light that travels around the border of its positioned parent.
 * Adapted from the Ali Imam UI animated border button demo. Use it sparingly
 * for a control that needs the engineer's attention.
 */
import { motion, useReducedMotion } from "motion/react";
import { cn } from "@/lib/utils";

export function AnimatedBorder({
  className,
  radius = 12,
  duration = 5,
  beamSize = 20,
}: {
  className?: string;
  /** Corner radius of the parent in pixels, used for the travel path. */
  radius?: number;
  duration?: number;
  beamSize?: number;
}) {
  const reduce = useReducedMotion();
  if (reduce) return null;
  return (
    <span
      aria-hidden="true"
      className={cn(
        "pointer-events-none absolute -inset-px rounded-[inherit] border-2 border-transparent [mask-clip:padding-box,border-box] [mask-composite:intersect] [mask-image:linear-gradient(transparent,transparent),linear-gradient(#000,#000)]",
        className,
      )}
    >
      <motion.span
        className="absolute aspect-square bg-linear-to-r from-transparent via-primary to-primary"
        animate={{ offsetDistance: ["0%", "100%"] }}
        style={{
          width: beamSize,
          offsetPath: `rect(0 auto auto 0 round ${radius}px)`,
        }}
        transition={{
          repeat: Number.POSITIVE_INFINITY,
          duration,
          ease: "linear",
        }}
      />
    </span>
  );
}
