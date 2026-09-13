/**
 * Radial gauge adapted from Ali Imam UI (https://aliimam.in/docs/components/gauge).
 * Trimmed to the options Quantix uses and coloured from theme tokens.
 */
import { useEffect, useRef, useState } from "react";
import { animate, useReducedMotion } from "motion/react";
import { cn } from "@/lib/utils";

export type GaugeProps = {
  /** Percentage from 0 to 100. */
  value: number;
  size?: number;
  strokeWidth?: number;
  gapPercent?: number;
  showValue?: boolean;
  unit?: string;
  primary?: string;
  secondary?: string;
  className?: string;
  "aria-label"?: string;
};

const CIRCLE = 100;

function toneFor(value: number) {
  if (value <= 25) return "var(--destructive)";
  if (value <= 50) return "var(--color-amber-500, #f59e0b)";
  if (value <= 75) return "var(--color-sky-500, #0ea5e9)";
  return "var(--color-emerald-500, #10b981)";
}

export function Gauge({
  value,
  size = 56,
  strokeWidth = 10,
  gapPercent = 5,
  showValue = true,
  unit = "%",
  primary,
  secondary = "var(--muted)",
  className,
  "aria-label": ariaLabel,
}: GaugeProps) {
  const target = Math.min(100, Math.max(0, Number.isFinite(value) ? value : 0));
  const reduce = useReducedMotion();
  const [shown, setShown] = useState(reduce ? target : 0);
  const shownRef = useRef(shown);
  shownRef.current = shown;

  useEffect(() => {
    if (reduce) {
      setShown(target);
      return;
    }
    const controls = animate(shownRef.current, target, {
      type: "spring",
      damping: 60,
      stiffness: 100,
      onUpdate: setShown,
    });
    return () => controls.stop();
  }, [target, reduce]);

  const radius = CIRCLE / 2 - strokeWidth / 2;
  const circumference = 2 * Math.PI * radius;
  const perPercent = circumference / 100;
  const primaryLength = Math.max(shown * perPercent, 0);
  const secondaryLength = Math.max(
    (100 - shown) * perPercent - gapPercent * 2 * perPercent,
    0,
  );
  const common = {
    cx: CIRCLE / 2,
    cy: CIRCLE / 2,
    r: radius,
    fill: "none",
    strokeWidth,
    strokeLinecap: "round" as const,
    style: { transformOrigin: "50% 50%" },
  };

  return (
    <svg
      role="img"
      aria-label={ariaLabel ?? `${Math.round(target)}${unit}`}
      viewBox={`0 0 ${CIRCLE} ${CIRCLE}`}
      width={size}
      height={size}
      className={cn("shrink-0 select-none", className)}
    >
      <circle
        {...common}
        stroke={secondary}
        strokeDasharray={`${secondaryLength} ${circumference}`}
        opacity={shown > 100 - gapPercent * 2 ? 0 : 1}
        style={{
          ...common.style,
          transform: `rotate(${270 - gapPercent * 3.6}deg) scaleY(-1)`,
        }}
      />
      <circle
        {...common}
        stroke={primary ?? toneFor(shown)}
        strokeDasharray={`${primaryLength} ${circumference}`}
        style={{ ...common.style, transform: "rotate(-90deg)" }}
      />
      {showValue ? (
        <text
          x={CIRCLE / 2}
          y={CIRCLE / 2}
          textAnchor="middle"
          dominantBaseline="central"
          fill="currentColor"
          fontSize={28}
          fontWeight={600}
          className="tabular-nums"
        >
          {Math.round(shown)}
        </text>
      ) : null}
    </svg>
  );
}
