/**
 * Shared SVG filters, mounted once at the app root.
 * - Gooey liquid effect adapted from Skiper UI (skiper64).
 * - Squircle corners adapted from Skiper UI (skiper63).
 * Skiper UI by @gurvinder-singh02 — https://skiper-ui.com
 */
import type { ReactNode } from "react";
import { cn } from "@/lib/utils";

export const GOOEY_FILTER = "url(#quantix-gooey)";

export function EffectFilters() {
  return (
    <svg
      aria-hidden="true"
      focusable="false"
      width="0"
      height="0"
      className="pointer-events-none absolute size-0 overflow-hidden"
    >
      <defs>
        <filter id="quantix-gooey">
          <feGaussianBlur in="SourceGraphic" stdDeviation="2.4" result="blur" />
          <feColorMatrix
            in="blur"
            mode="matrix"
            values="1 0 0 0 0  0 1 0 0 0  0 0 1 0 0  0 0 0 20 -7"
            result="goo"
          />
          <feBlend in="SourceGraphic" in2="goo" />
        </filter>
        <filter id="quantix-squircle-sm">
          <feGaussianBlur in="SourceGraphic" stdDeviation="2.5" result="blur" />
          <feColorMatrix
            in="blur"
            mode="matrix"
            values="1 0 0 0 0  0 1 0 0 0  0 0 1 0 0  0 0 0 20 -7"
            result="goo"
          />
          <feBlend in="SourceGraphic" in2="goo" />
        </filter>
        <filter id="quantix-squircle-md">
          <feGaussianBlur in="SourceGraphic" stdDeviation="5" result="blur" />
          <feColorMatrix
            in="blur"
            mode="matrix"
            values="1 0 0 0 0  0 1 0 0 0  0 0 1 0 0  0 0 0 20 -7"
            result="goo"
          />
          <feBlend in="SourceGraphic" in2="goo" />
        </filter>
      </defs>
    </svg>
  );
}

/**
 * A tile with smooth squircle corners. The filter shapes only the surface
 * layer, so icons and letters inside stay crisp.
 */
export function Squircle({
  children,
  className,
  surfaceClassName,
  size = "sm",
}: {
  children?: ReactNode;
  className?: string;
  surfaceClassName?: string;
  size?: "sm" | "md";
}) {
  return (
    <span
      className={cn(
        "relative isolate inline-flex shrink-0 items-center justify-center",
        className,
      )}
    >
      <span
        aria-hidden="true"
        className={cn(
          "absolute inset-0 -z-10 rounded-[28%] bg-primary",
          surfaceClassName,
        )}
        style={{ filter: `url(#quantix-squircle-${size})` }}
      />
      {children}
    </span>
  );
}
