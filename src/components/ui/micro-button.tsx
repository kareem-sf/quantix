/** Amicro action interactions, adapted for Quantix. See docs/interface-motion.md. */
import {
  AnimatePresence,
  motion,
  useReducedMotion,
  type HTMLMotionProps,
} from "motion/react";
import {
  Check,
  Copy,
  Download,
  Moon,
  Pause,
  Play,
  RefreshCw,
  Search,
  Sun,
  Trash2,
  Upload,
  X,
} from "lucide-react";
import { cn } from "@/lib/utils";
import type { ReactNode } from "react";

const icons = {
  copy: [Copy, Check],
  delete: [Trash2, Trash2],
  preview: [Play, Pause],
  search: [Search, X],
  theme: [Moon, Sun],
  download: [Download, Download],
  upload: [Upload, Upload],
  refresh: [RefreshCw, RefreshCw],
} as const;

type MicroButtonProps = Omit<HTMLMotionProps<"button">, "children"> & {
  kind: keyof typeof icons;
  active?: boolean;
  children: ReactNode;
};

export function MicroButton({
  kind,
  active = false,
  children,
  className,
  disabled,
  type = "button",
  ...props
}: MicroButtonProps) {
  const reduce = useReducedMotion();
  const Icon = icons[kind][active ? 1 : 0];
  const hover =
    kind === "delete"
      ? { y: [0, -2, 0, -2, 0], rotate: [0, -10, 10, -10, 0] }
      : kind === "refresh"
        ? { rotate: 90 }
        : kind === "download"
          ? { y: 2 }
          : kind === "upload"
            ? { y: -2 }
            : {};
  return (
    <motion.button
      {...props}
      type={type}
      disabled={disabled}
      initial="rest"
      animate="rest"
      whileHover={reduce || disabled ? undefined : "hover"}
      whileFocus={reduce || disabled ? undefined : "hover"}
      whileTap={reduce || disabled ? undefined : "press"}
      variants={{
        rest: { scale: 1 },
        hover: { scale: 1.02 },
        press: { scale: 0.96 },
      }}
      className={cn(
        "relative inline-flex h-9 max-w-full shrink-0 cursor-pointer items-center justify-center gap-2.5 rounded-[40px] border border-foreground/5 bg-foreground/[0.04] px-6 text-sm font-medium whitespace-nowrap outline-none transition-colors duration-150 hover:bg-foreground/[0.06] focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background disabled:cursor-not-allowed disabled:opacity-50 motion-reduce:transition-none",
        kind === "delete" ? "text-destructive" : "text-foreground",
        className,
      )}
    >
      <span
        aria-hidden="true"
        className="relative flex size-4 shrink-0 items-center justify-center"
      >
        <AnimatePresence mode="popLayout" initial={false}>
          <motion.span
            key={`${kind}-${active}`}
            initial={{ opacity: reduce ? 1 : 0, scale: reduce ? 1 : 0.5 }}
            animate={{ opacity: 1, scale: 1 }}
            exit={{ opacity: 0, scale: reduce ? 1 : 0.5 }}
            transition={
              reduce
                ? { duration: 0 }
                : { type: "spring", stiffness: 600, damping: 25 }
            }
            className="absolute inset-0 flex items-center justify-center"
          >
            <motion.span
              className="flex"
              variants={{
                rest: { y: 0, rotate: 0 },
                hover: reduce ? {} : hover,
              }}
              transition={{ duration: reduce ? 0 : 0.4 }}
            >
              <Icon className="size-4" />
            </motion.span>
          </motion.span>
        </AnimatePresence>
      </span>
      <span className="min-w-0 truncate">{children}</span>
    </motion.button>
  );
}
