import { act, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { windowOf } from "../test/window";
import { useFit, usePresence } from "./layout";

afterEach(() => vi.useRealTimers());

describe("how the window's width shapes the shell", () => {
  it("is wide, medium, narrow or small by the window's width", () => {
    const view = windowOf(1440);
    const { result } = renderHook(() => useFit());
    const fits = [1440, 1280, 1279, 1024, 1023, 640, 639, 375].map((width) => {
      view.resize(width);
      return [width, result.current];
    });

    expect(fits).toEqual([
      [1440, "wide"],
      [1280, "wide"],
      [1279, "medium"],
      [1024, "medium"],
      [1023, "narrow"],
      [640, "narrow"],
      [639, "small"],
      [375, "small"],
    ]);
  });

  it("stops following the window once the shell has gone", () => {
    const view = windowOf(1440);
    const { unmount } = renderHook(() => useFit());
    expect(view.listeners).toHaveLength(3);

    unmount();
    expect(view.listeners).toHaveLength(0);
  });

  it("is wide where the window can't be measured", () => {
    expect(renderHook(() => useFit()).result.current).toBe("wide");
  });
});

describe("a panel's closing animation", () => {
  it("keeps the panel on screen while it leaves, then removes it", () => {
    vi.useFakeTimers();
    const { result, rerender } = renderHook(({ open }) => usePresence(open), { initialProps: { open: false } });
    expect(result.current).toEqual({ shown: false, leaving: false });

    rerender({ open: true });
    expect(result.current).toEqual({ shown: true, leaving: false });
    rerender({ open: false });
    expect(result.current).toEqual({ shown: true, leaving: true });
    act(() => vi.advanceTimersByTime(159));
    expect(result.current).toEqual({ shown: true, leaving: true });
    act(() => vi.advanceTimersByTime(1));
    expect(result.current).toEqual({ shown: false, leaving: false });
  });

  it("stays when reopened before it has left", () => {
    vi.useFakeTimers();
    const { result, rerender } = renderHook(({ open }) => usePresence(open, 300), { initialProps: { open: true } });

    rerender({ open: false });
    act(() => vi.advanceTimersByTime(200));
    rerender({ open: true });
    act(() => vi.advanceTimersByTime(300));
    expect(result.current).toEqual({ shown: true, leaving: false });
  });
});
