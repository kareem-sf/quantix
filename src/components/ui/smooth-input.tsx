/**
 * Text input with a spring-animated caret. Adapted from Skiper UI (skiper106)
 * by @gurvinder-singh02 — https://skiper-ui.com, without the dialkit tuning
 * panel. Right-to-left text keeps the native caret because canvas-style
 * prefix measurement is only reliable for left-to-right runs.
 */
import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type ComponentProps,
  type Ref,
} from "react";
import {
  motion,
  useMotionValue,
  useReducedMotion,
  useSpring,
} from "motion/react";
import { cn } from "@/lib/utils";

type SmoothInputProps = Omit<ComponentProps<"input">, "type"> & {
  type?: "text" | "search";
  wrapperClassName?: string;
  ref?: Ref<HTMLInputElement>;
};

const RTL_TEXT = /[֐-ࣿיִ-﷿ﹰ-﻿]/;

export function SmoothInput({
  className,
  wrapperClassName,
  onChange,
  onBlur,
  onFocus,
  ref,
  type = "text",
  ...props
}: SmoothInputProps) {
  const caretX = useMotionValue(0);
  const caretOpacity = useMotionValue(0);
  const reduce = useReducedMotion();
  const springX = useSpring(
    caretX,
    reduce
      ? { stiffness: 10000, damping: 100, mass: 0.1 }
      : { stiffness: 500, damping: 30, mass: 0.5 },
  );
  const inputRef = useRef<HTMLInputElement | null>(null);
  const measureRef = useRef<HTMLSpanElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const [nativeCaret, setNativeCaret] = useState(false);

  const update = useCallback(
    (target: HTMLInputElement) => {
      const measure = measureRef.current;
      if (!measure) return;
      const styles = window.getComputedStyle(target);
      const rtl = styles.direction === "rtl" || RTL_TEXT.test(target.value);
      setNativeCaret(rtl);
      if (rtl) {
        caretOpacity.set(0);
        return;
      }
      const start = target.selectionStart ?? 0;
      const end = target.selectionEnd ?? 0;
      const index = target.selectionDirection === "backward" ? start : end;
      measure.style.font = `${styles.fontStyle} ${styles.fontWeight} ${styles.fontSize} ${styles.fontFamily}`;
      measure.style.letterSpacing = styles.letterSpacing;
      measure.textContent = target.value.slice(0, index);
      const paddingLeft = parseFloat(styles.paddingLeft) || 0;
      const paddingRight = parseFloat(styles.paddingRight) || 0;
      const width = index > 0 ? measure.offsetWidth + paddingLeft : paddingLeft;
      const visibleRight = target.scrollLeft + target.clientWidth - paddingRight;
      if (width > visibleRight)
        target.scrollLeft = width - target.clientWidth + paddingRight;
      else if (width < target.scrollLeft + paddingLeft)
        target.scrollLeft = Math.max(0, width - paddingLeft);
      const position = width - target.scrollLeft;
      caretX.set(Math.min(position, target.clientWidth - paddingRight));
      caretOpacity.set(start === end ? 1 : 0);
    },
    [caretOpacity, caretX],
  );

  useEffect(() => {
    const input = inputRef.current;
    const container = containerRef.current;
    if (!input || !container) return;
    const refresh = () => {
      if (document.activeElement === input) update(input);
    };
    const onSelection = () => requestAnimationFrame(refresh);
    document.addEventListener("selectionchange", onSelection);
    input.addEventListener("scroll", refresh);
    const fonts = (document as Document & { fonts?: FontFaceSet }).fonts;
    fonts?.addEventListener?.("loadingdone", refresh);
    const observer =
      typeof ResizeObserver === "undefined" ? null : new ResizeObserver(refresh);
    observer?.observe(container);
    return () => {
      document.removeEventListener("selectionchange", onSelection);
      input.removeEventListener("scroll", refresh);
      fonts?.removeEventListener?.("loadingdone", refresh);
      observer?.disconnect();
    };
  }, [update]);

  useEffect(() => {
    const input = inputRef.current;
    if (input && document.activeElement === input) update(input);
  }, [props.value, update]);

  return (
    <div
      ref={containerRef}
      className={cn("relative grid min-w-0 grid-cols-1", wrapperClassName)}
    >
      <input
        {...props}
        type={type}
        ref={(node) => {
          inputRef.current = node;
          if (typeof ref === "function") ref(node);
          else if (ref) ref.current = node;
        }}
        className={cn(
          "col-start-1 row-start-1 w-full min-w-0 bg-transparent outline-none",
          !nativeCaret && "caret-transparent",
          className,
        )}
        onChange={(event) => {
          onChange?.(event);
          const target = event.currentTarget;
          requestAnimationFrame(() => update(target));
        }}
        onFocus={(event) => {
          update(event.currentTarget);
          onFocus?.(event);
        }}
        onBlur={(event) => {
          caretOpacity.set(0);
          onBlur?.(event);
        }}
      />
      <span
        ref={measureRef}
        aria-hidden="true"
        className="pointer-events-none invisible absolute top-0 left-0 whitespace-pre"
      />
      <motion.span
        aria-hidden="true"
        className="pointer-events-none col-start-1 row-start-1 h-[1.1em] w-0.5 self-center rounded-full bg-primary"
        style={{ x: springX, opacity: caretOpacity }}
      />
    </div>
  );
}
