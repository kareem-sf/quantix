/**
 * Expanding single-choice filter. Adapted from Watermelon UI "Filter
 * Disclosure" (https://ui.watermelon.sh) with lucide icons, theme tokens,
 * compact Quantix sizing, keyboard dismissal and a controlled value.
 */
import { useEffect, useId, useRef, useState, type ComponentType } from "react";
import {
  AnimatePresence,
  MotionConfig,
  motion,
  useReducedMotion,
} from "motion/react";
import { Check, ListFilter } from "lucide-react";
import { cn } from "@/lib/utils";

export type FilterDisclosureItem = {
  id: string;
  label: string;
  icon: ComponentType<{ className?: string }>;
  count?: number;
};

export function FilterDisclosure({
  items,
  value,
  onChange,
  label,
  className,
}: {
  items: FilterDisclosureItem[];
  value: string;
  onChange: (id: string) => void;
  label: string;
  className?: string;
}) {
  const [open, setOpen] = useState(false);
  const layoutId = useId();
  const root = useRef<HTMLDivElement>(null);
  const trigger = useRef<HTMLButtonElement>(null);
  const reduce = useReducedMotion();
  const active = items.find((item) => item.id === value) ?? items[0];
  const ActiveIcon = active?.icon ?? ListFilter;

  useEffect(() => {
    if (!open) return;
    const onPointer = (event: PointerEvent) => {
      if (!root.current?.contains(event.target as Node)) setOpen(false);
    };
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      setOpen(false);
      requestAnimationFrame(() => trigger.current?.focus());
    };
    document.addEventListener("pointerdown", onPointer);
    document.addEventListener("keydown", onKey);
    root.current
      ?.querySelector<HTMLButtonElement>("[aria-pressed='true']")
      ?.focus();
    return () => {
      document.removeEventListener("pointerdown", onPointer);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);

  function select(id: string) {
    onChange(id);
    window.setTimeout(
      () => {
        setOpen(false);
        requestAnimationFrame(() => trigger.current?.focus());
      },
      reduce ? 0 : 180,
    );
  }

  return (
    <div ref={root} className={cn("relative h-8", className)}>
      <MotionConfig
        reducedMotion="user"
        transition={{ type: "spring", bounce: 0.2, duration: 0.55 }}
      >
        <AnimatePresence mode="popLayout" initial={false}>
          {open ? (
            <motion.div
              key="open"
              layoutId={layoutId}
              role="group"
              aria-label={label}
              initial={{ opacity: 0 }}
              animate={{ opacity: 1 }}
              exit={{ opacity: 0, transition: { duration: 0 } }}
              style={{ borderRadius: 14, transformOrigin: "0% 0%" }}
              className="absolute start-0 top-0 z-30 flex w-56 flex-col gap-0.5 overflow-hidden border bg-popover p-1 text-popover-foreground shadow-lg"
            >
              {items.map((item, index) => {
                const Icon = item.icon;
                const selected = item.id === value;
                return (
                  <motion.button
                    key={item.id}
                    type="button"
                    aria-pressed={selected}
                    onClick={() => select(item.id)}
                    initial={{ opacity: 0, y: 14, scale: 1.04 }}
                    animate={{ opacity: 1, y: 0, scale: 1 }}
                    whileTap={{ scale: 0.98 }}
                    transition={{
                      type: "spring",
                      stiffness: 260,
                      damping: 22,
                      delay: (index + 1) * 0.035,
                    }}
                    className="flex w-full cursor-pointer items-center justify-between gap-3 rounded-[10px] px-2 py-1.5 text-start text-sm outline-none transition-colors hover:bg-accent focus-visible:bg-accent"
                  >
                    <span className="flex min-w-0 items-center gap-2.5">
                      <Icon className="size-4 shrink-0 text-muted-foreground" />
                      <span className="truncate">{item.label}</span>
                      {item.count !== undefined ? (
                        <span className="text-xs text-muted-foreground tabular-nums">
                          {item.count}
                        </span>
                      ) : null}
                    </span>
                    <span
                      className={cn(
                        "flex size-4 shrink-0 items-center justify-center rounded-full border-2 transition-colors",
                        selected
                          ? "border-primary bg-primary"
                          : "border-muted-foreground/40",
                      )}
                    >
                      <motion.span
                        initial={false}
                        animate={{
                          scale: selected ? 1 : 0,
                          opacity: selected ? 1 : 0,
                        }}
                        transition={{
                          type: "spring",
                          stiffness: 520,
                          damping: 30,
                        }}
                      >
                        <Check
                          className="size-2.5 text-primary-foreground"
                          strokeWidth={4}
                        />
                      </motion.span>
                    </span>
                  </motion.button>
                );
              })}
            </motion.div>
          ) : (
            <div key="closed" className="flex items-center">
              <motion.button
                ref={trigger}
                type="button"
                layoutId={layoutId}
                aria-label={`${label}: ${active?.label ?? ""}`}
                aria-expanded={false}
                onClick={() => setOpen(true)}
                initial={{ opacity: 0 }}
                animate={{ opacity: 1 }}
                exit={{ opacity: 0, transition: { duration: 0 } }}
                whileTap={{ scale: 0.95 }}
                style={{ borderRadius: 16 }}
                className="relative z-20 flex size-8 cursor-pointer items-center justify-center border bg-background shadow-xs outline-none transition-colors hover:bg-accent focus-visible:ring-3 focus-visible:ring-ring/50"
              >
                <ListFilter className="size-4" />
              </motion.button>
              <motion.span
                aria-hidden="true"
                initial={{ x: -10 }}
                animate={{ x: 0 }}
                transition={{ type: "spring", bounce: 0, duration: 0.8 }}
                className="z-10 -ms-2.5 flex h-8 items-center rounded-full border bg-background ps-3.5 pe-3 text-xs text-muted-foreground shadow-xs"
              >
                <AnimatePresence mode="popLayout" initial={false}>
                  <motion.span
                    key={value}
                    initial={{ opacity: 0, scale: 0.7 }}
                    animate={{ opacity: 1, scale: 1 }}
                    exit={{ opacity: 0, scale: 0.7 }}
                    className="flex items-center gap-1.5"
                  >
                    <ActiveIcon className="size-3.5" />
                    {active?.label}
                  </motion.span>
                </AnimatePresence>
              </motion.span>
            </div>
          )}
        </AnimatePresence>
      </MotionConfig>
    </div>
  );
}
