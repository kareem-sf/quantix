/**
 * Progressive blur edge. Adapted from Skiper UI (skiper41) by
 * @gurvinder-singh02 — https://skiper-ui.com. Place it inside a positioned
 * parent; it follows the theme background instead of a fixed colour.
 */
import type { CSSProperties } from "react";
import { cn } from "@/lib/utils";

type ProgressiveBlurProps = {
  className?: string;
  position?: "top" | "bottom";
  height?: string;
  blurAmount?: string;
  backgroundColor?: string;
};

export function ProgressiveBlur({
  className,
  position = "top",
  height = "3rem",
  blurAmount = "4px",
  backgroundColor = "var(--background)",
}: ProgressiveBlurProps) {
  const top = position === "top";
  const mask = `linear-gradient(${top ? "to bottom" : "to top"}, #000 50%, transparent)`;
  const style: CSSProperties = {
    [top ? "top" : "bottom"]: 0,
    height,
    background: `linear-gradient(${top ? "to top" : "to bottom"}, transparent, ${backgroundColor})`,
    maskImage: mask,
    WebkitMaskImage: mask,
    backdropFilter: `blur(${blurAmount})`,
    WebkitBackdropFilter: `blur(${blurAmount})`,
  };
  return (
    <div
      aria-hidden="true"
      className={cn(
        "pointer-events-none absolute inset-x-0 z-10 select-none",
        className,
      )}
      style={style}
    />
  );
}
