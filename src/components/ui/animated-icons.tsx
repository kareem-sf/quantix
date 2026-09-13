/**
 * Animated icons adapted from Skiper UI (skiper99) by @gurvinder-singh02 —
 * https://skiper-ui.com.
 *
 * ArrowIcon reacts to hover on the nearest parent marked `group/arrow`.
 */
import { motion, useReducedMotion } from "motion/react";
import { ChevronRight } from "lucide-react";
import { cn } from "@/lib/utils";

export function ArrowIcon({ className }: { className?: string }) {
  return (
    <span
      aria-hidden="true"
      className={cn(
        "relative inline-grid size-4 shrink-0 items-center justify-center rtl:-scale-x-100",
        className,
      )}
    >
      <ChevronRight className="size-4 transition-transform duration-500 ease-out group-hover/arrow:translate-x-0.5 group-focus-visible/arrow:translate-x-0.5" />
      <span className="absolute right-[5px] h-[1.5px] w-2.5 origin-right scale-x-0 rounded-[1px] bg-current transition-all duration-300 ease-out group-hover/arrow:right-[3px] group-hover/arrow:scale-x-100 group-focus-visible/arrow:right-[3px] group-focus-visible/arrow:scale-x-100" />
    </span>
  );
}

/** Three lines that fold into a close mark while `open` is true. */
export function MenuIcon({
  open,
  className,
}: {
  open: boolean;
  className?: string;
}) {
  const reduce = useReducedMotion();
  const transition = reduce ? { duration: 0 } : undefined;
  return (
    <span
      aria-hidden="true"
      className={cn(
        "relative grid size-4 shrink-0 items-center justify-center",
        className,
      )}
    >
      <motion.span
        initial={false}
        animate={{ y: open ? 0 : -5, rotate: open ? 45 : 0 }}
        transition={transition}
        className="absolute h-[1.5px] w-full rounded-full bg-current"
      />
      <motion.span
        initial={false}
        animate={{ opacity: open ? 0 : 1 }}
        transition={reduce ? { duration: 0 } : { duration: 0.1 }}
        className="absolute h-[1.5px] w-full rounded-full bg-current"
      />
      <motion.span
        initial={false}
        animate={{ y: open ? 0 : 5, rotate: open ? -45 : 0 }}
        transition={transition}
        className="absolute h-[1.5px] w-full rounded-full bg-current"
      />
    </span>
  );
}
