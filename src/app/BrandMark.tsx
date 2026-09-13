import { brandMarks } from "../brand";
import { useResolvedTheme } from "../theme";
import { cn } from "@/lib/utils";

export function BrandMark({
  size = 32,
  className,
  "data-brand-anchor": anchor,
}: {
  size?: number;
  className?: string;
  /** Marks the logo the launch splash lands on. */
  "data-brand-anchor"?: string;
}) {
  const theme = useResolvedTheme();
  return (
    <img
      aria-hidden="true"
      alt=""
      data-slot="brand-mark"
      data-brand-anchor={anchor}
      src={brandMarks[theme][size <= 24 ? "flat" : "dimensional"]}
      width={size}
      height={size}
      draggable={false}
      className={cn("block shrink-0 object-contain", className)}
    />
  );
}
