import { act, renderHook } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ApiContext, type Api, type Schema } from "../../api";
import { useOffice } from "./useOffice";

type Snapshot = Schema<"OfficeSnapshot">;
type OfficeEvent = Schema<"OfficeEvent">;
type EventPage = Schema<"OfficeEventPageWithInstance">;

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, resolve, reject };
}

function snapshot(
  _tenderId: string,
  sequence: number,
  cursor: string | null,
  instanceId = "instance-a",
): Snapshot {
  return {
    manager: {} as Snapshot["manager"],
    staff: [],
    assignments: [],
    messages: { items: [], next_cursor: null },
    cursor,
    sequence,
    instance_id: instanceId,
    interrupted_assignment_count: 0,
    staff_total: 0,
    assignments_total: 0,
    partial_flags: {
      staff: false,
      assignments: false,
      messages: false,
      work_orders: false,
      results: false,
      receipts: false,
      versions: false,
    },
  };
}

function event(
  tenderId: string,
  eventId: string,
  sequence: number,
): OfficeEvent {
  return {
    event_id: eventId,
    sequence,
    tender_id: tenderId,
    event_type: "staff_created",
    actor_id: null,
    assignment_id: null,
    record_ref: null,
    payload: null,
    occurred_at: "2026-09-10T00:00:00Z",
  };
}

function page(
  instanceId: string,
  items: OfficeEvent[],
  cursor?: string,
): EventPage {
  return {
    items,
    cursor: cursor ?? items.at(-1)?.event_id ?? null,
    has_more: false,
    reset_required: false,
    instance_id: instanceId,
  };
}

function wrapper(api: Api) {
  return ({ children }: { children: React.ReactNode }) => (
    <ApiContext.Provider value={api}>{children}</ApiContext.Provider>
  );
}

describe("useOffice", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("retries the first failed snapshot instead of treating an empty event page as a baseline", async () => {
    const api = {
      get: vi
        .fn()
        .mockRejectedValueOnce(new Error("Synthetic first snapshot failure"))
        .mockResolvedValueOnce(snapshot("tender-a", 0, null)),
    } as unknown as Api;
    const view = renderHook(() => useOffice("tender-a"), {
      wrapper: wrapper(api),
    });
    await act(async () => {});
    expect(view.result.current.snapshot).toBeNull();
    await act(async () => {
      await vi.advanceTimersByTimeAsync(1000);
    });
    expect(api.get).toHaveBeenLastCalledWith(
      "/tenders/tender-a/office",
      expect.any(AbortSignal),
    );
    expect(view.result.current.snapshot?.sequence).toBe(0);
    expect(view.result.current.connectionState).toBe("connected");
  });

  it("clears former-instance history when manual refresh adopts a restarted service", async () => {
    const api = {
      get: vi
        .fn()
        .mockResolvedValueOnce(snapshot("tender-a", 0, null))
        .mockResolvedValueOnce(
          page("instance-a", [event("tender-a", "event-a", 1)]),
        )
        .mockResolvedValueOnce(snapshot("tender-a", 1, "event-a"))
        .mockResolvedValueOnce(snapshot("tender-a", 0, null, "instance-b")),
    } as unknown as Api;
    const view = renderHook(() => useOffice("tender-a"), {
      wrapper: wrapper(api),
    });
    await act(async () => {});
    await act(async () => {
      await vi.advanceTimersByTimeAsync(1500);
    });
    expect(view.result.current.arrivalIds).toEqual(["event-a"]);
    await act(async () => view.result.current.refresh());
    expect(view.result.current.snapshot?.instance_id).toBe("instance-b");
    expect(view.result.current.arrivalIds).toEqual([]);
    expect(view.result.current.events).toEqual([]);
  });

  it("does not animate events from an old instance against a new-instance snapshot", async () => {
    const api = {
      get: vi
        .fn()
        .mockResolvedValueOnce(snapshot("tender-a", 0, null))
        .mockResolvedValueOnce(
          page("instance-a", [event("tender-a", "old-event", 1)]),
        )
        .mockResolvedValueOnce(snapshot("tender-a", 0, null, "instance-b")),
    } as unknown as Api;
    const view = renderHook(() => useOffice("tender-a"), {
      wrapper: wrapper(api),
    });
    await act(async () => {});
    await act(async () => {
      await vi.advanceTimersByTimeAsync(1500);
    });
    expect(view.result.current.snapshot?.instance_id).toBe("instance-b");
    expect(view.result.current.arrivalIds).toEqual([]);
    expect(view.result.current.events).toEqual([]);
  });

  it("loads a Tender snapshot as the baseline and starts authenticated polling", async () => {
    const first = deferred<Snapshot>();
    const api = {
      get: vi.fn().mockReturnValueOnce(first.promise),
    } as unknown as Api;
    const view = renderHook(() => useOffice("tender-a"), {
      wrapper: wrapper(api),
    });

    expect(view.result.current.connectionState).toBe("connecting");
    expect(api.get).toHaveBeenCalledWith(
      "/tenders/tender-a/office",
      expect.any(AbortSignal),
    );

    await act(async () => first.resolve(snapshot("tender-a", 0, null)));
    expect(view.result.current.snapshot?.sequence).toBe(0);
    expect(view.result.current.connectionState).toBe("connected");
    expect(view.result.current.events).toEqual([]);
    expect(view.result.current.arrivalIds).toEqual([]);

    await act(async () => {
      await vi.advanceTimersByTimeAsync(1_500);
    });
    expect(api.get).toHaveBeenLastCalledWith(
      "/tenders/tender-a/office/events",
      expect.any(AbortSignal),
    );
  });

  it("records a live event once, refreshes the full snapshot, and deduplicates repeats", async () => {
    const baseline = deferred<Snapshot>();
    const refreshed = deferred<Snapshot>();
    const live = event("tender-a", "event-a", 1);
    const api = {
      get: vi
        .fn()
        .mockReturnValueOnce(baseline.promise)
        .mockResolvedValueOnce(page("instance-a", [live]))
        .mockReturnValueOnce(refreshed.promise)
        .mockResolvedValueOnce(page("instance-a", [live]))
        .mockResolvedValueOnce(snapshot("tender-a", 1, "event-a")),
    } as unknown as Api;
    const view = renderHook(() => useOffice("tender-a"), {
      wrapper: wrapper(api),
    });
    await act(async () => baseline.resolve(snapshot("tender-a", 0, null)));

    await act(async () => {
      await vi.advanceTimersByTimeAsync(1_500);
    });
    expect(view.result.current.events).toEqual([]);
    expect(view.result.current.arrivalIds).toEqual([]);
    expect(api.get).toHaveBeenLastCalledWith(
      "/tenders/tender-a/office",
      expect.any(AbortSignal),
    );

    await act(async () =>
      refreshed.resolve(snapshot("tender-a", 1, "event-a")),
    );
    await act(async () => {
      await vi.advanceTimersByTimeAsync(1_500);
    });
    expect(view.result.current.events).toEqual([live]);
    expect(view.result.current.arrivalIds).toEqual(["event-a"]);
  });

  it("does not animate an arrival whose snapshot failed before reconnecting", async () => {
    const api = {
      get: vi
        .fn()
        .mockResolvedValueOnce(snapshot("tender-a", 0, null))
        .mockResolvedValueOnce(
          page("instance-a", [event("tender-a", "event-a", 1)]),
        )
        .mockRejectedValueOnce(new Error("Synthetic snapshot disconnection"))
        .mockResolvedValueOnce(snapshot("tender-a", 1, "event-a")),
    } as unknown as Api;
    const view = renderHook(() => useOffice("tender-a"), {
      wrapper: wrapper(api),
    });
    await act(async () => {});
    await act(async () => {
      await vi.advanceTimersByTimeAsync(1_500);
    });
    expect(view.result.current.connectionState).toBe("reconnecting");
    expect(view.result.current.arrivalIds).toEqual([]);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(1_000);
    });
    expect(view.result.current.snapshot?.sequence).toBe(1);
    expect(view.result.current.connectionState).toBe("connected");
    expect(view.result.current.arrivalIds).toEqual([]);
  });

  it.each([
    [
      "a sequence gap",
      (current: Snapshot) =>
        page("instance-a", [event("tender-a", "event-c", 3)]),
      "instance-a",
    ],
    [
      "an instance change",
      (_current: Snapshot) =>
        page("instance-b", [event("tender-a", "event-b", 2)]),
      "instance-b",
    ],
  ] as const)(
    "requests a fresh snapshot after %s without arrival animation",
    async (_label, makePage, nextInstance) => {
      const baseline = deferred<Snapshot>();
      const refreshed = deferred<Snapshot>();
      const api = {
        get: vi
          .fn()
          .mockReturnValueOnce(baseline.promise)
          .mockResolvedValueOnce(makePage(snapshot("tender-a", 1, "event-a")))
          .mockReturnValueOnce(refreshed.promise),
      } as unknown as Api;
      const view = renderHook(() => useOffice("tender-a"), {
        wrapper: wrapper(api),
      });
      await act(async () =>
        baseline.resolve(snapshot("tender-a", 1, "event-a")),
      );

      await act(async () => {
        await vi.advanceTimersByTimeAsync(1_500);
      });
      expect(view.result.current.events).toEqual([]);
      expect(view.result.current.arrivalIds).toEqual([]);
      expect(api.get).toHaveBeenLastCalledWith(
        "/tenders/tender-a/office",
        expect.any(AbortSignal),
      );

      await act(async () => {
        refreshed.resolve(snapshot("tender-a", 3, "event-c", nextInstance));
      });
      expect(view.result.current.snapshot?.instance_id).toBe(nextInstance);
      expect(view.result.current.arrivalIds).toEqual([]);
    },
  );

  it("retains the last snapshot while reporting a reconnecting failure", async () => {
    const baseline = deferred<Snapshot>();
    const failure = new Error("service unavailable");
    const api = {
      get: vi
        .fn()
        .mockReturnValueOnce(baseline.promise)
        .mockRejectedValueOnce(failure),
    } as unknown as Api;
    const view = renderHook(() => useOffice("tender-a"), {
      wrapper: wrapper(api),
    });
    const known = snapshot("tender-a", 0, null);
    await act(async () => baseline.resolve(known));
    await act(async () => {
      await vi.advanceTimersByTimeAsync(1_500);
    });

    expect(view.result.current.snapshot).toBe(known);
    expect(view.result.current.connectionState).toBe("reconnecting");
    expect(view.result.current.error).toBe(failure);
  });

  it("aborts an in-flight request when unmounted", async () => {
    const first = deferred<Snapshot>();
    const api = {
      get: vi.fn().mockReturnValueOnce(first.promise),
    } as unknown as Api;
    const view = renderHook(() => useOffice("tender-a"), {
      wrapper: wrapper(api),
    });
    const signal = (api.get as ReturnType<typeof vi.fn>).mock
      .calls[0][1] as AbortSignal;
    view.unmount();
    expect(signal.aborted).toBe(true);
    await act(async () => first.resolve(snapshot("tender-a", 0, null)));
  });

  it("clears a previous Tender and ignores its late response", async () => {
    const first = deferred<Snapshot>();
    const second = deferred<Snapshot>();
    const api = {
      get: vi
        .fn()
        .mockReturnValueOnce(first.promise)
        .mockReturnValueOnce(second.promise),
    } as unknown as Api;
    const view = renderHook(
      ({ tenderId }: { tenderId: string }) => useOffice(tenderId),
      {
        initialProps: { tenderId: "tender-a" },
        wrapper: wrapper(api),
      },
    );

    view.rerender({ tenderId: "tender-b" });
    expect(view.result.current.snapshot).toBeNull();
    expect(view.result.current.events).toEqual([]);
    expect(view.result.current.arrivalIds).toEqual([]);
    expect(view.result.current.connectionState).toBe("connecting");
    expect(api.get).toHaveBeenLastCalledWith(
      "/tenders/tender-b/office",
      expect.any(AbortSignal),
    );

    await act(async () => {
      first.resolve(snapshot("tender-a", 0, null));
      second.resolve(snapshot("tender-b", 0, null));
    });
    expect(view.result.current.snapshot?.instance_id).toBe("instance-a");
    expect(view.result.current.snapshot).not.toBe(first);
  });

  it("does not queue arrival animation while the document is hidden", async () => {
    const baseline = deferred<Snapshot>();
    const live = event("tender-a", "event-hidden", 1);
    const api = {
      get: vi
        .fn()
        .mockReturnValueOnce(baseline.promise)
        .mockResolvedValueOnce(page("instance-a", [live]))
        .mockResolvedValueOnce(snapshot("tender-a", 1, "event-hidden")),
    } as unknown as Api;
    const view = renderHook(() => useOffice("tender-a"), {
      wrapper: wrapper(api),
    });
    await act(async () => baseline.resolve(snapshot("tender-a", 0, null)));
    Object.defineProperty(document, "visibilityState", {
      configurable: true,
      value: "hidden",
    });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(1_500);
    });
    expect(view.result.current.events).toEqual([live]);
    expect(view.result.current.arrivalIds).toEqual([]);
  });
});
