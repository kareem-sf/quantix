/**
 * Animated counter built on Number Flow, following the Skiper UI (skiper37)
 * counter. Number Flow respects the reduced-motion preference by default.
 */
import NumberFlow, { type Format } from "@number-flow/react";
import { cn } from "@/lib/utils";

export function AnimatedNumber({
  value,
  format,
  prefix,
  suffix,
  className,
}: {
  value: number;
  format?: Format;
  prefix?: string;
  suffix?: string;
  className?: string;
}) {
  const text = `${prefix ?? ""}${new Intl.NumberFormat(undefined, format).format(value)}${suffix ?? ""}`;
  return (
    <span className={cn("inline-flex tabular-nums", className)}>
      <span className="sr-only">{text}</span>
      <NumberFlow
        aria-hidden="true"
        value={value}
        format={format}
        prefix={prefix}
        suffix={suffix}
      />
    </span>
  );
}
