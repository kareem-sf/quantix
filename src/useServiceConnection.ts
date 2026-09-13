import { useCallback, useEffect, useState } from "react";
import { connect, type Api } from "./api";

/** Retry only the read-only startup handshake; never replay engineering actions. */
export function useServiceConnection() {
  const [api, setApi] = useState<Api | null>(null);
  const [error, setError] = useState<unknown>(null);
  const [attempt, setAttempt] = useState(0);
  const retry = useCallback(() => setAttempt((value) => value + 1), []);

  useEffect(() => {
    const controller = new AbortController();
    const startedAt = Date.now();
    let failures = 0;
    let timer: ReturnType<typeof setTimeout> | undefined;
    setError(null);

    async function establish() {
      try {
        const next = await connect(controller.signal);
        if (controller.signal.aborted) return;
        setError(null);
        setApi(next);
      } catch (failure) {
        if (controller.signal.aborted) return;
        failures += 1;
        // Development reloads briefly remove the connection record. Keep the
        // loading screen during that normal gap, but expose persistent errors.
        if (Date.now() - startedAt >= 15_000) setError(failure);
        timer = setTimeout(
          () => void establish(),
          Math.min(failures * 1000, 5000),
        );
      }
    }
    void establish();
    return () => {
      controller.abort();
      clearTimeout(timer);
    };
  }, [attempt]);

  return { api, error, retry };
}
