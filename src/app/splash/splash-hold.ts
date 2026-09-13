import {
  createContext,
  useContext,
  useLayoutEffect,
  useSyncExternalStore,
} from "react";

/**
 * Screens that are still loading their first data hold the splash open, so it
 * lifts onto a finished page instead of a loading line. Screens that want to
 * make an entrance can also wait until it has lifted.
 */
export function createSplashHolds(lifted = false) {
  let count = 0;
  const listeners = new Set<() => void>();
  const emit = () => listeners.forEach((listener) => listener());
  return {
    hold() {
      count += 1;
      emit();
      let released = false;
      return () => {
        if (released) return;
        released = true;
        count -= 1;
        emit();
      };
    },
    lift() {
      lifted = true;
      emit();
    },
    subscribe(listener: () => void) {
      listeners.add(listener);
      return () => void listeners.delete(listener);
    },
    count: () => count,
    lifted: () => lifted,
  };
}

export type SplashHolds = ReturnType<typeof createSplashHolds>;

export const SplashHoldContext = createContext<SplashHolds | null>(null);

export function useSplashHold(active: boolean) {
  const holds = useContext(SplashHoldContext);
  useLayoutEffect(() => {
    if (!active || !holds) return;
    return holds.hold();
  }, [active, holds]);
}

export function useSplashHoldCount(holds: SplashHolds) {
  return useSyncExternalStore(holds.subscribe, holds.count, holds.count);
}

const alwaysLifted = () => true;
const noSubscription = () => () => undefined;

/** True once the launch splash is gone (or when there is no splash at all). */
export function useSplashLifted() {
  const holds = useContext(SplashHoldContext);
  return useSyncExternalStore(
    holds?.subscribe ?? noSubscription,
    holds?.lifted ?? alwaysLifted,
    holds?.lifted ?? alwaysLifted,
  );
}
