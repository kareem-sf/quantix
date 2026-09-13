import { useCallback, useEffect, useRef, useState } from "react";
import { tenderPath, useApi, type Schema } from "../../api";

export type OfficeSnapshot = Schema<"OfficeSnapshot">;
export type OfficeEvent = Schema<"OfficeEvent">;

export type OfficeConnectionState =
  "disabled" | "connecting" | "connected" | "reconnecting" | "disconnected";

export type OfficeResult = {
  snapshot: OfficeSnapshot | null;
  connectionState: OfficeConnectionState;
  error: Error | null;
  events: OfficeEvent[];
  /** Event IDs eligible for a short live arrival transition. */
  arrivalIds: string[];
  refresh: () => void;
  retry: () => void;
};

export const OFFICE_EVENT_BUFFER_LIMIT = 100;
const POLL_DELAY_MS = 1_500;
const HIDDEN_POLL_DELAY_MS = 5_000;
const RETRY_DELAYS_MS = [1_000, 2_000, 4_000, 8_000, 15_000] as const;

/**
 * Reads the current office view and reconciles its durable event cursor.
 *
 * Office events identify that something changed; they are deliberately not
 * applied as model patches. A new snapshot is fetched after every accepted
 * event page so partial flags and all bounded collections remain server-owned.
 */
export function useOffice(tenderId: string, enabled = true): OfficeResult {
  const api = useApi();
  const bindingKey = `${tenderId}\u0000${enabled ? "enabled" : "disabled"}`;
  const bindingRef = useRef(bindingKey);
  const generationRef = useRef(0);
  const mountedRef = useRef(false);
  const requestInFlightRef = useRef(false);
  const requestControllerRef = useRef<AbortController | null>(null);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const timerKindRef = useRef<"poll" | "retry" | null>(null);
  const refreshRequestedRef = useRef(false);
  const pendingSnapshotRef = useRef(false);
  const retryAttemptRef = useRef(0);
  const cursorRef = useRef<string | null>(null);
  const sequenceRef = useRef(0);
  const observedSequenceRef = useRef(0);
  const instanceRef = useRef<string | null>(null);
  const transportEventIdsRef = useRef(new Set<string>());
  const outputEventIdsRef = useRef(new Set<string>());
  const arrivalIdsRef = useRef(new Set<string>());
  const snapshotRef = useRef<OfficeSnapshot | null>(null);
  const connectionStateRef = useRef<OfficeConnectionState>(
    enabled ? "connecting" : "disabled",
  );
  const manualRefreshRef = useRef<(() => void) | null>(null);

  const [snapshot, setSnapshot] = useState<OfficeSnapshot | null>(null);
  const [connectionState, setConnectionState] = useState<OfficeConnectionState>(
    enabled ? "connecting" : "disabled",
  );
  const [error, setError] = useState<Error | null>(null);
  const [events, setEvents] = useState<OfficeEvent[]>([]);
  const [arrivalIds, setArrivalIds] = useState<string[]>([]);

  useEffect(() => {
    // Cleanup owns network/timer effects. A render for another Tender hides
    // the previous state below without mutating the committed subscription.
    bindingRef.current = bindingKey;
    generationRef.current += 1;
    requestControllerRef.current?.abort();
    requestControllerRef.current = null;
    requestInFlightRef.current = false;
    if (timerRef.current !== null) clearTimeout(timerRef.current);
    timerRef.current = null;
    timerKindRef.current = null;
    refreshRequestedRef.current = false;
    pendingSnapshotRef.current = false;
    retryAttemptRef.current = 0;
    cursorRef.current = null;
    sequenceRef.current = 0;
    observedSequenceRef.current = 0;
    instanceRef.current = null;
    transportEventIdsRef.current.clear();
    outputEventIdsRef.current.clear();
    arrivalIdsRef.current.clear();
    snapshotRef.current = null;
    connectionStateRef.current = enabled ? "connecting" : "disabled";
    manualRefreshRef.current = null;
    setSnapshot(null);
    setEvents([]);
    setArrivalIds([]);
    setError(null);
    setConnectionState(connectionStateRef.current);
    if (!enabled) {
      connectionStateRef.current = "disabled";
      setConnectionState("disabled");
      return;
    }

    const generation = ++generationRef.current;
    mountedRef.current = true;
    const officePath = `${tenderPath(tenderId)}/office`;
    const eventsPath = `${officePath}/events`;

    const isCurrent = () =>
      mountedRef.current && generationRef.current === generation;

    const setConnection = (next: OfficeConnectionState) => {
      if (!isCurrent()) return;
      connectionStateRef.current = next;
      setConnectionState(next);
    };

    const setRequestError = (failure: unknown) => {
      if (!isCurrent()) return;
      const next = toError(failure);
      setError(next);
      connectionStateRef.current = snapshotRef.current
        ? "reconnecting"
        : "disconnected";
      setConnectionState(connectionStateRef.current);
      retryAttemptRef.current = Math.min(
        retryAttemptRef.current + 1,
        RETRY_DELAYS_MS.length,
      );
    };

    const clearTimer = () => {
      if (timerRef.current !== null) clearTimeout(timerRef.current);
      timerRef.current = null;
      timerKindRef.current = null;
    };

    const schedule = (kind: "poll" | "retry", delay?: number) => {
      if (!isCurrent()) return;
      clearTimer();
      const actualDelay =
        delay ??
        (kind === "poll"
          ? getVisibility() === "hidden"
            ? HIDDEN_POLL_DELAY_MS
            : POLL_DELAY_MS
          : (RETRY_DELAYS_MS[
              Math.min(retryAttemptRef.current, RETRY_DELAYS_MS.length) - 1
            ] ?? RETRY_DELAYS_MS[0]));
      timerKindRef.current = kind;
      timerRef.current = setTimeout(
        () => {
          timerRef.current = null;
          timerKindRef.current = null;
          if (!isCurrent()) return;
          if (kind === "retry") {
            setConnection(snapshotRef.current ? "reconnecting" : "connecting");
          }
          if (pendingSnapshotRef.current) void runSnapshot();
          else void runPoll();
        },
        Math.max(0, actualDelay),
      );
    };

    const beginRequest = () => {
      if (!isCurrent() || requestInFlightRef.current) return null;
      const controller = new AbortController();
      requestInFlightRef.current = true;
      requestControllerRef.current = controller;
      return controller;
    };

    const endRequest = (controller: AbortController) => {
      if (requestControllerRef.current === controller) {
        requestControllerRef.current = null;
        requestInFlightRef.current = false;
      }
    };

    const applySnapshot = (next: OfficeSnapshot) => {
      if (!isCurrent()) return;
      if (
        (instanceRef.current !== null &&
          instanceRef.current !== next.instance_id) ||
        next.sequence < sequenceRef.current
      ) {
        outputEventIdsRef.current.clear();
        arrivalIdsRef.current.clear();
        setEvents([]);
        setArrivalIds([]);
      }
      snapshotRef.current = next;
      setSnapshot(next);
      cursorRef.current = next.cursor ?? null;
      sequenceRef.current = next.sequence;
      observedSequenceRef.current = next.sequence;
      instanceRef.current = next.instance_id;
      transportEventIdsRef.current.clear();
      pendingSnapshotRef.current = false;
    };

    const appendObservedEvents = (
      observed: OfficeEvent[],
      eligibleForArrival: boolean,
    ) => {
      if (!isCurrent() || observed.length === 0) return;
      const additions = observed.filter((item) => {
        if (outputEventIdsRef.current.has(item.event_id)) return false;
        outputEventIdsRef.current.add(item.event_id);
        return true;
      });
      if (additions.length === 0) return;
      while (outputEventIdsRef.current.size > OFFICE_EVENT_BUFFER_LIMIT) {
        const oldest = outputEventIdsRef.current.values().next().value;
        if (oldest === undefined) break;
        outputEventIdsRef.current.delete(oldest);
      }
      setEvents((current) =>
        [...current, ...additions].slice(-OFFICE_EVENT_BUFFER_LIMIT),
      );

      if (!eligibleForArrival || getVisibility() === "hidden") return;
      const liveArrivalIds = additions
        .map((item) => item.event_id)
        .filter((eventId) => {
          if (arrivalIdsRef.current.has(eventId)) return false;
          arrivalIdsRef.current.add(eventId);
          return true;
        });
      if (liveArrivalIds.length === 0) return;
      while (arrivalIdsRef.current.size > OFFICE_EVENT_BUFFER_LIMIT) {
        const oldest = arrivalIdsRef.current.values().next().value;
        if (oldest === undefined) break;
        arrivalIdsRef.current.delete(oldest);
      }
      setArrivalIds((current) =>
        [...current, ...liveArrivalIds].slice(-OFFICE_EVENT_BUFFER_LIMIT),
      );
    };

    const requestSnapshot = async (signal: AbortSignal) => {
      const next = await api.get<OfficeSnapshot>(officePath, signal);
      if (!isCurrent() || signal.aborted) return false;
      applySnapshot(next);
      return true;
    };

    async function runSnapshot() {
      const controller = beginRequest();
      if (!controller) {
        refreshRequestedRef.current = true;
        return;
      }
      clearTimer();
      setConnection(snapshotRef.current ? "reconnecting" : "connecting");
      let succeeded = false;
      try {
        succeeded = await requestSnapshot(controller.signal);
        if (succeeded) {
          retryAttemptRef.current = 0;
          setError(null);
          setConnection("connected");
        }
      } catch (failure) {
        if (!isAbortError(failure, controller.signal)) setRequestError(failure);
      } finally {
        endRequest(controller);
        if (!isCurrent()) return;
        if (refreshRequestedRef.current) {
          refreshRequestedRef.current = false;
          void runSnapshot();
        } else if (succeeded) {
          schedule("poll");
        } else {
          schedule("retry");
        }
      }
    }

    async function runPoll() {
      const controller = beginRequest();
      if (!controller) return;
      clearTimer();
      let succeeded = false;
      try {
        if (pendingSnapshotRef.current || snapshotRef.current === null) {
          succeeded = await requestSnapshot(controller.signal);
          if (succeeded) {
            retryAttemptRef.current = 0;
            setError(null);
            setConnection("connected");
          }
          return;
        }

        const cursor = cursorRef.current;
        const path = cursor
          ? `${eventsPath}?after=${encodeURIComponent(cursor)}`
          : eventsPath;
        const response = await api.get<Schema<"OfficeEventPageWithInstance">>(
          path,
          controller.signal,
        );
        if (!isCurrent() || controller.signal.aborted) return;

        const beforePollState = connectionStateRef.current;
        if (
          response.reset_required ||
          (instanceRef.current !== null &&
            response.instance_id !== instanceRef.current)
        ) {
          outputEventIdsRef.current.clear();
          arrivalIdsRef.current.clear();
          setEvents([]);
          setArrivalIds([]);
          pendingSnapshotRef.current = true;
          succeeded = await requestSnapshot(controller.signal);
        } else {
          const items = response.items ?? [];
          const baseSequence = observedSequenceRef.current;
          let acceptedThrough = baseSequence;
          const accepted: OfficeEvent[] = [];
          let gap = false;
          for (const item of items) {
            if (
              item.tender_id !== tenderId ||
              transportEventIdsRef.current.has(item.event_id) ||
              item.sequence <= acceptedThrough
            ) {
              continue;
            }
            if (item.sequence !== acceptedThrough + 1) {
              gap = true;
              break;
            }
            accepted.push(item);
            acceptedThrough = item.sequence;
            transportEventIdsRef.current.add(item.event_id);
          }

          if (gap) {
            outputEventIdsRef.current.clear();
            arrivalIdsRef.current.clear();
            setEvents([]);
            setArrivalIds([]);
            pendingSnapshotRef.current = true;
            transportEventIdsRef.current.clear();
            succeeded = await requestSnapshot(controller.signal);
          } else if (accepted.length > 0) {
            observedSequenceRef.current = acceptedThrough;
            pendingSnapshotRef.current = true;
            // Event payloads are signals only. Reconcile the complete view.
            succeeded = await requestSnapshot(controller.signal);
            if (
              succeeded &&
              instanceRef.current === response.instance_id &&
              (snapshotRef.current?.sequence ?? -1) >= acceptedThrough
            ) {
              appendObservedEvents(accepted, beforePollState === "connected");
            }
          } else {
            cursorRef.current = response.cursor ?? cursorRef.current;
            succeeded = true;
            setError(null);
            setConnection("connected");
          }
        }

        if (succeeded) {
          retryAttemptRef.current = 0;
          setError(null);
          setConnection("connected");
        }
      } catch (failure) {
        if (!isAbortError(failure, controller.signal)) setRequestError(failure);
      } finally {
        endRequest(controller);
        if (!isCurrent()) return;
        if (refreshRequestedRef.current) {
          refreshRequestedRef.current = false;
          void runSnapshot();
        } else if (succeeded) {
          schedule("poll");
        } else {
          schedule("retry");
        }
      }
    }

    const requestRefresh = () => {
      clearTimer();
      retryAttemptRef.current = 0;
      if (requestInFlightRef.current) {
        refreshRequestedRef.current = true;
        return;
      }
      void runSnapshot();
    };
    manualRefreshRef.current = requestRefresh;

    const onVisibilityChange = () => {
      if (!isCurrent() || timerRef.current === null) return;
      const kind = timerKindRef.current;
      if (kind === "poll") schedule("poll");
    };
    document.addEventListener("visibilitychange", onVisibilityChange);

    setConnection(snapshotRef.current ? "reconnecting" : "connecting");
    void runSnapshot();

    return () => {
      mountedRef.current = false;
      generationRef.current += 1;
      manualRefreshRef.current = null;
      refreshRequestedRef.current = false;
      requestControllerRef.current?.abort();
      requestControllerRef.current = null;
      requestInFlightRef.current = false;
      clearTimer();
      document.removeEventListener("visibilitychange", onVisibilityChange);
    };
  }, [api, bindingKey, enabled, tenderId]);

  const refresh = useCallback(() => {
    manualRefreshRef.current?.();
  }, []);

  return {
    snapshot: bindingRef.current === bindingKey ? snapshot : null,
    connectionState:
      bindingRef.current === bindingKey
        ? connectionState
        : enabled
          ? "connecting"
          : "disabled",
    error: bindingRef.current === bindingKey ? error : null,
    events: bindingRef.current === bindingKey ? events : [],
    arrivalIds: bindingRef.current === bindingKey ? arrivalIds : [],
    refresh,
    retry: refresh,
  };
}

function getVisibility(): DocumentVisibilityState | "visible" {
  return typeof document === "undefined" ? "visible" : document.visibilityState;
}

function isAbortError(error: unknown, signal: AbortSignal) {
  return (
    signal.aborted ||
    (error instanceof DOMException && error.name === "AbortError")
  );
}

function toError(error: unknown) {
  return error instanceof Error
    ? error
    : new Error("The office could not be loaded.");
}
