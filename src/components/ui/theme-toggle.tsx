/**
 * Theme toggle with a circular reveal that grows from the control that was
 * used. Adapted from Skiper UI (skiper26, "circle" variant) by
 * @gurvinder-singh02 — https://skiper-ui.com, inspired by rudrodip's
 * theme-toggle-effect. Rebuilt on the Quantix ThemeProvider instead of
 * next-themes; the reveal CSS lives in index.css.
 */
import { useCallback } from "react";
import { flushSync } from "react-dom";
import { useReducedMotion } from "motion/react";
import { systemTheme, useTheme, type Theme } from "@/theme";
import { MicroButton } from "./micro-button";

type Origin = { x: number; y: number };

export function useThemeTransition() {
  const { theme, resolvedTheme, setTheme } = useTheme();
  const reduce = useReducedMotion();

  const transitionTo = useCallback(
    (next: Theme, origin?: Origin) => {
      const nextResolved = next === "system" ? systemTheme() : next;
      const root = document.documentElement;
      if (
        reduce ||
        nextResolved === resolvedTheme ||
        typeof document.startViewTransition !== "function"
      ) {
        setTheme(next);
        return;
      }
      const x = origin?.x ?? window.innerWidth / 2;
      const y = origin?.y ?? window.innerHeight / 2;
      const radius = Math.hypot(
        Math.max(x, window.innerWidth - x),
        Math.max(y, window.innerHeight - y),
      );
      root.style.setProperty("--theme-x", `${x}px`);
      root.style.setProperty("--theme-y", `${y}px`);
      root.style.setProperty("--theme-r", `${radius}px`);
      root.dataset.themeTransition = "";
      const transition = document.startViewTransition(() => {
        flushSync(() => setTheme(next));
      });
      void transition.finished.finally(() => {
        delete root.dataset.themeTransition;
      });
    },
    [reduce, resolvedTheme, setTheme],
  );

  const toggle = useCallback(
    (origin?: Origin) =>
      transitionTo(resolvedTheme === "dark" ? "light" : "dark", origin),
    [resolvedTheme, transitionTo],
  );

  return { theme, resolvedTheme, transitionTo, toggle };
}

export function originOf(element: Element): Origin {
  const rect = element.getBoundingClientRect();
  return { x: rect.left + rect.width / 2, y: rect.top + rect.height / 2 };
}

export function ThemeToggleButton({ className }: { className?: string }) {
  const { resolvedTheme, toggle } = useThemeTransition();
  const dark = resolvedTheme === "dark";
  return (
    <MicroButton
      kind="theme"
      active={dark}
      aria-label={dark ? "Switch to light theme" : "Switch to dark theme"}
      title={dark ? "Switch to light theme" : "Switch to dark theme"}
      className={className}
      onClick={(event) => toggle(originOf(event.currentTarget))}
    >
      Theme
    </MicroButton>
  );
}
