/**
 * Loop counter adapted from Skiper UI (skiper62) by @gurvinder-singh02 —
 * https://skiper-ui.com. The counter pauses while `enabled` is false.
 */
import { useEffect, useState } from "react";

export function useLoop(delay = 1000, enabled = true) {
  const [key, setKey] = useState(0);
  useEffect(() => {
    if (!enabled) return;
    const interval = window.setInterval(
      () => setKey((value) => value + 1),
      delay,
    );
    return () => window.clearInterval(interval);
  }, [delay, enabled]);
  return { key };
}
