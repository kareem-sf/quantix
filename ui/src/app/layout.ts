import { useEffect, useState, useSyncExternalStore } from "react";

/** How the window's width shapes the shell. Wide: the sidebar and the team sit beside the screen. Medium: the team
 * floats over it. Narrow: the sidebar shows its icons and opens over the screen. Small: the sidebar is a drawer and
 * the team takes the whole window. */
export type Fit = "wide" | "medium" | "narrow" | "small";

const QUERIES: [Fit, string][] = [
  ["small", "(max-width: 639px)"],
  ["narrow", "(max-width: 1023px)"],
  ["medium", "(max-width: 1279px)"],
];

function fit(): Fit {
  if (typeof window === "undefined" || !window.matchMedia) return "wide";
  return QUERIES.find(([, query]) => window.matchMedia(query).matches)?.[0] ?? "wide";
}

function subscribe(changed: () => void) {
  if (!window.matchMedia) return () => {};
  const lists = QUERIES.map(([, query]) => window.matchMedia(query));
  lists.forEach((list) => list.addEventListener("change", changed));
  return () => lists.forEach((list) => list.removeEventListener("change", changed));
}

export function useFit(): Fit {
  return useSyncExternalStore(subscribe, fit, () => "wide");
}

/** Keeps something on screen for its closing animation: `shown` while open, then `leaving` for `ms`. */
export function usePresence(open: boolean, ms = 160): { shown: boolean; leaving: boolean } {
  const [shown, setShown] = useState(open);
  useEffect(() => {
    if (open) {
      setShown(true);
      return;
    }
    const done = window.setTimeout(() => setShown(false), ms);
    return () => window.clearTimeout(done);
  }, [open, ms]);
  return { shown: open || shown, leaving: !open && shown };
}
